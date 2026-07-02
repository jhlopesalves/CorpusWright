use super::distribution::selected_document_mask;
use super::shared::{
    ANNOTATED_DP_BROADLY_DISPERSED_MAX, ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN, ENGLISH_STOPWORDS,
    MAX_QUERY_WINDOW,
};
use super::{
    AnalysisCorpus, AnalysisDocument, AnalysisDocumentSelection, AnnotationCacheAvailability,
    AnnotationCacheState, compute_dispersion,
};
use crate::annotation::UniversalPos;
use regex::{Regex, RegexBuilder};
use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

pub(super) const ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE: &str =
    "Build the annotation cache to enable lemma and POS frequency.";

/// Match mode for frequency token queries.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencyMatchMode {
    #[default]
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
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum FrequencySortKey {
    Token,
    #[default]
    Frequency,
    Range,
    Rate,
    Dispersion,
}

/// Sort direction for frequency results.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    #[default]
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

/// Parameters for a bounded annotated frequency query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotatedFrequencyQuery {
    pub selection: AnalysisDocumentSelection,
    pub mode: AnnotatedFrequencyMode,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub match_mode: FrequencyMatchMode,
    #[serde(default)]
    #[ts(type = "number")]
    pub minimum_frequency: u64,
    #[serde(default)]
    pub hide_stopwords: bool,
    #[serde(default)]
    pub case_fold: bool,
    #[serde(default)]
    pub pos_filter: Option<String>,
    #[serde(default)]
    pub sort_key: FrequencySortKey,
    #[serde(default)]
    pub sort_direction: SortDirection,
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
    pub query_error: Option<String>,
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

