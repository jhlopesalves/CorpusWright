use crate::analysis::{
    AnalysisCorpus, AnnotationBuildTimingCounters, AnnotationLayer, AnnotationLayerDocument,
};
use crate::annotation::{ANNOTATION_PIPELINE_VERSION, LemmaTable};
use crate::decode::DECODE_POLICY_VERSION;
use crate::hash::sha256_hex;
use crate::scan::{DocumentRecord, DocumentType};
use crate::tokenization::TokenizationManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

pub const ANNOTATION_LAYER_SCHEMA_VERSION: u32 = 1;
pub const ANNOTATION_LAYER_CACHE_FILE_VERSION: u32 = 2;
const VERIFY_CACHE_WRITE_ENV: &str = "CORPUSWRIGHT_VERIFY_CACHE_WRITE";

#[cfg(test)]
static VERIFY_CACHE_WRITE_OVERRIDE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(0);

fn legacy_annotation_pipeline_version() -> u32 {
    1
}

/// Full identity of the annotation layer and the corpus it was built from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationManifest {
    pub schema_version: u32,
    #[serde(default = "legacy_annotation_pipeline_version")]
    pub annotation_pipeline_version: u32,
    pub tokenization: TokenizationManifest,
    pub corpus: AnnotationCorpusIdentity,
    pub languages: AnnotationLanguageIdentity,
    pub models: Vec<AnnotationModelIdentity>,
}

/// Corpus-level identity hashes computed from deterministic ordered payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationCorpusIdentity {
    pub documents_hash_sha256: String,
    pub surface_terms_hash_sha256: String,
    pub processed_texts_hash_sha256: String,
    pub decode_policy_version: u32,
    pub surface_term_count: usize,
    pub documents: Vec<AnnotationDocumentIdentity>,
}

/// Per-document identity proving the exact text used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationDocumentIdentity {
    pub analysis_document_index: usize,
    pub original_record_index: usize,
    pub source_path: String,
    pub relative_path: String,
    pub document_type: DocumentType,
    pub token_count: u64,
    pub processed_hash_sha256: String,
}

/// Language assignment identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationLanguageIdentity {
    pub overrides: Vec<AnnotationLanguageOverrideInput>,
    pub resolved_documents: Vec<AnnotationDocumentLanguage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationLanguageOverrideInput {
    pub original_record_index: usize,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationDocumentLanguage {
    pub analysis_document_index: usize,
    pub effective_language: String,
}

/// Identity of one UDPipe model used during annotation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationModelIdentity {
    pub language: String,
    pub model_filename: String,
    pub udpipe_model_name: String,
    pub sha256: String,
}

#[derive(Serialize)]
struct ProcessedTextsIdentityPayload<'a> {
    decode_policy_version: u32,
    document_hashes: &'a [String],
}

/// On-disk cache file envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AnnotationLayerCacheEnvelope {
    pub cache_file_version: u32,
    pub manifest: AnnotationManifest,
    pub layer: AnnotationLayerDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AnnotationLayerDto {
    pub surface_terms: Vec<String>,
    pub documents: Vec<AnnotationLayerDocument>,
    pub lemma_table: LemmaTableDto,
    pub languages_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct LemmaTableDto {
    pub entries: Vec<String>,
}

impl From<&LemmaTable> for LemmaTableDto {
    fn from(table: &LemmaTable) -> Self {
        Self {
            entries: table.entries().to_vec(),
        }
    }
}

impl TryFrom<LemmaTableDto> for LemmaTable {
    type Error = String;

    fn try_from(dto: LemmaTableDto) -> Result<Self, Self::Error> {
        let mut table = LemmaTable::new();
        let mut unique = std::collections::HashSet::new();
        for entry in dto.entries {
            if !unique.insert(entry.clone()) {
                return Err(format!("Duplicate lemma entry: {}", entry));
            }
            table.intern(&entry);
        }
        Ok(table)
    }
}

/// Result of loading a cache file.
///
/// Semantics:
/// - `Ready`: Cache loaded successfully and matches expected manifest.
/// - `Stale`: Cache file exists but schema version, file version, or manifest data has changed.
/// - `Missing`: Cache file does not exist.
/// - `Invalid`: Cache file exists but is corrupt, partial, or violates consistency invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationLayerCacheLoad {
    Ready(AnnotationLayer),
    Stale,
    Missing,
    Invalid(String),
}

