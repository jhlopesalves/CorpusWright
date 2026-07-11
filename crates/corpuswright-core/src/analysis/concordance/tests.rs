use super::*;
use crate::analysis::test_support::*;
use crate::decode::{DecodeStrategy, SourceEncoding, decode_source_bytes};
use crate::index::{InMemoryIndexStore, IndexStore, SurfaceIndexDocumentInput, SurfaceIndexError};
use crate::tokenization::TokenizationConfig;

#[test]
fn concordance_routing_types_remain_rust_only_without_ts_export() {
    let source = include_str!("mod.rs");
    let start = source
        .find("pub enum ConcordanceBackend")
        .expect("routing type source should exist");
    let end = source
        .find("#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ConcordanceError")
        .expect("concordance error source should exist");
    let routing_source = &source[start..end];

    assert!(!routing_source.contains("#[ts(export)]"));
    assert!(!routing_source.contains("TS"));
}

#[test]
fn kwic_finds_surface_token_with_token_windows() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "alpha beta gamma beta delta epsilon",
    )];
    let mut query = concordance_query("beta");
    query.window_tokens = 2;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(result.rows[0].left_context, ["alpha"]);
    assert_eq!(result.rows[0].node, "beta");
    assert_eq!(result.rows[0].right_context, ["gamma", "beta"]);
    assert_eq!(result.rows[1].left_context, ["beta", "gamma"]);
    assert_eq!(result.rows[1].right_context, ["delta", "epsilon"]);
}

#[test]
fn kwic_finds_phrase_and_context_around_full_node() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "alpha not very good movie today not very good ending",
    )];
    let mut query = concordance_query("not very good");
    query.window_tokens = 1;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(result.rows[0].left_context, ["alpha"]);
    assert_eq!(result.rows[0].node, "not very good");
    assert_eq!(result.rows[0].right_context, ["movie"]);
    assert_eq!(result.rows[1].left_context, ["today"]);
    assert_eq!(result.rows[1].right_context, ["ending"]);
}

#[test]
fn kwic_does_not_match_substrings_inside_tokens() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "the there breathe other The",
    )];

    let result = concordance_for(&documents, concordance_query("the")).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| row.node.as_str())
            .collect::<Vec<_>>(),
        ["the", "The"]
    );
}

#[test]
fn kwic_is_case_insensitive_by_default() {
    let documents = vec![concordance_document(0, "doc.txt", "The the THE theme")];

    let result = concordance_for(&documents, concordance_query("the")).unwrap();

    assert_eq!(result.total_hits, 3);
}

#[test]
fn kwic_can_match_case_sensitive_when_enabled() {
    let documents = vec![concordance_document(0, "doc.txt", "The the THE")];
    let mut query = concordance_query("the");
    query.case_sensitive = true;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 1);
    assert_eq!(result.rows[0].node, "the");
}

#[test]
fn kwic_rejects_empty_query() {
    let documents = vec![concordance_document(0, "doc.txt", "alpha")];

    let error = concordance_for(&documents, concordance_query("   ")).unwrap_err();

    assert_eq!(error, ConcordanceError::EmptyQuery);
}

#[test]
fn kwic_reports_total_hits_and_documents_with_hits() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta alpha"),
        concordance_document(1, "b.txt", "beta gamma"),
        concordance_document(2, "c.txt", "alpha"),
    ];

    let result = concordance_for(&documents, concordance_query("alpha")).unwrap();

    assert_eq!(result.total_hits, 3);
    assert_eq!(result.documents_with_hits, 2);
}

#[test]
fn kwic_stats_use_full_hit_set_and_document_token_proportions() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha alpha"),
        concordance_document(1, "b.txt", "beta beta beta beta beta beta"),
    ];
    let mut query = concordance_query("alpha");
    query.limit = 1;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.total_hits, 2);
    assert_eq!(result.documents_with_hits, 1);
    assert_eq!(result.selected_documents_total, 2);
    assert_eq!(result.selected_tokens_total, 8);
    assert_close(result.dp, 0.75);
    assert_close(result.top_document_share, 1.0);
}

#[test]
fn kwic_context_filter_counts_full_filtered_hit_set() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "red target blue target green target red",
    )];
    let mut query = concordance_query("target");
    query.window_tokens = 1;
    query.limit = 1;
    query.context_filter = Some("blue".to_string());

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.context_filter.as_deref(), Some("blue"));
    assert_eq!(result.total_hits, 2);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].right_context, ["blue"]);
}

