use super::build::AnalysisDocumentSelection;
use super::frequency::{annotated_dispersion_label, case_fold_surface};
use super::shared::MAX_QUERY_WINDOW;
use crate::index::{
    InMemoryIndexStore, IndexStore, SurfaceIndexBuilder, SurfaceIndexCacheLoad,
    SurfaceIndexDocumentInput, SurfaceIndexError, SurfaceIndexHit, SurfaceIndexManifest,
    SurfaceIndexQuery, SurfaceIndexResourceLimits, load_surface_index_cache,
    materialize_surface_index_kwic_row, write_surface_index_cache,
};
use crate::tokenization::{
    Token, TokenizationConfig, TokenizationManifest, tokenize, visit_tokens,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use ts_rs::TS;

const CONCORDANCE_DEFAULT_WINDOW_TOKENS: usize = 5;
const CONCORDANCE_MAX_WINDOW_TOKENS: usize = 20;
const CONCORDANCE_DEFAULT_EXPANSION_WINDOW_TOKENS: usize = 40;
const CONCORDANCE_DEFAULT_LIMIT: usize = 100;
const CONCORDANCE_MAX_LIMIT: usize = 500;
const CONCORDANCE_DEFAULT_DISTRIBUTION_LIMIT: usize = 10;

/// Phases of concordance indexing and collocation aggregation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordancePhase {
    BuildingIndex,
    Searching,
    AggregatingCollocations,
}

/// Progress reported during concordance index preparation or searching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub current_phase: ConcordancePhase,
}


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

/// Side of the node that a context filter may match.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordanceContextFilterDirection {
    Left,
    Right,
    #[default]
    Both,
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
    pub context_filter_direction: ConcordanceContextFilterDirection,
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
    IndexError,
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
    pub execution_stats: ConcordanceExecutionStats,
}

#[derive(Debug, Default)]
pub struct ConcordanceSearchContext {
    index_cache: Option<ConcordanceIndexCacheEntry>,
    index_build_count: usize,
    index_cache_load_count: usize,
    cache_root: Option<PathBuf>,
}

impl ConcordanceSearchContext {
    pub fn with_cache_root(cache_root: PathBuf) -> Self {
        Self {
            cache_root: Some(cache_root),
            ..Self::default()
        }
    }

