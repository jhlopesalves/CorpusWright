//! Rust-owned corpus analysis and bounded query APIs.

#[cfg(test)]
use crate::annotation::LemmaTable;
#[cfg(test)]
use crate::annotation::{AnnotatedToken, UniversalPos, default_model_dir};
#[cfg(test)]
use crate::clean::CleaningConfig;
use crate::hash::sha256_hex;
use crate::index::{
    InMemoryIndexStore, IndexStore, SurfaceIndexDocumentInput, SurfaceIndexError,
    SurfaceIndexKwicRow, SurfaceIndexManifest, SurfaceIndexQuery, search_surface_index_kwic,
};
#[cfg(test)]
use crate::scan::{DocumentRecord, DocumentType};
use crate::tokenization::{TokenizationConfig, TokenizationManifest, tokenize};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use ts_rs::TS;

mod annotation_build;
mod build;
mod collocation;
mod distribution;
mod frequency;
mod input;
mod overview;

pub(crate) use build::compare_document_records;
pub use build::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildPhase, AnalysisBuildProgress,
    AnalysisCorpus, AnalysisDocumentSelection, AnalysisDocumentSummary, build_analysis_corpus,
};
use build::{
    AnalysisBuildMetadata, AnalysisDocument, AnalysisTerm, compute_dispersion, document_name,
};
#[cfg(test)]
use build::{AnalysisBuildProduct, build_analysis_corpus_with_mapping};

pub(crate) use annotation_build::AnnotationBuildTimingCounters;
pub use annotation_build::{
    AnnotatedTermCount, AnnotationAttachError, AnnotationCacheAvailability,
    AnnotationCacheBuildError, AnnotationCacheBuildPhase, AnnotationCacheBuildProgress,
    AnnotationCacheBuildReport, AnnotationCacheState, AnnotationLayer, AnnotationLayerBuildError,
    AnnotationLayerBuildOptions, AnnotationLayerDocument, annotation_cache_availability_for_corpus,
    build_and_write_annotation_cache,
    build_and_write_annotation_cache_with_reusable_surface_corpus, build_annotation_layer,
};
#[cfg(test)]
use annotation_build::{
    LocalAnnotatedTermCount, TEST_ANNOTATION_TIMING_ENABLED, build_annotation_layer_with_mapping,
    build_surface_term_lookup, load_annotation_document_inputs,
    resolve_annotated_tokens_to_local_counts, validate_reusable_surface_corpus,
};
use annotation_build::{attach_annotation_layer, build_expected_manifest};

pub use input::{
    AnalysisInputProvenance, AnalysisInputResolution, AnalysisInputSource, AnalysisInputStatus,
    cleaning_config_signature, resolve_analysis_input,
};

pub use collocation::{
    ConcordanceCollocateRow, ConcordanceCollocationAggregate,
    ConcordanceCollocationAggregationSignature, ConcordanceCollocationDirection,
    ConcordanceCollocationMeasure, ConcordanceCollocationQuery, ConcordanceCollocationResult,
    ConcordanceCollocationSortKey, aggregate_concordance_collocations_documents,
    concordance_collocation_aggregation_signature, query_concordance_collocations_documents,
    query_concordance_collocations_from_aggregate,
};

pub use distribution::{
    AnalysisDocumentPage, AnalysisDocumentPageItem, AnnotatedTermDistributionQuery,
    AnnotatedTermSelector, TermDistributionItem, TermDistributionPage, TermDistributionQuery,
    query_analysis_documents, query_annotated_term_distribution, query_term_distribution,
};

#[cfg(test)]
use frequency::{ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE, frequency_distribution_stats};
pub use frequency::{
    AnnotatedFrequencyMode, AnnotatedFrequencyQuery, AnnotatedFrequencyResult, FrequencyGroupMode,
    FrequencyMatchMode, FrequencyNotice, FrequencyNoticeKind, FrequencyPage, FrequencyQuery,
    FrequencySortKey, FrequencyTermItem, FrequencyTokenClass, LexFrequencyRow, PosFrequencyRow,
    SortDirection, query_annotated_frequency, query_frequency_list,
};
use frequency::{annotated_dispersion_label, case_fold_surface};

pub use overview::{
    CorpusOverview, CorpusOverviewBalance, CorpusOverviewBuild, CorpusOverviewContext,
    CorpusOverviewDispersion, CorpusOverviewDocument, CorpusOverviewExtractionQuality,
    CorpusOverviewFinding, CorpusOverviewFindingKind, CorpusOverviewFindingSeverity,
    CorpusOverviewFrequencyConcentration, CorpusOverviewLexicalSpectrum, CorpusOverviewPopulation,
    CorpusOverviewRankFrequency, CorpusOverviewReading, CorpusOverviewTotals,
    CorpusOverviewVocabularyGrowth, FrequencySpectrumPoint, RankFrequencyPoint,
    VocabularyGrowthPoint,
};
use overview::{
    MATTR_WINDOW_TOKENS, MTLD_MIN_DOCUMENT_TOKENS, MTLD_TTR_THRESHOLD, build_overview, mattr, mtld,
};
#[cfg(test)]
use overview::{
    OVERVIEW_DOCUMENT_LIMIT, balance_severity, coefficient_of_variation, dispersion_severity,
    effective_document_count, entropy_metrics, extraction_severity, frequency_spectrum,
    gini_coefficient, rank_frequency_summary, vocabulary_growth_summary,
};

const MAX_QUERY_WINDOW: usize = 500;
const CONCORDANCE_DEFAULT_WINDOW_TOKENS: usize = 5;
const CONCORDANCE_MAX_WINDOW_TOKENS: usize = 20;
const CONCORDANCE_DEFAULT_EXPANSION_WINDOW_TOKENS: usize = 40;
const CONCORDANCE_DEFAULT_LIMIT: usize = 100;
const CONCORDANCE_MAX_LIMIT: usize = 500;
const CONCORDANCE_DEFAULT_DISTRIBUTION_LIMIT: usize = 10;
/// Heuristic DP threshold below which an item is treated as broadly dispersed.
///
/// Plain Gries-style DP is unnormalised here: its upper bound depends on the
/// document-size shares, so these labels are corpus-internal orientation aids,
/// not cross-corpus cut-offs.
const ANNOTATED_DP_BROADLY_DISPERSED_MAX: f64 = 0.30;
/// Heuristic DP threshold from which an item is treated as highly concentrated.
const ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN: f64 = 0.70;

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

/// Server-side sort key for a bounded surface-token concordance query.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordanceSortKey {
    #[default]
    FilePosition,
    Node,
    Left1,
    Left2,
    Left3,
    Right1,
    Right2,
    Right3,
}

/// Parameters for a bounded surface-token concordance query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceQuery {
    #[serde(default)]
    pub selection: AnalysisDocumentSelection,
    pub query: String,
    #[serde(default)]
    pub context_filter: Option<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub sort_key: ConcordanceSortKey,
    pub window_tokens: usize,
    pub offset: usize,
    pub limit: usize,
    #[serde(default)]
    pub distribution_offset: usize,
    #[serde(default)]
    pub distribution_limit: usize,
}

/// Parameters for expanding one surface-token concordance hit in its document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceExpansionQuery {
    pub document_index: usize,
    pub token_index: usize,
    pub node_token_length: usize,
    pub window_tokens: usize,
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
    pub node_token_length: usize,
    pub left_context: Vec<String>,
    pub node: String,
    pub right_context: Vec<String>,
}

/// Wider context for one surface-token concordance hit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceExpansionResult {
    pub document_index: usize,
    pub source_path: String,
    pub display_name: String,
    pub token_index: usize,
    pub node_token_length: usize,
    pub window_tokens: usize,
    pub left_context: Vec<String>,
    pub node_tokens: Vec<String>,
    pub node: String,
    pub right_context: Vec<String>,
}

/// One per-document row for a concordance hit distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceDistributionItem {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    #[ts(type = "number")]
    pub token_count: u64,
    #[ts(type = "number")]
    pub count: u64,
    pub rate_per_10k: f64,
    pub rate_per_million: f64,
}

/// Bounded per-document distribution for the current concordance query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceDistributionPage {
    pub total_documents: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<ConcordanceDistributionItem>,
    #[ts(type = "number")]
    pub max_count: u64,
}

/// Bounded surface-token concordance response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceResult {
    pub query: String,
    pub context_filter: Option<String>,
    pub case_sensitive: bool,
    pub sort_key: ConcordanceSortKey,
    pub window_tokens: usize,
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub selected_documents_total: usize,
    #[ts(type = "number")]
    pub selected_tokens_total: u64,
    pub dp: f64,
    pub dispersion_label: String,
    pub top_document_share: f64,
    pub offset: usize,
    pub limit: usize,
    pub rows: Vec<ConcordanceRow>,
    pub distribution: ConcordanceDistributionPage,
}

/// Text supplied by the desktop layer for an on-demand concordance scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcordanceDocument {
    pub document_index: usize,
    pub source_path: String,
    pub display_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConcordanceBackend {
    #[default]
    Scan,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcordanceFallbackReason {
    UnsupportedFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcordanceBackendUsed {
    Scan,
    Index,
    ScanFallback { reason: ConcordanceFallbackReason },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutedConcordanceResult {
    pub result: ConcordanceResult,
    pub backend_used: ConcordanceBackendUsed,
    pub index_rebuilt: bool,
}

#[derive(Debug, Default)]
pub struct ConcordanceSearchContext {
    index_cache: Option<ConcordanceIndexCacheEntry>,
    index_build_count: usize,
}

impl ConcordanceSearchContext {
    fn index_store_for(
        &mut self,
        documents: &[ConcordanceDocument],
        selected_document_ids: &HashSet<usize>,
        tokenization_config: &TokenizationConfig,
    ) -> Result<(&InMemoryIndexStore, bool), ConcordanceError> {
        let manifest = concordance_surface_index_manifest(
            documents,
            selected_document_ids,
            tokenization_config,
        );
        let cache_valid = self
            .index_cache
            .as_ref()
            .is_some_and(|entry| entry.manifest.is_valid_for(&manifest));

        if !cache_valid {
            let inputs = documents
                .iter()
                .filter(|document| selected_document_ids.contains(&document.document_index))
                .map(|document| SurfaceIndexDocumentInput {
                    document_id: document.document_index,
                    source_path: document.source_path.clone(),
                    display_name: document.display_name.clone(),
                    prepared_text: document.text.as_str(),
                })
                .collect::<Vec<_>>();
            let store = InMemoryIndexStore::build(&inputs, tokenization_config)
                .map_err(ConcordanceError::Index)?;
            self.index_cache = Some(ConcordanceIndexCacheEntry { manifest, store });
            self.index_build_count += 1;
        }

        Ok((
            &self
                .index_cache
                .as_ref()
                .expect("index cache exists after build")
                .store,
            !cache_valid,
        ))
    }

    #[cfg(test)]
    fn index_build_count(&self) -> usize {
        self.index_build_count
    }
}

#[derive(Debug)]
struct ConcordanceIndexCacheEntry {
    manifest: SurfaceIndexManifest,
    store: InMemoryIndexStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcordanceError {
    EmptyQuery,
    InvalidDocumentIdentity,
    InvalidHitIdentity,
    Index(SurfaceIndexError),
}

impl std::fmt::Display for ConcordanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyQuery => formatter.write_str("Enter a word or phrase to search."),
            Self::InvalidDocumentIdentity => {
                formatter.write_str("The concordance document no longer matches the result.")
            }
            Self::InvalidHitIdentity => {
                formatter.write_str("The concordance hit no longer matches the document.")
            }
            Self::Index(error) => write!(formatter, "The concordance index failed: {error}"),
        }
    }
}

impl std::error::Error for ConcordanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Index(error) => Some(error),
            _ => None,
        }
    }
}

