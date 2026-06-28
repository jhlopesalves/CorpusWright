//! Rust-owned corpus analysis and bounded query APIs.

use crate::annotation::{
    AnnotatedToken, AnnotationError, LemmaTable, ModelManager, UniversalPos, default_model_dir,
};
use crate::clean::CleaningConfig;
use crate::language::detect_and_resolve;
use crate::scan::{DocumentRecord, DocumentType};
use crate::tokenization::{TokenizationConfig, tokenize};
use rayon::prelude::*;
use regex::Regex;
use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

const OVERVIEW_DOCUMENT_LIMIT: usize = 20;
const FREQUENT_TERM_THRESHOLD: u64 = 10;
const HIGH_DISPERSION_THRESHOLD: f64 = 0.5;
const MAX_QUERY_WINDOW: usize = 500;
const MAX_DISTRIBUTION_POINTS: usize = 200;
const CONCORDANCE_DEFAULT_WINDOW_TOKENS: usize = 5;
const CONCORDANCE_MAX_WINDOW_TOKENS: usize = 20;
const CONCORDANCE_DEFAULT_LIMIT: usize = 100;
const CONCORDANCE_MAX_LIMIT: usize = 500;
const ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE: &str =
    "Build the annotation cache to enable lemma and POS frequency.";
// These lexical-diversity parameters are provisional and should remain visible in the UI.
const MATTR_WINDOW_TOKENS: usize = 100;
const MTLD_TTR_THRESHOLD: f64 = 0.720;
const MTLD_MIN_DOCUMENT_TOKENS: usize = 100;

/// Heuristic DP threshold below which an item is treated as broadly dispersed.
///
/// Plain Gries-style DP is unnormalised here: its upper bound depends on the
/// document-size shares, so these labels are corpus-internal orientation aids,
/// not cross-corpus cut-offs.
const ANNOTATED_DP_BROADLY_DISPERSED_MAX: f64 = 0.30;
/// Heuristic DP threshold from which an item is treated as highly concentrated.
const ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN: f64 = 0.70;

// These are provisional descriptive thresholds for orientation, not inferential tests.
mod overview_thresholds {
    pub(super) const BALANCE_WORTH_A_LOOK: f64 = 0.15;
    pub(super) const BALANCE_NEEDS_FIXING: f64 = 0.25;
    pub(super) const LOW_EFFECTIVE_DOCUMENT_RATIO: f64 = 0.70;
    pub(super) const NUMERIC_WORTH_A_LOOK: f64 = 0.05;
    pub(super) const NUMERIC_NEEDS_FIXING: f64 = 0.20;
    pub(super) const DISPERSION_WORTH_A_LOOK: f64 = 0.30;
    pub(super) const DISPERSION_NEEDS_FIXING: f64 = 0.60;
    pub(super) const DEGENERATE_TYPE_COUNT: usize = 50;
}

static ENGLISH_STOPWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "a about above after again against all am an and any are as at be because been before \
     being below between both but by can did do does doing down during each few for from \
     further had has have having he her here hers herself him himself his how i if in into \
     is it its itself just me more most my myself no nor not now of off on once only or \
     other our ours ourselves out over own same she should so some such than that the their \
     theirs them themselves then there these they this those through to too under until up \
     very was we were what when where which while who whom why will with you your yours \
     yourself yourselves"
        .split_whitespace()
        .collect()
});

/// Text source used to construct an analysis index.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisInputSource {
    Processed,
    ProcessedWithSourceTextFallback,
    SourceTextFallback,
}

/// Provenance of the text offered to the analysis index for one source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisInputProvenance {
    Processed,
    SourceTextFallback,
    EmptyOrFailed,
}

/// Outcome of resolving one source for analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisInputStatus {
    Ready,
    ReadyWithFallback { processed_error: String },
    Empty,
    Failed { error: String },
}

/// One source resolved through the active processing pipeline for analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisInputResolution {
    pub text: Option<String>,
    pub provenance: AnalysisInputProvenance,
    pub source_id: String,
    pub source_name: String,
    pub status: AnalysisInputStatus,
}

/// Resolves one analysis input, preferring the active processed pipeline.
///
/// Source text is used only when processed text cannot be produced. An empty
/// processed result is intentionally excluded rather than replaced with raw
/// source text.
pub fn resolve_analysis_input<ProcessedLoader, SourceTextLoader>(
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
    load_processed: ProcessedLoader,
    load_source_text: SourceTextLoader,
) -> AnalysisInputResolution
where
    ProcessedLoader: FnOnce(&DocumentRecord, &CleaningConfig) -> Result<String, String>,
    SourceTextLoader: FnOnce(&DocumentRecord, &CleaningConfig) -> Result<String, String>,
{
    let source_id = record.source_path.to_string_lossy().into_owned();
    let source_name = record.relative_path.to_string_lossy().into_owned();

    match load_processed(record, cleaning_config) {
        Ok(text) if !text.trim().is_empty() => AnalysisInputResolution {
            text: Some(text),
            provenance: AnalysisInputProvenance::Processed,
            source_id,
            source_name,
            status: AnalysisInputStatus::Ready,
        },
        Ok(_) => AnalysisInputResolution {
            text: None,
            provenance: AnalysisInputProvenance::EmptyOrFailed,
            source_id,
            source_name,
            status: AnalysisInputStatus::Empty,
        },
        Err(processed_error) => match load_source_text(record, cleaning_config) {
            Ok(text) if !text.trim().is_empty() => AnalysisInputResolution {
                text: Some(text),
                provenance: AnalysisInputProvenance::SourceTextFallback,
                source_id,
                source_name,
                status: AnalysisInputStatus::ReadyWithFallback { processed_error },
            },
            Ok(_) => AnalysisInputResolution {
                text: None,
                provenance: AnalysisInputProvenance::EmptyOrFailed,
                source_id,
                source_name,
                status: AnalysisInputStatus::Empty,
            },
            Err(source_error) => AnalysisInputResolution {
                text: None,
                provenance: AnalysisInputProvenance::EmptyOrFailed,
                source_id,
                source_name,
                status: AnalysisInputStatus::Failed {
                    error: format!(
                        "Processed text failed: {processed_error} Source text failed: {source_error}"
                    ),
                },
            },
        },
    }
}

/// Returns a stable signature for every active processing option and rule.
pub fn cleaning_config_signature(cleaning_config: &CleaningConfig) -> String {
    let bytes = serde_json::to_vec(cleaning_config)
        .expect("CleaningConfig serialization must remain infallible");
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

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

/// Corpus totals computed during the Rust build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewTotals {
    pub tokens: u64,
    pub types: usize,
    pub documents: usize,
}

/// Project and analysis population counts available for this snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewPopulation {
    pub sources_in_project: Option<usize>,
    pub included_in_analysis: usize,
    pub processed_used: usize,
    pub using_source_text_fallback: usize,
    pub empty_or_failed: usize,
    pub sources_with_warnings: Option<usize>,
    pub input_source: String,
    pub reading: String,
}

/// One bounded document-length row in the overview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewDocument {
    pub name: String,
    pub tokens: u64,
    pub share: Option<f64>,
    pub relative_length: Option<f64>,
}

/// Document-level balance and concentration metrics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewBalance {
    pub largest_share: Option<f64>,
    pub top3_share: Option<f64>,
    pub gini: Option<f64>,
    pub coefficient_of_variation: Option<f64>,
    pub effective_documents: Option<f64>,
    pub median_tokens: Option<f64>,
    pub minimum_tokens: Option<u64>,
    pub maximum_tokens: Option<u64>,
    pub remaining_documents: usize,
    pub reading: String,
}

/// Type-form diagnostics computed from the indexed vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewExtractionQuality {
    pub word_like_count: usize,
    pub word_like_share: Option<f64>,
    pub numeric_only_count: usize,
    pub numeric_only_share: Option<f64>,
    pub single_character_count: Option<usize>,
    pub single_character_share: Option<f64>,
    pub reading: String,
}

/// Frequency-class partition of the indexed vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewLexicalSpectrum {
    pub types: usize,
    pub hapax_count: usize,
    pub dis_legomena_count: usize,
    pub band_3_to_9_count: usize,
    pub band_10plus_count: usize,
    pub hapax_share: Option<f64>,
    pub dis_legomena_share: Option<f64>,
    pub band_3_to_9_share: Option<f64>,
    pub band_10plus_share: Option<f64>,
    pub frequency_spectrum: Vec<FrequencySpectrumPoint>,
    pub frequency_spectrum_truncated: bool,
    pub mattr_window: u32,
    pub mattr_median: Option<f64>,
    pub mattr_min: Option<f64>,
    pub mattr_max: Option<f64>,
    pub mattr_docs_excluded: u32,
    pub mattr_docs_used: u32,
    pub mtld_threshold: f64,
    pub mtld_min_document_tokens: u32,
    pub mtld_median: Option<f64>,
    pub mtld_min: Option<f64>,
    pub mtld_max: Option<f64>,
    pub mtld_docs_excluded: u32,
    pub mtld_docs_used: u32,
    pub reading: String,
}

/// Token-mass coverage at vocabulary thresholds supported by this corpus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewFrequencyConcentration {
    pub top_100_coverage: Option<f64>,
    pub top_1000_coverage: Option<f64>,
    pub top_10000_coverage: Option<f64>,
    pub shannon_entropy_bits: Option<f64>,
    pub evenness_j: Option<f64>,
    pub perplexity: Option<f64>,
    pub effective_vocabulary_entropy: Option<f64>,
    pub reading: String,
}

/// Number of types observed at one exact corpus frequency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrequencySpectrumPoint {
    pub frequency: u64,
    pub type_count: usize,
}

/// One bounded point from the corpus rank-frequency distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankFrequencyPoint {
    pub rank: u64,
    pub frequency: u64,
    pub rate_per_million: f64,
}

/// Zipf fit and bounded rank-frequency series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewRankFrequency {
    pub zipf_exponent: Option<f64>,
    pub zipf_coefficient: Option<f64>,
    pub r_squared: Option<f64>,
    pub fitted_rank_start: Option<u64>,
    pub fitted_rank_end: Option<u64>,
    pub points: Vec<RankFrequencyPoint>,
    pub reading: String,
}

/// One deterministic, bounded vocabulary-growth checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyGrowthPoint {
    pub tokens: u64,
    pub types: usize,
}

/// Heaps-law fit and bounded vocabulary-growth series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewVocabularyGrowth {
    pub heaps_k: Option<f64>,
    pub heaps_beta: Option<f64>,
    pub r_squared: Option<f64>,
    pub points: Vec<VocabularyGrowthPoint>,
    pub reading: String,
}

/// Vocabulary dispersion and document-frequency summaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewDispersion {
    pub frequent_types: usize,
    pub high_dp_frequent_types: usize,
    pub median_dp: Option<f64>,
    pub df_in_1_count: usize,
    pub df_in_2_to_n_minus_1_count: usize,
    pub df_in_all_count: usize,
    pub df_in_1_share: Option<f64>,
    pub df_in_2_to_n_minus_1_share: Option<f64>,
    pub df_in_all_share: Option<f64>,
    pub median_documents_per_type: Option<f64>,
    pub reading: String,
}

/// Overview finding category.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CorpusOverviewFindingKind {
    Balance,
    ExtractionQuality,
    LexicalSpectrum,
    Dispersion,
}

/// Descriptive attention level assigned by the overview thresholds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum CorpusOverviewFindingSeverity {
    NoIssue,
    WorthALook,
    NeedsFixing,
}

impl CorpusOverviewFindingSeverity {
    fn label(self) -> &'static str {
        match self {
            Self::NoIssue => "no immediate issue",
            Self::WorthALook => "worth a look",
            Self::NeedsFixing => "needs fixing",
        }
    }
}

/// One computed finding shown in the corpus reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewFinding {
    pub key: CorpusOverviewFindingKind,
    pub severity: CorpusOverviewFindingSeverity,
    pub severity_label: String,
    pub text: String,
}

/// Headline interpretation derived from the computed overview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewReading {
    pub verdict: String,
    pub findings: Vec<CorpusOverviewFinding>,
}

/// Analysis build details that are known for this snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverviewBuild {
    pub tokenisation: Option<String>,
    pub case_handling: Option<String>,
    pub lemmatisation: Option<String>,
    pub number_punctuation_handling: Option<String>,
    pub input_source: String,
    pub config_version: Option<String>,
    pub snapshot_date: Option<String>,
    pub metadata_attached: bool,
}

/// Project metadata supplied by the desktop layer when an analysis is built.
#[derive(Debug, Clone, Default)]
pub struct CorpusOverviewContext {
    pub sources_in_project: Option<usize>,
    pub sources_with_warnings: Option<usize>,
    pub failed_sources: usize,
    pub config_version: Option<String>,
    pub metadata_attached: bool,
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

/// Render-ready first-stage corpus EDA summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverview {
    pub document_count: usize,
    pub source_count: usize,
    pub total_tokens: u64,
    pub total_types: usize,
    pub annotation_available: bool,
    pub input_text_source: AnalysisInputSource,
    pub built_at: String,
    pub build_signature: String,
    pub processing_config_signature: String,
    pub skipped_documents: usize,
    pub skipped_pdf_documents: usize,
    pub totals: CorpusOverviewTotals,
    pub population: CorpusOverviewPopulation,
    pub documents: Vec<CorpusOverviewDocument>,
    pub documents_truncated: bool,
    pub balance: CorpusOverviewBalance,
    pub extraction_quality: CorpusOverviewExtractionQuality,
    pub lexical_spectrum: CorpusOverviewLexicalSpectrum,
    pub frequency_concentration: CorpusOverviewFrequencyConcentration,
    pub rank_frequency: CorpusOverviewRankFrequency,
    pub vocabulary_growth: CorpusOverviewVocabularyGrowth,
    pub dispersion: CorpusOverviewDispersion,
    pub reading: CorpusOverviewReading,
    pub build: CorpusOverviewBuild,
}

#[derive(Debug, Clone)]
struct AnalysisDocument {
    summary: AnalysisDocumentSummary,
    surface_term_counts: Vec<(u32, u64)>,
    annotated_term_counts: Vec<AnnotatedTermCount>,
    annotation_available: bool,
    mattr: Option<f64>,
    mtld: Option<f64>,
    processed_hash_sha256: String,
}

#[derive(Debug, Clone)]
struct AnalysisTerm {
    token: String,
    frequency: u64,
    document_frequency: usize,
    document_counts: Vec<(usize, u64)>,
    dispersion: f64,
}

#[derive(Debug, Clone)]
struct AnalysisSurfaceTerm {
    token: String,
    document_counts: Vec<(usize, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotatedTermCount {
    pub surface_term_id: u32,
    pub lemma_id: u32,
    pub upos: UniversalPos,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationLayer {
    pub documents: Vec<AnnotationLayerDocument>,
    pub lemma_table: LemmaTable,
    pub languages_used: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationLayerDocument {
    /// Index into `AnalysisCorpus.documents`.
    pub analysis_document_index: usize,
    pub source_path: String,
    pub relative_path: String,
    pub term_counts: Vec<AnnotatedTermCount>,
}

#[derive(Debug, Clone, Default)]
pub struct AnnotationLayerBuildOptions {
    pub udpipe_model_dir: Option<PathBuf>,
    /// Per-document language override keyed by the original records slice,
    /// before the corpus build sorts records into analysis order.
    pub language_overrides: HashMap<usize, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationLayerBuildError {
    Cancelled,
    TextLoadFailed {
        source_path: String,
        error: String,
    },
    CorpusDocumentMismatch {
        reason: String,
    },
    UnsupportedLanguages {
        languages: Vec<String>,
    },
    ModelPreflightFailed {
        language: String,
        error: String,
    },
    ModelTaggingFailed {
        analysis_document_index: usize,
        language: String,
        error: String,
    },
    AlignmentOrIdentityMismatch {
        analysis_document_index: usize,
        reason: String,
    },
}

impl std::fmt::Display for AnnotationLayerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Annotation layer build cancelled."),
            Self::TextLoadFailed { source_path, error } => {
                write!(
                    formatter,
                    "Failed to load analysis text for {source_path}: {error}"
                )
            }
            Self::CorpusDocumentMismatch { reason } => {
                write!(
                    formatter,
                    "Annotation inputs do not match the analysis corpus: {reason}"
                )
            }
            Self::UnsupportedLanguages { languages } => {
                write!(
                    formatter,
                    "No UDPipe model is available for language(s): {}",
                    languages.join(", ")
                )
            }
            Self::ModelPreflightFailed { language, error } => {
                write!(
                    formatter,
                    "Failed to preflight UDPipe model for language {language}: {error}"
                )
            }
            Self::ModelTaggingFailed {
                analysis_document_index,
                language,
                error,
            } => {
                write!(
                    formatter,
                    "UDPipe tagging failed for analysis document {analysis_document_index} ({language}): {error}"
                )
            }
            Self::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason,
            } => {
                write!(
                    formatter,
                    "Annotation identity mismatch for analysis document {analysis_document_index}: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for AnnotationLayerBuildError {}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type")]
pub enum AnnotationCacheBuildPhase {
    BuildingSurface { phase: String },
    LoadingModel,
    Annotating,
    WritingCache,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct AnnotationCacheBuildProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub current_phase: AnnotationCacheBuildPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationCacheBuildReport {
    pub documents_annotated: usize,
    pub total_annotated_term_count_rows: usize,
    pub languages_used: Vec<String>,
    pub cache_root: String,
    pub reused_existing: bool,
    pub cache_written: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AnnotationCacheBuildError {
    Cancelled,
    MissingCacheRoot,
    SurfaceBuildFailed { error: String },
    UnsupportedLanguages { languages: Vec<String> },
    AnnotationFailed { error: String },
    ManifestError { error: String },
    CacheWriteError { error: String },
}

impl std::fmt::Display for AnnotationCacheBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Annotation cache build cancelled."),
            Self::MissingCacheRoot => formatter.write_str("Missing annotation cache root."),
            Self::SurfaceBuildFailed { error } => {
                write!(formatter, "Surface build failed: {error}")
            }
            Self::UnsupportedLanguages { languages } => {
                write!(
                    formatter,
                    "No UDPipe model is available for language(s): {}",
                    languages.join(", ")
                )
            }
            Self::AnnotationFailed { error } => {
                write!(formatter, "Annotation build failed: {error}")
            }
            Self::ManifestError { error } => write!(formatter, "Manifest error: {error}"),
            Self::CacheWriteError { error } => write!(formatter, "Cache write error: {error}"),
        }
    }
}

impl std::error::Error for AnnotationCacheBuildError {}

struct AnalysisBuildMetadata {
    source_count: usize,
    total_tokens: u64,
    input_text_source: AnalysisInputSource,
    processed_documents: usize,
    source_text_fallback_documents: usize,
    built_at: String,
    build_signature: String,
    processing_config_signature: String,
    skipped_documents: usize,
    skipped_pdf_documents: usize,
    vocabulary_growth: Vec<VocabularyGrowthPoint>,
    overview_context: CorpusOverviewContext,
}

struct AnalysisBuildProduct {
    corpus: AnalysisCorpus,
    #[cfg(test)]
    terms: Vec<AnalysisTerm>,
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

struct AnnotationDocumentInput {
    analysis_document_index: usize,
    source_path: String,
    relative_path: String,
    text: String,
    language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalAnnotatedTermCount {
    surface_term_id: u32,
    lemma: String,
    upos: UniversalPos,
    count: u64,
}

struct WorkerAnnotationDocument {
    analysis_document_index: usize,
    source_path: String,
    relative_path: String,
    term_counts: Vec<LocalAnnotatedTermCount>,
}

/// Sparse Rust-owned analysis index.
#[derive(Debug, Clone)]
pub struct AnalysisCorpus {
    documents: Vec<AnalysisDocument>,
    surface_terms: Vec<AnalysisSurfaceTerm>,
    lemma_table: LemmaTable,
    overview: CorpusOverview,
    pub tokenization: TokenizationConfig,
}

impl AnalysisCorpus {
    /// Returns the compact summary computed with the index.
    pub fn overview(&self) -> &CorpusOverview {
        &self.overview
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

/// Match mode for frequency token queries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyMatchMode {
    Contains,
    Exact,
    StartsWith,
    Regex,
}

/// Token-class filter applied by Rust.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyTokenClass {
    Alphabetic,
    IncludeSingles,
    IncludeNumerals,
    All,
}

/// Sort column for frequency results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencySortKey {
    Token,
    Frequency,
    Range,
    Rate,
    Dispersion,
}

/// Sort direction for frequency results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Grouping strategy for the bounded frequency query.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyGroupMode {
    #[default]
    Surface,
    Stem,
    Lemma,
}

/// Machine-readable notices returned with frequency query windows.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyNoticeKind {
    StemmingUnsupportedLanguage,
    AnnotationUnavailable,
    PosFilterUnavailable,
}

/// One explicit notice about a frequency query fallback or limitation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyNotice {
    pub kind: FrequencyNoticeKind,
    pub message: String,
}

/// Parameters for one bounded frequency-list query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyQuery {
    pub selection: AnalysisDocumentSelection,
    pub query: String,
    pub match_mode: FrequencyMatchMode,
    #[ts(type = "number")]
    pub minimum_frequency: u64,
    pub token_class: FrequencyTokenClass,
    pub hide_stopwords: bool,
    #[serde(default)]
    pub group_by: FrequencyGroupMode,
    #[serde(default)]
    pub case_fold: bool,
    #[serde(default)]
    pub language: Option<String>,
    /// UPOS filter (e.g., "NOUN").
    #[serde(default)]
    pub pos_filter: Option<String>,
    pub sort_key: FrequencySortKey,
    pub sort_direction: SortDirection,
    pub offset: usize,
    pub limit: usize,
}

/// One frequency result row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyTermItem {
    pub token: String,
    #[ts(type = "number")]
    pub frequency: u64,
    pub document_frequency: usize,
    pub rate_per_10k: f64,
    pub rate_per_million: f64,
    pub dispersion: f64,
}

/// Bounded frequency-list response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyPage {
    pub total_terms: usize,
    pub scoped_total_types: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<FrequencyTermItem>,
    pub selected_document_count: usize,
    pub total_document_count: usize,
    #[ts(type = "number")]
    pub scoped_total_tokens: u64,
    pub query_error: Option<String>,
    pub notices: Vec<FrequencyNotice>,
}

/// Annotated frequency view requested by the desktop panel.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotatedFrequencyMode {
    LemmaPos,
    Pos,
}

/// Cache state used to gate annotation-only frequency views.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotationCacheState {
    Ready,
    Missing,
    Stale,
    Invalid,
}

/// Readiness detail derived from the existing annotation-cache loader.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotationCacheAvailability {
    pub state: AnnotationCacheState,
    pub detail: Option<String>,
}

/// Parameters for a bounded annotated frequency query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotatedFrequencyQuery {
    pub selection: AnalysisDocumentSelection,
    pub mode: AnnotatedFrequencyMode,
    pub offset: usize,
    pub limit: usize,
}

/// UPOS frequency with document dispersion statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PosFrequencyRow {
    pub upos: String,
    #[ts(type = "number")]
    pub raw: u64,
    pub per_10k: f64,
    pub doc_freq: usize,
    pub doc_total: usize,
    pub dp: f64,
    pub dispersion_label: String,
    pub top_doc_share: f64,
}

