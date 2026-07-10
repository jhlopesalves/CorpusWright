use super::build::AnalysisDocumentSelection;
use super::concordance::{
    ConcordanceBackend, ConcordanceBackendUsed, ConcordanceDocument, ConcordanceError,
    ConcordanceFallbackReason, ConcordanceSearchContext, analysis_word_tokens, concordance_key,
    resolve_concordance_query_tokens, selected_concordance_document_ids,
};
use super::frequency::SortDirection;
use super::shared::{StopwordLanguage, stopword_matches};
use crate::index::{
    IndexStore, SurfaceIndexError, SurfaceIndexQuery, SurfaceIndexTokenRecord, search_surface_index,
};
use crate::tokenization::TokenizationConfig;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
    pub candidate_counts: HashMap<String, ConcordanceCollocationDistanceCounts>,
    pub collocate_frequencies: HashMap<String, u64>,
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

    fn merge(&mut self, other: &Self) {
        for (left, right) in self.counts.iter_mut().zip(other.counts) {
            *left += right;
        }
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

fn add_collocation_window_token_keys(
    token_keys: &[String],
    window: CollocationWindow,
    query_keys: &HashSet<String>,
    r1: &mut ConcordanceCollocationDistanceCounts,
    candidate_counts: &mut HashMap<String, ConcordanceCollocationDistanceCounts>,
) {
    for (token_index, candidate) in token_keys
        .iter()
        .enumerate()
        .take(window.end)
        .skip(window.start)
    {
        let distance = match window.side {
            CollocationWindowSide::Left => window.node_edge - token_index,
            CollocationWindowSide::Right => token_index - window.node_edge + 1,
        };
        r1.add(window.side, distance, 1);

        if query_keys.contains(candidate) {
            continue;
        }
        candidate_counts
            .entry(candidate.clone())
            .or_default()
            .add(window.side, distance, 1);
    }
}

fn add_collocation_window_record_keys(
    store: &impl IndexStore,
    records: &[SurfaceIndexTokenRecord],
    window: CollocationWindow,
    query_keys: &HashSet<String>,
    case_sensitive: bool,
    r1: &mut ConcordanceCollocationDistanceCounts,
    candidate_counts: &mut HashMap<String, ConcordanceCollocationDistanceCounts>,
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
        r1.add(window.side, distance, 1);

        let candidate = store
            .concordance_key_for_record(record, case_sensitive)
            .ok_or_else(|| {
                ConcordanceError::Index(SurfaceIndexError::InconsistentStore {
                    document_id: record.document_id,
                    token_ordinal_in_document: record.token_ordinal_in_document,
                    node_token_length: 1,
                })
            })?;
        if query_keys.contains(candidate) {
            continue;
        }
        candidate_counts
            .entry(candidate.to_string())
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

#[derive(Debug)]
struct ConcordanceCollocationDocumentAggregate {
    document_index: usize,
    candidate_counts: HashMap<String, ConcordanceCollocationDistanceCounts>,
    collocate_frequencies: HashMap<String, u64>,
    r1: ConcordanceCollocationDistanceCounts,
    n: u64,
    f_node: u64,
}

impl ConcordanceCollocationAggregate {
    fn merge_document(&mut self, document: ConcordanceCollocationDocumentAggregate) {
        for (candidate, counts) in document.candidate_counts {
            self.candidate_counts
                .entry(candidate)
                .or_default()
                .merge(&counts);
        }
        for (candidate, frequency) in document.collocate_frequencies {
            *self.collocate_frequencies.entry(candidate).or_default() += frequency;
        }
        self.r1.merge(&document.r1);
        self.n += document.n;
        self.f_node += document.f_node;
    }
}

fn aggregate_concordance_collocations_document(
    document: &ConcordanceDocument,
    query_keys: &[String],
    query_key_set: &HashSet<String>,
    case_sensitive: bool,
    tokenization_config: &TokenizationConfig,
) -> ConcordanceCollocationDocumentAggregate {
    let tokens = analysis_word_tokens(
        &document.text,
        document.document_index as u32,
        tokenization_config,
    );
    let token_keys = tokens
        .iter()
        .map(|token| concordance_key(token.surface, case_sensitive))
        .collect::<Vec<_>>();
    let mut collocate_frequencies = HashMap::<String, u64>::new();
    let mut candidate_counts = HashMap::<String, ConcordanceCollocationDistanceCounts>::new();
    let mut r1 = ConcordanceCollocationDistanceCounts::default();
    let mut f_node = 0u64;

    for key in &token_keys {
        *collocate_frequencies.entry(key.clone()).or_default() += 1;
    }

    for token_index in 0..token_keys.len() {
        let node_end = token_index + query_keys.len();
        if node_end > token_keys.len() {
            break;
        }
        let node_matches = token_keys[token_index..node_end]
            .iter()
            .zip(query_keys)
            .all(|(token_key, query_key)| token_key == query_key);
        if !node_matches {
            continue;
        }

        f_node += 1;
        let left_start = token_index.saturating_sub(CONCORDANCE_COLLOCATION_MAX_SPAN);
        add_collocation_window_token_keys(
            &token_keys,
            CollocationWindow {
                start: left_start,
                end: token_index,
                node_edge: token_index,
                side: CollocationWindowSide::Left,
            },
            query_key_set,
            &mut r1,
            &mut candidate_counts,
        );

        let right_end = (node_end + CONCORDANCE_COLLOCATION_MAX_SPAN).min(token_keys.len());
        add_collocation_window_token_keys(
            &token_keys,
            CollocationWindow {
                start: node_end,
                end: right_end,
                node_edge: node_end,
                side: CollocationWindowSide::Right,
            },
            query_key_set,
            &mut r1,
            &mut candidate_counts,
        );
    }

    ConcordanceCollocationDocumentAggregate {
        document_index: document.document_index,
        candidate_counts,
        collocate_frequencies,
        r1,
        n: token_keys.len() as u64,
        f_node,
    }
}

/// Counts collocate candidates around every concordance hit in the selected documents.
pub fn aggregate_concordance_collocations_documents(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceCollocationAggregate, ConcordanceError> {
    let (signature, query_keys, query_key_set) =
        resolve_concordance_collocation_aggregation(query, tokenization_config)?;
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);

    let mut document_aggregates = documents
        .par_iter()
        .filter(|document| selected_document_ids.contains(&document.document_index))
        .map(|document| {
            aggregate_concordance_collocations_document(
                document,
                &query_keys,
                &query_key_set,
                query.case_sensitive,
                tokenization_config,
            )
        })
        .collect::<Vec<_>>();
    document_aggregates.sort_by_key(|aggregate| aggregate.document_index);

    let mut aggregate = ConcordanceCollocationAggregate {
        signature,
        candidate_counts: HashMap::new(),
        collocate_frequencies: HashMap::new(),
        r1: ConcordanceCollocationDistanceCounts::default(),
        n: 0,
        f_node: 0,
    };
    for document_aggregate in document_aggregates {
        aggregate.merge_document(document_aggregate);
    }

    Ok(aggregate)
}

pub fn aggregate_concordance_collocations_index(
    store: &impl IndexStore,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<ConcordanceCollocationAggregate, ConcordanceError> {
    let (signature, _, query_key_set) =
        resolve_concordance_collocation_aggregation(query, tokenization_config)?;
    let index_query = SurfaceIndexQuery::from_surface_text_with_case(
        &query.query,
        query.case_sensitive,
        tokenization_config,
    )
    .map_err(collocation_error_from_surface_index)?;
    let index =
        search_surface_index(store, &index_query).map_err(collocation_error_from_surface_index)?;

    let mut aggregate = ConcordanceCollocationAggregate {
        signature,
        candidate_counts: HashMap::new(),
        collocate_frequencies: HashMap::new(),
        r1: ConcordanceCollocationDistanceCounts::default(),
        n: store.total_indexed_tokens() as u64,
        f_node: index.hits.len() as u64,
    };

    for term in store.indexed_terms_for_concordance_key(query.case_sensitive) {
        let frequency = store
            .postings_for_concordance_key(term, query.case_sensitive)
            .map(|postings| postings.len() as u64)
            .unwrap_or_default();
        if frequency > 0 {
            aggregate
                .collocate_frequencies
                .insert(term.to_string(), frequency);
        }
    }

    for hit in index.hits {
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
        add_collocation_window_record_keys(
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
            &mut aggregate.r1,
            &mut aggregate.candidate_counts,
        )?;

        let right_end = node_end
            .saturating_add(CONCORDANCE_COLLOCATION_MAX_SPAN)
            .min(records.len());
        add_collocation_window_record_keys(
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
            &mut aggregate.r1,
            &mut aggregate.candidate_counts,
        )?;
    }

    Ok(aggregate)
}

pub fn aggregate_concordance_collocations_documents_routed(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
) -> Result<RoutedConcordanceCollocationAggregate, ConcordanceError> {
    match backend {
        ConcordanceBackend::Scan => Ok(RoutedConcordanceCollocationAggregate {
            aggregate: aggregate_concordance_collocations_documents(
                documents,
                query,
                tokenization_config,
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
                aggregate_concordance_collocations_index(store, query, tokenization_config)
            })();

            match index_result {
                Ok(aggregate) => Ok(RoutedConcordanceCollocationAggregate {
                    aggregate,
                    backend_used: ConcordanceBackendUsed::Index,
                    index_rebuilt,
                }),
                Err(ConcordanceError::Index(_)) => Ok(RoutedConcordanceCollocationAggregate {
                    aggregate: aggregate_concordance_collocations_documents(
                        documents,
                        query,
                        tokenization_config,
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
    let mut rows = aggregate
        .candidate_counts
        .iter()
        .filter_map(|(surface, counts)| {
            if query.hide_stopwords
                && stopword_matches(
                    surface,
                    StopwordLanguage::from_subtag(query.language.as_deref()),
                )
            {
                return None;
            }
            let collocate_frequency = *aggregate.collocate_frequencies.get(surface).unwrap_or(&0);
            let o11 = counts.sum(span, direction);
            if o11 == 0
                || o11 < query.min_cooccurrence
                || collocate_frequency < query.min_collocate_frequency
            {
                return None;
            }
            Some(collocation_row(
                surface.clone(),
                o11,
                collocate_frequency,
                r1,
                aggregate.n,
            ))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        compare_collocation_rows(
            left,
            right,
            query.sort_key,
            query.sort_measure,
            query.sort_direction,
        )
    });
    let total_candidates = rows.len();
    rows.truncate(limit);

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
) -> Result<ConcordanceCollocationResult, ConcordanceError> {
    let aggregate =
        aggregate_concordance_collocations_documents(documents, query, tokenization_config)?;
    Ok(query_concordance_collocations_from_aggregate(
        &aggregate, query,
    ))
}

#[cfg(test)]
#[path = "collocation_tests.rs"]
mod tests;
