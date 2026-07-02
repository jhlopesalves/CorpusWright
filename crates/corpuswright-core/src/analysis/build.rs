use super::annotation_build::{
    AnnotatedTermCount, attach_annotation_layer, build_expected_manifest,
};
use super::concordance::analysis_word_tokens;
use super::input::{
    AnalysisInputProvenance, AnalysisInputResolution, AnalysisInputSource,
    cleaning_config_signature,
};
use super::overview::{
    CorpusOverview, CorpusOverviewContext, MATTR_WINDOW_TOKENS, MTLD_MIN_DOCUMENT_TOKENS,
    MTLD_TTR_THRESHOLD, VocabularyGrowthPoint, build_overview, mattr, mtld,
};
use crate::annotation::LemmaTable;
use crate::clean::CleaningConfig;
use crate::language::detect_and_resolve;
use crate::scan::{DocumentRecord, DocumentType};
use crate::tokenization::TokenizationConfig;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use ts_rs::TS;
/// Current phase of a corpus analysis build.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisBuildPhase {
    LoadingSources,
    ReadingFiles,
    Tokenising,
    ComputingDispersion,
    FinalisingSummaries,
}

/// Progress reported while Rust reads and indexes a corpus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBuildProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub tokens_seen: u64,
    pub types_seen: usize,
    pub current_phase: AnalysisBuildPhase,
}

/// Analysis build failure that callers can distinguish from cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisBuildError {
    Cancelled,
    Message(String),
}

impl std::fmt::Display for AnalysisBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Analysis build cancelled."),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AnalysisBuildError {}

/// One document summary safe to return in a bounded window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDocumentSummary {
    pub id: usize,
    pub source_path: String,
    pub relative_path: String,
    pub token_count: u64,
    /// Detected language ISO 639-1 code.
    #[serde(default)]
    pub language: Option<String>,
    /// Detection confidence 0.0–1.0.
    #[serde(default)]
    pub language_confidence: Option<f64>,
    /// True if the language was manually overridden.
    #[serde(default)]
    pub language_is_overridden: bool,
}

/// Configuration and snapshot metadata for one analysis build.
#[derive(Debug, Clone)]
pub struct AnalysisBuildOptions {
    pub built_at: String,
    pub build_signature: String,
    pub processing_config_signature: String,
    pub overview_context: CorpusOverviewContext,
    pub udpipe_model_dir: Option<std::path::PathBuf>,
    pub force_compute_hashes: bool,
    /// Per-document language override keyed by document index (0-based in the
    /// original records slice). When present, this always wins over detection.
    pub language_overrides: std::collections::HashMap<usize, String>,
    pub tokenization: TokenizationConfig,
    pub annotation_cache_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub(super) struct AnalysisDocument {
    pub(super) summary: AnalysisDocumentSummary,
    pub(super) surface_term_counts: Vec<(u32, u64)>,
    pub(super) annotated_term_counts: Vec<AnnotatedTermCount>,
    pub(super) annotation_available: bool,
    pub(super) mattr: Option<f64>,
    pub(super) mtld: Option<f64>,
    pub(super) processed_hash_sha256: String,
}

#[derive(Debug, Clone)]
pub(super) struct AnalysisTerm {
    pub(super) token: String,
    pub(super) frequency: u64,
    pub(super) document_frequency: usize,
    pub(super) document_counts: Vec<(usize, u64)>,
    pub(super) dispersion: f64,
}

#[derive(Debug, Clone)]
pub(super) struct AnalysisSurfaceTerm {
    pub(super) token: String,
    pub(super) document_counts: Vec<(usize, u64)>,
}

/// Sparse Rust-owned analysis index.
#[derive(Debug, Clone)]
pub struct AnalysisCorpus {
    pub(super) documents: Vec<AnalysisDocument>,
    pub(super) surface_terms: Vec<AnalysisSurfaceTerm>,
    pub(super) lemma_table: LemmaTable,
    pub(super) overview: CorpusOverview,
    pub tokenization: TokenizationConfig,
}

impl AnalysisCorpus {
    /// Returns the compact summary computed with the index.
    pub fn overview(&self) -> &CorpusOverview {
        &self.overview
    }

