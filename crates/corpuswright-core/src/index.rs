use crate::analysis::concordance_key;
use crate::decode::DECODE_POLICY_VERSION;
use crate::tokenization::{TokenizationConfig, TokenizationManifest, tokenize, visit_tokens};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    time::Instant,
};

pub const INDEX_SCHEMA_VERSION: u32 = 4;
pub const SURFACE_INDEX_CACHE_FILE_VERSION: u32 = 2;

const SURFACE_INDEX_CACHE_MAGIC: &[u8; 8] = b"CWIIDX02";
const SURFACE_INDEX_CHECKSUM_BYTES: usize = 32;
const SURFACE_INDEX_MAX_MANIFEST_BYTES: u64 = 1_048_576;
const SURFACE_INDEX_MAX_RETAINED_FILES: usize = 2;
const SURFACE_INDEX_MAX_RETAINED_AGGREGATE_BYTES: u64 = 2_147_483_648;

const DEFAULT_WINDOW_TOKENS: usize = 5;
const MAX_WINDOW_TOKENS: usize = 20;

const DEFAULT_MAX_PROJECTED_RESIDENT_INDEX_BYTES: u64 = 1_073_741_824;
const DEFAULT_MAX_STAGED_PREPARED_TEXT_BYTES: u64 = 67_108_864;
const DEFAULT_MAX_STAGED_TOKEN_RECORDS: u64 = 1_000_000;
const DEFAULT_MAX_RETAINED_QUERY_SORT_RECORDS: u64 = 100_000;

#[allow(dead_code)]
struct LegacySurfaceIndexTokenRecordLayout {
    document_id: usize,
    token_ordinal_in_document: usize,
    surface_symbol_id: u32,
    normalised_surface_symbol_id: u32,
    exact_surface_symbol_id: u32,
    prepared_char_start: usize,
    prepared_char_end: usize,
    counts_as_word: bool,
}

#[allow(dead_code)]
struct LegacySurfacePostingLayout {
    document_id: usize,
    token_ordinal_in_document: usize,
}

/// Aggregate resource budget enforced by surface-index build and query paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceIndexResourceLimits {
    pub max_projected_resident_index_bytes: u64,
    pub max_staged_prepared_text_bytes: u64,
    pub max_staged_token_records: u64,
    pub max_retained_query_sort_records: u64,
}

impl Default for SurfaceIndexResourceLimits {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl SurfaceIndexResourceLimits {
    /// Conservative defaults can be overridden for diagnostics. Invalid values
    /// are ignored so deployment cannot accidentally disable a limit.
    pub fn from_environment() -> Self {
        Self {
            max_projected_resident_index_bytes: resource_limit_from_environment(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_RESIDENT_BYTES",
                DEFAULT_MAX_PROJECTED_RESIDENT_INDEX_BYTES,
            ),
            max_staged_prepared_text_bytes: resource_limit_from_environment(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TEXT_BYTES",
                DEFAULT_MAX_STAGED_PREPARED_TEXT_BYTES,
            ),
            max_staged_token_records: resource_limit_from_environment(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TOKEN_RECORDS",
                DEFAULT_MAX_STAGED_TOKEN_RECORDS,
            ),
            max_retained_query_sort_records: resource_limit_from_environment(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_SORT_RECORDS",
                DEFAULT_MAX_RETAINED_QUERY_SORT_RECORDS,
            ),
        }
    }
}

fn resource_limit_from_environment(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Conservative resident-memory projection used before corpus-proportional
/// allocation. `fixed_array_bytes` excludes strings and lookup-table slack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceIndexResourceEstimate {
    pub token_count: u64,
    pub document_count: u64,
    pub fixed_array_bytes: u64,
    pub symbol_utf8_bytes: u64,
    pub lookup_overhead_bytes: u64,
    pub projected_resident_bytes: u64,
}

impl SurfaceIndexResourceEstimate {
    /// Structural lower bound of the representation inherited by this branch:
    /// one token record plus two `(usize, usize)` postings per indexed token.
    /// Vec headers, allocation slack, symbols, documents and lookup maps are
    /// intentionally excluded, so this is evidence of a floor, not a forecast.
    pub fn legacy_structural_lower_bound(token_count: u64) -> Option<u64> {
        let bytes_per_token =
            u64::try_from(std::mem::size_of::<LegacySurfaceIndexTokenRecordLayout>())
                .ok()?
                .checked_add(
                    u64::try_from(std::mem::size_of::<LegacySurfacePostingLayout>())
                        .ok()?
                        .checked_mul(2)?,
                )?;
        token_count.checked_mul(bytes_per_token)
    }
}

/// Measurements recorded by bounded builders. Later commits populate the
/// phase timings and exact compact-store estimate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SurfaceIndexBuildStats {
    pub total_documents: u64,
    pub total_tokens: u64,
    pub distinct_surfaces: u64,
    pub distinct_normalised_keys: u64,
    pub maximum_staged_prepared_text_bytes: u64,
    pub maximum_staged_token_records: u64,
    pub final_estimated_resident_bytes: u64,
    pub large_documents_streamed: u64,
    pub tokenization_elapsed: Duration,
    pub postings_elapsed: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct SurfaceIndexCancellationToken(Arc<AtomicBool>);

impl SurfaceIndexCancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, AtomicOrdering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(AtomicOrdering::Acquire)
    }
}

impl From<Arc<AtomicBool>> for SurfaceIndexCancellationToken {
    fn from(cancel: Arc<AtomicBool>) -> Self {
        Self(cancel)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexDocumentInput<'text> {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub prepared_text: &'text str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceIndexDocument {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub global_token_start: u32,
    pub token_count: u32,
}

/// Compatibility alias for one entry in the compact token-surface stream.
pub type SurfaceIndexTokenRecord = u32;
/// One compact global token position in the single CSR postings population.
pub type SurfacePosting = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceIndexError {
    EmptyQuery,
    DuplicateDocumentId(usize),
    DocumentIdTooLarge(usize),
    InvalidPreparedOffset {
        document_id: usize,
        byte_start: usize,
        byte_end: usize,
    },
    InconsistentStore {
        document_id: usize,
        token_ordinal_in_document: usize,
        node_token_length: usize,
    },
    ProjectedResidentIndexExceedsLimit {
        projected_bytes: u64,
        limit_bytes: u64,
    },
    StagedBuildStateExceedsLimit {
        prepared_text_bytes: u64,
        token_records: u64,
        prepared_text_limit: u64,
        token_record_limit: u64,
    },
    TokenPopulationExceedsU32Capacity {
        token_count: u64,
    },
    RetainedSortDepthExceedsQueryBudget {
        requested_records: u64,
        limit_records: u64,
    },
    Cancelled,
    Allocation(String),
}

impl std::fmt::Display for SurfaceIndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyQuery => formatter.write_str("surface index query contains no word tokens"),
            Self::DuplicateDocumentId(document_id) => {
                write!(
                    formatter,
                    "duplicate surface index document id: {document_id}"
                )
            }
            Self::DocumentIdTooLarge(document_id) => write!(
                formatter,
                "surface index document id {document_id} cannot be passed to the canonical tokenizer"
            ),
            Self::InvalidPreparedOffset {
                document_id,
                byte_start,
                byte_end,
            } => write!(
                formatter,
                "canonical token offsets are not prepared-text character boundaries for document {document_id}: {byte_start}..{byte_end}"
            ),
            Self::InconsistentStore {
                document_id,
                token_ordinal_in_document,
                node_token_length,
            } => write!(
                formatter,
                "surface index store is missing token span document={document_id} ordinal={token_ordinal_in_document} length={node_token_length}"
            ),
            Self::ProjectedResidentIndexExceedsLimit {
                projected_bytes,
                limit_bytes,
            } => write!(
                formatter,
                "projected resident surface index size {projected_bytes} bytes exceeds configured limit {limit_bytes} bytes"
            ),
            Self::StagedBuildStateExceedsLimit {
                prepared_text_bytes,
                token_records,
                prepared_text_limit,
                token_record_limit,
            } => write!(
                formatter,
                "staged surface index build state exceeds its budget: prepared_text_bytes={prepared_text_bytes}/{prepared_text_limit}, token_records={token_records}/{token_record_limit}"
            ),
            Self::TokenPopulationExceedsU32Capacity { token_count } => write!(
                formatter,
                "surface index token population {token_count} exceeds the u32 position format capacity"
            ),
            Self::RetainedSortDepthExceedsQueryBudget {
                requested_records,
                limit_records,
            } => write!(
                formatter,
                "concordance sort requires {requested_records} retained records, exceeding configured limit {limit_records}"
            ),
            Self::Cancelled => formatter.write_str("surface index operation was cancelled"),
            Self::Allocation(detail) => {
                write!(formatter, "surface index allocation failed: {detail}")
            }
        }
    }
}

impl std::error::Error for SurfaceIndexError {}

impl SurfaceIndexError {
    pub fn is_resource_or_cancellation(&self) -> bool {
        matches!(
            self,
            Self::ProjectedResidentIndexExceedsLimit { .. }
                | Self::StagedBuildStateExceedsLimit { .. }
                | Self::TokenPopulationExceedsU32Capacity { .. }
                | Self::RetainedSortDepthExceedsQueryBudget { .. }
                | Self::Cancelled
        )
    }
}

