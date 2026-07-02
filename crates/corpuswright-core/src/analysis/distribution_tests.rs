use super::*;
use crate::analysis::test_support::*;
use crate::annotation::UniversalPos;

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
        &TermDistributionQuery {
            selection: AnalysisDocumentSelection::default(),
            group_by: FrequencyGroupMode::Surface,
            case_fold: false,
            language: None,
            offset: 0,
            limit: 1,
        },
    );
    assert_eq!(distribution.total_documents, 2);
    assert_eq!(distribution.items.len(), 1);
    assert_eq!(distribution.items[0].count, 1);
}

#[test]
fn annotated_term_distribution_counts_pos_and_lemma_pos_per_document() {
    let mut corpus = corpus_for(&[("a.txt", "Cats run run"), ("b.txt", "cat run")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 2),
        ],
    );
    add_document_annotations(
        &mut corpus,
        1,
        &[
            ("cat", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 1),
        ],
    );

    let pos_distribution = query_annotated_term_distribution(
        &corpus,
        &AnnotatedTermDistributionQuery {
            selection: AnalysisDocumentSelection::default(),
            selector: AnnotatedTermSelector::Pos {
                upos: "VERB".to_string(),
            },
            offset: 0,
            limit: 10,
        },
    );
    let lemma_distribution = query_annotated_term_distribution(
        &corpus,
        &AnnotatedTermDistributionQuery {
            selection: AnalysisDocumentSelection::default(),
            selector: AnnotatedTermSelector::LemmaPos {
                lemma: "cat".to_string(),
                upos: "NOUN".to_string(),
            },
            offset: 0,
            limit: 10,
        },
    );

    assert_eq!(pos_distribution.total_documents, 2);
    assert_eq!(
        pos_distribution
            .items
            .iter()
            .map(|item| (item.relative_path.as_str(), item.count))
            .collect::<Vec<_>>(),
        vec![("a.txt", 2), ("b.txt", 1)]
    );
    assert_close(pos_distribution.items[0].rate_per_10k, 20_000.0 / 3.0);
    assert_eq!(
        lemma_distribution
            .items
            .iter()
            .map(|item| (item.relative_path.as_str(), item.count))
            .collect::<Vec<_>>(),
        vec![("a.txt", 1), ("b.txt", 1)]
    );
}
