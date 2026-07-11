use super::build::AnalysisDocumentSelection;
use super::concordance::{
    ConcordanceBackend, ConcordanceBackendUsed, ConcordanceDocument, ConcordanceError,
    ConcordanceFallbackReason, ConcordanceSearchContext, concordance_key,
    resolve_concordance_query_tokens, selected_concordance_document_ids, visit_index_query_hits,
};
use super::frequency::SortDirection;
use super::shared::{StopwordLanguage, stopword_matches};
use crate::index::{
    IndexStore, SurfaceIndexCancellationToken, SurfaceIndexError, SurfaceIndexQuery,
    SurfaceIndexResourceLimits, SurfaceIndexTokenRecord,
};
use crate::tokenization::{TokenizationConfig, visit_tokens};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use ts_rs::TS;

const CONCORDANCE_COLLOCATION_DEFAULT_SPAN: usize = 5;
const CONCORDANCE_COLLOCATION_MAX_SPAN: usize = 20;
const CONCORDANCE_COLLOCATION_DISTANCE_BUCKETS: usize = CONCORDANCE_COLLOCATION_MAX_SPAN * 2;
const CONCORDANCE_COLLOCATION_DEFAULT_LIMIT: usize = 100;
const CONCORDANCE_COLLOCATION_MAX_LIMIT: usize = 500;

/// Span side used for surface-token collocation around a concordance node.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordanceCollocationDirection {
    Left,
    Right,
    #[default]
    Both,
}

/// Association measure used to rank surface-token concordance collocates.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordanceCollocationMeasure {
    Mi,
    Mi3,
    TScore,
    ZScore,
    G2,
    #[default]
    LogDice,
    LogRatio,
}

/// Primary key used to rank surface-token concordance collocates before truncation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConcordanceCollocationSortKey {
    #[default]
    Association,
    NearQuery,
    CorpusFrequency,
}

/// Parameters for a surface-token collocation profile around concordance nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceCollocationQuery {
    #[serde(default)]
    pub selection: AnalysisDocumentSelection,
    pub query: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub span: usize,
    #[serde(default)]
    pub direction: ConcordanceCollocationDirection,
    #[serde(default = "default_collocation_floor")]
    #[ts(type = "number")]
    pub min_cooccurrence: u64,
    #[serde(default = "default_collocation_floor")]
    #[ts(type = "number")]
    pub min_collocate_frequency: u64,
    #[serde(default)]
    pub hide_stopwords: bool,
    #[serde(default)]
    pub sort_measure: ConcordanceCollocationMeasure,
    #[serde(default)]
    pub sort_key: ConcordanceCollocationSortKey,
    #[serde(default)]
    pub sort_direction: SortDirection,
    #[serde(default)]
    pub limit: usize,
}

/// One surface-token collocate with contingency counts and association scores.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceCollocateRow {
    pub surface: String,
    #[ts(type = "number")]
    pub o11: u64,
    #[ts(type = "number")]
    pub o12: i64,
    #[ts(type = "number")]
    pub o21: i64,
    #[ts(type = "number")]
    pub o22: i64,
    #[ts(type = "number")]
    pub collocate_frequency: u64,
    pub mi: Option<f64>,
    pub mi3: Option<f64>,
    pub t_score: Option<f64>,
    pub z_score: Option<f64>,
    pub g2: Option<f64>,
    pub log_dice: Option<f64>,
    pub log_ratio: Option<f64>,
}

/// Surface-token collocation profile for the current concordance query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConcordanceCollocationResult {
    pub query: String,
    pub case_sensitive: bool,
    pub span: usize,
    pub direction: ConcordanceCollocationDirection,
    #[ts(type = "number")]
    pub min_cooccurrence: u64,
    #[ts(type = "number")]
    pub min_collocate_frequency: u64,
    pub hide_stopwords: bool,
    pub sort_measure: ConcordanceCollocationMeasure,
    pub sort_key: ConcordanceCollocationSortKey,
    pub sort_direction: SortDirection,
    pub limit: usize,
    #[ts(type = "number")]
    pub r1: u64,
    #[ts(type = "number")]
    pub n: u64,
    #[ts(type = "number")]
    pub f_node: u64,
    pub total_candidates: usize,
    pub rows: Vec<ConcordanceCollocateRow>,
}

/// Aggregation inputs normalised with the same tokenisation as KWIC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcordanceCollocationAggregationSignature {
    pub query: String,
    pub case_sensitive: bool,
}

/// Per-distance counts for left distances 1..=20 followed by right distances 1..=20.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcordanceCollocationDistanceCounts {
    counts: [u64; CONCORDANCE_COLLOCATION_DISTANCE_BUCKETS],
}

/// Corpus-scan output for concordance collocates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcordanceCollocationAggregate {
    pub signature: ConcordanceCollocationAggregationSignature,
    pub symbols: Vec<String>,
    pub candidate_counts: HashMap<u32, ConcordanceCollocationDistanceCounts>,
    pub collocate_frequencies: Vec<u64>,
    pub r1: ConcordanceCollocationDistanceCounts,
    pub n: u64,
    pub f_node: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedConcordanceCollocationAggregate {
    pub aggregate: ConcordanceCollocationAggregate,
    pub backend_used: ConcordanceBackendUsed,
    pub index_rebuilt: bool,
}