    pub fn document_summary(&self, document_index: usize) -> Option<&AnalysisDocumentSummary> {
        self.documents
            .get(document_index)
            .map(|document| &document.summary)
    }

    pub fn is_stale_for_cleaning_config(&self, cleaning_config: &CleaningConfig) -> bool {
        self.overview.processing_config_signature != cleaning_config_signature(cleaning_config)
    }

    pub(crate) fn document_summaries(&self) -> impl Iterator<Item = &AnalysisDocumentSummary> {
        self.documents.iter().map(|doc| &doc.summary)
    }

    pub(crate) fn surface_term_tokens(&self) -> impl Iterator<Item = &str> {
        self.surface_terms.iter().map(|term| term.token.as_str())
    }
}

/// Compact selection of analysis documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDocumentSelection {
    pub all_documents: bool,
    #[serde(default)]
    pub ids: Vec<usize>,
    #[serde(default)]
    pub excluded_ids: Vec<usize>,
}

impl Default for AnalysisDocumentSelection {
    fn default() -> Self {
        Self {
            all_documents: true,
            ids: Vec::new(),
            excluded_ids: Vec::new(),
        }
    }
}

pub(super) struct AnalysisBuildMetadata {
    pub(super) source_count: usize,
    pub(super) total_tokens: u64,
    pub(super) input_text_source: AnalysisInputSource,
    pub(super) processed_documents: usize,
    pub(super) source_text_fallback_documents: usize,
    pub(super) built_at: String,
    pub(super) build_signature: String,
    pub(super) processing_config_signature: String,
    pub(super) skipped_documents: usize,
    pub(super) skipped_pdf_documents: usize,
    pub(super) vocabulary_growth: Vec<VocabularyGrowthPoint>,
    pub(super) overview_context: CorpusOverviewContext,
}

pub(super) struct AnalysisBuildProduct {
    pub(super) corpus: AnalysisCorpus,
    #[cfg(test)]
    pub(super) terms: Vec<AnalysisTerm>,
}

enum LoadedAnalysisInput<'record> {
    Ready {
        original_record_index: usize,
        record: &'record DocumentRecord,
        text: String,
    },
    Skipped,
}

enum MappedAnalysisInput<'record> {
    Ready(MappedAnalysisDocument<'record>),
    Skipped,
}

struct MappedAnalysisDocument<'record> {
    record: &'record DocumentRecord,
    prepared_terms: Vec<LocalTermCount>,
    surface_terms: Vec<LocalTermCount>,
    token_count: u64,
    mattr: Option<f64>,
    mtld: Option<f64>,
    language: Option<String>,
    language_confidence: Option<f64>,
    language_is_overridden: bool,
    processed_hash_sha256: String,
}

struct LocalTermCount {
    token: String,
    count: u64,
    first_token_position: u64,
}

fn document_summary(
    record: &DocumentRecord,
    id: usize,
    token_count: u64,
    language: Option<String>,
    language_confidence: Option<f64>,
    language_is_overridden: bool,
) -> AnalysisDocumentSummary {
    AnalysisDocumentSummary {
        id,
        source_path: record.source_path.to_string_lossy().to_string(),
        relative_path: record.relative_path.to_string_lossy().to_string(),
        token_count,
        language,
        language_confidence,
        language_is_overridden,
    }
}

pub(super) fn document_name(document: &AnalysisDocumentSummary) -> &str {
    if document.relative_path.is_empty() {
        &document.source_path
    } else {
        &document.relative_path
    }
}

pub(super) fn compute_dispersion(
    document_counts: &[(usize, u64)],
    frequency: u64,
    documents: &[AnalysisDocument],
    total_tokens: u64,
) -> f64 {
    if frequency == 0 || total_tokens == 0 {
        return 0.0;
    }
    let mut present_document_share = 0.0;
    let mut difference = 0.0;
    for &(document_id, count) in document_counts {
        let document_share =
            documents[document_id].summary.token_count as f64 / total_tokens as f64;
        present_document_share += document_share;
        difference += (count as f64 / frequency as f64 - document_share).abs();
    }
    0.5 * (difference + (1.0 - present_document_share).max(0.0))
}