/// Lemma + UPOS frequency with document dispersion statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LexFrequencyRow {
    pub lemma: String,
    pub upos: String,
    #[ts(type = "number")]
    pub raw: u64,
    pub per_10k: f64,
    pub doc_freq: usize,
    pub doc_total: usize,
    pub dp: f64,
    pub dispersion_label: String,
    pub top_doc_share: f64,
}

/// Bounded response for annotation-only frequency views.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotatedFrequencyResult {
    pub mode: AnnotatedFrequencyMode,
    pub cache: AnnotationCacheAvailability,
    pub unavailable_message: Option<String>,
    #[ts(type = "number")]
    pub total_tokens: u64,
    pub selected_document_count: usize,
    pub total_document_count: usize,
    pub total_rows: usize,
    pub offset: usize,
    pub limit: usize,
    pub pos_rows: Vec<PosFrequencyRow>,
    pub lex_rows: Vec<LexFrequencyRow>,
}

/// Parameters for a bounded surface-token concordance query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceQuery {
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    pub window_tokens: usize,
    pub offset: usize,
    pub limit: usize,
}

/// One KWIC row with token context around the matching node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceRow {
    pub document_index: usize,
    pub source_path: String,
    pub display_name: String,
    pub token_index: usize,
    pub left_context: Vec<String>,
    pub node: String,
    pub right_context: Vec<String>,
}

/// Bounded surface-token concordance response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceResult {
    pub query: String,
    pub case_sensitive: bool,
    pub window_tokens: usize,
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub offset: usize,
    pub limit: usize,
    pub rows: Vec<ConcordanceRow>,
}

/// Text supplied by the desktop layer for an on-demand concordance scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcordanceDocument {
    pub document_index: usize,
    pub source_path: String,
    pub display_name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcordanceError {
    EmptyQuery,
    MultiTokenQuery,
}

impl std::fmt::Display for ConcordanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyQuery => formatter.write_str("Enter one surface word to search."),
            Self::MultiTokenQuery => {
                formatter.write_str("KWIC alpha currently supports one-token surface queries only.")
            }
        }
    }
}

impl std::error::Error for ConcordanceError {}

/// One document in a bounded file-scope picker response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDocumentPageItem {
    pub id: usize,
    pub source_path: String,
    pub relative_path: String,
    pub token_count: u64,
    pub selected: bool,
}

/// Bounded analysis-document response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDocumentPage {
    pub total_count: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<AnalysisDocumentPageItem>,
    pub selected_count: usize,
    pub total_document_count: usize,
}

/// One document row in a term-distribution response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TermDistributionItem {
    pub document_id: usize,
    pub source_path: String,
    pub relative_path: String,
    pub token_count: u64,
    pub count: u64,
    pub rate_per_10k: f64,
    pub rate_per_million: f64,
}

/// Bounded per-document term distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TermDistributionPage {
    pub token: String,
    pub total_documents: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<TermDistributionItem>,
    pub max_rate_per_10k: f64,
    pub max_rate_per_million: f64,
}

/// Parameters for one bounded per-document frequency distribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermDistributionQuery {
    pub selection: AnalysisDocumentSelection,
    pub group_by: FrequencyGroupMode,
    pub case_fold: bool,
    pub language: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

fn percentile(sorted_values: &[u64], percentile_value: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let position = (sorted_values.len() - 1) as f64 * percentile_value;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return Some(sorted_values[lower] as f64);
    }
    let weight = position - lower as f64;
    Some(sorted_values[lower] as f64 * (1.0 - weight) + sorted_values[upper] as f64 * weight)
}

fn median_f64(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) / 2.0)
    } else {
        Some(values[middle])
    }
}

fn mattr(tokens: &[String], window: usize) -> Option<f64> {
    if window == 0 || tokens.len() < window {
        return None;
    }

    let mut counts = HashMap::<&str, usize>::new();
    for token in &tokens[..window] {
        *counts.entry(token.as_str()).or_default() += 1;
    }
    let mut ttr_sum = counts.len() as f64 / window as f64;
    let window_count = tokens.len() - window + 1;
    for start in 1..window_count {
        let outgoing = tokens[start - 1].as_str();
        let remove_outgoing = counts.get_mut(outgoing).is_some_and(|count| {
            *count -= 1;
            *count == 0
        });
        if remove_outgoing {
            counts.remove(outgoing);
        }
        *counts
            .entry(tokens[start + window - 1].as_str())
            .or_default() += 1;
        ttr_sum += counts.len() as f64 / window as f64;
    }
    Some(ttr_sum / window_count as f64)
}

fn mtld_direction<'a>(
    tokens: impl Iterator<Item = &'a str>,
    token_count: usize,
    threshold: f64,
) -> Option<f64> {
    if token_count == 0 || !(0.0..1.0).contains(&threshold) {
        return None;
    }

    let mut types = HashSet::new();
    let mut segment_tokens = 0usize;
    let mut factors = 0.0;
    for token in tokens {
        segment_tokens += 1;
        types.insert(token);
        let ttr = types.len() as f64 / segment_tokens as f64;
        if ttr <= threshold {
            factors += 1.0;
            types.clear();
            segment_tokens = 0;
        }
    }
    if segment_tokens > 0 {
        let ttr = types.len() as f64 / segment_tokens as f64;
        factors += (1.0 - ttr) / (1.0 - threshold);
    }
    (factors > 0.0)
        .then(|| token_count as f64 / factors)
        .filter(|value| value.is_finite())
}

fn mtld(tokens: &[String], threshold: f64) -> Option<f64> {
    let forward = mtld_direction(tokens.iter().map(String::as_str), tokens.len(), threshold)?;
    let backward = mtld_direction(
        tokens.iter().rev().map(String::as_str),
        tokens.len(),
        threshold,
    )?;
    Some((forward + backward) / 2.0)
}

#[derive(Debug, Clone, Copy)]
struct MetricSummary {
    median: Option<f64>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    documents_used: usize,
    documents_excluded: usize,
}

fn summarise_document_metric(
    documents: &[AnalysisDocument],
    metric: impl Fn(&AnalysisDocument) -> Option<f64>,
) -> MetricSummary {
    let mut values = documents
        .iter()
        .filter_map(metric)
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    let documents_used = values.len();
    let minimum = values.iter().copied().min_by(f64::total_cmp);
    let maximum = values.iter().copied().max_by(f64::total_cmp);
    let median = median_f64(&mut values);
    MetricSummary {
        median,
        minimum,
        maximum,
        documents_used,
        documents_excluded: documents.len().saturating_sub(documents_used),
    }
}

fn entropy_metrics(
    frequencies: &[u64],
    total_tokens: u64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    if total_tokens == 0 || frequencies.len() <= 1 {
        return (None, None, None);
    }
    let entropy = -frequencies
        .iter()
        .filter(|frequency| **frequency > 0)
        .map(|frequency| {
            let probability = *frequency as f64 / total_tokens as f64;
            probability * probability.log2()
        })
        .sum::<f64>();
    let evenness = (entropy / (frequencies.len() as f64).log2()).clamp(0.0, 1.0);
    let effective_vocabulary = 2.0f64.powf(entropy);
    if entropy.is_finite() && evenness.is_finite() && effective_vocabulary.is_finite() {
        (Some(entropy), Some(evenness), Some(effective_vocabulary))
    } else {
        (None, None, None)
    }
}

fn linear_regression(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if points.len() < 2 {
        return None;
    }
    let count = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / count;
    let ss_x = points
        .iter()
        .map(|(x, _)| (x - mean_x).powi(2))
        .sum::<f64>();
    if ss_x <= f64::EPSILON {
        return None;
    }
    let covariance = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>();
    let slope = covariance / ss_x;
    let intercept = mean_y - slope * mean_x;
    let ss_total = points
        .iter()
        .map(|(_, y)| (y - mean_y).powi(2))
        .sum::<f64>();
    let ss_residual = points
        .iter()
        .map(|(x, y)| (y - (intercept + slope * x)).powi(2))
        .sum::<f64>();
    let r_squared = if ss_total <= f64::EPSILON {
        1.0
    } else {
        (1.0 - ss_residual / ss_total).clamp(0.0, 1.0)
    };
    (slope.is_finite() && intercept.is_finite() && r_squared.is_finite())
        .then_some((slope, intercept, r_squared))
}

fn sampled_indices(length: usize, maximum: usize) -> Vec<usize> {
    if length <= maximum {
        return (0..length).collect();
    }
    (0..maximum)
        .map(|index| index * (length - 1) / (maximum - 1))
        .collect()
}

fn rank_frequency_summary(frequencies: &[u64], total_tokens: u64) -> CorpusOverviewRankFrequency {
    let fit_points = frequencies
        .iter()
        .enumerate()
        .filter(|(index, frequency)| *index > 0 && **frequency > 1)
        .map(|(index, frequency)| (((index + 1) as f64).ln(), (*frequency as f64).ln()))
        .collect::<Vec<_>>();
    let fit = linear_regression(&fit_points);
    let fitted_rank_start = fit.as_ref().map(|_| 2);
    let fitted_rank_end = fit
        .as_ref()
        .map(|_| u64::try_from(fit_points.len() + 1).unwrap_or(u64::MAX));
    let points = sampled_indices(frequencies.len(), MAX_DISTRIBUTION_POINTS)
        .into_iter()
        .map(|index| RankFrequencyPoint {
            rank: u64::try_from(index + 1).unwrap_or(u64::MAX),
            frequency: frequencies[index],
            rate_per_million: if total_tokens > 0 {
                frequencies[index] as f64 / total_tokens as f64 * 1_000_000.0
            } else {
                0.0
            },
        })
        .collect();
    CorpusOverviewRankFrequency {
        zipf_exponent: fit.map(|(slope, _, _)| -slope),
        zipf_coefficient: fit.map(|(_, intercept, _)| intercept.exp()),
        r_squared: fit.map(|(_, _, r_squared)| r_squared),
        fitted_rank_start,
        fitted_rank_end,
        points,
        reading: "The Zipf fit uses log frequency against log rank, excluding the highest-frequency type and the frequency-one tail where the classic law is least reliable."
            .to_string(),
    }
}

fn vocabulary_growth_summary(points: &[VocabularyGrowthPoint]) -> CorpusOverviewVocabularyGrowth {
    let fit_points = points
        .iter()
        .filter(|point| point.tokens > 1 && point.types > 1)
        .map(|point| ((point.tokens as f64).ln(), (point.types as f64).ln()))
        .collect::<Vec<_>>();
    let fit = linear_regression(&fit_points);
    CorpusOverviewVocabularyGrowth {
        heaps_k: fit.map(|(_, intercept, _)| intercept.exp()),
        heaps_beta: fit.map(|(slope, _, _)| slope),
        r_squared: fit.map(|(_, _, r_squared)| r_squared),
        points: points.to_vec(),
        reading: "Heaps' law is fitted to deterministic, power-of-two token checkpoints in stable source-path order; it describes this snapshot rather than predicting a universal vocabulary ceiling."
            .to_string(),
    }
}

fn frequency_spectrum(frequencies: &[u64]) -> (Vec<FrequencySpectrumPoint>, bool) {
    let mut counts = BTreeMap::<u64, usize>::new();
    for &frequency in frequencies {
        *counts.entry(frequency).or_default() += 1;
    }
    let all_points = counts
        .into_iter()
        .map(|(frequency, type_count)| FrequencySpectrumPoint {
            frequency,
            type_count,
        })
        .collect::<Vec<_>>();
    let truncated = all_points.len() > MAX_DISTRIBUTION_POINTS;
    let points = sampled_indices(all_points.len(), MAX_DISTRIBUTION_POINTS)
        .into_iter()
        .map(|index| all_points[index].clone())
        .collect();
    (points, truncated)
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

fn document_name(document: &AnalysisDocumentSummary) -> &str {
    if document.relative_path.is_empty() {
        &document.source_path
    } else {
        &document.relative_path
    }
}

fn compute_dispersion(
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

fn annotated_dispersion_label(dp: f64) -> &'static str {
    if dp < ANNOTATED_DP_BROADLY_DISPERSED_MAX {
        "broadly dispersed"
    } else if dp < ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN {
        "moderately concentrated"
    } else {
        "highly concentrated"
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FrequencyDistributionStats {
    raw: u64,
    per_10k: f64,
    doc_freq: usize,
    dp: f64,
    top_doc_share: f64,
}

fn frequency_distribution_stats(
    document_counts: &[(usize, u64)],
    raw: u64,
    documents: &[AnalysisDocument],
    total_tokens: u64,
) -> FrequencyDistributionStats {
    let doc_freq = document_counts
        .iter()
        .filter(|(_, count)| *count > 0)
        .count();
    let top_doc_share = if raw > 0 {
        document_counts
            .iter()
            .map(|(_, count)| *count)
            .max()
            .unwrap_or(0) as f64
            / raw as f64
    } else {
        0.0
    };
    FrequencyDistributionStats {
        raw,
        per_10k: if total_tokens > 0 {
            raw as f64 / total_tokens as f64 * 10_000.0
        } else {
            0.0
        },
        doc_freq,
        dp: compute_dispersion(document_counts, raw, documents, total_tokens),
        top_doc_share,
    }
}

fn gini_coefficient(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let total = sorted.iter().sum::<u64>() as f64;
    if total == 0.0 {
        return Some(0.0);
    }
    let weighted_sum = sorted
        .iter()
        .enumerate()
        .map(|(index, value)| (index + 1) as f64 * *value as f64)
        .sum::<f64>();
    let count = sorted.len() as f64;
    Some((2.0 * weighted_sum) / (count * total) - (count + 1.0) / count)
}

fn coefficient_of_variation(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mean = values.iter().sum::<u64>() as f64 / values.len() as f64;
    if mean == 0.0 {
        return None;
    }
    let variance = values
        .iter()
        .map(|value| {
            let difference = *value as f64 - mean;
            difference * difference
        })
        .sum::<f64>()
        / values.len() as f64;
    Some(variance.sqrt() / mean)
}

fn effective_document_count(values: &[u64], total_tokens: u64) -> Option<f64> {
    if values.is_empty() || total_tokens == 0 {
        return None;
    }
    let squared_shares = values
        .iter()
        .map(|value| {
            let share = *value as f64 / total_tokens as f64;
            share * share
        })
        .sum::<f64>();
    (squared_shares > 0.0).then(|| 1.0 / squared_shares)
}

fn numeric_only(token: &str) -> bool {
    // Separators are accepted only when every other scalar is numeric; this avoids
    // classifying mixed alphanumeric identifiers as numeric extraction debris.
    let mut has_digit = false;
    for character in token.chars() {
        if character.is_numeric() {
            has_digit = true;
        } else if !matches!(character, '.' | ',' | ':' | '-' | '/') {
            return false;
        }
    }
    has_digit
}

fn coverage_at(frequencies: &[u64], threshold: usize, total_tokens: u64) -> Option<f64> {
    if frequencies.len() < threshold || total_tokens == 0 {
        return None;
    }
    Some(frequencies.iter().take(threshold).sum::<u64>() as f64 / total_tokens as f64)
}

fn type_share(count: usize, total: usize) -> Option<f64> {
    (total > 0).then(|| count as f64 / total as f64)
}

fn balance_severity(
    document_count: usize,
    total_tokens: u64,
    largest_share: Option<f64>,
    effective_documents: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{
        BALANCE_NEEDS_FIXING, BALANCE_WORTH_A_LOOK, LOW_EFFECTIVE_DOCUMENT_RATIO,
    };

    if document_count == 0 || total_tokens == 0 {
        return NeedsFixing;
    }
    if document_count == 1 {
        return WorthALook;
    }
    let mut severity = match largest_share {
        Some(value) if value > BALANCE_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= BALANCE_WORTH_A_LOOK => WorthALook,
        Some(_) => NoIssue,
        None => NeedsFixing,
    };
    if effective_documents
        .is_some_and(|value| value / (document_count as f64) < LOW_EFFECTIVE_DOCUMENT_RATIO)
    {
        severity = severity.max(WorthALook);
    }
    severity
}

fn extraction_severity(
    total_types: usize,
    numeric_only_share: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{NUMERIC_NEEDS_FIXING, NUMERIC_WORTH_A_LOOK};

    if total_types == 0 {
        return NeedsFixing;
    }
    match numeric_only_share {
        Some(value) if value > NUMERIC_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= NUMERIC_WORTH_A_LOOK => WorthALook,
        _ => NoIssue,
    }
}

fn lexical_severity(total_tokens: u64, total_types: usize) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    if total_tokens == 0 {
        NeedsFixing
    } else if total_types < overview_thresholds::DEGENERATE_TYPE_COUNT {
        WorthALook
    } else {
        NoIssue
    }
}

fn dispersion_severity(
    document_count: usize,
    frequent_types: usize,
    median_dp: Option<f64>,
) -> CorpusOverviewFindingSeverity {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};
    use overview_thresholds::{DISPERSION_NEEDS_FIXING, DISPERSION_WORTH_A_LOOK};

    if document_count < 2 || frequent_types == 0 {
        return NoIssue;
    }
    match median_dp {
        Some(value) if value > DISPERSION_NEEDS_FIXING => NeedsFixing,
        Some(value) if value >= DISPERSION_WORTH_A_LOOK => WorthALook,
        Some(_) => NoIssue,
        None => NoIssue,
    }
}

fn finding(
    key: CorpusOverviewFindingKind,
    severity: CorpusOverviewFindingSeverity,
    text: String,
) -> CorpusOverviewFinding {
    CorpusOverviewFinding {
        key,
        severity,
        severity_label: severity.label().to_string(),
        text,
    }
}

fn analysis_input_label(processed_documents: usize, fallback_documents: usize) -> String {
    match (processed_documents, fallback_documents) {
        (0, 0) => "No usable analysis text".to_string(),
        (0, _) => "Source text fallback".to_string(),
        (_, 0) => "Processed text".to_string(),
        (_, fallback) => format!("Processed text; {fallback} using source-text fallback"),
    }
}

fn analysis_word_tokens<'text>(
    text: &'text str,
    document_id: u32,
    config: &TokenizationConfig,
) -> Vec<crate::tokenization::Token<'text>> {
    tokenize(text, document_id, config)
        .into_iter()
        .filter(|token| token.counts_as_word())
        .collect()
}

