use crate::analysis::{AnalysisCorpus, AnnotationLayer, AnnotationLayerDocument};
use crate::annotation::LemmaTable;
use crate::hash::sha256_hex;
use crate::scan::{DocumentRecord, DocumentType};
use crate::tokenization::TokenizationManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const ANNOTATION_LAYER_SCHEMA_VERSION: u32 = 1;
pub const ANNOTATION_LAYER_CACHE_FILE_VERSION: u32 = 1;

/// Full identity of the annotation layer and the corpus it was built from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationManifest {
    pub schema_version: u32,
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

/// On-disk cache file envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AnnotationLayerCacheEnvelope {
    pub cache_file_version: u32,
    pub manifest: AnnotationManifest,
    pub layer: AnnotationLayerDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct AnnotationLayerDto {
    pub documents: Vec<AnnotationLayerDocument>,
    pub lemma_table: LemmaTableDto,
    pub languages_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LemmaTableDto {
    pub entries: Vec<String>,
}

impl From<&LemmaTable> for LemmaTableDto {
    fn from(table: &LemmaTable) -> Self {
        Self {
            entries: table.entries().to_vec(),
        }
    }
}

impl From<LemmaTableDto> for LemmaTable {
    fn from(dto: LemmaTableDto) -> Self {
        let mut table = LemmaTable::new();
        for entry in dto.entries {
            table.intern(&entry);
        }
        table
    }
}

/// Result of loading a cache file.
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
    models.sort_by(|a, b| a.language.cmp(&b.language));

    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records.sort_by(|left, right| {
        left.1
            .relative_path
            .cmp(&right.1.relative_path)
            .then_with(|| left.1.source_path.cmp(&right.1.source_path))
    });

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

    let processed_texts_payload = serde_json::to_vec(document_hashes)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;
    let processed_texts_hash_sha256 = sha256_hex(&processed_texts_payload);

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
        tokenization,
        corpus: AnnotationCorpusIdentity {
            documents_hash_sha256,
            surface_terms_hash_sha256,
            processed_texts_hash_sha256,
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

/// Writes the annotation layer cache to disk atomically.
pub fn write_annotation_layer_cache(
    cache_root: &Path,
    manifest: &AnnotationManifest,
    layer: &AnnotationLayer,
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
            documents: layer.documents.clone(),
            lemma_table: LemmaTableDto::from(&layer.lemma_table),
            languages_used: layer.languages_used.clone(),
        },
    };

    let json = serde_json::to_string(&envelope)
        .map_err(|e| AnnotationLayerCacheError::SerializationError(e.to_string()))?;

    std::fs::write(&temp_path, json)
        .map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;

    // Read back and verify
    let read_back =
        std::fs::read(&temp_path).map_err(|e| AnnotationLayerCacheError::IoError(e.to_string()))?;
    let _parsed: AnnotationLayerCacheEnvelope =
        serde_json::from_slice(&read_back).map_err(|e| {
            AnnotationLayerCacheError::ValidationError(format!(
                "Roundtrip validation failed: {}",
                e
            ))
        })?;

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

    if envelope.cache_file_version != ANNOTATION_LAYER_CACHE_FILE_VERSION {
        return AnnotationLayerCacheLoad::Stale;
    }

    if envelope.manifest.schema_version != expected_manifest.schema_version {
        return AnnotationLayerCacheLoad::Stale;
    }
    if envelope.manifest != *expected_manifest {
        return AnnotationLayerCacheLoad::Stale;
    }

    let max_lemma_id = envelope.layer.lemma_table.entries.len() as u32;
    let mut prev_doc_idx = None;
    for doc in &envelope.layer.documents {
        if prev_doc_idx.is_some_and(|prev| doc.analysis_document_index <= prev) {
            return AnnotationLayerCacheLoad::Invalid(
                "Document indices are not strictly ordered".to_string(),
            );
        }
        prev_doc_idx = Some(doc.analysis_document_index);

        for term in &doc.term_counts {
            if term.lemma_id >= max_lemma_id {
                return AnnotationLayerCacheLoad::Invalid(format!(
                    "Lemma ID {} out of range",
                    term.lemma_id
                ));
            }
        }
    }

    let mut langs = envelope.layer.languages_used.clone();
    let original_langs = langs.clone();
    langs.sort_unstable();
    langs.dedup();
    if langs != original_langs {
        return AnnotationLayerCacheLoad::Invalid(
            "languages_used is not sorted and deduplicated".to_string(),
        );
    }

    let layer = AnnotationLayer {
        documents: envelope.layer.documents,
        lemma_table: envelope.layer.lemma_table.into(),
        languages_used: envelope.layer.languages_used,
    };

    AnnotationLayerCacheLoad::Ready(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::DocumentType;
    use crate::tokenization::TokenizationConfig;

    fn empty_manifest() -> AnnotationManifest {
        AnnotationManifest {
            schema_version: ANNOTATION_LAYER_SCHEMA_VERSION,
            tokenization: TokenizationManifest {
                id: "test".to_string(),
                version: "1.0".to_string(),
                unicode_segmentation_version: "1.0".to_string(),
                config: TokenizationConfig::default(),
            },
            corpus: AnnotationCorpusIdentity {
                documents_hash_sha256: "hash1".to_string(),
                surface_terms_hash_sha256: "hash2".to_string(),
                processed_texts_hash_sha256: "hash3".to_string(),
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
        let mut layer = AnnotationLayer {
            documents: vec![],
            lemma_table: LemmaTable::new(),
            languages_used: vec![],
        };
        layer.lemma_table.intern("test");
        layer.languages_used.push("en".to_string());

        assert!(write_annotation_layer_cache(cache_root, &manifest, &layer).is_ok());

        let loaded = load_annotation_layer_cache(cache_root, &manifest);
        match loaded {
            AnnotationLayerCacheLoad::Ready(loaded_layer) => {
                assert_eq!(loaded_layer, layer);
            }
            _ => panic!("Expected Ready, got {:?}", loaded),
        }
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

        assert!(write_annotation_layer_cache(cache_root, &manifest, &layer).is_ok());

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
        let rebuilt: LemmaTable = dto.into();

        assert_eq!(table, rebuilt);
    }
}
