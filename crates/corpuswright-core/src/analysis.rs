//! Rust-owned corpus analysis and bounded query APIs.

use crate::clean::CleaningConfig;
use crate::scan::{DocumentRecord, DocumentType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use unicode_normalization::UnicodeNormalization;

const OVERVIEW_DOCUMENT_LIMIT: usize = 20;
const FREQUENT_TERM_THRESHOLD: u64 = 10;
const HIGH_DISPERSION_THRESHOLD: f64 = 0.5;
const MAX_QUERY_WINDOW: usize = 500;
// These lexical-diversity parameters are provisional and should remain visible in the UI.
const MATTR_WINDOW_TOKENS: usize = 100;
const MTLD_TTR_THRESHOLD: f64 = 0.720;
const MTLD_MIN_DOCUMENT_TOKENS: usize = 100;

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

static TOKEN_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\p{L}\p{N}]+(?:['’\-][\p{L}\p{N}]+)*")
        .expect("analysis token pattern must compile")
});

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
    pub raw_ttr: Option<f64>,
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
    pub evenness_j: Option<f64>,
    pub effective_vocabulary_entropy: Option<f64>,
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
}

/// Render-ready first-stage corpus EDA summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CorpusOverview {
    pub document_count: usize,
    pub source_count: usize,
    pub total_tokens: u64,
    pub total_types: usize,
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
    pub dispersion: CorpusOverviewDispersion,
    pub reading: CorpusOverviewReading,
    pub build: CorpusOverviewBuild,
}

#[derive(Debug, Clone)]
struct AnalysisDocument {
    summary: AnalysisDocumentSummary,
    term_counts: Vec<(usize, u64)>,
    mattr: Option<f64>,
    mtld: Option<f64>,
}

#[derive(Debug, Clone)]
struct AnalysisTerm {
    token: String,
    frequency: u64,
    document_frequency: usize,
    document_counts: Vec<(usize, u64)>,
    dispersion: f64,
}

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
    overview_context: CorpusOverviewContext,
}

/// Sparse Rust-owned analysis index.
#[derive(Debug, Clone)]
pub struct AnalysisCorpus {
    documents: Vec<AnalysisDocument>,
    terms: Vec<AnalysisTerm>,
    term_index: HashMap<String, usize>,
    overview: CorpusOverview,
}

impl AnalysisCorpus {
    /// Returns the compact summary computed with the index.
    pub fn overview(&self) -> &CorpusOverview {
        &self.overview
    }

    /// Returns whether active processing settings differ from this snapshot.
    pub fn is_stale_for_cleaning_config(&self, cleaning_config: &CleaningConfig) -> bool {
        self.overview.processing_config_signature != cleaning_config_signature(cleaning_config)
    }
}

/// Compact selection of analysis documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyMatchMode {
    Contains,
    Exact,
    StartsWith,
    Regex,
}

/// Token-class filter applied by Rust.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyTokenClass {
    Alphabetic,
    IncludeSingles,
    IncludeNumerals,
    All,
}

/// Sort column for frequency results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FrequencySortKey {
    Token,
    Frequency,
    Range,
    Rate,
    Dispersion,
}

/// Sort direction for frequency results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Parameters for one bounded frequency-list query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyQuery {
    pub selection: AnalysisDocumentSelection,
    pub query: String,
    pub match_mode: FrequencyMatchMode,
    pub minimum_frequency: u64,
    pub token_class: FrequencyTokenClass,
    pub hide_stopwords: bool,
    pub sort_key: FrequencySortKey,
    pub sort_direction: SortDirection,
    pub offset: usize,
    pub limit: usize,
}

/// One frequency result row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyTermItem {
    pub token: String,
    pub frequency: u64,
    pub document_frequency: usize,
    pub rate_per_10k: f64,
    pub dispersion: f64,
}

/// Bounded frequency-list response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyPage {
    pub total_terms: usize,
    pub scoped_total_types: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<FrequencyTermItem>,
    pub selected_document_count: usize,
    pub total_document_count: usize,
    pub scoped_total_tokens: u64,
    pub query_error: Option<String>,
}

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
}