pub(super) fn annotated_dispersion_label(dp: f64) -> &'static str {
    if dp < ANNOTATED_DP_BROADLY_DISPERSED_MAX {
        "broadly dispersed"
    } else if dp < ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN {
        "moderately concentrated"
    } else {
        "highly concentrated"
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FrequencyDistributionStats {
    pub(super) raw: u64,
    pub(super) per_10k: f64,
    pub(super) doc_freq: usize,
    pub(super) dp: f64,
    pub(super) top_doc_share: f64,
}

pub(super) fn frequency_distribution_stats(
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

pub(super) fn case_fold_surface(value: &str) -> String {
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

pub(super) struct ResolvedFrequencyGrouping {
    pub(super) group_by: FrequencyGroupMode,
    pub(super) case_fold: bool,
    pub(super) stemmer: Option<Stemmer>,
    pub(super) notices: Vec<FrequencyNotice>,
}

pub(super) fn resolve_frequency_grouping(
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

pub(super) fn selected_documents_have_annotation(
    corpus: &AnalysisCorpus,
    selected: &[bool],
) -> bool {
    corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
        .all(|document| document.annotation_available)
}

pub(super) fn frequency_group_key(surface: &str, grouping: &ResolvedFrequencyGrouping) -> String {
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

pub(super) fn annotated_frequency_group_key(
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

pub(super) fn frequency_query_key(query: &str, grouping: &ResolvedFrequencyGrouping) -> String {
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
    query_error: Option<String>,
    selected_document_count: usize,
    total_document_count: usize,
    total_tokens: u64,
) -> AnnotatedFrequencyResult {
    AnnotatedFrequencyResult {
        mode: query.mode,
        cache,
        unavailable_message,
        query_error,
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

enum AnnotatedTextMatch {
    Empty,
    Literal {
        needle: String,
        match_mode: FrequencyMatchMode,
        case_fold: bool,
    },
    Regex(Regex),
}

impl AnnotatedTextMatch {
    fn new(query: &str, match_mode: FrequencyMatchMode, case_fold: bool) -> Result<Self, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Self::Empty);
        }
        if match_mode == FrequencyMatchMode::Regex {
            return RegexBuilder::new(query)
                .case_insensitive(case_fold)
                .build()
                .map(Self::Regex)
                .map_err(|_| "Invalid regex pattern.".to_string());
        }
        Ok(Self::Literal {
            needle: if case_fold {
                case_fold_surface(query)
            } else {
                query.to_string()
            },
            match_mode,
            case_fold,
        })
    }

    fn matches(&self, value: &str) -> bool {
        match self {
            Self::Empty => true,
            Self::Literal {
                needle,
                match_mode,
                case_fold,
            } => {
                let value = if *case_fold {
                    case_fold_surface(value)
                } else {
                    value.to_string()
                };
                match match_mode {
                    FrequencyMatchMode::Contains => value.contains(needle),
                    FrequencyMatchMode::Exact => value == *needle,
                    FrequencyMatchMode::StartsWith => value.starts_with(needle),
                    FrequencyMatchMode::Regex => {
                        unreachable!("regex queries use the Regex variant")
                    }
                }
            }
            Self::Regex(regex) => regex.is_match(value),
        }
    }
}

fn compare_pos_frequency_rows(
    left: &PosFrequencyRow,
    right: &PosFrequencyRow,
    sort_key: FrequencySortKey,
) -> Ordering {
    match sort_key {
        FrequencySortKey::Token => left.upos.cmp(&right.upos),
        FrequencySortKey::Frequency => left.raw.cmp(&right.raw),
        FrequencySortKey::Range => left.doc_freq.cmp(&right.doc_freq),
        FrequencySortKey::Rate => left.per_10k.total_cmp(&right.per_10k),
        FrequencySortKey::Dispersion => left.dp.total_cmp(&right.dp),
    }
}

fn compare_pos_identity(left: &PosFrequencyRow, right: &PosFrequencyRow) -> Ordering {
    left.upos.cmp(&right.upos)
}

fn sort_pos_frequency_rows(
    rows: &mut [PosFrequencyRow],
    sort_key: FrequencySortKey,
    sort_direction: SortDirection,
) {
    rows.sort_by(|left, right| {
        let ordering = compare_pos_frequency_rows(left, right, sort_key);
        let ordering = if ordering == Ordering::Equal {
            compare_pos_identity(left, right)
        } else {
            ordering
        };
        match sort_direction {
            SortDirection::Asc => ordering,
            SortDirection::Desc if sort_key == FrequencySortKey::Token => ordering.reverse(),
            SortDirection::Desc => {
                let primary = compare_pos_frequency_rows(left, right, sort_key).reverse();
                if primary == Ordering::Equal {
                    compare_pos_identity(left, right)
                } else {
                    primary
                }
            }
        }
    });
}

fn compare_lex_identity(left: &LexFrequencyRow, right: &LexFrequencyRow) -> Ordering {
    left.lemma
        .cmp(&right.lemma)
        .then_with(|| left.upos.cmp(&right.upos))
}

fn compare_lex_frequency_rows(
    left: &LexFrequencyRow,
    right: &LexFrequencyRow,
    sort_key: FrequencySortKey,
) -> Ordering {
    match sort_key {
        FrequencySortKey::Token => compare_lex_identity(left, right),
        FrequencySortKey::Frequency => left.raw.cmp(&right.raw),
        FrequencySortKey::Range => left.doc_freq.cmp(&right.doc_freq),
        FrequencySortKey::Rate => left.per_10k.total_cmp(&right.per_10k),
        FrequencySortKey::Dispersion => left.dp.total_cmp(&right.dp),
    }
}

fn sort_lex_frequency_rows(
    rows: &mut [LexFrequencyRow],
    sort_key: FrequencySortKey,
    sort_direction: SortDirection,
) {
    rows.sort_by(|left, right| {
        let ordering = compare_lex_frequency_rows(left, right, sort_key);
        let ordering = if ordering == Ordering::Equal {
            compare_lex_identity(left, right)
        } else {
            ordering
        };
        match sort_direction {
            SortDirection::Asc => ordering,
            SortDirection::Desc if sort_key == FrequencySortKey::Token => ordering.reverse(),
            SortDirection::Desc => {
                let primary = compare_lex_frequency_rows(left, right, sort_key).reverse();
                if primary == Ordering::Equal {
                    compare_lex_identity(left, right)
                } else {
                    primary
                }
            }
        }
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

    groups
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
        .collect::<Vec<_>>()
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

    groups
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
        .collect::<Vec<_>>()
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
            None,
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
            None,
            selected_document_count,
            corpus.documents.len(),
            total_tokens,
        );
    }

    match query.mode {
        AnnotatedFrequencyMode::Pos => {
            let text_match = match AnnotatedTextMatch::new(&query.query, query.match_mode, true) {
                Ok(text_match) => text_match,
                Err(query_error) => {
                    return empty_annotated_frequency_result(
                        query,
                        cache,
                        None,
                        Some(query_error),
                        selected_document_count,
                        corpus.documents.len(),
                        total_tokens,
                    );
                }
            };
            let mut rows =
                pos_frequency_rows(corpus, &selected, selected_document_count, total_tokens);
            rows.retain(|row| text_match.matches(&row.upos));
            rows.retain(|row| row.raw >= query.minimum_frequency);
            sort_pos_frequency_rows(&mut rows, query.sort_key, query.sort_direction);
            let total_rows = rows.len();
            let offset = query.offset.min(total_rows);
            let pos_rows = rows.into_iter().skip(offset).take(limit).collect();
            AnnotatedFrequencyResult {
                mode: query.mode,
                cache,
                unavailable_message: None,
                query_error: None,
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
            let text_match =
                match AnnotatedTextMatch::new(&query.query, query.match_mode, query.case_fold) {
                    Ok(text_match) => text_match,
                    Err(query_error) => {
                        return empty_annotated_frequency_result(
                            query,
                            cache,
                            None,
                            Some(query_error),
                            selected_document_count,
                            corpus.documents.len(),
                            total_tokens,
                        );
                    }
                };
            let pos_filter = query
                .pos_filter
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let mut rows =
                lemma_pos_frequency_rows(corpus, &selected, selected_document_count, total_tokens);
            rows.retain(|row| text_match.matches(&row.lemma));
            if let Some(pos_filter) = pos_filter {
                rows.retain(|row| row.upos.eq_ignore_ascii_case(pos_filter));
            }
            if query.hide_stopwords {
                rows.retain(|row| {
                    !ENGLISH_STOPWORDS.contains(case_fold_surface(&row.lemma).as_str())
                });
            }
            rows.retain(|row| row.raw >= query.minimum_frequency);
            sort_lex_frequency_rows(&mut rows, query.sort_key, query.sort_direction);
            let total_rows = rows.len();
            let offset = query.offset.min(total_rows);
            let lex_rows = rows.into_iter().skip(offset).take(limit).collect();
            AnnotatedFrequencyResult {
                mode: query.mode,
                cache,
                unavailable_message: None,
                query_error: None,
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
