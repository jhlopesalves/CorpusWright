use super::*;
use crate::analysis::test_support::*;

#[test]
fn surface_collocations_compute_window_counts_and_scores() {
    let documents = vec![
        concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
        concordance_document(1, "b.txt", "yellow target blue green"),
    ];

    let result = collocations_for(&documents, collocation_query("target")).unwrap();
    let blue = collocate(&result, "blue");

    assert_eq!(result.f_node, 2);
    assert_eq!(result.r1, 7);
    assert_eq!(result.n, 11);
    assert_eq!(blue.o11, 2);
    assert_eq!(blue.o12, 5);
    assert_eq!(blue.o21, 1);
    assert_eq!(blue.o22, 3);
    assert_eq!(blue.collocate_frequency, 3);
    // Hand values for blue: E11 = 7 * 3 / 11 = 1.9091; O12/O21/O22 = 5/1/3.
    assert_close_4dp(blue.mi, 0.0671141959);
    assert_close_4dp(blue.mi3, 2.0671141959);
    assert_close_4dp(blue.t_score, 0.0642824347);
    assert_close_4dp(blue.z_score, 0.0657951695);
    assert_close_4dp(blue.g2, 0.0165022055);
    assert_close_4dp(blue.log_dice, 12.6780719051);
    assert_close_4dp(blue.log_ratio, 0.1926450779);
}

#[test]
fn surface_collocation_floors_exclude_below_threshold_candidates() {
    let documents = vec![
        concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
        concordance_document(1, "b.txt", "yellow target blue green"),
    ];
    let mut query = collocation_query("target");
    query.min_cooccurrence = 2;
    query.min_collocate_frequency = 2;

    let result = collocations_for(&documents, query).unwrap();

    let surfaces = result
        .rows
        .iter()
        .map(|row| row.surface.as_str())
        .collect::<Vec<_>>();
    assert_eq!(surfaces.len(), 3);
    assert!(surfaces.contains(&"alpha"));
    assert!(surfaces.contains(&"blue"));
    assert!(surfaces.contains(&"green"));
    assert!(!surfaces.contains(&"yellow"));
}

#[test]
fn surface_collocation_direction_changes_window_counts() {
    let documents = vec![
        concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
        concordance_document(1, "b.txt", "yellow target blue green"),
    ];
    let mut left_query = collocation_query("target");
    left_query.direction = ConcordanceCollocationDirection::Left;
    let mut right_query = collocation_query("target");
    right_query.direction = ConcordanceCollocationDirection::Right;
    let both_query = collocation_query("target");

    let left = collocations_for(&documents, left_query).unwrap();
    let right = collocations_for(&documents, right_query).unwrap();
    let both = collocations_for(&documents, both_query).unwrap();

    assert_eq!(left.r1, 3);
    assert_eq!(right.r1, 4);
    assert_eq!(both.r1, 7);
    assert_eq!(collocate(&left, "blue").o11, 1);
    assert_eq!(collocate(&right, "blue").o11, 1);
    assert_eq!(collocate(&both, "blue").o11, 2);
}

#[test]
fn surface_collocations_do_not_return_node_tokens_as_collocates() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "target target alpha target",
    )];
    let mut query = collocation_query("target");
    query.span = 1;

    let result = collocations_for(&documents, query).unwrap();

    assert_eq!(result.f_node, 3);
    assert!(result.rows.iter().all(|row| row.surface != "target"));
    assert_eq!(collocate(&result, "alpha").o11, 2);
}

#[test]
fn surface_collocation_phrase_windows_wrap_the_whole_node_span() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "pre alpha beta post mid alpha beta tail",
    )];
    let mut query = collocation_query("alpha beta");
    query.span = 1;

    let result = collocations_for(&documents, query).unwrap();

    assert_eq!(result.f_node, 2);
    assert_eq!(result.r1, 4);
    assert_eq!(collocate(&result, "pre").o11, 1);
    assert_eq!(collocate(&result, "post").o11, 1);
    assert_eq!(collocate(&result, "mid").o11, 1);
    assert_eq!(collocate(&result, "tail").o11, 1);
    assert!(result.rows.iter().all(|row| row.surface != "alpha"));
    assert!(result.rows.iter().all(|row| row.surface != "beta"));
}