    pub(super) fn index_store_for(
        &mut self,
        documents: &[ConcordanceDocument],
        _selected_document_ids: &HashSet<usize>,
        tokenization_config: &TokenizationConfig,
    ) -> Result<(&InMemoryIndexStore, bool), ConcordanceError> {
        let manifest = concordance_surface_index_manifest(documents, tokenization_config)?;
        let cache_valid = self
            .index_cache
            .as_ref()
            .is_some_and(|entry| entry.manifest.is_valid_for(&manifest));

        let mut index_rebuilt = false;
        if !cache_valid {
            let mut inputs = Vec::new();
            inputs.try_reserve(documents.len()).map_err(|error| {
                ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                    "index document inputs: {error}"
                )))
            })?;
            inputs.extend(documents.iter().map(|document| SurfaceIndexDocumentInput {
                document_id: document.document_index,
                source_path: document.source_path.clone(),
                display_name: document.display_name.clone(),
                prepared_text: document.text.as_str(),
            }));
            inputs.sort_by_key(|document| document.document_id);
            let loaded_store = self.cache_root.as_deref().and_then(|cache_root| {
                match load_surface_index_cache(cache_root, &manifest) {
                    SurfaceIndexCacheLoad::Ready(mut store) => store
                        .refresh_document_metadata(&inputs)
                        .ok()
                        .map(|()| store),
                    SurfaceIndexCacheLoad::Missing
                    | SurfaceIndexCacheLoad::Stale
                    | SurfaceIndexCacheLoad::Invalid => None,
                }
            });
            let store = if let Some(store) = loaded_store {
                self.index_cache_load_count += 1;
                store
            } else {
                let store = InMemoryIndexStore::build(&inputs, tokenization_config)
                    .map_err(ConcordanceError::Index)?;
                if let Some(cache_root) = self.cache_root.as_deref() {
                    let _ = write_surface_index_cache(cache_root, &manifest, &store);
                }
                self.index_build_count += 1;
                index_rebuilt = true;
                store
            };
            self.index_cache = Some(ConcordanceIndexCacheEntry { manifest, store });
        }

        Ok((
            &self
                .index_cache
                .as_ref()
                .expect("index cache exists after build")
                .store,
            index_rebuilt,
        ))
    }

    /// Prepares the full usable-corpus index by resolving one document at a
    /// time. A warm in-process store returns before invoking the provider.
    pub fn index_store_for_provider<Resolve, Progress>(
        &mut self,
        document_slots: usize,
        tokenization_config: &TokenizationConfig,
        mut resolve: Resolve,
        mut report_progress: Progress,
    ) -> Result<(&InMemoryIndexStore, bool), ConcordanceError>
    where
        Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
        Progress: FnMut(ConcordanceProgress),
    {
        if self.index_cache.is_some() {
            return Ok((
                &self.index_cache.as_ref().expect("checked above").store,
                false,
            ));
        }

        let mut hasher = Sha256::new();
        hasher.update(b"corpuswright-concordance-full-corpus-content-v3\0");
        hasher.update((document_slots as u64).to_le_bytes());
        let mut metadata_inputs = Vec::new();
        metadata_inputs
            .try_reserve(document_slots)
            .map_err(|error| {
                ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                    "streaming index metadata: {error}"
                )))
            })?;
        for document_index in 0..document_slots {
            report_progress(ConcordanceProgress {
                files_done: document_index,
                files_total: document_slots,
                current_phase: ConcordancePhase::BuildingIndex,
            });
            hasher.update((document_index as u64).to_le_bytes());
            if let Some(document) = resolve(document_index) {
                if document.document_index != document_index {
                    return Err(ConcordanceError::InvalidDocumentIdentity);
                }
                hasher.update([1]);
                hasher.update((document.text.len() as u64).to_le_bytes());
                hasher.update(document.text.as_bytes());
                metadata_inputs.push(SurfaceIndexDocumentInput {
                    document_id: document_index,
                    source_path: document.source_path,
                    display_name: document.display_name,
                    prepared_text: "",
                });
            } else {
                hasher.update([0]);
            }
        }
        let manifest = SurfaceIndexManifest::new(
            format!("{:x}", hasher.finalize()),
            TokenizationManifest {
                config: *tokenization_config,
                ..TokenizationManifest::default()
            },
        );
        let loaded_store = self.cache_root.as_deref().and_then(|cache_root| {
            match load_surface_index_cache(cache_root, &manifest) {
                SurfaceIndexCacheLoad::Ready(mut store) => store
                    .refresh_document_metadata(&metadata_inputs)
                    .ok()
                    .map(|()| store),
                SurfaceIndexCacheLoad::Missing
                | SurfaceIndexCacheLoad::Stale
                | SurfaceIndexCacheLoad::Invalid => None,
            }
        });

        let mut index_rebuilt = false;
        let store = if let Some(store) = loaded_store {
            self.index_cache_load_count += 1;
            store
        } else {
            let mut builder = SurfaceIndexBuilder::new(
                *tokenization_config,
                SurfaceIndexResourceLimits::default(),
            );
            for document_index in 0..document_slots {
                report_progress(ConcordanceProgress {
                    files_done: document_index,
                    files_total: document_slots,
                    current_phase: ConcordancePhase::BuildingIndex,
                });
                if let Some(document) = resolve(document_index) {
                    if document.document_index != document_index {
                        return Err(ConcordanceError::InvalidDocumentIdentity);
                    }
                    builder
                        .add_document(&SurfaceIndexDocumentInput {
                            document_id: document.document_index,
                            source_path: document.source_path,
                            display_name: document.display_name,
                            prepared_text: &document.text,
                        })
                        .map_err(ConcordanceError::Index)?;
                }
            }
            let (store, _) = builder.finish().map_err(ConcordanceError::Index)?;
            if let Some(cache_root) = self.cache_root.as_deref() {
                let _ = write_surface_index_cache(cache_root, &manifest, &store);
            }
            self.index_build_count += 1;
            index_rebuilt = true;
            store
        };
        self.index_cache = Some(ConcordanceIndexCacheEntry { manifest, store });
        Ok((
            &self.index_cache.as_ref().expect("inserted above").store,
            index_rebuilt,
        ))
    }

    #[cfg(test)]
    pub(super) fn index_build_count(&self) -> usize {
        self.index_build_count
    }

    #[cfg(test)]
    pub(super) fn index_cache_load_count(&self) -> usize {
        self.index_cache_load_count
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

impl ConcordanceError {
    /// The inherited scan implementation still materialises all hits. Until it
    /// is replaced by the shared bounded collector, resource and cancellation
    /// failures must fail closed instead of entering that fallback.
    pub fn prevents_unbounded_scan_fallback(&self) -> bool {
        matches!(self, Self::Index(error) if error.is_resource_or_cancellation())
    }
}

pub(super) fn compute_dispersion_from_token_counts(
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

fn concordance_surface_matches_key(surface: &str, key: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        surface == key
    } else if surface.is_ascii() && key.is_ascii() {
        surface.eq_ignore_ascii_case(key)
    } else {
        case_fold_surface(surface) == key
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConcordanceExecutionStats {
    pub candidates_visited: u64,
    pub valid_hits: u64,
    pub compact_sort_records_peak: usize,
    pub rows_materialised: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactConcordanceHit {
    document_id: usize,
    token_index: usize,
    node_token_length: usize,
    left_keys: [Option<u32>; 3],
    right_keys: [Option<u32>; 3],
}

#[derive(Debug, Clone, Copy)]
struct CandidateConcordanceHit<'key> {
    document_id: usize,
    token_index: usize,
    node_token_length: usize,
    left_keys: [Option<&'key str>; 3],
    right_keys: [Option<&'key str>; 3],
}

#[derive(Debug, Default)]
struct SortSymbolArena {
    symbols: Vec<String>,
    ids: HashMap<String, u32>,
}

impl SortSymbolArena {
    fn intern(&mut self, value: &str) -> Result<u32, ConcordanceError> {
        if let Some(id) = self.ids.get(value) {
            return Ok(*id);
        }
        let id = u32::try_from(self.symbols.len()).map_err(|_| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(
                "query sort symbol table exceeds u32 capacity".to_string(),
            ))
        })?;
        self.symbols.try_reserve(1).map_err(|error| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                "query sort symbols: {error}"
            )))
        })?;
        let mut owned = String::new();
        owned.try_reserve(value.len()).map_err(|error| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                "query sort symbol text: {error}"
            )))
        })?;
        owned.push_str(value);
        self.ids.try_reserve(1).map_err(|error| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                "query sort symbol lookup: {error}"
            )))
        })?;
        self.symbols.push(owned.clone());
        self.ids.insert(owned, id);
        Ok(id)
    }

    fn value(&self, id: u32) -> &str {
        &self.symbols[id as usize]
    }
}

struct BoundedConcordanceCollector {
    sort_key: ConcordanceSortKey,
    offset: usize,
    limit: usize,
    retained_depth: usize,
    heap: Vec<CompactConcordanceHit>,
    arena: SortSymbolArena,
    total_hits: usize,
    document_hit_counts: BTreeMap<usize, u64>,
    stats: ConcordanceExecutionStats,
}