#[test]
fn kwic_context_filter_direction_defaults_to_both_when_omitted() {
    let query: ConcordanceQuery = serde_json::from_value(serde_json::json!({
        "query": "target",
        "contextFilter": "blue",
        "windowTokens": 1,
        "offset": 0,
        "limit": 100
    }))
    .unwrap();

    assert_eq!(
        query.context_filter_direction,
        ConcordanceContextFilterDirection::Both
    );
}

#[test]
fn kwic_context_filter_respects_direction() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "red target blue target green",
    )];
    let token_indexes_for_direction = |direction: ConcordanceContextFilterDirection| -> Vec<usize> {
        let mut query = concordance_query("target");
        query.window_tokens = 1;
        query.context_filter = Some("blue".to_string());
        query.context_filter_direction = direction;

        concordance_for(&documents, query)
            .unwrap()
            .rows
            .iter()
            .map(|row| row.token_index)
            .collect()
    };

    assert_eq!(
        token_indexes_for_direction(ConcordanceContextFilterDirection::Both),
        [1, 3]
    );
    assert_eq!(
        token_indexes_for_direction(ConcordanceContextFilterDirection::Right),
        [1]
    );
    assert_eq!(
        token_indexes_for_direction(ConcordanceContextFilterDirection::Left),
        [3]
    );
}

#[test]
fn kwic_respects_offset_and_limit() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "alpha beta alpha gamma alpha delta alpha",
    )];
    let mut query = concordance_query("alpha");
    query.offset = 1;
    query.limit = 2;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 4);
    assert_eq!(result.offset, 1);
    assert_eq!(result.limit, 2);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| row.token_index)
            .collect::<Vec<_>>(),
        [2, 4]
    );
}

#[test]
fn kwic_sorts_full_hit_set_before_pagination() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "alpha z alpha a alpha m",
    )];
    let mut query = concordance_query("alpha");
    query.sort_key = ConcordanceSortKey::Right1;
    query.offset = 1;
    query.limit = 1;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 3);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].right_context, ["m"]);
}

#[test]
fn kwic_left_sort_uses_missing_context_last() {
    let documents = vec![concordance_document(0, "doc.txt", "alpha b a alpha")];
    let mut query = concordance_query("alpha");
    query.sort_key = ConcordanceSortKey::Left1;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(
        result.rows[0].left_context.last().map(String::as_str),
        Some("a")
    );
    assert!(result.rows[1].left_context.is_empty());
}

#[test]
fn kwic_file_position_sort_ties_by_document_and_offset() {
    let documents = vec![
        concordance_document(1, "b.txt", "alpha beta alpha"),
        concordance_document(0, "a.txt", "beta alpha"),
    ];

    let result = concordance_for(&documents, concordance_query("alpha")).unwrap();

    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| (row.document_index, row.token_index))
            .collect::<Vec<_>>(),
        [(0, 1), (1, 0), (1, 2)]
    );
}

#[test]
fn concordance_default_route_uses_scan_and_matches_public_entry() {
    let documents = vec![concordance_document(0, "doc.txt", "Movie movie MOVIE")];
    let query = concordance_query("movie");
    let public = concordance_for(&documents, query.clone()).unwrap();
    let mut context = ConcordanceSearchContext::default();

    let routed = routed_concordance_for(
        &documents,
        &query,
        &mut context,
        ConcordanceBackend::default(),
    )
    .unwrap();

    assert_eq!(routed.backend_used, ConcordanceBackendUsed::Scan);
    assert_eq!(routed.result, public);
    assert_eq!(context.index_build_count(), 0);
}

#[test]
fn routed_index_concordance_parity_holds_for_required_english_queries() {
    let documents = vec![
        concordance_document(
            0,
            "a.txt",
            "Movie, movie. MOVIE. good movie. Good movie! I love it. i love it.",
        ),
        concordance_document(
            1,
            "b.txt",
            "A movie is not the same as a Good movie? I love it.",
        ),
    ];

    for query in [
        "movie",
        "Movie",
        "MOVIE",
        "good movie",
        "Good movie",
        "I love it",
        "i love it",
    ] {
        assert_routed_index_parity_both_modes(&documents, query);
    }
}

#[test]
fn routed_index_concordance_parity_holds_for_required_portuguese_queries() {
    let documents = vec![
        concordance_document(
            0,
            "pt-a.txt",
            "ação Ação coração Coração não Não João joão maçã Maçã.",
        ),
        concordance_document(1, "pt-b.txt", "A ação toca o coração, e João diz não."),
    ];

    for query in [
        "ação",
        "Ação",
        "coração",
        "Coração",
        "não",
        "Não",
        "João",
        "joão",
    ] {
        assert_routed_index_parity_both_modes(&documents, query);
    }
}

