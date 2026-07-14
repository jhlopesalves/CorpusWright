use super::*;
use crate::analysis::test_support::{
    assert_close, concordance_document, concordance_for, concordance_query,
};
use crate::text::tokenisation::TokenizationConfig;
use std::collections::HashMap;

fn compare_query(expressions: &[&str]) -> CompareExpressionsQuery {
    CompareExpressionsQuery {
        selection: AnalysisDocumentSelection::default(),
        expressions: expressions
            .iter()
            .map(|expression| expression.to_string())
            .collect(),
        case_sensitive: false,
        min_document_tokens: 0,
        offset: 0,
        limit: 100,
    }
}

fn compare_for(
    documents: &[ConcordanceDocument],
    query: CompareExpressionsQuery,
) -> Result<CompareExpressionsResult, CompareError> {
    compare_expressions(documents, &query, &TokenizationConfig::default())
}

fn assert_compare_agrees_with_concordance(
    documents: &[ConcordanceDocument],
    expressions: &[&str],
    case_sensitive: bool,
) -> CompareExpressionsResult {
    let mut query = compare_query(expressions);
    query.case_sensitive = case_sensitive;
    query.limit = 500;
    let compare = compare_for(documents, query).unwrap();

    for (expression_index, expression) in expressions.iter().enumerate() {
        let mut concordance_query = concordance_query(expression);
        concordance_query.case_sensitive = case_sensitive;
        concordance_query.limit = 500;
        concordance_query.distribution_limit = 500;
        let concordance = concordance_for(documents, concordance_query).unwrap();
        let summary = &compare.expressions[expression_index];
        assert_eq!(summary.total_hits, concordance.total_hits as u64);
        assert_eq!(summary.documents_with_hits, concordance.documents_with_hits);
        assert_eq!(summary.dp, concordance.dp);
        assert_eq!(summary.top_document_share, concordance.top_document_share);

        let concordance_counts = concordance
            .distribution
            .as_ref()
            .expect("distribution was requested")
            .items
            .iter()
            .map(|item| (item.document_id, item.count))
            .collect::<HashMap<_, _>>();
        for row in &compare.documents {
            assert_eq!(
                row.cells[expression_index].count,
                concordance_counts
                    .get(&row.document_id)
                    .copied()
                    .unwrap_or_default(),
                "{expression} document {}",
                row.document_id
            );
        }
    }

    compare
}

#[test]
fn single_word_expression_counts_by_document() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta alpha"),
        concordance_document(1, "b.txt", "beta gamma"),
        concordance_document(2, "c.txt", "alpha"),
    ];

    let result = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();

    assert_eq!(result.expressions_queried, ["alpha", "beta"]);
    assert_eq!(result.selected_documents_total, 3);
    assert_eq!(result.selected_tokens_total, 6);
    assert_eq!(result.documents.len(), 3);
    assert_eq!(
        result
            .documents
            .iter()
            .map(|row| {
                (
                    row.document_id,
                    row.cells[0].count,
                    row.cells[1].count,
                    row.included_in_rate,
                )
            })
            .collect::<Vec<_>>(),
        [(0, 2, 1, true), (1, 0, 1, true), (2, 1, 0, true)]
    );
    assert_eq!(result.expressions[0].total_hits, 3);
    assert_eq!(result.expressions[0].documents_with_hits, 2);
    assert_eq!(result.expressions[1].total_hits, 2);
    assert_eq!(result.expressions[1].documents_with_hits, 2);
}

#[test]
fn multi_word_phrase_counts_by_document() {
    let documents = vec![
        concordance_document(0, "a.txt", "very good very good"),
        concordance_document(1, "b.txt", "not very good"),
    ];

    let result = compare_for(&documents, compare_query(&["very good", "not very"])).unwrap();

    assert_eq!(
        result
            .documents
            .iter()
            .map(|row| (row.document_id, row.cells[0].count, row.cells[1].count))
            .collect::<Vec<_>>(),
        [(0, 2, 0), (1, 1, 1)]
    );
    assert_eq!(result.expressions[0].total_hits, 3);
    assert_eq!(result.expressions[1].total_hits, 1);
}