/// Builds a sparse analysis index while reading one document at a time.
pub fn build_analysis_corpus<TextLoader, Progress>(
    records: &[DocumentRecord],
    options: AnalysisBuildOptions,
    cancel: &AtomicBool,
    load_text: TextLoader,
    report_progress: Progress,
) -> Result<AnalysisCorpus, AnalysisBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
    Progress: FnMut(AnalysisBuildProgress),
{
    build_analysis_corpus_with_mapping(records, options, cancel, load_text, report_progress, true)
        .map(|product| product.corpus)
}

pub(crate) fn compare_document_records(
    left: &DocumentRecord,
    right: &DocumentRecord,
) -> std::cmp::Ordering {
    left.relative_path
        .cmp(&right.relative_path)
        .then_with(|| left.source_path.cmp(&right.source_path))
}

pub(super) fn build_analysis_corpus_with_mapping<TextLoader, Progress>(
    records: &[DocumentRecord],
    options: AnalysisBuildOptions,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    mut report_progress: Progress,
    parallel_map: bool,
) -> Result<AnalysisBuildProduct, AnalysisBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
    Progress: FnMut(AnalysisBuildProgress),
{
    let AnalysisBuildOptions {
        built_at,
        build_signature,
        processing_config_signature,
        overview_context,
        udpipe_model_dir,
        force_compute_hashes,
        language_overrides,
        tokenization,
        annotation_cache_root,
    } = options;
    let cache_enabled = annotation_cache_root.is_some() || force_compute_hashes;

    report_progress(AnalysisBuildProgress {
        files_done: 0,
        files_total: records.len(),
        tokens_seen: 0,
        types_seen: 0,
        current_phase: AnalysisBuildPhase::LoadingSources,
    });

    let mut skipped_documents = 0usize;
    let mut skipped_pdf_documents = 0usize;
    let mut processed_documents = 0usize;
    let mut source_text_fallback_documents = 0usize;
    report_progress(AnalysisBuildProgress {
        files_done: 0,
        files_total: records.len(),
        tokens_seen: 0,
        types_seen: 0,
        current_phase: AnalysisBuildPhase::ReadingFiles,
    });

    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records.sort_by(|left, right| compare_document_records(left.1, right.1));

    let mut loaded_inputs = Vec::with_capacity(records.len());
    for (record_index, (original_record_index, record)) in ordered_records.into_iter().enumerate() {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnalysisBuildError::Cancelled);
        }
        let resolution = load_text(record).map_err(AnalysisBuildError::Message)?;
        let text = match resolution.provenance {
            AnalysisInputProvenance::Processed => {
                processed_documents += 1;
                resolution.text
            }
            AnalysisInputProvenance::SourceTextFallback => {
                source_text_fallback_documents += 1;
                resolution.text
            }
            AnalysisInputProvenance::EmptyOrFailed => None,
        };
        if let Some(text) = text.filter(|value| !value.trim().is_empty()) {
            loaded_inputs.push(LoadedAnalysisInput::Ready {
                original_record_index,
                record,
                text,
            });
        } else {
            skipped_documents += 1;
            if record.document_type == DocumentType::Pdf {
                skipped_pdf_documents += 1;
            }
            loaded_inputs.push(LoadedAnalysisInput::Skipped);
            report_progress(AnalysisBuildProgress {
                files_done: record_index + 1,
                files_total: records.len(),
                tokens_seen: 0,
                types_seen: 0,
                current_phase: AnalysisBuildPhase::ReadingFiles,
            });
            continue;
        }
        report_progress(AnalysisBuildProgress {
            files_done: record_index + 1,
            files_total: records.len(),
            tokens_seen: 0,
            types_seen: 0,
            current_phase: AnalysisBuildPhase::ReadingFiles,
        });
    }

    let mapped_inputs = map_loaded_analysis_inputs(
        loaded_inputs,
        &language_overrides,
        cancel,
        parallel_map,
        &tokenization,
        cache_enabled,
    )?;

    let mut documents = Vec::with_capacity(records.len().saturating_sub(skipped_documents));
    let mut terms = Vec::<AnalysisTerm>::new();
    let mut term_index = HashMap::<String, u32>::new();
    let mut surface_terms = Vec::<AnalysisSurfaceTerm>::new();
    let mut surface_term_index = HashMap::<String, u32>::new();
    let lemma_table = LemmaTable::new();
    let mut total_tokens = 0u64;
    let mut growth_tokens = 0u64;
    let mut next_growth_checkpoint = 1u64;
    let mut vocabulary_growth = Vec::<VocabularyGrowthPoint>::new();

    report_progress(AnalysisBuildProgress {
        files_done: 0,
        files_total: records.len(),
        tokens_seen: 0,
        types_seen: 0,
        current_phase: AnalysisBuildPhase::Tokenising,
    });

    for (record_index, input) in mapped_inputs.iter().enumerate() {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnalysisBuildError::Cancelled);
        }
        if let MappedAnalysisInput::Ready(mapped_document) = input {
            let document_id = u32::try_from(documents.len()).map_err(|_| {
                AnalysisBuildError::Message(
                    "Analysis contains more than u32::MAX documents.".to_string(),
                )
            })?;
            let document_term_counts = assign_prepared_term_ids(
                mapped_document,
                &mut term_index,
                &mut terms,
                &mut growth_tokens,
                &mut next_growth_checkpoint,
                &mut vocabulary_growth,
            )?;
            let mut surface_sparse_counts = Vec::with_capacity(mapped_document.surface_terms.len());
            for local_term in &mapped_document.surface_terms {
                let term_id = surface_term_id(
                    &local_term.token,
                    &mut surface_term_index,
                    &mut surface_terms,
                )?;
                surface_terms[term_id as usize]
                    .document_counts
                    .push((document_id as usize, local_term.count));
                surface_sparse_counts.push((term_id, local_term.count));
            }
            surface_sparse_counts.sort_unstable_by_key(|(term_id, _)| *term_id);
            for (term_id, count) in document_term_counts {
                let term = &mut terms[term_id as usize];
                term.frequency += count;
                term.document_frequency += 1;
                term.document_counts.push((document_id as usize, count));
            }
            total_tokens += mapped_document.token_count;
            documents.push(AnalysisDocument {
                summary: document_summary(
                    mapped_document.record,
                    document_id as usize,
                    mapped_document.token_count,
                    mapped_document.language.clone(),
                    mapped_document.language_confidence,
                    mapped_document.language_is_overridden,
                ),
                surface_term_counts: surface_sparse_counts,
                annotated_term_counts: Vec::new(),
                annotation_available: false,
                mattr: mapped_document.mattr,
                mtld: mapped_document.mtld,
                processed_hash_sha256: mapped_document.processed_hash_sha256.clone(),
            });
        }
        report_progress(AnalysisBuildProgress {
            files_done: record_index + 1,
            files_total: records.len(),
            tokens_seen: total_tokens,
            types_seen: terms.len(),
            current_phase: AnalysisBuildPhase::Tokenising,
        });
    }

    if total_tokens > 0
        && vocabulary_growth
            .last()
            .is_none_or(|point| point.tokens != total_tokens)
    {
        vocabulary_growth.push(VocabularyGrowthPoint {
            tokens: total_tokens,
            types: terms.len(),
        });
    }

    report_progress(AnalysisBuildProgress {
        files_done: records.len(),
        files_total: records.len(),
        tokens_seen: total_tokens,
        types_seen: terms.len(),
        current_phase: AnalysisBuildPhase::ComputingDispersion,
    });
    for term in &mut terms {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnalysisBuildError::Cancelled);
        }
        term.dispersion = compute_dispersion(
            &term.document_counts,
            term.frequency,
            &documents,
            total_tokens,
        );
    }

    report_progress(AnalysisBuildProgress {
        files_done: records.len(),
        files_total: records.len(),
        tokens_seen: total_tokens,
        types_seen: terms.len(),
        current_phase: AnalysisBuildPhase::FinalisingSummaries,
    });
    let overview = build_overview(
        &documents,
        &terms,
        AnalysisBuildMetadata {
            source_count: records.len(),
            total_tokens,
            input_text_source: match (processed_documents, source_text_fallback_documents) {
                (0, _) => AnalysisInputSource::SourceTextFallback,
                (_, 0) => AnalysisInputSource::Processed,
                _ => AnalysisInputSource::ProcessedWithSourceTextFallback,
            },
            processed_documents,
            source_text_fallback_documents,
            built_at,
            build_signature,
            processing_config_signature,
            skipped_documents,
            skipped_pdf_documents,
            vocabulary_growth,
            overview_context,
        },
    );

    let mut corpus = AnalysisCorpus {
        documents,
        surface_terms,
        lemma_table,
        overview,
        tokenization,
    };

    if let Some(ref cache_root) = annotation_cache_root {
        let expected_manifest = build_expected_manifest(
            &corpus,
            records,
            udpipe_model_dir.as_deref(),
            &language_overrides,
        );
        if let Ok(expected_manifest) = expected_manifest {
            let load_res = crate::annotation_cache::load_annotation_layer_cache(
                cache_root,
                &expected_manifest,
            );
            if let crate::annotation_cache::AnnotationLayerCacheLoad::Ready(layer) = load_res {
                let _ = attach_annotation_layer(&mut corpus, layer);
            }
        }
    }

    Ok(AnalysisBuildProduct {
        corpus,
        #[cfg(test)]
        terms,
    })
}

