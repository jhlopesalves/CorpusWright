use super::*;
use crate::analysis::test_support::*;
use crate::analysis::*;
use crate::annotation::UniversalPos;
use std::sync::atomic::AtomicBool;

#[test]
fn frequency_invariant_under_punctuation() {
    let punctuated = corpus_for(&[("punct.txt", "(alpha), beta! alpha; beta?")]);
    let plain = corpus_for(&[("plain.txt", "alpha beta alpha beta")]);

    let punctuated_page = query_frequency_list(&punctuated, &base_frequency_query());
    let plain_page = query_frequency_list(&plain, &base_frequency_query());

    assert_eq!(
        frequency_tokens(&punctuated_page),
        frequency_tokens(&plain_page)
    );
    assert_eq!(
        punctuated.overview().total_tokens,
        plain.overview().total_tokens
    );
    assert_eq!(
        punctuated.overview().total_types,
        plain.overview().total_types
    );
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
            group_by: FrequencyGroupMode::Surface,
            case_fold: false,
            language: None,
            pos_filter: None,
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
fn frequency_query_window_bounds() {
    let corpus = corpus_for(&[("one.txt", "alpha beta gamma delta epsilon")]);
    let first = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            limit: 2,
            ..base_frequency_query()
        },
    );
    let second = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            offset: 2,
            limit: 2,
            ..base_frequency_query()
        },
    );

    assert_eq!(first.total_terms, 5);
    assert_eq!(second.total_terms, 5);
    assert_eq!(first.offset, 0);
    assert_eq!(first.items.len(), 2);
    assert_eq!(second.offset, 2);
    assert_eq!(second.items.len(), 2);
    assert_eq!(frequency_tokens(&first), [("alpha", 1), ("beta", 1)]);
    assert_eq!(frequency_tokens(&second), [("delta", 1), ("epsilon", 1)]);
}

#[test]
fn frequency_group_by_stem_en() {
    let corpus = corpus_for(&[(
        "stem.txt",
        "running runs connected connecting connects unrelated",
    )]);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            group_by: FrequencyGroupMode::Stem,
            language: Some("en".to_string()),
            sort_key: FrequencySortKey::Frequency,
            sort_direction: SortDirection::Desc,
            ..base_frequency_query()
        },
    );

    assert!(page.notices.is_empty());
    assert_eq!(page.items[0].token, "connect");
    assert_eq!(page.items[0].frequency, 3);
    assert_eq!(page.items[1].token, "run");
    assert_eq!(page.items[1].frequency, 2);
}

#[test]
fn stemming_unsupported_language_emits_notice() {
    let corpus = corpus_for(&[("stem.txt", "Running running")]);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            group_by: FrequencyGroupMode::Stem,
            language: Some("zz".to_string()),
            ..base_frequency_query()
        },
    );

    assert_eq!(page.notices.len(), 1);
    assert_eq!(
        page.notices[0].kind,
        FrequencyNoticeKind::StemmingUnsupportedLanguage
    );
    assert!(page.notices[0].message.contains("zz"));
    assert_eq!(frequency_tokens(&page), [("Running", 1), ("running", 1)]);
}

#[test]
fn frequency_group_by_lemma_uses_annotation() {
    let mut corpus = corpus_for(&[("lemma.txt", "Cats cat ran running")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("cat", "cat", UniversalPos::Noun, 1),
            ("ran", "run", UniversalPos::Verb, 1),
            ("running", "run", UniversalPos::Verb, 1),
        ],
    );

    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            group_by: FrequencyGroupMode::Lemma,
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            ..base_frequency_query()
        },
    );

    assert!(page.notices.is_empty());
    assert_eq!(frequency_tokens(&page), [("cat", 2), ("run", 2)]);
}

#[test]
fn frequency_pos_filter_uses_annotation() {
    let mut corpus = corpus_for(&[("pos.txt", "Cats cat ran running")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("cat", "cat", UniversalPos::Noun, 1),
            ("ran", "run", UniversalPos::Verb, 1),
            ("running", "run", UniversalPos::Verb, 1),
        ],
    );

    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            pos_filter: Some("NOUN".to_string()),
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            ..base_frequency_query()
        },
    );

    assert!(page.notices.is_empty());
    assert_eq!(frequency_tokens(&page), [("Cats", 1), ("cat", 1)]);
}