pub trait IndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]>;
    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]>;
    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument>;
    fn document_and_ordinal_for_position(&self, position: u32) -> Option<(usize, usize)>;
    fn global_position_for_document_token(
        &self,
        document_id: usize,
        token_ordinal_in_document: usize,
    ) -> Option<u32>;
    fn total_indexed_tokens(&self) -> usize;
    fn total_indexed_documents(&self) -> usize;
    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store>;
    fn indexed_exact_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store>;
    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store>;
    fn surface_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
    ) -> Option<&'store str>;
    fn concordance_key_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<&'store str>;
    fn concordance_symbol_id_for_record(
        &self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<u32>;
    fn concordance_symbol_id(&self, key: &str, case_sensitive: bool) -> Option<u32>;
    fn concordance_symbols<'store>(
        &'store self,
        case_sensitive: bool,
    ) -> Box<dyn Iterator<Item = &'store str> + 'store>;

    fn document_frequency(&self, normalised_surface: &str) -> usize {
        let Some(postings) = self.postings(normalised_surface) else {
            return 0;
        };
        let mut last_document_id = None;
        let mut count = 0usize;
        for posting in postings {
            let Some((document_id, _)) = self.document_and_ordinal_for_position(*posting) else {
                continue;
            };
            if last_document_id != Some(document_id) {
                count += 1;
                last_document_id = Some(document_id);
            }
        }
        count
    }

    fn postings_for_concordance_key(
        &self,
        concordance_key: &str,
        case_sensitive: bool,
    ) -> Option<&[SurfacePosting]> {
        let normalised =
            case_sensitive.then(|| crate::analysis::concordance_key(concordance_key, false));
        self.postings(normalised.as_deref().unwrap_or(concordance_key))
    }

    fn indexed_terms_for_concordance_key<'store>(
        &'store self,
        case_sensitive: bool,
    ) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        if case_sensitive {
            self.indexed_exact_terms()
        } else {
            self.indexed_terms()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InMemoryIndexStore {
    documents: Vec<SurfaceIndexDocument>,
    surface_symbols: Vec<String>,
    normalised_symbols: Vec<String>,
    surface_to_normalised: Vec<u32>,
    token_surface_ids: Vec<u32>,
    postings_offsets: Vec<u64>,
    postings_positions: Vec<u32>,
    #[serde(skip)]
    surface_ids_by_hash: HashMap<u64, Vec<u32>>,
    #[serde(skip)]
    normalised_ids_by_hash: HashMap<u64, Vec<u32>>,
}

pub struct SurfaceIndexBuilder {
    store: InMemoryIndexStore,
    tokenization_config: TokenizationConfig,
    limits: SurfaceIndexResourceLimits,
    cancellation: SurfaceIndexCancellationToken,
    stats: SurfaceIndexBuildStats,
    last_document_id: Option<usize>,
}

impl SurfaceIndexBuilder {
    pub fn new(
        tokenization_config: TokenizationConfig,
        limits: SurfaceIndexResourceLimits,
    ) -> Self {
        Self::with_cancellation(
            tokenization_config,
            limits,
            SurfaceIndexCancellationToken::default(),
        )
    }

    pub fn with_cancellation(
        tokenization_config: TokenizationConfig,
        limits: SurfaceIndexResourceLimits,
        cancellation: SurfaceIndexCancellationToken,
    ) -> Self {
        Self {
            store: InMemoryIndexStore::default(),
            tokenization_config,
            limits,
            cancellation,
            stats: SurfaceIndexBuildStats::default(),
            last_document_id: None,
        }
    }

    pub fn add_document(
        &mut self,
        document: &SurfaceIndexDocumentInput<'_>,
    ) -> Result<(), SurfaceIndexError> {
        if self.cancellation.is_cancelled() {
            return Err(SurfaceIndexError::Cancelled);
        }
        if self
            .last_document_id
            .is_some_and(|last| document.document_id <= last)
        {
            return Err(SurfaceIndexError::DuplicateDocumentId(document.document_id));
        }
        let tokenizer_document_id = u32::try_from(document.document_id)
            .map_err(|_| SurfaceIndexError::DocumentIdTooLarge(document.document_id))?;
        let prepared_text_bytes = document.prepared_text.len() as u64;
        if prepared_text_bytes > self.limits.max_staged_prepared_text_bytes {
            return Err(SurfaceIndexError::StagedBuildStateExceedsLimit {
                prepared_text_bytes,
                token_records: 0,
                prepared_text_limit: self.limits.max_staged_prepared_text_bytes,
                token_record_limit: self.limits.max_staged_token_records,
            });
        }
        self.stats.maximum_staged_prepared_text_bytes = self
            .stats
            .maximum_staged_prepared_text_bytes
            .max(prepared_text_bytes);
        if prepared_text_bytes > self.limits.max_staged_prepared_text_bytes / 2 {
            self.stats.large_documents_streamed += 1;
        }

        let global_token_start =
            u32::try_from(self.store.token_surface_ids.len()).map_err(|_| {
                SurfaceIndexError::TokenPopulationExceedsU32Capacity {
                    token_count: self.store.token_surface_ids.len() as u64,
                }
            })?;
        let tokenization_started = Instant::now();
        let mut tokens_seen = 0usize;
        let tokenization_config = self.tokenization_config;
        visit_tokens(
            document.prepared_text,
            tokenizer_document_id,
            &tokenization_config,
            |token| {
                tokens_seen += 1;
                if tokens_seen.is_multiple_of(1_024) && self.cancellation.is_cancelled() {
                    return Err(SurfaceIndexError::Cancelled);
                }
                if !token.counts_as_word() {
                    return Ok(());
                }
                if self.store.token_surface_ids.len() >= u32::MAX as usize {
                    return Err(SurfaceIndexError::TokenPopulationExceedsU32Capacity {
                        token_count: self.store.token_surface_ids.len() as u64 + 1,
                    });
                }
                let surface_symbol_id = self.store.intern_surface(token.surface)?;
                try_reserve(&mut self.store.token_surface_ids, 1, "token surface ids")?;
                self.store.token_surface_ids.push(surface_symbol_id);
                if self.store.token_surface_ids.len().is_multiple_of(4_096) {
                    self.enforce_resident_limit()?;
                }
                Ok(())
            },
        )?;
        self.stats.tokenization_elapsed += tokenization_started.elapsed();

        let token_count =
            u32::try_from(self.store.token_surface_ids.len() - global_token_start as usize)
                .map_err(|_| SurfaceIndexError::TokenPopulationExceedsU32Capacity {
                    token_count: self.store.token_surface_ids.len() as u64,
                })?;
        try_reserve(&mut self.store.documents, 1, "indexed documents")?;
        self.store.documents.push(SurfaceIndexDocument {
            document_id: document.document_id,
            source_path: fallible_string_clone(&document.source_path, "source path")?,
            display_name: fallible_string_clone(&document.display_name, "display name")?,
            global_token_start,
            token_count,
        });
        self.last_document_id = Some(document.document_id);
        self.stats.total_documents += 1;
        self.stats.total_tokens += token_count as u64;
        self.enforce_resident_limit()
    }

    fn enforce_resident_limit(&self) -> Result<(), SurfaceIndexError> {
        let projected_bytes = self
            .store
            .estimated_resident_bytes()
            .saturating_add(
                (self.store.token_surface_ids.len() * std::mem::size_of::<u32>()) as u64,
            )
            .saturating_add(
                ((self.store.normalised_symbols.len() + 1) * std::mem::size_of::<u64>()) as u64,
            );
        if projected_bytes > self.limits.max_projected_resident_index_bytes {
            return Err(SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                projected_bytes,
                limit_bytes: self.limits.max_projected_resident_index_bytes,
            });
        }
        Ok(())
    }

    pub fn finish(
        mut self,
    ) -> Result<(InMemoryIndexStore, SurfaceIndexBuildStats), SurfaceIndexError> {
        self.enforce_resident_limit()?;
        let postings_started = Instant::now();
        self.store.build_postings(&self.cancellation)?;
        self.stats.postings_elapsed = postings_started.elapsed();
        self.stats.distinct_surfaces = self.store.surface_symbols.len() as u64;
        self.stats.distinct_normalised_keys = self.store.normalised_symbols.len() as u64;
        self.stats.final_estimated_resident_bytes = self.store.estimated_resident_bytes();
        if self.stats.final_estimated_resident_bytes
            > self.limits.max_projected_resident_index_bytes
        {
            return Err(SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                projected_bytes: self.stats.final_estimated_resident_bytes,
                limit_bytes: self.limits.max_projected_resident_index_bytes,
            });
        }
        debug_assert!(self.store.has_valid_internal_references());
        Ok((self.store, self.stats))
    }
}