fn build_overview(
    documents: &[AnalysisDocument],
    terms: &[AnalysisTerm],
    metadata: AnalysisBuildMetadata,
) -> CorpusOverview {
    let total_tokens = metadata.total_tokens;
    let mut sorted_documents = documents
        .iter()
        .map(|document| document.summary.clone())
        .collect::<Vec<_>>();
    sorted_documents.sort_by(|left, right| {
        right
            .token_count
            .cmp(&left.token_count)
            .then_with(|| document_name(left).cmp(document_name(right)))
    });
    let mut token_counts = documents
        .iter()
        .map(|document| document.summary.token_count)
        .collect::<Vec<_>>();
    token_counts.sort_unstable();
    let median_tokens = percentile(&token_counts, 0.5);
    let largest_share = sorted_documents
        .first()
        .filter(|_| total_tokens > 0)
        .map(|document| document.token_count as f64 / total_tokens as f64);
    let top3_share = (total_tokens > 0 && !sorted_documents.is_empty()).then(|| {
        sorted_documents
            .iter()
            .take(3)
            .map(|document| document.token_count)
            .sum::<u64>() as f64
            / total_tokens as f64
    });
    let gini = gini_coefficient(&token_counts);
    let coefficient_of_variation = coefficient_of_variation(&token_counts);
    let effective_documents = effective_document_count(&token_counts, total_tokens);
    let mattr_summary = summarise_document_metric(documents, |document| document.mattr);
    let mtld_summary = summarise_document_metric(documents, |document| document.mtld);
    let balance_severity = balance_severity(
        documents.len(),
        total_tokens,
        largest_share,
        effective_documents,
    );

    let mut hapax_count = 0usize;
    let mut dis_legomena = 0usize;
    let mut band_3_to_9_count = 0usize;
    let mut band_10plus_count = 0usize;
    let mut word_like_count = 0usize;
    let mut numeric_only_types = 0usize;
    let mut single_character_types = 0usize;
    for term in terms {
        match term.frequency {
            1 => hapax_count += 1,
            2 => dis_legomena += 1,
            3..=9 => band_3_to_9_count += 1,
            _ => band_10plus_count += 1,
        }
        if term.token.chars().any(char::is_alphabetic) {
            word_like_count += 1;
        }
        if numeric_only(&term.token) {
            numeric_only_types += 1;
        }
        // The analysis token representation is Unicode-scalar based; grapheme clusters
        // are not available without changing the tokeniser's established contract.
        if term.token.chars().count() == 1 {
            single_character_types += 1;
        }
    }
    let total_types = terms.len();

    let frequent_terms = terms
        .iter()
        .filter(|term| term.frequency >= FREQUENT_TERM_THRESHOLD)
        .collect::<Vec<_>>();
    let mut frequent_dispersions = if documents.len() >= 2 {
        frequent_terms
            .iter()
            .map(|term| term.dispersion)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    debug_assert_eq!(band_10plus_count, frequent_terms.len());
    let document_count = documents.len();
    let annotation_available = !documents.is_empty()
        && documents
            .iter()
            .all(|document| document.annotation_available);
    let (df_in_1_count, df_in_2_to_n_minus_1_count, df_in_all_count) = if document_count == 0 {
        (0, 0, 0)
    } else if document_count == 1 {
        (0, 0, total_types)
    } else {
        (
            terms
                .iter()
                .filter(|term| term.document_frequency == 1)
                .count(),
            terms
                .iter()
                .filter(|term| {
                    term.document_frequency >= 2 && term.document_frequency < document_count
                })
                .count(),
            terms
                .iter()
                .filter(|term| term.document_frequency == document_count)
                .count(),
        )
    };
    let mut document_frequencies = terms
        .iter()
        .map(|term| term.document_frequency as u64)
        .collect::<Vec<_>>();
    document_frequencies.sort_unstable();
    let median_documents_per_type = percentile(&document_frequencies, 0.5);
    let median_dp = median_f64(&mut frequent_dispersions);
    let high_dp_frequent_types = if document_count >= 2 {
        frequent_terms
            .iter()
            .filter(|term| term.dispersion >= HIGH_DISPERSION_THRESHOLD)
            .count()
    } else {
        0
    };
    let extraction_severity =
        extraction_severity(total_types, type_share(numeric_only_types, total_types));
    let lexical_severity = lexical_severity(total_tokens, total_types);
    let dispersion_severity = dispersion_severity(document_count, frequent_terms.len(), median_dp);

    let mut frequencies = terms.iter().map(|term| term.frequency).collect::<Vec<_>>();
    frequencies.sort_unstable_by(|left, right| right.cmp(left));
    let top_100_coverage = coverage_at(&frequencies, 100, total_tokens);
    let top_1000_coverage = coverage_at(&frequencies, 1_000, total_tokens);
    let top_10000_coverage = coverage_at(&frequencies, 10_000, total_tokens);
    let (shannon_entropy_bits, evenness_j, effective_vocabulary_entropy) =
        entropy_metrics(&frequencies, total_tokens);
    let (frequency_spectrum, frequency_spectrum_truncated) = frequency_spectrum(&frequencies);
    let rank_frequency = rank_frequency_summary(&frequencies, total_tokens);
    let vocabulary_growth = vocabulary_growth_summary(&metadata.vocabulary_growth);

    let sources_in_project = metadata
        .overview_context
        .sources_in_project
        .or(Some(metadata.source_count));
    let empty_or_failed = sources_in_project.map_or_else(
        || {
            metadata
                .skipped_documents
                .saturating_add(metadata.overview_context.failed_sources)
        },
        |sources| sources.saturating_sub(document_count),
    );
    let population_reading = match sources_in_project {
        Some(sources) if empty_or_failed == 0 && document_count == sources => format!(
            "No empty or failed sources; the analysis population contains all {sources} project sources."
        ),
        Some(sources) => format!(
            "{empty_or_failed} sources were empty or failed; analysis contains {document_count} of {sources} project sources."
        ),
        None => format!("Analysis contains {document_count} sources."),
    };

    let percent = |value: Option<f64>| {
        value
            .map(|number| format!("{:.1}%", number * 100.0))
            .unwrap_or_else(|| "unavailable".to_string())
    };
    let balance_finding_text = if document_count == 0 {
        "No documents are available for a balance assessment.".to_string()
    } else if total_tokens == 0 {
        "All document token counts are zero, so document balance is unavailable.".to_string()
    } else if document_count == 1 {
        "The analysis contains one document, so between-document balance cannot be assessed."
            .to_string()
    } else {
        format!(
            "The largest document contains {} of tokens; the top three contain {}. Effective documents: {:.1} of {}.",
            percent(largest_share),
            percent(top3_share),
            effective_documents.unwrap_or(0.0),
            document_count
        )
    };
    let extraction_finding_text = if total_types == 0 {
        "No token types are available for extraction-quality diagnostics.".to_string()
    } else {
        format!(
            "Numeric-only forms account for {} of {} types; single-character forms account for {}.",
            percent(type_share(numeric_only_types, total_types)),
            total_types,
            percent(type_share(single_character_types, total_types))
        )
    };
    let lexical_finding_text = if total_types == 0 {
        "No token types were counted.".to_string()
    } else {
        format!(
            "{} of {} types occur once, and {} occur at least 10 times.",
            hapax_count, total_types, band_10plus_count
        )
    };
    let dispersion_finding_text = if document_count < 2 {
        "Dispersion requires at least two documents and is not assessed here.".to_string()
    } else if frequent_terms.is_empty() {
        "No types occur at least 10 times, so frequent-type DP is unavailable.".to_string()
    } else {
        format!(
            "Median DP is {:.2}; {} of {} frequent types have DP at least {:.1}.",
            median_dp.unwrap_or(0.0),
            high_dp_frequent_types,
            frequent_terms.len(),
            HIGH_DISPERSION_THRESHOLD
        )
    };
    let findings = vec![
        finding(
            CorpusOverviewFindingKind::Balance,
            balance_severity,
            balance_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::ExtractionQuality,
            extraction_severity,
            extraction_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::LexicalSpectrum,
            lexical_severity,
            lexical_finding_text,
        ),
        finding(
            CorpusOverviewFindingKind::Dispersion,
            dispersion_severity,
            dispersion_finding_text,
        ),
    ];
    let highest_severity = findings
        .iter()
        .map(|finding| finding.severity)
        .max()
        .unwrap_or(CorpusOverviewFindingSeverity::NoIssue);
    let verdict = if total_tokens == 0 {
        "The analysis contains no tokens; inspect source extraction and processing before analysis."
            .to_string()
    } else {
        match highest_severity {
            CorpusOverviewFindingSeverity::NeedsFixing =>
                "The corpus has diagnostics that need fixing before aggregate results are relied on."
                    .to_string(),
            CorpusOverviewFindingSeverity::WorthALook =>
                "The corpus is available for exploration, with diagnostics worth inspecting alongside aggregate results."
                    .to_string(),
            CorpusOverviewFindingSeverity::NoIssue =>
                "No immediate issue is indicated by these first-stage corpus diagnostics."
                    .to_string(),
        }
    };

    let balance_reading = if document_count < 2 {
        "Analysis unit: the document. At least two documents are required to assess document balance."
            .to_string()
    } else if balance_severity == CorpusOverviewFindingSeverity::NoIssue {
        "Analysis unit: the document. Token totals are descriptive; read aggregate frequencies alongside dispersion even when document lengths are relatively even."
            .to_string()
    } else {
        "Analysis unit: the document. Token totals are descriptive, but a few documents carry a disproportionate share of the token mass; read aggregate frequencies alongside dispersion, not on their own."
            .to_string()
    };

    let displayed_document_count = if document_count > 50 {
        OVERVIEW_DOCUMENT_LIMIT
    } else {
        document_count
    };
    let overview_documents = sorted_documents
        .iter()
        .take(displayed_document_count)
        .map(|document| CorpusOverviewDocument {
            name: {
                let name = document_name(document);
                if name.is_empty() {
                    "Unnamed source".to_string()
                } else {
                    name.to_string()
                }
            },
            tokens: document.token_count,
            share: (total_tokens > 0).then(|| document.token_count as f64 / total_tokens as f64),
            relative_length: sorted_documents
                .first()
                .filter(|largest| largest.token_count > 0)
                .map(|largest| document.token_count as f64 / largest.token_count as f64),
        })
        .collect::<Vec<_>>();

    CorpusOverview {
        document_count,
        source_count: sources_in_project.unwrap_or(metadata.source_count),
        total_tokens,
        total_types,
        annotation_available,
        input_text_source: metadata.input_text_source,
        built_at: metadata.built_at.clone(),
        build_signature: metadata.build_signature,
        processing_config_signature: metadata.processing_config_signature,
        skipped_documents: metadata.skipped_documents,
        skipped_pdf_documents: metadata.skipped_pdf_documents,
        totals: CorpusOverviewTotals {
            tokens: total_tokens,
            types: total_types,
            documents: document_count,
        },
        population: CorpusOverviewPopulation {
            sources_in_project,
            included_in_analysis: document_count,
            processed_used: metadata.processed_documents,
            using_source_text_fallback: metadata.source_text_fallback_documents,
            empty_or_failed,
            sources_with_warnings: metadata.overview_context.sources_with_warnings,
            input_source: analysis_input_label(
                metadata.processed_documents,
                metadata.source_text_fallback_documents,
            ),
            reading: population_reading,
        },
        documents: overview_documents,
        documents_truncated: document_count > 50,
        balance: CorpusOverviewBalance {
            largest_share,
            top3_share,
            gini,
            coefficient_of_variation,
            effective_documents,
            median_tokens,
            minimum_tokens: token_counts.first().copied(),
            maximum_tokens: token_counts.last().copied(),
            remaining_documents: document_count.saturating_sub(displayed_document_count),
            reading: balance_reading,
        },
        extraction_quality: CorpusOverviewExtractionQuality {
            word_like_count,
            word_like_share: type_share(word_like_count, total_types),
            numeric_only_count: numeric_only_types,
            numeric_only_share: type_share(numeric_only_types, total_types),
            single_character_count: Some(single_character_types),
            single_character_share: type_share(single_character_types, total_types),
            reading: "Numeric-only forms may include page numbers, table fragments, formula fragments, references, or legitimate numeric examples. Inspect before removing numeric-heavy material."
                .to_string(),
        },
        lexical_spectrum: CorpusOverviewLexicalSpectrum {
            types: total_types,
            hapax_count,
            dis_legomena_count: dis_legomena,
            band_3_to_9_count,
            band_10plus_count,
            hapax_share: type_share(hapax_count, total_types),
            dis_legomena_share: type_share(dis_legomena, total_types),
            band_3_to_9_share: type_share(band_3_to_9_count, total_types),
            band_10plus_share: type_share(band_10plus_count, total_types),
            frequency_spectrum,
            frequency_spectrum_truncated,
            mattr_window: MATTR_WINDOW_TOKENS as u32,
            mattr_median: mattr_summary.median,
            mattr_min: mattr_summary.minimum,
            mattr_max: mattr_summary.maximum,
            mattr_docs_excluded: u32::try_from(mattr_summary.documents_excluded)
                .unwrap_or(u32::MAX),
            mattr_docs_used: u32::try_from(mattr_summary.documents_used).unwrap_or(u32::MAX),
            mtld_threshold: MTLD_TTR_THRESHOLD,
            mtld_min_document_tokens: MTLD_MIN_DOCUMENT_TOKENS as u32,
            mtld_median: mtld_summary.median,
            mtld_min: mtld_summary.minimum,
            mtld_max: mtld_summary.maximum,
            mtld_docs_excluded: u32::try_from(mtld_summary.documents_excluded)
                .unwrap_or(u32::MAX),
            mtld_docs_used: u32::try_from(mtld_summary.documents_used).unwrap_or(u32::MAX),
            reading: "A long tail dominated by rare words is expected in natural-language corpora; it is not an extraction-quality problem."
                .to_string(),
        },
        frequency_concentration: CorpusOverviewFrequencyConcentration {
            top_100_coverage,
            top_1000_coverage,
            top_10000_coverage,
            shannon_entropy_bits,
            evenness_j,
            perplexity: effective_vocabulary_entropy,
            effective_vocabulary_entropy,
            reading: "A small set of high-frequency forms can carry much of the token mass even when most types are rare."
                .to_string(),
        },
        rank_frequency,
        vocabulary_growth,
        dispersion: CorpusOverviewDispersion {
            frequent_types: frequent_terms.len(),
            high_dp_frequent_types,
            median_dp,
            df_in_1_count,
            df_in_2_to_n_minus_1_count,
            df_in_all_count,
            df_in_1_share: type_share(df_in_1_count, total_types),
            df_in_2_to_n_minus_1_share: type_share(
                df_in_2_to_n_minus_1_count,
                total_types,
            ),
            df_in_all_share: type_share(df_in_all_count, total_types),
            median_documents_per_type,
            reading: if document_count < 2 {
                "Dispersion and document-frequency spread require at least two documents."
                    .to_string()
            } else {
                "Most types may occur in few documents, which is common in sparse vocabularies. Interpret this with token-mass coverage and frequent-type dispersion."
                    .to_string()
            },
        },
        reading: CorpusOverviewReading { verdict, findings },
        build: CorpusOverviewBuild {
            tokenisation: Some(
                "UAX #29 word boundaries; ASCII Letter/Number hyphen chains and U+0027/U+2019 contractions retained; dictionary-free CJK/Thai segmentation remains limited"
                    .to_string(),
            ),
            case_handling: Some(
                "Surface case preserved; NFC Unicode lowercase aggregation keys".to_string(),
            ),
            lemmatisation: Some(
                "Surface-only build; lemma/POS annotation is not built yet.".to_string(),
            ),
            number_punctuation_handling: Some(
                "Numbers retained; decimal points and thousands separators split; standalone punctuation excluded from corpus metrics"
                    .to_string(),
            ),
            input_source: analysis_input_label(
                metadata.processed_documents,
                metadata.source_text_fallback_documents,
            ),
            config_version: metadata.overview_context.config_version,
            snapshot_date: Some(metadata.built_at),
            metadata_attached: metadata.overview_context.metadata_attached,
        },
    }
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

pub fn build_annotation_layer<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    load_text: TextLoader,
    docs_done: &std::sync::atomic::AtomicUsize,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    build_annotation_layer_with_mapping(corpus, records, options, cancel, load_text, true, docs_done)
}

pub fn build_and_write_annotation_cache<TextLoader, Progress>(
    records: &[DocumentRecord],
    mut analysis_options: AnalysisBuildOptions,
    annotation_options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    mut report_progress: Progress,
) -> Result<AnnotationCacheBuildReport, AnnotationCacheBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String> + Send,
    Progress: FnMut(AnnotationCacheBuildProgress),
{
    let cache_root = analysis_options
        .annotation_cache_root
        .take()
        .ok_or(AnnotationCacheBuildError::MissingCacheRoot)?;
    analysis_options.force_compute_hashes = true;

    let cache_root_str = cache_root.to_string_lossy().into_owned();

    let files_total = records.len();
    report_progress(AnnotationCacheBuildProgress {
        files_done: 0,
        files_total,
        current_phase: AnnotationCacheBuildPhase::BuildingSurface { phase: "Init".to_string() },
    });

    let corpus = build_analysis_corpus(records, analysis_options, cancel, &mut load_text, |p| {
        report_progress(AnnotationCacheBuildProgress {
            files_done: p.files_done,
            files_total: p.files_total,
            current_phase: AnnotationCacheBuildPhase::BuildingSurface { phase: format!("{:?}", p.current_phase) },
        });
    })
        .map_err(|e| {
        if matches!(e, AnalysisBuildError::Cancelled) {
            AnnotationCacheBuildError::Cancelled
        } else {
            AnnotationCacheBuildError::SurfaceBuildFailed {
                error: e.to_string(),
            }
        }
    })?;

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(AnnotationCacheBuildError::Cancelled);
    }

    let manifest = build_expected_manifest(
        &corpus,
        records,
        annotation_options.udpipe_model_dir.as_deref(),
        &annotation_options.language_overrides,
    )
    .map_err(|e| AnnotationCacheBuildError::ManifestError {
        error: format!("{:?}", e),
    })?;

    let cache_load = crate::annotation_cache::load_annotation_layer_cache(&cache_root, &manifest);
    if let crate::annotation_cache::AnnotationLayerCacheLoad::Ready(layer) = cache_load {
        let documents_annotated = layer.documents.len();
        let total_annotated_term_count_rows = layer
            .documents
            .iter()
            .map(|doc| doc.term_counts.len())
            .sum();

        return Ok(AnnotationCacheBuildReport {
            documents_annotated,
            total_annotated_term_count_rows,
            languages_used: layer.languages_used,
            cache_root: cache_root_str,
            reused_existing: true,
            cache_written: false,
        });
    }

    report_progress(AnnotationCacheBuildProgress {
        files_done: 0,
        files_total,
        current_phase: AnnotationCacheBuildPhase::LoadingModel,
    });

    let docs_done = std::sync::atomic::AtomicUsize::new(0);
    
    let layer = std::thread::scope(|s| {
        let handle = s.spawn(|| {
            build_annotation_layer(&corpus, records, annotation_options, cancel, &mut load_text, &docs_done)
        });

        while !handle.is_finished() {
            let done = docs_done.load(std::sync::atomic::Ordering::Relaxed);
            report_progress(AnnotationCacheBuildProgress {
                files_done: done,
                files_total,
                current_phase: AnnotationCacheBuildPhase::Annotating,
            });
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        handle.join().unwrap()
    }).map_err(|e| match e {
        AnnotationLayerBuildError::Cancelled => AnnotationCacheBuildError::Cancelled,
        AnnotationLayerBuildError::UnsupportedLanguages { languages } => {
            AnnotationCacheBuildError::UnsupportedLanguages { languages }
        }
        other => AnnotationCacheBuildError::AnnotationFailed {
            error: other.to_string(),
        },
    })?;

    report_progress(AnnotationCacheBuildProgress {
        files_done: files_total,
        files_total,
        current_phase: AnnotationCacheBuildPhase::WritingCache,
    });

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(AnnotationCacheBuildError::Cancelled);
    }

    crate::annotation_cache::write_annotation_layer_cache(&cache_root, &manifest, &layer).map_err(
        |e| AnnotationCacheBuildError::CacheWriteError {
            error: format!("{:?}", e),
        },
    )?;

    let documents_annotated = layer.documents.len();
    let total_annotated_term_count_rows = layer
        .documents
        .iter()
        .map(|doc| doc.term_counts.len())
        .sum();

    Ok(AnnotationCacheBuildReport {
        documents_annotated,
        total_annotated_term_count_rows,
        languages_used: layer.languages_used,
        cache_root: cache_root_str,
        reused_existing: false,
        cache_written: true,
    })
}

pub(crate) fn compare_document_records(
    left: &DocumentRecord,
    right: &DocumentRecord,
) -> std::cmp::Ordering {
    left.relative_path
        .cmp(&right.relative_path)
        .then_with(|| left.source_path.cmp(&right.source_path))
}

fn build_expected_manifest(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    model_dir: Option<&Path>,
    language_overrides: &HashMap<usize, String>,
) -> Result<
    crate::annotation_cache::AnnotationManifest,
    crate::annotation_cache::AnnotationLayerCacheError,
> {
    let tokenization = crate::tokenization::TokenizationManifest {
        id: crate::tokenization::TOKENIZER_ID.to_string(),
        version: crate::tokenization::TOKENIZER_VERSION.to_string(),
        unicode_segmentation_version: crate::tokenization::UNICODE_SEGMENTATION_VERSION.to_string(),
        config: corpus.tokenization,
    };

    let document_languages: Vec<String> = corpus
        .documents
        .iter()
        .map(|doc| doc.summary.language.clone().unwrap_or_default())
        .collect();

    let languages_set: BTreeSet<String> = document_languages.iter().cloned().collect();
    let default_dir;
    let resolved_model_dir = match model_dir {
        Some(dir) => dir,
        None => {
            default_dir = default_model_dir();
            &default_dir
        }
    };

    let mut models = Vec::new();
    for lang in &languages_set {
        let (language, model_filename, udpipe_model_name, sha256) =
            crate::annotation::model_identity(lang, resolved_model_dir).map_err(|e| {
                crate::annotation_cache::AnnotationLayerCacheError::ValidationError(e.to_string())
            })?;
        models.push(crate::annotation_cache::AnnotationModelIdentity {
            language,
            model_filename,
            udpipe_model_name,
            sha256,
        });
    }

    let document_hashes: Vec<String> = corpus
        .documents
        .iter()
        .map(|doc| doc.processed_hash_sha256.clone())
        .collect();

    crate::annotation_cache::build_annotation_manifest(
        corpus,
        records,
        tokenization,
        language_overrides,
        models,
        &document_hashes,
        &document_languages,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationAttachError {
    DocumentCountMismatch {
        expected: usize,
        got: usize,
    },
    DocumentIndexMismatch {
        index: usize,
        got: usize,
    },
    DocumentPathMismatch {
        index: usize,
        expected_source: String,
        got_source: String,
        expected_relative: String,
        got_relative: String,
    },
    SurfaceTermIdOutOfRange {
        index: usize,
        term_id: u32,
        limit: usize,
    },
    LemmaIdOutOfRange {
        index: usize,
        lemma_id: u32,
        limit: usize,
    },
    LanguagesUsedNotSortedOrDeduplicated,
}

fn attach_annotation_layer(
    corpus: &mut AnalysisCorpus,
    layer: AnnotationLayer,
) -> Result<(), AnnotationAttachError> {
    if layer.documents.len() != corpus.documents.len() {
        return Err(AnnotationAttachError::DocumentCountMismatch {
            expected: corpus.documents.len(),
            got: layer.documents.len(),
        });
    }

    let mut langs = layer.languages_used.clone();
    let original_langs = langs.clone();
    langs.sort_unstable();
    langs.dedup();
    if langs != original_langs {
        return Err(AnnotationAttachError::LanguagesUsedNotSortedOrDeduplicated);
    }

    let max_surface_term_len = corpus.surface_terms.len();
    let max_lemma_len = layer.lemma_table.len();

    for (idx, layer_doc) in layer.documents.iter().enumerate() {
        if layer_doc.analysis_document_index != idx {
            return Err(AnnotationAttachError::DocumentIndexMismatch {
                index: idx,
                got: layer_doc.analysis_document_index,
            });
        }

        let corpus_doc = &corpus.documents[idx];
        if corpus_doc.summary.source_path != layer_doc.source_path
            || corpus_doc.summary.relative_path != layer_doc.relative_path
        {
            return Err(AnnotationAttachError::DocumentPathMismatch {
                index: idx,
                expected_source: corpus_doc.summary.source_path.clone(),
                got_source: layer_doc.source_path.clone(),
                expected_relative: corpus_doc.summary.relative_path.clone(),
                got_relative: layer_doc.relative_path.clone(),
            });
        }

        for count in &layer_doc.term_counts {
            if count.surface_term_id as usize >= max_surface_term_len {
                return Err(AnnotationAttachError::SurfaceTermIdOutOfRange {
                    index: idx,
                    term_id: count.surface_term_id,
                    limit: max_surface_term_len,
                });
            }
            if count.lemma_id as usize >= max_lemma_len {
                return Err(AnnotationAttachError::LemmaIdOutOfRange {
                    index: idx,
                    lemma_id: count.lemma_id,
                    limit: max_lemma_len,
                });
            }
        }
    }

    // Now mutate corpus (all-or-nothing check passed!)
    corpus.lemma_table = layer.lemma_table;
    for (idx, layer_doc) in layer.documents.into_iter().enumerate() {
        let corpus_doc = &mut corpus.documents[idx];
        corpus_doc.annotated_term_counts = layer_doc.term_counts;
        corpus_doc.annotation_available = true;
    }
    corpus.overview.annotation_available = true;

    Ok(())
}

fn build_annotation_layer_with_mapping<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    parallel_map: bool,
    docs_done: &std::sync::atomic::AtomicUsize,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let model_dir = options.udpipe_model_dir.unwrap_or_else(default_model_dir);
    let inputs = load_annotation_document_inputs(
        corpus,
        records,
        &options.language_overrides,
        cancel,
        &mut load_text,
    )?;
    let surface_term_lookup = build_surface_term_lookup(corpus)?;
    let languages_used = inputs
        .iter()
        .map(|input| input.language.clone())
        .collect::<BTreeSet<_>>();

    preflight_annotation_languages(&languages_used, &model_dir)?;

    let worker_documents = annotate_annotation_documents(
        &inputs,
        &surface_term_lookup,
        &model_dir,
        cancel,
        parallel_map,
        &corpus.tokenization,
        docs_done,
    )?;
    reduce_annotation_documents(worker_documents, languages_used.into_iter().collect())
}

fn load_annotation_document_inputs<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    language_overrides: &HashMap<usize, String>,
    cancel: &AtomicBool,
    load_text: &mut TextLoader,
) -> Result<Vec<AnnotationDocumentInput>, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records.sort_by(|left, right| compare_document_records(left.1, right.1));

    let mut inputs = Vec::with_capacity(corpus.documents.len());
    for (original_record_index, record) in ordered_records {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnnotationLayerBuildError::Cancelled);
        }

        let source_path = record.source_path.to_string_lossy().into_owned();
        let relative_path = record.relative_path.to_string_lossy().into_owned();
        let resolution =
            load_text(record).map_err(|error| AnnotationLayerBuildError::TextLoadFailed {
                source_path: source_path.clone(),
                error,
            })?;
        let text = match resolution.provenance {
            AnalysisInputProvenance::Processed | AnalysisInputProvenance::SourceTextFallback => {
                resolution.text
            }
            AnalysisInputProvenance::EmptyOrFailed => None,
        };
        let Some(text) = text.filter(|value| !value.trim().is_empty()) else {
            continue;
        };

        let analysis_document_index = inputs.len();
        let Some(document) = corpus.documents.get(analysis_document_index) else {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "canonical text supplied more usable documents than the analysis corpus; first extra source is {relative_path}"
                ),
            });
        };
        verify_annotation_document_identity(
            analysis_document_index,
            document,
            &source_path,
            &relative_path,
            &text,
            &corpus.tokenization,
        )?;

        let language =
            if let Some(override_language) = language_overrides.get(&original_record_index) {
                detect_and_resolve(&text, Some(override_language))
                    .resolution
                    .effective_language
            } else if let Some(language) = document.summary.language.clone() {
                language
            } else {
                detect_and_resolve(&text, None)
                    .resolution
                    .effective_language
            };

        inputs.push(AnnotationDocumentInput {
            analysis_document_index,
            source_path,
            relative_path,
            text,
            language,
        });
    }

    if inputs.len() != corpus.documents.len() {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "canonical text supplied {} usable documents, but the analysis corpus contains {} documents",
                inputs.len(),
                corpus.documents.len()
            ),
        });
    }

    Ok(inputs)
}

