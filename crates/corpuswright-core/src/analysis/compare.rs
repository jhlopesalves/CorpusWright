use super::build::AnalysisDocumentSelection;
use super::concordance::{
    ConcordanceDocument, analysis_word_tokens, compute_dispersion_from_token_counts,
    count_concordance_token_sequence_matches, resolve_concordance_query_tokens,
    selected_concordance_document_ids,
};
use super::frequency::annotated_dispersion_label;
use super::shared::MAX_QUERY_WINDOW;
use crate::text::tokenisation::TokenizationConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use ts_rs::TS;

const COMPARE_MIN_EXPRESSIONS: usize = 2;
const COMPARE_MAX_EXPRESSIONS: usize = 6;
const COMPARE_DEFAULT_LIMIT: usize = 10;

/// Parameters for comparing surface expression counts across documents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompareExpressionsQuery {
    #[serde(default)]
    pub selection: AnalysisDocumentSelection,
    pub expressions: Vec<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    #[ts(type = "number")]
    pub min_document_tokens: u64,
    pub offset: usize,
    pub limit: usize,
}

/// Bounded corpus-fact response for a compare-expressions query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompareExpressionsResult {
    pub expressions_queried: Vec<String>,
    pub case_sensitive: bool,
    #[ts(type = "number")]
    pub min_document_tokens: u64,
    pub selected_documents_total: usize,
    #[ts(type = "number")]
    pub selected_tokens_total: u64,
    pub expressions: Vec<CompareExpressionSummary>,
    pub total_documents: usize,
    pub offset: usize,
    pub limit: usize,
    pub documents: Vec<CompareDocumentRow>,
}

/// Corpus-level summary for one compared expression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompareExpressionSummary {
    pub expression: String,
    #[ts(type = "number")]
    pub total_hits: u64,
    pub documents_with_hits: usize,
    pub rate_per_10k: f64,
    pub rate_per_million: f64,
    pub dp: f64,
    pub dispersion_label: String,
    pub top_document_share: f64,
}

/// One document row in the compare-expressions response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompareDocumentRow {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    #[ts(type = "number")]
    pub token_count: u64,
    pub included_in_rate: bool,
    pub cells: Vec<CompareDocumentCell>,
}

/// One document/expression count cell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompareDocumentCell {
    #[ts(type = "number")]
    pub count: u64,
    pub rate_per_10k: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompareError {
    EmptyExpressions,
    TooFewExpressions,
    TooManyExpressions,
    BlankExpression,
    InvalidExpression { expression: String },
    DuplicateExpression { expression: String },
    EquivalentExpression { expression: String },
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyExpressions | Self::TooFewExpressions => {
                formatter.write_str("Enter at least two expressions to compare.")
            }
            Self::TooManyExpressions => {
                formatter.write_str("Compare up to six expressions at a time.")
            }
            Self::BlankExpression => {
                formatter.write_str("Expressions must contain at least one word.")
            }
            Self::InvalidExpression { expression } => write!(
                formatter,
                "Expression \"{expression}\" must contain at least one word."
            ),
            Self::DuplicateExpression { expression } => {
                write!(formatter, "Remove duplicate expression \"{expression}\".")
            }
            Self::EquivalentExpression { expression } => write!(
                formatter,
                "Expression \"{expression}\" duplicates another expression after case-insensitive token matching."
            ),
        }
    }
}

impl std::error::Error for CompareError {}

struct ResolvedCompareExpression {
    expression: String,
    query_keys: Vec<String>,
}

fn compare_limit(limit: usize) -> usize {
    if limit == 0 {
        COMPARE_DEFAULT_LIMIT
    } else {
        limit.clamp(1, MAX_QUERY_WINDOW)
    }
}