fn map_loaded_analysis_inputs<'record>(
    loaded_inputs: Vec<LoadedAnalysisInput<'record>>,
    language_overrides: &HashMap<usize, String>,
    cancel: &AtomicBool,
    parallel_map: bool,
    tokenization_config: &TokenizationConfig,
    cache_enabled: bool,
) -> Result<Vec<MappedAnalysisInput<'record>>, AnalysisBuildError> {
    if parallel_map {
        loaded_inputs
            .into_par_iter()
            .map(|input| {
                map_loaded_analysis_input(
                    input,
                    language_overrides,
                    cancel,
                    tokenization_config,
                    cache_enabled,
                )
            })
            .collect()
    } else {
        loaded_inputs
            .into_iter()
            .map(|input| {
                map_loaded_analysis_input(
                    input,
                    language_overrides,
                    cancel,
                    tokenization_config,
                    cache_enabled,
                )
            })
            .collect()
    }
}

fn map_loaded_analysis_input<'record>(
    input: LoadedAnalysisInput<'record>,
    language_overrides: &HashMap<usize, String>,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    cache_enabled: bool,
) -> Result<MappedAnalysisInput<'record>, AnalysisBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        return Err(AnalysisBuildError::Cancelled);
    }
    match input {
        LoadedAnalysisInput::Ready {
            original_record_index,
            record,
            text,
        } => {
            let processed_hash_sha256 = if cache_enabled {
                crate::hash::sha256_hex(text.as_bytes())
            } else {
                String::new()
            };
            let tokens = analysis_word_tokens(&text, 0, tokenization_config);
            let token_keys = tokens
                .iter()
                .map(|token| token.aggregation_key(tokenization_config))
                .collect::<Vec<_>>();
            let document_mattr = mattr(&token_keys, MATTR_WINDOW_TOKENS);
            let document_mtld = (token_keys.len() >= MTLD_MIN_DOCUMENT_TOKENS)
                .then(|| mtld(&token_keys, MTLD_TTR_THRESHOLD))
                .flatten();
            let prepared_terms = collect_local_term_counts(token_keys.iter().cloned());
            let surface_terms =
                collect_local_term_counts(tokens.iter().map(|token| token.surface.to_string()));
            drop(tokens);
            let document_language = detect_and_resolve(
                &text,
                language_overrides
                    .get(&original_record_index)
                    .map(String::as_str),
            );
            Ok(MappedAnalysisInput::Ready(MappedAnalysisDocument {
                record,
                token_count: token_keys.len() as u64,
                prepared_terms,
                surface_terms,
                mattr: document_mattr,
                mtld: document_mtld,
                language: Some(document_language.resolution.effective_language),
                language_confidence: document_language.resolution.confidence,
                language_is_overridden: document_language.resolution.is_overridden,
                processed_hash_sha256,
            }))
        }
        LoadedAnalysisInput::Skipped => Ok(MappedAnalysisInput::Skipped),
    }
}