#[test]
fn dp_is_zero_for_perfectly_even_distribution() {
    let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
    let stats = frequency_distribution_stats(&[(0, 1), (1, 1)], 2, &corpus.documents, 4);

    assert_close(stats.dp, 0.0);
}

#[test]
fn dp_increases_with_concentration() {
    let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
    let even = frequency_distribution_stats(&[(0, 1), (1, 1)], 2, &corpus.documents, 4);
    let concentrated = frequency_distribution_stats(&[(0, 2)], 2, &corpus.documents, 4);

    assert!(concentrated.dp > even.dp);
}

#[test]
fn dp_uses_document_sizes_not_equal_parts() {
    let corpus = corpus_for(&[("short.txt", "alpha"), ("z_long.txt", "alpha beta gamma")]);
    // doc sizes 1:3. Counts proportional to sizes => DP = 0 ONLY under size weighting
    // (under equal-parts weighting this would be 0.25).
    let even = frequency_distribution_stats(&[(0, 1), (1, 3)], 4, &corpus.documents, 4);
    assert_close(even.dp, 0.0);
    // Counts inverse to sizes => maximally skewed for this 2-part split.
    let concentrated = frequency_distribution_stats(&[(0, 3), (1, 1)], 4, &corpus.documents, 4);
    assert_close(concentrated.dp, 0.5);
}

#[test]
fn top_document_share_reports_max_single_doc_proportion() {
    let corpus = corpus_for(&[("a.txt", "alpha alpha"), ("b.txt", "beta beta")]);
    let stats = frequency_distribution_stats(&[(0, 2), (1, 6)], 8, &corpus.documents, 4);

    assert_close(stats.top_doc_share, 0.75);
}

#[test]
fn normalised_frequency_per_10k_is_correct() {
    let corpus = corpus_for(&[("a.txt", "alpha beta gamma"), ("b.txt", "delta epsilon")]);
    let stats = frequency_distribution_stats(&[(0, 3), (1, 2)], 5, &corpus.documents, 10);

    assert_close(stats.per_10k, 5_000.0);
}

#[test]
fn document_frequency_counts_documents_with_at_least_one_hit() {
    let corpus = corpus_for(&[("a.txt", "alpha"), ("b.txt", "beta"), ("c.txt", "gamma")]);
    let stats = frequency_distribution_stats(&[(0, 2), (1, 0), (2, 3)], 5, &corpus.documents, 3);

    assert_eq!(stats.doc_freq, 2);
}

#[test]
fn pos_distribution_lists_all_present_upos_tags() {
    let mut corpus = corpus_for(&[("a.txt", "Cats run quickly"), ("b.txt", "Cats run")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 1),
            ("quickly", "quickly", UniversalPos::Adv, 1),
        ],
    );
    add_document_annotations(
        &mut corpus,
        1,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 1),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &annotated_frequency_query(AnnotatedFrequencyMode::Pos),
        ready_annotation_cache(),
    );
    let rows = result
        .pos_rows
        .iter()
        .map(|row| (row.upos.as_str(), row.raw, row.doc_freq, row.doc_total))
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![("NOUN", 2, 2, 2), ("VERB", 2, 2, 2), ("ADV", 1, 1, 2),]
    );
}

#[test]
fn lemma_pos_frequency_groups_by_lemma_and_upos() {
    let mut corpus = corpus_for(&[("a.txt", "run runs"), ("b.txt", "run running")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("run", "run", UniversalPos::Verb, 1),
            ("runs", "run", UniversalPos::Verb, 1),
        ],
    );
    add_document_annotations(
        &mut corpus,
        1,
        &[
            ("run", "run", UniversalPos::Noun, 1),
            ("running", "run", UniversalPos::Verb, 1),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos),
        ready_annotation_cache(),
    );
    let rows = result
        .lex_rows
        .iter()
        .map(|row| (row.lemma.as_str(), row.upos.as_str(), row.raw, row.doc_freq))
        .collect::<Vec<_>>();

    assert_eq!(rows, vec![("run", "VERB", 3, 2), ("run", "NOUN", 1, 1)]);
}