/// Test/benchmark-facing instrumentation for collocation aggregation.
#[derive(Debug, Clone, Default)]
pub struct CollocationAggregationStats {
    pub candidates_visited: u64,
    pub valid_node_occurrences: u64,
    pub aggregate_entries: usize,
    pub projected_aggregate_bytes: u64,
    pub peak_aggregate_entries: usize,
    pub max_span_cache_reuse: bool,
    pub index_reuse: bool,
    pub fallback_used: bool,
    pub fallback_reason: Option<ConcordanceFallbackReason>,
}

impl Default for ConcordanceCollocationDistanceCounts {
    fn default() -> Self {
        Self {
            counts: [0; CONCORDANCE_COLLOCATION_DISTANCE_BUCKETS],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollocationWindowSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CollocationWindow {
    start: usize,
    end: usize,
    node_edge: usize,
    side: CollocationWindowSide,
}

impl ConcordanceCollocationDistanceCounts {
    fn add(&mut self, side: CollocationWindowSide, distance: usize, count: u64) {
        debug_assert!((1..=CONCORDANCE_COLLOCATION_MAX_SPAN).contains(&distance));
        let offset = match side {
            CollocationWindowSide::Left => 0,
            CollocationWindowSide::Right => CONCORDANCE_COLLOCATION_MAX_SPAN,
        };
        self.counts[offset + distance - 1] += count;
    }

    fn sum(&self, span: usize, direction: ConcordanceCollocationDirection) -> u64 {
        let span = span.min(CONCORDANCE_COLLOCATION_MAX_SPAN);
        let mut total = 0;
        if matches!(
            direction,
            ConcordanceCollocationDirection::Left | ConcordanceCollocationDirection::Both
        ) {
            total += self.counts[..span].iter().sum::<u64>();
        }
        if matches!(
            direction,
            ConcordanceCollocationDirection::Right | ConcordanceCollocationDirection::Both
        ) {
            let start = CONCORDANCE_COLLOCATION_MAX_SPAN;
            total += self.counts[start..start + span].iter().sum::<u64>();
        }
        total
    }
}

fn default_collocation_floor() -> u64 {
    5
}

fn concordance_collocation_span(span: usize) -> usize {
    if span == 0 {
        CONCORDANCE_COLLOCATION_DEFAULT_SPAN
    } else {
        span.clamp(1, CONCORDANCE_COLLOCATION_MAX_SPAN)
    }
}

fn concordance_collocation_limit(limit: usize) -> usize {
    if limit == 0 {
        CONCORDANCE_COLLOCATION_DEFAULT_LIMIT
    } else {
        limit.clamp(1, CONCORDANCE_COLLOCATION_MAX_LIMIT)
    }
}

fn add_collocation_window_record_ids(
    store: &impl IndexStore,
    records: &[SurfaceIndexTokenRecord],
    window: CollocationWindow,
    query_keys: &HashSet<String>,
    case_sensitive: bool,
    aggregate: &mut ConcordanceCollocationAggregate,
    limits: SurfaceIndexResourceLimits,
) -> Result<(), ConcordanceError> {
    for (token_index, record) in records
        .iter()
        .enumerate()
        .take(window.end)
        .skip(window.start)
    {
        let distance = match window.side {
            CollocationWindowSide::Left => window.node_edge - token_index,
            CollocationWindowSide::Right => token_index - window.node_edge + 1,
        };
        aggregate.r1.add(window.side, distance, 1);
        let symbol_id = store
            .concordance_symbol_id_for_record(record, case_sensitive)
            .ok_or({
                ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                    document_id: 0,
                    token_ordinal_in_document: 0,
                    node_token_length: 1,
                })
            })?;
        let candidate = aggregate
            .symbols
            .get(symbol_id as usize)
            .ok_or(ConcordanceError::InvalidHitIdentity)?;
        if query_keys.contains(candidate) {
            continue;
        }
        if !aggregate.candidate_counts.contains_key(&symbol_id) {
            let projected = (aggregate.candidate_counts.len() as u64 + 1).saturating_mul(
                (std::mem::size_of::<ConcordanceCollocationDistanceCounts>() + 32) as u64,
            ) + aggregate
                .symbols
                .iter()
                .map(|symbol| symbol.len() as u64 + 96)
                .sum::<u64>();
            if projected > limits.max_projected_resident_index_bytes {
                return Err(ConcordanceError::Index(
                    SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                        projected_bytes: projected,
                        limit_bytes: limits.max_projected_resident_index_bytes,
                    },
                ));
            }
            aggregate
                .candidate_counts
                .try_reserve(1)
                .map_err(collocation_allocation("indexed collocation candidates"))?;
        }
        aggregate
            .candidate_counts
            .entry(symbol_id)
            .or_default()
            .add(window.side, distance, 1);
    }
    Ok(())
}