impl InMemoryIndexStore {
    pub fn build(
        documents: &[SurfaceIndexDocumentInput<'_>],
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        Self::build_with_limits(
            documents,
            tokenization_config,
            SurfaceIndexResourceLimits::default(),
        )
    }

    pub fn build_with_limits(
        documents: &[SurfaceIndexDocumentInput<'_>],
        tokenization_config: &TokenizationConfig,
        limits: SurfaceIndexResourceLimits,
    ) -> Result<Self, SurfaceIndexError> {
        Self::build_with_limits_and_stats(documents, tokenization_config, limits)
            .map(|(store, _)| store)
    }

    pub fn build_with_limits_and_stats(
        documents: &[SurfaceIndexDocumentInput<'_>],
        tokenization_config: &TokenizationConfig,
        limits: SurfaceIndexResourceLimits,
    ) -> Result<(Self, SurfaceIndexBuildStats), SurfaceIndexError> {
        let mut sorted_documents = Vec::new();
        try_reserve(
            &mut sorted_documents,
            documents.len(),
            "document input ordering",
        )?;
        sorted_documents.extend(documents.iter());
        sorted_documents.sort_by_key(|document| document.document_id);
        for pair in sorted_documents.windows(2) {
            if pair[0].document_id == pair[1].document_id {
                return Err(SurfaceIndexError::DuplicateDocumentId(pair[0].document_id));
            }
        }

        let mut builder = SurfaceIndexBuilder::new(*tokenization_config, limits);
        for document in sorted_documents {
            builder.add_document(document)?;
        }
        builder.finish()
    }

    fn intern_surface(&mut self, symbol: &str) -> Result<u32, SurfaceIndexError> {
        let hash = symbol_hash(symbol);
        if let Some(ids) = self.surface_ids_by_hash.get(&hash) {
            for &id in ids {
                if self
                    .surface_symbols
                    .get(id as usize)
                    .is_some_and(|known| known == symbol)
                {
                    return Ok(id);
                }
            }
        }

        let normalised = concordance_key(symbol, false);
        let normalised_id = self.intern_normalised(&normalised)?;
        let id = u32::try_from(self.surface_symbols.len()).map_err(|_| {
            SurfaceIndexError::Allocation("surface symbol table exceeds u32 capacity".to_string())
        })?;
        try_reserve(&mut self.surface_symbols, 1, "surface symbol table")?;
        try_reserve(
            &mut self.surface_to_normalised,
            1,
            "surface normalisation map",
        )?;
        self.surface_symbols
            .push(fallible_string_clone(symbol, "interned symbol")?);
        self.surface_to_normalised.push(normalised_id);

        if !self.surface_ids_by_hash.contains_key(&hash) {
            self.surface_ids_by_hash
                .try_reserve(1)
                .map_err(|error| allocation_error("surface symbol hash table", error))?;
        }
        let ids = self.surface_ids_by_hash.entry(hash).or_default();
        try_reserve(ids, 1, "surface symbol collision bucket")?;
        ids.push(id);
        Ok(id)
    }

    fn intern_normalised(&mut self, symbol: &str) -> Result<u32, SurfaceIndexError> {
        let hash = symbol_hash(symbol);
        if let Some(id) = self.normalised_ids_by_hash.get(&hash).and_then(|ids| {
            ids.iter().copied().find(|&id| {
                self.normalised_symbols
                    .get(id as usize)
                    .is_some_and(|known| known == symbol)
            })
        }) {
            return Ok(id);
        }
        let id = u32::try_from(self.normalised_symbols.len()).map_err(|_| {
            SurfaceIndexError::Allocation(
                "normalised symbol table exceeds u32 capacity".to_string(),
            )
        })?;
        try_reserve(&mut self.normalised_symbols, 1, "normalised symbol table")?;
        self.normalised_symbols
            .push(fallible_string_clone(symbol, "normalised symbol")?);
        if !self.normalised_ids_by_hash.contains_key(&hash) {
            self.normalised_ids_by_hash
                .try_reserve(1)
                .map_err(|error| allocation_error("normalised symbol hash table", error))?;
        }
        let ids = self.normalised_ids_by_hash.entry(hash).or_default();
        try_reserve(ids, 1, "normalised symbol collision bucket")?;
        ids.push(id);
        Ok(id)
    }

    fn normalised_symbol_id(&self, symbol: &str) -> Option<u32> {
        lookup_symbol_id(
            &self.normalised_symbols,
            &self.normalised_ids_by_hash,
            symbol,
        )
    }

    fn surface_symbol_id(&self, symbol: &str) -> Option<u32> {
        lookup_symbol_id(&self.surface_symbols, &self.surface_ids_by_hash, symbol)
    }

    pub(crate) fn rebuild_symbol_lookup(&mut self) -> Result<(), SurfaceIndexError> {
        rebuild_symbol_lookup(
            &self.surface_symbols,
            &mut self.surface_ids_by_hash,
            "surface symbol",
        )?;
        rebuild_symbol_lookup(
            &self.normalised_symbols,
            &mut self.normalised_ids_by_hash,
            "normalised symbol",
        )?;
        Ok(())
    }

    fn build_postings(
        &mut self,
        cancellation: &SurfaceIndexCancellationToken,
    ) -> Result<(), SurfaceIndexError> {
        let mut counts = Vec::<u64>::new();
        try_reserve(&mut counts, self.normalised_symbols.len(), "posting counts")?;
        counts.resize(self.normalised_symbols.len(), 0);
        for (index, &surface_id) in self.token_surface_ids.iter().enumerate() {
            if index % 16_384 == 0 && cancellation.is_cancelled() {
                return Err(SurfaceIndexError::Cancelled);
            }
            let normalised_id = self.surface_to_normalised[surface_id as usize] as usize;
            counts[normalised_id] = counts[normalised_id].checked_add(1).ok_or_else(|| {
                SurfaceIndexError::Allocation("posting count overflow".to_string())
            })?;
        }
        try_reserve(
            &mut self.postings_offsets,
            counts.len().saturating_add(1),
            "posting offsets",
        )?;
        self.postings_offsets.push(0);
        for (index, count) in counts.into_iter().enumerate() {
            if index % 16_384 == 0 && cancellation.is_cancelled() {
                return Err(SurfaceIndexError::Cancelled);
            }
            let next = self
                .postings_offsets
                .last()
                .copied()
                .unwrap_or(0)
                .checked_add(count)
                .ok_or_else(|| {
                    SurfaceIndexError::Allocation("posting offset overflow".to_string())
                })?;
            self.postings_offsets.push(next);
        }
        try_reserve(
            &mut self.postings_positions,
            self.token_surface_ids.len(),
            "posting positions",
        )?;
        self.postings_positions
            .resize(self.token_surface_ids.len(), 0);
        let mut cursors = self.postings_offsets[..self.normalised_symbols.len()].to_vec();
        for (position, &surface_id) in self.token_surface_ids.iter().enumerate() {
            if position % 16_384 == 0 && cancellation.is_cancelled() {
                return Err(SurfaceIndexError::Cancelled);
            }
            let position = u32::try_from(position).map_err(|_| {
                SurfaceIndexError::TokenPopulationExceedsU32Capacity {
                    token_count: self.token_surface_ids.len() as u64,
                }
            })?;
            let normalised_id = self.surface_to_normalised[surface_id as usize] as usize;
            let cursor = cursors[normalised_id] as usize;
            self.postings_positions[cursor] = position;
            cursors[normalised_id] += 1;
        }
        Ok(())
    }

    /// Bytes occupied by compact numeric arrays. This includes vocabulary-sized
    /// maps as well as the two corpus-sized `u32` arrays.
    pub fn fixed_array_bytes(&self) -> u64 {
        (self.surface_to_normalised.len() * std::mem::size_of::<u32>()
            + self.token_surface_ids.len() * std::mem::size_of::<u32>()
            + self.postings_offsets.len() * std::mem::size_of::<u64>()
            + self.postings_positions.len() * std::mem::size_of::<u32>()) as u64
    }

    pub fn symbol_utf8_bytes(&self) -> u64 {
        self.surface_symbols
            .iter()
            .chain(&self.normalised_symbols)
            .map(|symbol| symbol.len() as u64)
            .sum()
    }

    /// Conservative lookup estimate: hash entries, collision IDs and String
    /// headers. Allocator-specific bucket slack is represented by a 2x factor.
    pub fn lookup_overhead_estimate(&self) -> u64 {
        let symbols = (self.surface_symbols.len() + self.normalised_symbols.len()) as u64;
        symbols.saturating_mul(
            (2 * (std::mem::size_of::<u64>() + std::mem::size_of::<Vec<u32>>())
                + std::mem::size_of::<String>()) as u64,
        )
    }

    pub fn estimated_resident_bytes(&self) -> u64 {
        let document_metadata = self.documents.iter().fold(0u64, |total, document| {
            total
                .saturating_add(std::mem::size_of::<SurfaceIndexDocument>() as u64)
                .saturating_add(document.source_path.len() as u64)
                .saturating_add(document.display_name.len() as u64)
        });
        self.fixed_array_bytes()
            .saturating_add(self.symbol_utf8_bytes())
            .saturating_add(self.lookup_overhead_estimate())
            .saturating_add(document_metadata)
    }

    #[cfg(test)]
    fn total_postings(&self) -> usize {
        self.postings_positions.len()
    }

    pub(crate) fn refresh_document_metadata(
        &mut self,
        inputs: &[SurfaceIndexDocumentInput<'_>],
    ) -> Result<(), SurfaceIndexError> {
        if inputs.len() != self.documents.len() {
            return Err(SurfaceIndexError::InconsistentStore {
                document_id: 0,
                token_ordinal_in_document: 0,
                node_token_length: 0,
            });
        }
        for input in inputs {
            let document = self.document_mut(input.document_id).ok_or(
                SurfaceIndexError::InconsistentStore {
                    document_id: input.document_id,
                    token_ordinal_in_document: 0,
                    node_token_length: 0,
                },
            )?;
            document.source_path = fallible_string_clone(&input.source_path, "source path")?;
            document.display_name = fallible_string_clone(&input.display_name, "display name")?;
        }
        Ok(())
    }

    fn document_mut(&mut self, document_id: usize) -> Option<&mut SurfaceIndexDocument> {
        self.documents
            .binary_search_by_key(&document_id, |document| document.document_id)
            .ok()
            .map(|index| &mut self.documents[index])
    }

    fn has_valid_internal_references(&self) -> bool {
        if self.surface_to_normalised.len() != self.surface_symbols.len()
            || self.postings_offsets.len() != self.normalised_symbols.len().saturating_add(1)
            || self.postings_positions.len() != self.token_surface_ids.len()
            || self
                .documents
                .windows(2)
                .any(|pair| pair[0].document_id >= pair[1].document_id)
        {
            return false;
        }
        let mut token_total = 0u64;
        for document in &self.documents {
            if document.global_token_start as u64 != token_total {
                return false;
            }
            let Some(next_total) = token_total.checked_add(document.token_count as u64) else {
                return false;
            };
            token_total = next_total;
        }
        if token_total != self.token_surface_ids.len() as u64
            || self
                .surface_to_normalised
                .iter()
                .any(|id| *id as usize >= self.normalised_symbols.len())
            || self
                .token_surface_ids
                .iter()
                .any(|id| *id as usize >= self.surface_symbols.len())
            || self.postings_offsets.first() != Some(&0)
            || self.postings_offsets.last().copied() != Some(self.postings_positions.len() as u64)
            || self
                .postings_offsets
                .windows(2)
                .any(|pair| pair[0] > pair[1])
            || self
                .postings_positions
                .iter()
                .any(|position| *position as usize >= self.token_surface_ids.len())
        {
            return false;
        }
        for normalised_id in 0..self.normalised_symbols.len() {
            let start = self.postings_offsets[normalised_id] as usize;
            let end = self.postings_offsets[normalised_id + 1] as usize;
            let postings = &self.postings_positions[start..end];
            if postings.windows(2).any(|pair| pair[0] >= pair[1]) {
                return false;
            }
            if postings.iter().any(|position| {
                let surface_id = self.token_surface_ids[*position as usize] as usize;
                self.surface_to_normalised[surface_id] as usize != normalised_id
            }) {
                return false;
            }
        }
        true
    }
}

impl IndexStore for InMemoryIndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]> {
        let symbol_id = self.normalised_symbol_id(normalised_surface)? as usize;
        let start = *self.postings_offsets.get(symbol_id)? as usize;
        let end = *self.postings_offsets.get(symbol_id + 1)? as usize;
        (start != end).then_some(&self.postings_positions[start..end])
    }

    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]> {
        let document = self.document(document_id)?;
        let start = document.global_token_start as usize;
        let end = start.checked_add(document.token_count as usize)?;
        self.token_surface_ids.get(start..end)
    }

    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument> {
        self.documents
            .binary_search_by_key(&document_id, |document| document.document_id)
            .ok()
            .map(|index| &self.documents[index])
    }

    fn total_indexed_tokens(&self) -> usize {
        self.token_surface_ids.len()
    }

    fn total_indexed_documents(&self) -> usize {
        self.documents.len()
    }

    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        Box::new(
            self.normalised_symbols
                .iter()
                .enumerate()
                .filter_map(|(index, symbol)| {
                    (self.postings_offsets[index] != self.postings_offsets[index + 1])
                        .then_some(symbol.as_str())
                }),
        )
    }

    fn indexed_exact_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        Box::new(self.surface_symbols.iter().map(String::as_str))
    }

    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store> {
        Box::new(self.documents.iter())
    }

    fn surface_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
    ) -> Option<&'store str> {
        self.surface_symbols
            .get(*record as usize)
            .map(String::as_str)
    }

    fn concordance_key_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<&'store str> {
        if case_sensitive {
            self.surface_for_record(record)
        } else {
            let normalised_id = *self.surface_to_normalised.get(*record as usize)?;
            self.normalised_symbols
                .get(normalised_id as usize)
                .map(String::as_str)
        }
    }

    fn concordance_symbol_id_for_record(
        &self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<u32> {
        if case_sensitive {
            self.surface_symbols.get(*record as usize).map(|_| *record)
        } else {
            self.surface_to_normalised.get(*record as usize).copied()
        }
    }

    fn concordance_symbol_id(&self, key: &str, case_sensitive: bool) -> Option<u32> {
        if case_sensitive {
            self.surface_symbol_id(key)
        } else {
            self.normalised_symbol_id(key)
        }
    }

    fn concordance_symbols<'store>(
        &'store self,
        case_sensitive: bool,
    ) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        if case_sensitive {
            Box::new(self.surface_symbols.iter().map(String::as_str))
        } else {
            Box::new(self.normalised_symbols.iter().map(String::as_str))
        }
    }

    fn document_and_ordinal_for_position(&self, position: u32) -> Option<(usize, usize)> {
        if position as usize >= self.token_surface_ids.len() {
            return None;
        }
        let index = self
            .documents
            .partition_point(|document| document.global_token_start <= position)
            .checked_sub(1)?;
        let document = &self.documents[index];
        let ordinal = position.checked_sub(document.global_token_start)? as usize;
        (ordinal < document.token_count as usize).then_some((document.document_id, ordinal))
    }

    fn global_position_for_document_token(
        &self,
        document_id: usize,
        token_ordinal_in_document: usize,
    ) -> Option<u32> {
        let document = self.document(document_id)?;
        if token_ordinal_in_document >= document.token_count as usize {
            return None;
        }
        document
            .global_token_start
            .checked_add(u32::try_from(token_ordinal_in_document).ok()?)
    }
}

fn lookup_symbol_id(
    symbols: &[String],
    ids_by_hash: &HashMap<u64, Vec<u32>>,
    symbol: &str,
) -> Option<u32> {
    ids_by_hash.get(&symbol_hash(symbol)).and_then(|ids| {
        ids.iter().copied().find(|id| {
            symbols
                .get(*id as usize)
                .is_some_and(|known| known == symbol)
        })
    })
}