impl BoundedConcordanceCollector {
    fn new(
        sort_key: ConcordanceSortKey,
        offset: usize,
        limit: usize,
        limits: SurfaceIndexResourceLimits,
    ) -> Result<Self, ConcordanceError> {
        let retained_depth = if sort_key == ConcordanceSortKey::FilePosition {
            limit
        } else {
            offset.checked_add(limit).ok_or_else(|| {
                ConcordanceError::Index(SurfaceIndexError::RetainedSortDepthExceedsQueryBudget {
                    requested_records: u64::MAX,
                    limit_records: limits.max_retained_query_sort_records,
                })
            })?
        };
        if retained_depth as u64 > limits.max_retained_query_sort_records {
            return Err(ConcordanceError::Index(
                SurfaceIndexError::RetainedSortDepthExceedsQueryBudget {
                    requested_records: retained_depth as u64,
                    limit_records: limits.max_retained_query_sort_records,
                },
            ));
        }
        let mut heap = Vec::new();
        heap.try_reserve(retained_depth.min(4_096))
            .map_err(|error| {
                ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                    "bounded concordance hits: {error}"
                )))
            })?;
        Ok(Self {
            sort_key,
            offset,
            limit,
            retained_depth,
            heap,
            arena: SortSymbolArena::default(),
            total_hits: 0,
            document_hit_counts: BTreeMap::new(),
            stats: ConcordanceExecutionStats::default(),
        })
    }

    fn record(&mut self, candidate: CandidateConcordanceHit<'_>) -> Result<(), ConcordanceError> {
        let hit_index = self.total_hits;
        self.total_hits += 1;
        self.stats.valid_hits += 1;
        *self
            .document_hit_counts
            .entry(candidate.document_id)
            .or_default() += 1;
        if self.retained_depth == 0 {
            return Ok(());
        }
        if self.sort_key == ConcordanceSortKey::FilePosition {
            if hit_index >= self.offset && self.heap.len() < self.limit {
                let compact = self.compact_candidate(candidate, false)?;
                self.heap.push(compact);
                self.update_peak();
            }
            return Ok(());
        }

        if self.heap.len() < self.retained_depth {
            let compact = self.compact_candidate(candidate, true)?;
            self.heap.push(compact);
            self.sift_up(self.heap.len() - 1);
            self.update_peak();
        } else if self.compare_candidate_to_compact(&candidate, &self.heap[0]) == Ordering::Less {
            let compact = self.compact_candidate(candidate, true)?;
            self.heap[0] = compact;
            self.sift_down(0);
            self.compact_arena_if_needed()?;
        }
        Ok(())
    }

    fn compact_candidate(
        &mut self,
        candidate: CandidateConcordanceHit<'_>,
        include_keys: bool,
    ) -> Result<CompactConcordanceHit, ConcordanceError> {
        let mut left_keys = [None; 3];
        let mut right_keys = [None; 3];
        if include_keys {
            for index in 0..3 {
                left_keys[index] = candidate.left_keys[index]
                    .map(|key| self.arena.intern(key))
                    .transpose()?;
                right_keys[index] = candidate.right_keys[index]
                    .map(|key| self.arena.intern(key))
                    .transpose()?;
            }
        }
        Ok(CompactConcordanceHit {
            document_id: candidate.document_id,
            token_index: candidate.token_index,
            node_token_length: candidate.node_token_length,
            left_keys,
            right_keys,
        })
    }

    fn update_peak(&mut self) {
        self.stats.compact_sort_records_peak =
            self.stats.compact_sort_records_peak.max(self.heap.len());
    }

    fn compare_compact(
        &self,
        left: &CompactConcordanceHit,
        right: &CompactConcordanceHit,
    ) -> Ordering {
        compare_compact_hit_keys(left, right, self.sort_key, &self.arena)
            .then_with(|| left.document_id.cmp(&right.document_id))
            .then_with(|| left.token_index.cmp(&right.token_index))
    }

    fn compare_candidate_to_compact(
        &self,
        left: &CandidateConcordanceHit<'_>,
        right: &CompactConcordanceHit,
    ) -> Ordering {
        compare_candidate_hit_keys(left, right, self.sort_key, &self.arena)
            .then_with(|| left.document_id.cmp(&right.document_id))
            .then_with(|| left.token_index.cmp(&right.token_index))
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if self.compare_compact(&self.heap[index], &self.heap[parent]) != Ordering::Greater {
                break;
            }
            self.heap.swap(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = index * 2 + 1;
            if left >= self.heap.len() {
                break;
            }
            let right = left + 1;
            let mut worse = left;
            if right < self.heap.len()
                && self.compare_compact(&self.heap[right], &self.heap[left]) == Ordering::Greater
            {
                worse = right;
            }
            if self.compare_compact(&self.heap[worse], &self.heap[index]) != Ordering::Greater {
                break;
            }
            self.heap.swap(index, worse);
            index = worse;
        }
    }

    fn compact_arena_if_needed(&mut self) -> Result<(), ConcordanceError> {
        let maximum_symbols = self.retained_depth.saturating_mul(12).saturating_add(64);
        if self.arena.symbols.len() <= maximum_symbols {
            return Ok(());
        }
        let old = std::mem::take(&mut self.arena);
        for hit in &mut self.heap {
            for key in hit.left_keys.iter_mut().chain(hit.right_keys.iter_mut()) {
                if let Some(old_id) = *key {
                    *key = Some(self.arena.intern(old.value(old_id))?);
                }
            }
        }
        Ok(())
    }

    fn finish(
        mut self,
    ) -> (
        Vec<CompactConcordanceHit>,
        usize,
        BTreeMap<usize, u64>,
        ConcordanceExecutionStats,
    ) {
        if self.sort_key != ConcordanceSortKey::FilePosition {
            let sort_key = self.sort_key;
            let arena = &self.arena;
            self.heap.sort_by(|left, right| {
                compare_compact_hit_keys(left, right, sort_key, arena)
                    .then_with(|| left.document_id.cmp(&right.document_id))
                    .then_with(|| left.token_index.cmp(&right.token_index))
            });
            self.heap = self
                .heap
                .into_iter()
                .skip(self.offset.min(self.total_hits))
                .take(self.limit)
                .collect();
        }
        (
            self.heap,
            self.total_hits,
            self.document_hit_counts,
            self.stats,
        )
    }
}