#[test]
fn case_sensitive_allows_case_only_contrasts() {
    let documents = vec![concordance_document(0, "movie.txt", "Movie movie MOVIE")];
    let mut query = compare_query(&["Movie", "movie"]);
    query.case_sensitive = true;

    let result = compare_for(&documents, query).unwrap();

    assert_eq!(result.documents[0].cells[0].count, 1);
    assert_eq!(result.documents[0].cells[1].count, 1);
}

#[test]
fn case_insensitive_folds_expected_equivalents() {
    let documents = vec![concordance_document(
        0,
        "movie.txt",
        "Movie movie MOVIE film",
    )];

    let result = compare_for(&documents, compare_query(&["movie", "film"])).unwrap();

    assert_eq!(result.documents[0].cells[0].count, 3);
    assert_eq!(result.documents[0].cells[1].count, 1);
}

#[test]
fn duplicate_expression_validation_rejects_exact_and_folded_duplicates() {
    let documents = vec![concordance_document(0, "movie.txt", "Movie movie")];

    let exact = compare_for(&documents, compare_query(&[" movie ", "movie"])).unwrap_err();
    assert_eq!(
        exact,
        CompareError::DuplicateExpression {
            expression: "movie".to_string()
        }
    );

    let folded = compare_for(&documents, compare_query(&["Movie", "movie"])).unwrap_err();
    assert_eq!(
        folded,
        CompareError::EquivalentExpression {
            expression: "movie".to_string()
        }
    );
}

#[test]
fn expression_count_validation_reports_user_input_errors() {
    let documents = vec![concordance_document(0, "a.txt", "alpha")];

    assert_eq!(
        compare_for(&documents, compare_query(&[])).unwrap_err(),
        CompareError::EmptyExpressions
    );
    assert_eq!(
        compare_for(&documents, compare_query(&["alpha"])).unwrap_err(),
        CompareError::TooFewExpressions
    );
    assert_eq!(
        compare_for(
            &documents,
            compare_query(&["a", "b", "c", "d", "e", "f", "g"])
        )
        .unwrap_err(),
        CompareError::TooManyExpressions
    );
    assert_eq!(
        compare_for(&documents, compare_query(&["alpha", "   "])).unwrap_err(),
        CompareError::BlankExpression
    );
}

#[test]
fn per_10k_and_per_million_rate_arithmetic_is_reported() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha alpha beta"),
        concordance_document(1, "b.txt", "beta beta"),
    ];

    let result = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();

    assert_close(result.expressions[0].rate_per_10k, 4_000.0);
    assert_close(result.expressions[0].rate_per_million, 400_000.0);
    assert_close(result.expressions[1].rate_per_10k, 6_000.0);
    assert_close(result.expressions[1].rate_per_million, 600_000.0);
    assert_close(
        result.documents[0].cells[0].rate_per_10k.unwrap(),
        2.0 / 3.0 * 10_000.0,
    );
    assert_close(
        result.documents[0].cells[1].rate_per_10k.unwrap(),
        1.0 / 3.0 * 10_000.0,
    );
    assert_close(result.documents[1].cells[0].rate_per_10k.unwrap(), 0.0);
    assert_close(result.documents[1].cells[1].rate_per_10k.unwrap(), 10_000.0);
}

#[test]
fn zero_token_documents_keep_rows_and_zero_corpus_rates() {
    let documents = vec![
        concordance_document(0, "empty.txt", ""),
        concordance_document(1, "punct.txt", "!!!"),
    ];

    let result = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();

    assert_eq!(result.selected_tokens_total, 0);
    for summary in &result.expressions {
        assert_eq!(summary.total_hits, 0);
        assert_close(summary.rate_per_10k, 0.0);
        assert_close(summary.rate_per_million, 0.0);
    }
    assert_eq!(result.documents.len(), 2);
    for row in &result.documents {
        assert_eq!(row.token_count, 0);
        assert!(!row.included_in_rate);
        assert!(row.cells.iter().all(|cell| cell.rate_per_10k.is_none()));
    }
}