fn verify_annotation_document_identity(
    analysis_document_index: usize,
    document: &AnalysisDocument,
    source_path: &str,
    relative_path: &str,
    text: &str,
    tokenization_config: &TokenizationConfig,
) -> Result<(), AnnotationLayerBuildError> {
    if document.summary.source_path != source_path
        || document.summary.relative_path != relative_path
    {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "document {analysis_document_index} is {}, but canonical text supplied {}",
                document_name(&document.summary),
                if relative_path.is_empty() {
                    source_path
                } else {
                    relative_path
                }
            ),
        });
    }

    let document_id = u32::try_from(analysis_document_index).map_err(|_| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: "analysis corpus contains more than u32::MAX documents".to_string(),
        }
    })?;
    let tokens = analysis_word_tokens(text, document_id, tokenization_config);
    if tokens.len() as u64 != document.summary.token_count {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "document {analysis_document_index} token count changed from {} to {}",
                document.summary.token_count,
                tokens.len()
            ),
        });
    }

    Ok(())
}

fn build_surface_term_lookup(
    corpus: &AnalysisCorpus,
) -> Result<HashMap<String, u32>, AnnotationLayerBuildError> {
    let mut lookup = HashMap::with_capacity(corpus.surface_terms.len());
    for (index, term) in corpus.surface_terms.iter().enumerate() {
        let term_id = u32::try_from(index).map_err(|_| {
            AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: "analysis surface vocabulary contains more than u32::MAX types".to_string(),
            }
        })?;
        if lookup.insert(term.token.clone(), term_id).is_some() {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "analysis surface vocabulary contains duplicate term {:?}",
                    term.token
                ),
            });
        }
    }
    Ok(lookup)
}

fn preflight_annotation_languages(
    languages: &BTreeSet<String>,
    model_dir: &Path,
) -> Result<(), AnnotationLayerBuildError> {
    let mut manager = ModelManager::new(model_dir.to_path_buf());
    let mut unsupported = Vec::new();
    for language in languages {
        match manager.load_model(language) {
            Ok(()) => {}
            Err(AnnotationError::ModelNotFound { language }) => unsupported.push(language),
            Err(error) => {
                return Err(AnnotationLayerBuildError::ModelPreflightFailed {
                    language: language.clone(),
                    error: error.to_string(),
                });
            }
        }
    }

    if !unsupported.is_empty() {
        return Err(AnnotationLayerBuildError::UnsupportedLanguages {
            languages: unsupported,
        });
    }

    Ok(())
}

fn annotate_annotation_documents(
    inputs: &[AnnotationDocumentInput],
    surface_term_lookup: &HashMap<String, u32>,
    model_dir: &Path,
    cancel: &AtomicBool,
    parallel_map: bool,
    tokenization_config: &TokenizationConfig,
    docs_done: &std::sync::atomic::AtomicUsize,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    if parallel_map {
        let model_dir = model_dir.to_path_buf();
        inputs
            .par_iter()
            .map_init(
                || ModelManager::new(model_dir.clone()),
                |manager, input| {
                    let result = annotate_annotation_document(
                        input,
                        surface_term_lookup,
                        manager,
                        cancel,
                        tokenization_config,
                    );
                    if result.is_ok() {
                        docs_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    result
                },
            )
            .collect()
    } else {
        let mut manager = ModelManager::new(model_dir.to_path_buf());
        inputs
            .iter()
            .map(|input| {
                let result = annotate_annotation_document(
                    input,
                    surface_term_lookup,
                    &mut manager,
                    cancel,
                    tokenization_config,
                );
                if result.is_ok() {
                    docs_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                result
            })
            .collect()
    }
}

fn annotate_annotation_document(
    input: &AnnotationDocumentInput,
    surface_term_lookup: &HashMap<String, u32>,
    manager: &mut ModelManager,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
) -> Result<WorkerAnnotationDocument, AnnotationLayerBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        return Err(AnnotationLayerBuildError::Cancelled);
    }

    manager
        .load_model(&input.language)
        .map_err(|error| annotation_document_error(input, error))?;

    let document_id = u32::try_from(input.analysis_document_index).map_err(|_| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: "analysis corpus contains more than u32::MAX documents".to_string(),
        }
    })?;
    let tokens = analysis_word_tokens(&input.text, document_id, tokenization_config);
    let token_surfaces = tokens.iter().map(|token| token.surface).collect::<Vec<_>>();
    let annotated_tokens = manager
        .tag_and_align(&input.language, &token_surfaces)
        .map_err(|error| annotation_document_error(input, error))?;
    let term_counts = resolve_annotated_tokens_to_local_counts(
        input.analysis_document_index,
        &annotated_tokens,
        surface_term_lookup,
    )?;

    Ok(WorkerAnnotationDocument {
        analysis_document_index: input.analysis_document_index,
        source_path: input.source_path.clone(),
        relative_path: input.relative_path.clone(),
        term_counts,
    })
}

fn annotation_document_error(
    input: &AnnotationDocumentInput,
    error: AnnotationError,
) -> AnnotationLayerBuildError {
    match error {
        AnnotationError::AlignmentFailed { reason } => {
            AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index: input.analysis_document_index,
                reason,
            }
        }
        AnnotationError::ModelNotFound { language } => {
            AnnotationLayerBuildError::UnsupportedLanguages {
                languages: vec![language],
            }
        }
        error => AnnotationLayerBuildError::ModelTaggingFailed {
            analysis_document_index: input.analysis_document_index,
            language: input.language.clone(),
            error: error.to_string(),
        },
    }
}

fn resolve_annotated_tokens_to_local_counts(
    analysis_document_index: usize,
    annotated_tokens: &[AnnotatedToken],
    surface_term_lookup: &HashMap<String, u32>,
) -> Result<Vec<LocalAnnotatedTermCount>, AnnotationLayerBuildError> {
    let mut counts = HashMap::<(u32, String, UniversalPos), u64>::new();
    for annotated_token in annotated_tokens {
        let Some(&surface_term_id) = surface_term_lookup.get(&annotated_token.surface) else {
            return Err(AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: format!(
                    "annotated surface {:?} at token {} does not exist in the analysis surface vocabulary",
                    annotated_token.surface, annotated_token.token_index
                ),
            });
        };
        let key = (
            surface_term_id,
            annotated_token.lemma.clone(),
            annotated_token.upos,
        );
        let count = counts.entry(key).or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: "annotated token count overflowed u64".to_string(),
            }
        })?;
    }

    let mut terms = counts
        .into_iter()
        .map(
            |((surface_term_id, lemma, upos), count)| LocalAnnotatedTermCount {
                surface_term_id,
                lemma,
                upos,
                count,
            },
        )
        .collect::<Vec<_>>();
    terms.sort_by(|left, right| {
        left.surface_term_id
            .cmp(&right.surface_term_id)
            .then_with(|| left.lemma.cmp(&right.lemma))
            .then_with(|| left.upos.as_upos_str().cmp(right.upos.as_upos_str()))
    });
    Ok(terms)
}

fn reduce_annotation_documents(
    mut worker_documents: Vec<WorkerAnnotationDocument>,
    languages_used: Vec<String>,
) -> Result<AnnotationLayer, AnnotationLayerBuildError> {
    worker_documents.sort_by_key(|document| document.analysis_document_index);
    let mut lemma_table = LemmaTable::new();
    let mut documents = Vec::with_capacity(worker_documents.len());

    for (expected_index, document) in worker_documents.into_iter().enumerate() {
        if document.analysis_document_index != expected_index {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "annotation worker returned document index {}, expected {expected_index}",
                    document.analysis_document_index
                ),
            });
        }
        let term_counts = document
            .term_counts
            .into_iter()
            .map(|local_count| AnnotatedTermCount {
                surface_term_id: local_count.surface_term_id,
                lemma_id: lemma_table.intern(&local_count.lemma),
                upos: local_count.upos,
                count: local_count.count,
            })
            .collect::<Vec<_>>();
        documents.push(AnnotationLayerDocument {
            analysis_document_index: document.analysis_document_index,
            source_path: document.source_path,
            relative_path: document.relative_path,
            term_counts,
        });
    }

    Ok(AnnotationLayer {
        documents,
        lemma_table,
        languages_used,
    })
}

fn build_analysis_corpus_with_mapping<TextLoader, Progress>(
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

fn selected_document_mask(
    corpus: &AnalysisCorpus,
    selection: &AnalysisDocumentSelection,
) -> (Vec<bool>, usize) {
    let mut mask = vec![false; corpus.documents.len()];
    if selection.all_documents {
        mask.fill(true);
        for &id in &selection.excluded_ids {
            if let Some(selected) = mask.get_mut(id) {
                *selected = false;
            }
        }
    } else {
        for &id in &selection.ids {
            if let Some(selected) = mask.get_mut(id) {
                *selected = true;
            }
        }
    }
    let selected_count = mask.iter().filter(|selected| **selected).count();
    (mask, selected_count)
}

/// Returns one bounded, searchable document window for the file-scope picker.
pub fn query_analysis_documents(
    corpus: &AnalysisCorpus,
    selection: &AnalysisDocumentSelection,
    query: &str,
    offset: usize,
    limit: usize,
) -> AnalysisDocumentPage {
    let (selected, selected_count) = selected_document_mask(corpus, selection);
    let query = query.trim().to_lowercase();
    let limit = limit.min(MAX_QUERY_WINDOW);
    let mut total_count = 0usize;
    let mut items = Vec::with_capacity(limit);
    for document in &corpus.documents {
        let summary = &document.summary;
        if !query.is_empty()
            && !summary.relative_path.to_lowercase().contains(&query)
            && !summary.source_path.to_lowercase().contains(&query)
        {
            continue;
        }
        if total_count >= offset && items.len() < limit {
            items.push(AnalysisDocumentPageItem {
                id: summary.id,
                source_path: summary.source_path.clone(),
                relative_path: summary.relative_path.clone(),
                token_count: summary.token_count,
                selected: selected[summary.id],
            });
        }
        total_count += 1;
    }
    AnalysisDocumentPage {
        total_count,
        offset: offset.min(total_count),
        limit,
        items,
        selected_count,
        total_document_count: corpus.documents.len(),
    }
}

fn concordance_window_tokens(window_tokens: usize) -> usize {
    if window_tokens == 0 {
        CONCORDANCE_DEFAULT_WINDOW_TOKENS
    } else {
        window_tokens.clamp(1, CONCORDANCE_MAX_WINDOW_TOKENS)
    }
}

fn concordance_limit(limit: usize) -> usize {
    if limit == 0 {
        CONCORDANCE_DEFAULT_LIMIT
    } else {
        limit.clamp(1, CONCORDANCE_MAX_LIMIT)
    }
}

fn concordance_key(value: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        value.to_string()
    } else {
        case_fold_surface(value)
    }
}

fn resolve_concordance_query_token<'text>(
    query: &'text str,
    tokenization_config: &TokenizationConfig,
) -> Result<&'text str, ConcordanceError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(ConcordanceError::EmptyQuery);
    }
    let query_tokens = tokenize(trimmed, 0, tokenization_config);
    if query_tokens.is_empty() {
        return Err(ConcordanceError::EmptyQuery);
    }
    if query_tokens.len() != 1 {
        return Err(ConcordanceError::MultiTokenQuery);
    }
    let token = query_tokens[0];
    if !token.counts_as_word() {
        return Err(ConcordanceError::EmptyQuery);
    }
    Ok(token.surface)
}

/// Searches processed text for a single surface word and returns KWIC rows.
pub fn search_concordance_documents(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceResult, ConcordanceError> {
    let window_tokens = concordance_window_tokens(query.window_tokens);
    let limit = concordance_limit(query.limit);
    let query_surface = resolve_concordance_query_token(&query.query, tokenization_config)?;
    let query_key = concordance_key(query_surface, query.case_sensitive);
    let mut total_hits = 0usize;
    let mut documents_with_hits = HashSet::<usize>::new();
    let mut rows = Vec::with_capacity(limit);

    for document in documents {
        let tokens = analysis_word_tokens(
            &document.text,
            document.document_index as u32,
            tokenization_config,
        );
        let mut document_has_hit = false;
        for (token_index, token) in tokens.iter().enumerate() {
            if concordance_key(token.surface, query.case_sensitive) != query_key {
                continue;
            }

            if total_hits >= query.offset && rows.len() < limit {
                let left_start = token_index.saturating_sub(window_tokens);
                let right_end = (token_index + 1 + window_tokens).min(tokens.len());
                rows.push(ConcordanceRow {
                    document_index: document.document_index,
                    source_path: document.source_path.clone(),
                    display_name: document.display_name.clone(),
                    token_index,
                    left_context: tokens[left_start..token_index]
                        .iter()
                        .map(|token| token.surface.to_string())
                        .collect(),
                    node: token.surface.to_string(),
                    right_context: tokens[token_index + 1..right_end]
                        .iter()
                        .map(|token| token.surface.to_string())
                        .collect(),
                });
            }

            total_hits += 1;
            document_has_hit = true;
        }
        if document_has_hit {
            documents_with_hits.insert(document.document_index);
        }
    }

    Ok(ConcordanceResult {
        query: query_surface.to_string(),
        case_sensitive: query.case_sensitive,
        window_tokens,
        total_hits,
        documents_with_hits: documents_with_hits.len(),
        offset: query.offset.min(total_hits),
        limit,
        rows,
    })
}

fn token_matches_class(token: &str, filter: FrequencyTokenClass) -> bool {
    let has_letter = token.chars().any(char::is_alphabetic);
    let has_number = token.chars().any(char::is_numeric);
    let is_single_character = token.chars().count() <= 1;
    match filter {
        FrequencyTokenClass::Alphabetic => has_letter && !has_number && !is_single_character,
        FrequencyTokenClass::IncludeSingles => has_letter && !has_number,
        FrequencyTokenClass::IncludeNumerals => !is_single_character,
        FrequencyTokenClass::All => true,
    }
}

fn case_fold_surface(value: &str) -> String {
    value.nfc().collect::<String>().to_lowercase()
}

fn stemming_algorithm(language: Option<&str>) -> Option<Algorithm> {
    match language.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "ar" | "ara" | "arabic" => Some(Algorithm::Arabic),
            "da" | "dan" | "danish" => Some(Algorithm::Danish),
            "nl" | "nld" | "dut" | "dutch" => Some(Algorithm::Dutch),
            "en" | "eng" | "english" => Some(Algorithm::English),
            "fi" | "fin" | "finnish" => Some(Algorithm::Finnish),
            "fr" | "fra" | "fre" | "french" => Some(Algorithm::French),
            "de" | "deu" | "ger" | "german" => Some(Algorithm::German),
            "el" | "ell" | "greek" => Some(Algorithm::Greek),
            "hu" | "hun" | "hungarian" => Some(Algorithm::Hungarian),
            "it" | "ita" | "italian" => Some(Algorithm::Italian),
            "no" | "nor" | "norwegian" => Some(Algorithm::Norwegian),
            "pt" | "por" | "portuguese" => Some(Algorithm::Portuguese),
            "ro" | "ron" | "rum" | "romanian" => Some(Algorithm::Romanian),
            "ru" | "rus" | "russian" => Some(Algorithm::Russian),
            "es" | "spa" | "spanish" => Some(Algorithm::Spanish),
            "sv" | "swe" | "swedish" => Some(Algorithm::Swedish),
            "ta" | "tam" | "tamil" => Some(Algorithm::Tamil),
            "tr" | "tur" | "turkish" => Some(Algorithm::Turkish),
            _ => None,
        },
        None => None,
    }
}

struct ResolvedFrequencyGrouping {
    group_by: FrequencyGroupMode,
    case_fold: bool,
    stemmer: Option<Stemmer>,
    notices: Vec<FrequencyNotice>,
}

fn resolve_frequency_grouping(
    query: &FrequencyQuery,
    annotation_available: bool,
) -> ResolvedFrequencyGrouping {
    match query.group_by {
        FrequencyGroupMode::Surface => ResolvedFrequencyGrouping {
            group_by: FrequencyGroupMode::Surface,
            case_fold: query.case_fold,
            stemmer: None,
            notices: Vec::new(),
        },
        FrequencyGroupMode::Lemma if annotation_available => ResolvedFrequencyGrouping {
            group_by: FrequencyGroupMode::Lemma,
            case_fold: query.case_fold,
            stemmer: None,
            notices: Vec::new(),
        },
        FrequencyGroupMode::Lemma => {
            // Lemma grouping requires annotation data. When annotation is
            // unavailable, we surface a notice and fall back to surface.
            let notices = vec![FrequencyNotice {
                kind: FrequencyNoticeKind::AnnotationUnavailable,
                message: "Lemma grouping requires UDPipe annotation for every selected document; surface grouping was used."
                    .to_string(),
            }];
            ResolvedFrequencyGrouping {
                group_by: FrequencyGroupMode::Surface,
                case_fold: query.case_fold,
                stemmer: None,
                notices,
            }
        }
        FrequencyGroupMode::Stem => {
            if let Some(algorithm) = stemming_algorithm(query.language.as_deref()) {
                return ResolvedFrequencyGrouping {
                    group_by: FrequencyGroupMode::Stem,
                    case_fold: true,
                    stemmer: Some(Stemmer::create(algorithm)),
                    notices: Vec::new(),
                };
            }
            let language = query
                .language
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unspecified");
            ResolvedFrequencyGrouping {
                group_by: FrequencyGroupMode::Surface,
                case_fold: query.case_fold,
                stemmer: None,
                notices: vec![FrequencyNotice {
                    kind: FrequencyNoticeKind::StemmingUnsupportedLanguage,
                    message: format!(
                        "Stemming is not available for language \"{language}\"; surface grouping was used."
                    ),
                }],
            }
        }
    }
}

fn selected_documents_have_annotation(corpus: &AnalysisCorpus, selected: &[bool]) -> bool {
    corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .all(|document| document.annotation_available)
}

pub fn annotation_cache_availability_for_corpus(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    cache_root: Option<&Path>,
    model_dir: Option<&Path>,
    language_overrides: &HashMap<usize, String>,
) -> AnnotationCacheAvailability {
    let Some(cache_root) = cache_root else {
        return AnnotationCacheAvailability {
            state: AnnotationCacheState::Missing,
            detail: None,
        };
    };
    let expected_manifest =
        match build_expected_manifest(corpus, records, model_dir, language_overrides) {
            Ok(manifest) => manifest,
            Err(error) => {
                return AnnotationCacheAvailability {
                    state: AnnotationCacheState::Invalid,
                    detail: Some(format!("{error:?}")),
                };
            }
        };

    match crate::annotation_cache::load_annotation_layer_cache(cache_root, &expected_manifest) {
        crate::annotation_cache::AnnotationLayerCacheLoad::Ready(_) => {
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Ready,
                detail: None,
            }
        }
        crate::annotation_cache::AnnotationLayerCacheLoad::Missing => AnnotationCacheAvailability {
            state: AnnotationCacheState::Missing,
            detail: None,
        },
        crate::annotation_cache::AnnotationLayerCacheLoad::Stale => AnnotationCacheAvailability {
            state: AnnotationCacheState::Stale,
            detail: None,
        },
        crate::annotation_cache::AnnotationLayerCacheLoad::Invalid(detail) => {
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Invalid,
                detail: Some(detail),
            }
        }
    }
}

fn frequency_group_key(surface: &str, grouping: &ResolvedFrequencyGrouping) -> String {
    match grouping.group_by {
        FrequencyGroupMode::Surface | FrequencyGroupMode::Lemma => {
            if grouping.case_fold {
                case_fold_surface(surface)
            } else {
                surface.to_string()
            }
        }
        FrequencyGroupMode::Stem => {
            let folded = case_fold_surface(surface);
            grouping
                .stemmer
                .as_ref()
                .map(|stemmer| stemmer.stem(&folded).into_owned())
                .unwrap_or(folded)
        }
    }
}

fn annotated_frequency_group_key(
    surface: &str,
    lemma: &str,
    grouping: &ResolvedFrequencyGrouping,
) -> String {
    match grouping.group_by {
        FrequencyGroupMode::Lemma => {
            if grouping.case_fold {
                case_fold_surface(lemma)
            } else {
                lemma.to_string()
            }
        }
        _ => frequency_group_key(surface, grouping),
    }
}

fn frequency_query_key(query: &str, grouping: &ResolvedFrequencyGrouping) -> String {
    let query = query.trim();
    if query.is_empty() {
        return String::new();
    }
    frequency_group_key(query, grouping)
}

#[derive(Default)]
struct FrequencyAccumulator {
    frequency: u64,
    document_counts: HashMap<usize, u64>,
}

#[derive(Default)]
struct AnnotatedFrequencyAccumulator {
    raw: u64,
    document_counts: HashMap<usize, u64>,
}

impl AnnotatedFrequencyAccumulator {
    fn add(&mut self, document_id: usize, count: u64) {
        self.raw += count;
        *self.document_counts.entry(document_id).or_default() += count;
    }

    fn sorted_document_counts(self) -> Vec<(usize, u64)> {
        let mut counts = self.document_counts.into_iter().collect::<Vec<_>>();
        counts.sort_unstable_by_key(|(document_id, _)| *document_id);
        counts
    }
}

fn compare_frequency_terms(
    left: &FrequencyTermItem,
    right: &FrequencyTermItem,
    sort_key: FrequencySortKey,
) -> Ordering {
    match sort_key {
        FrequencySortKey::Token => left.token.cmp(&right.token),
        FrequencySortKey::Frequency => left.frequency.cmp(&right.frequency),
        FrequencySortKey::Range => left.document_frequency.cmp(&right.document_frequency),
        FrequencySortKey::Rate => left.rate_per_10k.total_cmp(&right.rate_per_10k),
        FrequencySortKey::Dispersion => left.dispersion.total_cmp(&right.dispersion),
    }
}

fn empty_annotated_frequency_result(
    query: &AnnotatedFrequencyQuery,
    cache: AnnotationCacheAvailability,
    unavailable_message: Option<String>,
    selected_document_count: usize,
    total_document_count: usize,
    total_tokens: u64,
) -> AnnotatedFrequencyResult {
    AnnotatedFrequencyResult {
        mode: query.mode,
        cache,
        unavailable_message,
        total_tokens,
        selected_document_count,
        total_document_count,
        total_rows: 0,
        offset: 0,
        limit: query.limit.min(MAX_QUERY_WINDOW),
        pos_rows: Vec::new(),
        lex_rows: Vec::new(),
    }
}