fn context_key_order(primary_index: usize) -> [usize; 3] {
    match primary_index {
        0 => [0, 1, 2],
        1 => [1, 0, 2],
        _ => [2, 1, 0],
    }
}

fn sort_key_context(sort_key: ConcordanceSortKey) -> Option<(bool, usize)> {
    match sort_key {
        ConcordanceSortKey::Left1 => Some((true, 0)),
        ConcordanceSortKey::Left2 => Some((true, 1)),
        ConcordanceSortKey::Left3 => Some((true, 2)),
        ConcordanceSortKey::Right1 => Some((false, 0)),
        ConcordanceSortKey::Right2 => Some((false, 1)),
        ConcordanceSortKey::Right3 => Some((false, 2)),
        ConcordanceSortKey::FilePosition | ConcordanceSortKey::Node => None,
    }
}

fn compare_compact_hit_keys(
    left: &CompactConcordanceHit,
    right: &CompactConcordanceHit,
    sort_key: ConcordanceSortKey,
    arena: &SortSymbolArena,
) -> Ordering {
    let Some((left_side, primary)) = sort_key_context(sort_key) else {
        return Ordering::Equal;
    };
    for index in context_key_order(primary) {
        let left = if left_side {
            left.left_keys[index]
        } else {
            left.right_keys[index]
        };
        let right = if left_side {
            right.left_keys[index]
        } else {
            right.right_keys[index]
        };
        let ordering = compare_optional_context(
            left.map(|id| arena.value(id)),
            right.map(|id| arena.value(id)),
        );
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

fn compare_candidate_hit_keys(
    left: &CandidateConcordanceHit<'_>,
    right: &CompactConcordanceHit,
    sort_key: ConcordanceSortKey,
    arena: &SortSymbolArena,
) -> Ordering {
    let Some((left_side, primary)) = sort_key_context(sort_key) else {
        return Ordering::Equal;
    };
    for index in context_key_order(primary) {
        let left = if left_side {
            left.left_keys[index]
        } else {
            left.right_keys[index]
        };
        let right = if left_side {
            right.left_keys[index]
        } else {
            right.right_keys[index]
        };
        let ordering = compare_optional_context(left, right.map(|id| arena.value(id)));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

#[derive(Debug, Clone)]
struct ConcordanceDocumentStats {
    document_id: usize,
    source_path: String,
    display_name: String,
    token_count: u64,
    hit_count: u64,
}

pub(super) fn concordance_token_sequence_matches<'text>(
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

pub(super) fn count_concordance_token_sequence_matches<'text>(
    tokens: &[crate::tokenization::Token<'text>],
    query_keys: &[String],
    case_sensitive: bool,
) -> u64 {
    if query_keys.is_empty() {
        return 0;
    }
    tokens
        .windows(query_keys.len())
        .filter(|window| concordance_token_sequence_matches(window, query_keys, case_sensitive))
        .count() as u64
}

fn compare_optional_context(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
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
    tokenization_config: &TokenizationConfig,
) -> Result<SurfaceIndexManifest, ConcordanceError> {
    Ok(SurfaceIndexManifest::new(
        concordance_processed_texts_hash(documents)?,
        TokenizationManifest {
            config: *tokenization_config,
            ..TokenizationManifest::default()
        },
    ))
}

fn concordance_processed_texts_hash(
    documents: &[ConcordanceDocument],
) -> Result<String, ConcordanceError> {
    let mut selected_documents = Vec::new();
    selected_documents
        .try_reserve(documents.len())
        .map_err(|error| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                "content hash document ordering: {error}"
            )))
        })?;
    selected_documents.extend(documents.iter());
    selected_documents.sort_by_key(|document| document.document_index);

    let mut hasher = Sha256::new();
    hasher.update(b"corpuswright-concordance-surface-index-content-v2\0");
    hasher.update((selected_documents.len() as u64).to_le_bytes());
    for document in selected_documents {
        hasher.update((document.document_index as u64).to_le_bytes());
        hasher.update((document.text.len() as u64).to_le_bytes());
        hasher.update(document.text.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn search_concordance_documents_routed(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
) -> Result<RoutedConcordanceResult, ConcordanceError> {
    match backend {
        ConcordanceBackend::Scan => {
            let (result, execution_stats) = search_concordance_documents_scan_with_stats(
                documents,
                query,
                tokenization_config,
            )?;
            Ok(RoutedConcordanceResult {
                result,
                backend_used: ConcordanceBackendUsed::Scan,
                index_rebuilt: false,
                execution_stats,
            })
        }
        ConcordanceBackend::Index => {
            let (result, index_rebuilt, execution_stats) =
                search_concordance_documents_index(documents, query, tokenization_config, context)?;
            Ok(RoutedConcordanceResult {
                result,
                backend_used: ConcordanceBackendUsed::Index,
                index_rebuilt,
                execution_stats,
            })
        }
    }
}

pub fn search_concordance_documents_routed_provider<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    resolve: Resolve,
    mut report_progress: Progress,
) -> Result<RoutedConcordanceResult, ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
    Progress: FnMut(ConcordanceProgress),
{
    let (store, index_rebuilt) =
        context.index_store_for_provider(document_slots, tokenization_config, resolve, &mut report_progress)?;
    let documents = store
        .indexed_documents()
        .map(|document| ConcordanceDocument {
            document_index: document.document_id,
            source_path: document.source_path.clone(),
            display_name: document.display_name.clone(),
            text: String::new(),
        })
        .collect::<Vec<_>>();
    let selected_document_ids = selected_concordance_document_ids(&documents, &query.selection);
    report_progress(ConcordanceProgress {
        current_phase: ConcordancePhase::Searching,
        files_done: 0,
        files_total: documents.len(),
    });
    let (result, _, execution_stats) = search_concordance_documents_index_from_store(
        &documents,
        query,
        tokenization_config,
        store,
        &selected_document_ids,
        index_rebuilt,
    )?;
    Ok(RoutedConcordanceResult {
        result,
        backend_used: ConcordanceBackendUsed::Index,
        index_rebuilt,
        execution_stats,
    })
}

pub fn search_concordance_documents_routed_scan_provider<Resolve>(
    document_slots: usize,
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    mut resolve: Resolve,
) -> Result<RoutedConcordanceResult, ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
{
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
    let mut collector = BoundedConcordanceCollector::new(
        query.sort_key,
        query.offset,
        limit,
        SurfaceIndexResourceLimits::default(),
    )?;
    let mut document_counts = Vec::<(usize, u64)>::new();
    let mut document_token_counts = HashMap::<usize, u64>::new();
    let mut document_stats = Vec::<ConcordanceDocumentStats>::new();
    let mut selected_tokens_total = 0u64;
    let mut selected_documents_total = 0usize;
    for document_index in 0..document_slots {
        if !selection_includes_document_id(&query.selection, document_index) {
            continue;
        }
        let Some(document) = resolve(document_index) else {
            continue;
        };
        if document.document_index != document_index {
            return Err(ConcordanceError::InvalidDocumentIdentity);
        }
        selected_documents_total += 1;
        let token_count = scan_concordance_document_into_collector(
            &document,
            &query_keys,
            context_filter_keys.as_deref(),
            query.context_filter_direction,
            query.case_sensitive,
            window_tokens,
            tokenization_config,
            &mut collector,
        )?;
        selected_tokens_total += token_count;
        document_token_counts.insert(document_index, token_count);
        let hit_count = collector
            .document_hit_counts
            .get(&document_index)
            .copied()
            .unwrap_or_default();
        if hit_count > 0 {
            document_counts.push((document_index, hit_count));
        }
        document_stats.push(ConcordanceDocumentStats {
            document_id: document_index,
            source_path: document.source_path,
            display_name: document.display_name,
            token_count,
            hit_count,
        });
    }
    let (page_hits, total_hits, _, mut execution_stats) = collector.finish();
    let offset = query.offset.min(total_hits);
    let mut rows = Vec::new();
    rows.try_reserve(page_hits.len()).map_err(|error| {
        ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
            "provider scan page rows: {error}"
        )))
    })?;
    for hit in &page_hits {
        let document = resolve(hit.document_id).ok_or(ConcordanceError::InvalidDocumentIdentity)?;
        rows.push(materialize_streaming_scan_hit(
            &document,
            hit,
            window_tokens,
            tokenization_config,
        )?);
        execution_stats.rows_materialised += 1;
    }
    let dp = compute_dispersion_from_token_counts(
        &document_counts,
        total_hits as u64,
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
    Ok(RoutedConcordanceResult {
        result: ConcordanceResult {
            query: query_tokens.join(" "),
            context_filter: context_filter_tokens.map(|tokens| tokens.join(" ")),
            case_sensitive: query.case_sensitive,
            sort_key: query.sort_key,
            window_tokens,
            total_hits,
            documents_with_hits: document_counts.len(),
            selected_documents_total,
            selected_tokens_total,
            dp,
            dispersion_label: annotated_dispersion_label(dp).to_string(),
            top_document_share,
            offset,
            limit,
            rows,
            distribution,
        },
        backend_used: ConcordanceBackendUsed::Scan,
        index_rebuilt: false,
        execution_stats,
    })
}