fn collect_local_term_counts(tokens: impl Iterator<Item = String>) -> Vec<LocalTermCount> {
    let mut term_lookup = HashMap::<String, usize>::new();
    let mut terms = Vec::<LocalTermCount>::new();
    for (index, token) in tokens.enumerate() {
        if let Some(&term_index) = term_lookup.get(&token) {
            terms[term_index].count += 1;
        } else {
            let term_index = terms.len();
            term_lookup.insert(token.clone(), term_index);
            terms.push(LocalTermCount {
                token,
                count: 1,
                first_token_position: index as u64 + 1,
            });
        }
    }
    terms
}

fn assign_prepared_term_ids(
    document: &MappedAnalysisDocument<'_>,
    term_index: &mut HashMap<String, u32>,
    terms: &mut Vec<AnalysisTerm>,
    growth_tokens: &mut u64,
    next_growth_checkpoint: &mut u64,
    vocabulary_growth: &mut Vec<VocabularyGrowthPoint>,
) -> Result<Vec<(u32, u64)>, AnalysisBuildError> {
    let document_start_tokens = *growth_tokens;
    let mut document_term_counts = Vec::with_capacity(document.prepared_terms.len());
    for local_term in &document.prepared_terms {
        let first_absolute_position = document_start_tokens
            .checked_add(local_term.first_token_position)
            .ok_or_else(|| {
                AnalysisBuildError::Message(
                    "Analysis contains more than u64::MAX tokens.".to_string(),
                )
            })?;
        while *next_growth_checkpoint < first_absolute_position {
            if !record_vocabulary_growth_checkpoint(
                vocabulary_growth,
                next_growth_checkpoint,
                terms.len(),
            ) {
                break;
            }
        }
        let term_id = prepared_term_id(&local_term.token, term_index, terms)?;
        while *next_growth_checkpoint <= first_absolute_position {
            if !record_vocabulary_growth_checkpoint(
                vocabulary_growth,
                next_growth_checkpoint,
                terms.len(),
            ) {
                break;
            }
        }
        document_term_counts.push((term_id, local_term.count));
    }
    *growth_tokens = growth_tokens
        .checked_add(document.token_count)
        .ok_or_else(|| {
            AnalysisBuildError::Message("Analysis contains more than u64::MAX tokens.".to_string())
        })?;
    while *next_growth_checkpoint <= *growth_tokens {
        if !record_vocabulary_growth_checkpoint(
            vocabulary_growth,
            next_growth_checkpoint,
            terms.len(),
        ) {
            break;
        }
    }
    Ok(document_term_counts)
}