fn tokenise(text: &str) -> Vec<String> {
    let normalised = text.nfc().collect::<String>().to_lowercase();
    TOKEN_PATTERN
        .find_iter(&normalised)
        .map(|matched| matched.as_str().to_string())
        .collect()
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

fn document_summary(
    record: &DocumentRecord,
    id: usize,
    token_count: u64,
) -> AnalysisDocumentSummary {
    AnalysisDocumentSummary {
        id,
        source_path: record.source_path.to_string_lossy().to_string(),
        relative_path: record.relative_path.to_string_lossy().to_string(),
        token_count,
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
        // are not available without changing the tokenizer's established contract.
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
    let (_, evenness_j, effective_vocabulary_entropy) = entropy_metrics(&frequencies, total_tokens);

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
            raw_ttr: (total_tokens > 0).then(|| total_types as f64 / total_tokens as f64),
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
            evenness_j,
            effective_vocabulary_entropy,
            reading: "A small set of high-frequency forms can carry much of the token mass even when most types are rare."
                .to_string(),
        },
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
                "Unicode letters and numbers; internal apostrophes and hyphens retained"
                    .to_string(),
            ),
            case_handling: Some("Unicode lowercase".to_string()),
            lemmatisation: Some("Not applied".to_string()),
            number_punctuation_handling: Some(
                "Numbers retained; punctuation delimits tokens except internal apostrophes and hyphens"
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
    mut load_text: TextLoader,
    mut report_progress: Progress,
) -> Result<AnalysisCorpus, AnalysisBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
    Progress: FnMut(AnalysisBuildProgress),
{
    let AnalysisBuildOptions {
        built_at,
        build_signature,
        processing_config_signature,
        overview_context,
    } = options;

    report_progress(AnalysisBuildProgress {
        files_done: 0,
        files_total: records.len(),
        tokens_seen: 0,
        types_seen: 0,
        current_phase: AnalysisBuildPhase::LoadingSources,
    });

    let mut documents = Vec::with_capacity(records.len());
    let mut terms = Vec::<AnalysisTerm>::new();
    let mut term_index = HashMap::<String, usize>::new();
    let mut total_tokens = 0u64;
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

    for (record_index, record) in records.iter().enumerate() {
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
        let Some(text) = text.filter(|value| !value.trim().is_empty()) else {
            skipped_documents += 1;
            if record.document_type == DocumentType::Pdf {
                skipped_pdf_documents += 1;
            }
            report_progress(AnalysisBuildProgress {
                files_done: record_index + 1,
                files_total: records.len(),
                tokens_seen: total_tokens,
                types_seen: terms.len(),
                current_phase: AnalysisBuildPhase::ReadingFiles,
            });
            continue;
        };
        let tokens = tokenise(&text);
        let document_mattr = mattr(&tokens, MATTR_WINDOW_TOKENS);
        let document_mtld = (tokens.len() >= MTLD_MIN_DOCUMENT_TOKENS)
            .then(|| mtld(&tokens, MTLD_TTR_THRESHOLD))
            .flatten();
        let mut document_terms = HashMap::<String, u64>::new();
        for token in tokens {
            *document_terms.entry(token).or_default() += 1;
        }
        let document_id = documents.len();
        let token_count = document_terms.values().sum::<u64>();
        let mut sparse_counts = Vec::with_capacity(document_terms.len());
        for (token, count) in document_terms {
            let term_id = if let Some(&term_id) = term_index.get(&token) {
                term_id
            } else {
                let term_id = terms.len();
                term_index.insert(token.clone(), term_id);
                terms.push(AnalysisTerm {
                    token,
                    frequency: 0,
                    document_frequency: 0,
                    document_counts: Vec::new(),
                    dispersion: 0.0,
                });
                term_id
            };
            let term = &mut terms[term_id];
            term.frequency += count;
            term.document_frequency += 1;
            term.document_counts.push((document_id, count));
            sparse_counts.push((term_id, count));
        }
        sparse_counts.sort_unstable_by_key(|(term_id, _)| *term_id);
        total_tokens += token_count;
        documents.push(AnalysisDocument {
            summary: document_summary(record, document_id, token_count),
            term_counts: sparse_counts,
            mattr: document_mattr,
            mtld: document_mtld,
        });
        report_progress(AnalysisBuildProgress {
            files_done: record_index + 1,
            files_total: records.len(),
            tokens_seen: total_tokens,
            types_seen: terms.len(),
            current_phase: AnalysisBuildPhase::Tokenising,
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
            overview_context,
        },
    );

    Ok(AnalysisCorpus {
        documents,
        terms,
        term_index,
        overview,
    })
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

/// Filters, sorts, and pages a frequency list entirely in Rust.
pub fn query_frequency_list(corpus: &AnalysisCorpus, query: &FrequencyQuery) -> FrequencyPage {
    let (selected, selected_document_count) = selected_document_mask(corpus, &query.selection);
    let all_documents_selected = selected_document_count == corpus.documents.len();
    let scoped_total_tokens = corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .map(|document| document.summary.token_count)
        .sum::<u64>();
    let token_query = query.query.trim().to_lowercase();
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
                };
            }
        }
    } else {
        None
    };

    let mut scoped_frequency = vec![0u64; corpus.terms.len()];
    let mut scoped_document_frequency = vec![0usize; corpus.terms.len()];
    if !all_documents_selected {
        for document in corpus
            .documents
            .iter()
            .filter(|document| selected[document.summary.id])
        {
            for &(term_id, count) in &document.term_counts {
                scoped_frequency[term_id] += count;
                scoped_document_frequency[term_id] += 1;
            }
        }
    }

    let mut scoped_total_types = 0usize;
    let mut items = Vec::new();
    for (term_id, term) in corpus.terms.iter().enumerate() {
        let frequency = if all_documents_selected {
            term.frequency
        } else {
            scoped_frequency[term_id]
        };
        if frequency == 0 {
            continue;
        }
        scoped_total_types += 1;
        if frequency < query.minimum_frequency
            || !token_matches_class(&term.token, query.token_class)
            || (query.hide_stopwords && ENGLISH_STOPWORDS.contains(term.token.as_str()))
        {
            continue;
        }
        let matches = if token_query.is_empty() {
            true
        } else {
            match query.match_mode {
                FrequencyMatchMode::Contains => term.token.contains(&token_query),
                FrequencyMatchMode::Exact => term.token == token_query,
                FrequencyMatchMode::StartsWith => term.token.starts_with(&token_query),
                FrequencyMatchMode::Regex => regex
                    .as_ref()
                    .is_some_and(|pattern| pattern.is_match(&term.token)),
            }
        };
        if !matches {
            continue;
        }
        let document_frequency = if all_documents_selected {
            term.document_frequency
        } else {
            scoped_document_frequency[term_id]
        };
        let dispersion = if all_documents_selected {
            term.dispersion
        } else {
            let scoped_counts = term
                .document_counts
                .iter()
                .copied()
                .filter(|(document_id, _)| selected[*document_id])
                .collect::<Vec<_>>();
            compute_dispersion(
                &scoped_counts,
                frequency,
                &corpus.documents,
                scoped_total_tokens,
            )
        };
        items.push(FrequencyTermItem {
            token: term.token.clone(),
            frequency,
            document_frequency,
            rate_per_10k: if scoped_total_tokens > 0 {
                frequency as f64 / scoped_total_tokens as f64 * 10_000.0
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
    }
}

/// Returns one bounded per-document distribution window for a token.
pub fn query_term_distribution(
    corpus: &AnalysisCorpus,
    token: &str,
    selection: &AnalysisDocumentSelection,
    offset: usize,
    limit: usize,
) -> TermDistributionPage {
    let (selected, selected_count) = selected_document_mask(corpus, selection);
    let normalised_token = token.nfc().collect::<String>().to_lowercase();
    let term = corpus
        .term_index
        .get(&normalised_token)
        .and_then(|term_id| corpus.terms.get(*term_id));
    let counts = term
        .map(|term| {
            term.document_counts
                .iter()
                .copied()
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let mut max_rate_per_10k = 0.0f64;
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
    }

    let limit = limit.min(MAX_QUERY_WINDOW);
    let offset = offset.min(selected_count);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{RemovalMatcher, RemovalRule, RemovalRuleSource, RemovalScope};
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
            |_, config| Ok(crate::clean::clean_text("alpha<br /><br />beta", config)),
            |_, _| {
                fallback_called.set(true);
                Ok("alpha<br />beta".to_string())
            },
        );

        assert_eq!(resolution.text.as_deref(), Some("alpha beta"));
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
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Desc,
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(page.total_terms, 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].token, "cats");
        assert_eq!(page.items[0].frequency, 2);
        assert_eq!(page.items[1].token, "dog");
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
            &AnalysisDocumentSelection::default(),
            0,
            1,
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
        assert_eq!(overview.lexical_spectrum.raw_ttr, None);
        assert_eq!(overview.lexical_spectrum.mattr_median, None);
        assert_eq!(overview.lexical_spectrum.mtld_median, None);
        assert_eq!(overview.frequency_concentration.evenness_j, None);
        assert_eq!(
            overview
                .frequency_concentration
                .effective_vocabulary_entropy,
            None
        );
        let json = serde_json::to_string(overview).unwrap();
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
}