fn selection_includes_document_id(
    selection: &AnalysisDocumentSelection,
    document_id: usize,
) -> bool {
    if selection.all_documents {
        !selection.excluded_ids.contains(&document_id)
    } else {
        selection.ids.contains(&document_id)
    }
}

fn search_concordance_documents_index(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
) -> Result<(ConcordanceResult, bool, ConcordanceExecutionStats), ConcordanceError> {
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);
    let (store, index_rebuilt) =
        context.index_store_for(documents, &selected_document_ids, tokenization_config)?;
    search_concordance_documents_index_from_store(
        documents,
        query,
        tokenization_config,
        store,
        &selected_document_ids,
        index_rebuilt,
    )
}

fn search_concordance_documents_index_from_store(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
    store: &InMemoryIndexStore,
    selected_document_ids: &HashSet<usize>,
    index_rebuilt: bool,
) -> Result<(ConcordanceResult, bool, ConcordanceExecutionStats), ConcordanceError> {
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
    let resolved_context_filter = context_filter_tokens.map(|tokens| tokens.join(" "));
    let mut collector = BoundedConcordanceCollector::new(
        query.sort_key,
        query.offset,
        limit,
        SurfaceIndexResourceLimits::default(),
    )?;
    let candidates_visited = visit_index_query_hits(store, &index_query, |candidate| {
        if !selected_document_ids.contains(&candidate.document_id) {
            return Ok(());
        }
        if let Some(keys) = context_filter_keys.as_deref()
            && !index_context_filter_matches(
                store,
                &candidate,
                window_tokens,
                keys,
                query.context_filter_direction,
                query.case_sensitive,
            )?
        {
            return Ok(());
        }
        let records = store
            .records_for_document(candidate.document_id)
            .ok_or_else(|| {
                ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                    document_id: candidate.document_id,
                    token_ordinal_in_document: candidate.token_ordinal_in_document,
                    node_token_length: candidate.node_token_length,
                })
            })?;
        let node_end = candidate.token_ordinal_in_document + candidate.node_token_length;
        let key_at = |index: Option<usize>| -> Result<Option<&str>, ConcordanceError> {
            let Some(record) = index.and_then(|index| records.get(index)) else {
                return Ok(None);
            };
            store
                .concordance_key_for_record(record, query.case_sensitive)
                .map(Some)
                .ok_or_else(|| {
                    ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                        document_id: candidate.document_id,
                        token_ordinal_in_document: candidate.token_ordinal_in_document,
                        node_token_length: candidate.node_token_length,
                    })
                })
        };
        let (left_keys, right_keys) = if sort_key_context(query.sort_key).is_some() {
            (
                [
                    key_at(candidate.token_ordinal_in_document.checked_sub(1))?,
                    key_at(candidate.token_ordinal_in_document.checked_sub(2))?,
                    key_at(candidate.token_ordinal_in_document.checked_sub(3))?,
                ],
                [
                    key_at(Some(node_end))?,
                    key_at(node_end.checked_add(1))?,
                    key_at(node_end.checked_add(2))?,
                ],
            )
        } else {
            ([None; 3], [None; 3])
        };
        collector.record(CandidateConcordanceHit {
            document_id: candidate.document_id,
            token_index: candidate.token_ordinal_in_document,
            node_token_length: candidate.node_token_length,
            left_keys,
            right_keys,
        })
    })?;
    let (page_hits, total_hits, filtered_document_hit_counts, mut execution_stats) =
        collector.finish();
    execution_stats.candidates_visited = candidates_visited;
    let mut rows = Vec::new();
    rows.try_reserve(page_hits.len()).map_err(|error| {
        ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
            "concordance page rows: {error}"
        )))
    })?;
    for hit in page_hits {
        let row = materialize_surface_index_kwic_row(
            store,
            &SurfaceIndexHit {
                document_id: hit.document_id,
                token_ordinal_in_document: hit.token_index,
                node_token_length: hit.node_token_length,
            },
            window_tokens,
        )
        .map_err(concordance_error_from_surface_index)?;
        rows.push(ConcordanceRow {
            document_index: row.document_id,
            source_path: row.source_path,
            display_name: row.display_name,
            token_index: row.token_ordinal_in_document,
            node_token_length: row.node_token_length,
            left_context: row.left_context,
            node: row.node,
            right_context: row.right_context,
        });
        execution_stats.rows_materialised += 1;
    }

    let offset = query.offset.min(total_hits);

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
        let hit_count = filtered_document_hit_counts
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
            context_filter: resolved_context_filter,
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
        execution_stats,
    ))
}

