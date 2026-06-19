//! Rust-owned corpus analysis and bounded query APIs.

use crate::scan::{DocumentRecord, DocumentType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use unicode_normalization::UnicodeNormalization;

const DOCUMENT_LENGTH_WINDOW: usize = 100;
const FREQUENT_TERM_THRESHOLD: u64 = 10;
const HIGH_DISPERSION_THRESHOLD: f64 = 0.5;
const MAX_QUERY_WINDOW: usize = 500;

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
    Extracted,
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

/// Corpus balance metrics computed during the Rust build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBalanceSummary {
    pub largest_document: Option<AnalysisDocumentSummary>,
    pub smallest_non_empty_document: Option<AnalysisDocumentSummary>,
    pub min_tokens: Option<f64>,
    pub max_tokens: Option<f64>,
    pub median_tokens: Option<f64>,
    pub q1_tokens: Option<f64>,
    pub q3_tokens: Option<f64>,
    pub iqr_tokens: Option<f64>,
    pub largest_document_share: Option<f64>,
    pub top_document_count: usize,
    pub top_document_share: Option<f64>,
    pub length_ratio: Option<f64>,
    pub effective_document_count: Option<f64>,
}

/// Lexical and extraction-quality metrics computed during the Rust build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisLexicalSummary {
    pub unique_types: usize,
    pub hapax_legomena: usize,
    pub dis_legomena: usize,
    pub repeated_types: usize,
    pub numeric_only_types: usize,
    pub numeric_only_share: Option<f64>,
    pub single_character_types: usize,
    pub single_character_share: Option<f64>,
    pub quality_other_types: usize,
    pub type_token_ratio: Option<f64>,
}

/// Term-dispersion metrics computed during the Rust build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDispersionSummary {
    pub document_count: usize,
    pub all_document_types: usize,
    pub one_document_types: usize,
    pub frequent_types: usize,
    pub median_frequent_dispersion: Option<f64>,
    pub high_dispersion_frequent_types: usize,
}

/// Compact overview returned after a corpus build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOverview {
    pub document_count: usize,
    pub source_count: usize,
    pub total_tokens: u64,
    pub total_types: usize,
    pub input_text_source: AnalysisInputSource,
    pub built_at: String,
    pub build_signature: String,
    pub skipped_documents: usize,
    pub skipped_pdf_documents: usize,
    pub balance: AnalysisBalanceSummary,
    pub lexical: AnalysisLexicalSummary,
    pub dispersion: AnalysisDispersionSummary,
    pub document_lengths: Vec<AnalysisDocumentSummary>,
    pub document_lengths_truncated: bool,
}

#[derive(Debug, Clone)]
struct AnalysisDocument {
    summary: AnalysisDocumentSummary,
    term_counts: Vec<(usize, u64)>,
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
    built_at: String,
    build_signature: String,
    skipped_documents: usize,
    skipped_pdf_documents: usize,
}

/// Sparse Rust-owned analysis index.
#[derive(Debug, Clone)]
pub struct AnalysisCorpus {
    documents: Vec<AnalysisDocument>,
    terms: Vec<AnalysisTerm>,
    term_index: HashMap<String, usize>,
    overview: AnalysisOverview,
}