fn finite_score(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn log2_positive(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then(|| value.log2())
}

fn i128_to_i64(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

fn g2_cell_contribution(observed: f64, expected: f64) -> Option<f64> {
    if observed < 0.0 || expected < 0.0 {
        return None;
    }
    if observed == 0.0 {
        return Some(0.0);
    }
    if expected <= 0.0 {
        return None;
    }
    finite_score(observed * (observed / expected).ln())
}

fn collocation_g2(
    observed: [f64; 4],
    row_totals: [f64; 2],
    column_totals: [f64; 2],
    n: f64,
) -> Option<f64> {
    let [r1, r2] = row_totals;
    let [c1, c2] = column_totals;
    if n <= 0.0 || r1 < 0.0 || r2 < 0.0 || c1 < 0.0 || c2 < 0.0 {
        return None;
    }
    let expected = [r1 * c1 / n, r1 * c2 / n, r2 * c1 / n, r2 * c2 / n];
    let mut sum = 0.0;
    for (observed, expected) in observed.into_iter().zip(expected) {
        sum += g2_cell_contribution(observed, expected)?;
    }
    finite_score(2.0 * sum)
}

fn collocation_row(
    surface: String,
    o11: u64,
    collocate_frequency: u64,
    r1: u64,
    n: u64,
) -> ConcordanceCollocateRow {
    let o12_i = i128_to_i64(r1 as i128 - o11 as i128);
    let o21_i = i128_to_i64(collocate_frequency as i128 - o11 as i128);
    let o22_i = i128_to_i64(n as i128 - r1 as i128 - collocate_frequency as i128 + o11 as i128);

    let o11_f = o11 as f64;
    let r1_f = r1 as f64;
    let n_f = n as f64;
    let f_c_f = collocate_frequency as f64;
    let r2_f = n_f - r1_f;
    let c2_f = n_f - f_c_f;
    let e11 = if n > 0 { r1_f * f_c_f / n_f } else { 0.0 };

    let mi = log2_positive(o11_f / e11);
    let mi3 = log2_positive(o11_f.powi(3) / e11);
    let t_score = if o11 > 0 {
        finite_score((o11_f - e11) / o11_f.sqrt())
    } else {
        None
    };
    let z_score = if e11 > 0.0 {
        finite_score((o11_f - e11) / e11.sqrt())
    } else {
        None
    };
    let g2 = collocation_g2(
        [o11_f, o12_i as f64, o21_i as f64, o22_i as f64],
        [r1_f, r2_f],
        [f_c_f, c2_f],
        n_f,
    );
    let log_dice =
        log2_positive((2.0 * o11_f) / (r1_f + f_c_f)).and_then(|value| finite_score(14.0 + value));
    let log_ratio = if r1_f > 0.0 && r2_f > 0.0 && o21_i > 0 {
        log2_positive((o11_f / r1_f) / (o21_i as f64 / r2_f))
    } else {
        None
    };

    ConcordanceCollocateRow {
        surface,
        o11,
        o12: o12_i,
        o21: o21_i,
        o22: o22_i,
        collocate_frequency,
        mi,
        mi3,
        t_score,
        z_score,
        g2,
        log_dice,
        log_ratio,
    }
}

fn collocation_score_for_measure(
    row: &ConcordanceCollocateRow,
    measure: ConcordanceCollocationMeasure,
) -> Option<f64> {
    match measure {
        ConcordanceCollocationMeasure::Mi => row.mi,
        ConcordanceCollocationMeasure::Mi3 => row.mi3,
        ConcordanceCollocationMeasure::TScore => row.t_score,
        ConcordanceCollocationMeasure::ZScore => row.z_score,
        ConcordanceCollocationMeasure::G2 => row.g2,
        ConcordanceCollocationMeasure::LogDice => row.log_dice,
        ConcordanceCollocationMeasure::LogRatio => row.log_ratio,
    }
}

fn compare_collocation_rows(
    left: &ConcordanceCollocateRow,
    right: &ConcordanceCollocateRow,
    sort_key: ConcordanceCollocationSortKey,
    measure: ConcordanceCollocationMeasure,
    sort_direction: SortDirection,
) -> Ordering {
    fn compare_count(left: u64, right: u64, sort_direction: SortDirection) -> Ordering {
        match sort_direction {
            SortDirection::Asc => left.cmp(&right),
            SortDirection::Desc => right.cmp(&left),
        }
    }

    fn compare_score(
        left: &ConcordanceCollocateRow,
        right: &ConcordanceCollocateRow,
        measure: ConcordanceCollocationMeasure,
        sort_direction: SortDirection,
    ) -> Ordering {
        match (
            collocation_score_for_measure(left, measure),
            collocation_score_for_measure(right, measure),
        ) {
            (Some(left), Some(right)) => {
                let ordering = left.partial_cmp(&right).unwrap_or(Ordering::Equal);
                match sort_direction {
                    SortDirection::Asc => ordering,
                    SortDirection::Desc => ordering.reverse(),
                }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }

    let score_ordering = match (sort_key, sort_direction) {
        (ConcordanceCollocationSortKey::Association, direction) => {
            compare_score(left, right, measure, direction)
        }
        (ConcordanceCollocationSortKey::NearQuery, direction) => {
            compare_count(left.o11, right.o11, direction)
        }
        (ConcordanceCollocationSortKey::CorpusFrequency, direction) => compare_count(
            left.collocate_frequency,
            right.collocate_frequency,
            direction,
        ),
    };
    score_ordering
        .then_with(|| compare_score(left, right, measure, SortDirection::Desc))
        .then_with(|| right.o11.cmp(&left.o11))
        .then_with(|| right.collocate_frequency.cmp(&left.collocate_frequency))
        .then_with(|| left.surface.cmp(&right.surface))
}

struct NumericCollocationRow {
    symbol_id: u32,
    row: ConcordanceCollocateRow,
}

fn compare_numeric_collocation_rows(
    left: &NumericCollocationRow,
    right: &NumericCollocationRow,
    query: &ConcordanceCollocationQuery,
    symbols: &[String],
) -> Ordering {
    compare_collocation_rows(
        &left.row,
        &right.row,
        query.sort_key,
        query.sort_measure,
        query.sort_direction,
    )
    .then_with(|| symbols[left.symbol_id as usize].cmp(&symbols[right.symbol_id as usize]))
}

fn push_bounded_collocation_row(
    heap: &mut Vec<NumericCollocationRow>,
    row: NumericCollocationRow,
    limit: usize,
    query: &ConcordanceCollocationQuery,
    symbols: &[String],
) {
    if limit == 0 {
        return;
    }
    if heap.len() < limit {
        heap.push(row);
        let mut index = heap.len() - 1;
        while index > 0 {
            let parent = (index - 1) / 2;
            if compare_numeric_collocation_rows(&heap[index], &heap[parent], query, symbols)
                != Ordering::Greater
            {
                break;
            }
            heap.swap(index, parent);
            index = parent;
        }
        return;
    }
    if compare_numeric_collocation_rows(&row, &heap[0], query, symbols) != Ordering::Less {
        return;
    }
    heap[0] = row;
    let mut index = 0;
    loop {
        let left = index * 2 + 1;
        if left >= heap.len() {
            break;
        }
        let right = left + 1;
        let mut worse = left;
        if right < heap.len()
            && compare_numeric_collocation_rows(&heap[right], &heap[left], query, symbols)
                == Ordering::Greater
        {
            worse = right;
        }
        if compare_numeric_collocation_rows(&heap[worse], &heap[index], query, symbols)
            != Ordering::Greater
        {
            break;
        }
        heap.swap(index, worse);
        index = worse;
    }
}

fn resolve_concordance_collocation_aggregation(
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<
    (
        ConcordanceCollocationAggregationSignature,
        Vec<String>,
        HashSet<String>,
    ),
    ConcordanceError,
> {
    let query_tokens = resolve_concordance_query_tokens(&query.query, tokenization_config)?;
    let query_keys = query_tokens
        .iter()
        .map(|token| concordance_key(token, query.case_sensitive))
        .collect::<Vec<_>>();
    let query_key_set = query_keys.iter().cloned().collect::<HashSet<_>>();
    Ok((
        ConcordanceCollocationAggregationSignature {
            query: query_tokens.join(" "),
            case_sensitive: query.case_sensitive,
        },
        query_keys,
        query_key_set,
    ))
}

/// Normalises the corpus-scan inputs used to key a cached collocation aggregate.
pub fn concordance_collocation_aggregation_signature(
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceCollocationAggregationSignature, ConcordanceError> {
    resolve_concordance_collocation_aggregation(query, tokenization_config)
        .map(|(signature, _, _)| signature)
}

struct NumericCollocationBuilder {
    symbols: Vec<String>,
    symbol_ids: HashMap<String, u32>,
    candidate_counts: HashMap<u32, ConcordanceCollocationDistanceCounts>,
    collocate_frequencies: Vec<u64>,
    r1: ConcordanceCollocationDistanceCounts,
    n: u64,
    f_node: u64,
    limits: SurfaceIndexResourceLimits,
    cancellation: SurfaceIndexCancellationToken,
    peak_candidate_entries: usize,
}

impl NumericCollocationBuilder {
    fn new(limits: SurfaceIndexResourceLimits, cancellation: SurfaceIndexCancellationToken) -> Self {
        Self {
            symbols: Vec::new(),
            symbol_ids: HashMap::new(),
            candidate_counts: HashMap::new(),
            collocate_frequencies: Vec::new(),
            r1: ConcordanceCollocationDistanceCounts::default(),
            n: 0,
            f_node: 0,
            limits,
            cancellation,
            peak_candidate_entries: 0,
        }
    }

    fn check_cancelled(&self) -> Result<(), ConcordanceError> {
        if self.f_node % 1_024 == 0 && self.cancellation.is_cancelled() {
            return Err(ConcordanceError::Index(SurfaceIndexError::Cancelled));
        }
        Ok(())
    }


    fn intern(&mut self, key: &str) -> Result<u32, ConcordanceError> {
        if let Some(id) = self.symbol_ids.get(key) {
            return Ok(*id);
        }
        let projected = self.projected_bytes().saturating_add(
            key.len() as u64
                + std::mem::size_of::<String>() as u64
                + std::mem::size_of::<u64>() as u64
                + 96,
        );
        if projected > self.limits.max_projected_resident_index_bytes {
            return Err(ConcordanceError::Index(
                SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                    projected_bytes: projected,
                    limit_bytes: self.limits.max_projected_resident_index_bytes,
                },
            ));
        }
        let id = u32::try_from(self.symbols.len()).map_err(|_| {
            ConcordanceError::Index(SurfaceIndexError::Allocation(
                "collocation symbol table exceeds u32 capacity".to_string(),
            ))
        })?;
        self.symbols.try_reserve(1).map_err(collocation_allocation("collocation symbols"))?;
        self.collocate_frequencies
            .try_reserve(1)
            .map_err(collocation_allocation("collocation frequencies"))?;
        self.symbol_ids
            .try_reserve(1)
            .map_err(collocation_allocation("collocation symbol lookup"))?;
        let mut owned = String::new();
        owned
            .try_reserve(key.len())
            .map_err(collocation_allocation("collocation symbol text"))?;
        owned.push_str(key);
        self.symbols.push(owned.clone());
        self.collocate_frequencies.push(0);
        self.symbol_ids.insert(owned, id);
        Ok(id)
    }

    fn record_token(&mut self, key: &str) -> Result<u32, ConcordanceError> {
        let id = self.intern(key)?;
        self.collocate_frequencies[id as usize] = self.collocate_frequencies[id as usize]
            .checked_add(1)
            .ok_or_else(|| {
                ConcordanceError::Index(SurfaceIndexError::Allocation(
                    "collocation frequency overflow".to_string(),
                ))
            })?;
        self.n += 1;
        Ok(id)
    }

    fn add_window_candidate(
        &mut self,
        symbol_id: u32,
        side: CollocationWindowSide,
        distance: usize,
        query_key_set: &HashSet<String>,
    ) -> Result<(), ConcordanceError> {
        self.r1.add(side, distance, 1);
        if query_key_set.contains(&self.symbols[symbol_id as usize]) {
            return Ok(());
        }
        if !self.candidate_counts.contains_key(&symbol_id) {
            let projected = self
                .projected_bytes()
                .saturating_add(std::mem::size_of::<ConcordanceCollocationDistanceCounts>() as u64)
                .saturating_add(32);
            if projected > self.limits.max_projected_resident_index_bytes {
                return Err(ConcordanceError::Index(
                    SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                        projected_bytes: projected,
                        limit_bytes: self.limits.max_projected_resident_index_bytes,
                    },
                ));
            }
            self.candidate_counts
                .try_reserve(1)
                .map_err(collocation_allocation("collocation candidate counts"))?;
        }
        self.candidate_counts
            .entry(symbol_id)
            .or_default()
            .add(side, distance, 1);
        if self.candidate_counts.len() > self.peak_candidate_entries {
            self.peak_candidate_entries = self.candidate_counts.len();
        }
        Ok(())
    }

    fn projected_bytes(&self) -> u64 {
        self.symbols.iter().map(|symbol| symbol.len() as u64).sum::<u64>()
            + (self.symbols.len()
                * (std::mem::size_of::<String>() + std::mem::size_of::<u64>() + 96))
                as u64
            + (self.candidate_counts.len()
                * (std::mem::size_of::<ConcordanceCollocationDistanceCounts>() + 32))
                as u64
    }

    fn finish(
        self,
        signature: ConcordanceCollocationAggregationSignature,
    ) -> (ConcordanceCollocationAggregate, CollocationAggregationStats) {
        let stats = CollocationAggregationStats {
            valid_node_occurrences: self.f_node,
            aggregate_entries: self.candidate_counts.len(),
            projected_aggregate_bytes: self.projected_bytes(),
            peak_aggregate_entries: self.peak_candidate_entries,
            ..Default::default()
        };
        let aggregate = ConcordanceCollocationAggregate {
            signature,
            symbols: self.symbols,
            candidate_counts: self.candidate_counts,
            collocate_frequencies: self.collocate_frequencies,
            r1: self.r1,
            n: self.n,
            f_node: self.f_node,
        };
        (aggregate, stats)
    }
}

fn collocation_allocation(
    purpose: &'static str,
) -> impl FnOnce(std::collections::TryReserveError) -> ConcordanceError {
    move |error| {
        ConcordanceError::Index(SurfaceIndexError::Allocation(format!(
            "{purpose}: {error}"
        )))
    }
}

fn aggregate_collocation_document_streaming(
    builder: &mut NumericCollocationBuilder,
    document: &ConcordanceDocument,
    query_keys: &[String],
    query_key_set: &HashSet<String>,
    case_sensitive: bool,
    tokenization_config: &TokenizationConfig,
) -> Result<(), ConcordanceError> {
    let document_id = u32::try_from(document.document_index)
        .map_err(|_| ConcordanceError::InvalidDocumentIdentity)?;
    let mut buffer = VecDeque::<u32>::new();
    let mut buffer_start = 0usize;
    let mut total_tokens = 0usize;
    let mut next_candidate = 0usize;
    visit_tokens(&document.text, document_id, tokenization_config, |token| {
        if !token.counts_as_word() {
            return Ok(());
        }
        let key = concordance_key(token.surface, case_sensitive);
        let symbol_id = builder.record_token(&key)?;
        buffer.push_back(symbol_id);
        total_tokens += 1;
        while next_candidate
            .checked_add(query_keys.len())
            .and_then(|end| end.checked_add(CONCORDANCE_COLLOCATION_MAX_SPAN))
            .is_some_and(|ready| ready <= total_tokens)
        {
            process_streaming_collocation_candidate(
                builder,
                &buffer,
                buffer_start,
                total_tokens,
                next_candidate,
                query_keys,
                query_key_set,
            )?;
            next_candidate += 1;
            let keep_from = next_candidate.saturating_sub(CONCORDANCE_COLLOCATION_MAX_SPAN);
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
        process_streaming_collocation_candidate(
            builder,
            &buffer,
            buffer_start,
            total_tokens,
            next_candidate,
            query_keys,
            query_key_set,
        )?;
        next_candidate += 1;
        let keep_from = next_candidate.saturating_sub(CONCORDANCE_COLLOCATION_MAX_SPAN);
        while buffer_start < keep_from {
            buffer.pop_front();
            buffer_start += 1;
        }
    }
    Ok(())
}

fn process_streaming_collocation_candidate(
    builder: &mut NumericCollocationBuilder,
    buffer: &VecDeque<u32>,
    buffer_start: usize,
    total_tokens: usize,
    token_index: usize,
    query_keys: &[String],
    query_key_set: &HashSet<String>,
) -> Result<(), ConcordanceError> {
    let node_end = token_index + query_keys.len();
    let node_matches = (token_index..node_end).zip(query_keys).all(|(index, key)| {
        collocation_buffer_symbol(buffer, buffer_start, index)
            .is_some_and(|id| builder.symbols[id as usize] == *key)
    });
    if !node_matches {
        return Ok(());
    }
    builder.f_node += 1;
    builder.check_cancelled()?;
    let left_start = token_index.saturating_sub(CONCORDANCE_COLLOCATION_MAX_SPAN);
    for index in left_start..token_index {
        let id = collocation_buffer_symbol(buffer, buffer_start, index)
            .ok_or(ConcordanceError::InvalidHitIdentity)?;
        builder.add_window_candidate(
            id,
            CollocationWindowSide::Left,
            token_index - index,
            query_key_set,
        )?;
    }
    let right_end = node_end
        .saturating_add(CONCORDANCE_COLLOCATION_MAX_SPAN)
        .min(total_tokens);
    for index in node_end..right_end {
        let id = collocation_buffer_symbol(buffer, buffer_start, index)
            .ok_or(ConcordanceError::InvalidHitIdentity)?;
        builder.add_window_candidate(
            id,
            CollocationWindowSide::Right,
            index - node_end + 1,
            query_key_set,
        )?;
    }
    Ok(())
}

fn collocation_buffer_symbol(
    buffer: &VecDeque<u32>,
    buffer_start: usize,
    absolute_index: usize,
) -> Option<u32> {
    absolute_index
        .checked_sub(buffer_start)
        .and_then(|index| buffer.get(index))
        .copied()
}

/// Counts collocate candidates around every concordance hit in the selected documents.
pub fn aggregate_concordance_collocations_documents(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    cancellation: SurfaceIndexCancellationToken,
) -> Result<ConcordanceCollocationAggregate, ConcordanceError> {
    let (signature, query_keys, query_key_set) =
        resolve_concordance_collocation_aggregation(query, tokenization_config)?;
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);

    let mut ordered_documents = Vec::new();
    ordered_documents
        .try_reserve(documents.len())
        .map_err(collocation_allocation("collocation document ordering"))?;
    ordered_documents.extend(
        documents
            .iter()
            .filter(|document| selected_document_ids.contains(&document.document_index)),
    );
    ordered_documents.sort_by_key(|document| document.document_index);
    let mut builder = NumericCollocationBuilder::new(
        SurfaceIndexResourceLimits::default(),
        cancellation.clone(),
    );
    for document in ordered_documents {
        if cancellation.is_cancelled() {
            return Err(ConcordanceError::Index(SurfaceIndexError::Cancelled));
        }
        aggregate_collocation_document_streaming(
            &mut builder,
            document,
            &query_keys,
            &query_key_set,
            query.case_sensitive,
            tokenization_config,
        )?;
    }
    let (aggregate, _stats) = builder.finish(signature);
    Ok(aggregate)
}

pub fn aggregate_concordance_collocations_index(
    store: &impl IndexStore,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    cancellation: SurfaceIndexCancellationToken,
) -> Result<(ConcordanceCollocationAggregate, CollocationAggregationStats), ConcordanceError> {
    let (signature, _, query_key_set) =
        resolve_concordance_collocation_aggregation(query, tokenization_config)?;
    let index_query = SurfaceIndexQuery::from_surface_text_with_case(
        &query.query,
        query.case_sensitive,
        tokenization_config,
    )
    .map_err(collocation_error_from_surface_index)?;
    let selected_document_ids = if query.selection.all_documents {
        let excluded = query
            .selection
            .excluded_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        store
            .indexed_documents()
            .filter(|document| !excluded.contains(&document.document_id))
            .map(|document| document.document_id)
            .collect::<HashSet<_>>()
    } else {
        let requested = query.selection.ids.iter().copied().collect::<HashSet<_>>();
        store
            .indexed_documents()
            .filter(|document| requested.contains(&document.document_id))
            .map(|document| document.document_id)
            .collect::<HashSet<_>>()
    };
    let selected_tokens = store
        .indexed_documents()
        .filter(|document| selected_document_ids.contains(&document.document_id))
        .map(|document| document.token_count as u64)
        .sum();
    let mut symbols = Vec::new();
    for symbol in store.concordance_symbols(query.case_sensitive) {
        symbols
            .try_reserve(1)
            .map_err(collocation_allocation("indexed collocation symbols"))?;
        let mut owned = String::new();
        owned
            .try_reserve(symbol.len())
            .map_err(collocation_allocation("indexed collocation symbol text"))?;
        owned.push_str(symbol);
        symbols.push(owned);
    }
    let limits = SurfaceIndexResourceLimits::default();
    let base_estimate = symbols.iter().map(|symbol| symbol.len() as u64).sum::<u64>()
        + (symbols.len() * (std::mem::size_of::<String>() + std::mem::size_of::<u64>() + 64))
            as u64;
    if base_estimate > limits.max_projected_resident_index_bytes {
        return Err(ConcordanceError::Index(
            SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                projected_bytes: base_estimate,
                limit_bytes: limits.max_projected_resident_index_bytes,
            },
        ));
    }
    let mut collocate_frequencies = Vec::new();
    collocate_frequencies
        .try_reserve_exact(symbols.len())
        .map_err(collocation_allocation("indexed collocation frequencies"))?;
    collocate_frequencies.resize(symbols.len(), 0);
    for document in store
        .indexed_documents()
        .filter(|document| selected_document_ids.contains(&document.document_id))
    {
        if cancellation.is_cancelled() {
            return Err(ConcordanceError::Index(SurfaceIndexError::Cancelled));
        }
        let records = store.records_for_document(document.document_id).ok_or_else(|| {
            ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                document_id: document.document_id,
                token_ordinal_in_document: 0,
                node_token_length: 0,
            })
        })?;
        for record in records {
            let id = store
                .concordance_symbol_id_for_record(record, query.case_sensitive)
                .ok_or_else(|| {
                    ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                        document_id: document.document_id,
                        token_ordinal_in_document: 0,
                        node_token_length: 1,
                    })
                })? as usize;
            collocate_frequencies[id] += 1;
        }
    }
    let mut aggregate = ConcordanceCollocationAggregate {
        signature,
        symbols,
        candidate_counts: HashMap::new(),
        collocate_frequencies,
        r1: ConcordanceCollocationDistanceCounts::default(),
        n: selected_tokens,
        f_node: 0,
    };

    let mut peak_aggregate_entries = 0;

    visit_index_query_hits(store, &index_query, |hit| {
        if aggregate.f_node % 1_024 == 0 && cancellation.is_cancelled() {
            return Err(ConcordanceError::Index(SurfaceIndexError::Cancelled));
        }
        if !selected_document_ids.contains(&hit.document_id) {
            return Ok(());
        }
        aggregate.f_node += 1;
        let records = store
            .records_for_document(hit.document_id)
            .ok_or_else(|| collocation_inconsistent_index_error(&hit))?;
        let node_end = hit
            .token_ordinal_in_document
            .checked_add(hit.node_token_length)
            .ok_or_else(|| collocation_inconsistent_index_error(&hit))?;
        if hit.node_token_length == 0 || node_end > records.len() {
            return Err(collocation_inconsistent_index_error(&hit));
        }

        let left_start = hit
            .token_ordinal_in_document
            .saturating_sub(CONCORDANCE_COLLOCATION_MAX_SPAN);
        add_collocation_window_record_ids(
            store,
            records,
            CollocationWindow {
                start: left_start,
                end: hit.token_ordinal_in_document,
                node_edge: hit.token_ordinal_in_document,
                side: CollocationWindowSide::Left,
            },
            &query_key_set,
            query.case_sensitive,
            &mut aggregate,
            limits,
        )?;
        if aggregate.candidate_counts.len() > peak_aggregate_entries {
            peak_aggregate_entries = aggregate.candidate_counts.len();
        }

        let right_end = node_end
            .saturating_add(CONCORDANCE_COLLOCATION_MAX_SPAN)
            .min(records.len());
        add_collocation_window_record_ids(
            store,
            records,
            CollocationWindow {
                start: node_end,
                end: right_end,
                node_edge: node_end,
                side: CollocationWindowSide::Right,
            },
            &query_key_set,
            query.case_sensitive,
            &mut aggregate,
            limits,
        )?;
        if aggregate.candidate_counts.len() > peak_aggregate_entries {
            peak_aggregate_entries = aggregate.candidate_counts.len();
        }
        Ok(())
    })?;

    let stats = CollocationAggregationStats {
        valid_node_occurrences: aggregate.f_node,
        aggregate_entries: aggregate.candidate_counts.len(),
        projected_aggregate_bytes: base_estimate, // Simplified for indexed, we don't recalculate the whole string overhead
        peak_aggregate_entries,
        ..Default::default()
    };
    Ok((aggregate, stats))
}