pub(super) fn visit_index_query_hits(
    store: &impl IndexStore,
    query: &SurfaceIndexQuery,
    mut visitor: impl FnMut(SurfaceIndexHit) -> Result<(), ConcordanceError>,
) -> Result<u64, ConcordanceError> {
    let terms = query.terms();
    let Some(first) = terms.first() else {
        return Err(ConcordanceError::EmptyQuery);
    };
    let postings = store
        .postings(&first.normalised_surface)
        .unwrap_or_default();
    let mut candidates_visited = 0u64;
    for &position in postings {
        candidates_visited += 1;
        let Some((document_id, token_ordinal_in_document)) =
            store.document_and_ordinal_for_position(position)
        else {
            return Err(ConcordanceError::Index(
                SurfaceIndexError::InconsistentStore {
                    document_id: 0,
                    token_ordinal_in_document: 0,
                    node_token_length: terms.len(),
                },
            ));
        };
        let records = store.records_for_document(document_id).ok_or_else(|| {
            ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                document_id,
                token_ordinal_in_document,
                node_token_length: terms.len(),
            })
        })?;
        let Some(node_end) = token_ordinal_in_document.checked_add(terms.len()) else {
            continue;
        };
        if node_end > records.len() {
            continue;
        }
        let mut matches = true;
        for (record, term) in records[token_ordinal_in_document..node_end]
            .iter()
            .zip(terms)
        {
            let expected = if query.case_sensitive() {
                term.exact_surface.as_str()
            } else {
                term.normalised_surface.as_str()
            };
            if store.concordance_key_for_record(record, query.case_sensitive()) != Some(expected) {
                matches = false;
                break;
            }
        }
        if matches {
            visitor(SurfaceIndexHit {
                document_id,
                token_ordinal_in_document,
                node_token_length: terms.len(),
            })?;
        }
    }
    Ok(candidates_visited)
}

fn concordance_error_from_surface_index(error: SurfaceIndexError) -> ConcordanceError {
    match error {
        SurfaceIndexError::EmptyQuery => ConcordanceError::EmptyQuery,
        error => ConcordanceError::Index(error),
    }
}