fn record_vocabulary_growth_checkpoint(
    vocabulary_growth: &mut Vec<VocabularyGrowthPoint>,
    next_growth_checkpoint: &mut u64,
    types: usize,
) -> bool {
    vocabulary_growth.push(VocabularyGrowthPoint {
        tokens: *next_growth_checkpoint,
        types,
    });
    let next = next_growth_checkpoint.saturating_mul(2);
    if next == *next_growth_checkpoint {
        return false;
    }
    *next_growth_checkpoint = next;
    true
}

fn prepared_term_id(
    token: &str,
    term_index: &mut HashMap<String, u32>,
    terms: &mut Vec<AnalysisTerm>,
) -> Result<u32, AnalysisBuildError> {
    if let Some(&term_id) = term_index.get(token) {
        return Ok(term_id);
    }
    let term_id = u32::try_from(terms.len()).map_err(|_| {
        AnalysisBuildError::Message(
            "Analysis vocabulary contains more than u32::MAX types.".to_string(),
        )
    })?;
    term_index.insert(token.to_string(), term_id);
    terms.push(AnalysisTerm {
        token: token.to_string(),
        frequency: 0,
        document_frequency: 0,
        document_counts: Vec::new(),
        dispersion: 0.0,
    });
    Ok(term_id)
}

fn surface_term_id(
    token: &str,
    surface_term_index: &mut HashMap<String, u32>,
    surface_terms: &mut Vec<AnalysisSurfaceTerm>,
) -> Result<u32, AnalysisBuildError> {
    if let Some(&term_id) = surface_term_index.get(token) {
        return Ok(term_id);
    }
    let term_id = u32::try_from(surface_terms.len()).map_err(|_| {
        AnalysisBuildError::Message(
            "Analysis surface vocabulary contains more than u32::MAX types.".to_string(),
        )
    })?;
    surface_term_index.insert(token.to_string(), term_id);
    surface_terms.push(AnalysisSurfaceTerm {
        token: token.to_string(),
        document_counts: Vec::new(),
    });
    Ok(term_id)
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
