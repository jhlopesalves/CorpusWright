use super::*;
use crate::analysis::test_support::*;
use crate::decode::{DecodeStrategy, SourceEncoding, decode_source_bytes};
use crate::index::{InMemoryIndexStore, IndexStore, SurfaceIndexDocumentInput};
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
fn routed_index_falls_back_for_context_filter_and_matches_scan() {
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

    assert_eq!(
        routed.backend_used,
        ConcordanceBackendUsed::ScanFallback {
            reason: ConcordanceFallbackReason::UnsupportedFilter,
        }
    );
    assert_eq!(routed.result, scan);
    assert_eq!(context.index_build_count(), 0);
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
        let record = records
            .iter()
            .find(|record| record.surface == expected_surface)
            .unwrap();
        let recovered = decoded
            .text
            .chars()
            .skip(record.prepared_char_start)
            .take(record.prepared_char_end - record.prepared_char_start)
            .collect::<String>();
        assert_eq!(recovered, expected_surface);
    }
    let acao = records
        .iter()
        .find(|record| record.surface == "ação")
        .unwrap();
    assert_eq!((acao.prepared_char_start, acao.prepared_char_end), (5, 9));

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