fn index_context_filter_matches(
    store: &impl IndexStore,
    hit: &SurfaceIndexHit,
    window_tokens: usize,
    context_filter_keys: &[String],
    direction: ConcordanceContextFilterDirection,
    case_sensitive: bool,
) -> Result<bool, ConcordanceError> {
    let records = store.records_for_document(hit.document_id).ok_or({
        ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        })
    })?;
    let node_end = hit
        .token_ordinal_in_document
        .checked_add(hit.node_token_length)
        .ok_or({
            ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                document_id: hit.document_id,
                token_ordinal_in_document: hit.token_ordinal_in_document,
                node_token_length: hit.node_token_length,
            })
        })?;
    if node_end > records.len() {
        return Err(ConcordanceError::Index(
            SurfaceIndexError::InconsistentStore {
                document_id: hit.document_id,
                token_ordinal_in_document: hit.token_ordinal_in_document,
                node_token_length: hit.node_token_length,
            },
        ));
    }
    let left_start = hit.token_ordinal_in_document.saturating_sub(window_tokens);
    let right_end = node_end.saturating_add(window_tokens).min(records.len());
    let left_matches = matches!(
        direction,
        ConcordanceContextFilterDirection::Left | ConcordanceContextFilterDirection::Both
    ) && index_record_sequence_matches(
        store,
        &records[left_start..hit.token_ordinal_in_document],
        context_filter_keys,
        case_sensitive,
    )?;
    if left_matches {
        return Ok(true);
    }
    if matches!(
        direction,
        ConcordanceContextFilterDirection::Right | ConcordanceContextFilterDirection::Both
    ) {
        index_record_sequence_matches(
            store,
            &records[node_end..right_end],
            context_filter_keys,
            case_sensitive,
        )
    } else {
        Ok(false)
    }
}

fn index_record_sequence_matches(
    store: &impl IndexStore,
    records: &[crate::index::SurfaceIndexTokenRecord],
    query_keys: &[String],
    case_sensitive: bool,
) -> Result<bool, ConcordanceError> {
    if query_keys.is_empty() || query_keys.len() > records.len() {
        return Ok(false);
    }
    for window in records.windows(query_keys.len()) {
        let mut matches = true;
        for (record, query_key) in window.iter().zip(query_keys) {
            let record_key = store
                .concordance_key_for_record(record, case_sensitive)
                .ok_or({
                    ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                        document_id: 0,
                        token_ordinal_in_document: 0,
                        node_token_length: 1,
                    })
                })?;
            if record_key != query_key {
                matches = false;
                break;
            }
        }
        if matches {
            return Ok(true);
        }
    }
    Ok(false)
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

fn search_concordance_documents_scan_with_stats(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<(ConcordanceResult, ConcordanceExecutionStats), ConcordanceError> {
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
    let mut selected_tokens_total = 0u64;
    let mut collector = BoundedConcordanceCollector::new(
        query.sort_key,
        query.offset,
        limit,
        SurfaceIndexResourceLimits::default(),
    )?;
    let mut sorted_documents = Vec::new();
    sorted_documents
        .try_reserve(documents.len())
        .map_err(|error| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
                "scan document ordering: {error}"
            )))
        })?;
    sorted_documents.extend(documents.iter());
    sorted_documents.sort_by_key(|document| document.document_index);

    for document in sorted_documents {
        if !selected_document_ids.contains(&document.document_index) {
            continue;
        }
        let document_token_count = scan_concordance_document_into_collector(
            document,
            &query_keys,
            context_filter_keys.as_deref(),
            query.context_filter_direction,
            query.case_sensitive,
            window_tokens,
            tokenization_config,
            &mut collector,
        )?;
        selected_tokens_total += document_token_count;
        document_token_counts.insert(document.document_index, document_token_count);
        let document_hit_count = collector
            .document_hit_counts
            .get(&document.document_index)
            .copied()
            .unwrap_or_default();
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
    let (page_hits, total_hits, _, mut execution_stats) = collector.finish();
    let offset = query.offset.min(total_hits);
    let mut rows = Vec::new();
    rows.try_reserve(page_hits.len()).map_err(|error| {
        ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
            "scan page rows: {error}"
        )))
    })?;
    for hit in &page_hits {
        let document = documents
            .iter()
            .find(|document| document.document_index == hit.document_id)
            .ok_or(ConcordanceError::InvalidDocumentIdentity)?;
        rows.push(materialize_streaming_scan_hit(
            document,
            hit,
            window_tokens,
            tokenization_config,
        )?);
        execution_stats.rows_materialised += 1;
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
        },
        execution_stats,
    ))
}

fn scan_concordance_document_into_collector(
    document: &ConcordanceDocument,
    query_keys: &[String],
    context_filter_keys: Option<&[String]>,
    context_filter_direction: ConcordanceContextFilterDirection,
    case_sensitive: bool,
    window_tokens: usize,
    tokenization_config: &TokenizationConfig,
    collector: &mut BoundedConcordanceCollector,
) -> Result<u64, ConcordanceError> {
    let document_id = u32::try_from(document.document_index)
        .map_err(|_| ConcordanceError::InvalidDocumentIdentity)?;
    let lookaround = window_tokens.max(3);
    let mut buffer = VecDeque::<Token<'_>>::new();
    let mut buffer_start = 0usize;
    let mut total_tokens = 0usize;
    let mut next_candidate = 0usize;
    visit_tokens(&document.text, document_id, tokenization_config, |token| {
        if !token.counts_as_word() {
            return Ok(());
        }
        buffer.push_back(token);
        total_tokens += 1;
        while next_candidate
            .checked_add(query_keys.len())
            .and_then(|end| end.checked_add(lookaround))
            .is_some_and(|ready| ready <= total_tokens)
        {
            process_streaming_scan_candidate(
                document.document_index,
                &buffer,
                buffer_start,
                total_tokens,
                next_candidate,
                query_keys,
                context_filter_keys,
                context_filter_direction,
                case_sensitive,
                window_tokens,
                collector,
            )?;
            next_candidate += 1;
            let keep_from = next_candidate.saturating_sub(lookaround);
            while buffer_start < keep_from {
                buffer.pop_front();
                buffer_start += 1;
            }
        }
        Ok::<(), ConcordanceError>(())
    })?;
    while next_candidate
        .checked_add(query_keys.len())
        .is_some_and(|end| end <= total_tokens)
    {
        process_streaming_scan_candidate(
            document.document_index,
            &buffer,
            buffer_start,
            total_tokens,
            next_candidate,
            query_keys,
            context_filter_keys,
            context_filter_direction,
            case_sensitive,
            window_tokens,
            collector,
        )?;
        next_candidate += 1;
        let keep_from = next_candidate.saturating_sub(lookaround);
        while buffer_start < keep_from {
            buffer.pop_front();
            buffer_start += 1;
        }
    }
    Ok(total_tokens as u64)
}