#[test]
fn routed_index_sort_keys_match_scan_beyond_visible_window() {
    let documents = vec![
        concordance_document(0, "a.txt", "z b alpha c y alpha d"),
        concordance_document(1, "b.txt", "a q alpha b r alpha c"),
    ];
    for sort_key in [
        ConcordanceSortKey::Node,
        ConcordanceSortKey::Left1,
        ConcordanceSortKey::Left2,
        ConcordanceSortKey::Right1,
        ConcordanceSortKey::Right2,
    ] {
        let mut query = parity_concordance_query("alpha", false);
        query.sort_key = sort_key;
        query.window_tokens = 1;
        let mut scan_context = ConcordanceSearchContext::default();
        let scan = routed_concordance_for(
            &documents,
            &query,
            &mut scan_context,
            ConcordanceBackend::Scan,
        )
        .unwrap();
        let mut index_context = ConcordanceSearchContext::default();
        let index = routed_concordance_for(
            &documents,
            &query,
            &mut index_context,
            ConcordanceBackend::Index,
        )
        .unwrap();

        assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(index.result, scan.result, "{sort_key:?}");
    }
}

#[test]
fn routed_index_parity_holds_for_selection_and_pagination() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta alpha gamma alpha"),
        concordance_document(1, "b.txt", "alpha delta alpha"),
        concordance_document(2, "c.txt", "epsilon alpha alpha alpha"),
    ];
    let mut query = parity_concordance_query("alpha", false);
    query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![1, 2],
        excluded_ids: Vec::new(),
    };
    query.offset = 1;
    query.limit = 2;

    let mut scan_context = ConcordanceSearchContext::default();
    let scan = routed_concordance_for(
        &documents,
        &query,
        &mut scan_context,
        ConcordanceBackend::Scan,
    )
    .unwrap();
    let mut index_context = ConcordanceSearchContext::default();
    let index = routed_concordance_for(
        &documents,
        &query,
        &mut index_context,
        ConcordanceBackend::Index,
    )
    .unwrap();

    assert_eq!(scan.backend_used, ConcordanceBackendUsed::Scan);
    assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(index.result, scan.result);
    assert_eq!(index.result.total_hits, 5);
    assert_eq!(index.result.selected_documents_total, 2);
    assert_eq!(index.result.offset, 1);
    assert_eq!(index.result.limit, 2);
    assert_eq!(index.result.rows.len(), 2);
    assert!(index.result.rows.iter().all(|row| row.document_index != 0));
    assert_eq!(
        index
            .result
            .rows
            .iter()
            .map(|row| (row.document_index, row.token_index))
            .collect::<Vec<_>>(),
        [(1, 2), (2, 1)]
    );
}

#[test]
fn provider_index_reuses_full_corpus_store_across_selection_changes_and_warm_queries() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta alpha"),
        concordance_document(1, "b.txt", "alpha gamma"),
        concordance_document(2, "c.txt", "delta alpha"),
    ];
    let resolutions = std::cell::Cell::new(0usize);
    let mut context = ConcordanceSearchContext::default();
    let mut first_query = parity_concordance_query("alpha", false);
    first_query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![0],
        excluded_ids: Vec::new(),
    };
    let first = search_concordance_documents_routed_provider(
        documents.len(),
        &first_query,
        &TokenizationConfig::default(),
        &mut context,
        |index| {
            resolutions.set(resolutions.get() + 1);
            Some(documents[index].clone())
        },
        |_| {},
    )
    .unwrap();
    let cold_resolutions = resolutions.get();

    let mut second_query = parity_concordance_query("alpha", false);
    second_query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![1, 2],
        excluded_ids: Vec::new(),
    };
    let second = search_concordance_documents_routed_provider(
        documents.len(),
        &second_query,
        &TokenizationConfig::default(),
        &mut context,
        |index| {
            resolutions.set(resolutions.get() + 1);
            Some(documents[index].clone())
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(first.result.total_hits, 2);
    assert_eq!(second.result.total_hits, 2);
    assert_eq!(context.index_build_count(), 1);
    assert_eq!(context.index_cache_load_count(), 0);
    assert_eq!(resolutions.get(), cold_resolutions);
    assert_eq!(cold_resolutions, documents.len() * 2);
}

#[test]
fn provider_cache_load_streams_identity_once_and_content_changes_invalidate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta"),
        concordance_document(1, "b.txt", "gamma alpha"),
    ];
    let query = parity_concordance_query("alpha", false);
    let config = TokenizationConfig::default();
    let mut first = ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    search_concordance_documents_routed_provider(
        documents.len(),
        &query,
        &config,
        &mut first,
        |index| Some(documents[index].clone()),
        |_| {},
    )
    .unwrap();

    let cache_resolutions = std::cell::Cell::new(0usize);
    let mut restarted = ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let cached = search_concordance_documents_routed_provider(
        documents.len(),
        &query,
        &config,
        &mut restarted,
        |index| {
            cache_resolutions.set(cache_resolutions.get() + 1);
            Some(documents[index].clone())
        },
        |_| {},
    )
    .unwrap();
    assert!(!cached.index_rebuilt);
    assert_eq!(restarted.index_cache_load_count(), 1);
    assert_eq!(cache_resolutions.get(), documents.len());

    let mut changed_documents = documents.clone();
    changed_documents[0].text.push_str(" alpha");
    let mut changed = ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let changed_result = search_concordance_documents_routed_provider(
        changed_documents.len(),
        &query,
        &config,
        &mut changed,
        |index| (index != 1).then(|| changed_documents[index].clone()),
        |_| {},
    )
    .unwrap();
    assert!(changed_result.index_rebuilt);
    assert_eq!(changed.index_build_count(), 1);
    assert_eq!(changed_result.result.total_hits, 2);
}