pub fn aggregate_concordance_collocations_documents_routed(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
    cancellation: &SurfaceIndexCancellationToken,
) -> Result<RoutedConcordanceCollocationAggregate, ConcordanceError> {
    match backend {
        ConcordanceBackend::Scan => Ok(RoutedConcordanceCollocationAggregate {
            aggregate: aggregate_concordance_collocations_documents(
                documents,
                query,
                tokenization_config,
                cancellation.clone(),
            )?,
            backend_used: ConcordanceBackendUsed::Scan,
            index_rebuilt: false,
        }),
        ConcordanceBackend::Index => {
            let selected_document_ids =
                selected_concordance_document_ids(documents, &query.selection);
            let mut index_rebuilt = false;
            let index_result = (|| {
                let (store, rebuilt) = context.index_store_for(
                    documents,
                    &selected_document_ids,
                    tokenization_config,
                )?;
                index_rebuilt = rebuilt;
                aggregate_concordance_collocations_index(store, query, tokenization_config, cancellation.clone())
                    .map(|(aggregate, _stats)| aggregate)
            })();

            match index_result {
                Ok(aggregate) => Ok(RoutedConcordanceCollocationAggregate {
                    aggregate,
                    backend_used: ConcordanceBackendUsed::Index,
                    index_rebuilt,
                }),
                Err(error @ ConcordanceError::Index(_))
                    if error.prevents_unbounded_scan_fallback() =>
                {
                    Err(error)
                }
                Err(ConcordanceError::Index(_)) => Ok(RoutedConcordanceCollocationAggregate {
                    aggregate: aggregate_concordance_collocations_documents(
                        documents,
                        query,
                        tokenization_config,
                        cancellation.clone(),
                    )?,
                    backend_used: ConcordanceBackendUsed::ScanFallback {
                        reason: ConcordanceFallbackReason::IndexError,
                    },
                    index_rebuilt,
                }),
                Err(error) => Err(error),
            }
        }
    }
}