/// Errors that can occur during cache I/O.
#[derive(Debug, Clone)]
pub enum AnnotationLayerCacheError {
    IoError(String),
    SerializationError(String),
    ValidationError(String),
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn verify_cache_write_enabled() -> bool {
    #[cfg(test)]
    match VERIFY_CACHE_WRITE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => return false,
        2 => return true,
        _ => {}
    }

    std::env::var(VERIFY_CACHE_WRITE_ENV).as_deref() == Ok("1")
}

/// Constructs the full AnnotationManifest for a given corpus analysis and options.
pub fn build_annotation_manifest(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    tokenization: TokenizationManifest,
    language_overrides: &HashMap<usize, String>,
    mut models: Vec<AnnotationModelIdentity>,
    document_hashes: &[String],
    document_languages: &[String],
) -> Result<AnnotationManifest, AnnotationLayerCacheError> {
    let expected_count = corpus.document_summaries().count();
    if document_hashes.len() != expected_count {
        return Err(AnnotationLayerCacheError::ValidationError(format!(
            "document_hashes length ({}) does not match corpus document count ({})",
            document_hashes.len(),
            expected_count
        )));
    }
    if document_languages.len() != expected_count {
        return Err(AnnotationLayerCacheError::ValidationError(format!(
            "document_languages length ({}) does not match corpus document count ({})",
            document_languages.len(),
            expected_count
        )));
    }

    models.sort_by(|a, b| a.language.cmp(&b.language));

    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records
        .sort_by(|left, right| crate::analysis::compare_document_records(left.1, right.1));

    let mut documents = Vec::with_capacity(corpus.document_summaries().count());
    let mut record_iter = ordered_records.into_iter().peekable();

    for (analysis_document_index, (summary, (doc_hash, _doc_lang))) in corpus
        .document_summaries()
        .zip(document_hashes.iter().zip(document_languages.iter()))
        .enumerate()
    {
        let source_path_str = summary.source_path.clone();
        let relative_path_str = summary.relative_path.clone();

        let mut matched_record = None;
        for (orig_idx, record) in record_iter.by_ref() {
            if record.source_path.to_string_lossy() == source_path_str
                && record.relative_path.to_string_lossy() == relative_path_str
            {
                matched_record = Some((orig_idx, record));
                break;
            }
        }

        let (original_record_index, record) = matched_record.ok_or_else(|| {
            AnnotationLayerCacheError::ValidationError(format!(
                "Corpus summary not found in provided records: {}",
                source_path_str
            ))
        })?;

        documents.push(AnnotationDocumentIdentity {
            analysis_document_index,
            original_record_index,
            source_path: normalize_path(&record.source_path),
            relative_path: normalize_path(&record.relative_path),
            document_type: record.document_type.clone(),
            token_count: summary.token_count,
            processed_hash_sha256: doc_hash.clone(),
        });
    }

    let documents_payload = serde_json::to_vec(&documents)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;
    let documents_hash_sha256 = sha256_hex(&documents_payload);

    let surface_terms: Vec<&str> = corpus.surface_term_tokens().collect();
    let surface_term_count = surface_terms.len();
    let surface_terms_payload = serde_json::to_vec(&surface_terms)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;
    let surface_terms_hash_sha256 = sha256_hex(&surface_terms_payload);

    let processed_texts_hash_sha256 =
        processed_texts_identity_hash(document_hashes, DECODE_POLICY_VERSION)?;

    let mut overrides = Vec::new();
    let mut override_keys: Vec<_> = language_overrides.keys().copied().collect();
    override_keys.sort_unstable();
    for &idx in &override_keys {
        overrides.push(AnnotationLanguageOverrideInput {
            original_record_index: idx,
            language: language_overrides[&idx].clone(),
        });
    }

    let resolved_documents = document_languages
        .iter()
        .enumerate()
        .map(|(idx, lang)| AnnotationDocumentLanguage {
            analysis_document_index: idx,
            effective_language: lang.clone(),
        })
        .collect();

    Ok(AnnotationManifest {
        schema_version: ANNOTATION_LAYER_SCHEMA_VERSION,
        annotation_pipeline_version: ANNOTATION_PIPELINE_VERSION,
        tokenization,
        corpus: AnnotationCorpusIdentity {
            documents_hash_sha256,
            surface_terms_hash_sha256,
            processed_texts_hash_sha256,
            decode_policy_version: DECODE_POLICY_VERSION,
            surface_term_count,
            documents,
        },
        languages: AnnotationLanguageIdentity {
            overrides,
            resolved_documents,
        },
        models,
    })
}