fn sort_pos_frequency_rows(rows: &mut [PosFrequencyRow]) {
    rows.sort_by(|left, right| {
        right
            .raw
            .cmp(&left.raw)
            .then_with(|| left.upos.cmp(&right.upos))
    });
}

fn sort_lex_frequency_rows(rows: &mut [LexFrequencyRow]) {
    rows.sort_by(|left, right| {
        right
            .raw
            .cmp(&left.raw)
            .then_with(|| left.lemma.cmp(&right.lemma))
            .then_with(|| left.upos.cmp(&right.upos))
    });
}

fn pos_frequency_rows(
    corpus: &AnalysisCorpus,
    selected: &[bool],
    selected_document_count: usize,
    total_tokens: u64,
) -> Vec<PosFrequencyRow> {
    let mut groups = HashMap::<UniversalPos, AnnotatedFrequencyAccumulator>::new();
    for document in corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
    {
        for count in &document.annotated_term_counts {
            groups
                .entry(count.upos)
                .or_default()
                .add(document.summary.id, count.count);
        }
    }

    let mut rows = groups
        .into_iter()
        .filter_map(|(upos, group)| {
            if group.raw == 0 {
                return None;
            }
            let raw = group.raw;
            let document_counts = group.sorted_document_counts();
            let stats = frequency_distribution_stats(
                &document_counts,
                raw,
                &corpus.documents,
                total_tokens,
            );
            Some(PosFrequencyRow {
                upos: upos.as_upos_str().to_string(),
                raw: stats.raw,
                per_10k: stats.per_10k,
                doc_freq: stats.doc_freq,
                doc_total: selected_document_count,
                dp: stats.dp,
                dispersion_label: annotated_dispersion_label(stats.dp).to_string(),
                top_doc_share: stats.top_doc_share,
            })
        })
        .collect::<Vec<_>>();
    sort_pos_frequency_rows(&mut rows);
    rows
}

fn lemma_pos_frequency_rows(
    corpus: &AnalysisCorpus,
    selected: &[bool],
    selected_document_count: usize,
    total_tokens: u64,
) -> Vec<LexFrequencyRow> {
    let mut groups = HashMap::<(u32, UniversalPos), AnnotatedFrequencyAccumulator>::new();
    for document in corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
    {
        for count in &document.annotated_term_counts {
            groups
                .entry((count.lemma_id, count.upos))
                .or_default()
                .add(document.summary.id, count.count);
        }
    }

    let mut rows = groups
        .into_iter()
        .filter_map(|((lemma_id, upos), group)| {
            if group.raw == 0 {
                return None;
            }
            let lemma = corpus.lemma_table.lookup(lemma_id)?.to_string();
            let raw = group.raw;
            let document_counts = group.sorted_document_counts();
            let stats = frequency_distribution_stats(
                &document_counts,
                raw,
                &corpus.documents,
                total_tokens,
            );
            Some(LexFrequencyRow {
                lemma,
                upos: upos.as_upos_str().to_string(),
                raw: stats.raw,
                per_10k: stats.per_10k,
                doc_freq: stats.doc_freq,
                doc_total: selected_document_count,
                dp: stats.dp,
                dispersion_label: annotated_dispersion_label(stats.dp).to_string(),
                top_doc_share: stats.top_doc_share,
            })
        })
        .collect::<Vec<_>>();
    sort_lex_frequency_rows(&mut rows);
    rows
}

/// Computes annotation-only frequency rows from the attached Ready annotation layer.
pub fn query_annotated_frequency(
    corpus: &AnalysisCorpus,
    query: &AnnotatedFrequencyQuery,
    cache: AnnotationCacheAvailability,
) -> AnnotatedFrequencyResult {
    let (selected, selected_document_count) = selected_document_mask(corpus, &query.selection);
    let total_tokens = corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .map(|document| document.summary.token_count)
        .sum::<u64>();
    let limit = if query.limit == 0 {
        MAX_QUERY_WINDOW
    } else {
        query.limit.min(MAX_QUERY_WINDOW)
    };

    if cache.state != AnnotationCacheState::Ready
        || !selected_documents_have_annotation(corpus, &selected)
    {
        return empty_annotated_frequency_result(
            query,
            cache,
            Some(ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE.to_string()),
            selected_document_count,
            corpus.documents.len(),
            total_tokens,
        );
    }
    if selected_document_count == 0 || total_tokens == 0 {
        return empty_annotated_frequency_result(
            query,
            cache,
            None,
            selected_document_count,
            corpus.documents.len(),
            total_tokens,
        );
    }

    match query.mode {
        AnnotatedFrequencyMode::Pos => {
            let rows = pos_frequency_rows(corpus, &selected, selected_document_count, total_tokens);
            let total_rows = rows.len();
            let offset = query.offset.min(total_rows);
            let pos_rows = rows.into_iter().skip(offset).take(limit).collect();
            AnnotatedFrequencyResult {
                mode: query.mode,
                cache,
                unavailable_message: None,
                total_tokens,
                selected_document_count,
                total_document_count: corpus.documents.len(),
                total_rows,
                offset,
                limit,
                pos_rows,
                lex_rows: Vec::new(),
            }
        }
        AnnotatedFrequencyMode::LemmaPos => {
            let rows =
                lemma_pos_frequency_rows(corpus, &selected, selected_document_count, total_tokens);
            let total_rows = rows.len();
            let offset = query.offset.min(total_rows);
            let lex_rows = rows.into_iter().skip(offset).take(limit).collect();
            AnnotatedFrequencyResult {
                mode: query.mode,
                cache,
                unavailable_message: None,
                total_tokens,
                selected_document_count,
                total_document_count: corpus.documents.len(),
                total_rows,
                offset,
                limit,
                pos_rows: Vec::new(),
                lex_rows,
            }
        }
    }
}

/// Filters, sorts, and pages a frequency list entirely in Rust.
pub fn query_frequency_list(corpus: &AnalysisCorpus, query: &FrequencyQuery) -> FrequencyPage {
    let (selected, selected_document_count) = selected_document_mask(corpus, &query.selection);
    let scoped_total_tokens = corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .map(|document| document.summary.token_count)
        .sum::<u64>();
    let requested_pos_filter = query
        .pos_filter
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let annotation_requested =
        query.group_by == FrequencyGroupMode::Lemma || requested_pos_filter.is_some();
    let annotation_available =
        !annotation_requested || selected_documents_have_annotation(corpus, &selected);
    let mut grouping = resolve_frequency_grouping(query, annotation_available);
    let active_pos_filter = if annotation_available {
        requested_pos_filter
    } else {
        if requested_pos_filter.is_some() {
            grouping.notices.push(FrequencyNotice {
                kind: FrequencyNoticeKind::PosFilterUnavailable,
                message:
                    "POS filtering requires UDPipe annotation for every selected document; the POS filter was not applied."
                        .to_string(),
            });
        }
        None
    };
    let token_query = frequency_query_key(&query.query, &grouping);
    let regex = if query.match_mode == FrequencyMatchMode::Regex && !token_query.is_empty() {
        match Regex::new(&token_query) {
            Ok(regex) => Some(regex),
            Err(_) => {
                return FrequencyPage {
                    total_terms: 0,
                    scoped_total_types: 0,
                    offset: 0,
                    limit: query.limit.min(MAX_QUERY_WINDOW),
                    items: Vec::new(),
                    selected_document_count,
                    total_document_count: corpus.documents.len(),
                    scoped_total_tokens,
                    query_error: Some("Invalid regex pattern.".to_string()),
                    notices: grouping.notices,
                };
            }
        }
    } else {
        None
    };

    let mut groups = HashMap::<String, FrequencyAccumulator>::new();
    let use_annotated_counts =
        grouping.group_by == FrequencyGroupMode::Lemma || active_pos_filter.is_some();
    for document in corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
    {
        if use_annotated_counts {
            for annotated_count in &document.annotated_term_counts {
                if active_pos_filter
                    .is_some_and(|filter| !annotated_count.upos.matches_filter(filter))
                {
                    continue;
                }
                let surface = &corpus.surface_terms[annotated_count.surface_term_id as usize].token;
                if !token_matches_class(surface, query.token_class) {
                    continue;
                }
                let lemma = corpus
                    .lemma_table
                    .lookup(annotated_count.lemma_id)
                    .unwrap_or(surface);
                let key = annotated_frequency_group_key(surface, lemma, &grouping);
                let group = groups.entry(key).or_default();
                group.frequency += annotated_count.count;
                *group
                    .document_counts
                    .entry(document.summary.id)
                    .or_default() += annotated_count.count;
            }
        } else {
            for &(surface_term_id, count) in &document.surface_term_counts {
                let surface = &corpus.surface_terms[surface_term_id as usize].token;
                if !token_matches_class(surface, query.token_class) {
                    continue;
                }
                let key = frequency_group_key(surface, &grouping);
                let group = groups.entry(key).or_default();
                group.frequency += count;
                *group
                    .document_counts
                    .entry(document.summary.id)
                    .or_default() += count;
            }
        }
    }

    let scoped_total_types = groups.len();
    let mut items = Vec::new();
    for (token, group) in groups {
        if group.frequency < query.minimum_frequency
            || (query.hide_stopwords && ENGLISH_STOPWORDS.contains(token.as_str()))
        {
            continue;
        }
        let matches = if token_query.is_empty() {
            true
        } else {
            match query.match_mode {
                FrequencyMatchMode::Contains => token.contains(&token_query),
                FrequencyMatchMode::Exact => token == token_query,
                FrequencyMatchMode::StartsWith => token.starts_with(&token_query),
                FrequencyMatchMode::Regex => regex
                    .as_ref()
                    .is_some_and(|pattern| pattern.is_match(&token)),
            }
        };
        if !matches {
            continue;
        }
        let mut document_counts = group.document_counts.into_iter().collect::<Vec<_>>();
        document_counts.sort_unstable_by_key(|(document_id, _)| *document_id);
        let dispersion = compute_dispersion(
            &document_counts,
            group.frequency,
            &corpus.documents,
            scoped_total_tokens,
        );
        items.push(FrequencyTermItem {
            token,
            frequency: group.frequency,
            document_frequency: document_counts.len(),
            rate_per_10k: if scoped_total_tokens > 0 {
                group.frequency as f64 / scoped_total_tokens as f64 * 10_000.0
            } else {
                0.0
            },
            rate_per_million: if scoped_total_tokens > 0 {
                group.frequency as f64 / scoped_total_tokens as f64 * 1_000_000.0
            } else {
                0.0
            },
            dispersion,
        });
    }

    items.sort_by(|left, right| {
        let ordering = compare_frequency_terms(left, right, query.sort_key);
        let ordering = if ordering == Ordering::Equal {
            left.token.cmp(&right.token)
        } else {
            ordering
        };
        match query.sort_direction {
            SortDirection::Asc => ordering,
            SortDirection::Desc if query.sort_key == FrequencySortKey::Token => ordering.reverse(),
            SortDirection::Desc => {
                let primary = compare_frequency_terms(left, right, query.sort_key).reverse();
                if primary == Ordering::Equal {
                    left.token.cmp(&right.token)
                } else {
                    primary
                }
            }
        }
    });
    let total_terms = items.len();
    let limit = query.limit.min(MAX_QUERY_WINDOW);
    let offset = query.offset.min(total_terms);
    let items = items.into_iter().skip(offset).take(limit).collect();

    FrequencyPage {
        total_terms,
        scoped_total_types,
        offset,
        limit,
        items,
        selected_document_count,
        total_document_count: corpus.documents.len(),
        scoped_total_tokens,
        query_error: None,
        notices: grouping.notices,
    }
}