fn resolve_compare_expressions(
    query: &CompareExpressionsQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<Vec<ResolvedCompareExpression>, CompareError> {
    match query.expressions.len() {
        0 => return Err(CompareError::EmptyExpressions),
        length if length < COMPARE_MIN_EXPRESSIONS => {
            return Err(CompareError::TooFewExpressions);
        }
        length if length > COMPARE_MAX_EXPRESSIONS => {
            return Err(CompareError::TooManyExpressions);
        }
        _ => {}
    }

    let mut exact_expressions = HashSet::<String>::new();
    let mut trimmed_expressions = Vec::with_capacity(query.expressions.len());
    for expression in &query.expressions {
        let trimmed = expression.trim();
        if trimmed.is_empty() {
            return Err(CompareError::BlankExpression);
        }
        if !exact_expressions.insert(trimmed.to_string()) {
            return Err(CompareError::DuplicateExpression {
                expression: trimmed.to_string(),
            });
        }
        trimmed_expressions.push(trimmed.to_string());
    }

    let mut resolved = Vec::with_capacity(trimmed_expressions.len());
    let mut folded_keys = HashSet::<Vec<String>>::new();
    for expression in trimmed_expressions {
        let query_tokens = resolve_concordance_query_tokens(&expression, tokenization_config)
            .map_err(|_| CompareError::InvalidExpression {
                expression: expression.clone(),
            })?;
        let query_keys = query_tokens
            .iter()
            .map(|token| super::concordance_key(token, query.case_sensitive))
            .collect::<Vec<_>>();
        if !query.case_sensitive && !folded_keys.insert(query_keys.clone()) {
            return Err(CompareError::EquivalentExpression { expression });
        }
        resolved.push(ResolvedCompareExpression {
            expression,
            query_keys,
        });
    }

    Ok(resolved)
}

/// Counts multiple surface expressions per document using Concordance matching.
pub fn compare_expressions(
    documents: &[ConcordanceDocument],
    query: &CompareExpressionsQuery,
    tokenization_config: &TokenizationConfig,
) -> Result<CompareExpressionsResult, CompareError> {
    let resolved_expressions = resolve_compare_expressions(query, tokenization_config)?;
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);
    let limit = compare_limit(query.limit);

    let mut document_token_counts = HashMap::<usize, u64>::new();
    let mut expression_document_counts =
        vec![Vec::<(usize, u64)>::new(); resolved_expressions.len()];
    let mut expression_total_hits = vec![0u64; resolved_expressions.len()];
    let mut rows = Vec::with_capacity(selected_document_ids.len());
    let mut selected_tokens_total = 0u64;

    for document in documents
        .iter()
        .filter(|document| selected_document_ids.contains(&document.document_index))
    {
        let tokens = analysis_word_tokens(
            &document.text,
            document.document_index as u32,
            tokenization_config,
        );
        let token_count = tokens.len() as u64;
        selected_tokens_total += token_count;
        document_token_counts.insert(document.document_index, token_count);
        let included_in_rate = token_count > 0 && token_count >= query.min_document_tokens;

        let mut cells = Vec::with_capacity(resolved_expressions.len());
        for (expression_index, expression) in resolved_expressions.iter().enumerate() {
            let count = count_concordance_token_sequence_matches(
                &tokens,
                &expression.query_keys,
                query.case_sensitive,
            );
            expression_total_hits[expression_index] += count;
            if count > 0 {
                expression_document_counts[expression_index].push((document.document_index, count));
            }
            cells.push(CompareDocumentCell {
                count,
                rate_per_10k: included_in_rate
                    .then(|| count as f64 / token_count as f64 * 10_000.0),
            });
        }

        rows.push(CompareDocumentRow {
            document_id: document.document_index,
            source_path: document.source_path.clone(),
            display_name: document.display_name.clone(),
            token_count,
            included_in_rate,
            cells,
        });
    }

    rows.sort_unstable_by_key(|row| row.document_id);
    let total_documents = rows.len();
    let offset = query.offset.min(total_documents);
    let documents = rows.into_iter().skip(offset).take(limit).collect();

    let expressions = resolved_expressions
        .iter()
        .enumerate()
        .map(|(expression_index, expression)| {
            let total_hits = expression_total_hits[expression_index];
            let document_counts = &expression_document_counts[expression_index];
            let dp = compute_dispersion_from_token_counts(
                document_counts,
                total_hits,
                &document_token_counts,
                selected_tokens_total,
            );
            CompareExpressionSummary {
                expression: expression.expression.clone(),
                total_hits,
                documents_with_hits: document_counts.len(),
                rate_per_10k: if selected_tokens_total > 0 {
                    total_hits as f64 / selected_tokens_total as f64 * 10_000.0
                } else {
                    0.0
                },
                rate_per_million: if selected_tokens_total > 0 {
                    total_hits as f64 / selected_tokens_total as f64 * 1_000_000.0
                } else {
                    0.0
                },
                dp,
                dispersion_label: annotated_dispersion_label(dp).to_string(),
                top_document_share: if total_hits > 0 {
                    document_counts
                        .iter()
                        .map(|(_, count)| *count)
                        .max()
                        .unwrap_or_default() as f64
                        / total_hits as f64
                } else {
                    0.0
                },
            }
        })
        .collect::<Vec<_>>();

    Ok(CompareExpressionsResult {
        expressions_queried: resolved_expressions
            .iter()
            .map(|expression| expression.expression.clone())
            .collect(),
        case_sensitive: query.case_sensitive,
        min_document_tokens: query.min_document_tokens,
        selected_documents_total: selected_document_ids.len(),
        selected_tokens_total,
        expressions,
        total_documents,
        offset,
        limit,
        documents,
    })
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