fn processed_texts_identity_hash(
    document_hashes: &[String],
    decode_policy_version: u32,
) -> Result<String, AnnotationLayerCacheError> {
    let payload = ProcessedTextsIdentityPayload {
        decode_policy_version,
        document_hashes,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

fn validate_annotation_cache_envelope(
    envelope: &AnnotationLayerCacheEnvelope,
    expected_manifest: Option<&AnnotationManifest>,
) -> Result<(), AnnotationLayerCacheLoad> {
    if envelope.cache_file_version != ANNOTATION_LAYER_CACHE_FILE_VERSION {
        return Err(AnnotationLayerCacheLoad::Stale);
    }

    if let Some(expected) = expected_manifest {
        if envelope.manifest.schema_version != expected.schema_version {
            return Err(AnnotationLayerCacheLoad::Stale);
        }
        if envelope.manifest.annotation_pipeline_version != ANNOTATION_PIPELINE_VERSION {
            return Err(AnnotationLayerCacheLoad::Stale);
        }
        if envelope.manifest != *expected {
            return Err(AnnotationLayerCacheLoad::Stale);
        }
    }

    if envelope.layer.surface_terms.len() != envelope.manifest.corpus.surface_term_count {
        return Err(AnnotationLayerCacheLoad::Invalid(format!(
            "Surface term table length {} does not match manifest count {}",
            envelope.layer.surface_terms.len(),
            envelope.manifest.corpus.surface_term_count
        )));
    }
    let surface_terms_for_hash = envelope
        .layer
        .surface_terms
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let surface_terms_payload = serde_json::to_vec(&surface_terms_for_hash).map_err(|error| {
        AnnotationLayerCacheLoad::Invalid(format!("Failed to hash surface term table: {error}"))
    })?;
    let surface_terms_hash_sha256 = sha256_hex(&surface_terms_payload);
    if surface_terms_hash_sha256 != envelope.manifest.corpus.surface_terms_hash_sha256 {
        return Err(AnnotationLayerCacheLoad::Invalid(
            "Surface term table hash does not match manifest".to_string(),
        ));
    }

    let max_lemma_id = envelope.layer.lemma_table.entries.len() as u32;
    let max_surface_term_id = envelope.layer.surface_terms.len() as u32;
    let mut prev_doc_idx = None;
    for doc in &envelope.layer.documents {
        if prev_doc_idx.is_some_and(|prev| doc.analysis_document_index <= prev) {
            return Err(AnnotationLayerCacheLoad::Invalid(
                "Document indices are not strictly ordered".to_string(),
            ));
        }
        prev_doc_idx = Some(doc.analysis_document_index);

        for term in &doc.term_counts {
            if term.surface_term_id >= max_surface_term_id {
                return Err(AnnotationLayerCacheLoad::Invalid(format!(
                    "Surface term ID {} out of range",
                    term.surface_term_id
                )));
            }
            if term.lemma_id >= max_lemma_id {
                return Err(AnnotationLayerCacheLoad::Invalid(format!(
                    "Lemma ID {} out of range",
                    term.lemma_id
                )));
            }
        }
    }

    let mut langs = envelope.layer.languages_used.clone();
    let original_langs = langs.clone();
    langs.sort_unstable();
    langs.dedup();
    if langs != original_langs {
        return Err(AnnotationLayerCacheLoad::Invalid(
            "languages_used is not sorted and deduplicated".to_string(),
        ));
    }

    Ok(())
}

/// Writes the annotation layer cache to disk atomically.
pub fn write_annotation_layer_cache(
    cache_root: &Path,
    manifest: &AnnotationManifest,
    surface_terms: &[String],
    layer: &AnnotationLayer,
) -> Result<(), AnnotationLayerCacheError> {
    write_annotation_layer_cache_with_timing(cache_root, manifest, surface_terms, layer, None)
}

pub(crate) fn write_annotation_layer_cache_with_timing(
    cache_root: &Path,
    manifest: &AnnotationManifest,
    surface_terms: &[String],
    layer: &AnnotationLayer,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<(), AnnotationLayerCacheError> {
    std::fs::create_dir_all(cache_root)
        .map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;
    let final_path = cache_root.join(format!(
        "annotation-layer.v{}.json",
        ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    let temp_path = cache_root.join(format!(
        "annotation-layer.v{}.tmp.json",
        ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));

    let envelope = AnnotationLayerCacheEnvelope {
        cache_file_version: ANNOTATION_LAYER_CACHE_FILE_VERSION,
        manifest: manifest.clone(),
        layer: AnnotationLayerDto {
            surface_terms: surface_terms.to_vec(),
            documents: layer.documents.clone(),
            lemma_table: LemmaTableDto::from(&layer.lemma_table),
            languages_used: layer.languages_used.clone(),
        },
    };

    let file =
        File::create(&temp_path).map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;
    let mut writer = BufWriter::new(file);

    let serialise_started = Instant::now();
    serde_json::to_writer(&mut writer, &envelope)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;
    if let Some(timing) = timing {
        timing.record_cache_serialise(serialise_started.elapsed());
    }

    let write_started = Instant::now();
    writer
        .flush()
        .map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;
    drop(writer);
    if let Some(timing) = timing {
        timing.record_cache_write(write_started.elapsed());
    }

    if verify_cache_write_enabled() {
        let readback_started = Instant::now();
        let read_back = std::fs::read(&temp_path)
            .map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;
        let parsed: AnnotationLayerCacheEnvelope =
            serde_json::from_slice(&read_back).map_err(|e| {
                AnnotationLayerCacheError::ValidationError(format!(
                    "Roundtrip validation failed: {}",
                    e
                ))
            })?;

        if let Err(load_state) = validate_annotation_cache_envelope(&parsed, Some(manifest)) {
            return Err(AnnotationLayerCacheError::ValidationError(format!(
                "Roundtrip invariants validation failed: {:?}",
                load_state
            )));
        }
        if let Err(e) = LemmaTable::try_from(parsed.layer.lemma_table) {
            return Err(AnnotationLayerCacheError::ValidationError(format!(
                "Roundtrip LemmaTable validation failed: {}",
                e
            )));
        }
        if let Some(timing) = timing {
            timing.record_cache_readback_verification(readback_started.elapsed());
        }
    }

    // Atomic rename (Windows safe)
    if let Err(e) = std::fs::rename(&temp_path, &final_path) {
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&temp_path, &final_path).map_err(|e2| {
            AnnotationLayerCacheError::IoError(format!(
                "Rename failed: {} (after retry: {})",
                e, e2
            ))
        })?;
    }

    Ok(())
}

/// Loads an annotation layer cache, validating it against the expected manifest.
pub fn load_annotation_layer_cache(
    cache_root: &Path,
    expected_manifest: &AnnotationManifest,
) -> AnnotationLayerCacheLoad {
    let final_path = cache_root.join(format!(
        "annotation-layer.v{}.json",
        ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    if !final_path.exists() {
        return AnnotationLayerCacheLoad::Missing;
    }

    let bytes = match std::fs::read(&final_path) {
        Ok(b) => b,
        Err(_) => {
            return AnnotationLayerCacheLoad::Invalid("Failed to read cache file".to_string());
        }
    };

    let envelope: AnnotationLayerCacheEnvelope = match serde_json::from_slice(&bytes) {
        Ok(env) => env,
        Err(e) => {
            return AnnotationLayerCacheLoad::Invalid(format!("Failed to parse cache file: {}", e));
        }
    };

    if let Err(load_state) = validate_annotation_cache_envelope(&envelope, Some(expected_manifest))
    {
        return load_state;
    }

    let lemma_table = match LemmaTable::try_from(envelope.layer.lemma_table) {
        Ok(t) => t,
        Err(e) => return AnnotationLayerCacheLoad::Invalid(e),
    };

    let layer = AnnotationLayer {
        documents: envelope.layer.documents,
        lemma_table,
        languages_used: envelope.layer.languages_used,
    };

    AnnotationLayerCacheLoad::Ready(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::DocumentType;
    use crate::tokenization::TokenizationConfig;

    struct VerifyCacheWriteEnvGuard {
        previous: u8,
    }

    impl VerifyCacheWriteEnvGuard {
        fn set(value: Option<bool>) -> Self {
            let next = match value {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };
            let previous =
                VERIFY_CACHE_WRITE_OVERRIDE.swap(next, std::sync::atomic::Ordering::Relaxed);
            Self { previous }
        }
    }

    impl Drop for VerifyCacheWriteEnvGuard {
        fn drop(&mut self) {
            VERIFY_CACHE_WRITE_OVERRIDE.store(self.previous, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn empty_manifest() -> AnnotationManifest {
        let empty_surface_terms: Vec<&str> = Vec::new();
        let surface_terms_hash_sha256 =
            sha256_hex(&serde_json::to_vec(&empty_surface_terms).unwrap());
        AnnotationManifest {
            schema_version: ANNOTATION_LAYER_SCHEMA_VERSION,
            annotation_pipeline_version: ANNOTATION_PIPELINE_VERSION,
            tokenization: TokenizationManifest {
                id: "test".to_string(),
                version: "1.0".to_string(),
                unicode_segmentation_version: "1.0".to_string(),
                config: TokenizationConfig::default(),
            },
            corpus: AnnotationCorpusIdentity {
                documents_hash_sha256: "hash1".to_string(),
                surface_terms_hash_sha256,
                processed_texts_hash_sha256: "hash3".to_string(),
                decode_policy_version: DECODE_POLICY_VERSION,
                surface_term_count: 0,
                documents: vec![],
            },
            languages: AnnotationLanguageIdentity {
                overrides: vec![],
                resolved_documents: vec![],
            },
            models: vec![],
        }
    }

    fn sample_layer() -> AnnotationLayer {
        let mut layer = AnnotationLayer {
            documents: vec![],
            lemma_table: LemmaTable::new(),
            languages_used: vec!["en".to_string()],
        };
        layer.lemma_table.intern("test");
        layer
    }

    fn cache_file_path(cache_root: &Path) -> std::path::PathBuf {
        cache_root.join(format!(
            "annotation-layer.v{}.json",
            ANNOTATION_LAYER_CACHE_FILE_VERSION
        ))
    }

    fn cache_envelope_for(
        manifest: &AnnotationManifest,
        layer: &AnnotationLayer,
    ) -> AnnotationLayerCacheEnvelope {
        AnnotationLayerCacheEnvelope {
            cache_file_version: ANNOTATION_LAYER_CACHE_FILE_VERSION,
            manifest: manifest.clone(),
            layer: AnnotationLayerDto {
                surface_terms: Vec::new(),
                documents: layer.documents.clone(),
                lemma_table: LemmaTableDto::from(&layer.lemma_table),
                languages_used: layer.languages_used.clone(),
            },
        }
    }

    #[test]
    fn manifest_equality_for_identical_inputs() {
        assert_eq!(empty_manifest(), empty_manifest());
    }

    #[test]
    fn manifest_stale_when_schema_version_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.schema_version += 1;
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_annotation_pipeline_version_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.annotation_pipeline_version += 1;
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_tokenization_config_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.tokenization.config.retain_ascii_hyphen_compounds =
            !m2.tokenization.config.retain_ascii_hyphen_compounds;
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_tokenization_version_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.tokenization.version = "1.1".to_string();
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_processed_text_hash_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.corpus.processed_texts_hash_sha256 = "different".to_string();
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_decode_policy_version_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.corpus.decode_policy_version += 1;
        assert_ne!(m1, m2);
    }

    #[test]
    fn processed_texts_hash_includes_decode_policy_version() {
        let document_hashes = vec!["abc".to_string(), "def".to_string()];

        let current =
            processed_texts_identity_hash(&document_hashes, DECODE_POLICY_VERSION).unwrap();
        let future =
            processed_texts_identity_hash(&document_hashes, DECODE_POLICY_VERSION + 1).unwrap();

        assert_ne!(current, future);
    }

    #[test]
    fn manifest_stale_when_surface_terms_hash_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.corpus.surface_terms_hash_sha256 = "different".to_string();
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_document_identity_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.corpus.documents.push(AnnotationDocumentIdentity {
            analysis_document_index: 0,
            original_record_index: 0,
            source_path: "a.txt".to_string(),
            relative_path: "a.txt".to_string(),
            document_type: DocumentType::Text,
            token_count: 10,
            processed_hash_sha256: "abc".to_string(),
        });
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_resolved_language_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.languages
            .resolved_documents
            .push(AnnotationDocumentLanguage {
                analysis_document_index: 0,
                effective_language: "fr".to_string(),
            });
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_override_input_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.languages
            .overrides
            .push(AnnotationLanguageOverrideInput {
                original_record_index: 0,
                language: "fr".to_string(),
            });
        assert_ne!(m1, m2);
    }

    #[test]
    fn manifest_stale_when_model_checksum_changes() {
        let m1 = empty_manifest();
        let mut m2 = empty_manifest();
        m2.models.push(AnnotationModelIdentity {
            language: "en".to_string(),
            model_filename: "mod.udpipe".to_string(),
            udpipe_model_name: "mod".to_string(),
            sha256: "sha".to_string(),
        });
        assert_ne!(m1, m2);
    }

    #[test]
    fn cache_write_then_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();

        let manifest = empty_manifest();
        let layer = sample_layer();
        let surface_terms = Vec::new();

        assert!(
            write_annotation_layer_cache(cache_root, &manifest, &surface_terms, &layer).is_ok()
        );

        let loaded = load_annotation_layer_cache(cache_root, &manifest);
        match loaded {
            AnnotationLayerCacheLoad::Ready(loaded_layer) => {
                assert_eq!(loaded_layer, layer);
            }
            _ => panic!("Expected Ready, got {:?}", loaded),
        }
    }

    #[test]
    fn cache_write_records_annotation_pipeline_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();
        let layer = sample_layer();
        let surface_terms = Vec::new();

        write_annotation_layer_cache(cache_root, &manifest, &surface_terms, &layer).unwrap();

        let bytes = std::fs::read(cache_file_path(cache_root)).unwrap();
        let envelope: AnnotationLayerCacheEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            envelope.manifest.annotation_pipeline_version,
            ANNOTATION_PIPELINE_VERSION
        );
        assert!(matches!(
            load_annotation_layer_cache(cache_root, &manifest),
            AnnotationLayerCacheLoad::Ready(_)
        ));
    }

    #[test]
    fn missing_annotation_pipeline_version_deserializes_as_legacy_and_stale() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();
        let layer = sample_layer();
        let envelope = cache_envelope_for(&manifest, &layer);
        let mut value = serde_json::to_value(&envelope).unwrap();
        value
            .get_mut("manifest")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .remove("annotation_pipeline_version");
        std::fs::create_dir_all(cache_root).unwrap();
        std::fs::write(
            cache_file_path(cache_root),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();

        let parsed: AnnotationLayerCacheEnvelope =
            serde_json::from_value(value).expect("old cache metadata should deserialize");
        assert_eq!(parsed.manifest.annotation_pipeline_version, 1);
        assert!(matches!(
            load_annotation_layer_cache(cache_root, &manifest),
            AnnotationLayerCacheLoad::Stale
        ));
    }

    #[test]
    fn cache_write_skips_internal_readback_by_default_but_public_load_validates() {
        let _env = VerifyCacheWriteEnvGuard::set(Some(false));
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();
        let layer = sample_layer();
        let surface_terms = Vec::new();
        let timing = AnnotationBuildTimingCounters::new();

        write_annotation_layer_cache_with_timing(
            cache_root,
            &manifest,
            &surface_terms,
            &layer,
            Some(&timing),
        )
        .unwrap();

        assert!(!timing.readback_verification_ran());
        let loaded = load_annotation_layer_cache(cache_root, &manifest);
        assert!(matches!(loaded, AnnotationLayerCacheLoad::Ready(_)));
    }

    #[test]
    fn cache_write_runs_internal_readback_when_env_flag_is_set() {
        let _env = VerifyCacheWriteEnvGuard::set(Some(true));
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();
        let layer = sample_layer();
        let surface_terms = Vec::new();
        let timing = AnnotationBuildTimingCounters::new();

        write_annotation_layer_cache_with_timing(
            cache_root,
            &manifest,
            &surface_terms,
            &layer,
            Some(&timing),
        )
        .unwrap();

        assert!(timing.readback_verification_ran());
    }

    #[test]
    fn streaming_cache_write_matches_in_memory_serializer_bytes() {
        let _env = VerifyCacheWriteEnvGuard::set(Some(false));
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();
        let layer = sample_layer();
        let envelope = cache_envelope_for(&manifest, &layer);
        let surface_terms = Vec::new();
        let expected = serde_json::to_string(&envelope).unwrap().into_bytes();

        write_annotation_layer_cache(cache_root, &manifest, &surface_terms, &layer).unwrap();

        let bytes = std::fs::read(cache_file_path(cache_root)).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn partial_cache_file_never_returns_ready() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let final_path = cache_root.join(format!(
            "annotation-layer.v{}.json",
            ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));

        std::fs::write(&final_path, b"{ \"invalid\": true }").unwrap();

        let manifest = empty_manifest();
        let loaded = load_annotation_layer_cache(cache_root, &manifest);
        assert!(matches!(loaded, AnnotationLayerCacheLoad::Invalid(_)));
    }

    #[test]
    fn missing_cache_file_returns_missing() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();
        let manifest = empty_manifest();

        let loaded = load_annotation_layer_cache(cache_root, &manifest);
        assert!(matches!(loaded, AnnotationLayerCacheLoad::Missing));
    }

    #[test]
    fn stale_cache_returns_stale() {
        let dir = tempfile::tempdir().unwrap();
        let cache_root = dir.path();

        let manifest = empty_manifest();
        let layer = AnnotationLayer {
            documents: vec![],
            lemma_table: LemmaTable::new(),
            languages_used: vec![],
        };
        let surface_terms = Vec::new();

        assert!(
            write_annotation_layer_cache(cache_root, &manifest, &surface_terms, &layer).is_ok()
        );

        let mut new_manifest = empty_manifest();
        new_manifest.schema_version += 1;

        let loaded = load_annotation_layer_cache(cache_root, &new_manifest);
        assert!(matches!(loaded, AnnotationLayerCacheLoad::Stale));
    }

    #[test]
    fn lemma_table_dto_roundtrip() {
        let mut table = LemmaTable::new();
        table.intern("hello");
        table.intern("world");

        let dto = LemmaTableDto::from(&table);
        let rebuilt: LemmaTable = dto.try_into().unwrap();

        assert_eq!(table, rebuilt);
    }

    #[test]
    fn duplicate_lemma_entries_return_invalid() {
        let dto = LemmaTableDto {
            entries: vec!["hello".to_string(), "hello".to_string()],
        };
        assert!(LemmaTable::try_from(dto).is_err());
    }
}