#[test]
fn provider_reports_progress_during_index_build_and_search() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha beta"),
        concordance_document(1, "b.txt", "gamma alpha"),
    ];
    let query = parity_concordance_query("alpha", false);
    let config = TokenizationConfig::default();
    let mut context = ConcordanceSearchContext::default();
    
    let mut recorded_phases = Vec::new();
    let mut total_files_reported = 0;
    
    search_concordance_documents_routed_provider(
        documents.len(),
        &query,
        &config,
        &mut context,
        |index| Some(documents[index].clone()),
        |progress| {
            if !recorded_phases.contains(&progress.current_phase) {
                recorded_phases.push(progress.current_phase);
            }
            total_files_reported = progress.files_total;
        },
    )
    .unwrap();

    assert!(recorded_phases.contains(&ConcordancePhase::BuildingIndex));
    assert!(recorded_phases.contains(&ConcordancePhase::Searching));
    assert_eq!(total_files_reported, 2);
}

#[test]
fn million_hit_query_retains_and_materialises_only_requested_page() {
    let documents = vec![concordance_document(
        0,
        "common.txt",
        &"common ".repeat(1_000_000),
    )];
    let mut query = parity_concordance_query("common", false);
    query.sort_key = ConcordanceSortKey::Node;
    query.window_tokens = 1;
    query.limit = 100;
    let config = TokenizationConfig::default();

    let mut index_context = ConcordanceSearchContext::default();
    let indexed = search_concordance_documents_routed(
        &documents,
        &query,
        &config,
        &mut index_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    let mut scan_context = ConcordanceSearchContext::default();
    let scanned = search_concordance_documents_routed(
        &documents,
        &query,
        &config,
        &mut scan_context,
        ConcordanceBackend::Scan,
    )
    .unwrap();

    for routed in [&indexed, &scanned] {
        assert_eq!(routed.result.total_hits, 1_000_000);
        assert_eq!(routed.result.rows.len(), 100);
        assert_eq!(routed.execution_stats.valid_hits, 1_000_000);
        assert!(routed.execution_stats.candidates_visited >= 1_000_000);
        assert!(routed.execution_stats.compact_sort_records_peak <= 100);
        assert_eq!(routed.execution_stats.rows_materialised, 100);
    }
    assert_eq!(indexed.result, scanned.result);
}

#[test]
fn retained_sort_depth_above_budget_is_a_typed_error() {
    let documents = vec![concordance_document(0, "a.txt", "alpha beta alpha")];
    let mut query = parity_concordance_query("alpha", false);
    query.sort_key = ConcordanceSortKey::Left1;
    query.offset = 100_000;
    query.limit = 500;
    let mut context = ConcordanceSearchContext::default();

    assert!(matches!(
        search_concordance_documents_routed(
            &documents,
            &query,
            &TokenizationConfig::default(),
            &mut context,
            ConcordanceBackend::Scan,
        ),
        Err(ConcordanceError::Index(
            SurfaceIndexError::RetainedSortDepthExceedsQueryBudget { .. }
        ))
    ));
}

#[test]
fn routed_index_handles_context_filter_and_matches_scan() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "red target blue target green target red",
    )];
    let mut query = concordance_query("target");
    query.window_tokens = 1;
    query.context_filter = Some("blue".to_string());
    query.context_filter_direction = ConcordanceContextFilterDirection::Right;
    let scan = concordance_for(&documents, query.clone()).unwrap();
    let mut context = ConcordanceSearchContext::default();

    let routed =
        routed_concordance_for(&documents, &query, &mut context, ConcordanceBackend::Index)
            .unwrap();

    assert_eq!(routed.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(routed.result, scan);
    assert_eq!(context.index_build_count(), 1);
}