fn compute_dispersion_from_token_counts(
    document_counts: &[(usize, u64)],
    frequency: u64,
    document_token_counts: &HashMap<usize, u64>,
    total_tokens: u64,
) -> f64 {
    if frequency == 0 || total_tokens == 0 {
        return 0.0;
    }
    let mut present_document_share = 0.0;
    let mut difference = 0.0;
    for &(document_id, count) in document_counts {
        let document_share = document_token_counts
            .get(&document_id)
            .copied()
            .unwrap_or_default() as f64
            / total_tokens as f64;
        present_document_share += document_share;
        difference += (count as f64 / frequency as f64 - document_share).abs();
    }
    0.5 * (difference + (1.0 - present_document_share).max(0.0))
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

fn concordance_window_tokens(window_tokens: usize) -> usize {
    if window_tokens == 0 {
        CONCORDANCE_DEFAULT_WINDOW_TOKENS
    } else {
        window_tokens.clamp(1, CONCORDANCE_MAX_WINDOW_TOKENS)
    }
}

fn concordance_expansion_window_tokens(window_tokens: usize) -> usize {
    if window_tokens == 0 {
        CONCORDANCE_DEFAULT_EXPANSION_WINDOW_TOKENS
    } else {
        window_tokens.clamp(1, MAX_QUERY_WINDOW)
    }
}

fn concordance_limit(limit: usize) -> usize {
    if limit == 0 {
        CONCORDANCE_DEFAULT_LIMIT
    } else {
        limit.clamp(1, CONCORDANCE_MAX_LIMIT)
    }
}

fn concordance_distribution_limit(limit: usize) -> usize {
    if limit == 0 {
        CONCORDANCE_DEFAULT_DISTRIBUTION_LIMIT
    } else {
        limit.clamp(1, MAX_QUERY_WINDOW)
    }
}

pub(crate) fn concordance_key(value: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        value.to_string()
    } else {
        case_fold_surface(value)
    }
}

fn selected_concordance_document_ids(
    documents: &[ConcordanceDocument],
    selection: &AnalysisDocumentSelection,
) -> HashSet<usize> {
    if selection.all_documents {
        let excluded_ids = selection
            .excluded_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        documents
            .iter()
            .filter(|document| !excluded_ids.contains(&document.document_index))
            .map(|document| document.document_index)
            .collect()
    } else {
        let requested_ids = selection.ids.iter().copied().collect::<HashSet<_>>();
        documents
            .iter()
            .filter(|document| requested_ids.contains(&document.document_index))
            .map(|document| document.document_index)
            .collect()
    }
}

fn resolve_concordance_query_tokens(
    query: &str,
    tokenization_config: &TokenizationConfig,
) -> Result<Vec<String>, ConcordanceError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(ConcordanceError::EmptyQuery);
    }
    let query_tokens = tokenize(trimmed, 0, tokenization_config)
        .into_iter()
        .filter(|token| token.counts_as_word())
        .map(|token| token.surface.to_string())
        .collect::<Vec<_>>();
    if query_tokens.is_empty() {
        return Err(ConcordanceError::EmptyQuery);
    }
    Ok(query_tokens)
}

fn resolve_optional_concordance_query_tokens(
    query: Option<&str>,
    tokenization_config: &TokenizationConfig,
) -> Result<Option<Vec<String>>, ConcordanceError> {
    match query.map(str::trim).filter(|value| !value.is_empty()) {
        Some(query) => resolve_concordance_query_tokens(query, tokenization_config).map(Some),
        None => Ok(None),
    }
}

#[derive(Debug, Clone)]
struct ConcordanceHit {
    row: ConcordanceRow,
    node_key: String,
    left_keys: [Option<String>; 3],
    right_keys: [Option<String>; 3],
}

#[derive(Debug, Clone)]
struct ConcordanceDocumentStats {
    document_id: usize,
    source_path: String,
    display_name: String,
    token_count: u64,
    hit_count: u64,
}

fn context_sort_key<'text>(
    tokens: &[crate::tokenization::Token<'text>],
    index: Option<usize>,
    case_sensitive: bool,
) -> Option<String> {
    index.map(|token_index| concordance_key(tokens[token_index].surface, case_sensitive))
}

fn concordance_token_sequence_matches<'text>(
    tokens: &[crate::tokenization::Token<'text>],
    query_keys: &[String],
    case_sensitive: bool,
) -> bool {
    !query_keys.is_empty()
        && tokens.windows(query_keys.len()).any(|window| {
            window.iter().zip(query_keys).all(|(token, query_key)| {
                concordance_key(token.surface, case_sensitive) == *query_key
            })
        })
}

fn concordance_context_filter_matches<'text>(
    tokens: &[crate::tokenization::Token<'text>],
    left_start: usize,
    token_index: usize,
    node_end: usize,
    right_end: usize,
    context_filter_keys: &[String],
    case_sensitive: bool,
) -> bool {
    concordance_token_sequence_matches(
        &tokens[left_start..token_index],
        context_filter_keys,
        case_sensitive,
    ) || concordance_token_sequence_matches(
        &tokens[node_end..right_end],
        context_filter_keys,
        case_sensitive,
    )
}

fn compare_optional_context(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_context_sequence(
    left: &ConcordanceHit,
    right: &ConcordanceHit,
    left_side: bool,
    primary_index: usize,
) -> Ordering {
    let keys = if primary_index == 0 {
        [0, 1, 2]
    } else if primary_index == 1 {
        [1, 0, 2]
    } else {
        [2, 1, 0]
    };
    for key_index in keys {
        let left_key = if left_side {
            left.left_keys[key_index].as_deref()
        } else {
            left.right_keys[key_index].as_deref()
        };
        let right_key = if left_side {
            right.left_keys[key_index].as_deref()
        } else {
            right.right_keys[key_index].as_deref()
        };
        let ordering = compare_optional_context(left_key, right_key);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.node_key.cmp(&right.node_key)
}

fn compare_concordance_hits(
    left: &ConcordanceHit,
    right: &ConcordanceHit,
    sort_key: ConcordanceSortKey,
) -> Ordering {
    let primary_ordering = match sort_key {
        ConcordanceSortKey::FilePosition => Ordering::Equal,
        ConcordanceSortKey::Node => left.node_key.cmp(&right.node_key),
        ConcordanceSortKey::Left1 => compare_context_sequence(left, right, true, 0),
        ConcordanceSortKey::Left2 => compare_context_sequence(left, right, true, 1),
        ConcordanceSortKey::Left3 => compare_context_sequence(left, right, true, 2),
        ConcordanceSortKey::Right1 => compare_context_sequence(left, right, false, 0),
        ConcordanceSortKey::Right2 => compare_context_sequence(left, right, false, 1),
        ConcordanceSortKey::Right3 => compare_context_sequence(left, right, false, 2),
    };

    primary_ordering
        .then_with(|| left.row.document_index.cmp(&right.row.document_index))
        .then_with(|| left.row.token_index.cmp(&right.row.token_index))
}

fn concordance_row_text<'text>(tokens: &[crate::tokenization::Token<'text>]) -> String {
    tokens
        .iter()
        .map(|token| token.surface)
        .collect::<Vec<_>>()
        .join(" ")
}

fn concordance_token_surfaces<'text>(tokens: &[crate::tokenization::Token<'text>]) -> Vec<String> {
    tokens
        .iter()
        .map(|token| token.surface.to_string())
        .collect()
}

fn concordance_distribution_page(
    document_stats: Vec<ConcordanceDocumentStats>,
    offset: usize,
    limit: usize,
) -> ConcordanceDistributionPage {
    let mut items = document_stats
        .into_iter()
        .map(|stats| ConcordanceDistributionItem {
            document_id: stats.document_id,
            source_path: stats.source_path,
            display_name: stats.display_name,
            token_count: stats.token_count,
            count: stats.hit_count,
            rate_per_10k: if stats.token_count > 0 {
                stats.hit_count as f64 / stats.token_count as f64 * 10_000.0
            } else {
                0.0
            },
            rate_per_million: if stats.token_count > 0 {
                stats.hit_count as f64 / stats.token_count as f64 * 1_000_000.0
            } else {
                0.0
            },
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        let left_has_hits = left.count > 0;
        let right_has_hits = right.count > 0;
        right_has_hits
            .cmp(&left_has_hits)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.document_id.cmp(&right.document_id))
    });
    let max_count = items.iter().map(|item| item.count).max().unwrap_or(0);
    let total_documents = items.len();
    let offset = offset.min(total_documents);
    ConcordanceDistributionPage {
        total_documents,
        offset,
        limit,
        items: items.into_iter().skip(offset).take(limit).collect(),
        max_count,
    }
}

fn concordance_surface_index_manifest(
    documents: &[ConcordanceDocument],
    selected_document_ids: &HashSet<usize>,
    tokenization_config: &TokenizationConfig,
) -> SurfaceIndexManifest {
    SurfaceIndexManifest::new(
        concordance_processed_texts_hash(documents, selected_document_ids),
        TokenizationManifest {
            config: *tokenization_config,
            ..TokenizationManifest::default()
        },
    )
}

fn concordance_processed_texts_hash(
    documents: &[ConcordanceDocument],
    selected_document_ids: &HashSet<usize>,
) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"corpuswright-concordance-surface-index-v1\0");
    for document in documents
        .iter()
        .filter(|document| selected_document_ids.contains(&document.document_index))
    {
        push_hash_usize(&mut bytes, document.document_index);
        push_hash_bytes(&mut bytes, document.source_path.as_bytes());
        push_hash_bytes(&mut bytes, document.display_name.as_bytes());
        push_hash_bytes(&mut bytes, document.text.as_bytes());
    }
    sha256_hex(&bytes)
}