/// Returns one bounded per-document distribution window for a token.
pub fn query_term_distribution(
    corpus: &AnalysisCorpus,
    token: &str,
    query: &TermDistributionQuery,
) -> TermDistributionPage {
    let (selected, selected_count) = selected_document_mask(corpus, &query.selection);
    let grouping_query = FrequencyQuery {
        selection: query.selection.clone(),
        query: String::new(),
        match_mode: FrequencyMatchMode::Contains,
        minimum_frequency: 1,
        token_class: FrequencyTokenClass::All,
        hide_stopwords: false,
        group_by: query.group_by,
        case_fold: query.case_fold,
        language: query.language.clone(),
        pos_filter: None,
        sort_key: FrequencySortKey::Token,
        sort_direction: SortDirection::Asc,
        offset: 0,
        limit: 1,
    };
    let annotation_available = query.group_by != FrequencyGroupMode::Lemma
        || selected_documents_have_annotation(corpus, &selected);
    let grouping = resolve_frequency_grouping(&grouping_query, annotation_available);
    let normalised_token = frequency_query_key(token, &grouping);
    let mut counts = HashMap::<usize, u64>::new();
    if grouping.group_by == FrequencyGroupMode::Lemma {
        for document in corpus
            .documents
            .iter()
            .filter(|document| selected[document.summary.id])
        {
            for annotated_count in &document.annotated_term_counts {
                let surface = &corpus.surface_terms[annotated_count.surface_term_id as usize].token;
                let lemma = corpus
                    .lemma_table
                    .lookup(annotated_count.lemma_id)
                    .unwrap_or(surface);
                if annotated_frequency_group_key(surface, lemma, &grouping) == normalised_token {
                    *counts.entry(document.summary.id).or_default() += annotated_count.count;
                }
            }
        }
    } else {
        for surface_term in &corpus.surface_terms {
            if frequency_group_key(&surface_term.token, &grouping) != normalised_token {
                continue;
            }
            for &(document_id, count) in &surface_term.document_counts {
                if selected[document_id] {
                    *counts.entry(document_id).or_default() += count;
                }
            }
        }
    }
    let mut max_rate_per_10k = 0.0f64;
    let mut max_rate_per_million = 0.0f64;
    for document in corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
    {
        let count = counts.get(&document.summary.id).copied().unwrap_or(0);
        let rate = if document.summary.token_count > 0 {
            count as f64 / document.summary.token_count as f64 * 10_000.0
        } else {
            0.0
        };
        max_rate_per_10k = max_rate_per_10k.max(rate);
        max_rate_per_million = max_rate_per_million.max(rate * 100.0);
    }

    let limit = query.limit.min(MAX_QUERY_WINDOW);
    let offset = query.offset.min(selected_count);
    let items = corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .skip(offset)
        .take(limit)
        .map(|document| {
            let summary = &document.summary;
            let count = counts.get(&summary.id).copied().unwrap_or(0);
            TermDistributionItem {
                document_id: summary.id,
                source_path: summary.source_path.clone(),
                relative_path: summary.relative_path.clone(),
                token_count: summary.token_count,
                count,
                rate_per_10k: if summary.token_count > 0 {
                    count as f64 / summary.token_count as f64 * 10_000.0
                } else {
                    0.0
                },
                rate_per_million: if summary.token_count > 0 {
                    count as f64 / summary.token_count as f64 * 1_000_000.0
                } else {
                    0.0
                },
            }
        })
        .collect();

    TermDistributionPage {
        token: normalised_token,
        total_documents: selected_count,
        offset,
        limit,
        items,
        max_rate_per_10k,
        max_rate_per_million,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{RemovalMatcher, RemovalRule, RemovalRuleSource, RemovalScope};
    use crate::language::{ProjectLanguage, resolve_effective_document_languages};
    use std::cell::Cell;
    use std::path::PathBuf;

    fn record(name: &str) -> DocumentRecord {
        DocumentRecord {
            source_path: PathBuf::from(name),
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Text,
            size_bytes: 0,
            extraction_route: None,
            pdf_profile: None,
            ocr_quality: None,
        }
    }

    fn build_options() -> AnalysisBuildOptions {
        let cleaning_config = CleaningConfig::default();
        AnalysisBuildOptions {
            built_at: "2026-01-01T00:00:00Z".to_string(),
            build_signature: "signature".to_string(),
            processing_config_signature: cleaning_config_signature(&cleaning_config),
            overview_context: CorpusOverviewContext::default(),
            udpipe_model_dir: Some(PathBuf::from("__missing_udpipe_models_for_tests__")),
            language_overrides: std::collections::HashMap::new(),
            tokenization: TokenizationConfig::default(),
            annotation_cache_root: None,
            force_compute_hashes: false,
        }
    }

    fn anywhere_literal_rule(text: &str) -> RemovalRule {
        RemovalRule {
            id: "rule-1".to_string(),
            label: format!("Anywhere: {text}"),
            source: RemovalRuleSource::PromotedRepeatedArtifact,
            matcher: RemovalMatcher::Literal {
                text: text.to_string(),
            },
            scope: RemovalScope::Anywhere,
            enabled: true,
        }
    }

    fn processed_resolution(record: &DocumentRecord, text: &str) -> AnalysisInputResolution {
        resolve_analysis_input(
            record,
            &CleaningConfig::default(),
            |_, _| Ok(text.to_string()),
            |_, _| panic!("processed test input must not use fallback"),
        )
    }

    fn corpus_for(texts: &[(&str, &str)]) -> AnalysisCorpus {
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        build_analysis_corpus(
            &records,
            build_options(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    text_by_name
                        .get(record.source_path.to_str().unwrap())
                        .copied()
                        .unwrap_or_default(),
                ))
            },
            |_| {},
        )
        .unwrap()
    }

    fn base_frequency_query() -> FrequencyQuery {
        FrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            query: String::new(),
            match_mode: FrequencyMatchMode::Contains,
            minimum_frequency: 1,
            token_class: FrequencyTokenClass::All,
            hide_stopwords: false,
            group_by: FrequencyGroupMode::Surface,
            case_fold: false,
            language: None,
            pos_filter: None,
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            offset: 0,
            limit: 100,
        }
    }

    fn frequency_tokens(page: &FrequencyPage) -> Vec<(&str, u64)> {
        page.items
            .iter()
            .map(|item| (item.token.as_str(), item.frequency))
            .collect()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {actual} to be close to {expected}"
        );
    }

    fn concordance_document(index: usize, name: &str, text: &str) -> ConcordanceDocument {
        ConcordanceDocument {
            document_index: index,
            source_path: name.to_string(),
            display_name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn concordance_query(query: &str) -> ConcordanceQuery {
        ConcordanceQuery {
            query: query.to_string(),
            case_sensitive: false,
            window_tokens: 5,
            offset: 0,
            limit: 100,
        }
    }

    fn concordance_for(
        documents: &[ConcordanceDocument],
        query: ConcordanceQuery,
    ) -> Result<ConcordanceResult, ConcordanceError> {
        search_concordance_documents(documents, &query, &TokenizationConfig::default())
    }

    #[derive(Debug, PartialEq)]
    struct AnalysisCorpusSnapshot {
        overview: CorpusOverview,
        documents: Vec<AnalysisDocumentSnapshot>,
        surface_terms: Vec<(String, Vec<(usize, u64)>)>,
        prepared_terms: Vec<AnalysisPreparedTermSnapshot>,
        frequency_by_token: FrequencyPage,
        frequency_by_frequency: FrequencyPage,
        alpha_distribution: TermDistributionPage,
    }

    #[derive(Debug, PartialEq)]
    struct AnalysisDocumentSnapshot {
        summary: AnalysisDocumentSummary,
        surface_term_counts: Vec<(u32, u64)>,
        annotated_term_counts: Vec<(u32, u32, UniversalPos, u64)>,
        annotation_available: bool,
        mattr: Option<f64>,
        mtld: Option<f64>,
    }

    #[derive(Debug, PartialEq)]
    struct AnalysisPreparedTermSnapshot {
        token: String,
        frequency: u64,
        document_frequency: usize,
        document_counts: Vec<(usize, u64)>,
        dispersion: f64,
    }

    fn analysis_product_for(
        texts: &[(&str, &str)],
        options: AnalysisBuildOptions,
        parallel_map: bool,
    ) -> AnalysisBuildProduct {
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        build_analysis_corpus_with_mapping(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    text_by_name
                        .get(record.source_path.to_str().unwrap())
                        .copied()
                        .unwrap_or_default(),
                ))
            },
            |_| {},
            parallel_map,
        )
        .unwrap()
    }

    fn annotation_options(
        language_overrides: HashMap<usize, String>,
    ) -> AnnotationLayerBuildOptions {
        AnnotationLayerBuildOptions {
            udpipe_model_dir: Some(default_model_dir()),
            language_overrides,
        }
    }

    fn annotation_layer_for(
        texts: &[(&str, &str)],
        language_overrides: HashMap<usize, String>,
        parallel_map: bool,
    ) -> (AnalysisCorpus, AnnotationLayer) {
        let mut analysis_options = build_options();
        analysis_options.language_overrides = language_overrides.clone();
        let corpus = analysis_product_for(texts, analysis_options, true).corpus;
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        let layer = build_annotation_layer_with_mapping(
            &corpus,
            &records,
            annotation_options(language_overrides),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    text_by_name
                        .get(record.source_path.to_str().unwrap())
                        .copied()
                        .unwrap_or_default(),
                ))
            },
            parallel_map,
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();
        (corpus, layer)
    }

    #[test]
    fn project_language_is_effective_for_all_documents() {
        let records = vec![record("a.txt"), record("b.txt")];
        let language_overrides =
            resolve_effective_document_languages(ProjectLanguage::Pt, &records);
        let mut options = build_options();
        options.language_overrides = language_overrides.clone();

        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                let text = if record.source_path == std::path::Path::new("a.txt") {
                    "The quick brown fox."
                } else {
                    "Le chat noir court."
                };
                Ok(processed_resolution(record, text))
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(language_overrides.len(), records.len());
        assert!(language_overrides.values().all(|language| language == "pt"));
        assert_eq!(corpus.documents.len(), 2);
        assert!(corpus.documents.iter().all(|document| {
            document.summary.language.as_deref() == Some("pt")
                && document.summary.language_is_overridden
                && document.summary.language_confidence.is_none()
        }));
    }

    #[test]
    fn annotation_cache_round_trips_ready_with_project_language() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let language_overrides =
            resolve_effective_document_languages(ProjectLanguage::En, &records);
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());
        options.language_overrides = language_overrides.clone();

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides,
        };

        build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        let corpus_cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();

        assert!(corpus_cached.overview().annotation_available);
    }

    #[test]
    fn kwic_finds_surface_token_with_token_windows() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "alpha beta gamma beta delta epsilon",
        )];
        let mut query = concordance_query("beta");
        query.window_tokens = 2;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 2);
        assert_eq!(result.rows[0].left_context, ["alpha"]);
        assert_eq!(result.rows[0].node, "beta");
        assert_eq!(result.rows[0].right_context, ["gamma", "beta"]);
        assert_eq!(result.rows[1].left_context, ["beta", "gamma"]);
        assert_eq!(result.rows[1].right_context, ["delta", "epsilon"]);
    }

    #[test]
    fn kwic_does_not_match_substrings_inside_tokens() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "the there breathe other The",
        )];

        let result = concordance_for(&documents, concordance_query("the")).unwrap();

        assert_eq!(result.total_hits, 2);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| row.node.as_str())
                .collect::<Vec<_>>(),
            ["the", "The"]
        );
    }

    #[test]
    fn kwic_is_case_insensitive_by_default() {
        let documents = vec![concordance_document(0, "doc.txt", "The the THE theme")];

        let result = concordance_for(&documents, concordance_query("the")).unwrap();

        assert_eq!(result.total_hits, 3);
    }

    #[test]
    fn kwic_can_match_case_sensitive_when_enabled() {
        let documents = vec![concordance_document(0, "doc.txt", "The the THE")];
        let mut query = concordance_query("the");
        query.case_sensitive = true;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 1);
        assert_eq!(result.rows[0].node, "the");
    }

    #[test]
    fn kwic_rejects_empty_query() {
        let documents = vec![concordance_document(0, "doc.txt", "alpha")];

        let error = concordance_for(&documents, concordance_query("   ")).unwrap_err();

        assert_eq!(error, ConcordanceError::EmptyQuery);
    }

    #[test]
    fn kwic_rejects_multi_token_query_for_alpha() {
        let documents = vec![concordance_document(0, "doc.txt", "alpha beta")];

        let error = concordance_for(&documents, concordance_query("alpha beta")).unwrap_err();

        assert_eq!(error, ConcordanceError::MultiTokenQuery);
        assert_eq!(
            error.to_string(),
            "KWIC alpha currently supports one-token surface queries only."
        );
    }

    #[test]
    fn kwic_reports_total_hits_and_documents_with_hits() {
        let documents = vec![
            concordance_document(0, "a.txt", "alpha beta alpha"),
            concordance_document(1, "b.txt", "beta gamma"),
            concordance_document(2, "c.txt", "alpha"),
        ];

        let result = concordance_for(&documents, concordance_query("alpha")).unwrap();

        assert_eq!(result.total_hits, 3);
        assert_eq!(result.documents_with_hits, 2);
    }

    #[test]
    fn kwic_respects_offset_and_limit() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "alpha beta alpha gamma alpha delta alpha",
        )];
        let mut query = concordance_query("alpha");
        query.offset = 1;
        query.limit = 2;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 4);
        assert_eq!(result.offset, 1);
        assert_eq!(result.limit, 2);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| row.token_index)
                .collect::<Vec<_>>(),
            [2, 4]
        );
    }

    #[test]
    fn kwic_uses_canonical_tokenisation() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "state-of-the-art state of the art",
        )];

        let compound = concordance_for(&documents, concordance_query("state-of-the-art")).unwrap();
        let single = concordance_for(&documents, concordance_query("state")).unwrap();

        assert_eq!(compound.total_hits, 1);
        assert_eq!(compound.rows[0].node, "state-of-the-art");
        assert_eq!(single.total_hits, 1);
    }

    fn snapshot_analysis_product(product: &AnalysisBuildProduct) -> AnalysisCorpusSnapshot {
        let corpus = &product.corpus;
        let mut frequency_by_frequency_query = base_frequency_query();
        frequency_by_frequency_query.sort_key = FrequencySortKey::Frequency;
        frequency_by_frequency_query.sort_direction = SortDirection::Desc;
        frequency_by_frequency_query.limit = 10;
        AnalysisCorpusSnapshot {
            overview: corpus.overview().clone(),
            documents: corpus
                .documents
                .iter()
                .map(|document| AnalysisDocumentSnapshot {
                    summary: document.summary.clone(),
                    surface_term_counts: document.surface_term_counts.clone(),
                    annotated_term_counts: document
                        .annotated_term_counts
                        .iter()
                        .map(|count| {
                            (
                                count.surface_term_id,
                                count.lemma_id,
                                count.upos,
                                count.count,
                            )
                        })
                        .collect(),
                    annotation_available: document.annotation_available,
                    mattr: document.mattr,
                    mtld: document.mtld,
                })
                .collect(),
            surface_terms: corpus
                .surface_terms
                .iter()
                .map(|term| (term.token.clone(), term.document_counts.clone()))
                .collect(),
            prepared_terms: product
                .terms
                .iter()
                .map(|term| AnalysisPreparedTermSnapshot {
                    token: term.token.clone(),
                    frequency: term.frequency,
                    document_frequency: term.document_frequency,
                    document_counts: term.document_counts.clone(),
                    dispersion: term.dispersion,
                })
                .collect(),
            frequency_by_token: query_frequency_list(corpus, &base_frequency_query()),
            frequency_by_frequency: query_frequency_list(corpus, &frequency_by_frequency_query),
            alpha_distribution: query_term_distribution(
                corpus,
                "alpha",
                &TermDistributionQuery {
                    selection: AnalysisDocumentSelection::default(),
                    group_by: FrequencyGroupMode::Surface,
                    case_fold: true,
                    language: None,
                    offset: 0,
                    limit: 10,
                },
            ),
        }
    }

    fn add_document_annotations(
        corpus: &mut AnalysisCorpus,
        document_id: usize,
        entries: &[(&str, &str, UniversalPos, u64)],
    ) {
        let counts = entries
            .iter()
            .map(|(surface, lemma, upos, count)| {
                let surface_term_id = corpus
                    .surface_terms
                    .iter()
                    .position(|term| term.token == *surface)
                    .expect("test surface term should exist")
                    as u32;
                let lemma_id = corpus.lemma_table.intern(lemma);
                AnnotatedTermCount {
                    surface_term_id,
                    lemma_id,
                    upos: *upos,
                    count: *count,
                }
            })
            .collect::<Vec<_>>();
        corpus.documents[document_id].annotation_available = true;
        corpus.documents[document_id].annotated_term_counts = counts;
    }

    fn ready_annotation_cache() -> AnnotationCacheAvailability {
        AnnotationCacheAvailability {
            state: AnnotationCacheState::Ready,
            detail: None,
        }
    }

    fn annotated_frequency_query(mode: AnnotatedFrequencyMode) -> AnnotatedFrequencyQuery {
        AnnotatedFrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            mode,
            offset: 0,
            limit: 100,
        }
    }

    #[test]
    fn build_is_surface_only_without_udpipe_models() {
        let corpus = corpus_for(&[("surface.txt", "Cats cat ran running")]);

        assert!(!corpus.overview().annotation_available);
        assert_eq!(
            corpus.overview().build.lemmatisation.as_deref(),
            Some("Surface-only build; lemma/POS annotation is not built yet.")
        );
        assert!(
            corpus
                .documents
                .iter()
                .all(|document| !document.annotation_available)
        );
        assert!(
            corpus
                .documents
                .iter()
                .all(|document| document.annotated_term_counts.is_empty())
        );

        let page = query_frequency_list(&corpus, &base_frequency_query());
        assert_eq!(
            frequency_tokens(&page),
            [("Cats", 1), ("cat", 1), ("ran", 1), ("running", 1)]
        );
    }

    #[test]
    fn annotation_input_language_overrides_use_original_record_indices() {
        let texts = [("z.txt", "Bonjour le monde"), ("a.txt", "Cats run")];
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        let mut language_overrides = HashMap::new();
        language_overrides.insert(0, "fr".to_string());
        language_overrides.insert(1, "en".to_string());
        let mut analysis_options = build_options();
        analysis_options.language_overrides = language_overrides.clone();
        let corpus = analysis_product_for(&texts, analysis_options, true).corpus;
        let mut load_text = |record: &DocumentRecord| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        };

        let inputs = load_annotation_document_inputs(
            &corpus,
            &records,
            &language_overrides,
            &AtomicBool::new(false),
            &mut load_text,
        )
        .unwrap();

        assert_eq!(inputs[0].relative_path, "a.txt");
        assert_eq!(inputs[0].language, "en");
        assert_eq!(inputs[1].relative_path, "z.txt");
        assert_eq!(inputs[1].language, "fr");
    }

    #[test]
    fn annotation_parallel_build_matches_serial_reference() {
        let texts = [("b.txt", "Dogs run quickly"), ("a.txt", "Cats chase mice")];
        let mut language_overrides = HashMap::new();
        language_overrides.insert(0, "en".to_string());
        language_overrides.insert(1, "en".to_string());

        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let (_, parallel) = annotation_layer_for(&texts, language_overrides, true);

        assert_eq!(parallel, serial);
    }

    #[test]
    fn annotation_build_blocks_unsupported_language_before_tagging() {
        let texts = [("unsupported.txt", "Hallo welt")];
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        let mut language_overrides = HashMap::new();
        language_overrides.insert(0, "zz".to_string());
        let mut analysis_options = build_options();
        analysis_options.language_overrides = language_overrides.clone();
        let corpus = analysis_product_for(&texts, analysis_options, true).corpus;

        let result = build_annotation_layer_with_mapping(
            &corpus,
            &records,
            annotation_options(language_overrides),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    text_by_name
                        .get(record.source_path.to_str().unwrap())
                        .copied()
                        .unwrap_or_default(),
                ))
            },
            true,
        &std::sync::atomic::AtomicUsize::new(0));

        assert_eq!(
            result.unwrap_err(),
            AnnotationLayerBuildError::UnsupportedLanguages {
                languages: vec!["zz".to_string()],
            }
        );
    }

    #[test]
    fn annotation_identity_mismatch_is_error_not_skip() {
        let corpus = corpus_for(&[("identity.txt", "Cats run")]);
        let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
        let annotated = vec![AnnotatedToken {
            surface: "Missing".to_string(),
            lemma: "missing".to_string(),
            upos: UniversalPos::Noun,
            token_index: 0,
            features: HashMap::new(),
        }];

        let result = resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup);

        assert!(matches!(
            result,
            Err(AnnotationLayerBuildError::AlignmentOrIdentityMismatch { .. })
        ));
    }

    #[test]
    fn annotation_surface_lookup_uses_raw_surface_terms_not_frequency_keys() {
        let corpus = corpus_for(&[("case.txt", "Beta beta")]);
        let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
        let upper_id = *surface_lookup.get("Beta").unwrap();
        let lower_id = *surface_lookup.get("beta").unwrap();
        assert_ne!(upper_id, lower_id);
        let annotated = vec![
            AnnotatedToken {
                surface: "Beta".to_string(),
                lemma: "beta".to_string(),
                upos: UniversalPos::Noun,
                token_index: 0,
                features: HashMap::new(),
            },
            AnnotatedToken {
                surface: "beta".to_string(),
                lemma: "beta".to_string(),
                upos: UniversalPos::Noun,
                token_index: 1,
                features: HashMap::new(),
            },
        ];

        let counts =
            resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup).unwrap();

        assert_eq!(
            counts,
            vec![
                LocalAnnotatedTermCount {
                    surface_term_id: upper_id,
                    lemma: "beta".to_string(),
                    upos: UniversalPos::Noun,
                    count: 1,
                },
                LocalAnnotatedTermCount {
                    surface_term_id: lower_id,
                    lemma: "beta".to_string(),
                    upos: UniversalPos::Noun,
                    count: 1,
                },
            ]
        );
    }

    #[test]
    fn annotation_layer_happy_path_populates_lemma_upos_and_surface_ids() {
        let texts = [("english.txt", "Dogs run")];
        let mut language_overrides = HashMap::new();
        language_overrides.insert(0, "en".to_string());
        let (corpus, layer) = annotation_layer_for(&texts, language_overrides, true);

        assert_eq!(layer.languages_used, vec!["en"]);
        assert_eq!(layer.documents.len(), corpus.documents.len());
        assert_eq!(layer.documents[0].analysis_document_index, 0);
        assert_eq!(layer.documents[0].relative_path, "english.txt");
        assert!(!layer.lemma_table.is_empty());

        let annotated_token_total = layer.documents[0]
            .term_counts
            .iter()
            .map(|count| count.count)
            .sum::<u64>();
        assert_eq!(
            annotated_token_total,
            corpus.documents[0].summary.token_count
        );
        assert!(
            layer.documents[0]
                .term_counts
                .iter()
                .any(|count| count.upos != UniversalPos::X)
        );
        for count in &layer.documents[0].term_counts {
            assert!((count.surface_term_id as usize) < corpus.surface_terms.len());
            assert!(layer.lemma_table.lookup(count.lemma_id).is_some());
        }
    }

    #[test]
    fn language_override_skips_detection_in_fast_build() {
        let records = vec![record("override.txt")];
        let mut options = build_options();
        options.language_overrides.insert(0, "en".to_string());
        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Bonjour le monde")),
            |_| {},
        )
        .unwrap();
        let summary = &corpus.documents[0].summary;

        assert_eq!(summary.language.as_deref(), Some("en"));
        assert_eq!(summary.language_confidence, None);
        assert!(summary.language_is_overridden);
    }

    #[test]
    fn parallel_map_matches_sequential_reference_build() {
        let long_text = (0..130)
            .map(|index| match index % 5 {
                0 => "alpha",
                1 => "beta",
                2 => "gamma",
                3 => "delta",
                _ => "epsilon",
            })
            .collect::<Vec<_>>()
            .join(" ");
        let texts = [
            ("z.txt", "Bonjour le monde alpha"),
            ("a.txt", long_text.as_str()),
            ("empty.txt", " \n"),
            ("b.txt", "Cats cat ran running alpha"),
        ];
        let mut options = build_options();
        options.language_overrides.insert(0, "fr".to_string());

        let sequential = analysis_product_for(&texts, options.clone(), false);
        let parallel = analysis_product_for(&texts, options, true);

        assert_eq!(
            snapshot_analysis_product(&parallel),
            snapshot_analysis_product(&sequential)
        );
    }

    #[test]
    fn golden_fixture_pins_document_and_term_ordering() {
        let texts = [
            ("z.txt", "Beta delta alpha"),
            ("a.txt", "Beta alpha beta"),
            ("m.txt", "alpha Gamma"),
            ("empty.txt", " "),
        ];
        let mut options = build_options();
        for index in 0..texts.len() {
            options.language_overrides.insert(index, "en".to_string());
        }
        let product = analysis_product_for(&texts, options, true);
        let corpus = &product.corpus;

        let summaries = corpus
            .documents
            .iter()
            .map(|document| {
                (
                    document.summary.id,
                    document.summary.relative_path.as_str(),
                    document.summary.token_count,
                    document.summary.language.as_deref(),
                    document.summary.language_confidence,
                    document.summary.language_is_overridden,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summaries,
            vec![
                (0, "a.txt", 3, Some("en"), None, true),
                (1, "m.txt", 2, Some("en"), None, true),
                (2, "z.txt", 3, Some("en"), None, true),
            ]
        );

        let prepared_terms = product
            .terms
            .iter()
            .map(|term| {
                (
                    term.token.as_str(),
                    term.frequency,
                    term.document_frequency,
                    term.document_counts.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            prepared_terms,
            vec![
                ("beta", 3, 2, vec![(0, 2), (2, 1)]),
                ("alpha", 3, 3, vec![(0, 1), (1, 1), (2, 1)]),
                ("gamma", 1, 1, vec![(1, 1)]),
                ("delta", 1, 1, vec![(2, 1)]),
            ]
        );

        let surface_terms = corpus
            .surface_terms
            .iter()
            .map(|term| (term.token.as_str(), term.document_counts.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            surface_terms,
            vec![
                ("Beta", vec![(0, 1), (2, 1)]),
                ("alpha", vec![(0, 1), (1, 1), (2, 1)]),
                ("beta", vec![(0, 1)]),
                ("Gamma", vec![(1, 1)]),
                ("delta", vec![(2, 1)]),
            ]
        );

        let document_surface_counts = corpus
            .documents
            .iter()
            .map(|document| {
                (
                    document.summary.relative_path.as_str(),
                    document.surface_term_counts.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            document_surface_counts,
            vec![
                ("a.txt", vec![(0, 1), (1, 1), (2, 1)]),
                ("m.txt", vec![(1, 1), (3, 1)]),
                ("z.txt", vec![(0, 1), (1, 1), (4, 1)]),
            ]
        );
        assert_eq!(
            corpus.overview().vocabulary_growth.points,
            vec![
                VocabularyGrowthPoint {
                    tokens: 1,
                    types: 1,
                },
                VocabularyGrowthPoint {
                    tokens: 2,
                    types: 2,
                },
                VocabularyGrowthPoint {
                    tokens: 4,
                    types: 2,
                },
                VocabularyGrowthPoint {
                    tokens: 8,
                    types: 4,
                },
            ]
        );
    }

    #[test]
    fn resolver_uses_active_processed_pipeline_output() {
        let record = record("processed.txt");
        let cleaning_config = CleaningConfig {
            remove_patterns: vec!["REMOVE".to_string()],
            ..CleaningConfig::default()
        };
        let fallback_called = Cell::new(false);

        let resolution = resolve_analysis_input(
            &record,
            &cleaning_config,
            |_, config| Ok(crate::clean::clean_text("keep REMOVE", config)),
            |_, _| {
                fallback_called.set(true);
                Ok("keep REMOVE".to_string())
            },
        );

        assert_eq!(resolution.text.as_deref(), Some("keep "));
        assert_eq!(resolution.provenance, AnalysisInputProvenance::Processed);
        assert_eq!(resolution.status, AnalysisInputStatus::Ready);
        assert!(!fallback_called.get());
    }

    #[test]
    fn resolver_uses_inline_repeated_artifact_removal_rule() {
        let record = record("inline.txt");
        let cleaning_config = CleaningConfig {
            normalize_whitespace_line_endings: true,
            removal_rules: vec![anywhere_literal_rule("<br />")],
            ..CleaningConfig::default()
        };
        let fallback_called = Cell::new(false);

        let resolution = resolve_analysis_input(
            &record,
            &cleaning_config,
            |_, config| Ok(crate::clean::clean_text("foo<br />bar", config)),
            |_, _| {
                fallback_called.set(true);
                Ok("foo<br />bar".to_string())
            },
        );

        assert_eq!(resolution.text.as_deref(), Some("foo bar"));
        assert_eq!(resolution.provenance, AnalysisInputProvenance::Processed);
        assert!(!fallback_called.get());
    }

    #[test]
    fn resolver_falls_back_only_when_processing_fails() {
        let record = record("fallback.txt");
        let resolution = resolve_analysis_input(
            &record,
            &CleaningConfig::default(),
            |_, _| Err("processing unavailable".to_string()),
            |_, _| Ok("native source text".to_string()),
        );

        assert_eq!(
            resolution.provenance,
            AnalysisInputProvenance::SourceTextFallback
        );
        assert_eq!(resolution.text.as_deref(), Some("native source text"));
        assert!(matches!(
            resolution.status,
            AnalysisInputStatus::ReadyWithFallback { .. }
        ));
    }

    #[test]
    fn empty_processed_output_is_excluded_without_raw_fallback() {
        let record = record("empty.txt");
        let fallback_called = Cell::new(false);
        let resolution = resolve_analysis_input(
            &record,
            &CleaningConfig::default(),
            |_, _| Ok(" \n".to_string()),
            |_, _| {
                fallback_called.set(true);
                Ok("raw text".to_string())
            },
        );

        assert_eq!(
            resolution.provenance,
            AnalysisInputProvenance::EmptyOrFailed
        );
        assert_eq!(resolution.status, AnalysisInputStatus::Empty);
        assert_eq!(resolution.text, None);
        assert!(!fallback_called.get());
    }

    #[test]
    fn analyse_excludes_punctuation_from_metrics() {
        let with_punctuation = corpus_for(&[("punct.txt", "Alpha, beta! Alpha? — beta.")]);
        let stripped = corpus_for(&[("plain.txt", "Alpha beta Alpha beta")]);

        assert_eq!(
            with_punctuation.overview().total_tokens,
            stripped.overview().total_tokens
        );
        assert_eq!(
            with_punctuation.overview().total_types,
            stripped.overview().total_types
        );
        assert_eq!(
            with_punctuation.overview().lexical_spectrum.hapax_count,
            stripped.overview().lexical_spectrum.hapax_count
        );
        assert_eq!(
            with_punctuation
                .overview()
                .frequency_concentration
                .shannon_entropy_bits,
            stripped
                .overview()
                .frequency_concentration
                .shannon_entropy_bits
        );

        let page = query_frequency_list(&with_punctuation, &base_frequency_query());
        assert_eq!(frequency_tokens(&page), [("Alpha", 2), ("beta", 2)]);
    }

    #[test]
    fn prepare_analyse_token_parity() {
        let config = CleaningConfig {
            extract_html: true,
            normalize_unicode: true,
            replace_diacritics: false,
            ..CleaningConfig::default()
        };
        let fixtures = [
            "Well-being, don't stop. 3.14 and 1,000.",
            "<p>Cafe\u{301} — café! Alpha&nbsp;beta.</p>",
            "leading- -trailing alpha—beta",
        ];

        for fixture in fixtures {
            let prepare =
                crate::word_count::word_token_surfaces_in_processed_text(fixture, &config);
            let html = crate::html::extract_html(fixture);
            let cleaned = crate::clean::clean_text(&html, &config);
            let analyse = analysis_word_tokens(&cleaned, 0, &TokenizationConfig::default())
                .into_iter()
                .map(|token| token.surface.to_string())
                .collect::<Vec<_>>();
            assert_eq!(analyse, prepare);
        }
    }

    #[test]
    fn frequency_invariant_under_punctuation() {
        let punctuated = corpus_for(&[("punct.txt", "(alpha), beta! alpha; beta?")]);
        let plain = corpus_for(&[("plain.txt", "alpha beta alpha beta")]);

        let punctuated_page = query_frequency_list(&punctuated, &base_frequency_query());
        let plain_page = query_frequency_list(&plain, &base_frequency_query());

        assert_eq!(
            frequency_tokens(&punctuated_page),
            frequency_tokens(&plain_page)
        );
        assert_eq!(
            punctuated.overview().total_tokens,
            plain.overview().total_tokens
        );
        assert_eq!(
            punctuated.overview().total_types,
            plain.overview().total_types
        );
    }

    #[test]
    fn provenance_counts_reconcile_for_mixed_corpus() {
        let records = vec![
            record("processed.txt"),
            record("fallback.txt"),
            record("empty.txt"),
        ];
        let mut options = build_options();
        options.overview_context.sources_in_project = Some(records.len());
        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                let name = record.relative_path.to_string_lossy();
                Ok(match name.as_ref() {
                    "processed.txt" => processed_resolution(record, "processed words"),
                    "fallback.txt" => resolve_analysis_input(
                        record,
                        &CleaningConfig::default(),
                        |_, _| Err("processing unavailable".to_string()),
                        |_, _| Ok("fallback words".to_string()),
                    ),
                    _ => processed_resolution(record, ""),
                })
            },
            |_| {},
        )
        .unwrap();
        let population = &corpus.overview().population;

        assert_eq!(population.processed_used, 1);
        assert_eq!(population.using_source_text_fallback, 1);
        assert_eq!(population.included_in_analysis, 2);
        assert_eq!(population.empty_or_failed, 1);
        assert_eq!(
            population.included_in_analysis,
            population.processed_used + population.using_source_text_fallback
        );
        assert_eq!(
            population.sources_in_project.unwrap(),
            population.included_in_analysis + population.empty_or_failed
        );
        assert_eq!(
            population.input_source,
            "Processed text; 1 using source-text fallback"
        );
        assert_eq!(
            corpus.overview().build.input_source,
            "Processed text; 1 using source-text fallback"
        );
    }

    #[test]
    fn cleaning_rule_changes_analysed_token_count_after_rebuild() {
        let records = vec![record("rules.txt")];
        let build = |cleaning_config: &CleaningConfig| {
            let mut options = build_options();
            options.processing_config_signature = cleaning_config_signature(cleaning_config);
            build_analysis_corpus(
                &records,
                options,
                &AtomicBool::new(false),
                |record| {
                    Ok(resolve_analysis_input(
                        record,
                        cleaning_config,
                        |_, config| Ok(crate::clean::clean_text("keep REMOVE", config)),
                        |_, _| unreachable!("processing succeeds"),
                    ))
                },
                |_| {},
            )
            .unwrap()
        };
        let before = build(&CleaningConfig::default());
        let after_config = CleaningConfig {
            remove_patterns: vec!["REMOVE".to_string()],
            ..CleaningConfig::default()
        };
        let after = build(&after_config);

        assert_eq!(before.overview().total_tokens, 2);
        assert_eq!(after.overview().total_tokens, 1);
    }

    #[test]
    fn inline_repeated_artifact_rule_is_absent_from_frequency_after_rebuild() {
        let records = vec![record("inline-frequency.txt")];
        let cleaning_config = CleaningConfig {
            normalize_whitespace_line_endings: true,
            removal_rules: vec![anywhere_literal_rule("<br />")],
            ..CleaningConfig::default()
        };
        let mut options = build_options();
        options.processing_config_signature = cleaning_config_signature(&cleaning_config);
        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(resolve_analysis_input(
                    record,
                    &cleaning_config,
                    |_, config| Ok(crate::clean::clean_text("alpha<br /><br />beta", config)),
                    |_, _| unreachable!("processing succeeds"),
                ))
            },
            |_| {},
        )
        .unwrap();

        assert_eq!(corpus.overview().total_tokens, 2);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                selection: AnalysisDocumentSelection::default(),
                query: "br".to_string(),
                match_mode: FrequencyMatchMode::Contains,
                minimum_frequency: 1,
                token_class: FrequencyTokenClass::All,
                hide_stopwords: false,
                group_by: FrequencyGroupMode::Surface,
                case_fold: false,
                language: None,
                pos_filter: None,
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Desc,
                offset: 0,
                limit: 10,
            },
        );

        assert_eq!(page.total_terms, 0);
    }

    #[test]
    fn cleaning_signature_marks_cached_analysis_stale() {
        let corpus = corpus_for(&[("stale.txt", "current text")]);
        let changed = CleaningConfig {
            remove_patterns: vec!["current".to_string()],
            ..CleaningConfig::default()
        };

        assert!(!corpus.is_stale_for_cleaning_config(&CleaningConfig::default()));
        assert!(corpus.is_stale_for_cleaning_config(&changed));
    }

    #[test]
    fn builds_overview_and_sparse_frequency_index() {
        let corpus = corpus_for(&[("one.txt", "Cats cats dog"), ("two.txt", "dog bird")]);
        let overview = corpus.overview();

        assert_eq!(overview.document_count, 2);
        assert_eq!(overview.total_tokens, 5);
        assert_eq!(overview.total_types, 3);
        assert_eq!(overview.lexical_spectrum.hapax_count, 1);
        assert_eq!(overview.lexical_spectrum.dis_legomena_count, 2);

        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                selection: AnalysisDocumentSelection::default(),
                query: String::new(),
                match_mode: FrequencyMatchMode::Contains,
                minimum_frequency: 1,
                token_class: FrequencyTokenClass::All,
                hide_stopwords: false,
                group_by: FrequencyGroupMode::Surface,
                case_fold: false,
                language: None,
                pos_filter: None,
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Desc,
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(page.total_terms, 4);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].token, "dog");
        assert_eq!(page.items[0].frequency, 2);
        assert_eq!(page.items[1].token, "Cats");
    }

    #[test]
    fn frequency_query_pages_and_scopes_without_materialising_frontend_rows() {
        let corpus = corpus_for(&[("one.txt", "alpha beta"), ("two.txt", "beta gamma")]);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                selection: AnalysisDocumentSelection {
                    all_documents: false,
                    ids: vec![1],
                    excluded_ids: Vec::new(),
                },
                query: String::new(),
                match_mode: FrequencyMatchMode::Contains,
                minimum_frequency: 1,
                token_class: FrequencyTokenClass::All,
                hide_stopwords: false,
                group_by: FrequencyGroupMode::Surface,
                case_fold: false,
                language: None,
                pos_filter: None,
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                offset: 1,
                limit: 1,
            },
        );

        assert_eq!(page.selected_document_count, 1);
        assert_eq!(page.scoped_total_tokens, 2);
        assert_eq!(page.total_terms, 2);
        assert_eq!(page.items[0].token, "gamma");
    }

    #[test]
    fn frequency_query_window_bounds() {
        let corpus = corpus_for(&[("one.txt", "alpha beta gamma delta epsilon")]);
        let first = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                limit: 2,
                ..base_frequency_query()
            },
        );
        let second = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                offset: 2,
                limit: 2,
                ..base_frequency_query()
            },
        );

        assert_eq!(first.total_terms, 5);
        assert_eq!(second.total_terms, 5);
        assert_eq!(first.offset, 0);
        assert_eq!(first.items.len(), 2);
        assert_eq!(second.offset, 2);
        assert_eq!(second.items.len(), 2);
        assert_eq!(frequency_tokens(&first), [("alpha", 1), ("beta", 1)]);
        assert_eq!(frequency_tokens(&second), [("delta", 1), ("epsilon", 1)]);
    }

    #[test]
    fn frequency_group_by_stem_en() {
        let corpus = corpus_for(&[(
            "stem.txt",
            "running runs connected connecting connects unrelated",
        )]);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                group_by: FrequencyGroupMode::Stem,
                language: Some("en".to_string()),
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Desc,
                ..base_frequency_query()
            },
        );

        assert!(page.notices.is_empty());
        assert_eq!(page.items[0].token, "connect");
        assert_eq!(page.items[0].frequency, 3);
        assert_eq!(page.items[1].token, "run");
        assert_eq!(page.items[1].frequency, 2);
    }

    #[test]
    fn stemming_unsupported_language_emits_notice() {
        let corpus = corpus_for(&[("stem.txt", "Running running")]);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                group_by: FrequencyGroupMode::Stem,
                language: Some("zz".to_string()),
                ..base_frequency_query()
            },
        );

        assert_eq!(page.notices.len(), 1);
        assert_eq!(
            page.notices[0].kind,
            FrequencyNoticeKind::StemmingUnsupportedLanguage
        );
        assert!(page.notices[0].message.contains("zz"));
        assert_eq!(frequency_tokens(&page), [("Running", 1), ("running", 1)]);
    }

    #[test]
    fn frequency_group_by_lemma_uses_annotation() {
        let mut corpus = corpus_for(&[("lemma.txt", "Cats cat ran running")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("cat", "cat", UniversalPos::Noun, 1),
                ("ran", "run", UniversalPos::Verb, 1),
                ("running", "run", UniversalPos::Verb, 1),
            ],
        );

        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                group_by: FrequencyGroupMode::Lemma,
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                ..base_frequency_query()
            },
        );

        assert!(page.notices.is_empty());
        assert_eq!(frequency_tokens(&page), [("cat", 2), ("run", 2)]);
    }

    #[test]
    fn frequency_pos_filter_uses_annotation() {
        let mut corpus = corpus_for(&[("pos.txt", "Cats cat ran running")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("cat", "cat", UniversalPos::Noun, 1),
                ("ran", "run", UniversalPos::Verb, 1),
                ("running", "run", UniversalPos::Verb, 1),
            ],
        );

        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                pos_filter: Some("NOUN".to_string()),
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                ..base_frequency_query()
            },
        );

        assert!(page.notices.is_empty());
        assert_eq!(frequency_tokens(&page), [("Cats", 1), ("cat", 1)]);
    }

    #[test]
    fn dp_is_zero_for_perfectly_even_distribution() {
        let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
        let stats = frequency_distribution_stats(&[(0, 1), (1, 1)], 2, &corpus.documents, 4);

        assert_close(stats.dp, 0.0);
    }

    #[test]
    fn dp_increases_with_concentration() {
        let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
        let even = frequency_distribution_stats(&[(0, 1), (1, 1)], 2, &corpus.documents, 4);
        let concentrated = frequency_distribution_stats(&[(0, 2)], 2, &corpus.documents, 4);

        assert!(concentrated.dp > even.dp);
    }

    #[test]
    fn dp_uses_document_sizes_not_equal_parts() {
        let corpus = corpus_for(&[("short.txt", "alpha"), ("z_long.txt", "alpha beta gamma")]);
        // doc sizes 1:3. Counts proportional to sizes => DP = 0 ONLY under size weighting
        // (under equal-parts weighting this would be 0.25).
        let even = frequency_distribution_stats(&[(0, 1), (1, 3)], 4, &corpus.documents, 4);
        assert_close(even.dp, 0.0);
        // Counts inverse to sizes => maximally skewed for this 2-part split.
        let concentrated = frequency_distribution_stats(&[(0, 3), (1, 1)], 4, &corpus.documents, 4);
        assert_close(concentrated.dp, 0.5);
    }

    #[test]
    fn top_document_share_reports_max_single_doc_proportion() {
        let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
        let stats = frequency_distribution_stats(&[(0, 2), (1, 6)], 8, &corpus.documents, 4);

        assert_close(stats.top_doc_share, 0.75);
    }

    #[test]
    fn normalised_frequency_per_10k_is_correct() {
        let corpus = corpus_for(&[("a.txt", "alpha beta gamma"), ("b.txt", "delta epsilon")]);
        let stats = frequency_distribution_stats(&[(0, 3), (1, 2)], 5, &corpus.documents, 10);

        assert_close(stats.per_10k, 5_000.0);
    }

    #[test]
    fn document_frequency_counts_documents_with_at_least_one_hit() {
        let corpus = corpus_for(&[("a.txt", "alpha"), ("b.txt", "beta"), ("c.txt", "gamma")]);
        let stats =
            frequency_distribution_stats(&[(0, 2), (1, 0), (2, 3)], 5, &corpus.documents, 3);

        assert_eq!(stats.doc_freq, 2);
    }

    #[test]
    fn pos_distribution_lists_all_present_upos_tags() {
        let mut corpus = corpus_for(&[("a.txt", "Cats run quickly"), ("b.txt", "Cats run")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
                ("quickly", "quickly", UniversalPos::Adv, 1),
            ],
        );
        add_document_annotations(
            &mut corpus,
            1,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &annotated_frequency_query(AnnotatedFrequencyMode::Pos),
            ready_annotation_cache(),
        );
        let rows = result
            .pos_rows
            .iter()
            .map(|row| (row.upos.as_str(), row.raw, row.doc_freq, row.doc_total))
            .collect::<Vec<_>>();

        assert_eq!(
            rows,
            vec![("NOUN", 2, 2, 2), ("VERB", 2, 2, 2), ("ADV", 1, 1, 2),]
        );
    }

    #[test]
    fn lemma_pos_frequency_groups_by_lemma_and_upos() {
        let mut corpus = corpus_for(&[("a.txt", "run runs"), ("b.txt", "run running")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("run", "run", UniversalPos::Verb, 1),
                ("runs", "run", UniversalPos::Verb, 1),
            ],
        );
        add_document_annotations(
            &mut corpus,
            1,
            &[
                ("run", "run", UniversalPos::Noun, 1),
                ("running", "run", UniversalPos::Verb, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos),
            ready_annotation_cache(),
        );
        let rows = result
            .lex_rows
            .iter()
            .map(|row| (row.lemma.as_str(), row.upos.as_str(), row.raw, row.doc_freq))
            .collect::<Vec<_>>();

        assert_eq!(rows, vec![("run", "VERB", 3, 2), ("run", "NOUN", 1, 1)]);
    }

    #[test]
    fn annotated_frequency_requires_ready_annotation_layer() {
        let mut corpus = corpus_for(&[("pos.txt", "Cats run")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &annotated_frequency_query(AnnotatedFrequencyMode::Pos),
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Missing,
                detail: None,
            },
        );

        assert!(result.pos_rows.is_empty());
        assert_eq!(
            result.unavailable_message.as_deref(),
            Some(ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE)
        );
    }

    #[test]
    fn single_document_corpus_has_zero_dp_and_full_top_doc_share() {
        let mut corpus = corpus_for(&[("one.txt", "Cats run")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &annotated_frequency_query(AnnotatedFrequencyMode::Pos),
            ready_annotation_cache(),
        );

        assert!(result.pos_rows.iter().all(|row| row.dp == 0.0));
        assert!(result.pos_rows.iter().all(|row| row.top_doc_share == 1.0));
    }

    #[test]
    fn frequency_lemma_requires_annotation_notice() {
        let corpus = corpus_for(&[("lemma.txt", "Cats cat")]);
        assert!(
            corpus
                .documents
                .iter()
                .all(|document| !document.annotation_available)
        );
        assert!(!corpus.overview().annotation_available);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                group_by: FrequencyGroupMode::Lemma,
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                ..base_frequency_query()
            },
        );

        assert_eq!(page.notices.len(), 1);
        assert_eq!(
            page.notices[0].kind,
            FrequencyNoticeKind::AnnotationUnavailable
        );
        assert_eq!(frequency_tokens(&page), [("Cats", 1), ("cat", 1)]);
    }

    #[test]
    fn frequency_pos_filter_requires_annotation_notice() {
        let corpus = corpus_for(&[("pos.txt", "Cats ran")]);
        assert!(
            corpus
                .documents
                .iter()
                .all(|document| !document.annotation_available)
        );
        assert!(!corpus.overview().annotation_available);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                pos_filter: Some("NOUN".to_string()),
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                ..base_frequency_query()
            },
        );

        assert_eq!(page.notices.len(), 1);
        assert_eq!(
            page.notices[0].kind,
            FrequencyNoticeKind::PosFilterUnavailable
        );
        assert_eq!(frequency_tokens(&page), [("Cats", 1), ("ran", 1)]);
    }

    #[test]
    fn frequency_min_count_and_casefold() {
        let corpus = corpus_for(&[("case.txt", "Alpha alpha ALPHA beta Beta gamma")]);
        let folded = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                minimum_frequency: 2,
                case_fold: true,
                ..base_frequency_query()
            },
        );
        let surface = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                minimum_frequency: 2,
                case_fold: false,
                ..base_frequency_query()
            },
        );

        assert_eq!(frequency_tokens(&folded), [("alpha", 3), ("beta", 2)]);
        assert_eq!(surface.total_terms, 0);
        assert_eq!(surface.scoped_total_types, 6);
    }

    #[test]
    fn document_and_distribution_queries_are_bounded() {
        let corpus = corpus_for(&[("one.txt", "alpha"), ("two.txt", "alpha beta")]);
        let documents =
            query_analysis_documents(&corpus, &AnalysisDocumentSelection::default(), "txt", 1, 1);
        assert_eq!(documents.total_count, 2);
        assert_eq!(documents.items.len(), 1);
        assert_eq!(documents.items[0].relative_path, "two.txt");

        let distribution = query_term_distribution(
            &corpus,
            "alpha",
            &TermDistributionQuery {
                selection: AnalysisDocumentSelection::default(),
                group_by: FrequencyGroupMode::Surface,
                case_fold: false,
                language: None,
                offset: 0,
                limit: 1,
            },
        );
        assert_eq!(distribution.total_documents, 2);
        assert_eq!(distribution.items.len(), 1);
        assert_eq!(distribution.items[0].count, 1);
    }

    #[test]
    fn empty_documents_are_skipped_and_single_document_dispersion_is_safe() {
        let corpus = corpus_for(&[("empty.txt", "  \n"), ("one.txt", "word word")]);
        assert_eq!(corpus.overview().document_count, 1);
        assert_eq!(corpus.overview().skipped_documents, 1);
        assert_eq!(corpus.overview().dispersion.median_dp, None);
    }

    #[test]
    fn balance_statistics_match_known_distributions() {
        assert_eq!(gini_coefficient(&[1, 1]), Some(0.0));
        assert_eq!(gini_coefficient(&[0, 10]), Some(0.5));
        assert_eq!(gini_coefficient(&[0, 0]), Some(0.0));

        assert_eq!(coefficient_of_variation(&[1, 1]), Some(0.0));
        assert_eq!(coefficient_of_variation(&[0, 2]), Some(1.0));
        assert_eq!(coefficient_of_variation(&[0, 0]), None);

        assert_eq!(effective_document_count(&[1, 1], 2), Some(2.0));
        assert_eq!(effective_document_count(&[0, 2], 2), Some(1.0));
        assert_eq!(effective_document_count(&[], 0), None);
    }

    #[test]
    fn mattr_averages_known_window_ttrs() {
        let tokens = ["a", "b", "a", "c"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        let value = mattr(&tokens, 3).unwrap();
        assert!((value - (5.0 / 6.0)).abs() < 1e-12);
        assert_eq!(mattr(&tokens, 5), None);
    }

    #[test]
    fn mtld_averages_forward_and_backward_runs() {
        let tokens = ["a", "b", "a", "c", "a", "b", "d", "a", "b", "c"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        let value = mtld(&tokens, 0.72).unwrap();
        assert!((value - (545.0 / 106.0)).abs() < 1e-12);
    }

    #[test]
    fn lexical_diversity_excludes_short_documents() {
        let short = std::iter::repeat_n("short", 99)
            .collect::<Vec<_>>()
            .join(" ");
        let eligible = std::iter::repeat_n("eligible", 100)
            .collect::<Vec<_>>()
            .join(" ");
        let corpus = corpus_for(&[
            ("short.txt", short.as_str()),
            ("eligible.txt", eligible.as_str()),
        ]);
        let lexical = &corpus.overview().lexical_spectrum;

        assert_eq!(lexical.mattr_docs_used, 1);
        assert_eq!(lexical.mattr_docs_excluded, 1);
        assert_eq!(lexical.mtld_docs_used, 1);
        assert_eq!(lexical.mtld_docs_excluded, 1);
        assert_eq!(lexical.mattr_median, Some(0.01));
        assert!(lexical.mtld_median.is_some());
    }

    #[test]
    fn entropy_metrics_match_uniform_and_effective_vocabularies() {
        let (entropy, evenness, effective_vocabulary) = entropy_metrics(&[1, 1, 1, 1], 4);
        assert_eq!(entropy, Some(2.0));
        assert_eq!(evenness, Some(1.0));
        assert_eq!(effective_vocabulary, Some(4.0));

        let (entropy, _, effective_vocabulary) = entropy_metrics(&[3, 1], 4);
        let entropy = entropy.unwrap();
        assert!((effective_vocabulary.unwrap() - 2.0f64.powf(entropy)).abs() < 1e-12);
    }

    #[test]
    fn entropy_metrics_omit_single_type_and_empty_distributions() {
        assert_eq!(entropy_metrics(&[10], 10), (None, None, None));
        assert_eq!(entropy_metrics(&[], 0), (None, None, None));
    }

    #[test]
    fn frequency_spectrum_counts_exact_frequency_classes() {
        let (points, truncated) = frequency_spectrum(&[5, 2, 2, 1]);
        assert!(!truncated);
        assert_eq!(
            points,
            vec![
                FrequencySpectrumPoint {
                    frequency: 1,
                    type_count: 1,
                },
                FrequencySpectrumPoint {
                    frequency: 2,
                    type_count: 2,
                },
                FrequencySpectrumPoint {
                    frequency: 5,
                    type_count: 1,
                },
            ]
        );
    }

    #[test]
    fn zipf_fit_recovers_known_synthetic_exponent() {
        let frequencies = (1..=20u64)
            .map(|rank| 1_000_000 / rank.pow(2))
            .collect::<Vec<_>>();
        let summary = rank_frequency_summary(&frequencies, frequencies.iter().sum());
        assert!((summary.zipf_exponent.unwrap() - 2.0).abs() < 0.02);
        assert!(summary.r_squared.unwrap() > 0.999);
    }

    #[test]
    fn heaps_fit_recovers_known_synthetic_shape() {
        let points = vec![
            VocabularyGrowthPoint {
                tokens: 100,
                types: 100,
            },
            VocabularyGrowthPoint {
                tokens: 400,
                types: 200,
            },
            VocabularyGrowthPoint {
                tokens: 900,
                types: 300,
            },
            VocabularyGrowthPoint {
                tokens: 1_600,
                types: 400,
            },
        ];
        let summary = vocabulary_growth_summary(&points);
        assert!((summary.heaps_k.unwrap() - 10.0).abs() < 1e-10);
        assert!((summary.heaps_beta.unwrap() - 0.5).abs() < 1e-12);
        assert!((summary.r_squared.unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn frequency_rows_report_raw_and_per_million_counts() {
        let corpus = corpus_for(&[("one.txt", "alpha alpha beta")]);
        let page = query_frequency_list(
            &corpus,
            &FrequencyQuery {
                selection: AnalysisDocumentSelection::default(),
                query: "alpha".to_string(),
                match_mode: FrequencyMatchMode::Exact,
                minimum_frequency: 1,
                token_class: FrequencyTokenClass::All,
                hide_stopwords: false,
                group_by: FrequencyGroupMode::Surface,
                case_fold: false,
                language: None,
                pos_filter: None,
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Desc,
                offset: 0,
                limit: 10,
            },
        );
        assert_eq!(page.items[0].frequency, 2);
        assert!((page.items[0].rate_per_million - 666_666.666_666_666_6).abs() < 1e-6);
    }

    #[test]
    fn shuffled_source_order_produces_identical_analysis() {
        let texts = HashMap::from([
            ("a.txt", "alpha alpha beta"),
            ("b.txt", "beta gamma"),
            ("c.txt", "gamma delta"),
        ]);
        let build = |names: &[&str]| {
            let records = names.iter().map(|name| record(name)).collect::<Vec<_>>();
            build_analysis_corpus(
                &records,
                build_options(),
                &AtomicBool::new(false),
                |record| {
                    Ok(processed_resolution(
                        record,
                        texts[record.source_path.to_str().unwrap()],
                    ))
                },
                |_| {},
            )
            .unwrap()
        };
        let forward = build(&["a.txt", "b.txt", "c.txt"]);
        let shuffled = build(&["c.txt", "a.txt", "b.txt"]);
        assert_eq!(forward.overview(), shuffled.overview());
        assert_eq!(
            query_frequency_list(&forward, &base_frequency_query()),
            query_frequency_list(&shuffled, &base_frequency_query())
        );
    }

    #[test]
    fn lexical_bands_partition_types_and_match_frequent_types() {
        let corpus = corpus_for(&[("bands.txt", "a b b c c c d d d d d d d d d d")]);
        let overview = corpus.overview();
        let lexical = &overview.lexical_spectrum;

        assert_eq!(lexical.hapax_count, 1);
        assert_eq!(lexical.dis_legomena_count, 1);
        assert_eq!(lexical.band_3_to_9_count, 1);
        assert_eq!(lexical.band_10plus_count, 1);
        assert_eq!(
            lexical.hapax_count
                + lexical.dis_legomena_count
                + lexical.band_3_to_9_count
                + lexical.band_10plus_count,
            lexical.types
        );
        assert_eq!(
            lexical.band_10plus_count,
            overview.dispersion.frequent_types
        );
    }

    #[test]
    fn frequency_coverage_requires_the_full_named_threshold() {
        let text = (0..100)
            .map(|index| format!("term{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let records = vec![record("coverage.txt")];
        let corpus = build_analysis_corpus(
            &records,
            build_options(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, &text)),
            |_| {},
        )
        .unwrap();

        assert_eq!(
            corpus.overview().frequency_concentration.top_100_coverage,
            Some(1.0)
        );
        assert_eq!(
            corpus.overview().frequency_concentration.top_1000_coverage,
            None
        );

        let small = corpus_for(&[("small.txt", "alpha beta gamma")]);
        assert_eq!(
            small.overview().frequency_concentration.top_100_coverage,
            None
        );
    }

    #[test]
    fn document_frequency_spectrum_and_median_are_computed_over_all_types() {
        let corpus = corpus_for(&[
            ("one.txt", "alpha shared common"),
            ("two.txt", "beta shared common"),
            ("three.txt", "gamma common"),
        ]);
        let dispersion = &corpus.overview().dispersion;

        assert_eq!(dispersion.df_in_1_count, 3);
        assert_eq!(dispersion.df_in_2_to_n_minus_1_count, 1);
        assert_eq!(dispersion.df_in_all_count, 1);
        assert_eq!(dispersion.median_documents_per_type, Some(1.0));
        assert_eq!(dispersion.median_dp, None);
    }

    #[test]
    fn severity_thresholds_use_the_documented_boundaries() {
        use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};

        assert_eq!(balance_severity(10, 100, Some(0.149), Some(10.0)), NoIssue);
        assert_eq!(
            balance_severity(10, 100, Some(0.15), Some(10.0)),
            WorthALook
        );
        assert_eq!(
            balance_severity(10, 100, Some(0.25), Some(10.0)),
            WorthALook
        );
        assert_eq!(
            balance_severity(10, 100, Some(0.251), Some(10.0)),
            NeedsFixing
        );
        assert_eq!(balance_severity(10, 100, Some(0.10), Some(6.9)), WorthALook);

        assert_eq!(extraction_severity(100, Some(0.049)), NoIssue);
        assert_eq!(extraction_severity(100, Some(0.05)), WorthALook);
        assert_eq!(extraction_severity(100, Some(0.20)), WorthALook);
        assert_eq!(extraction_severity(100, Some(0.201)), NeedsFixing);

        assert_eq!(dispersion_severity(3, 10, Some(0.299)), NoIssue);
        assert_eq!(dispersion_severity(3, 10, Some(0.30)), WorthALook);
        assert_eq!(dispersion_severity(3, 10, Some(0.60)), WorthALook);
        assert_eq!(dispersion_severity(3, 10, Some(0.601)), NeedsFixing);
        assert_eq!(dispersion_severity(3, 0, None), NoIssue);
    }

    #[test]
    fn empty_corpus_overview_has_no_non_finite_values() {
        let corpus = build_analysis_corpus(
            &[],
            build_options(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "")),
            |_| {},
        )
        .unwrap();
        let overview = corpus.overview();

        assert_eq!(overview.totals.tokens, 0);
        assert_eq!(overview.totals.types, 0);
        assert_eq!(overview.totals.documents, 0);
        assert_eq!(overview.balance.gini, None);
        assert_eq!(overview.balance.coefficient_of_variation, None);
        assert_eq!(overview.balance.effective_documents, None);
        assert_eq!(overview.lexical_spectrum.mattr_median, None);
        assert_eq!(overview.lexical_spectrum.mtld_median, None);
        assert_eq!(overview.frequency_concentration.shannon_entropy_bits, None);
        assert_eq!(overview.frequency_concentration.evenness_j, None);
        assert_eq!(overview.frequency_concentration.perplexity, None);
        assert_eq!(
            overview
                .frequency_concentration
                .effective_vocabulary_entropy,
            None
        );
        let json = serde_json::to_string(overview).unwrap();
        assert!(!json.contains("rawTtr"));
        assert!(!json.contains("NaN"));
        assert!(!json.contains("Infinity"));
    }

    #[test]
    fn builds_many_tiny_documents_without_a_large_fixture() {
        let records = (0..1_000)
            .map(|index| record(&format!("{index}.txt")))
            .collect::<Vec<_>>();
        let corpus = build_analysis_corpus(
            &records,
            build_options(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "shared unique")),
            |_| {},
        )
        .unwrap();

        assert_eq!(corpus.overview().document_count, 1_000);
        assert_eq!(corpus.overview().total_tokens, 2_000);
        assert_eq!(corpus.overview().total_types, 2);
        assert!(corpus.overview().documents_truncated);
        assert_eq!(corpus.overview().documents.len(), OVERVIEW_DOCUMENT_LIMIT);
    }

    #[test]
    fn build_can_be_cancelled_between_documents() {
        let records = vec![record("one.txt"), record("two.txt")];
        let cancel = AtomicBool::new(false);
        let result = build_analysis_corpus(
            &records,
            build_options(),
            &cancel,
            |record| Ok(processed_resolution(record, "one two")),
            |progress| {
                if progress.files_done == 1 {
                    cancel.store(true, AtomicOrdering::Relaxed);
                }
            },
        );

        assert_eq!(result.unwrap_err(), AnalysisBuildError::Cancelled);
    }

    #[test]
    fn none_path_skips_processed_hash() {
        let product = analysis_product_for(
            &[("hash.txt", "The quick brown fox.")],
            build_options(),
            false,
        );

        assert_eq!(product.corpus.documents[0].processed_hash_sha256, "");
    }

    #[test]
    fn some_path_hash_unchanged() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let product = analysis_product_for(&[("hash.txt", "The quick brown fox.")], options, false);

        assert_eq!(
            product.corpus.documents[0].processed_hash_sha256,
            crate::hash::sha256_hex("The quick brown fox.".as_bytes())
        );
    }

    #[test]
    fn some_path_cache_still_ready() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let text = "The quick brown fox.";
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let mut corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, text)),
            |_| {},
        )
        .unwrap();
        let surface_term_id = corpus
            .surface_terms
            .iter()
            .position(|term| term.token == "The")
            .unwrap() as u32;
        let lemma_id = corpus.lemma_table.intern("the");
        let layer = AnnotationLayer {
            documents: vec![AnnotationLayerDocument {
                analysis_document_index: 0,
                source_path: "doc.txt".to_string(),
                relative_path: "doc.txt".to_string(),
                term_counts: vec![AnnotatedTermCount {
                    surface_term_id,
                    lemma_id,
                    upos: UniversalPos::Det,
                    count: 1,
                }],
            }],
            lemma_table: corpus.lemma_table.clone(),
            languages_used: Vec::new(),
        };
        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        let cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, text)),
            |_| {},
        )
        .unwrap();

        assert!(cached.overview().annotation_available);
        assert_eq!(
            cached.documents[0].annotated_term_counts,
            layer.documents[0].term_counts
        );
    }

    #[test]
    fn manifest_builder_rejects_too_short_document_hash_input() {
        let corpus = corpus_for(&[("doc1.txt", "hello")]);
        let result = crate::annotation_cache::build_annotation_manifest(
            &corpus,
            &[],
            crate::tokenization::TokenizationManifest {
                id: "".into(),
                version: String::new(),
                unicode_segmentation_version: "".into(),
                config: crate::tokenization::TokenizationConfig::default(),
            },
            &std::collections::HashMap::new(),
            vec![],
            &[], // too few
            &["en".to_string()],
        );
        assert!(matches!(
            result,
            Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
        ));
    }

    #[test]
    fn manifest_builder_rejects_too_long_document_hash_input() {
        let corpus = corpus_for(&[]); // count = 0
        let result = crate::annotation_cache::build_annotation_manifest(
            &corpus,
            &[],
            crate::tokenization::TokenizationManifest {
                id: "".into(),
                version: String::new(),
                unicode_segmentation_version: "".into(),
                config: crate::tokenization::TokenizationConfig::default(),
            },
            &std::collections::HashMap::new(),
            vec![],
            &["hash1".to_string()], // too many
            &[],
        );
        assert!(matches!(
            result,
            Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
        ));
    }

    #[test]
    fn manifest_builder_rejects_too_short_document_language_input() {
        let corpus = corpus_for(&[("doc1.txt", "hello")]);
        let result = crate::annotation_cache::build_annotation_manifest(
            &corpus,
            &[],
            crate::tokenization::TokenizationManifest {
                id: "".into(),
                version: String::new(),
                unicode_segmentation_version: "".into(),
                config: crate::tokenization::TokenizationConfig::default(),
            },
            &std::collections::HashMap::new(),
            vec![],
            &["hash1".to_string()],
            &[], // too few
        );
        assert!(matches!(
            result,
            Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
        ));
    }

    #[test]
    fn manifest_builder_rejects_too_long_document_language_input() {
        let corpus = corpus_for(&[]); // count = 0
        let result = crate::annotation_cache::build_annotation_manifest(
            &corpus,
            &[],
            crate::tokenization::TokenizationManifest {
                id: "".into(),
                version: String::new(),
                unicode_segmentation_version: "".into(),
                config: crate::tokenization::TokenizationConfig::default(),
            },
            &std::collections::HashMap::new(),
            vec![],
            &[],
            &["en".to_string()], // too many
        );
        assert!(matches!(
            result,
            Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
        ));
    }

    #[test]
    fn test_annotation_cache_seam_none_root() {
        let corpus = corpus_for(&[("doc.txt", "The quick brown fox jumps over the lazy dog.")]);
        assert!(!corpus.overview().annotation_available);
        assert!(corpus.documents.iter().all(|doc| !doc.annotation_available));
        assert!(
            corpus
                .documents
                .iter()
                .all(|doc| doc.annotated_term_counts.is_empty())
        );
    }

    #[test]
    fn test_annotation_cache_seam_missing_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus.overview().annotation_available);
        assert!(corpus.documents.iter().all(|doc| !doc.annotation_available));
    }

    #[test]
    fn test_annotation_cache_seam_stale_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        // 1. Build a corpus to generate expected manifest
        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        let mut manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        // Make it stale
        manifest.schema_version += 1;

        let layer = AnnotationLayer {
            documents: vec![],
            lemma_table: LemmaTable::new(),
            languages_used: vec![],
        };

        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        // 2. Build with cache root
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus2 = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus2.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_invalid_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        let final_path = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        std::fs::write(&final_path, b"{ corrupt json").unwrap();

        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());

        let corpus = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_valid_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        // 1. Build unannotated corpus
        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        options.force_compute_hashes = true;
        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        // 2. Call build_annotation_layer to create a valid layer
        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };
        let layer = build_annotation_layer(
            &corpus,
            &records,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();

        // 3. Build manifest and write to cache
        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        // 4. Run fresh analysis build with cache root
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus_cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        // 5. Assertions
        assert!(corpus_cached.overview().annotation_available);
        assert!(
            corpus_cached
                .documents
                .iter()
                .all(|doc| doc.annotation_available)
        );
        assert!(!corpus_cached.documents[0].annotated_term_counts.is_empty());
        assert_eq!(corpus_cached.lemma_table, layer.lemma_table);

        // Verify query lights up
        let mut query = base_frequency_query();
        query.group_by = FrequencyGroupMode::Lemma;
        let page = query_frequency_list(&corpus_cached, &query);
        assert!(!page.items.is_empty());
        // Verify no AnnotationUnavailable notices
        assert!(
            page.notices
                .iter()
                .all(|n| n.kind != FrequencyNoticeKind::AnnotationUnavailable)
        );
    }

    #[test]
    fn test_annotation_cache_seam_validation_reject_bad_layer_docs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc1.txt"), record("doc2.txt")];

        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };
        let mut layer = build_annotation_layer(
            &corpus,
            &records,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();

        // Mess up the documents in layer: make mismatched path/source
        layer.documents[1].analysis_document_index = 1;
        layer.documents[1].source_path = "mismatched".to_string();

        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();

        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus2 = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus2.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_validation_reject_bad_ids() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };
        let mut layer = build_annotation_layer(
            &corpus,
            &records,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();

        // Mess up term/lemma IDs: make surface_term_id out of range
        if !layer.documents[0].term_counts.is_empty() {
            layer.documents[0].term_counts[0].surface_term_id = 99999;
        }

        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus2 = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick brown fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus2.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_tokenisation_threading() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        let custom_config = TokenizationConfig {
            retain_ascii_hyphen_compounds: false,
            ..Default::default()
        };

        let mut options_custom = build_options();
        options_custom.tokenization = custom_config;
        options_custom.udpipe_model_dir = Some(default_model_dir());

        let corpus = build_analysis_corpus(
            &records,
            options_custom.clone(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick-brown-fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        // Expect custom tokenization config to flow into manifest
        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options_custom.udpipe_model_dir.as_deref(),
            &options_custom.language_overrides,
        )
        .unwrap();
        assert!(!manifest.tokenization.config.retain_ascii_hyphen_compounds);

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options_custom.udpipe_model_dir.clone(),
            language_overrides: options_custom.language_overrides.clone(),
        };
        let layer = build_annotation_layer(
            &corpus,
            &records,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick-brown-fox jumps over the lazy dog.",
                ))
            },
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();

        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        // Build with default tokenization config (should read Stale)
        let mut options_default = build_options();
        options_default.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options_default.udpipe_model_dir = Some(default_model_dir());

        let corpus_default = build_analysis_corpus(
            &records,
            options_default,
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    "The quick-brown-fox jumps over the lazy dog.",
                ))
            },
            |_| {},
        )
        .unwrap();

        assert!(!corpus_default.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_mixed_en_pt() {
        let temp_dir = tempfile::tempdir().unwrap();
        // doc1 is Portuguese, doc2 is English
        let records = vec![record("pt.txt"), record("en.txt")];
        let mut overrides = HashMap::new();
        overrides.insert(0, "pt".to_string());
        overrides.insert(1, "en".to_string());

        let mut options = build_options();
        options.language_overrides = overrides;
        options.udpipe_model_dir = Some(default_model_dir());
        options.force_compute_hashes = true;

        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("pt") {
                    Ok(processed_resolution(record, "O gato preto correu."))
                } else {
                    Ok(processed_resolution(record, "The quick brown fox jumps."))
                }
            },
            |_| {},
        )
        .unwrap();

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };
        let layer = build_annotation_layer(
            &corpus,
            &records,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("pt") {
                    Ok(processed_resolution(record, "O gato preto correu."))
                } else {
                    Ok(processed_resolution(record, "The quick brown fox jumps."))
                }
            },
        &std::sync::atomic::AtomicUsize::new(0))
        .unwrap();

        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        // Assert English and Portuguese models both present in manifest
        assert!(manifest.models.iter().any(|m| m.language == "pt"));
        assert!(manifest.models.iter().any(|m| m.language == "en"));

        crate::annotation_cache::write_annotation_layer_cache(temp_dir.path(), &manifest, &layer)
            .unwrap();

        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus_cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("pt") {
                    Ok(processed_resolution(record, "O gato preto correu."))
                } else {
                    Ok(processed_resolution(record, "The quick brown fox jumps."))
                }
            },
            |_| {},
        )
        .unwrap();

        assert!(corpus_cached.overview().annotation_available);
    }

    #[test]
    fn test_annotation_cache_seam_unsupported_language_preflight() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut overrides = HashMap::new();
        overrides.insert(0, "unsupported_lang".to_string());

        let mut options = build_options();
        options.language_overrides = overrides;
        options.udpipe_model_dir = Some(default_model_dir());

        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Hello world.")),
            |_| {},
        )
        .unwrap();

        // Attempting to build expected manifest should fail (ValidationError because unsupported_lang model is not found)
        let manifest_res = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        );
        assert!(manifest_res.is_err());

        // Run analysis build with cache root (should skip attach cleanly, leaving build successful)
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let corpus_res = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Hello world.")),
            |_| {},
        );

        assert!(corpus_res.is_ok());
        let corpus_built = corpus_res.unwrap();
        assert!(!corpus_built.overview().annotation_available);
    }

    #[test]
    fn test_explicit_build_writes_ready_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let report = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        assert!(report.cache_written);
        assert!(!report.reused_existing);
        assert_eq!(report.documents_annotated, 1);
        assert!(report.total_annotated_term_count_rows > 0);

        // Assert we can read it and it's Ready
        let mut manifest_options = build_options();
        manifest_options.udpipe_model_dir = options.udpipe_model_dir.clone();
        manifest_options.force_compute_hashes = true;
        let corpus = build_analysis_corpus(
            &records,
            manifest_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();
        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();
        let load = crate::annotation_cache::load_annotation_layer_cache(temp_dir.path(), &manifest);
        assert!(matches!(
            load,
            crate::annotation_cache::AnnotationLayerCacheLoad::Ready(_)
        ));
    }

    #[test]
    fn test_fresh_analysis_attaches_after_explicit_build() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        let corpus_cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();

        assert!(corpus_cached.overview().annotation_available);
    }

    #[test]
    fn test_persisted_manifest_matches_reader() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        let bytes = std::fs::read(&cache_file).unwrap();
        let envelope: crate::annotation_cache::AnnotationLayerCacheEnvelope =
            serde_json::from_slice(&bytes).unwrap();

        let mut manifest_options = build_options();
        manifest_options.udpipe_model_dir = options.udpipe_model_dir.clone();
        manifest_options.force_compute_hashes = true;
        let corpus = build_analysis_corpus(
            &records,
            manifest_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();
        let manifest = build_expected_manifest(
            &corpus,
            &records,
            options.udpipe_model_dir.as_deref(),
            &options.language_overrides,
        )
        .unwrap();

        assert_eq!(envelope.manifest, manifest);
    }

    #[test]
    fn test_build_is_deterministic() {
        let temp_dir1 = tempfile::tempdir().unwrap();
        let temp_dir2 = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        let mut options1 = build_options();
        options1.annotation_cache_root = Some(temp_dir1.path().to_path_buf());
        options1.udpipe_model_dir = Some(default_model_dir());
        let build_ann_options1 = AnnotationLayerBuildOptions {
            udpipe_model_dir: options1.udpipe_model_dir.clone(),
            language_overrides: options1.language_overrides.clone(),
        };
        build_and_write_annotation_cache(
            &records,
            options1,
            build_ann_options1,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        let mut options2 = build_options();
        options2.annotation_cache_root = Some(temp_dir2.path().to_path_buf());
        options2.udpipe_model_dir = Some(default_model_dir());
        let build_ann_options2 = AnnotationLayerBuildOptions {
            udpipe_model_dir: options2.udpipe_model_dir.clone(),
            language_overrides: options2.language_overrides.clone(),
        };
        build_and_write_annotation_cache(
            &records,
            options2,
            build_ann_options2,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        let cache_file1 = temp_dir1.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        let cache_file2 = temp_dir2.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));

        let bytes1 = std::fs::read(&cache_file1).unwrap();
        let bytes2 = std::fs::read(&cache_file2).unwrap();

        // Use semantic envelope equality
        let envelope1: crate::annotation_cache::AnnotationLayerCacheEnvelope =
            serde_json::from_slice(&bytes1).unwrap();
        let envelope2: crate::annotation_cache::AnnotationLayerCacheEnvelope =
            serde_json::from_slice(&bytes2).unwrap();

        assert_eq!(envelope1, envelope2);
    }

    #[test]
    fn test_ready_cache_skips_rebuild() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let report1 = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        assert!(report1.cache_written);
        assert!(!report1.reused_existing);

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        let mtime1 = std::fs::metadata(&cache_file).unwrap().modified().unwrap();

        let report2 = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        assert!(!report2.cache_written);
        assert!(report2.reused_existing);
        assert_eq!(report1.documents_annotated, report2.documents_annotated);
        assert_eq!(
            report1.total_annotated_term_count_rows,
            report2.total_annotated_term_count_rows
        );

        let mtime2 = std::fs::metadata(&cache_file).unwrap().modified().unwrap();
        assert_eq!(mtime1, mtime2);
    }

    #[test]
    fn test_stale_or_invalid_replaced_after_full_build() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        std::fs::create_dir_all(temp_dir.path()).unwrap();
        std::fs::write(&cache_file, "INVALID JSON DATA").unwrap();

        let report = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {})
        .unwrap();

        assert!(report.cache_written);
        assert!(!report.reused_existing);

        let bytes = std::fs::read(&cache_file).unwrap();
        assert!(
            serde_json::from_slice::<crate::annotation_cache::AnnotationLayerCacheEnvelope>(&bytes)
                .is_ok()
        );
    }

    #[test]
    fn test_unsupported_language_writes_no_partial_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut overrides = HashMap::new();
        overrides.insert(0, "unsupported_lang".to_string());

        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());
        options.language_overrides = overrides;

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let res = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {});

        assert!(matches!(
            res,
            Err(AnnotationCacheBuildError::ManifestError { .. })
        ));

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        assert!(!cache_file.exists());
    }
    #[test]
    fn test_unsupported_language_does_not_overwrite_existing_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];

        // 1. Build a valid cache first
        let mut options_valid = build_options();
        options_valid.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options_valid.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options_valid = AnnotationLayerBuildOptions {
            udpipe_model_dir: options_valid.udpipe_model_dir.clone(),
            language_overrides: options_valid.language_overrides.clone(),
        };

        build_and_write_annotation_cache(
            &records,
            options_valid,
            build_ann_options_valid,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Valid content.")),
        |_| {})
        .unwrap();

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));

        // Save the valid cache content
        let original_cache_content = std::fs::read(&cache_file).unwrap();

        // 2. Attempt a build with unsupported language
        let mut overrides = HashMap::new();
        overrides.insert(0, "unsupported_lang".to_string());

        let mut options_invalid = build_options();
        options_invalid.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options_invalid.udpipe_model_dir = Some(default_model_dir());
        options_invalid.language_overrides = overrides;

        let build_ann_options_invalid = AnnotationLayerBuildOptions {
            udpipe_model_dir: options_invalid.udpipe_model_dir.clone(),
            language_overrides: options_invalid.language_overrides.clone(),
        };

        let res = build_and_write_annotation_cache(
            &records,
            options_invalid,
            build_ann_options_invalid,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {});

        assert!(matches!(
            res,
            Err(AnnotationCacheBuildError::ManifestError { .. })
        ));

        // 3. Verify the cache file is untouched
        let new_cache_content = std::fs::read(&cache_file).unwrap();
        assert_eq!(original_cache_content, new_cache_content);
    }

    #[test]
    fn test_cancelled_build_writes_no_final_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let cancel = AtomicBool::new(true);
        let res = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &cancel,
            |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {});

        assert!(matches!(res, Err(AnnotationCacheBuildError::Cancelled)));

        let cache_file = temp_dir.path().join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        assert!(!cache_file.exists());
    }

    #[test]
    fn test_mixed_en_pt_round_trips_explicit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("pt.txt"), record("en.txt")];
        let mut overrides = HashMap::new();
        overrides.insert(0, "pt".to_string());
        overrides.insert(1, "en".to_string());

        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());
        options.language_overrides = overrides;

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("pt") {
                    Ok(processed_resolution(record, "O gato preto correu."))
                } else {
                    Ok(processed_resolution(record, "The quick brown fox jumps."))
                }
            },
        |_| {})
        .unwrap();

        let corpus_cached = build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("pt") {
                    Ok(processed_resolution(record, "O gato preto correu."))
                } else {
                    Ok(processed_resolution(record, "The quick brown fox jumps."))
                }
            },
            |_| {},
        )
        .unwrap();

        assert!(corpus_cached.overview().annotation_available);
    }
}