#[test]
fn surface_collocation_sort_key_ranks_before_limit() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "rare target common target common target common common common common common common common common common common",
    )];
    let mut association_query = collocation_query("target");
    association_query.direction = ConcordanceCollocationDirection::Left;
    association_query.limit = 1;

    let mut near_query = association_query.clone();
    near_query.sort_key = ConcordanceCollocationSortKey::NearQuery;

    let mut corpus_frequency_query = association_query.clone();
    corpus_frequency_query.sort_key = ConcordanceCollocationSortKey::CorpusFrequency;

    let association = collocations_for(&documents, association_query).unwrap();
    let near = collocations_for(&documents, near_query).unwrap();
    let corpus_frequency = collocations_for(&documents, corpus_frequency_query).unwrap();

    assert_eq!(association.rows.len(), 1);
    assert_eq!(near.rows.len(), 1);
    assert_eq!(corpus_frequency.rows.len(), 1);
    assert_eq!(association.rows[0].surface, "rare");
    assert_eq!(near.rows[0].surface, "common");
    assert_eq!(corpus_frequency.rows[0].surface, "common");
}

#[test]
fn surface_collocation_aggregate_post_processing_matches_full_query() {
    let documents = vec![
        concordance_document(0, "a.txt", "red alpha blue target green alpha blue"),
        concordance_document(1, "b.txt", "yellow target blue green"),
    ];
    let query = collocation_query("target");

    let one_shot = collocations_for(&documents, query.clone()).unwrap();
    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();
    let from_aggregate = query_concordance_collocations_from_aggregate(&aggregate, &query);

    assert_eq!(from_aggregate.query, one_shot.query);
    assert_eq!(from_aggregate.r1, one_shot.r1);
    assert_eq!(from_aggregate.n, one_shot.n);
    assert_eq!(from_aggregate.f_node, one_shot.f_node);
    assert_eq!(from_aggregate.total_candidates, one_shot.total_candidates);
    assert_eq!(from_aggregate.rows, one_shot.rows);
}

#[test]
fn surface_collocation_presentation_changes_reuse_full_aggregate() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "rare target common target common target common common common common common common common common common common",
    )];
    let mut query = collocation_query("target");
    query.direction = ConcordanceCollocationDirection::Left;
    query.limit = 1;
    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();

    let association = query_concordance_collocations_from_aggregate(&aggregate, &query);
    let mut near_query = query.clone();
    near_query.sort_key = ConcordanceCollocationSortKey::NearQuery;
    let near = query_concordance_collocations_from_aggregate(&aggregate, &near_query);
    let mut corpus_frequency_query = query.clone();
    corpus_frequency_query.sort_key = ConcordanceCollocationSortKey::CorpusFrequency;
    let corpus_frequency =
        query_concordance_collocations_from_aggregate(&aggregate, &corpus_frequency_query);

    assert_eq!(association.rows[0].surface, "rare");
    assert_eq!(near.rows[0].surface, "common");
    assert_eq!(corpus_frequency.rows[0].surface, "common");
}

#[test]
fn surface_collocation_stopwords_filter_after_aggregation() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "the target useful target the target useful",
    )];
    let mut query = collocation_query("target");
    query.span = 1;
    query.min_cooccurrence = 1;
    query.min_collocate_frequency = 1;
    query.sort_key = ConcordanceCollocationSortKey::NearQuery;
    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();

    let shown = query_concordance_collocations_from_aggregate(&aggregate, &query);
    let mut hidden_query = query.clone();
    hidden_query.hide_stopwords = true;
    let hidden = query_concordance_collocations_from_aggregate(&aggregate, &hidden_query);

    assert!(shown.rows.iter().any(|row| row.surface == "the"));
    assert!(hidden.rows.iter().all(|row| row.surface != "the"));
    assert!(hidden.rows.iter().any(|row| row.surface == "useful"));
}