fn push_hash_usize(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&(value as u64).to_le_bytes());
}

fn push_hash_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value);
}

pub fn search_concordance_documents_routed(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
) -> Result<RoutedConcordanceResult, ConcordanceError> {
    match backend {
        ConcordanceBackend::Scan => Ok(RoutedConcordanceResult {
            result: search_concordance_documents_scan(documents, query, tokenization_config)?,
            backend_used: ConcordanceBackendUsed::Scan,
            index_rebuilt: false,
        }),
        ConcordanceBackend::Index => {
            if let Some(reason) = concordance_index_fallback_reason(query) {
                return Ok(RoutedConcordanceResult {
                    result: search_concordance_documents_scan(
                        documents,
                        query,
                        tokenization_config,
                    )?,
                    backend_used: ConcordanceBackendUsed::ScanFallback { reason },
                    index_rebuilt: false,
                });
            }

            let (result, index_rebuilt) =
                search_concordance_documents_index(documents, query, tokenization_config, context)?;
            Ok(RoutedConcordanceResult {
                result,
                backend_used: ConcordanceBackendUsed::Index,
                index_rebuilt,
            })
        }
    }
}

fn concordance_index_fallback_reason(
    query: &ConcordanceQuery,
) -> Option<ConcordanceFallbackReason> {
    query
        .context_filter
        .as_deref()
        .is_some_and(|filter| !filter.trim().is_empty())
        .then_some(ConcordanceFallbackReason::UnsupportedFilter)
}

fn search_concordance_documents_index(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
) -> Result<(ConcordanceResult, bool), ConcordanceError> {
    let window_tokens = concordance_window_tokens(query.window_tokens);
    let limit = concordance_limit(query.limit);
    let distribution_limit = concordance_distribution_limit(query.distribution_limit);
    let index_query = SurfaceIndexQuery::from_surface_text_with_case(
        &query.query,
        query.case_sensitive,
        tokenization_config,
    )
    .map_err(concordance_error_from_surface_index)?;
    let resolved_query = index_query
        .terms()
        .iter()
        .map(|term| term.surface.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);
    let (store, index_rebuilt) =
        context.index_store_for(documents, &selected_document_ids, tokenization_config)?;
    let index = search_surface_index_kwic(store, &index_query, window_tokens)
        .map_err(concordance_error_from_surface_index)?;

    let mut hits = index
        .rows
        .into_iter()
        .map(|row| concordance_hit_from_index_row(store, row, query.case_sensitive))
        .collect::<Result<Vec<_>, _>>()?;
    hits.sort_by(|left, right| compare_concordance_hits(left, right, query.sort_key));

    let total_hits = hits.len();
    let offset = query.offset.min(total_hits);
    let rows = hits
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|hit| hit.row)
        .collect::<Vec<_>>();

    let mut document_counts = Vec::<(usize, u64)>::new();
    let mut document_token_counts = HashMap::<usize, u64>::new();
    let mut document_stats = Vec::<ConcordanceDocumentStats>::new();
    let mut selected_tokens_total = 0u64;

    for document in documents
        .iter()
        .filter(|document| selected_document_ids.contains(&document.document_index))
    {
        let indexed_document = store.document(document.document_index).ok_or({
            ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                document_id: document.document_index,
                token_ordinal_in_document: 0,
                node_token_length: 0,
            })
        })?;
        let token_count = indexed_document.token_count as u64;
        let hit_count = index
            .document_hit_counts
            .get(&document.document_index)
            .copied()
            .unwrap_or_default();
        selected_tokens_total += token_count;
        document_token_counts.insert(document.document_index, token_count);
        if hit_count > 0 {
            document_counts.push((document.document_index, hit_count));
        }
        document_stats.push(ConcordanceDocumentStats {
            document_id: document.document_index,
            source_path: document.source_path.clone(),
            display_name: document.display_name.clone(),
            token_count,
            hit_count,
        });
    }

    let total_hits_u64 = total_hits as u64;
    let dp = compute_dispersion_from_token_counts(
        &document_counts,
        total_hits_u64,
        &document_token_counts,
        selected_tokens_total,
    );
    let top_document_share = if total_hits == 0 {
        0.0
    } else {
        document_counts
            .iter()
            .map(|(_, count)| *count)
            .max()
            .unwrap_or(0) as f64
            / total_hits as f64
    };
    let distribution = concordance_distribution_page(
        document_stats,
        query.distribution_offset,
        distribution_limit,
    );

    Ok((
        ConcordanceResult {
            query: resolved_query,
            context_filter: None,
            case_sensitive: query.case_sensitive,
            sort_key: query.sort_key,
            window_tokens,
            total_hits,
            documents_with_hits: document_counts.len(),
            selected_documents_total: selected_document_ids.len(),
            selected_tokens_total,
            dp,
            dispersion_label: annotated_dispersion_label(dp).to_string(),
            top_document_share,
            offset,
            limit,
            rows,
            distribution,
        },
        index_rebuilt,
    ))
}

fn concordance_error_from_surface_index(error: SurfaceIndexError) -> ConcordanceError {
    match error {
        SurfaceIndexError::EmptyQuery => ConcordanceError::EmptyQuery,
        error => ConcordanceError::Index(error),
    }
}

fn concordance_hit_from_index_row(
    store: &impl IndexStore,
    row: SurfaceIndexKwicRow,
    case_sensitive: bool,
) -> Result<ConcordanceHit, ConcordanceError> {
    let records = store.records_for_document(row.document_id).ok_or({
        ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
            document_id: row.document_id,
            token_ordinal_in_document: row.token_ordinal_in_document,
            node_token_length: row.node_token_length,
        })
    })?;
    let node_end = row
        .token_ordinal_in_document
        .checked_add(row.node_token_length)
        .ok_or({
            ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                document_id: row.document_id,
                token_ordinal_in_document: row.token_ordinal_in_document,
                node_token_length: row.node_token_length,
            })
        })?;
    if row.node_token_length == 0 || node_end > records.len() {
        return Err(ConcordanceError::Index(
            SurfaceIndexError::InconsistentStore {
                document_id: row.document_id,
                token_ordinal_in_document: row.token_ordinal_in_document,
                node_token_length: row.node_token_length,
            },
        ));
    }

    Ok(ConcordanceHit {
        node_key: concordance_key(&row.node, case_sensitive),
        left_keys: [
            index_context_sort_key(
                records,
                row.token_ordinal_in_document.checked_sub(1),
                case_sensitive,
            ),
            index_context_sort_key(
                records,
                row.token_ordinal_in_document.checked_sub(2),
                case_sensitive,
            ),
            index_context_sort_key(
                records,
                row.token_ordinal_in_document.checked_sub(3),
                case_sensitive,
            ),
        ],
        right_keys: [
            index_context_sort_key(records, Some(node_end), case_sensitive),
            index_context_sort_key(records, node_end.checked_add(1), case_sensitive),
            index_context_sort_key(records, node_end.checked_add(2), case_sensitive),
        ],
        row: ConcordanceRow {
            document_index: row.document_id,
            source_path: row.source_path,
            display_name: row.display_name,
            token_index: row.token_ordinal_in_document,
            node_token_length: row.node_token_length,
            left_context: row.left_context,
            node: row.node,
            right_context: row.right_context,
        },
    })
}

fn index_context_sort_key(
    records: &[crate::index::SurfaceIndexTokenRecord],
    index: Option<usize>,
    case_sensitive: bool,
) -> Option<String> {
    index
        .and_then(|token_index| records.get(token_index))
        .map(|record| concordance_key(&record.surface, case_sensitive))
}

/// Searches processed text for a surface token sequence and returns KWIC rows.
pub fn search_concordance_documents(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceResult, ConcordanceError> {
    let mut context = ConcordanceSearchContext::default();
    search_concordance_documents_routed(
        documents,
        query,
        tokenization_config,
        &mut context,
        ConcordanceBackend::default(),
    )
    .map(|routed| routed.result)
}

fn search_concordance_documents_scan(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceResult, ConcordanceError> {
    let window_tokens = concordance_window_tokens(query.window_tokens);
    let limit = concordance_limit(query.limit);
    let distribution_limit = concordance_distribution_limit(query.distribution_limit);
    let query_tokens = resolve_concordance_query_tokens(&query.query, tokenization_config)?;
    let query_keys = query_tokens
        .iter()
        .map(|token| concordance_key(token, query.case_sensitive))
        .collect::<Vec<_>>();
    let context_filter_tokens = resolve_optional_concordance_query_tokens(
        query.context_filter.as_deref(),
        tokenization_config,
    )?;
    let context_filter_keys = context_filter_tokens.as_ref().map(|tokens| {
        tokens
            .iter()
            .map(|token| concordance_key(token, query.case_sensitive))
            .collect::<Vec<_>>()
    });
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);
    let mut document_counts = Vec::<(usize, u64)>::new();
    let mut document_token_counts = HashMap::<usize, u64>::new();
    let mut document_stats = Vec::<ConcordanceDocumentStats>::new();
    let mut hits = Vec::<ConcordanceHit>::new();
    let mut selected_tokens_total = 0u64;

    for document in documents {
        if !selected_document_ids.contains(&document.document_index) {
            continue;
        }
        let tokens = analysis_word_tokens(
            &document.text,
            document.document_index as u32,
            tokenization_config,
        );
        let document_token_count = tokens.len() as u64;
        selected_tokens_total += document_token_count;
        document_token_counts.insert(document.document_index, document_token_count);
        let mut document_hit_count = 0u64;
        for token_index in 0..tokens.len() {
            let node_end = token_index + query_keys.len();
            if node_end > tokens.len() {
                break;
            }
            let node_matches =
                tokens[token_index..node_end]
                    .iter()
                    .zip(&query_keys)
                    .all(|(token, query_key)| {
                        concordance_key(token.surface, query.case_sensitive) == *query_key
                    });
            if !node_matches {
                continue;
            }

            let left_start = token_index.saturating_sub(window_tokens);
            let right_end = (node_end + window_tokens).min(tokens.len());
            if context_filter_keys.as_ref().is_some_and(|keys| {
                !concordance_context_filter_matches(
                    &tokens,
                    left_start,
                    token_index,
                    node_end,
                    right_end,
                    keys,
                    query.case_sensitive,
                )
            }) {
                continue;
            }
            let node_tokens = &tokens[token_index..node_end];
            let node = concordance_row_text(node_tokens);
            let node_key = concordance_key(&node, query.case_sensitive);
            hits.push(ConcordanceHit {
                row: ConcordanceRow {
                    document_index: document.document_index,
                    source_path: document.source_path.clone(),
                    display_name: document.display_name.clone(),
                    token_index,
                    node_token_length: query_keys.len(),
                    left_context: tokens[left_start..token_index]
                        .iter()
                        .map(|token| token.surface.to_string())
                        .collect(),
                    node,
                    right_context: tokens[node_end..right_end]
                        .iter()
                        .map(|token| token.surface.to_string())
                        .collect(),
                },
                node_key,
                left_keys: [
                    context_sort_key(&tokens, token_index.checked_sub(1), query.case_sensitive),
                    context_sort_key(&tokens, token_index.checked_sub(2), query.case_sensitive),
                    context_sort_key(&tokens, token_index.checked_sub(3), query.case_sensitive),
                ],
                right_keys: [
                    context_sort_key(
                        &tokens,
                        (node_end < tokens.len()).then_some(node_end),
                        query.case_sensitive,
                    ),
                    context_sort_key(
                        &tokens,
                        (node_end + 1 < tokens.len()).then_some(node_end + 1),
                        query.case_sensitive,
                    ),
                    context_sort_key(
                        &tokens,
                        (node_end + 2 < tokens.len()).then_some(node_end + 2),
                        query.case_sensitive,
                    ),
                ],
            });

            document_hit_count += 1;
        }
        if document_hit_count > 0 {
            document_counts.push((document.document_index, document_hit_count));
        }
        document_stats.push(ConcordanceDocumentStats {
            document_id: document.document_index,
            source_path: document.source_path.clone(),
            display_name: document.display_name.clone(),
            token_count: document_token_count,
            hit_count: document_hit_count,
        });
    }
    hits.sort_by(|left, right| compare_concordance_hits(left, right, query.sort_key));

    let total_hits = hits.len();
    let offset = query.offset.min(total_hits);
    let rows = hits
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|hit| hit.row)
        .collect::<Vec<_>>();
    let total_hits_u64 = total_hits as u64;
    let dp = compute_dispersion_from_token_counts(
        &document_counts,
        total_hits_u64,
        &document_token_counts,
        selected_tokens_total,
    );
    let top_document_share = if total_hits == 0 {
        0.0
    } else {
        document_counts
            .iter()
            .map(|(_, count)| *count)
            .max()
            .unwrap_or(0) as f64
            / total_hits as f64
    };
    let distribution = concordance_distribution_page(
        document_stats,
        query.distribution_offset,
        distribution_limit,
    );

    Ok(ConcordanceResult {
        query: query_tokens.join(" "),
        context_filter: context_filter_tokens.map(|tokens| tokens.join(" ")),
        case_sensitive: query.case_sensitive,
        sort_key: query.sort_key,
        window_tokens,
        total_hits,
        documents_with_hits: document_counts.len(),
        selected_documents_total: selected_document_ids.len(),
        selected_tokens_total,
        dp,
        dispersion_label: annotated_dispersion_label(dp).to_string(),
        top_document_share,
        offset,
        limit,
        rows,
        distribution,
    })
}

