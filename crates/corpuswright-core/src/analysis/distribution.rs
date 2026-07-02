use super::build::{AnalysisCorpus, AnalysisDocumentSelection};
use super::frequency::{
    FrequencyGroupMode, FrequencyMatchMode, FrequencyQuery, FrequencySortKey, FrequencyTokenClass,
    SortDirection, annotated_frequency_group_key, frequency_group_key, frequency_query_key,
    resolve_frequency_grouping, selected_documents_have_annotation,
};
use super::shared::MAX_QUERY_WINDOW;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TermDistributionItem {
    pub document_id: usize,
    pub source_path: String,
    pub relative_path: String,
    #[ts(type = "number")]
    pub token_count: u64,
    #[ts(type = "number")]
    pub count: u64,
    pub rate_per_10k: f64,
    pub rate_per_million: f64,
}

/// Bounded per-document term distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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

/// Typed selector for an annotated term distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "mode", rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotatedTermSelector {
    Pos { upos: String },
    LemmaPos { lemma: String, upos: String },
}

impl AnnotatedTermSelector {
    fn label(&self) -> String {
        match self {
            Self::Pos { upos } => upos.trim().to_string(),
            Self::LemmaPos { lemma, upos } => format!("{} / {}", lemma, upos.trim()),
        }
    }
}

/// Parameters for one bounded annotated per-document distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotatedTermDistributionQuery {
    pub selection: AnalysisDocumentSelection,
    pub selector: AnnotatedTermSelector,
    pub offset: usize,
    pub limit: usize,
}

pub(super) fn selected_document_mask(
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

/// Returns one bounded per-document distribution window for an annotated term.
pub fn query_annotated_term_distribution(
    corpus: &AnalysisCorpus,
    query: &AnnotatedTermDistributionQuery,
) -> TermDistributionPage {
    let (selected, selected_count) = selected_document_mask(corpus, &query.selection);
    let mut counts = HashMap::<usize, u64>::new();

    for document in corpus
        .documents
        .iter()
        .filter(|document| selected[document.summary.id])
    {
        for annotated_count in &document.annotated_term_counts {
            let matches = match &query.selector {
                AnnotatedTermSelector::Pos { upos } => annotated_count.upos.matches_filter(upos),
                AnnotatedTermSelector::LemmaPos { lemma, upos } => {
                    annotated_count.upos.matches_filter(upos)
                        && corpus
                            .lemma_table
                            .lookup(annotated_count.lemma_id)
                            .is_some_and(|candidate| candidate == lemma)
                }
            };
            if matches {
                *counts.entry(document.summary.id).or_default() += annotated_count.count;
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
        token: query.selector.label(),
        total_documents: selected_count,
        offset,
        limit,
        items,
        max_rate_per_10k,
        max_rate_per_million,
    }
}

#[cfg(test)]
#[path = "distribution_tests.rs"]
mod tests;