#[allow(clippy::too_many_arguments)]
fn process_streaming_scan_candidate(
    document_id: usize,
    buffer: &VecDeque<Token<'_>>,
    buffer_start: usize,
    total_tokens: usize,
    token_index: usize,
    query_keys: &[String],
    context_filter_keys: Option<&[String]>,
    context_filter_direction: ConcordanceContextFilterDirection,
    case_sensitive: bool,
    window_tokens: usize,
    collector: &mut BoundedConcordanceCollector,
) -> Result<(), ConcordanceError> {
    collector.stats.candidates_visited += 1;
    let node_end = token_index + query_keys.len();
    if !(token_index..node_end).zip(query_keys).all(|(index, key)| {
        streaming_scan_token(buffer, buffer_start, index).is_some_and(|token| {
            concordance_surface_matches_key(token.surface, key, case_sensitive)
        })
    }) {
        return Ok(());
    }
    let left_start = token_index.saturating_sub(window_tokens);
    let right_end = node_end.saturating_add(window_tokens).min(total_tokens);
    if let Some(keys) = context_filter_keys {
        let left_matches = matches!(
            context_filter_direction,
            ConcordanceContextFilterDirection::Left | ConcordanceContextFilterDirection::Both
        ) && streaming_scan_range_matches(
            buffer,
            buffer_start,
            left_start,
            token_index,
            keys,
            case_sensitive,
        );
        let right_matches = matches!(
            context_filter_direction,
            ConcordanceContextFilterDirection::Right | ConcordanceContextFilterDirection::Both
        ) && streaming_scan_range_matches(
            buffer,
            buffer_start,
            node_end,
            right_end,
            keys,
            case_sensitive,
        );
        if !left_matches && !right_matches {
            return Ok(());
        }
    }

    let (left_owned, right_owned) = if sort_key_context(collector.sort_key).is_some() {
        let key_at = |index: Option<usize>| {
            index
                .and_then(|index| streaming_scan_token(buffer, buffer_start, index))
                .map(|token| concordance_key(token.surface, case_sensitive))
        };
        (
            [
                key_at(token_index.checked_sub(1)),
                key_at(token_index.checked_sub(2)),
                key_at(token_index.checked_sub(3)),
            ],
            [
                key_at((node_end < total_tokens).then_some(node_end)),
                key_at((node_end + 1 < total_tokens).then_some(node_end + 1)),
                key_at((node_end + 2 < total_tokens).then_some(node_end + 2)),
            ],
        )
    } else {
        ([None, None, None], [None, None, None])
    };
    collector.record(CandidateConcordanceHit {
        document_id,
        token_index,
        node_token_length: query_keys.len(),
        left_keys: left_owned.each_ref().map(|key| key.as_deref()),
        right_keys: right_owned.each_ref().map(|key| key.as_deref()),
    })
}

fn streaming_scan_token<'text>(
    buffer: &'text VecDeque<Token<'text>>,
    buffer_start: usize,
    absolute_index: usize,
) -> Option<&'text Token<'text>> {
    absolute_index
        .checked_sub(buffer_start)
        .and_then(|index| buffer.get(index))
}

fn streaming_scan_range_matches(
    buffer: &VecDeque<Token<'_>>,
    buffer_start: usize,
    start: usize,
    end: usize,
    query_keys: &[String],
    case_sensitive: bool,
) -> bool {
    !query_keys.is_empty()
        && end.saturating_sub(start) >= query_keys.len()
        && (start..=end - query_keys.len()).any(|candidate| {
            (candidate..candidate + query_keys.len())
                .zip(query_keys)
                .all(|(index, key)| {
                    streaming_scan_token(buffer, buffer_start, index).is_some_and(|token| {
                        concordance_surface_matches_key(token.surface, key, case_sensitive)
                    })
                })
        })
}

fn materialize_streaming_scan_hit(
    document: &ConcordanceDocument,
    hit: &CompactConcordanceHit,
    window_tokens: usize,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceRow, ConcordanceError> {
    let document_id = u32::try_from(document.document_index)
        .map_err(|_| ConcordanceError::InvalidDocumentIdentity)?;
    let node_end = hit.token_index + hit.node_token_length;
    let left_start = hit.token_index.saturating_sub(window_tokens);
    let right_end = node_end.saturating_add(window_tokens);
    let mut ordinal = 0usize;
    let mut left_context = Vec::new();
    let mut node_tokens = Vec::new();
    let mut right_context = Vec::new();
    let _stopped_after_window =
        visit_tokens(&document.text, document_id, tokenization_config, |token| {
            if !token.counts_as_word() {
                return Ok(());
            }
            if ordinal >= right_end {
                return Err(());
            }
            if (left_start..hit.token_index).contains(&ordinal) {
                left_context.push(token.surface.to_string());
            } else if (hit.token_index..node_end).contains(&ordinal) {
                node_tokens.push(token.surface.to_string());
            } else if (node_end..right_end).contains(&ordinal) {
                right_context.push(token.surface.to_string());
            }
            ordinal += 1;
            if ordinal >= right_end {
                Err(())
            } else {
                Ok(())
            }
        });
    if node_tokens.len() != hit.node_token_length {
        return Err(ConcordanceError::InvalidHitIdentity);
    }
    Ok(ConcordanceRow {
        document_index: document.document_index,
        source_path: document.source_path.clone(),
        display_name: document.display_name.clone(),
        token_index: hit.token_index,
        node_token_length: hit.node_token_length,
        left_context,
        node: node_tokens.join(" "),
        right_context,
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
mod tests;