pub fn aggregate_concordance_collocations_routed_provider<Resolve>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    cancellation: &SurfaceIndexCancellationToken,
    resolve: Resolve,
) -> Result<RoutedConcordanceCollocationAggregate, ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
{
    let (store, index_rebuilt) =
        context.index_store_for_provider(document_slots, tokenization_config, resolve)?;
    let (aggregate, _stats) = aggregate_concordance_collocations_index(store, query, tokenization_config, cancellation.clone())?;
    Ok(RoutedConcordanceCollocationAggregate {
        aggregate,
        backend_used: ConcordanceBackendUsed::Index,
        index_rebuilt,
    })
}

pub fn aggregate_concordance_collocations_scan_provider<Resolve>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    cancellation: &SurfaceIndexCancellationToken,
    mut resolve: Resolve,
) -> Result<RoutedConcordanceCollocationAggregate, ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
{
    let (signature, query_keys, query_key_set) =
        resolve_concordance_collocation_aggregation(query, tokenization_config)?;
    let mut builder = NumericCollocationBuilder::new(SurfaceIndexResourceLimits::default(), cancellation.clone());
    for document_index in 0..document_slots {
        let selected = if query.selection.all_documents {
            !query.selection.excluded_ids.contains(&document_index)
        } else {
            query.selection.ids.contains(&document_index)
        };
        if !selected {
            continue;
        }
        let Some(document) = resolve(document_index) else {
            continue;
        };
        if document.document_index != document_index {
            return Err(ConcordanceError::InvalidDocumentIdentity);
        }
        aggregate_collocation_document_streaming(
            &mut builder,
            &document,
            &query_keys,
            &query_key_set,
            query.case_sensitive,
            tokenization_config,
        )?;
    }
    let (aggregate, _stats) = builder.finish(signature);
    Ok(RoutedConcordanceCollocationAggregate {
        aggregate,
        backend_used: ConcordanceBackendUsed::Scan,
        index_rebuilt: false,
    })
}

