use super::*;
use crate::analysis::test_support::concordance_document;
use crate::tokenization::TokenizationConfig;

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