fn rebuild_symbol_lookup(
    symbols: &[String],
    ids_by_hash: &mut HashMap<u64, Vec<u32>>,
    purpose: &str,
) -> Result<(), SurfaceIndexError> {
    ids_by_hash.clear();
    ids_by_hash
        .try_reserve(symbols.len())
        .map_err(|error| allocation_error(&format!("{purpose} hash table"), error))?;
    for (index, symbol) in symbols.iter().enumerate() {
        let id = u32::try_from(index).map_err(|_| {
            SurfaceIndexError::Allocation(format!("{purpose} table exceeds u32 capacity"))
        })?;
        let ids = ids_by_hash.entry(symbol_hash(symbol)).or_default();
        try_reserve(ids, 1, &format!("{purpose} collision bucket"))?;
        ids.push(id);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexTerm {
    pub surface: String,
    pub normalised_surface: String,
    pub exact_surface: String,
}

impl SurfaceIndexTerm {
    fn concordance_key(&self, case_sensitive: bool) -> &str {
        if case_sensitive {
            &self.exact_surface
        } else {
            &self.normalised_surface
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceIndexQuery {
    Term {
        term: SurfaceIndexTerm,
        case_sensitive: bool,
    },
    Phrase {
        terms: Vec<SurfaceIndexTerm>,
        case_sensitive: bool,
    },
}

impl SurfaceIndexQuery {
    pub fn from_surface_text(
        query: &str,
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        Self::from_surface_text_with_case(query, false, tokenization_config)
    }

    pub fn from_surface_text_with_case(
        query: &str,
        case_sensitive: bool,
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        let terms = query_terms(query, tokenization_config)?;
        if terms.len() == 1 {
            Ok(Self::Term {
                term: terms.into_iter().next().expect("term exists"),
                case_sensitive,
            })
        } else {
            Ok(Self::Phrase {
                terms,
                case_sensitive,
            })
        }
    }

    pub fn terms(&self) -> &[SurfaceIndexTerm] {
        match self {
            Self::Term { term, .. } => std::slice::from_ref(term),
            Self::Phrase { terms, .. } => terms.as_slice(),
        }
    }

    pub fn case_sensitive(&self) -> bool {
        match self {
            Self::Term { case_sensitive, .. } | Self::Phrase { case_sensitive, .. } => {
                *case_sensitive
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexHit {
    pub document_id: usize,
    pub token_ordinal_in_document: usize,
    pub node_token_length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceIndexSearchResult {
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub selected_documents_total: usize,
    pub selected_tokens_total: usize,
    pub top_document_share: f64,
    pub document_hit_counts: BTreeMap<usize, u64>,
    pub hits: Vec<SurfaceIndexHit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexKwicRow {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub token_ordinal_in_document: usize,
    pub node_token_length: usize,
    pub left_context: Vec<String>,
    pub node: String,
    pub right_context: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceIndexKwicResult {
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub selected_documents_total: usize,
    pub selected_tokens_total: usize,
    pub top_document_share: f64,
    pub document_hit_counts: BTreeMap<usize, u64>,
    pub rows: Vec<SurfaceIndexKwicRow>,
}

pub fn search_surface_index(
    store: &impl IndexStore,
    query: &SurfaceIndexQuery,
) -> Result<SurfaceIndexSearchResult, SurfaceIndexError> {
    let terms = query.terms();
    if terms.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }

    let case_sensitive = query.case_sensitive();
    let mut hits = if terms.len() == 1 {
        term_hits(store, &terms[0], case_sensitive)
    } else {
        phrase_hits(store, terms, case_sensitive)
    };
    hits.sort_by(|left, right| {
        left.document_id.cmp(&right.document_id).then_with(|| {
            left.token_ordinal_in_document
                .cmp(&right.token_ordinal_in_document)
        })
    });

    let mut document_hit_counts = BTreeMap::<usize, u64>::new();
    for hit in &hits {
        *document_hit_counts.entry(hit.document_id).or_default() += 1;
    }
    let top_document_share = if hits.is_empty() {
        0.0
    } else {
        document_hit_counts.values().copied().max().unwrap_or(0) as f64 / hits.len() as f64
    };

    Ok(SurfaceIndexSearchResult {
        total_hits: hits.len(),
        documents_with_hits: document_hit_counts.len(),
        selected_documents_total: store.total_indexed_documents(),
        selected_tokens_total: store.total_indexed_tokens(),
        top_document_share,
        document_hit_counts,
        hits,
    })
}

pub fn search_surface_index_kwic(
    store: &impl IndexStore,
    query: &SurfaceIndexQuery,
    window_tokens: usize,
) -> Result<SurfaceIndexKwicResult, SurfaceIndexError> {
    let window_tokens = surface_index_window_tokens(window_tokens);
    let search = search_surface_index(store, query)?;
    let mut rows = Vec::new();
    try_reserve(&mut rows, search.hits.len(), "materialized KWIC rows")?;
    for hit in &search.hits {
        rows.push(materialize_surface_index_kwic_row(
            store,
            hit,
            window_tokens,
        )?);
    }

    Ok(SurfaceIndexKwicResult {
        total_hits: search.total_hits,
        documents_with_hits: search.documents_with_hits,
        selected_documents_total: search.selected_documents_total,
        selected_tokens_total: search.selected_tokens_total,
        top_document_share: search.top_document_share,
        document_hit_counts: search.document_hit_counts,
        rows,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceIndexManifest {
    pub schema_version: u32,
    pub processed_texts_hash_sha256: String,
    pub decode_policy_version: u32,
    pub tokenization: TokenizationManifest,
}

#[derive(Debug)]
pub enum SurfaceIndexCacheLoad {
    Ready(Box<InMemoryIndexStore>),
    Missing,
    Stale,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexCacheError(String);

impl std::fmt::Display for SurfaceIndexCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SurfaceIndexCacheError {}

struct ChecksummedWriter<W> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for ChecksummedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write> ChecksummedWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> std::io::Result<W> {
        let Self { mut inner, hasher } = self;
        inner.write_all(&hasher.finalize())?;
        inner.flush()?;
        Ok(inner)
    }
}

struct ChecksummedReader<R> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> Read for ChecksummedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        Ok(read)
    }
}

impl<R: Read> ChecksummedReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn verify_trailer(self) -> std::io::Result<()> {
        let Self { mut inner, hasher } = self;
        let expected = hasher.finalize();
        let mut actual = [0u8; SURFACE_INDEX_CHECKSUM_BYTES];
        inner.read_exact(&mut actual)?;
        if actual.as_slice() != expected.as_slice() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "surface index checksum mismatch",
            ));
        }
        let mut trailing = [0u8; 1];
        if inner.read(&mut trailing)? != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "surface index contains trailing data",
            ));
        }
        Ok(())
    }
}

pub fn surface_index_cache_path(cache_root: &Path, manifest: &SurfaceIndexManifest) -> PathBuf {
    let mut key_bytes = Vec::new();
    key_bytes.extend_from_slice(b"corpuswright-surface-index-cache-key-v2\0");
    key_bytes.extend_from_slice(&SURFACE_INDEX_CACHE_FILE_VERSION.to_le_bytes());
    if let Ok(manifest_bytes) = serde_json::to_vec(manifest) {
        key_bytes.extend_from_slice(&manifest_bytes);
    }
    let key = crate::hash::sha256_hex(&key_bytes);
    cache_root.join(format!(
        "surface-index.v{}.{}.cwi",
        SURFACE_INDEX_CACHE_FILE_VERSION, key
    ))
}

pub fn write_surface_index_cache(
    cache_root: &Path,
    manifest: &SurfaceIndexManifest,
    store: &InMemoryIndexStore,
) -> Result<(), SurfaceIndexCacheError> {
    std::fs::create_dir_all(cache_root)
        .map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    let final_path = surface_index_cache_path(cache_root, manifest);
    let temp_path = final_path.with_extension(format!("{}.tmp", std::process::id()));
    let file =
        File::create(&temp_path).map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    if manifest_bytes.len() as u64 > SURFACE_INDEX_MAX_MANIFEST_BYTES {
        return Err(SurfaceIndexCacheError(
            "surface index manifest exceeds binary cache limit".to_string(),
        ));
    }
    let document_string_bytes = store
        .documents
        .iter()
        .try_fold(0u64, |total, document| {
            total
                .checked_add(document.source_path.len() as u64)?
                .checked_add(document.display_name.len() as u64)
        })
        .ok_or_else(|| SurfaceIndexCacheError("document string byte count overflow".to_string()))?;
    let surface_utf8_bytes = store
        .surface_symbols
        .iter()
        .try_fold(0u64, |total, symbol| total.checked_add(symbol.len() as u64))
        .ok_or_else(|| SurfaceIndexCacheError("surface symbol byte count overflow".to_string()))?;
    let normalised_utf8_bytes = store
        .normalised_symbols
        .iter()
        .try_fold(0u64, |total, symbol| total.checked_add(symbol.len() as u64))
        .ok_or_else(|| {
            SurfaceIndexCacheError("normalised symbol byte count overflow".to_string())
        })?;

    let mut writer = ChecksummedWriter::new(BufWriter::new(file));
    writer
        .write_all(SURFACE_INDEX_CACHE_MAGIC)
        .map_err(cache_io_error)?;
    write_cache_u32(&mut writer, SURFACE_INDEX_CACHE_FILE_VERSION)?;
    write_cache_u32(&mut writer, INDEX_SCHEMA_VERSION)?;
    for value in [
        manifest_bytes.len() as u64,
        store.documents.len() as u64,
        store.surface_symbols.len() as u64,
        store.normalised_symbols.len() as u64,
        store.surface_to_normalised.len() as u64,
        store.token_surface_ids.len() as u64,
        store.postings_offsets.len() as u64,
        store.postings_positions.len() as u64,
        document_string_bytes,
        surface_utf8_bytes,
        normalised_utf8_bytes,
    ] {
        write_cache_u64(&mut writer, value)?;
    }
    writer.write_all(&manifest_bytes).map_err(cache_io_error)?;
    for document in &store.documents {
        write_cache_u64(&mut writer, document.document_id as u64)?;
        write_cache_u32(&mut writer, document.global_token_start)?;
        write_cache_u32(&mut writer, document.token_count)?;
        write_cache_string(&mut writer, &document.source_path)?;
        write_cache_string(&mut writer, &document.display_name)?;
    }
    for symbol in &store.surface_symbols {
        write_cache_string(&mut writer, symbol)?;
    }
    for symbol in &store.normalised_symbols {
        write_cache_string(&mut writer, symbol)?;
    }
    for &value in &store.surface_to_normalised {
        write_cache_u32(&mut writer, value)?;
    }
    for &value in &store.token_surface_ids {
        write_cache_u32(&mut writer, value)?;
    }
    for &value in &store.postings_offsets {
        write_cache_u64(&mut writer, value)?;
    }
    for &value in &store.postings_positions {
        write_cache_u32(&mut writer, value)?;
    }
    let mut writer = writer.finish().map_err(cache_io_error)?;
    writer.flush().map_err(cache_io_error)?;
    writer.get_ref().sync_all().map_err(cache_io_error)?;
    drop(writer);
    std::fs::rename(&temp_path, &final_path).map_err(|error| {
        let _ = std::fs::remove_file(&temp_path);
        SurfaceIndexCacheError(format!("cache rename failed: {error}"))
    })?;
    if let Ok(directory) = File::open(cache_root) {
        let _ = directory.sync_all();
    }
    prune_surface_index_cache(cache_root, Some(&final_path));
    Ok(())
}

pub fn load_surface_index_cache(
    cache_root: &Path,
    expected_manifest: &SurfaceIndexManifest,
) -> SurfaceIndexCacheLoad {
    load_surface_index_cache_with_limits(
        cache_root,
        expected_manifest,
        SurfaceIndexResourceLimits::default(),
    )
}

pub fn load_surface_index_cache_with_limits(
    cache_root: &Path,
    expected_manifest: &SurfaceIndexManifest,
    limits: SurfaceIndexResourceLimits,
) -> SurfaceIndexCacheLoad {
    let path = surface_index_cache_path(cache_root, expected_manifest);
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return SurfaceIndexCacheLoad::Missing;
        }
        Err(_) => return SurfaceIndexCacheLoad::Invalid,
    };
    match read_surface_index_cache(file, expected_manifest, limits) {
        Ok(store) => {
            prune_surface_index_cache(cache_root, Some(&path));
            SurfaceIndexCacheLoad::Ready(Box::new(store))
        }
        Err(CacheReadFailure::Stale) => SurfaceIndexCacheLoad::Stale,
        Err(CacheReadFailure::Invalid) => SurfaceIndexCacheLoad::Invalid,
    }
}

#[derive(Debug, Clone, Copy)]
enum CacheReadFailure {
    Stale,
    Invalid,
}

fn read_surface_index_cache(
    file: File,
    expected_manifest: &SurfaceIndexManifest,
    limits: SurfaceIndexResourceLimits,
) -> Result<InMemoryIndexStore, CacheReadFailure> {
    let mut reader = ChecksummedReader::new(BufReader::new(file));
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|_| CacheReadFailure::Invalid)?;
    if &magic != SURFACE_INDEX_CACHE_MAGIC {
        return Err(CacheReadFailure::Invalid);
    }
    let cache_version = read_cache_u32(&mut reader)?;
    let schema_version = read_cache_u32(&mut reader)?;
    if cache_version != SURFACE_INDEX_CACHE_FILE_VERSION || schema_version != INDEX_SCHEMA_VERSION {
        return Err(CacheReadFailure::Stale);
    }
    let manifest_bytes = read_cache_u64(&mut reader)?;
    let document_count = read_cache_u64(&mut reader)?;
    let surface_count = read_cache_u64(&mut reader)?;
    let normalised_count = read_cache_u64(&mut reader)?;
    let surface_map_count = read_cache_u64(&mut reader)?;
    let token_count = read_cache_u64(&mut reader)?;
    let offset_count = read_cache_u64(&mut reader)?;
    let position_count = read_cache_u64(&mut reader)?;
    let document_string_bytes = read_cache_u64(&mut reader)?;
    let surface_utf8_bytes = read_cache_u64(&mut reader)?;
    let normalised_utf8_bytes = read_cache_u64(&mut reader)?;

    if manifest_bytes > SURFACE_INDEX_MAX_MANIFEST_BYTES
        || token_count > u32::MAX as u64
        || surface_count > u32::MAX as u64
        || normalised_count > u32::MAX as u64
        || surface_map_count != surface_count
        || position_count != token_count
        || offset_count != normalised_count.saturating_add(1)
    {
        return Err(CacheReadFailure::Invalid);
    }
    let fixed_bytes = surface_map_count
        .checked_mul(4)
        .and_then(|value| value.checked_add(token_count.checked_mul(4)?))
        .and_then(|value| value.checked_add(offset_count.checked_mul(8)?))
        .and_then(|value| value.checked_add(position_count.checked_mul(4)?))
        .ok_or(CacheReadFailure::Invalid)?;
    let projected_bytes = fixed_bytes
        .checked_add(document_string_bytes)
        .and_then(|value| value.checked_add(surface_utf8_bytes))
        .and_then(|value| value.checked_add(normalised_utf8_bytes))
        .and_then(|value| {
            value.checked_add(
                document_count.checked_mul(std::mem::size_of::<SurfaceIndexDocument>() as u64)?,
            )
        })
        .and_then(|value| {
            value.checked_add(
                surface_count
                    .checked_add(normalised_count)?
                    .checked_mul(80)?,
            )
        })
        .ok_or(CacheReadFailure::Invalid)?;
    if projected_bytes > limits.max_projected_resident_index_bytes {
        return Err(CacheReadFailure::Invalid);
    }

    let mut manifest_buffer = Vec::new();
    reserve_cache_vec(&mut manifest_buffer, manifest_bytes)?;
    manifest_buffer.resize(manifest_bytes as usize, 0);
    reader
        .read_exact(&mut manifest_buffer)
        .map_err(|_| CacheReadFailure::Invalid)?;
    let manifest: SurfaceIndexManifest =
        serde_json::from_slice(&manifest_buffer).map_err(|_| CacheReadFailure::Invalid)?;
    if manifest.schema_version != INDEX_SCHEMA_VERSION || !manifest.is_valid_for(expected_manifest)
    {
        return Err(CacheReadFailure::Stale);
    }

    let mut store = InMemoryIndexStore::default();
    reserve_cache_vec(&mut store.documents, document_count)?;
    let mut remaining_document_bytes = document_string_bytes;
    for _ in 0..document_count {
        let document_id =
            usize::try_from(read_cache_u64(&mut reader)?).map_err(|_| CacheReadFailure::Invalid)?;
        let global_token_start = read_cache_u32(&mut reader)?;
        let token_count = read_cache_u32(&mut reader)?;
        let source_path = read_cache_string(&mut reader, &mut remaining_document_bytes)?;
        let display_name = read_cache_string(&mut reader, &mut remaining_document_bytes)?;
        store.documents.push(SurfaceIndexDocument {
            document_id,
            source_path,
            display_name,
            global_token_start,
            token_count,
        });
    }
    if remaining_document_bytes != 0 {
        return Err(CacheReadFailure::Invalid);
    }
    read_cache_symbol_table(
        &mut reader,
        &mut store.surface_symbols,
        surface_count,
        surface_utf8_bytes,
    )?;
    read_cache_symbol_table(
        &mut reader,
        &mut store.normalised_symbols,
        normalised_count,
        normalised_utf8_bytes,
    )?;
    read_cache_u32_vec(
        &mut reader,
        &mut store.surface_to_normalised,
        surface_map_count,
    )?;
    read_cache_u32_vec(&mut reader, &mut store.token_surface_ids, token_count)?;
    read_cache_u64_vec(&mut reader, &mut store.postings_offsets, offset_count)?;
    read_cache_u32_vec(&mut reader, &mut store.postings_positions, position_count)?;
    reader
        .verify_trailer()
        .map_err(|_| CacheReadFailure::Invalid)?;
    store
        .rebuild_symbol_lookup()
        .map_err(|_| CacheReadFailure::Invalid)?;
    if !store.has_valid_internal_references() {
        return Err(CacheReadFailure::Invalid);
    }
    Ok(store)
}

fn cache_io_error(error: std::io::Error) -> SurfaceIndexCacheError {
    SurfaceIndexCacheError(error.to_string())
}

fn write_cache_u32(writer: &mut impl Write, value: u32) -> Result<(), SurfaceIndexCacheError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(cache_io_error)
}

fn write_cache_u64(writer: &mut impl Write, value: u64) -> Result<(), SurfaceIndexCacheError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(cache_io_error)
}