/// Expands one surface-token concordance hit in its source document.
pub fn expand_concordance_context(
    document: &ConcordanceDocument,
    query: &ConcordanceExpansionQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceExpansionResult, ConcordanceError> {
    if document.document_index != query.document_index {
        return Err(ConcordanceError::InvalidDocumentIdentity);
    }
    let Some(node_end) = query.token_index.checked_add(query.node_token_length) else {
        return Err(ConcordanceError::InvalidHitIdentity);
    };
    if query.node_token_length == 0 {
        return Err(ConcordanceError::InvalidHitIdentity);
    }

    let window_tokens = concordance_expansion_window_tokens(query.window_tokens);
    let tokens = analysis_word_tokens(
        &document.text,
        document.document_index as u32,
        tokenization_config,
    );
    if node_end > tokens.len() {
        return Err(ConcordanceError::InvalidHitIdentity);
    }

    let left_start = query.token_index.saturating_sub(window_tokens);
    let right_end = (node_end + window_tokens).min(tokens.len());
    let node_tokens = concordance_token_surfaces(&tokens[query.token_index..node_end]);

    Ok(ConcordanceExpansionResult {
        document_index: document.document_index,
        source_path: document.source_path.clone(),
        display_name: document.display_name.clone(),
        token_index: query.token_index,
        node_token_length: query.node_token_length,
        window_tokens,
        left_context: concordance_token_surfaces(&tokens[left_start..query.token_index]),
        node: node_tokens.join(" "),
        node_tokens,
        right_context: concordance_token_surfaces(&tokens[node_end..right_end]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{RemovalMatcher, RemovalRule, RemovalRuleSource, RemovalScope};
    use crate::decode::{DecodeStrategy, SourceEncoding, decode_source_bytes};
    use crate::index::{InMemoryIndexStore, SurfaceIndexDocumentInput};
    use crate::language::{ProjectLanguage, resolve_effective_document_languages};
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

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

    fn read_annotation_cache_envelope(
        cache_root: &std::path::Path,
    ) -> crate::annotation_cache::AnnotationLayerCacheEnvelope {
        let cache_file = cache_root.join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        let bytes = std::fs::read(cache_file).unwrap();
        serde_json::from_slice(&bytes).unwrap()
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

    fn assert_close_4dp(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("score should be present");
        assert!(
            (actual - expected).abs() < 0.00005,
            "expected {actual:.4} to be close to {expected:.4}"
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
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            case_sensitive: false,
            sort_key: ConcordanceSortKey::FilePosition,
            window_tokens: 5,
            offset: 0,
            limit: 100,
            distribution_offset: 0,
            distribution_limit: 10,
        }
    }

    fn concordance_for(
        documents: &[ConcordanceDocument],
        query: ConcordanceQuery,
    ) -> Result<ConcordanceResult, ConcordanceError> {
        search_concordance_documents(documents, &query, &TokenizationConfig::default())
    }

    fn routed_concordance_for(
        documents: &[ConcordanceDocument],
        query: &ConcordanceQuery,
        context: &mut ConcordanceSearchContext,
        backend: ConcordanceBackend,
    ) -> Result<RoutedConcordanceResult, ConcordanceError> {
        search_concordance_documents_routed(
            documents,
            query,
            &TokenizationConfig::default(),
            context,
            backend,
        )
    }

    fn parity_concordance_query(query: &str, case_sensitive: bool) -> ConcordanceQuery {
        ConcordanceQuery {
            case_sensitive,
            limit: 500,
            distribution_limit: 500,
            ..concordance_query(query)
        }
    }

    fn assert_routed_index_parity(
        documents: &[ConcordanceDocument],
        query_text: &str,
        case_sensitive: bool,
    ) {
        let query = parity_concordance_query(query_text, case_sensitive);
        let mut scan_context = ConcordanceSearchContext::default();
        let scan = routed_concordance_for(
            documents,
            &query,
            &mut scan_context,
            ConcordanceBackend::Scan,
        )
        .unwrap();
        assert_eq!(scan.backend_used, ConcordanceBackendUsed::Scan);

        let mut index_context = ConcordanceSearchContext::default();
        let index = routed_concordance_for(
            documents,
            &query,
            &mut index_context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(
            index.result, scan.result,
            "{query_text} case_sensitive={case_sensitive}"
        );
    }

    fn assert_routed_index_parity_both_modes(documents: &[ConcordanceDocument], query_text: &str) {
        assert_routed_index_parity(documents, query_text, false);
        assert_routed_index_parity(documents, query_text, true);
    }

    fn collocation_query(query: &str) -> ConcordanceCollocationQuery {
        ConcordanceCollocationQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            case_sensitive: false,
            span: 2,
            direction: ConcordanceCollocationDirection::Both,
            min_cooccurrence: 1,
            min_collocate_frequency: 1,
            hide_stopwords: false,
            sort_measure: ConcordanceCollocationMeasure::LogDice,
            sort_key: ConcordanceCollocationSortKey::Association,
            sort_direction: SortDirection::Desc,
            limit: 100,
        }
    }

    fn collocations_for(
        documents: &[ConcordanceDocument],
        query: ConcordanceCollocationQuery,
    ) -> Result<ConcordanceCollocationResult, ConcordanceError> {
        query_concordance_collocations_documents(documents, &query, &TokenizationConfig::default())
    }

    fn collocation_aggregate_for(
        documents: &[ConcordanceDocument],
        query: &ConcordanceCollocationQuery,
    ) -> Result<ConcordanceCollocationAggregate, ConcordanceError> {
        aggregate_concordance_collocations_documents(
            documents,
            query,
            &TokenizationConfig::default(),
        )
    }

    fn collocate<'a>(
        result: &'a ConcordanceCollocationResult,
        surface: &str,
    ) -> &'a ConcordanceCollocateRow {
        result
            .rows
            .iter()
            .find(|row| row.surface == surface)
            .unwrap_or_else(|| panic!("missing collocate {surface}"))
    }

    fn concordance_expansion_for(
        document: &ConcordanceDocument,
        query: ConcordanceExpansionQuery,
    ) -> Result<ConcordanceExpansionResult, ConcordanceError> {
        expand_concordance_context(document, &query, &TokenizationConfig::default())
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            |_| {},
        )
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
    fn kwic_finds_phrase_and_context_around_full_node() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "alpha not very good movie today not very good ending",
        )];
        let mut query = concordance_query("not very good");
        query.window_tokens = 1;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 2);
        assert_eq!(result.rows[0].left_context, ["alpha"]);
        assert_eq!(result.rows[0].node, "not very good");
        assert_eq!(result.rows[0].right_context, ["movie"]);
        assert_eq!(result.rows[1].left_context, ["today"]);
        assert_eq!(result.rows[1].right_context, ["ending"]);
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
    fn kwic_stats_use_full_hit_set_and_document_token_proportions() {
        let documents = vec![
            concordance_document(0, "a.txt", "alpha alpha"),
            concordance_document(1, "b.txt", "beta beta beta beta beta beta"),
        ];
        let mut query = concordance_query("alpha");
        query.limit = 1;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.total_hits, 2);
        assert_eq!(result.documents_with_hits, 1);
        assert_eq!(result.selected_documents_total, 2);
        assert_eq!(result.selected_tokens_total, 8);
        assert_close(result.dp, 0.75);
        assert_close(result.top_document_share, 1.0);
    }

    #[test]
    fn kwic_context_filter_counts_full_filtered_hit_set() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "red target blue target green target red",
        )];
        let mut query = concordance_query("target");
        query.window_tokens = 1;
        query.limit = 1;
        query.context_filter = Some("blue".to_string());

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.context_filter.as_deref(), Some("blue"));
        assert_eq!(result.total_hits, 2);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].right_context, ["blue"]);
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
    fn kwic_sorts_full_hit_set_before_pagination() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "alpha z alpha a alpha m",
        )];
        let mut query = concordance_query("alpha");
        query.sort_key = ConcordanceSortKey::Right1;
        query.offset = 1;
        query.limit = 1;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 3);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].right_context, ["m"]);
    }

    #[test]
    fn kwic_left_sort_uses_missing_context_last() {
        let documents = vec![concordance_document(0, "doc.txt", "alpha b a alpha")];
        let mut query = concordance_query("alpha");
        query.sort_key = ConcordanceSortKey::Left1;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 2);
        assert_eq!(
            result.rows[0].left_context.last().map(String::as_str),
            Some("a")
        );
        assert!(result.rows[1].left_context.is_empty());
    }

    #[test]
    fn kwic_file_position_sort_ties_by_document_and_offset() {
        let documents = vec![
            concordance_document(1, "b.txt", "alpha beta alpha"),
            concordance_document(0, "a.txt", "beta alpha"),
        ];

        let result = concordance_for(&documents, concordance_query("alpha")).unwrap();

        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| (row.document_index, row.token_index))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 0), (1, 2)]
        );
    }

    #[test]
    fn concordance_default_route_uses_scan_and_matches_public_entry() {
        let documents = vec![concordance_document(0, "doc.txt", "Movie movie MOVIE")];
        let query = concordance_query("movie");
        let public = concordance_for(&documents, query.clone()).unwrap();
        let mut context = ConcordanceSearchContext::default();

        let routed = routed_concordance_for(
            &documents,
            &query,
            &mut context,
            ConcordanceBackend::default(),
        )
        .unwrap();

        assert_eq!(routed.backend_used, ConcordanceBackendUsed::Scan);
        assert_eq!(routed.result, public);
        assert_eq!(context.index_build_count(), 0);
    }

    #[test]
    fn routed_index_concordance_parity_holds_for_required_english_queries() {
        let documents = vec![
            concordance_document(
                0,
                "a.txt",
                "Movie, movie. MOVIE. good movie. Good movie! I love it. i love it.",
            ),
            concordance_document(
                1,
                "b.txt",
                "A movie is not the same as a Good movie? I love it.",
            ),
        ];

        for query in [
            "movie",
            "Movie",
            "MOVIE",
            "good movie",
            "Good movie",
            "I love it",
            "i love it",
        ] {
            assert_routed_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn routed_index_concordance_parity_holds_for_required_portuguese_queries() {
        let documents = vec![
            concordance_document(
                0,
                "pt-a.txt",
                "ação Ação coração Coração não Não João joão maçã Maçã.",
            ),
            concordance_document(1, "pt-b.txt", "A ação toca o coração, e João diz não."),
        ];

        for query in [
            "ação",
            "Ação",
            "coração",
            "Coração",
            "não",
            "Não",
            "João",
            "joão",
        ] {
            assert_routed_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn routed_index_sort_keys_match_scan_beyond_visible_window() {
        let documents = vec![
            concordance_document(0, "a.txt", "z b alpha c y alpha d"),
            concordance_document(1, "b.txt", "a q alpha b r alpha c"),
        ];
        for sort_key in [
            ConcordanceSortKey::Node,
            ConcordanceSortKey::Left1,
            ConcordanceSortKey::Left2,
            ConcordanceSortKey::Right1,
            ConcordanceSortKey::Right2,
        ] {
            let mut query = parity_concordance_query("alpha", false);
            query.sort_key = sort_key;
            query.window_tokens = 1;
            let mut scan_context = ConcordanceSearchContext::default();
            let scan = routed_concordance_for(
                &documents,
                &query,
                &mut scan_context,
                ConcordanceBackend::Scan,
            )
            .unwrap();
            let mut index_context = ConcordanceSearchContext::default();
            let index = routed_concordance_for(
                &documents,
                &query,
                &mut index_context,
                ConcordanceBackend::Index,
            )
            .unwrap();

            assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
            assert_eq!(index.result, scan.result, "{sort_key:?}");
        }
    }

    #[test]
    fn routed_index_parity_holds_for_selection_and_pagination() {
        let documents = vec![
            concordance_document(0, "a.txt", "alpha beta alpha gamma alpha"),
            concordance_document(1, "b.txt", "alpha delta alpha"),
            concordance_document(2, "c.txt", "epsilon alpha alpha alpha"),
        ];
        let mut query = parity_concordance_query("alpha", false);
        query.selection = AnalysisDocumentSelection {
            all_documents: false,
            ids: vec![1, 2],
            excluded_ids: Vec::new(),
        };
        query.offset = 1;
        query.limit = 2;

        let mut scan_context = ConcordanceSearchContext::default();
        let scan = routed_concordance_for(
            &documents,
            &query,
            &mut scan_context,
            ConcordanceBackend::Scan,
        )
        .unwrap();
        let mut index_context = ConcordanceSearchContext::default();
        let index = routed_concordance_for(
            &documents,
            &query,
            &mut index_context,
            ConcordanceBackend::Index,
        )
        .unwrap();

        assert_eq!(scan.backend_used, ConcordanceBackendUsed::Scan);
        assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(index.result, scan.result);
        assert_eq!(index.result.total_hits, 5);
        assert_eq!(index.result.selected_documents_total, 2);
        assert_eq!(index.result.offset, 1);
        assert_eq!(index.result.limit, 2);
        assert_eq!(index.result.rows.len(), 2);
        assert!(index.result.rows.iter().all(|row| row.document_index != 0));
        assert_eq!(
            index
                .result
                .rows
                .iter()
                .map(|row| (row.document_index, row.token_index))
                .collect::<Vec<_>>(),
            [(1, 2), (2, 1)]
        );
    }

    #[test]
    fn routed_index_falls_back_for_context_filter_and_matches_scan() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "red target blue target green target red",
        )];
        let mut query = concordance_query("target");
        query.window_tokens = 1;
        query.context_filter = Some("blue".to_string());
        let scan = concordance_for(&documents, query.clone()).unwrap();
        let mut context = ConcordanceSearchContext::default();

        let routed =
            routed_concordance_for(&documents, &query, &mut context, ConcordanceBackend::Index)
                .unwrap();

        assert_eq!(
            routed.backend_used,
            ConcordanceBackendUsed::ScanFallback {
                reason: ConcordanceFallbackReason::UnsupportedFilter,
            }
        );
        assert_eq!(routed.result, scan);
        assert_eq!(context.index_build_count(), 0);
    }

    #[test]
    fn routed_index_cache_reuses_store_and_rebuilds_when_manifest_changes() {
        let mut documents = vec![
            concordance_document(0, "a.txt", "Movie movie good movie"),
            concordance_document(1, "b.txt", "Good movie"),
        ];
        let mut context = ConcordanceSearchContext::default();
        let movie = parity_concordance_query("movie", false);
        let phrase = parity_concordance_query("good movie", false);

        let first =
            routed_concordance_for(&documents, &movie, &mut context, ConcordanceBackend::Index)
                .unwrap();
        let second =
            routed_concordance_for(&documents, &phrase, &mut context, ConcordanceBackend::Index)
                .unwrap();
        documents[0].text.push_str(" movie");
        let third =
            routed_concordance_for(&documents, &movie, &mut context, ConcordanceBackend::Index)
                .unwrap();

        assert_eq!(first.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(second.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(third.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(context.index_build_count(), 2);
        assert_eq!(first.result.total_hits, 4);
        assert_eq!(second.result.total_hits, 2);
        assert_eq!(third.result.total_hits, 5);
    }

    #[test]
    fn routed_index_windows_1252_portuguese_offsets_and_parity_hold() {
        let bytes = b"caf\xe9 a\xe7\xe3o A\xe7\xe3o Jo\xe3o jo\xe3o cora\xe7\xe3o Cora\xe7\xe3o n\xe3o N\xe3o ma\xe7\xe3 Ma\xe7\xe3";
        let decoded = decode_source_bytes(bytes);
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert!(decoded.had_utf8_error);
        assert_eq!(
            decoded.text,
            "café ação Ação João joão coração Coração não Não maçã Maçã"
        );
        assert!(!decoded.text.contains('\u{fffd}'));

        let index_inputs = [SurfaceIndexDocumentInput {
            document_id: 0,
            source_path: "pt-win1252.txt".to_string(),
            display_name: "pt-win1252.txt".to_string(),
            prepared_text: decoded.text.as_str(),
        }];
        let store =
            InMemoryIndexStore::build(&index_inputs, &TokenizationConfig::default()).unwrap();
        let records = store.records_for_document(0).unwrap();
        for expected_surface in [
            "ação",
            "Ação",
            "João",
            "joão",
            "coração",
            "Coração",
            "não",
            "Não",
            "maçã",
            "Maçã",
        ] {
            let record = records
                .iter()
                .find(|record| record.surface == expected_surface)
                .unwrap();
            let recovered = decoded
                .text
                .chars()
                .skip(record.prepared_char_start)
                .take(record.prepared_char_end - record.prepared_char_start)
                .collect::<String>();
            assert_eq!(recovered, expected_surface);
        }
        let acao = records
            .iter()
            .find(|record| record.surface == "ação")
            .unwrap();
        assert_eq!((acao.prepared_char_start, acao.prepared_char_end), (5, 9));

        let concordance_documents = vec![concordance_document(
            0,
            "pt-win1252.txt",
            decoded.text.as_str(),
        )];
        for query in [
            "ação",
            "Ação",
            "coração",
            "Coração",
            "não",
            "Não",
            "João",
            "joão",
        ] {
            assert_routed_index_parity_both_modes(&concordance_documents, query);
        }
    }

    #[test]
    fn concordance_routing_types_remain_rust_only_without_ts_export() {
        let source = include_str!("mod.rs");
        let start = source
            .find("pub enum ConcordanceBackend")
            .expect("routing type source should exist");
        let end = source
            .find("#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ConcordanceError")
            .expect("concordance error source should exist");
        let routing_source = &source[start..end];

        assert!(!routing_source.contains("#[ts(export)]"));
        assert!(!routing_source.contains("TS"));
    }

    #[test]
    fn kwic_distribution_counts_and_paginates_after_sorting() {
        let documents = vec![
            concordance_document(0, "c.txt", "alpha beta"),
            concordance_document(1, "a.txt", "alpha alpha beta"),
            concordance_document(2, "b.txt", "beta beta"),
        ];
        let mut query = concordance_query("alpha");
        query.distribution_offset = 1;
        query.distribution_limit = 1;

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.distribution.total_documents, 3);
        assert_eq!(result.distribution.offset, 1);
        assert_eq!(result.distribution.items.len(), 1);
        assert_eq!(result.distribution.items[0].display_name, "c.txt");
        assert_eq!(result.distribution.items[0].count, 1);
        assert_eq!(result.distribution.items[0].token_count, 2);
        assert_eq!(result.distribution.max_count, 2);
    }

    #[test]
    fn kwic_honours_selected_file_scope() {
        let documents = vec![
            concordance_document(0, "a.txt", "alpha alpha"),
            concordance_document(1, "b.txt", "alpha beta"),
            concordance_document(2, "c.txt", "alpha gamma"),
        ];
        let mut query = concordance_query("alpha");
        query.selection = AnalysisDocumentSelection {
            all_documents: false,
            ids: vec![1, 2],
            excluded_ids: Vec::new(),
        };

        let result = concordance_for(&documents, query).unwrap();

        assert_eq!(result.total_hits, 2);
        assert_eq!(result.selected_documents_total, 2);
        assert_eq!(
            result
                .rows
                .iter()
                .map(|row| row.display_name.as_str())
                .collect::<Vec<_>>(),
            ["b.txt", "c.txt"]
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

    #[test]
    fn kwic_expansion_returns_wider_context_for_same_hit() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "zero one two target phrase three four five six",
        )];
        let mut query = concordance_query("target phrase");
        query.window_tokens = 1;
        let result = concordance_for(&documents, query).unwrap();
        let row = &result.rows[0];

        let expansion = concordance_expansion_for(
            &documents[0],
            ConcordanceExpansionQuery {
                document_index: row.document_index,
                token_index: row.token_index,
                node_token_length: row.node_token_length,
                window_tokens: 3,
            },
        )
        .unwrap();

        assert!(expansion.left_context.len() > row.left_context.len());
        assert!(expansion.right_context.len() > row.right_context.len());
        assert_eq!(expansion.node_tokens, ["target", "phrase"]);
        assert_eq!(expansion.node, "target phrase");
    }

    #[test]
    fn kwic_expansion_clamps_at_document_start() {
        let document = concordance_document(0, "doc.txt", "alpha beta gamma delta");

        let expansion = concordance_expansion_for(
            &document,
            ConcordanceExpansionQuery {
                document_index: 0,
                token_index: 0,
                node_token_length: 1,
                window_tokens: 40,
            },
        )
        .unwrap();

        assert!(expansion.left_context.is_empty());
        assert_eq!(expansion.node_tokens, ["alpha"]);
        assert_eq!(expansion.right_context, ["beta", "gamma", "delta"]);
    }

    #[test]
    fn kwic_expansion_clamps_at_document_end() {
        let document = concordance_document(0, "doc.txt", "alpha beta gamma delta");

        let expansion = concordance_expansion_for(
            &document,
            ConcordanceExpansionQuery {
                document_index: 0,
                token_index: 3,
                node_token_length: 1,
                window_tokens: 40,
            },
        )
        .unwrap();

        assert_eq!(expansion.left_context, ["alpha", "beta", "gamma"]);
        assert_eq!(expansion.node_tokens, ["delta"]);
        assert!(expansion.right_context.is_empty());
    }

    #[test]
    fn kwic_expansion_rejects_invalid_identity_without_panic() {
        let document = concordance_document(0, "doc.txt", "alpha beta");

        let document_error = concordance_expansion_for(
            &document,
            ConcordanceExpansionQuery {
                document_index: 1,
                token_index: 0,
                node_token_length: 1,
                window_tokens: 40,
            },
        )
        .unwrap_err();
        assert_eq!(document_error, ConcordanceError::InvalidDocumentIdentity);

        let hit_error = concordance_expansion_for(
            &document,
            ConcordanceExpansionQuery {
                document_index: 0,
                token_index: 1,
                node_token_length: 2,
                window_tokens: 40,
            },
        )
        .unwrap_err();
        assert_eq!(hit_error, ConcordanceError::InvalidHitIdentity);
    }

    #[test]
    fn kwic_expansion_retokenises_one_supplied_document() {
        let documents = [
            concordance_document(0, "a.txt", "alpha beta gamma"),
            concordance_document(1, "b.txt", "beta should not be inspected"),
        ];

        let expansion = concordance_expansion_for(
            &documents[0],
            ConcordanceExpansionQuery {
                document_index: 0,
                token_index: 1,
                node_token_length: 1,
                window_tokens: 40,
            },
        )
        .unwrap();

        assert_eq!(expansion.display_name, "a.txt");
        assert_eq!(expansion.node_tokens, ["beta"]);
        assert_eq!(expansion.right_context, ["gamma"]);
    }

    #[test]
    fn surface_collocations_compute_window_counts_and_scores() {
        let documents = vec![
            concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
            concordance_document(1, "b.txt", "yellow target blue green"),
        ];

        let result = collocations_for(&documents, collocation_query("target")).unwrap();
        let blue = collocate(&result, "blue");

        assert_eq!(result.f_node, 2);
        assert_eq!(result.r1, 7);
        assert_eq!(result.n, 11);
        assert_eq!(blue.o11, 2);
        assert_eq!(blue.o12, 5);
        assert_eq!(blue.o21, 1);
        assert_eq!(blue.o22, 3);
        assert_eq!(blue.collocate_frequency, 3);
        // Hand values for blue: E11 = 7 * 3 / 11 = 1.9091; O12/O21/O22 = 5/1/3.
        assert_close_4dp(blue.mi, 0.0671141959);
        assert_close_4dp(blue.mi3, 2.0671141959);
        assert_close_4dp(blue.t_score, 0.0642824347);
        assert_close_4dp(blue.z_score, 0.0657951695);
        assert_close_4dp(blue.g2, 0.0165022055);
        assert_close_4dp(blue.log_dice, 12.6780719051);
        assert_close_4dp(blue.log_ratio, 0.1926450779);
    }

    #[test]
    fn surface_collocation_floors_exclude_below_threshold_candidates() {
        let documents = vec![
            concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
            concordance_document(1, "b.txt", "yellow target blue green"),
        ];
        let mut query = collocation_query("target");
        query.min_cooccurrence = 2;
        query.min_collocate_frequency = 2;

        let result = collocations_for(&documents, query).unwrap();

        let surfaces = result
            .rows
            .iter()
            .map(|row| row.surface.as_str())
            .collect::<Vec<_>>();
        assert_eq!(surfaces.len(), 3);
        assert!(surfaces.contains(&"alpha"));
        assert!(surfaces.contains(&"blue"));
        assert!(surfaces.contains(&"green"));
        assert!(!surfaces.contains(&"yellow"));
    }

    #[test]
    fn surface_collocation_direction_changes_window_counts() {
        let documents = vec![
            concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
            concordance_document(1, "b.txt", "yellow target blue green"),
        ];
        let mut left_query = collocation_query("target");
        left_query.direction = ConcordanceCollocationDirection::Left;
        let mut right_query = collocation_query("target");
        right_query.direction = ConcordanceCollocationDirection::Right;
        let both_query = collocation_query("target");

        let left = collocations_for(&documents, left_query).unwrap();
        let right = collocations_for(&documents, right_query).unwrap();
        let both = collocations_for(&documents, both_query).unwrap();

        assert_eq!(left.r1, 3);
        assert_eq!(right.r1, 4);
        assert_eq!(both.r1, 7);
        assert_eq!(collocate(&left, "blue").o11, 1);
        assert_eq!(collocate(&right, "blue").o11, 1);
        assert_eq!(collocate(&both, "blue").o11, 2);
    }

    #[test]
    fn surface_collocations_do_not_return_node_tokens_as_collocates() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "target target alpha target",
        )];
        let mut query = collocation_query("target");
        query.span = 1;

        let result = collocations_for(&documents, query).unwrap();

        assert_eq!(result.f_node, 3);
        assert!(result.rows.iter().all(|row| row.surface != "target"));
        assert_eq!(collocate(&result, "alpha").o11, 2);
    }

    #[test]
    fn surface_collocation_phrase_windows_wrap_the_whole_node_span() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "pre alpha beta post mid alpha beta tail",
        )];
        let mut query = collocation_query("alpha beta");
        query.span = 1;

        let result = collocations_for(&documents, query).unwrap();

        assert_eq!(result.f_node, 2);
        assert_eq!(result.r1, 4);
        assert_eq!(collocate(&result, "pre").o11, 1);
        assert_eq!(collocate(&result, "post").o11, 1);
        assert_eq!(collocate(&result, "mid").o11, 1);
        assert_eq!(collocate(&result, "tail").o11, 1);
        assert!(result.rows.iter().all(|row| row.surface != "alpha"));
        assert!(result.rows.iter().all(|row| row.surface != "beta"));
    }

    #[test]
    fn surface_collocation_sort_key_ranks_before_limit() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "rare target common target common target common common common common common common common common common common",
        )];
        let mut association_query = collocation_query("target");
        association_query.direction = ConcordanceCollocationDirection::Left;
        association_query.limit = 1;

        let mut near_query = association_query.clone();
        near_query.sort_key = ConcordanceCollocationSortKey::NearQuery;

        let mut corpus_frequency_query = association_query.clone();
        corpus_frequency_query.sort_key = ConcordanceCollocationSortKey::CorpusFrequency;

        let association = collocations_for(&documents, association_query).unwrap();
        let near = collocations_for(&documents, near_query).unwrap();
        let corpus_frequency = collocations_for(&documents, corpus_frequency_query).unwrap();

        assert_eq!(association.rows.len(), 1);
        assert_eq!(near.rows.len(), 1);
        assert_eq!(corpus_frequency.rows.len(), 1);
        assert_eq!(association.rows[0].surface, "rare");
        assert_eq!(near.rows[0].surface, "common");
        assert_eq!(corpus_frequency.rows[0].surface, "common");
    }

    #[test]
    fn surface_collocation_aggregate_post_processing_matches_full_query() {
        let documents = vec![
            concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
            concordance_document(1, "b.txt", "yellow target blue green"),
        ];
        let query = collocation_query("target");

        let one_shot = collocations_for(&documents, query.clone()).unwrap();
        let aggregate = collocation_aggregate_for(&documents, &query).unwrap();
        let from_aggregate = query_concordance_collocations_from_aggregate(&aggregate, &query);

        assert_eq!(from_aggregate.query, one_shot.query);
        assert_eq!(from_aggregate.r1, one_shot.r1);
        assert_eq!(from_aggregate.n, one_shot.n);
        assert_eq!(from_aggregate.f_node, one_shot.f_node);
        assert_eq!(from_aggregate.total_candidates, one_shot.total_candidates);
        assert_eq!(from_aggregate.rows, one_shot.rows);
    }

    #[test]
    fn surface_collocation_presentation_changes_reuse_full_aggregate() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "rare target common target common target common common common common common common common common common common",
        )];
        let mut query = collocation_query("target");
        query.direction = ConcordanceCollocationDirection::Left;
        query.limit = 1;
        let aggregate = collocation_aggregate_for(&documents, &query).unwrap();

        let association = query_concordance_collocations_from_aggregate(&aggregate, &query);
        let mut near_query = query.clone();
        near_query.sort_key = ConcordanceCollocationSortKey::NearQuery;
        let near = query_concordance_collocations_from_aggregate(&aggregate, &near_query);
        let mut corpus_frequency_query = query.clone();
        corpus_frequency_query.sort_key = ConcordanceCollocationSortKey::CorpusFrequency;
        let corpus_frequency =
            query_concordance_collocations_from_aggregate(&aggregate, &corpus_frequency_query);

        assert_eq!(association.rows[0].surface, "rare");
        assert_eq!(near.rows[0].surface, "common");
        assert_eq!(corpus_frequency.rows[0].surface, "common");
    }

    #[test]
    fn surface_collocation_stopwords_filter_after_aggregation() {
        let documents = vec![concordance_document(
            0,
            "doc.txt",
            "the target useful target the target useful",
        )];
        let mut query = collocation_query("target");
        query.span = 1;
        query.min_cooccurrence = 1;
        query.min_collocate_frequency = 1;
        query.sort_key = ConcordanceCollocationSortKey::NearQuery;
        let aggregate = collocation_aggregate_for(&documents, &query).unwrap();

        let shown = query_concordance_collocations_from_aggregate(&aggregate, &query);
        let mut hidden_query = query.clone();
        hidden_query.hide_stopwords = true;
        let hidden = query_concordance_collocations_from_aggregate(&aggregate, &hidden_query);

        assert!(shown.rows.iter().any(|row| row.surface == "the"));
        assert!(hidden.rows.iter().all(|row| row.surface != "the"));
        assert!(hidden.rows.iter().any(|row| row.surface == "useful"));
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
            query: String::new(),
            match_mode: FrequencyMatchMode::Contains,
            minimum_frequency: 1,
            hide_stopwords: false,
            case_fold: false,
            pos_filter: None,
            sort_key: FrequencySortKey::Frequency,
            sort_direction: SortDirection::Desc,
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
            None,
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
            &std::sync::atomic::AtomicUsize::new(0),
        );

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
    fn annotation_local_counts_group_duplicates_by_surface_lemma_and_upos() {
        let corpus = corpus_for(&[("grouping.txt", "Alpha alpha Alpha")]);
        let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
        let upper_id = *surface_lookup.get("Alpha").unwrap();
        let lower_id = *surface_lookup.get("alpha").unwrap();
        let annotated = vec![
            AnnotatedToken {
                surface: "Alpha".to_string(),
                lemma: "alpha".to_string(),
                upos: UniversalPos::Noun,
                token_index: 0,
                features: HashMap::new(),
            },
            AnnotatedToken {
                surface: "alpha".to_string(),
                lemma: "alpha".to_string(),
                upos: UniversalPos::Noun,
                token_index: 1,
                features: HashMap::new(),
            },
            AnnotatedToken {
                surface: "Alpha".to_string(),
                lemma: "alpha".to_string(),
                upos: UniversalPos::Noun,
                token_index: 2,
                features: HashMap::new(),
            },
            AnnotatedToken {
                surface: "Alpha".to_string(),
                lemma: "alpha".to_string(),
                upos: UniversalPos::Verb,
                token_index: 3,
                features: HashMap::new(),
            },
        ];

        let counts =
            resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup).unwrap();
        let mut expected = vec![
            LocalAnnotatedTermCount {
                surface_term_id: upper_id,
                lemma: "alpha".to_string(),
                upos: UniversalPos::Noun,
                count: 2,
            },
            LocalAnnotatedTermCount {
                surface_term_id: upper_id,
                lemma: "alpha".to_string(),
                upos: UniversalPos::Verb,
                count: 1,
            },
            LocalAnnotatedTermCount {
                surface_term_id: lower_id,
                lemma: "alpha".to_string(),
                upos: UniversalPos::Noun,
                count: 1,
            },
        ];
        expected.sort_by(|left, right| {
            left.surface_term_id
                .cmp(&right.surface_term_id)
                .then_with(|| left.lemma.cmp(&right.lemma))
                .then_with(|| left.upos.as_upos_str().cmp(right.upos.as_upos_str()))
        });

        assert_eq!(counts, expected);
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
    fn lemma_pos_frequency_honours_filters_before_pagination() {
        let mut corpus = corpus_for(&[
            ("a.txt", "Alpha alpha the run"),
            ("b.txt", "ALPHA the run run"),
        ]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Alpha", "Alpha", UniversalPos::Noun, 1),
                ("alpha", "alpha", UniversalPos::Noun, 1),
                ("the", "the", UniversalPos::Det, 1),
                ("run", "run", UniversalPos::Verb, 1),
            ],
        );
        add_document_annotations(
            &mut corpus,
            1,
            &[
                ("ALPHA", "alpha", UniversalPos::Noun, 1),
                ("the", "the", UniversalPos::Det, 1),
                ("run", "run", UniversalPos::Verb, 2),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &AnnotatedFrequencyQuery {
                query: "ALPHA".to_string(),
                match_mode: FrequencyMatchMode::Exact,
                case_fold: true,
                pos_filter: Some("noun".to_string()),
                minimum_frequency: 2,
                limit: 1,
                ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
            },
            ready_annotation_cache(),
        );

        assert_eq!(result.total_rows, 1);
        assert_eq!(result.lex_rows.len(), 1);
        assert_eq!(result.lex_rows[0].lemma, "alpha");
        assert_eq!(result.lex_rows[0].upos, "NOUN");
        assert_eq!(result.lex_rows[0].raw, 2);
    }

    #[test]
    fn lemma_pos_frequency_hides_stopwords_by_lemma() {
        let mut corpus = corpus_for(&[("a.txt", "The work"), ("b.txt", "the work")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("The", "The", UniversalPos::Det, 1),
                ("work", "work", UniversalPos::Noun, 1),
            ],
        );
        add_document_annotations(
            &mut corpus,
            1,
            &[
                ("the", "the", UniversalPos::Det, 1),
                ("work", "work", UniversalPos::Verb, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &AnnotatedFrequencyQuery {
                hide_stopwords: true,
                sort_key: FrequencySortKey::Token,
                sort_direction: SortDirection::Asc,
                ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
            },
            ready_annotation_cache(),
        );

        let rows = result
            .lex_rows
            .iter()
            .map(|row| (row.lemma.as_str(), row.upos.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(result.total_rows, 2);
        assert_eq!(rows, vec![("work", "NOUN"), ("work", "VERB")]);
    }

    #[test]
    fn annotated_frequency_sorts_with_shared_keys_and_direction() {
        let mut corpus = corpus_for(&[("a.txt", "alpha beta beta"), ("b.txt", "gamma gamma")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("alpha", "alpha", UniversalPos::Noun, 1),
                ("beta", "beta", UniversalPos::Noun, 2),
            ],
        );
        add_document_annotations(&mut corpus, 1, &[("gamma", "gamma", UniversalPos::Verb, 2)]);

        let result = query_annotated_frequency(
            &corpus,
            &AnnotatedFrequencyQuery {
                sort_key: FrequencySortKey::Frequency,
                sort_direction: SortDirection::Asc,
                ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
            },
            ready_annotation_cache(),
        );

        let rows = result
            .lex_rows
            .iter()
            .map(|row| (row.lemma.as_str(), row.raw))
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![("alpha", 1), ("beta", 2), ("gamma", 2)]);
    }

    #[test]
    fn pos_frequency_query_matches_upos_case_insensitively() {
        let mut corpus = corpus_for(&[("a.txt", "Cats run quickly")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
                ("quickly", "quickly", UniversalPos::Adv, 1),
            ],
        );

        let result = query_annotated_frequency(
            &corpus,
            &AnnotatedFrequencyQuery {
                query: "verb".to_string(),
                match_mode: FrequencyMatchMode::Exact,
                ..annotated_frequency_query(AnnotatedFrequencyMode::Pos)
            },
            ready_annotation_cache(),
        );

        assert_eq!(result.total_rows, 1);
        assert_eq!(result.pos_rows[0].upos, "VERB");
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
    fn annotated_term_distribution_counts_pos_and_lemma_pos_per_document() {
        let mut corpus = corpus_for(&[("a.txt", "Cats run run"), ("b.txt", "cat run")]);
        add_document_annotations(
            &mut corpus,
            0,
            &[
                ("Cats", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 2),
            ],
        );
        add_document_annotations(
            &mut corpus,
            1,
            &[
                ("cat", "cat", UniversalPos::Noun, 1),
                ("run", "run", UniversalPos::Verb, 1),
            ],
        );

        let pos_distribution = query_annotated_term_distribution(
            &corpus,
            &AnnotatedTermDistributionQuery {
                selection: AnalysisDocumentSelection::default(),
                selector: AnnotatedTermSelector::Pos {
                    upos: "VERB".to_string(),
                },
                offset: 0,
                limit: 10,
            },
        );
        let lemma_distribution = query_annotated_term_distribution(
            &corpus,
            &AnnotatedTermDistributionQuery {
                selection: AnalysisDocumentSelection::default(),
                selector: AnnotatedTermSelector::LemmaPos {
                    lemma: "cat".to_string(),
                    upos: "NOUN".to_string(),
                },
                offset: 0,
                limit: 10,
            },
        );

        assert_eq!(pos_distribution.total_documents, 2);
        assert_eq!(
            pos_distribution
                .items
                .iter()
                .map(|item| (item.relative_path.as_str(), item.count))
                .collect::<Vec<_>>(),
            vec![("a.txt", 2), ("b.txt", 1)]
        );
        assert_close(pos_distribution.items[0].rate_per_10k, 20_000.0 / 3.0);
        assert_eq!(
            lemma_distribution
                .items
                .iter()
                .map(|item| (item.relative_path.as_str(), item.count))
                .collect::<Vec<_>>(),
            vec![("a.txt", 1), ("b.txt", 1)]
        );
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            &std::sync::atomic::AtomicUsize::new(0),
        )
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
            |_| {},
        )
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
            |_| {},
        )
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
            |_| {},
        )
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
            |_| {},
        )
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
            |_| {},
        )
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
    fn test_annotation_timing_does_not_change_cache_output() {
        struct TimingReset;
        impl Drop for TimingReset {
            fn drop(&mut self) {
                TEST_ANNOTATION_TIMING_ENABLED.store(false, AtomicOrdering::Relaxed);
            }
        }

        let _reset = TimingReset;
        let records = vec![record("doc.txt")];

        let build_cache = |cache_root: &std::path::Path| {
            let mut options = build_options();
            options.annotation_cache_root = Some(cache_root.to_path_buf());
            options.udpipe_model_dir = Some(default_model_dir());
            let build_ann_options = AnnotationLayerBuildOptions {
                udpipe_model_dir: options.udpipe_model_dir.clone(),
                language_overrides: options.language_overrides.clone(),
            };

            build_and_write_annotation_cache(
                &records,
                options,
                build_ann_options,
                &AtomicBool::new(false),
                |record| Ok(processed_resolution(record, "The quick brown fox.")),
                |_| {},
            )
            .unwrap();

            let cache_file = cache_root.join(format!(
                "annotation-layer.v{}.json",
                crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
            ));
            let bytes = std::fs::read(cache_file).unwrap();
            serde_json::from_slice::<crate::annotation_cache::AnnotationLayerCacheEnvelope>(&bytes)
                .unwrap()
        };

        let untimed_dir = tempfile::tempdir().unwrap();
        TEST_ANNOTATION_TIMING_ENABLED.store(false, AtomicOrdering::Relaxed);
        let untimed = build_cache(untimed_dir.path());

        let timed_dir = tempfile::tempdir().unwrap();
        TEST_ANNOTATION_TIMING_ENABLED.store(true, AtomicOrdering::Relaxed);
        let timed = build_cache(timed_dir.path());

        assert_eq!(untimed, timed);
    }

    #[test]
    fn test_reused_surface_corpus_skips_rebuild_and_matches_rebuild_output() {
        let records = vec![record("doc.txt")];
        let text = "The quick brown fox.";
        let mut surface_options = build_options();
        surface_options.udpipe_model_dir = Some(default_model_dir());
        surface_options.force_compute_hashes = true;
        let active_corpus = build_analysis_corpus(
            &records,
            surface_options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, text)),
            |_| {},
        )
        .unwrap();

        let reuse_dir = tempfile::tempdir().unwrap();
        let mut reuse_options = surface_options.clone();
        reuse_options.annotation_cache_root = Some(reuse_dir.path().to_path_buf());
        let reuse_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: reuse_options.udpipe_model_dir.clone(),
            language_overrides: reuse_options.language_overrides.clone(),
        };
        let reuse_load_calls = Arc::new(AtomicUsize::new(0));
        let reuse_load_calls_for_closure = reuse_load_calls.clone();
        let mut reuse_progress = Vec::new();
        let reuse_report = build_and_write_annotation_cache_with_reusable_surface_corpus(
            &records,
            reuse_options,
            reuse_ann_options,
            &AtomicBool::new(false),
            |record| {
                reuse_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
                Ok(processed_resolution(record, text))
            },
            |progress| reuse_progress.push(progress),
            Some(&active_corpus),
        )
        .unwrap();
        assert!(reuse_report.cache_written);
        assert_eq!(reuse_load_calls.load(AtomicOrdering::Relaxed), 1);
        assert!(reuse_progress.iter().all(|progress| {
            !matches!(
                progress.current_phase,
                AnnotationCacheBuildPhase::BuildingSurface { .. }
            )
        }));

        let ready_load_calls = Arc::new(AtomicUsize::new(0));
        let ready_load_calls_for_closure = ready_load_calls.clone();
        let mut ready_options = surface_options.clone();
        ready_options.annotation_cache_root = Some(reuse_dir.path().to_path_buf());
        let ready_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: ready_options.udpipe_model_dir.clone(),
            language_overrides: ready_options.language_overrides.clone(),
        };
        let ready_report = build_and_write_annotation_cache_with_reusable_surface_corpus(
            &records,
            ready_options,
            ready_ann_options,
            &AtomicBool::new(false),
            |record| {
                ready_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
                Ok(processed_resolution(record, text))
            },
            |_| {},
            Some(&active_corpus),
        )
        .unwrap();
        assert!(ready_report.reused_existing);
        assert_eq!(ready_load_calls.load(AtomicOrdering::Relaxed), 0);

        let rebuild_dir = tempfile::tempdir().unwrap();
        let mut rebuild_options = surface_options.clone();
        rebuild_options.annotation_cache_root = Some(rebuild_dir.path().to_path_buf());
        let rebuild_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: rebuild_options.udpipe_model_dir.clone(),
            language_overrides: rebuild_options.language_overrides.clone(),
        };
        let rebuild_load_calls = Arc::new(AtomicUsize::new(0));
        let rebuild_load_calls_for_closure = rebuild_load_calls.clone();
        let rebuild_report = build_and_write_annotation_cache(
            &records,
            rebuild_options,
            rebuild_ann_options,
            &AtomicBool::new(false),
            |record| {
                rebuild_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
                Ok(processed_resolution(record, text))
            },
            |_| {},
        )
        .unwrap();
        assert!(rebuild_report.cache_written);
        assert_eq!(rebuild_load_calls.load(AtomicOrdering::Relaxed), 2);
        assert_eq!(
            read_annotation_cache_envelope(reuse_dir.path()),
            read_annotation_cache_envelope(rebuild_dir.path())
        );
    }

    #[test]
    fn test_reusable_surface_corpus_gate_rejects_identity_mismatches() {
        let records = vec![record("doc.txt")];
        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        options.force_compute_hashes = true;
        let corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();
        assert!(
            validate_reusable_surface_corpus(
                &corpus,
                &records,
                &options,
                &options.language_overrides
            )
            .is_ok()
        );

        let other_records = vec![record("other.txt")];
        assert!(
            validate_reusable_surface_corpus(
                &corpus,
                &other_records,
                &options,
                &options.language_overrides
            )
            .is_err()
        );

        let mut build_signature_mismatch = options.clone();
        build_signature_mismatch.build_signature = "different".to_string();
        assert!(
            validate_reusable_surface_corpus(
                &corpus,
                &records,
                &build_signature_mismatch,
                &options.language_overrides
            )
            .is_err()
        );

        let mut missing_hash = corpus.clone();
        missing_hash.documents[0].processed_hash_sha256.clear();
        assert!(
            validate_reusable_surface_corpus(
                &missing_hash,
                &records,
                &options,
                &options.language_overrides
            )
            .is_err()
        );

        let mut invalid_hash = corpus.clone();
        invalid_hash.documents[0].processed_hash_sha256 = "z".repeat(64);
        assert!(
            validate_reusable_surface_corpus(
                &invalid_hash,
                &records,
                &options,
                &options.language_overrides
            )
            .is_err()
        );

        let mut cleaning_mismatch = options.clone();
        cleaning_mismatch.processing_config_signature = "different".to_string();
        assert!(
            validate_reusable_surface_corpus(
                &corpus,
                &records,
                &cleaning_mismatch,
                &options.language_overrides
            )
            .is_err()
        );

        let mut tokenisation_mismatch = options.clone();
        tokenisation_mismatch
            .tokenization
            .retain_ascii_hyphen_compounds = !tokenisation_mismatch
            .tokenization
            .retain_ascii_hyphen_compounds;
        assert!(
            validate_reusable_surface_corpus(
                &corpus,
                &records,
                &tokenisation_mismatch,
                &options.language_overrides
            )
            .is_err()
        );

        let mut override_mismatch = HashMap::new();
        override_mismatch.insert(0, "pt".to_string());
        assert!(
            validate_reusable_surface_corpus(&corpus, &records, &options, &override_mismatch)
                .is_err()
        );

        let mut missing_language = corpus.clone();
        missing_language.documents[0].summary.language = None;
        assert!(
            validate_reusable_surface_corpus(
                &missing_language,
                &records,
                &options,
                &options.language_overrides
            )
            .is_err()
        );
    }

    #[test]
    fn test_rejected_surface_corpus_reuse_falls_back_to_rebuild() {
        let records = vec![record("doc.txt")];
        let text = "The quick brown fox.";
        let mut options = build_options();
        options.udpipe_model_dir = Some(default_model_dir());
        options.force_compute_hashes = true;
        let mut invalid_reuse_corpus = build_analysis_corpus(
            &records,
            options.clone(),
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, text)),
            |_| {},
        )
        .unwrap();
        invalid_reuse_corpus.documents[0]
            .processed_hash_sha256
            .clear();

        let temp_dir = tempfile::tempdir().unwrap();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };
        let load_calls = Arc::new(AtomicUsize::new(0));
        let load_calls_for_closure = load_calls.clone();
        let mut progress = Vec::new();
        let report = build_and_write_annotation_cache_with_reusable_surface_corpus(
            &records,
            options,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
                Ok(processed_resolution(record, text))
            },
            |event| progress.push(event),
            Some(&invalid_reuse_corpus),
        )
        .unwrap();

        assert!(report.cache_written);
        assert_eq!(load_calls.load(AtomicOrdering::Relaxed), 2);
        assert!(progress.iter().any(|event| matches!(
            event.current_phase,
            AnnotationCacheBuildPhase::BuildingSurface { .. }
        )));
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
            |_| {},
        )
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
            |_| {},
        )
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
    fn test_annotation_cache_build_reports_phase_progress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![record("doc1.txt"), record("doc2.txt")];
        let mut options = build_options();
        options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());

        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        let mut progress_events = Vec::new();
        let report = build_and_write_annotation_cache(
            &records,
            options,
            build_ann_options,
            &AtomicBool::new(false),
            |record| {
                if record.source_path.to_string_lossy().contains("doc1") {
                    Ok(processed_resolution(record, "The quick brown fox."))
                } else {
                    Ok(processed_resolution(record, "The slow red fox."))
                }
            },
            |progress| progress_events.push(progress),
        )
        .unwrap();

        assert_eq!(report.documents_annotated, 2);
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::BuildingSurface { .. }
        )));
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::LoadingModel
        )));
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::Annotating
        )));
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::WritingCache
        )));
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::Completed
        )));

        let annotating_done = progress_events
            .iter()
            .filter_map(|progress| match progress.current_phase {
                AnnotationCacheBuildPhase::Annotating => Some(progress.files_done),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!annotating_done.is_empty());
        assert!(
            annotating_done
                .windows(2)
                .all(|window| window[0] <= window[1])
        );
        assert_eq!(annotating_done.last(), Some(&2));
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
            |_| {},
        )
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
            |_| {},
        );

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
            |_| {},
        )
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
            |_| {},
        );

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

        let mut progress_events = Vec::new();
        let cancel = AtomicBool::new(true);
        let res = build_and_write_annotation_cache(
            &records,
            options.clone(),
            build_ann_options,
            &cancel,
            |record| Ok(processed_resolution(record, "Hello world.")),
            |progress| progress_events.push(progress),
        );

        assert!(matches!(res, Err(AnnotationCacheBuildError::Cancelled)));
        assert!(progress_events.iter().any(|progress| matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::Cancelled
        )));

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
            |_| {},
        )
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