#[test]
fn routed_index_mixed_fixture_context_filter_has_literal_parity() {
    let documents = vec![
        concordance_document(0, "a.txt", "Blue target red target blue unique"),
        concordance_document(
            1,
            "b.txt",
            "target green TARGET blue target repeated repeated",
        ),
        concordance_document(2, "c.txt", "unique words only"),
    ];
    let mut query = parity_concordance_query("target", false);
    query.window_tokens = 1;
    query.context_filter = Some("blue".to_string());
    query.context_filter_direction = ConcordanceContextFilterDirection::Both;
    let scan =
        search_concordance_documents(&documents, &query, &TokenizationConfig::default()).unwrap();
    let mut context = ConcordanceSearchContext::default();
    let indexed =
        routed_concordance_for(&documents, &query, &mut context, ConcordanceBackend::Index)
            .unwrap();

    assert_eq!(indexed.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(indexed.result, scan);
    assert_eq!(indexed.result.total_hits, 4);
    assert_eq!(indexed.result.documents_with_hits, 2);
    assert_eq!(indexed.result.context_filter.as_deref(), Some("blue"));
    assert_eq!(
        indexed
            .result
            .rows
            .iter()
            .map(|row| (row.document_index, row.token_index, row.node.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, 1, "target"),
            (0, 3, "target"),
            (1, 2, "TARGET"),
            (1, 4, "target"),
        ]
    );
}

#[test]
fn routed_index_cache_reuses_store_and_rebuilds_when_manifest_changes() {
    let mut documents = vec![
        concordance_document(0, "a.txt", "Movie movie good movie"),
        concordance_document(1, "b.txt", "Good movie"),
    ];
    let mut context = ConcordanceSearchContext::default();
    let movie = parity_concordance_query("movie", false);
    let phrase = parity_concordance_query("good movie", false);

    let first = routed_concordance_for(&documents, &movie, &mut context, ConcordanceBackend::Index)
        .unwrap();
    let second =
        routed_concordance_for(&documents, &phrase, &mut context, ConcordanceBackend::Index)
            .unwrap();
    documents[0].text.push_str(" movie");
    let third = routed_concordance_for(&documents, &movie, &mut context, ConcordanceBackend::Index)
        .unwrap();

    assert_eq!(first.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(second.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(third.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(context.index_build_count(), 2);
    assert_eq!(first.result.total_hits, 4);
    assert_eq!(second.result.total_hits, 2);
    assert_eq!(third.result.total_hits, 5);
}

#[test]
fn routed_index_disk_cache_reuses_content_across_restart_and_recovers_from_corruption() {
    let temp_dir = tempfile::tempdir().unwrap();
    let documents = vec![
        concordance_document(0, "a.txt", "Movie movie good movie"),
        concordance_document(1, "b.txt", "Good movie"),
    ];
    let query = parity_concordance_query("movie", false);
    let config = TokenizationConfig::default();
    let mut first_context =
        ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let first = search_concordance_documents_routed(
        &documents,
        &query,
        &config,
        &mut first_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    assert!(first.index_rebuilt);
    assert_eq!(first_context.index_build_count(), 1);

    let renamed_documents = vec![
        concordance_document(0, "renamed-a.txt", "Movie movie good movie"),
        concordance_document(1, "renamed-b.txt", "Good movie"),
    ];
    let mut restarted_context =
        ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let restarted = search_concordance_documents_routed(
        &renamed_documents,
        &query,
        &config,
        &mut restarted_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    assert!(!restarted.index_rebuilt);
    assert_eq!(restarted_context.index_build_count(), 0);
    assert_eq!(restarted_context.index_cache_load_count(), 1);
    assert!(
        restarted
            .result
            .rows
            .iter()
            .all(|row| row.source_path.starts_with("renamed-"))
    );

    let mut changed_config = config;
    changed_config.include_punctuation = false;
    let mut changed_config_context =
        ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let changed = search_concordance_documents_routed(
        &renamed_documents,
        &query,
        &changed_config,
        &mut changed_config_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    assert!(changed.index_rebuilt);
    assert_eq!(changed_config_context.index_build_count(), 1);

    let manifest = concordance_surface_index_manifest(&renamed_documents, &config).unwrap();
    let cache_path = crate::index::surface_index_cache_path(temp_dir.path(), &manifest);
    std::fs::write(&cache_path, b"truncated").unwrap();
    let mut corrupted_context =
        ConcordanceSearchContext::with_cache_root(temp_dir.path().to_path_buf());
    let recovered = search_concordance_documents_routed(
        &renamed_documents,
        &query,
        &config,
        &mut corrupted_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    assert!(recovered.index_rebuilt);
    assert_eq!(corrupted_context.index_build_count(), 1);
    assert_eq!(recovered.result, restarted.result);
}

#[test]
fn routed_index_windows_1252_portuguese_offsets_and_parity_hold() {
    let bytes = b"caf\xe9 a\xe7\xe3o A\xe7\xe3o Jo\xe3o jo\xe3o cora\xe7\xe3o Cora\xe7\xe3o n\xe3o N\xe3o ma\xe7\xe3 Ma\xe7\xe3";
    let decoded = decode_source_bytes(bytes);
    assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
    assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
    assert!(decoded.had_utf8_error);
    assert_eq!(
        decoded.text,
        "café ação Ação João joão coração Coração não Não maçã Maçã"
    );
    assert!(!decoded.text.contains('\u{fffd}'));

    let index_inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "pt-win1252.txt".to_string(),
        display_name: "pt-win1252.txt".to_string(),
        prepared_text: decoded.text.as_str(),
    }];
    let store = InMemoryIndexStore::build(&index_inputs, &TokenizationConfig::default()).unwrap();
    let records = store.records_for_document(0).unwrap();
    for expected_surface in [
        "ação",
        "Ação",
        "João",
        "joão",
        "coração",
        "Coração",
        "não",
        "Não",
        "maçã",
        "Maçã",
    ] {
        assert!(
            records
                .iter()
                .any(|record| store.surface_for_record(record) == Some(expected_surface))
        );
    }

    let concordance_documents = vec![concordance_document(
        0,
        "pt-win1252.txt",
        decoded.text.as_str(),
    )];
    for query in [
        "ação",
        "Ação",
        "coração",
        "Coração",
        "não",
        "Não",
        "João",
        "joão",
    ] {
        assert_routed_index_parity_both_modes(&concordance_documents, query);
    }
}

#[test]
fn kwic_distribution_counts_and_paginates_after_sorting() {
    let documents = vec![
        concordance_document(0, "c.txt", "alpha beta"),
        concordance_document(1, "a.txt", "alpha alpha beta"),
        concordance_document(2, "b.txt", "beta beta"),
    ];
    let mut query = concordance_query("alpha");
    query.distribution_offset = 1;
    query.distribution_limit = 1;

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.distribution.total_documents, 3);
    assert_eq!(result.distribution.offset, 1);
    assert_eq!(result.distribution.items.len(), 1);
    assert_eq!(result.distribution.items[0].display_name, "c.txt");
    assert_eq!(result.distribution.items[0].count, 1);
    assert_eq!(result.distribution.items[0].token_count, 2);
    assert_eq!(result.distribution.max_count, 2);
}

#[test]
fn kwic_honours_selected_file_scope() {
    let documents = vec![
        concordance_document(0, "a.txt", "alpha alpha"),
        concordance_document(1, "b.txt", "alpha beta"),
        concordance_document(2, "c.txt", "alpha gamma"),
    ];
    let mut query = concordance_query("alpha");
    query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![1, 2],
        excluded_ids: Vec::new(),
    };

    let result = concordance_for(&documents, query).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(result.selected_documents_total, 2);
    assert_eq!(
        result
            .rows
            .iter()
            .map(|row| row.display_name.as_str())
            .collect::<Vec<_>>(),
        ["b.txt", "c.txt"]
    );
}

#[test]
fn kwic_uses_canonical_tokenisation() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "state-of-the-art state of the art",
    )];

    let compound = concordance_for(&documents, concordance_query("state-of-the-art")).unwrap();
    let single = concordance_for(&documents, concordance_query("state")).unwrap();

    assert_eq!(compound.total_hits, 1);
    assert_eq!(compound.rows[0].node, "state-of-the-art");
    assert_eq!(single.total_hits, 1);
}

#[test]
fn kwic_expansion_returns_wider_context_for_same_hit() {
    let documents = vec![concordance_document(
        0,
        "doc.txt",
        "zero one two target phrase three four five six",
    )];
    let mut query = concordance_query("target phrase");
    query.window_tokens = 1;
    let result = concordance_for(&documents, query).unwrap();
    let row = &result.rows[0];

    let expansion = concordance_expansion_for(
        &documents[0],
        ConcordanceExpansionQuery {
            document_index: row.document_index,
            token_index: row.token_index,
            node_token_length: row.node_token_length,
            window_tokens: 3,
        },
    )
    .unwrap();

    assert!(expansion.left_context.len() > row.left_context.len());
    assert!(expansion.right_context.len() > row.right_context.len());
    assert_eq!(expansion.node_tokens, ["target", "phrase"]);
    assert_eq!(expansion.node, "target phrase");
}

#[test]
fn kwic_expansion_clamps_at_document_start() {
    let document = concordance_document(0, "doc.txt", "alpha beta gamma delta");

    let expansion = concordance_expansion_for(
        &document,
        ConcordanceExpansionQuery {
            document_index: 0,
            token_index: 0,
            node_token_length: 1,
            window_tokens: 40,
        },
    )
    .unwrap();

    assert!(expansion.left_context.is_empty());
    assert_eq!(expansion.node_tokens, ["alpha"]);
    assert_eq!(expansion.right_context, ["beta", "gamma", "delta"]);
}

#[test]
fn kwic_expansion_clamps_at_document_end() {
    let document = concordance_document(0, "doc.txt", "alpha beta gamma delta");

    let expansion = concordance_expansion_for(
        &document,
        ConcordanceExpansionQuery {
            document_index: 0,
            token_index: 3,
            node_token_length: 1,
            window_tokens: 40,
        },
    )
    .unwrap();

    assert_eq!(expansion.left_context, ["alpha", "beta", "gamma"]);
    assert_eq!(expansion.node_tokens, ["delta"]);
    assert!(expansion.right_context.is_empty());
}

#[test]
fn kwic_expansion_rejects_invalid_identity_without_panic() {
    let document = concordance_document(0, "doc.txt", "alpha beta");

    let document_error = concordance_expansion_for(
        &document,
        ConcordanceExpansionQuery {
            document_index: 1,
            token_index: 0,
            node_token_length: 1,
            window_tokens: 40,
        },
    )
    .unwrap_err();
    assert_eq!(document_error, ConcordanceError::InvalidDocumentIdentity);

    let hit_error = concordance_expansion_for(
        &document,
        ConcordanceExpansionQuery {
            document_index: 0,
            token_index: 1,
            node_token_length: 2,
            window_tokens: 40,
        },
    )
    .unwrap_err();
    assert_eq!(hit_error, ConcordanceError::InvalidHitIdentity);
}

#[test]
fn kwic_expansion_retokenises_one_supplied_document() {
    let documents = [
        concordance_document(0, "a.txt", "alpha beta gamma"),
        concordance_document(1, "b.txt", "beta should not be inspected"),
    ];

    let expansion = concordance_expansion_for(
        &documents[0],
        ConcordanceExpansionQuery {
            document_index: 0,
            token_index: 1,
            node_token_length: 1,
            window_tokens: 40,
        },
    )
    .unwrap();

    assert_eq!(expansion.display_name, "a.txt");
    assert_eq!(expansion.node_tokens, ["beta"]);
    assert_eq!(expansion.right_context, ["gamma"]);
}

#[test]
#[ignore]
fn benchmark_concordance_scale() {
    use std::time::Instant;
    use std::env;
    use crate::analysis::concordance::{
        ConcordanceDocument, ConcordanceQuery, ConcordanceSearchContext, ConcordanceSortKey,
        search_concordance_documents_routed_provider, ConcordancePhase
    };
    use crate::analysis::collocation::{
        ConcordanceCollocationQuery, aggregate_concordance_collocations_routed_provider,
    };
    use crate::analysis::AnalysisDocumentSelection;
    use crate::tokenization::TokenizationConfig;
    use crate::index::SurfaceIndexCancellationToken;

    let num_tokens: usize = env::var("CONCORDANCE_BENCH_TOKENS")
        .unwrap_or_else(|_| "5000000".to_string())
        .parse()
        .unwrap_or(5_000_000);
    
    let scenario = env::var("CONCORDANCE_BENCH_SCENARIO")
        .unwrap_or_else(|_| "mixed".to_string());

    println!("Running Concordance Scale Benchmark");
    println!("Target tokens: {}", num_tokens);
    println!("Scenario: {}", scenario);

    let (documents, doc_len) = match scenario.as_str() {
        "whale" => {
            // One massive document
            let text = "the quick brown fox jumps over the lazy dog alpha beta gamma ".repeat(num_tokens / 12);
            let docs = vec![ConcordanceDocument {
                document_index: 0,
                source_path: "whale.txt".into(),
                display_name: "whale.txt".into(),
                text,
            }];
            (docs, 1)
        }
        "tiny-file" => {
            // Many tiny documents
            let num_docs = num_tokens / 10;
            let mut docs = Vec::with_capacity(num_docs);
            let text = "the quick brown fox jumps over the lazy dog";
            for i in 0..num_docs {
                docs.push(ConcordanceDocument {
                    document_index: i,
                    source_path: format!("doc_{}.txt", i).into(),
                    display_name: format!("doc_{}.txt", i).into(),
                    text: text.to_string(),
                });
            }
            (docs, num_docs)
        }
        _ => {
            // Mixed (typical corpus)
            let num_docs = 1000;
            let tokens_per_doc = num_tokens / num_docs;
            let mut docs = Vec::with_capacity(num_docs);
            for i in 0..num_docs {
                let text = "the quick brown fox jumps over the lazy dog alpha beta gamma ".repeat(tokens_per_doc / 12);
                docs.push(ConcordanceDocument {
                    document_index: i,
                    source_path: format!("mixed_{}.txt", i).into(),
                    display_name: format!("mixed_{}.txt", i).into(),
                    text,
                });
            }
            (docs, num_docs)
        }
    };

    println!("Generated {} documents", doc_len);

    let config = TokenizationConfig::default();
    let mut context = ConcordanceSearchContext::default();

    let query = ConcordanceQuery {
        query: "the".to_string(),
        case_sensitive: false,
        selection: AnalysisDocumentSelection {
            all_documents: true,
            ids: Vec::new(),
            excluded_ids: Vec::new(),
        },
        context_filter: None,
        context_filter_direction: crate::analysis::concordance::ConcordanceContextFilterDirection::Both,
        sort_key: ConcordanceSortKey::Node,
        window_tokens: 5,
        offset: 0,
        limit: 100,
        distribution_offset: 0,
        distribution_limit: 100,
    };

    let start = Instant::now();
    let result = search_concordance_documents_routed_provider(
        doc_len,
        &query,
        &config,
        &mut context,
        |idx| Some(documents[idx].clone()),
        |progress| {
            if progress.current_phase == ConcordancePhase::Searching && progress.files_done == 0 {
                println!("  [Progress] Phase: {:?}", progress.current_phase);
            }
        },
    ).expect("Concordance search failed");

    let duration = start.elapsed();
    println!("Concordance Search completed in {:?}", duration);
    println!("Hits: {}", result.result.total_hits);
    println!("Index rebuilt: {}", result.index_rebuilt);
    
    // Test Collocations
    let col_query = ConcordanceCollocationQuery {
        query: "the".to_string(),
        case_sensitive: false,
        selection: AnalysisDocumentSelection {
            all_documents: true,
            ids: Vec::new(),
            excluded_ids: Vec::new(),
        },
        span: 5,
        language: None,
        direction: crate::analysis::collocation::ConcordanceCollocationDirection::Both,
        min_cooccurrence: 1,
        min_collocate_frequency: 1,
        hide_stopwords: false,
        sort_measure: crate::analysis::collocation::ConcordanceCollocationMeasure::LogDice,
        sort_key: crate::analysis::collocation::ConcordanceCollocationSortKey::Association,
        sort_direction: crate::analysis::frequency::SortDirection::Desc,
        limit: 100,
    };

    let cancel = SurfaceIndexCancellationToken::default();
    
    let col_start = Instant::now();
    let col_result = aggregate_concordance_collocations_routed_provider(
        doc_len,
        &col_query,
        &config,
        &mut context,
        &cancel,
        |idx| Some(documents[idx].clone()),
        |_progress| {},
    ).expect("Collocation aggregation failed");

    let col_duration = col_start.elapsed();
    println!("Collocation Aggregation completed in {:?}", col_duration);
    println!("Collocates: {}", col_result.aggregate.candidate_counts.len());
    println!("Total occurrences: {}", col_result.aggregate.n);
    
    // Now test warm cache performance
    println!("Running Warm Cache Search...");
    let warm_start = Instant::now();
    let warm_result = search_concordance_documents_routed_provider(
        doc_len,
        &query,
        &config,
        &mut context,
        |idx| Some(documents[idx].clone()),
        |_progress| {},
    ).expect("Warm Concordance search failed");
    let warm_duration = warm_start.elapsed();
    
    println!("Warm Concordance Search completed in {:?}", warm_duration);
    println!("Index rebuilt: {}", warm_result.index_rebuilt);
}