#[test]
fn lemma_pos_frequency_honours_filters_before_pagination() {
    let mut corpus = corpus_for(&[
        ("a.txt", "Alpha alpha the run"),
        ("b.txt", "ALPHA the run run"),
    ]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Alpha", "Alpha", UniversalPos::Noun, 1),
            ("alpha", "alpha", UniversalPos::Noun, 1),
            ("the", "the", UniversalPos::Det, 1),
            ("run", "run", UniversalPos::Verb, 1),
        ],
    );
    add_document_annotations(
        &mut corpus,
        1,
        &[
            ("ALPHA", "alpha", UniversalPos::Noun, 1),
            ("the", "the", UniversalPos::Det, 1),
            ("run", "run", UniversalPos::Verb, 2),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &AnnotatedFrequencyQuery {
            query: "ALPHA".to_string(),
            match_mode: FrequencyMatchMode::Exact,
            case_fold: true,
            pos_filter: Some("noun".to_string()),
            minimum_frequency: 2,
            limit: 1,
            ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
        },
        ready_annotation_cache(),
    );

    assert_eq!(result.total_rows, 1);
    assert_eq!(result.lex_rows.len(), 1);
    assert_eq!(result.lex_rows[0].lemma, "alpha");
    assert_eq!(result.lex_rows[0].upos, "NOUN");
    assert_eq!(result.lex_rows[0].raw, 2);
}

#[test]
fn lemma_pos_frequency_hides_stopwords_by_lemma() {
    let mut corpus = corpus_for(&[("a.txt", "The work"), ("b.txt", "the work")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("The", "The", UniversalPos::Det, 1),
            ("work", "work", UniversalPos::Noun, 1),
        ],
    );
    add_document_annotations(
        &mut corpus,
        1,
        &[
            ("the", "the", UniversalPos::Det, 1),
            ("work", "work", UniversalPos::Verb, 1),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &AnnotatedFrequencyQuery {
            hide_stopwords: true,
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
        },
        ready_annotation_cache(),
    );

    let rows = result
        .lex_rows
        .iter()
        .map(|row| (row.lemma.as_str(), row.upos.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(result.total_rows, 2);
    assert_eq!(rows, vec![("work", "NOUN"), ("work", "VERB")]);
}

#[test]
fn annotated_frequency_sorts_with_shared_keys_and_direction() {
    let mut corpus = corpus_for(&[("a.txt", "alpha beta beta"), ("b.txt", "gamma gamma")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("alpha", "alpha", UniversalPos::Noun, 1),
            ("beta", "beta", UniversalPos::Noun, 2),
        ],
    );
    add_document_annotations(&mut corpus, 1, &[("gamma", "gamma", UniversalPos::Verb, 2)]);

    let result = query_annotated_frequency(
        &corpus,
        &AnnotatedFrequencyQuery {
            sort_key: FrequencySortKey::Frequency,
            sort_direction: SortDirection::Asc,
            ..annotated_frequency_query(AnnotatedFrequencyMode::LemmaPos)
        },
        ready_annotation_cache(),
    );

    let rows = result
        .lex_rows
        .iter()
        .map(|row| (row.lemma.as_str(), row.raw))
        .collect::<Vec<_>>();
    assert_eq!(rows, vec![("alpha", 1), ("beta", 2), ("gamma", 2)]);
}

#[test]
fn pos_frequency_query_matches_upos_case_insensitively() {
    let mut corpus = corpus_for(&[("a.txt", "Cats run quickly")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 1),
            ("quickly", "quickly", UniversalPos::Adv, 1),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &AnnotatedFrequencyQuery {
            query: "verb".to_string(),
            match_mode: FrequencyMatchMode::Exact,
            ..annotated_frequency_query(AnnotatedFrequencyMode::Pos)
        },
        ready_annotation_cache(),
    );

    assert_eq!(result.total_rows, 1);
    assert_eq!(result.pos_rows[0].upos, "VERB");
}

#[test]
fn annotated_frequency_requires_ready_annotation_layer() {
    let mut corpus = corpus_for(&[("pos.txt", "Cats run")]);
    add_document_annotations(
        &mut corpus,
        0,
        &[
            ("Cats", "cat", UniversalPos::Noun, 1),
            ("run", "run", UniversalPos::Verb, 1),
        ],
    );

    let result = query_annotated_frequency(
        &corpus,
        &annotated_frequency_query(AnnotatedFrequencyMode::Pos),
        AnnotationCacheAvailability {
            state: AnnotationCacheState::Missing,
            detail: None,
        },
    );

    assert!(result.pos_rows.is_empty());
    assert_eq!(
        result.unavailable_message.as_deref(),
        Some(ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE)
    );
}

#[test]
fn frequency_lemma_requires_annotation_notice() {
    let corpus = corpus_for(&[("lemma.txt", "Cats cat")]);
    assert!(
        corpus
            .documents
            .iter()
            .all(|document| !document.annotation_available)
    );
    assert!(!corpus.overview().annotation_available);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            group_by: FrequencyGroupMode::Lemma,
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            ..base_frequency_query()
        },
    );

    assert_eq!(page.notices.len(), 1);
    assert_eq!(
        page.notices[0].kind,
        FrequencyNoticeKind::AnnotationUnavailable
    );
    assert_eq!(frequency_tokens(&page), [("Cats", 1), ("cat", 1)]);
}

#[test]
fn frequency_pos_filter_requires_annotation_notice() {
    let corpus = corpus_for(&[("pos.txt", "Cats ran")]);
    assert!(
        corpus
            .documents
            .iter()
            .all(|document| !document.annotation_available)
    );
    assert!(!corpus.overview().annotation_available);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            pos_filter: Some("NOUN".to_string()),
            sort_key: FrequencySortKey::Token,
            sort_direction: SortDirection::Asc,
            ..base_frequency_query()
        },
    );

    assert_eq!(page.notices.len(), 1);
    assert_eq!(
        page.notices[0].kind,
        FrequencyNoticeKind::PosFilterUnavailable
    );
    assert_eq!(frequency_tokens(&page), [("Cats", 1), ("ran", 1)]);
}