fn write_cache_string(writer: &mut impl Write, value: &str) -> Result<(), SurfaceIndexCacheError> {
    write_cache_u64(writer, value.len() as u64)?;
    writer.write_all(value.as_bytes()).map_err(cache_io_error)
}

fn read_cache_u32(reader: &mut impl Read) -> Result<u32, CacheReadFailure> {
    let mut bytes = [0u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| CacheReadFailure::Invalid)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_cache_u64(reader: &mut impl Read) -> Result<u64, CacheReadFailure> {
    let mut bytes = [0u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| CacheReadFailure::Invalid)?;
    Ok(u64::from_le_bytes(bytes))
}

fn reserve_cache_vec<T>(values: &mut Vec<T>, count: u64) -> Result<(), CacheReadFailure> {
    let count = usize::try_from(count).map_err(|_| CacheReadFailure::Invalid)?;
    values
        .try_reserve_exact(count)
        .map_err(|_| CacheReadFailure::Invalid)
}

fn read_cache_string(
    reader: &mut impl Read,
    remaining_bytes: &mut u64,
) -> Result<String, CacheReadFailure> {
    let length = read_cache_u64(reader)?;
    if length > *remaining_bytes {
        return Err(CacheReadFailure::Invalid);
    }
    let length = usize::try_from(length).map_err(|_| CacheReadFailure::Invalid)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| CacheReadFailure::Invalid)?;
    bytes.resize(length, 0);
    reader
        .read_exact(&mut bytes)
        .map_err(|_| CacheReadFailure::Invalid)?;
    *remaining_bytes -= length as u64;
    String::from_utf8(bytes).map_err(|_| CacheReadFailure::Invalid)
}

fn read_cache_symbol_table(
    reader: &mut impl Read,
    symbols: &mut Vec<String>,
    count: u64,
    utf8_bytes: u64,
) -> Result<(), CacheReadFailure> {
    reserve_cache_vec(symbols, count)?;
    let mut remaining = utf8_bytes;
    for _ in 0..count {
        symbols.push(read_cache_string(reader, &mut remaining)?);
    }
    (remaining == 0)
        .then_some(())
        .ok_or(CacheReadFailure::Invalid)
}

fn read_cache_u32_vec(
    reader: &mut impl Read,
    values: &mut Vec<u32>,
    count: u64,
) -> Result<(), CacheReadFailure> {
    reserve_cache_vec(values, count)?;
    for _ in 0..count {
        values.push(read_cache_u32(reader)?);
    }
    Ok(())
}

fn read_cache_u64_vec(
    reader: &mut impl Read,
    values: &mut Vec<u64>,
    count: u64,
) -> Result<(), CacheReadFailure> {
    reserve_cache_vec(values, count)?;
    for _ in 0..count {
        values.push(read_cache_u64(reader)?);
    }
    Ok(())
}

fn prune_surface_index_cache(cache_root: &Path, protected_path: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(cache_root) else {
        return;
    };
    let mut current_files = Vec::<(PathBuf, std::time::SystemTime, u64, bool)>::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("surface-index.v") {
            continue;
        }
        if name.ends_with(".tmp") || name.ends_with(".json") {
            let _ = std::fs::remove_file(path);
            continue;
        }
        if !name.ends_with(".cwi") {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            let protected = protected_path.is_some_and(|protected| protected == path);
            current_files.push((
                path,
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                metadata.len(),
                protected,
            ));
        }
    }
    current_files.sort_by(|left, right| {
        right
            .3
            .cmp(&left.3)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.0.cmp(&left.0))
    });
    let mut retained = 0usize;
    let mut retained_bytes = 0u64;
    for (path, _, bytes, _) in current_files {
        let keep = retained == 0
            || (retained < SURFACE_INDEX_MAX_RETAINED_FILES
                && retained_bytes.saturating_add(bytes)
                    <= SURFACE_INDEX_MAX_RETAINED_AGGREGATE_BYTES);
        if keep {
            retained += 1;
            retained_bytes = retained_bytes.saturating_add(bytes);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl SurfaceIndexManifest {
    pub fn new(
        processed_texts_hash_sha256: String,
        tokenization: TokenizationManifest,
    ) -> SurfaceIndexManifest {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            processed_texts_hash_sha256,
            decode_policy_version: DECODE_POLICY_VERSION,
            tokenization,
        }
    }

    pub fn is_valid_for(&self, expected: &SurfaceIndexManifest) -> bool {
        self == expected
    }
}

fn symbol_hash(symbol: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    symbol.hash(&mut hasher);
    hasher.finish()
}

fn try_reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    purpose: &str,
) -> Result<(), SurfaceIndexError> {
    values
        .try_reserve(additional)
        .map_err(|error| allocation_error(purpose, error))
}

fn allocation_error(purpose: &str, error: impl std::fmt::Display) -> SurfaceIndexError {
    SurfaceIndexError::Allocation(format!("{purpose}: {error}"))
}

fn fallible_string_clone(value: &str, purpose: &str) -> Result<String, SurfaceIndexError> {
    let mut cloned = String::new();
    cloned
        .try_reserve(value.len())
        .map_err(|error| allocation_error(purpose, error))?;
    cloned.push_str(value);
    Ok(cloned)
}

fn query_terms(
    query: &str,
    tokenization_config: &TokenizationConfig,
) -> Result<Vec<SurfaceIndexTerm>, SurfaceIndexError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }
    let terms = tokenize(trimmed, 0, tokenization_config)
        .into_iter()
        .filter(|token| token.counts_as_word())
        .map(|token| SurfaceIndexTerm {
            surface: token.surface.to_string(),
            normalised_surface: concordance_key(token.surface, false),
            exact_surface: concordance_key(token.surface, true),
        })
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }
    Ok(terms)
}

