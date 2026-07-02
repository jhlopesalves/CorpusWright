use super::shared::MAX_QUERY_WINDOW;
use super::{AnalysisDocumentSelection, annotated_dispersion_label, case_fold_surface};
use crate::hash::sha256_hex;
use crate::index::{
    InMemoryIndexStore, IndexStore, SurfaceIndexDocumentInput, SurfaceIndexError,
    SurfaceIndexKwicRow, SurfaceIndexManifest, SurfaceIndexQuery, search_surface_index_kwic,
};
use crate::tokenization::{TokenizationConfig, TokenizationManifest, tokenize};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use ts_rs::TS;

const CONCORDANCE_DEFAULT_WINDOW_TOKENS: usize = 5;
const CONCORDANCE_MAX_WINDOW_TOKENS: usize = 20;
const CONCORDANCE_DEFAULT_EXPANSION_WINDOW_TOKENS: usize = 40;
const CONCORDANCE_DEFAULT_LIMIT: usize = 100;
const CONCORDANCE_MAX_LIMIT: usize = 500;
const CONCORDANCE_DEFAULT_DISTRIBUTION_LIMIT: usize = 10;

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
    pub(super) fn index_build_count(&self) -> usize {
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

pub(super) fn analysis_word_tokens<'text>(
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

pub(super) fn selected_concordance_document_ids(
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

pub(super) fn resolve_concordance_query_tokens(
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
}