impl AnalysisCorpus {
    /// Returns the compact summary computed with the index.
    pub fn overview(&self) -> &AnalysisOverview {
        &self.overview
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

fn build_overview(
    documents: &[AnalysisDocument],
    terms: &[AnalysisTerm],
    metadata: AnalysisBuildMetadata,
) -> AnalysisOverview {
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
    let non_empty_documents = sorted_documents
        .iter()
        .filter(|document| document.token_count > 0)
        .collect::<Vec<_>>();
    let largest_document = non_empty_documents
        .first()
        .map(|document| (*document).clone());
    let smallest_non_empty_document = non_empty_documents
        .last()
        .map(|document| (*document).clone());
    let mut token_counts = documents
        .iter()
        .map(|document| document.summary.token_count)
        .collect::<Vec<_>>();
    token_counts.sort_unstable();
    let min_tokens = token_counts.first().map(|value| *value as f64);
    let max_tokens = token_counts.last().map(|value| *value as f64);
    let median_tokens = percentile(&token_counts, 0.5);
    let q1_tokens = percentile(&token_counts, 0.25);
    let q3_tokens = percentile(&token_counts, 0.75);
    let iqr_tokens = q1_tokens.zip(q3_tokens).map(|(q1, q3)| q3 - q1);
    let largest_document_share = largest_document
        .as_ref()
        .filter(|_| total_tokens > 0)
        .map(|document| document.token_count as f64 / total_tokens as f64);
    let top_document_count = sorted_documents.len().min(3);
    let top_document_share = (total_tokens > 0 && top_document_count > 0).then(|| {
        sorted_documents
            .iter()
            .take(top_document_count)
            .map(|document| document.token_count)
            .sum::<u64>() as f64
            / total_tokens as f64
    });
    let length_ratio = largest_document
        .as_ref()
        .zip(smallest_non_empty_document.as_ref())
        .filter(|(_, smallest)| smallest.token_count > 0)
        .map(|(largest, smallest)| largest.token_count as f64 / smallest.token_count as f64);
    let hhi = if total_tokens > 0 {
        documents
            .iter()
            .map(|document| {
                let share = document.summary.token_count as f64 / total_tokens as f64;
                share * share
            })
            .sum::<f64>()
    } else {
        0.0
    };
    let effective_document_count = (hhi > 0.0).then(|| 1.0 / hhi);

    let mut hapax_legomena = 0usize;
    let mut dis_legomena = 0usize;
    let mut numeric_only_types = 0usize;
    let mut single_character_types = 0usize;
    let mut quality_other_types = 0usize;
    for term in terms {
        match term.frequency {
            1 => hapax_legomena += 1,
            2 => dis_legomena += 1,
            _ => {}
        }
        let has_letter = term.token.chars().any(char::is_alphabetic);
        let has_number = term.token.chars().any(char::is_numeric);
        if term.token.chars().count() <= 1 {
            single_character_types += 1;
        } else if has_number && !has_letter {
            numeric_only_types += 1;
        } else {
            quality_other_types += 1;
        }
    }
    let total_types = terms.len();
    let type_share = |count: usize| (total_types > 0).then(|| count as f64 / total_types as f64);
    let repeated_types = total_types.saturating_sub(hapax_legomena + dis_legomena);

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
    let all_document_types = if documents.is_empty() {
        0
    } else {
        terms
            .iter()
            .filter(|term| term.document_frequency == documents.len())
            .count()
    };

    AnalysisOverview {
        document_count: documents.len(),
        source_count: metadata.source_count,
        total_tokens,
        total_types,
        input_text_source: metadata.input_text_source,
        built_at: metadata.built_at,
        build_signature: metadata.build_signature,
        skipped_documents: metadata.skipped_documents,
        skipped_pdf_documents: metadata.skipped_pdf_documents,
        balance: AnalysisBalanceSummary {
            largest_document,
            smallest_non_empty_document,
            min_tokens,
            max_tokens,
            median_tokens,
            q1_tokens,
            q3_tokens,
            iqr_tokens,
            largest_document_share,
            top_document_count,
            top_document_share,
            length_ratio,
            effective_document_count,
        },
        lexical: AnalysisLexicalSummary {
            unique_types: total_types,
            hapax_legomena,
            dis_legomena,
            repeated_types,
            numeric_only_types,
            numeric_only_share: type_share(numeric_only_types),
            single_character_types,
            single_character_share: type_share(single_character_types),
            quality_other_types,
            type_token_ratio: (total_tokens > 0).then(|| total_types as f64 / total_tokens as f64),
        },
        dispersion: AnalysisDispersionSummary {
            document_count: documents.len(),
            all_document_types,
            one_document_types: terms
                .iter()
                .filter(|term| term.document_frequency == 1)
                .count(),
            frequent_types: frequent_terms.len(),
            median_frequent_dispersion: median_f64(&mut frequent_dispersions),
            high_dispersion_frequent_types: if documents.len() >= 2 {
                frequent_terms
                    .iter()
                    .filter(|term| term.dispersion >= HIGH_DISPERSION_THRESHOLD)
                    .count()
            } else {
                0
            },
        },
        document_lengths: sorted_documents
            .iter()
            .take(DOCUMENT_LENGTH_WINDOW)
            .cloned()
            .collect(),
        document_lengths_truncated: sorted_documents.len() > DOCUMENT_LENGTH_WINDOW,
    }
}

/// Builds a sparse analysis index while reading one document at a time.
pub fn build_analysis_corpus<TextLoader, Progress>(
    records: &[DocumentRecord],
    input_text_source: AnalysisInputSource,
    built_at: String,
    build_signature: String,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    mut report_progress: Progress,
) -> Result<AnalysisCorpus, AnalysisBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<Option<String>, String>,
    Progress: FnMut(AnalysisBuildProgress),
{
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
    let mut source_paths = HashSet::new();

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
        let text = load_text(record).map_err(AnalysisBuildError::Message)?;
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
        source_paths.insert(record.source_path.to_string_lossy().to_string());
        documents.push(AnalysisDocument {
            summary: document_summary(record, document_id, token_count),
            term_counts: sparse_counts,
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
            source_count: source_paths.len(),
            total_tokens,
            input_text_source,
            built_at,
            build_signature,
            skipped_documents,
            skipped_pdf_documents,
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

    fn corpus_for(texts: &[(&str, &str)]) -> AnalysisCorpus {
        let records = texts
            .iter()
            .map(|(name, _)| record(name))
            .collect::<Vec<_>>();
        let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
        build_analysis_corpus(
            &records,
            AnalysisInputSource::Extracted,
            "2026-01-01T00:00:00Z".to_string(),
            "signature".to_string(),
            &AtomicBool::new(false),
            |record| {
                Ok(text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .map(|text| (*text).to_string()))
            },
            |_| {},
        )
        .unwrap()
    }

    #[test]
    fn builds_overview_and_sparse_frequency_index() {
        let corpus = corpus_for(&[("one.txt", "Cats cats dog"), ("two.txt", "dog bird")]);
        let overview = corpus.overview();

        assert_eq!(overview.document_count, 2);
        assert_eq!(overview.total_tokens, 5);
        assert_eq!(overview.total_types, 3);
        assert_eq!(overview.lexical.hapax_legomena, 1);
        assert_eq!(overview.lexical.dis_legomena, 2);

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
        assert_eq!(
            corpus.overview().dispersion.median_frequent_dispersion,
            None
        );
    }

    #[test]
    fn builds_many_tiny_documents_without_a_large_fixture() {
        let records = (0..1_000)
            .map(|index| record(&format!("{index}.txt")))
            .collect::<Vec<_>>();
        let corpus = build_analysis_corpus(
            &records,
            AnalysisInputSource::Extracted,
            "2026-01-01T00:00:00Z".to_string(),
            "signature".to_string(),
            &AtomicBool::new(false),
            |_| Ok(Some("shared unique".to_string())),
            |_| {},
        )
        .unwrap();

        assert_eq!(corpus.overview().document_count, 1_000);
        assert_eq!(corpus.overview().total_tokens, 2_000);
        assert_eq!(corpus.overview().total_types, 2);
        assert!(corpus.overview().document_lengths_truncated);
        assert_eq!(
            corpus.overview().document_lengths.len(),
            DOCUMENT_LENGTH_WINDOW
        );
    }

    #[test]
    fn build_can_be_cancelled_between_documents() {
        let records = vec![record("one.txt"), record("two.txt")];
        let cancel = AtomicBool::new(false);
        let result = build_analysis_corpus(
            &records,
            AnalysisInputSource::Extracted,
            "2026-01-01T00:00:00Z".to_string(),
            "signature".to_string(),
            &cancel,
            |_| Ok(Some("one two".to_string())),
            |progress| {
                if progress.files_done == 1 {
                    cancel.store(true, AtomicOrdering::Relaxed);
                }
            },
        );

        assert_eq!(result.unwrap_err(), AnalysisBuildError::Cancelled);
    }
}