fn term_hits(
    store: &impl IndexStore,
    term: &SurfaceIndexTerm,
    case_sensitive: bool,
) -> Vec<SurfaceIndexHit> {
    store
        .postings(&term.normalised_surface)
        .unwrap_or_default()
        .iter()
        .filter_map(|posting| {
            let (document_id, token_ordinal_in_document) =
                store.document_and_ordinal_for_position(*posting)?;
            let record = store
                .records_for_document(document_id)?
                .get(token_ordinal_in_document)?;
            if case_sensitive
                && store.concordance_key_for_record(record, true)
                    != Some(term.exact_surface.as_str())
            {
                return None;
            }
            Some(SurfaceIndexHit {
                document_id,
                token_ordinal_in_document,
                node_token_length: 1,
            })
        })
        .collect()
}

fn phrase_hits(
    store: &impl IndexStore,
    terms: &[SurfaceIndexTerm],
    case_sensitive: bool,
) -> Vec<SurfaceIndexHit> {
    let Some(first_postings) = store.postings(&terms[0].normalised_surface) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for posting in first_postings {
        let Some((document_id, token_ordinal_in_document)) =
            store.document_and_ordinal_for_position(*posting)
        else {
            continue;
        };
        let Some(records) = store.records_for_document(document_id) else {
            continue;
        };
        let Some(last_index) = token_ordinal_in_document.checked_add(terms.len().saturating_sub(1))
        else {
            continue;
        };
        if records.get(last_index).is_none() {
            continue;
        }
        let mut matches = true;
        for (offset, term) in terms.iter().enumerate() {
            let Some(token_ordinal) = token_ordinal_in_document.checked_add(offset) else {
                matches = false;
                break;
            };
            let Some(record) = records.get(token_ordinal) else {
                matches = false;
                break;
            };
            if store.concordance_key_for_record(record, case_sensitive)
                != Some(term.concordance_key(case_sensitive))
            {
                matches = false;
                break;
            }
        }
        if matches {
            hits.push(SurfaceIndexHit {
                document_id,
                token_ordinal_in_document,
                node_token_length: terms.len(),
            });
        }
    }
    hits
}

pub fn materialize_surface_index_kwic_row(
    store: &impl IndexStore,
    hit: &SurfaceIndexHit,
    window_tokens: usize,
) -> Result<SurfaceIndexKwicRow, SurfaceIndexError> {
    let records = store.records_for_document(hit.document_id).ok_or(
        SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        },
    )?;
    let document = store
        .document(hit.document_id)
        .ok_or(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        })?;
    let node_end = hit
        .token_ordinal_in_document
        .checked_add(hit.node_token_length)
        .ok_or(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        })?;
    if hit.node_token_length == 0 || node_end > records.len() {
        return Err(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        });
    }

    let left_start = hit.token_ordinal_in_document.saturating_sub(window_tokens);
    let right_end = (node_end + window_tokens).min(records.len());
    let left_context = surfaces(store, &records[left_start..hit.token_ordinal_in_document])?;
    let node_tokens = surfaces(store, &records[hit.token_ordinal_in_document..node_end])?;
    let right_context = surfaces(store, &records[node_end..right_end])?;

    Ok(SurfaceIndexKwicRow {
        document_id: hit.document_id,
        source_path: document.source_path.clone(),
        display_name: document.display_name.clone(),
        token_ordinal_in_document: hit.token_ordinal_in_document,
        node_token_length: hit.node_token_length,
        left_context,
        node: node_tokens.join(" "),
        right_context,
    })
}

fn surfaces(
    store: &impl IndexStore,
    records: &[SurfaceIndexTokenRecord],
) -> Result<Vec<String>, SurfaceIndexError> {
    let mut surfaces = Vec::new();
    try_reserve(&mut surfaces, records.len(), "materialized KWIC surfaces")?;
    for record in records {
        let surface =
            store
                .surface_for_record(record)
                .ok_or(SurfaceIndexError::InconsistentStore {
                    document_id: 0,
                    token_ordinal_in_document: 0,
                    node_token_length: 1,
                })?;
        surfaces.push(surface.to_string());
    }
    Ok(surfaces)
}