#[test]
fn min_document_tokens_only_controls_document_rate_cells() {
    let documents = vec![
        concordance_document(0, "long.txt", "alpha alpha"),
        concordance_document(1, "short.txt", "beta"),
    ];
    let baseline = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();
    let mut threshold_query = compare_query(&["alpha", "beta"]);
    threshold_query.min_document_tokens = 2;

    let thresholded = compare_for(&documents, threshold_query).unwrap();

    assert_eq!(thresholded.expressions, baseline.expressions);
    let short_row = thresholded
        .documents
        .iter()
        .find(|row| row.document_id == 1)
        .unwrap();
    assert_eq!(short_row.cells[0].count, 0);
    assert_eq!(short_row.cells[1].count, 1);
    assert!(!short_row.included_in_rate);
    assert_eq!(short_row.cells[0].rate_per_10k, None);
    assert_eq!(short_row.cells[1].rate_per_10k, None);
}

#[test]
fn zero_hit_documents_are_retained_with_zero_rates_when_included() {
    let documents = vec![
        concordance_document(0, "hit.txt", "alpha beta"),
        concordance_document(1, "miss.txt", "gamma delta"),
    ];

    let result = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();

    let miss = &result.documents[1];
    assert_eq!(miss.document_id, 1);
    assert!(miss.included_in_rate);
    assert_eq!(miss.cells[0].count, 0);
    assert_eq!(miss.cells[1].count, 0);
    assert_eq!(miss.cells[0].rate_per_10k, Some(0.0));
    assert_eq!(miss.cells[1].rate_per_10k, Some(0.0));
}

#[test]
fn document_rows_are_ordered_by_document_id() {
    let documents = vec![
        concordance_document(2, "c.txt", "gamma"),
        concordance_document(0, "a.txt", "alpha"),
        concordance_document(1, "b.txt", "beta"),
    ];

    let result = compare_for(&documents, compare_query(&["alpha", "beta"])).unwrap();

    assert_eq!(
        result
            .documents
            .iter()
            .map(|row| row.document_id)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn pagination_slices_document_rows_without_changing_totals() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha"),
        concordance_document(1, "b.txt", "beta"),
        concordance_document(2, "c.txt", "gamma"),
    ];
    let mut query = compare_query(&["alpha", "beta"]);
    query.offset = 1;
    query.limit = 1;

    let result = compare_for(&documents, query).unwrap();

    assert_eq!(result.total_documents, 3);
    assert_eq!(result.selected_documents_total, 3);
    assert_eq!(result.offset, 1);
    assert_eq!(result.limit, 1);
    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.documents[0].document_id, 1);
}

#[test]
fn compare_agrees_with_concordance_for_surface_counts_and_summaries() {
    let documents = vec![
        concordance_document(
            0,
            "a.txt",
            "Movie, movie. good movie. I love it. i love it.",
        ),
        concordance_document(1, "b.txt", "A movie is not a Good movie. I love it."),
        concordance_document(2, "c.txt", "No matching phrase here."),
    ];

    assert_compare_agrees_with_concordance(&documents, &["movie", "good movie"], false);
    assert_compare_agrees_with_concordance(&documents, &["Movie", "Good movie"], true);
    assert_compare_agrees_with_concordance(&documents, &["I love it", "movie"], false);
    assert_compare_agrees_with_concordance(&documents, &["I love it", "i love it"], true);
}

#[test]
fn overlap_fixture_agrees_with_concordance() {
    let documents = vec![concordance_document(
        0,
        "overlap.txt",
        "very very very good",
    )];

    assert_compare_agrees_with_concordance(&documents, &["very very", "very good"], false);
}

#[test]
fn overlap_fixture_pins_concrete_count() {
    let documents = vec![concordance_document(
        0,
        "overlap.txt",
        "very very very good",
    )];

    let result = compare_for(&documents, compare_query(&["very very", "very good"])).unwrap();

    // This pins Compare overlap semantics to Concordance's existing matcher behaviour.
    assert_eq!(result.documents[0].cells[0].count, 2);
}