#[test]
fn frequency_min_count_and_casefold() {
    let corpus = corpus_for(&[("case.txt", "Alpha alpha ALPHA beta Beta gamma")]);
    let folded = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            minimum_frequency: 2,
            case_fold: true,
            ..base_frequency_query()
        },
    );
    let surface = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            minimum_frequency: 2,
            case_fold: false,
            ..base_frequency_query()
        },
    );

    assert_eq!(frequency_tokens(&folded), [("alpha", 3), ("beta", 2)]);
    assert_eq!(surface.total_terms, 0);
    assert_eq!(surface.scoped_total_types, 6);
}

#[test]
fn frequency_rows_report_raw_and_per_million_counts() {
    let corpus = corpus_for(&[("one.txt", "alpha alpha beta")]);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            query: "alpha".to_string(),
            match_mode: FrequencyMatchMode::Exact,
            minimum_frequency: 1,
            token_class: FrequencyTokenClass::All,
            hide_stopwords: false,
            group_by: FrequencyGroupMode::Surface,
            case_fold: false,
            language: None,
            pos_filter: None,
            sort_key: FrequencySortKey::Frequency,
            sort_direction: SortDirection::Desc,
            offset: 0,
            limit: 10,
        },
    );
    assert_eq!(page.items[0].frequency, 2);
    assert!((page.items[0].rate_per_million - 666_666.666_666_666_6).abs() < 1e-6);
}

#[test]
fn document_frequency_spectrum_and_median_are_computed_over_all_types() {
    let corpus = corpus_for(&[
        ("one.txt", "alpha shared common"),
        ("two.txt", "beta shared common"),
        ("three.txt", "gamma common"),
    ]);
    let dispersion = &corpus.overview().dispersion;

    assert_eq!(dispersion.df_in_1_count, 3);
    assert_eq!(dispersion.df_in_2_to_n_minus_1_count, 1);
    assert_eq!(dispersion.df_in_all_count, 1);
    assert_eq!(dispersion.median_documents_per_type, Some(1.0));
    assert_eq!(dispersion.median_dp, None);
}

#[test]
fn frequency_coverage_requires_the_full_named_threshold() {
    let text = (0..100)
        .map(|index| format!("term{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let records = vec![record("coverage.txt")];
    let corpus = build_analysis_corpus(
        &records,
        build_options(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, &text)),
        |_| {},
    )
    .unwrap();

    assert_eq!(
        corpus.overview().frequency_concentration.top_100_coverage,
        Some(1.0)
    );
    assert_eq!(
        corpus.overview().frequency_concentration.top_1000_coverage,
        None
    );

    let small = corpus_for(&[("small.txt", "alpha beta gamma")]);
    assert_eq!(
        small.overview().frequency_concentration.top_100_coverage,
        None
    );
}