fn collocation_error_from_surface_index(error: SurfaceIndexError) -> ConcordanceError {
    match error {
        SurfaceIndexError::EmptyQuery => ConcordanceError::EmptyQuery,
        error => ConcordanceError::Index(error),
    }
}

fn collocation_inconsistent_index_error(hit: &crate::index::SurfaceIndexHit) -> ConcordanceError {
    ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
        document_id: hit.document_id,
        token_ordinal_in_document: hit.token_ordinal_in_document,
        node_token_length: hit.node_token_length,
    })
}

/// Applies presentation filters, scoring, ranking, and truncation to a cached aggregate.
pub fn query_concordance_collocations_from_aggregate(
    aggregate: &ConcordanceCollocationAggregate,
    query: &ConcordanceCollocationQuery,
) -> ConcordanceCollocationResult {
    let span = concordance_collocation_span(query.span);
    let direction = query.direction;
    let limit = concordance_collocation_limit(query.limit);
    let r1 = aggregate.r1.sum(span, direction);
    let mut retained = Vec::<NumericCollocationRow>::new();
    let _ = retained.try_reserve(limit);
    let mut total_candidates = 0usize;
    for (&symbol_id, counts) in &aggregate.candidate_counts {
        let Some(surface) = aggregate.symbols.get(symbol_id as usize) else {
            continue;
        };
        if query.hide_stopwords
            && stopword_matches(
                surface,
                StopwordLanguage::from_subtag(query.language.as_deref()),
            )
        {
            continue;
        }
        let collocate_frequency = aggregate
            .collocate_frequencies
            .get(symbol_id as usize)
            .copied()
            .unwrap_or_default();
        let o11 = counts.sum(span, direction);
        if o11 == 0
            || o11 < query.min_cooccurrence
            || collocate_frequency < query.min_collocate_frequency
        {
            continue;
        }
        total_candidates += 1;
        push_bounded_collocation_row(
            &mut retained,
            NumericCollocationRow {
                symbol_id,
                row: collocation_row(
                    String::new(),
                    o11,
                    collocate_frequency,
                    r1,
                    aggregate.n,
                ),
            },
            limit,
            query,
            &aggregate.symbols,
        );
    }
    retained.sort_by(|left, right| {
        compare_numeric_collocation_rows(left, right, query, &aggregate.symbols)
    });
    let rows = retained
        .into_iter()
        .map(|mut retained| {
            retained.row.surface = aggregate.symbols[retained.symbol_id as usize].clone();
            retained.row
        })
        .collect::<Vec<_>>();

    ConcordanceCollocationResult {
        query: aggregate.signature.query.clone(),
        case_sensitive: aggregate.signature.case_sensitive,
        span,
        direction,
        min_cooccurrence: query.min_cooccurrence,
        min_collocate_frequency: query.min_collocate_frequency,
        hide_stopwords: query.hide_stopwords,
        sort_measure: query.sort_measure,
        sort_key: query.sort_key,
        sort_direction: query.sort_direction,
        limit,
        r1,
        n: aggregate.n,
        f_node: aggregate.f_node,
        total_candidates,
        rows,
    }
}

/// Computes surface-token collocates around every concordance hit in the selected documents.
pub fn query_concordance_collocations_documents(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    cancellation: &SurfaceIndexCancellationToken,
) -> Result<ConcordanceCollocationResult, ConcordanceError> {
    let aggregate =
        aggregate_concordance_collocations_documents(documents, query, tokenization_config, cancellation.clone())?;
    Ok(query_concordance_collocations_from_aggregate(
        &aggregate, query,
    ))
}

#[cfg(test)]
#[path = "collocation_tests.rs"]
mod tests;