fn surface_index_window_tokens(window_tokens: usize) -> usize {
    if window_tokens == 0 {
        DEFAULT_WINDOW_TOKENS
    } else {
        window_tokens.clamp(1, MAX_WINDOW_TOKENS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        AnalysisDocumentSelection, ConcordanceBackend, ConcordanceContextFilterDirection,
        ConcordanceDocument, ConcordanceQuery, ConcordanceSearchContext, ConcordanceSortKey,
        search_concordance_documents, search_concordance_documents_routed,
    };
    use crate::decode::{DecodeStrategy, SourceEncoding, decode_source_bytes};

    fn document_input<'text>(
        document_id: usize,
        name: &str,
        prepared_text: &'text str,
    ) -> SurfaceIndexDocumentInput<'text> {
        SurfaceIndexDocumentInput {
            document_id,
            source_path: name.to_string(),
            display_name: name.to_string(),
            prepared_text,
        }
    }

    fn concordance_document(index: usize, name: &str, text: &str) -> ConcordanceDocument {
        ConcordanceDocument {
            document_index: index,
            source_path: name.to_string(),
            display_name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn build_index<'text>(documents: &[SurfaceIndexDocumentInput<'text>]) -> InMemoryIndexStore {
        InMemoryIndexStore::build(documents, &TokenizationConfig::default()).unwrap()
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 0.000_000_1, "{left} != {right}");
    }

    fn scan_query(query: &str, case_sensitive: bool) -> ConcordanceQuery {
        ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            context_filter_direction: ConcordanceContextFilterDirection::Both,
            case_sensitive,
            sort_key: ConcordanceSortKey::FilePosition,
            window_tokens: 5,
            offset: 0,
            limit: 500,
            distribution_offset: 0,
            distribution_limit: 500,
        }
    }

    fn assert_scan_index_parity(
        documents: &[ConcordanceDocument],
        query_text: &str,
        case_sensitive: bool,
    ) {
        let tokenization_config = TokenizationConfig::default();
        let scan = search_concordance_documents(
            documents,
            &scan_query(query_text, case_sensitive),
            &tokenization_config,
        )
        .unwrap();
        let inputs = documents
            .iter()
            .map(|document| SurfaceIndexDocumentInput {
                document_id: document.document_index,
                source_path: document.source_path.clone(),
                display_name: document.display_name.clone(),
                prepared_text: document.text.as_str(),
            })
            .collect::<Vec<_>>();
        let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
        let query = SurfaceIndexQuery::from_surface_text_with_case(
            query_text,
            case_sensitive,
            &tokenization_config,
        )
        .unwrap();
        let index = search_surface_index_kwic(&store, &query, scan.window_tokens).unwrap();
        let message = format!("{query_text} case_sensitive={case_sensitive}");

        assert_eq!(index.total_hits, scan.total_hits, "{message}");
        assert_eq!(
            index.documents_with_hits, scan.documents_with_hits,
            "{message}"
        );
        assert_eq!(
            index.selected_documents_total, scan.selected_documents_total,
            "{message}"
        );
        assert_eq!(
            index.selected_tokens_total as u64, scan.selected_tokens_total,
            "{message}"
        );
        assert_close(index.top_document_share, scan.top_document_share);
        let scan_hit_counts = scan
            .distribution
            .items
            .iter()
            .filter(|item| item.count > 0)
            .map(|item| (item.document_id, item.count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(index.document_hit_counts, scan_hit_counts, "{message}");
        assert_eq!(index.rows.len(), scan.rows.len(), "{message}");
        for (index_row, scan_row) in index.rows.iter().zip(&scan.rows) {
            assert_eq!(index_row.document_id, scan_row.document_index, "{message}");
            assert_eq!(
                index_row.token_ordinal_in_document, scan_row.token_index,
                "{message}"
            );
            assert_eq!(
                index_row.node_token_length, scan_row.node_token_length,
                "{message}"
            );
            assert_eq!(index_row.left_context, scan_row.left_context, "{message}");
            assert_eq!(index_row.node, scan_row.node, "{message}");
            assert_eq!(index_row.right_context, scan_row.right_context, "{message}");
        }
    }

    fn assert_scan_index_parity_both_modes(documents: &[ConcordanceDocument], query_text: &str) {
        assert_scan_index_parity(documents, query_text, false);
        assert_scan_index_parity(documents, query_text, true);
    }

    fn hit_positions(result: &SurfaceIndexSearchResult) -> Vec<(usize, usize)> {
        result
            .hits
            .iter()
            .map(|hit| (hit.document_id, hit.token_ordinal_in_document))
            .collect()
    }

    fn search_positions(
        store: &InMemoryIndexStore,
        query_text: &str,
        case_sensitive: bool,
    ) -> Vec<(usize, usize)> {
        let config = TokenizationConfig::default();
        let query =
            SurfaceIndexQuery::from_surface_text_with_case(query_text, case_sensitive, &config)
                .unwrap();
        hit_positions(&search_surface_index(store, &query).unwrap())
    }

    fn recovered_posting_positions(
        store: &InMemoryIndexStore,
        postings: &[SurfacePosting],
    ) -> Vec<(usize, usize)> {
        postings
            .iter()
            .map(|position| store.document_and_ordinal_for_position(*position).unwrap())
            .collect()
    }

    #[derive(Debug)]
    struct CacheArrayOffsets {
        first_surface_utf8: usize,
        token_surface_ids: usize,
        postings_offsets: usize,
        postings_positions: usize,
    }

    fn test_cache_u64(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    fn test_cache_array_offsets(bytes: &[u8]) -> CacheArrayOffsets {
        let manifest_bytes = test_cache_u64(bytes, 16) as usize;
        let document_count = test_cache_u64(bytes, 24) as usize;
        let surface_count = test_cache_u64(bytes, 32) as usize;
        let normalised_count = test_cache_u64(bytes, 40) as usize;
        let surface_map_count = test_cache_u64(bytes, 48) as usize;
        let token_count = test_cache_u64(bytes, 56) as usize;
        let offset_count = test_cache_u64(bytes, 64) as usize;
        let mut cursor = 104 + manifest_bytes;
        for _ in 0..document_count {
            cursor += 16;
            let source_length = test_cache_u64(bytes, cursor) as usize;
            cursor += 8 + source_length;
            let display_length = test_cache_u64(bytes, cursor) as usize;
            cursor += 8 + display_length;
        }
        let first_surface_utf8 = cursor + 8;
        for _ in 0..surface_count {
            let length = test_cache_u64(bytes, cursor) as usize;
            cursor += 8 + length;
        }
        for _ in 0..normalised_count {
            let length = test_cache_u64(bytes, cursor) as usize;
            cursor += 8 + length;
        }
        let token_surface_ids = cursor + surface_map_count * 4;
        let postings_offsets = token_surface_ids + token_count * 4;
        let postings_positions = postings_offsets + offset_count * 8;
        CacheArrayOffsets {
            first_surface_utf8,
            token_surface_ids,
            postings_offsets,
            postings_positions,
        }
    }

    fn rewrite_test_cache_checksum(bytes: &mut [u8]) {
        let payload_end = bytes.len() - SURFACE_INDEX_CHECKSUM_BYTES;
        let checksum = Sha256::digest(&bytes[..payload_end]);
        bytes[payload_end..].copy_from_slice(&checksum);
    }

    #[test]
    fn compact_token_stream_preserves_canonical_surfaces_and_case_keys() {
        let text = "ação coração não João maçã 42";
        let documents = [document_input(7, "pt.txt", text)];

        let store = build_index(&documents);
        let records = store.records_for_document(7).unwrap();

        assert_eq!(
            records
                .iter()
                .map(|record| store.surface_for_record(record).unwrap())
                .collect::<Vec<_>>(),
            ["ação", "coração", "não", "João", "maçã", "42"]
        );
        assert_eq!(
            store.concordance_key_for_record(&records[0], false),
            Some("ação")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[3], false),
            Some("joão")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[3], true),
            Some("João")
        );
        assert_eq!(store.document_and_ordinal_for_position(3), Some((7, 3)));
        assert_eq!(store.global_position_for_document_token(7, 3), Some(3));
    }

    #[test]
    fn default_aggregation_key_matches_concordance_case_insensitive_key() {
        let config = TokenizationConfig::default();
        let tokens = tokenize("Movie ação João", 0, &config)
            .into_iter()
            .filter(|token| token.counts_as_word())
            .collect::<Vec<_>>();

        for token in tokens {
            assert_eq!(
                token.aggregation_key(&config),
                concordance_key(token.surface, false)
            );
        }
    }

    #[test]
    fn exact_search_verifies_candidates_from_one_normalised_postings_population() {
        let documents = [
            document_input(2, "b.txt", "MOVIE movie"),
            document_input(0, "a.txt", "Movie movie MOVIE"),
        ];

        let store = build_index(&documents);
        let normalised_postings = store.postings("movie").unwrap();

        assert_eq!(
            recovered_posting_positions(&store, normalised_postings),
            [(0, 0), (0, 1), (0, 2), (2, 0), (2, 1)]
        );
        assert_eq!(search_positions(&store, "movie", true), [(0, 1), (2, 1)]);
        assert_eq!(search_positions(&store, "Movie", true), [(0, 0)]);
        assert_eq!(search_positions(&store, "MOVIE", true), [(0, 2), (2, 0)]);
        assert_eq!(store.postings_positions.len(), store.total_indexed_tokens());
    }

    #[test]
    fn compact_arrays_meet_structural_token_budget_and_validate_references() {
        let text = "Alpha alpha ".repeat(100_000);
        let store = build_index(&[document_input(0, "repetitive.txt", &text)]);
        let token_count = store.total_indexed_tokens() as u64;
        let corpus_proportional_bytes = (store.token_surface_ids.len() * std::mem::size_of::<u32>()
            + store.postings_positions.len() * std::mem::size_of::<u32>())
            as u64;

        assert_eq!(
            store.token_surface_ids.len(),
            store.postings_positions.len()
        );
        assert_eq!(
            store.surface_to_normalised.len(),
            store.surface_symbols.len()
        );
        assert_eq!(
            store.postings_offsets.len(),
            store.normalised_symbols.len() + 1
        );
        assert_eq!(store.postings_offsets.last(), Some(&(token_count)));
        assert!(store.has_valid_internal_references());
        assert!(corpus_proportional_bytes / token_count <= 12);
        assert!(store.fixed_array_bytes() / token_count <= 12);
    }

    #[test]
    fn incremental_builder_handles_fifty_thousand_tiny_documents_without_token_staging() {
        let texts = (0..50_000)
            .map(|index| format!("token{}", index % 17))
            .collect::<Vec<_>>();
        let inputs = texts
            .iter()
            .enumerate()
            .map(|(document_id, text)| SurfaceIndexDocumentInput {
                document_id,
                source_path: format!("{document_id}.txt"),
                display_name: format!("{document_id}.txt"),
                prepared_text: text,
            })
            .collect::<Vec<_>>();
        let (store, stats) = InMemoryIndexStore::build_with_limits_and_stats(
            &inputs,
            &TokenizationConfig::default(),
            SurfaceIndexResourceLimits::default(),
        )
        .unwrap();

        assert_eq!(store.total_indexed_documents(), 50_000);
        assert_eq!(store.total_indexed_tokens(), 50_000);
        assert_eq!(stats.maximum_staged_token_records, 0);
        assert!(stats.maximum_staged_prepared_text_bytes <= 7);
    }

    #[test]
    fn mixed_tiny_and_whale_build_is_streamed_and_bounded() {
        let whale = "whale token ".repeat(50_000);
        let inputs = [
            document_input(0, "tiny.txt", "tiny token"),
            document_input(1, "whale.txt", &whale),
            document_input(2, "tiny-2.txt", "another tiny token"),
        ];
        let limits = SurfaceIndexResourceLimits {
            max_staged_prepared_text_bytes: 1_048_576,
            ..SurfaceIndexResourceLimits::default()
        };
        let (store, stats) = InMemoryIndexStore::build_with_limits_and_stats(
            &inputs,
            &TokenizationConfig::default(),
            limits,
        )
        .unwrap();

        assert_eq!(store.total_indexed_documents(), 3);
        assert_eq!(stats.maximum_staged_token_records, 0);
        assert!(stats.maximum_staged_prepared_text_bytes <= limits.max_staged_prepared_text_bytes);
        assert_eq!(stats.large_documents_streamed, 1);
    }

    #[test]
    fn large_document_visitor_observes_cancellation() {
        let cancellation = SurfaceIndexCancellationToken::default();
        let mut builder = SurfaceIndexBuilder::with_cancellation(
            TokenizationConfig::default(),
            SurfaceIndexResourceLimits::default(),
            cancellation.clone(),
        );
        cancellation.cancel();
        let whale = "token ".repeat(100_000);

        assert_eq!(
            builder.add_document(&document_input(0, "whale.txt", &whale)),
            Err(SurfaceIndexError::Cancelled)
        );
    }

    #[test]
    fn resident_limit_fails_before_postings_overallocation() {
        let text = "alpha beta gamma delta ".repeat(10_000);
        let limits = SurfaceIndexResourceLimits {
            max_projected_resident_index_bytes: 32_768,
            ..SurfaceIndexResourceLimits::default()
        };

        assert!(matches!(
            InMemoryIndexStore::build_with_limits(
                &[document_input(0, "bounded.txt", &text)],
                &TokenizationConfig::default(),
                limits,
            ),
            Err(SurfaceIndexError::ProjectedResidentIndexExceedsLimit { .. })
        ));
    }

    #[test]
    fn postings_are_sorted_by_document_and_token_ordinal() {
        let documents = [
            document_input(2, "b.txt", "alpha beta alpha"),
            document_input(0, "a.txt", "beta alpha"),
        ];

        let store = build_index(&documents);
        let postings = store.postings("alpha").unwrap();

        assert_eq!(
            recovered_posting_positions(&store, postings),
            [(0, 1), (2, 0), (2, 2)]
        );
        assert_eq!(store.document_frequency("alpha"), 2);
        assert_eq!(
            search_positions(&store, "alpha", true),
            [(0, 1), (2, 0), (2, 2)]
        );
    }

    #[test]
    fn query_ast_resolves_case_modes_without_crossing_documents() {
        let documents = [
            document_input(
                0,
                "a.txt",
                "Movie, movie. MOVIE. good movie. Good movie! I love it. i love it.",
            ),
            document_input(1, "b.txt", "good"),
            document_input(2, "c.txt", "movie"),
        ];
        let store = build_index(&documents);

        assert_eq!(
            search_positions(&store, "movie", false),
            [(0, 0), (0, 1), (0, 2), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(
            search_positions(&store, "Movie", false),
            [(0, 0), (0, 1), (0, 2), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(
            search_positions(&store, "movie", true),
            [(0, 1), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(search_positions(&store, "Movie", true), [(0, 0)]);
        assert_eq!(search_positions(&store, "MOVIE", true), [(0, 2)]);

        assert_eq!(
            search_positions(&store, "good movie", false),
            [(0, 3), (0, 5)]
        );
        assert_eq!(
            search_positions(&store, "Good movie", false),
            [(0, 3), (0, 5)]
        );
        assert_eq!(search_positions(&store, "good movie", true), [(0, 3)]);
        assert_eq!(search_positions(&store, "Good movie", true), [(0, 5)]);

        let boundary_documents = [
            document_input(1, "boundary-a.txt", "good"),
            document_input(2, "boundary-b.txt", "movie"),
        ];
        let boundary_store = build_index(&boundary_documents);
        assert!(search_positions(&boundary_store, "good movie", false).is_empty());
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_english_queries() {
        let documents = [
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
            assert_scan_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_portuguese_queries() {
        let documents = [
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
            assert_scan_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn windows_1252_recovered_portuguese_offsets_and_parity_hold() {
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

        let documents = [document_input(0, "pt-win1252.txt", &decoded.text)];
        let store = build_index(&documents);
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
            assert!(
                records
                    .iter()
                    .any(|record| store.surface_for_record(record) == Some(expected_surface))
            );
        }

        assert!(
            records
                .iter()
                .any(|record| store.surface_for_record(record) == Some("João"))
        );
        assert!(
            records
                .iter()
                .any(|record| store.surface_for_record(record) == Some("joão"))
        );

        let concordance_documents = [concordance_document(
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
            assert_scan_index_parity_both_modes(&concordance_documents, query);
        }
    }

    #[test]
    fn compact_surface_stream_matches_canonical_word_tokens() {
        let config = TokenizationConfig::default();
        let text = "ação Coração não João maçã ".repeat(40);
        let documents = [document_input(3, "long-pt.txt", &text)];
        let store = build_index(&documents);
        let records = store.records_for_document(3).unwrap();
        let word_tokens = tokenize(&text, 3, &config)
            .into_iter()
            .filter(|token| token.counts_as_word())
            .collect::<Vec<_>>();

        assert_eq!(records.len(), word_tokens.len());
        assert_eq!(
            records
                .iter()
                .map(|record| store.surface_for_record(record).unwrap())
                .collect::<Vec<_>>(),
            word_tokens
                .iter()
                .map(|token| token.surface)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn manifest_invalidates_when_identity_inputs_change() {
        let expected = SurfaceIndexManifest::new(
            "processed-hash".to_string(),
            TokenizationManifest::default(),
        );
        assert_eq!(expected.schema_version, INDEX_SCHEMA_VERSION);

        let mut changed_schema = expected.clone();
        changed_schema.schema_version = 2;
        assert!(!changed_schema.is_valid_for(&expected));

        let mut changed_decode = expected.clone();
        changed_decode.decode_policy_version += 1;
        assert!(!changed_decode.is_valid_for(&expected));

        let mut changed_tokenization = expected.clone();
        changed_tokenization.tokenization.version = "2".to_string();
        assert!(!changed_tokenization.is_valid_for(&expected));

        let mut changed_processed_text = expected.clone();
        changed_processed_text.processed_texts_hash_sha256 = "other-hash".to_string();
        assert!(!changed_processed_text.is_valid_for(&expected));
    }

    #[test]
    fn index_types_remain_rust_only_without_ts_export() {
        let source = include_str!("index.rs");
        let ts_export_attr = concat!("#[", "ts(export)]");
        let ts_derive = concat!("derive(", "TS");

        assert!(!source.contains(ts_export_attr));
        assert!(!source.contains(ts_derive));
    }

    #[test]
    fn parallel_index_build_is_deterministic_across_thread_counts_and_input_order() {
        let texts = (0..256)
            .map(|index| {
                format!(
                    "shared Movie movie MOVIE alpha{} beta{} shared ação João ",
                    index % 17,
                    index % 11
                )
                .repeat(12)
            })
            .collect::<Vec<_>>();
        let one_thread_inputs = (0..texts.len())
            .map(|document_id| SurfaceIndexDocumentInput {
                document_id,
                source_path: format!("{document_id}.txt"),
                display_name: format!("{document_id}.txt"),
                prepared_text: texts[document_id].as_str(),
            })
            .collect::<Vec<_>>();
        let four_thread_inputs = (0..texts.len())
            .rev()
            .map(|document_id| SurfaceIndexDocumentInput {
                document_id,
                source_path: format!("{document_id}.txt"),
                display_name: format!("{document_id}.txt"),
                prepared_text: texts[document_id].as_str(),
            })
            .collect::<Vec<_>>();
        let one_thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let four_thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        assert_eq!(one_thread_pool.current_num_threads(), 1);
        assert_eq!(four_thread_pool.current_num_threads(), 4);

        let one_thread = one_thread_pool.install(|| build_index(&one_thread_inputs));
        let four_thread = four_thread_pool.install(|| build_index(&four_thread_inputs));
        assert_eq!(one_thread, four_thread);
        assert_eq!(
            search_positions(&one_thread, "movie", false),
            search_positions(&four_thread, "movie", false)
        );
        assert_eq!(
            search_positions(&one_thread, "Movie", true),
            search_positions(&four_thread, "Movie", true)
        );
    }

    #[test]
    fn interned_surfaces_resolve_across_case_handling_paths() {
        let documents = [document_input(
            0,
            "case.txt",
            "Movie movie MOVIE Movie movie MOVIE",
        )];
        let store = build_index(&documents);
        let records = store.records_for_document(0).unwrap();

        assert_eq!(store.surface_symbols.len(), 3);
        assert_eq!(store.normalised_symbols.len(), 1);
        assert_eq!(store.surface_for_record(&records[0]), Some("Movie"));
        assert_eq!(store.surface_for_record(&records[1]), Some("movie"));
        assert_eq!(store.surface_for_record(&records[2]), Some("MOVIE"));
        assert_eq!(
            store.concordance_key_for_record(&records[0], false),
            Some("movie")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[0], true),
            Some("Movie")
        );
        assert_eq!(search_positions(&store, "movie", false).len(), 6);
        assert_eq!(search_positions(&store, "Movie", true).len(), 2);
    }

    #[test]
    fn surface_index_binary_cache_round_trip_and_corruption_misses() {
        let temp_dir = tempfile::tempdir().unwrap();
        let documents = [
            document_input(1, "b.txt", "Movie alpha movie"),
            document_input(0, "a.txt", "ação Movie João"),
        ];
        let store = build_index(&documents);
        let manifest =
            SurfaceIndexManifest::new("content-hash".to_string(), TokenizationManifest::default());
        write_surface_index_cache(temp_dir.path(), &manifest, &store).unwrap();
        let cache_path = surface_index_cache_path(temp_dir.path(), &manifest);
        assert_eq!(
            cache_path.extension().and_then(|value| value.to_str()),
            Some("cwi")
        );
        let loaded = match load_surface_index_cache(temp_dir.path(), &manifest) {
            SurfaceIndexCacheLoad::Ready(store) => store,
            state => panic!("expected ready cache, got {state:?}"),
        };
        assert_eq!(*loaded, store);

        let mut changed_config = manifest.clone();
        changed_config.tokenization.config.include_punctuation = false;
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &changed_config),
            SurfaceIndexCacheLoad::Missing
        ));

        let pristine = std::fs::read(&cache_path).unwrap();
        let assert_invalid = |bytes: &[u8]| {
            std::fs::write(&cache_path, bytes).unwrap();
            assert!(matches!(
                load_surface_index_cache(temp_dir.path(), &manifest),
                SurfaceIndexCacheLoad::Invalid
            ));
        };

        let mut bad_magic = pristine.clone();
        bad_magic[0] ^= 0xff;
        assert_invalid(&bad_magic);

        let mut stale_version = pristine.clone();
        stale_version[8..12].copy_from_slice(&(SURFACE_INDEX_CACHE_FILE_VERSION + 1).to_le_bytes());
        std::fs::write(&cache_path, stale_version).unwrap();
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &manifest),
            SurfaceIndexCacheLoad::Stale
        ));

        assert_invalid(&pristine[..12]);
        assert_invalid(&pristine[..pristine.len() - 40]);

        let offsets = test_cache_array_offsets(&pristine);
        let mut invalid_utf8 = pristine.clone();
        invalid_utf8[offsets.first_surface_utf8] = 0xff;
        rewrite_test_cache_checksum(&mut invalid_utf8);
        assert_invalid(&invalid_utf8);

        let mut invalid_checksum = pristine.clone();
        let checksum_byte = invalid_checksum.len() - 1;
        invalid_checksum[checksum_byte] ^= 1;
        assert_invalid(&invalid_checksum);

        let mut non_monotonic_offsets = pristine.clone();
        non_monotonic_offsets[offsets.postings_offsets + 8..offsets.postings_offsets + 16]
            .copy_from_slice(&u64::MAX.to_le_bytes());
        rewrite_test_cache_checksum(&mut non_monotonic_offsets);
        assert_invalid(&non_monotonic_offsets);

        let mut out_of_range_symbol = pristine.clone();
        out_of_range_symbol[offsets.token_surface_ids..offsets.token_surface_ids + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        rewrite_test_cache_checksum(&mut out_of_range_symbol);
        assert_invalid(&out_of_range_symbol);

        let mut out_of_range_position = pristine.clone();
        out_of_range_position[offsets.postings_positions..offsets.postings_positions + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        rewrite_test_cache_checksum(&mut out_of_range_position);
        assert_invalid(&out_of_range_position);

        let mut huge_declared_token_count = pristine.clone();
        huge_declared_token_count[56..64].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_invalid(&huge_declared_token_count);

        let mut trailing_data = pristine.clone();
        trailing_data.push(0);
        assert_invalid(&trailing_data);
    }

    #[test]
    fn surface_index_cache_prunes_only_dedicated_cache_artifacts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let legacy = temp_dir.path().join("surface-index.v1.legacy.json");
        let abandoned = temp_dir.path().join("surface-index.v2.abandoned.tmp");
        let unrelated = temp_dir.path().join("keep-me.json");
        std::fs::write(&legacy, b"legacy").unwrap();
        std::fs::write(&abandoned, b"temporary").unwrap();
        std::fs::write(&unrelated, b"unrelated").unwrap();
        for index in 0..4 {
            std::fs::write(
                temp_dir
                    .path()
                    .join(format!("surface-index.v2.old-{index}.cwi")),
                b"obsolete",
            )
            .unwrap();
        }
        let store = build_index(&[document_input(0, "a.txt", "alpha beta")]);
        let manifest =
            SurfaceIndexManifest::new("pruning".to_string(), TokenizationManifest::default());
        let current = surface_index_cache_path(temp_dir.path(), &manifest);
        write_surface_index_cache(temp_dir.path(), &manifest, &store).unwrap();

        assert!(!legacy.exists());
        assert!(!abandoned.exists());
        assert!(unrelated.exists());
        assert!(current.exists());
        let retained = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry.file_name().to_str().is_some_and(|name| {
                    name.starts_with("surface-index.v2.") && name.ends_with(".cwi")
                })
            })
            .count();
        assert!(retained <= SURFACE_INDEX_MAX_RETAINED_FILES);
    }

    #[test]
    fn legacy_representation_lower_bound_exposes_large_corpus_risk_without_allocation() {
        let bytes_per_token = std::mem::size_of::<LegacySurfaceIndexTokenRecordLayout>()
            + 2 * std::mem::size_of::<LegacySurfacePostingLayout>();
        let forty_million =
            SurfaceIndexResourceEstimate::legacy_structural_lower_bound(40_000_000).unwrap();
        let hundred_million =
            SurfaceIndexResourceEstimate::legacy_structural_lower_bound(100_000_000).unwrap();

        assert_eq!(forty_million, 40_000_000 * bytes_per_token as u64);
        assert_eq!(hundred_million, 100_000_000 * bytes_per_token as u64);
        assert!(forty_million >= 3_000_000_000);
        assert!(hundred_million >= 8_000_000_000);
    }

    #[test]
    fn legacy_build_fails_before_allocating_above_resident_budget() {
        let text = "a ".repeat(1_000);
        let inputs = [SurfaceIndexDocumentInput {
            document_id: 0,
            source_path: "large.txt".to_string(),
            display_name: "large.txt".to_string(),
            prepared_text: &text,
        }];
        let limits = SurfaceIndexResourceLimits {
            max_projected_resident_index_bytes: 1,
            ..SurfaceIndexResourceLimits::default()
        };

        assert!(matches!(
            InMemoryIndexStore::build_with_limits(&inputs, &TokenizationConfig::default(), limits,),
            Err(SurfaceIndexError::ProjectedResidentIndexExceedsLimit { .. })
        ));
    }

    #[test]
    fn concordance_index_performance_instrumentation() {
        let texts = (0..2_000)
            .map(|index| {
                format!(
                    "alpha beta target blue gamma Movie ação document{} ",
                    index % 97
                )
                .repeat(5)
            })
            .collect::<Vec<_>>();
        let documents = texts
            .iter()
            .enumerate()
            .map(|(document_index, text)| ConcordanceDocument {
                document_index,
                source_path: format!("{document_index}.txt"),
                display_name: format!("{document_index}.txt"),
                text: text.clone(),
            })
            .collect::<Vec<_>>();
        let inputs = documents
            .iter()
            .map(|document| SurfaceIndexDocumentInput {
                document_id: document.document_index,
                source_path: document.source_path.clone(),
                display_name: document.display_name.clone(),
                prepared_text: document.text.as_str(),
            })
            .collect::<Vec<_>>();
        let config = TokenizationConfig::default();
        let build_started = std::time::Instant::now();
        let store = InMemoryIndexStore::build(&inputs, &config).unwrap();
        let build_ms = build_started.elapsed().as_secs_f64() * 1_000.0;
        let postings_count = store.total_postings();

        let mut query = scan_query("target", false);
        query.context_filter = Some("blue".to_string());
        query.context_filter_direction = ConcordanceContextFilterDirection::Right;
        let scan_started = std::time::Instant::now();
        let scan = search_concordance_documents(&documents, &query, &config).unwrap();
        let scan_ms = scan_started.elapsed().as_secs_f64() * 1_000.0;

        let cache_dir = tempfile::tempdir().unwrap();
        let mut context = ConcordanceSearchContext::with_cache_root(cache_dir.path().to_path_buf());
        let first_started = std::time::Instant::now();
        let first = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let first_ms = first_started.elapsed().as_secs_f64() * 1_000.0;
        let warm_started = std::time::Instant::now();
        let warm = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let warm_ms = warm_started.elapsed().as_secs_f64() * 1_000.0;
        let mut restarted_context =
            ConcordanceSearchContext::with_cache_root(cache_dir.path().to_path_buf());
        let disk_started = std::time::Instant::now();
        let disk = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut restarted_context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let disk_ms = disk_started.elapsed().as_secs_f64() * 1_000.0;

        assert_eq!(first.result, scan);
        assert_eq!(warm.result, scan);
        assert_eq!(disk.result, scan);
        assert!(first.index_rebuilt);
        assert!(!warm.index_rebuilt);
        assert!(!disk.index_rebuilt);
        let evidence = serde_json::json!({
            "documents": documents.len(),
            "tokens": store.total_indexed_tokens(),
            "postings": postings_count,
            "build_ms": build_ms,
            "scan_first_query_ms": scan_ms,
            "first_query_ms": first_ms,
            "warm_query_ms": warm_ms,
            "cold_start_with_disk_cache_ms": disk_ms,
        });
        std::fs::write(
            "/tmp/corpuswright-concordance-index-timing.json",
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        eprintln!("concordance index timing: {evidence}");
    }
}
