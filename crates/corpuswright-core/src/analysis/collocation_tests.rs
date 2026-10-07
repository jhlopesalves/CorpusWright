use super::*;
use crate::analysis::concordance::analysis_word_tokens;
use crate::analysis::concordance::index::{
    InMemoryIndexStore, IndexStore, SurfaceIndexCancellationToken, SurfaceIndexDocumentInput,
    SurfaceIndexQuery, search_surface_index,
};
use crate::analysis::test_support::*;
use crate::analysis::{ConcordanceBackend, aggregate_concordance_collocations_documents_routed};
use crate::text::tokenisation::TokenizationConfig;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn collocation_routing_types_remain_rust_only_without_ts_export() {
    let source = include_str!("collocation.rs");
    let start = source
        .find("pub struct RoutedConcordanceCollocationAggregate")
        .expect("collocation routing type source should exist");
    let end = source
        .find("impl Default for ConcordanceCollocationDistanceCounts")
        .expect("collocation aggregate implementation source should exist");
    let routing_source = &source[start..end];

    assert!(!routing_source.contains("#[ts(export)]"));
    assert!(!routing_source.contains("TS"));
}

#[test]
fn collocation_hides_stopwords_multilingual() {
    let docs = [concordance_document(0, "doc", "alpha the le como alpha")];

    // English
    let mut en_query = collocation_query("alpha");
    en_query.hide_stopwords = true;
    en_query.language = Some("en".to_string());
    let en_rows = naive_collocations_for(&docs, &en_query).unwrap().rows;
    assert!(!en_rows.iter().any(|r| r.surface == "the"));
    assert!(en_rows.iter().any(|r| r.surface == "le"));
    assert!(en_rows.iter().any(|r| r.surface == "como"));

    // French
    let mut fr_query = collocation_query("alpha");
    fr_query.hide_stopwords = true;
    fr_query.language = Some("fr".to_string());
    let fr_rows = naive_collocations_for(&docs, &fr_query).unwrap().rows;
    assert!(!fr_rows.iter().any(|r| r.surface == "le"));
    assert!(fr_rows.iter().any(|r| r.surface == "the"));

    // Portuguese
    let mut pt_query = collocation_query("alpha");
    pt_query.hide_stopwords = true;
    pt_query.language = Some("pt".to_string());
    let pt_rows = naive_collocations_for(&docs, &pt_query).unwrap().rows;
    assert!(!pt_rows.iter().any(|r| r.surface == "como"));
    assert!(pt_rows.iter().any(|r| r.surface == "the"));
}

fn naive_collocations_for(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
) -> Result<ConcordanceCollocationResult, ConcordanceError> {
    let tokenization_config = TokenizationConfig::default();
    let (signature, query_keys, query_key_set) =
        resolve_concordance_collocation_aggregation(query, &tokenization_config)?;
    let selected_document_ids = selected_concordance_document_ids(documents, &query.selection);
    let span = concordance_collocation_span(query.span);
    let direction = query.direction;
    let mut collocate_frequencies = HashMap::<String, u64>::new();
    let mut candidate_counts = HashMap::<String, ConcordanceCollocationDistanceCounts>::new();
    let mut n = 0u64;
    let mut r1 = 0u64;
    let mut f_node = 0u64;

    for document in documents {
        if !selected_document_ids.contains(&document.document_index) {
            continue;
        }
        let tokens = analysis_word_tokens(
            &document.text,
            document.document_index as u32,
            &tokenization_config,
        );
        let token_keys = tokens
            .iter()
            .map(|token| concordance_key(token.surface, query.case_sensitive))
            .collect::<Vec<_>>();
        n += token_keys.len() as u64;

        for key in &token_keys {
            *collocate_frequencies.entry(key.clone()).or_default() += 1;
        }

        for token_index in 0..token_keys.len() {
            let node_end = token_index + query_keys.len();
            if node_end > token_keys.len() {
                break;
            }
            let node_matches = token_keys[token_index..node_end]
                .iter()
                .zip(&query_keys)
                .all(|(token_key, query_key)| token_key == query_key);
            if !node_matches {
                continue;
            }

            f_node += 1;
            if matches!(
                direction,
                ConcordanceCollocationDirection::Left | ConcordanceCollocationDirection::Both
            ) {
                let left_start = token_index.saturating_sub(span);
                r1 += (token_index - left_start) as u64;
                for (candidate_index, candidate) in token_keys
                    .iter()
                    .enumerate()
                    .take(token_index)
                    .skip(left_start)
                {
                    if query_key_set.contains(candidate) {
                        continue;
                    }
                    candidate_counts.entry(candidate.clone()).or_default().add(
                        CollocationWindowSide::Left,
                        token_index - candidate_index,
                        1,
                    );
                }
            }
            if matches!(
                direction,
                ConcordanceCollocationDirection::Right | ConcordanceCollocationDirection::Both
            ) {
                let right_end = (node_end + span).min(token_keys.len());
                r1 += (right_end - node_end) as u64;
                for (candidate_index, candidate) in
                    token_keys.iter().enumerate().take(right_end).skip(node_end)
                {
                    if query_key_set.contains(candidate) {
                        continue;
                    }
                    candidate_counts.entry(candidate.clone()).or_default().add(
                        CollocationWindowSide::Right,
                        candidate_index - node_end + 1,
                        1,
                    );
                }
            }
        }
    }

    let limit = concordance_collocation_limit(query.limit);
    let mut rows = candidate_counts
        .into_iter()
        .filter_map(|(surface, counts)| {
            if query.hide_stopwords
                && crate::analysis::shared::stopword_matches(
                    &surface,
                    crate::analysis::shared::StopwordLanguage::from_subtag(
                        query.language.as_deref(),
                    ),
                )
            {
                return None;
            }
            let collocate_frequency = *collocate_frequencies.get(&surface).unwrap_or(&0);
            let o11 = counts.sum(span, direction);
            if o11 < query.min_cooccurrence || collocate_frequency < query.min_collocate_frequency {
                return None;
            }
            Some(collocation_row(
                surface,
                o11,
                collocate_frequency,
                r1,
                n,
                counts.first_five(span, direction),
            ))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        compare_collocation_rows(
            left,
            right,
            query.sort_key,
            query.sort_measure,
            query.sort_direction,
        )
    });
    let total_candidates = rows.len();
    rows.truncate(limit);

    Ok(ConcordanceCollocationResult {
        query: signature.query,
        case_sensitive: signature.case_sensitive,
        span,
        direction,
        min_cooccurrence: query.min_cooccurrence,
        min_collocate_frequency: query.min_collocate_frequency,
        hide_stopwords: query.hide_stopwords,
        sort_measure: query.sort_measure,
        sort_key: query.sort_key,
        sort_direction: query.sort_direction,
        limit,
        r1,
        n,
        f_node,
        total_candidates,
        rows,
    })
}

fn assert_collocation_matches_naive(
    documents: &[ConcordanceDocument],
    query: ConcordanceCollocationQuery,
) {
    let naive = naive_collocations_for(documents, &query).unwrap();
    let aggregate = collocation_aggregate_for(documents, &query).unwrap();
    let from_aggregate = query_concordance_collocations_from_aggregate(&aggregate, &query);
    assert_eq!(
        from_aggregate, naive,
        "query={} span={} direction={:?} case_sensitive={}",
        query.query, query.span, query.direction, query.case_sensitive
    );
}

struct CollocationIndexFixture {
    name: &'static str,
    documents: Vec<ConcordanceDocument>,
    query: ConcordanceCollocationQuery,
    representative_span: usize,
    representative_direction: ConcordanceCollocationDirection,
}

fn collocation_index_fixtures() -> Vec<CollocationIndexFixture> {
    let mut empty_selection = collocation_query("target");
    empty_selection.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: Vec::new(),
        excluded_ids: Vec::new(),
    };

    vec![
        CollocationIndexFixture {
            name: "english",
            documents: vec![
                concordance_document(
                    0,
                    "en-a.txt",
                    "Movie movie MOVIE good movie good film movie ending credits",
                ),
                concordance_document(
                    1,
                    "en-b.txt",
                    "A good movie is not the same as a Movie but movie fans return",
                ),
            ],
            query: collocation_query("movie"),
            representative_span: 5,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
        CollocationIndexFixture {
            name: "portuguese",
            documents: vec![
                concordance_document(
                    0,
                    "pt-a.txt",
                    "ação Ação coração Coração não Não João joão maçã Maçã",
                ),
                concordance_document(
                    1,
                    "pt-b.txt",
                    "A ação toca o coração e João diz não quando a ação termina",
                ),
            ],
            query: collocation_query("ação"),
            representative_span: 4,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
        CollocationIndexFixture {
            name: "overlap",
            documents: vec![concordance_document(0, "overlap.txt", "a a a a a")],
            query: collocation_query("a a"),
            representative_span: 2,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
        CollocationIndexFixture {
            name: "multi-token",
            documents: vec![concordance_document(
                0,
                "phrase.txt",
                "pre alpha beta post mid alpha beta tail",
            )],
            query: collocation_query("alpha beta"),
            representative_span: 1,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
        CollocationIndexFixture {
            name: "boundary",
            documents: vec![concordance_document(
                0,
                "edges.txt",
                "target start middle end target",
            )],
            query: collocation_query("target"),
            representative_span: 3,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
        CollocationIndexFixture {
            name: "empty-selection",
            documents: vec![concordance_document(
                0,
                "empty-selection.txt",
                "left target right",
            )],
            query: empty_selection,
            representative_span: 2,
            representative_direction: ConcordanceCollocationDirection::Both,
        },
    ]
}

#[test]
fn surface_collocation_index_aggregate_matches_scan_matrix() {
    let tokenization_config = TokenizationConfig::default();

    for fixture in collocation_index_fixtures() {
        for case_sensitive in [false, true] {
            let mut query = fixture.query.clone();
            query.case_sensitive = case_sensitive;
            let scan = aggregate_concordance_collocations_documents(
                &fixture.documents,
                &query,
                &tokenization_config,
                SurfaceIndexCancellationToken::default(),
            )
            .unwrap();
            let mut context = ConcordanceSearchContext::default();
            let index = aggregate_concordance_collocations_documents_routed(
                &fixture.documents,
                &query,
                &tokenization_config,
                &mut context,
                ConcordanceBackend::Index,
                &SurfaceIndexCancellationToken::default(),
            )
            .unwrap();

            let scan_result = query_concordance_collocations_from_aggregate(&scan, &query);
            let index_result =
                query_concordance_collocations_from_aggregate(&index.aggregate, &query);

            assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
            assert_eq!(
                index_result, scan_result,
                "{} case_sensitive={case_sensitive}",
                fixture.name
            );
        }

        let mut representative = fixture.query.clone();
        representative.span = fixture.representative_span;
        representative.direction = fixture.representative_direction;
        let mut context = ConcordanceSearchContext::default();
        let index = aggregate_concordance_collocations_documents_routed(
            &fixture.documents,
            &representative,
            &tokenization_config,
            &mut context,
            ConcordanceBackend::Index,
            &SurfaceIndexCancellationToken::default(),
        )
        .unwrap();
        let from_index =
            query_concordance_collocations_from_aggregate(&index.aggregate, &representative);
        let naive = naive_collocations_for(&fixture.documents, &representative).unwrap();

        assert_eq!(
            from_index, naive,
            "{} representative span={} direction={:?}",
            fixture.name, representative.span, representative.direction
        );
    }
}

#[test]
fn surface_collocation_index_phrase_hits_match_overlapping_scan_nodes() {
    let documents = vec![concordance_document(0, "overlap.txt", "a a a a a")];
    let query = collocation_query("a a");
    let tokenization_config = TokenizationConfig::default();
    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "overlap.txt".to_string(),
        display_name: "overlap.txt".to_string(),
        prepared_text: documents[0].text.as_str(),
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let index_query =
        SurfaceIndexQuery::from_surface_text_with_case("a a", false, &tokenization_config).unwrap();
    let hits = search_surface_index(&store, &index_query).unwrap();

    assert_eq!(
        hits.hits
            .iter()
            .map(|hit| (hit.document_id, hit.token_ordinal_in_document))
            .collect::<Vec<_>>(),
        [(0, 0), (0, 1), (0, 2), (0, 3)]
    );

    let scan = aggregate_concordance_collocations_documents(
        &documents,
        &query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let (index, _) = aggregate_concordance_collocations_index(
        &store,
        &query,
        &tokenization_config,
        crate::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
    )
    .unwrap();

    assert_eq!(scan.f_node, 4);
    assert_eq!(index.f_node, 4);
    assert_eq!(
        query_concordance_collocations_from_aggregate(&index, &query),
        query_concordance_collocations_from_aggregate(&scan, &query),
    );
}

#[test]
fn surface_collocation_and_concordance_share_warm_index_context_in_both_orders() {
    let documents = vec![
        concordance_document(0, "a.txt", "Movie movie good movie"),
        concordance_document(1, "b.txt", "Good movie"),
    ];
    let tokenization_config = TokenizationConfig::default();
    let concordance = parity_concordance_query("movie", false);
    let collocation = collocation_query("good movie");

    let mut concordance_first_context = ConcordanceSearchContext::default();
    let concordance_first = crate::analysis::search_concordance_documents_routed_provider(
        documents.len(),
        &concordance,
        &tokenization_config,
        &mut concordance_first_context,
        |document_index| Some(documents[document_index].clone()),
        |_| {},
    )
    .unwrap();
    let collocation_second = aggregate_concordance_collocations_documents_routed(
        &documents,
        &collocation,
        &tokenization_config,
        &mut concordance_first_context,
        ConcordanceBackend::Index,
        &SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    assert_eq!(
        concordance_first.backend_used,
        ConcordanceBackendUsed::Index
    );
    assert_eq!(
        collocation_second.backend_used,
        ConcordanceBackendUsed::Index
    );
    assert_eq!(concordance_first_context.index_build_count(), 1);

    let mut collocation_first_context = ConcordanceSearchContext::default();
    let collocation_first = aggregate_concordance_collocations_documents_routed(
        &documents,
        &collocation,
        &tokenization_config,
        &mut collocation_first_context,
        ConcordanceBackend::Index,
        &SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let concordance_second = crate::analysis::search_concordance_documents_routed_provider(
        documents.len(),
        &concordance,
        &tokenization_config,
        &mut collocation_first_context,
        |document_index| Some(documents[document_index].clone()),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        collocation_first.backend_used,
        ConcordanceBackendUsed::Index
    );
    assert_eq!(
        concordance_second.backend_used,
        ConcordanceBackendUsed::Index
    );
    assert_eq!(collocation_first_context.index_build_count(), 1);
}

#[test]
fn surface_collocation_index_error_falls_back_to_scan_aggregate() {
    let documents = vec![
        concordance_document(0, "a.txt", "left target right"),
        concordance_document(0, "duplicate.txt", "other target side"),
    ];
    let query = collocation_query("target");
    let tokenization_config = TokenizationConfig::default();
    let scan = aggregate_concordance_collocations_documents(
        &documents,
        &query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let mut context = ConcordanceSearchContext::default();

    let routed = aggregate_concordance_collocations_documents_routed(
        &documents,
        &query,
        &tokenization_config,
        &mut context,
        ConcordanceBackend::Index,
        &SurfaceIndexCancellationToken::default(),
    )
    .unwrap();

    assert_eq!(
        routed.backend_used,
        ConcordanceBackendUsed::ScanFallback {
            reason: ConcordanceFallbackReason::IndexError,
        }
    );
    assert_eq!(routed.aggregate, scan);
    assert_eq!(context.index_build_count(), 0);
}

#[test]
fn surface_collocation_distance_aggregate_matches_naive_reference_matrix() {
    let fixtures = [
        (
            "english",
            vec![
                concordance_document(
                    0,
                    "en-a.txt",
                    "Movie movie MOVIE good movie good film movie ending credits",
                ),
                concordance_document(
                    1,
                    "en-b.txt",
                    "A good movie is not the same as a Movie but movie fans return",
                ),
            ],
            "movie",
        ),
        (
            "portuguese",
            vec![
                concordance_document(
                    0,
                    "pt-a.txt",
                    "ação Ação coração Coração não Não João joão maçã Maçã",
                ),
                concordance_document(
                    1,
                    "pt-b.txt",
                    "A ação toca o coração e João diz não quando a ação termina",
                ),
            ],
            "ação",
        ),
    ];

    for (fixture_name, documents, query_text) in fixtures {
        for span in [1, 2, 5, 19, 20] {
            for direction in [
                ConcordanceCollocationDirection::Left,
                ConcordanceCollocationDirection::Right,
                ConcordanceCollocationDirection::Both,
            ] {
                for case_sensitive in [true, false] {
                    let mut query = collocation_query(query_text);
                    query.span = span;
                    query.direction = direction;
                    query.case_sensitive = case_sensitive;
                    let naive = naive_collocations_for(&documents, &query).unwrap();
                    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();
                    let from_aggregate =
                        query_concordance_collocations_from_aggregate(&aggregate, &query);
                    assert_eq!(
                        from_aggregate, naive,
                        "{fixture_name} span={span} direction={direction:?} case_sensitive={case_sensitive}"
                    );
                }
            }
        }
    }
}

#[test]
fn surface_collocation_distance_aggregate_matches_naive_edge_cases() {
    let directions = [
        ConcordanceCollocationDirection::Left,
        ConcordanceCollocationDirection::Right,
        ConcordanceCollocationDirection::Both,
    ];
    let cases = [
        (
            vec![concordance_document(
                0,
                "edges.txt",
                "target start middle end target",
            )],
            "target",
            3,
        ),
        (
            vec![concordance_document(
                0,
                "phrase.txt",
                "pre alpha beta post mid alpha beta tail",
            )],
            "alpha beta",
            2,
        ),
        (
            vec![concordance_document(0, "overlap.txt", "a a a a a")],
            "a a",
            2,
        ),
        (
            vec![concordance_document(0, "short.txt", "left target right")],
            "target",
            99,
        ),
    ];

    for (documents, query_text, span) in cases {
        for direction in directions {
            let mut query = collocation_query(query_text);
            query.span = span;
            query.direction = direction;
            assert_collocation_matches_naive(&documents, query);
        }
    }

    let documents = vec![concordance_document(
        0,
        "empty-selection.txt",
        "left target right",
    )];
    let mut query = collocation_query("target");
    query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: Vec::new(),
        excluded_ids: Vec::new(),
    };
    let result = collocations_for(&documents, query.clone()).unwrap();
    assert_eq!(result, naive_collocations_for(&documents, &query).unwrap());
    assert_eq!(result.n, 0);
    assert_eq!(result.r1, 0);
    assert_eq!(result.f_node, 0);
    assert!(result.rows.is_empty());
}

#[test]
fn collocate_position_counts_expose_exact_bounded_bins() {
    let documents = vec![concordance_document(
        0,
        "positions.txt",
        "alpha beta alpha gamma alpha target alpha delta alpha epsilon alpha",
    )];
    let mut query = collocation_query("target");
    query.span = 5;
    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();

    let both = query_concordance_collocations_from_aggregate(&aggregate, &query);
    let alpha = collocate(&both, "alpha");
    assert_eq!(alpha.o11, 6);
    assert_eq!(alpha.position_counts.left, [1, 0, 1, 0, 1]);
    assert_eq!(alpha.position_counts.right, [1, 0, 1, 0, 1]);

    let mut left_query = query.clone();
    left_query.direction = ConcordanceCollocationDirection::Left;
    let left = query_concordance_collocations_from_aggregate(&aggregate, &left_query);
    let alpha = collocate(&left, "alpha");
    assert_eq!(alpha.o11, 3);
    assert_eq!(alpha.position_counts.left, [1, 0, 1, 0, 1]);
    assert_eq!(alpha.position_counts.right, [0, 0, 0, 0, 0]);

    let mut right_query = query.clone();
    right_query.direction = ConcordanceCollocationDirection::Right;
    let right = query_concordance_collocations_from_aggregate(&aggregate, &right_query);
    let alpha = collocate(&right, "alpha");
    assert_eq!(alpha.o11, 3);
    assert_eq!(alpha.position_counts.left, [0, 0, 0, 0, 0]);
    assert_eq!(alpha.position_counts.right, [1, 0, 1, 0, 1]);

    let mut span_two_query = query.clone();
    span_two_query.span = 2;
    let span_two = query_concordance_collocations_from_aggregate(&aggregate, &span_two_query);
    let alpha = collocate(&span_two, "alpha");
    assert_eq!(alpha.o11, 2);
    assert_eq!(alpha.position_counts.left, [1, 0, 0, 0, 0]);
    assert_eq!(alpha.position_counts.right, [1, 0, 0, 0, 0]);
}

#[test]
fn collocate_position_counts_follow_file_selection() {
    let documents = vec![
        concordance_document(
            0,
            "excluded.txt",
            "alpha beta alpha target alpha beta alpha",
        ),
        concordance_document(1, "selected.txt", "omega target alpha beta"),
    ];
    let mut query = collocation_query("target");
    query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![1],
        excluded_ids: Vec::new(),
    };
    query.span = 5;

    let result = collocations_for(&documents, query.clone()).unwrap();
    assert_eq!(result, naive_collocations_for(&documents, &query).unwrap());
    let alpha = collocate(&result, "alpha");
    assert_eq!(alpha.o11, 1);
    assert_eq!(alpha.position_counts.left, [0, 0, 0, 0, 0]);
    assert_eq!(alpha.position_counts.right, [1, 0, 0, 0, 0]);
}

#[test]
fn surface_collocation_parallel_aggregation_is_deterministic() {
    let documents = vec![
        concordance_document(
            0,
            "a.txt",
            "red alpha blue target green alpha blue target yellow",
        ),
        concordance_document(
            1,
            "b.txt",
            "yellow target blue green target alpha blue green",
        ),
        concordance_document(
            2,
            "c.txt",
            "target red target blue target green target alpha",
        ),
    ];
    let mut query = collocation_query("target");
    query.span = 20;

    let default_pool = collocation_aggregate_for(&documents, &query).unwrap();
    let single_thread_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| collocation_aggregate_for(&documents, &query).unwrap());

    assert_eq!(default_pool, single_thread_pool);
}

fn docs_fixture_documents() -> Vec<ConcordanceDocument> {
    let docs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .join("docs");
    let mut paths = std::fs::read_dir(&docs_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("html" | "md")
            )
        })
        .collect::<Vec<_>>();
    paths.sort();

    paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let text = std::fs::read_to_string(&path).unwrap();
            let display_name = path.file_name().unwrap().to_string_lossy();
            concordance_document(index, &display_name, &text)
        })
        .collect()
}

#[test]
#[ignore = "prints local timing evidence for the collocation aggregate cache path"]
fn surface_collocation_performance_evidence_on_docs_fixture() {
    let documents = docs_fixture_documents();
    let mut query = collocation_query("corpus");
    query.span = 5;
    query.limit = 500;

    let direct_cold_start = std::time::Instant::now();
    let direct_cold = naive_collocations_for(&documents, &query).unwrap();
    let direct_cold_ms = direct_cold_start.elapsed().as_secs_f64() * 1000.0;

    let cold_start = std::time::Instant::now();
    let aggregate = collocation_aggregate_for(&documents, &query).unwrap();
    let cold = query_concordance_collocations_from_aggregate(&aggregate, &query);
    let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cold, direct_cold);

    let tokenization_config = TokenizationConfig::default();
    let mut warm_context = ConcordanceSearchContext::default();
    let warm_seed = aggregate_concordance_collocations_documents_routed(
        &documents,
        &query,
        &tokenization_config,
        &mut warm_context,
        ConcordanceBackend::Index,
        &SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    assert!(warm_seed.index_rebuilt);
    assert_eq!(warm_seed.aggregate, aggregate);

    let mut span_query = query.clone();
    span_query.span = 20;
    let direct_span_start = std::time::Instant::now();
    let direct_span = naive_collocations_for(&documents, &span_query).unwrap();
    let direct_span_ms = direct_span_start.elapsed().as_secs_f64() * 1000.0;
    let cached_span_start = std::time::Instant::now();
    let cached_span = query_concordance_collocations_from_aggregate(&aggregate, &span_query);
    let cached_span_ms = cached_span_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cached_span, direct_span);

    let mut direction_query = query.clone();
    direction_query.direction = ConcordanceCollocationDirection::Left;
    let direct_direction_start = std::time::Instant::now();
    let direct_direction = naive_collocations_for(&documents, &direction_query).unwrap();
    let direct_direction_ms = direct_direction_start.elapsed().as_secs_f64() * 1000.0;
    let cached_direction_start = std::time::Instant::now();
    let cached_direction =
        query_concordance_collocations_from_aggregate(&aggregate, &direction_query);
    let cached_direction_ms = cached_direction_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(cached_direction, direct_direction);

    let mut new_query = query.clone();
    new_query.query = "analysis".to_string();
    let direct_new_query_start = std::time::Instant::now();
    let direct_new_query = naive_collocations_for(&documents, &new_query).unwrap();
    let direct_new_query_ms = direct_new_query_start.elapsed().as_secs_f64() * 1000.0;
    let new_query_scan_aggregate_start = std::time::Instant::now();
    let new_query_aggregate = collocation_aggregate_for(&documents, &new_query).unwrap();
    let new_query_scan_aggregate_ms =
        new_query_scan_aggregate_start.elapsed().as_secs_f64() * 1000.0;
    let new_query_warm_index_start = std::time::Instant::now();
    let new_query_index = aggregate_concordance_collocations_documents_routed(
        &documents,
        &new_query,
        &tokenization_config,
        &mut warm_context,
        ConcordanceBackend::Index,
        &SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let new_query_warm_index_ms = new_query_warm_index_start.elapsed().as_secs_f64() * 1000.0;
    assert!(!new_query_index.index_rebuilt);
    assert_eq!(new_query_index.aggregate, new_query_aggregate);
    let new_query_result =
        query_concordance_collocations_from_aggregate(&new_query_index.aggregate, &new_query);
    assert_eq!(new_query_result, direct_new_query);

    let candidate_count = aggregate.candidate_counts.len();
    let distance_value_bytes =
        candidate_count * std::mem::size_of::<ConcordanceCollocationDistanceCounts>();
    eprintln!(
        "docs fixture: documents={} tokens={} candidates={} distance_value_bytes={}",
        documents.len(),
        cold.n,
        candidate_count,
        distance_value_bytes
    );
    eprintln!("first query: direct={direct_cold_ms:.3}ms aggregate+query={cold_ms:.3}ms");
    eprintln!("span change: direct={direct_span_ms:.3}ms cached_query={cached_span_ms:.3}ms");
    eprintln!(
        "direction change: direct={direct_direction_ms:.3}ms cached_query={cached_direction_ms:.3}ms"
    );
    eprintln!(
        "new query: direct={direct_new_query_ms:.3}ms scan_aggregate={new_query_scan_aggregate_ms:.3}ms warm_index_aggregate={new_query_warm_index_ms:.3}ms"
    );
}

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
fn association_scores_match_independent_numeric_witnesses() {
    // Fixed hand-calculated witnesses cover both attraction and depletion. The
    // final two values retain the inactive wire score and the window log ratio.
    let cases = [
        (
            (8, 16, 32, 128),
            [
                1.0,
                7.0,
                1.414213562373095,
                2.0,
                5.391203279261245,
                12.415037499278844,
                1.584962500721156,
            ],
        ),
        (
            (2, 32, 32, 128),
            [
                -2.0,
                0.0,
                -4.242640687119285,
                -2.1213203435596424,
                9.746546950594475,
                10.0,
                -2.321928094887362,
            ],
        ),
    ];
    for ((events, frequency, slots, tokens), expected) in cases {
        let row = collocation_row(
            "witness".to_string(),
            events,
            frequency,
            slots,
            tokens,
            ConcordanceCollocatePositionCounts {
                left: vec![0; 5],
                right: vec![0; 5],
            },
        );
        let actual = [
            row.mi,
            row.mi3,
            row.t_score,
            row.z_score,
            row.g2,
            row.log_dice,
            row.log_ratio,
        ];
        for (actual, expected) in actual.into_iter().zip(expected) {
            let actual = actual.expect("numeric witness should be available");
            assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
        }
    }
}

#[test]
fn association_scores_preserve_zero_and_unavailable_conventions() {
    let positions = || ConcordanceCollocatePositionCounts {
        left: vec![0; 5],
        right: vec![0; 5],
    };
    let empty = collocation_row("empty".to_string(), 0, 0, 0, 0, positions());
    assert_eq!(empty.mi, None);
    assert_eq!(empty.mi3, None);
    assert_eq!(empty.t_score, None);
    assert_eq!(empty.z_score, None);
    assert_eq!(empty.g2, None);
    assert_eq!(empty.log_dice, None);
    assert_eq!(empty.log_ratio, None);

    let zero_events = collocation_row("zero".to_string(), 0, 16, 32, 128, positions());
    assert_eq!(zero_events.mi, None);
    assert_eq!(zero_events.mi3, None);
    assert_eq!(zero_events.t_score, None);
    assert_eq!(zero_events.z_score, Some(-2.0));
    assert!(zero_events.g2.is_some());
    assert_eq!(zero_events.log_dice, None);
    assert_eq!(zero_events.log_ratio, None);

    // Zero observed and expected cells contribute zero; no pseudo-count is added.
    let complete = collocation_row("complete".to_string(), 1, 1, 1, 1, positions());
    assert_eq!(complete.mi, Some(0.0));
    assert_eq!(complete.g2, Some(0.0));
    assert_eq!(complete.log_ratio, None);
}

#[test]
fn overlapping_windows_count_events_without_fabricating_contingency_scores() {
    let documents = vec![concordance_document(0, "overlap.txt", "target blue target")];
    let mut query = collocation_query("target");
    query.span = 1;
    let result = collocations_for(&documents, query).unwrap();
    let blue = collocate(&result, "blue");
    assert_eq!((result.f_node, result.r1, result.n), (2, 2, 3));
    assert_eq!((blue.o11, blue.collocate_frequency, blue.o21), (2, 1, -1));
    assert!((blue.mi.unwrap() - 1.584962500721156).abs() < 1e-12);
    assert_eq!(blue.g2, None);
    assert_eq!(blue.log_ratio, None);
    // This retained legacy result exceeds the usual Dice maximum; it is not an
    // authoritative logDice choice in the product's association selector.
    assert!((blue.log_dice.unwrap() - 14.415037499278844).abs() < 1e-12);
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
fn cached_collocation_aggregate_derives_span_frequency_and_direction_views() {
    let documents = vec![
        concordance_document(
            0,
            "a.txt",
            "rare far near target common near target common common target far",
        ),
        concordance_document(1, "b.txt", "common target near common target rare"),
    ];
    let base_query = collocation_query("target");
    let aggregate = collocation_aggregate_for(&documents, &base_query).unwrap();

    let mut narrowed_span = base_query.clone();
    narrowed_span.span = 1;
    let mut raised_frequency = base_query.clone();
    raised_frequency.min_collocate_frequency = 3;
    raised_frequency.min_cooccurrence = 2;
    let mut single_direction = base_query.clone();
    single_direction.direction = ConcordanceCollocationDirection::Left;

    for query in [narrowed_span, raised_frequency, single_direction] {
        let cached = query_concordance_collocations_from_aggregate(&aggregate, &query);
        let direct = naive_collocations_for(&documents, &query).unwrap();
        assert_eq!(cached, direct);
    }
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
        "the target apple target the target apple",
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
    assert!(hidden.rows.iter().any(|row| row.surface == "apple"));
}

#[test]
fn surface_collocation_high_node_frequency_without_materializing_all_occurrences() {
    let mut text = String::new();
    for _ in 0..10_000 {
        text.push_str("target filler ");
    }
    let query = collocation_query("target");
    let tokenization_config = TokenizationConfig::default();

    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "doc.txt".to_string(),
        display_name: "doc.txt".to_string(),
        prepared_text: &text,
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let (_, stats) = aggregate_concordance_collocations_index(
        &store,
        &query,
        &tokenization_config,
        crate::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
    )
    .unwrap();

    assert_eq!(stats.valid_node_occurrences, 10_000);
    assert!(stats.peak_aggregate_entries < 10);
}

#[test]
fn surface_collocation_index_finalises_only_actual_candidates() {
    let selected = "alpha target beta alpha";
    let mut unrelated_vocabulary = String::new();
    for index in 0..10_000 {
        unrelated_vocabulary.push_str(&format!("vocabulary{index} "));
    }
    let tokenization_config = TokenizationConfig::default();
    let inputs = [
        SurfaceIndexDocumentInput {
            document_id: 0,
            source_path: "selected.txt".to_string(),
            display_name: "selected.txt".to_string(),
            prepared_text: selected,
        },
        SurfaceIndexDocumentInput {
            document_id: 1,
            source_path: "unselected.txt".to_string(),
            display_name: "unselected.txt".to_string(),
            prepared_text: &unrelated_vocabulary,
        },
    ];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let complete_vocabulary = store.concordance_symbols(false).count();
    let complete_symbol_bytes = store
        .concordance_symbols(false)
        .map(|symbol| symbol.len() as u64)
        .sum::<u64>();
    let mut query = collocation_query("target");
    query.selection = AnalysisDocumentSelection {
        all_documents: false,
        ids: vec![0],
        excluded_ids: Vec::new(),
    };

    let (aggregate, stats) = aggregate_concordance_collocations_index(
        &store,
        &query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let (repeat, repeat_stats) = aggregate_concordance_collocations_index(
        &store,
        &query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();

    assert!(complete_vocabulary > 10_000);
    assert_eq!(aggregate.symbols, ["alpha", "beta"]);
    assert_eq!(aggregate.symbols, repeat.symbols);
    assert_eq!(aggregate.candidate_counts, repeat.candidate_counts);
    assert_eq!(aggregate.collocate_frequencies.len(), 2);
    assert_eq!(stats.final_candidate_count, 2);
    assert_eq!(stats.aggregate_entries, 2);
    assert_eq!(stats.candidate_symbol_bytes, 9);
    assert!(stats.candidate_symbol_bytes < complete_symbol_bytes);
    assert_eq!(stats.final_projected_bytes, stats.projected_aggregate_bytes);
    assert!(stats.peak_projected_bytes >= stats.final_projected_bytes);
    assert_eq!(
        stats.final_projected_bytes,
        repeat_stats.final_projected_bytes
    );

    let result = query_concordance_collocations_from_aggregate(&aggregate, &query);
    assert_eq!(
        result
            .rows
            .iter()
            .find(|row| row.surface == "alpha")
            .unwrap()
            .collocate_frequency,
        2,
    );
    assert_eq!(
        result
            .rows
            .iter()
            .find(|row| row.surface == "beta")
            .unwrap()
            .collocate_frequency,
        1,
    );
}

#[test]
fn surface_collocation_final_strings_materialised_only_for_returned_rows() {
    let documents = [concordance_document(
        0,
        "a.txt",
        "one target two target three target four target five target six",
    )];
    let mut query = collocation_query("target");
    query.limit = 2;
    let tokenization_config = TokenizationConfig::default();

    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "a.txt".to_string(),
        display_name: "a.txt".to_string(),
        prepared_text: documents[0].text.as_str(),
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let (aggregate, _) = aggregate_concordance_collocations_index(
        &store,
        &query,
        &tokenization_config,
        crate::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let result = super::query_concordance_collocations_from_aggregate(&aggregate, &query);

    assert_eq!(result.rows.len(), 2);
}

#[test]
fn surface_collocation_vocabulary_sized_aggregate_resource_limit_failure() {
    let limits = crate::analysis::concordance::index::SurfaceIndexResourceLimits {
        max_projected_resident_index_bytes: 1,
        ..Default::default()
    };
    let mut builder = super::NumericCollocationBuilder::new(
        limits,
        crate::analysis::concordance::index::SurfaceIndexCancellationToken::default(),
    );
    let err = builder.intern("any").unwrap_err();
    assert!(matches!(
        err,
        ConcordanceError::Index(
            crate::analysis::concordance::index::SurfaceIndexError::ProjectedResidentIndexExceedsLimit { .. }
        )
    ));
}

#[test]
fn surface_collocation_index_refuses_candidate_growth_before_budget() {
    let text = "alpha target beta";
    let tokenization_config = TokenizationConfig::default();
    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "doc.txt".to_string(),
        display_name: "doc.txt".to_string(),
        prepared_text: text,
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let limits = crate::analysis::concordance::index::SurfaceIndexResourceLimits {
        max_projected_resident_index_bytes: 1,
        ..Default::default()
    };
    let error = aggregate_concordance_collocations_index_with_limits(
        &store,
        &collocation_query("target"),
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
        limits,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ConcordanceError::Index(
            crate::analysis::concordance::index::SurfaceIndexError::ProjectedResidentIndexExceedsLimit {
                projected_bytes,
                limit_bytes: 1,
            }
        ) if projected_bytes > 1
    ));
}

#[test]
fn concordance_analysis_memory_admission_is_combined_and_underflow_safe() {
    let limits = ConcordanceAnalysisMemoryLimits {
        max_combined_bytes: 1_000,
        safety_margin_bytes: 100,
    };
    let admitted = ConcordanceAnalysisMemoryEstimate::admit(800, 75, limits).unwrap();
    assert_eq!(admitted.remaining_collocation_bytes, 100);
    assert_eq!(admitted.combined_projected_bytes, 975);

    let near_limit = ConcordanceAnalysisMemoryEstimate::admit(901, 0, limits).unwrap_err();
    assert!(matches!(
        near_limit,
        ConcordanceError::AnalysisMemoryLimitExceeded {
            estimated_resident_index_bytes: 901,
            projected_aggregate_bytes: 0,
            combined_limit_bytes: 1_000,
            safety_margin_bytes: 100,
        }
    ));

    let aggregate_too_large =
        ConcordanceAnalysisMemoryEstimate::admit(800, 101, limits).unwrap_err();
    assert!(matches!(
        aggregate_too_large,
        ConcordanceError::AnalysisMemoryLimitExceeded {
            projected_aggregate_bytes: 101,
            ..
        }
    ));

    let arithmetic_underflow = ConcordanceAnalysisMemoryEstimate::admit(
        u64::MAX,
        u64::MAX,
        ConcordanceAnalysisMemoryLimits {
            max_combined_bytes: 1,
            safety_margin_bytes: u64::MAX,
        },
    )
    .unwrap_err();
    assert!(matches!(
        arithmetic_underflow,
        ConcordanceError::AnalysisMemoryLimitExceeded { .. }
    ));
}

#[test]
fn surface_collocation_combined_budget_admits_small_and_refuses_growth() {
    let text = "alpha target beta alpha";
    let tokenization_config = TokenizationConfig::default();
    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "doc.txt".to_string(),
        display_name: "doc.txt".to_string(),
        prepared_text: text,
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let index_bytes = store.estimated_resident_bytes();
    let surface_limits = crate::analysis::concordance::index::SurfaceIndexResourceLimits::default();
    let safety_margin_bytes = 1_024;
    let admitted_limit = index_bytes
        .saturating_add(safety_margin_bytes)
        .saturating_add(1_000_000);
    let (_, stats) = aggregate_concordance_collocations_index_with_memory_limits(
        &store,
        &collocation_query("target"),
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
        surface_limits,
        ConcordanceAnalysisMemoryLimits {
            max_combined_bytes: admitted_limit,
            safety_margin_bytes,
        },
    )
    .unwrap();
    assert_eq!(stats.estimated_resident_index_bytes, index_bytes);
    assert_eq!(stats.combined_limit_bytes, admitted_limit);
    assert_eq!(stats.safety_margin_bytes, safety_margin_bytes);
    assert!(stats.combined_projected_bytes <= admitted_limit);

    let refusal = aggregate_concordance_collocations_index_with_memory_limits(
        &store,
        &collocation_query("target"),
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
        surface_limits,
        ConcordanceAnalysisMemoryLimits {
            max_combined_bytes: index_bytes
                .saturating_add(safety_margin_bytes)
                .saturating_add(1),
            safety_margin_bytes,
        },
    )
    .unwrap_err();
    assert!(matches!(
        &refusal,
        ConcordanceError::AnalysisMemoryLimitExceeded {
            estimated_resident_index_bytes,
            projected_aggregate_bytes,
            safety_margin_bytes: 1_024,
            ..
        } if *estimated_resident_index_bytes == index_bytes && *projected_aggregate_bytes > 1
    ));
    assert!(refusal.prevents_unbounded_scan_fallback());
    let message = refusal.to_string();
    assert!(message.contains("Narrow the file scope"));
    assert!(message.contains("less frequent node"));
    assert!(!message.contains("add more RAM"));
}

#[test]
fn surface_collocation_cancellation_interrupts_frequency_finalisation() {
    let text = "alpha target alpha alpha";
    let tokenization_config = TokenizationConfig::default();
    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "doc.txt".to_string(),
        display_name: "doc.txt".to_string(),
        prepared_text: text,
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let source_id = store.concordance_symbol_id("alpha", false).unwrap();
    let mut candidates = IndexedCollocationCandidates::default();
    candidates.counts.insert(source_id, Default::default());
    candidates.peak_candidate_entries = 1;
    let cancellation = SurfaceIndexCancellationToken::default();

    let error = finalise_indexed_collocation_candidates(
        &store,
        ConcordanceCollocationAggregationSignature {
            query: "target".to_string(),
            case_sensitive: false,
        },
        &HashSet::from([0]),
        4,
        candidates,
        false,
        &cancellation,
        crate::analysis::concordance::index::SurfaceIndexResourceLimits::default(),
        || cancellation.cancel(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ConcordanceError::Index(crate::analysis::concordance::index::SurfaceIndexError::Cancelled)
    ));
}

fn collocation_scale_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[test]
#[ignore = "release-only synthetic evidence; configure with CORPUSWRIGHT_COLLOCATION_SCALE_* env vars"]
fn surface_collocation_scale_evidence() {
    const PROJECTED_TOKENS: u64 = 60_000_000;
    let token_count = collocation_scale_usize("CORPUSWRIGHT_COLLOCATION_SCALE_TOKENS", 5_000_000);
    let vocabulary_size =
        collocation_scale_usize("CORPUSWRIGHT_COLLOCATION_SCALE_VOCABULARY", 4_096);
    let candidate_percent =
        collocation_scale_usize("CORPUSWRIGHT_COLLOCATION_SCALE_CANDIDATE_PERCENT", 1).min(100);
    let tokens_per_document = collocation_scale_usize(
        "CORPUSWRIGHT_COLLOCATION_SCALE_TOKENS_PER_DOCUMENT",
        4_000_000,
    );
    assert!(token_count > vocabulary_size);
    assert!(vocabulary_size >= 4);
    let candidate_vocabulary = vocabulary_size
        .saturating_mul(candidate_percent)
        .div_ceil(100)
        .clamp(1, vocabulary_size - 1);
    let non_candidate_vocabulary = vocabulary_size - candidate_vocabulary;
    let vocabulary = (0..vocabulary_size)
        .map(|index| format!("v{index}"))
        .collect::<Vec<_>>();

    let generation_started = Instant::now();
    let document_count = token_count.div_ceil(tokens_per_document);
    let mut texts = Vec::with_capacity(document_count);
    let mut generated_tokens = 0usize;
    while generated_tokens < token_count {
        let document_tokens = (token_count - generated_tokens).min(tokens_per_document);
        let mut text = String::with_capacity(document_tokens.saturating_mul(8));
        for local_position in 0..document_tokens {
            let position = generated_tokens + local_position;
            let token = if position < vocabulary_size {
                vocabulary[position].as_str()
            } else {
                let phase = (position - vocabulary_size) % 64;
                if phase == 0 {
                    "commonnode"
                } else if phase == 32 && (position - vocabulary_size) % 1_024 == 32 {
                    "moderatenode"
                } else if phase <= 20 || phase >= 44 {
                    vocabulary[position % candidate_vocabulary].as_str()
                } else {
                    vocabulary[candidate_vocabulary + position % non_candidate_vocabulary].as_str()
                }
            };
            text.push_str(token);
            text.push(' ');
        }
        texts.push(text);
        generated_tokens += document_tokens;
    }
    let generation_elapsed = generation_started.elapsed();

    let tokenization_config = TokenizationConfig::default();
    let inputs = texts
        .iter()
        .enumerate()
        .map(|(document_id, text)| SurfaceIndexDocumentInput {
            document_id,
            source_path: format!("synthetic-scale-{document_id}.txt"),
            display_name: format!("synthetic-scale-{document_id}.txt"),
            prepared_text: text.as_str(),
        })
        .collect::<Vec<_>>();
    let index_started = Instant::now();
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
    let index_elapsed = index_started.elapsed();
    drop(inputs);
    drop(texts);

    let mut common_query = collocation_query("commonnode");
    common_query.limit = 500;
    let common_started = Instant::now();
    let (common_aggregate, common_stats) = aggregate_concordance_collocations_index(
        &store,
        &common_query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let common_elapsed = common_started.elapsed();
    let common_result =
        query_concordance_collocations_from_aggregate(&common_aggregate, &common_query);

    let mut moderate_query = collocation_query("moderatenode");
    moderate_query.limit = 500;
    let moderate_started = Instant::now();
    let (moderate_aggregate, moderate_stats) = aggregate_concordance_collocations_index(
        &store,
        &moderate_query,
        &tokenization_config,
        SurfaceIndexCancellationToken::default(),
    )
    .unwrap();
    let moderate_elapsed = moderate_started.elapsed();
    let moderate_result =
        query_concordance_collocations_from_aggregate(&moderate_aggregate, &moderate_query);

    let cached = Arc::new(common_aggregate);
    let cache_handle = Arc::clone(&cached);
    let cache_arc_strong_count = Arc::strong_count(&cached);
    assert!(Arc::ptr_eq(&cached, &cache_handle));
    let complete_vocabulary = store.concordance_symbols(false).count();
    let projected_candidate_symbol_bytes = vocabulary
        .iter()
        .take(candidate_vocabulary)
        .map(|symbol| symbol.len() as u64)
        .sum::<u64>();
    let projected_aggregate_bytes = indexed_final_aggregate_bytes(
        "commonnode".len() as u64,
        candidate_vocabulary,
        projected_candidate_symbol_bytes,
    );
    let compact_60m_token_array_floor =
        crate::analysis::concordance::index::SurfaceIndexResourceEstimate::compact_token_array_lower_bound(
            PROJECTED_TOKENS,
        )
        .unwrap();
    let legacy_60m_structural_floor =
        crate::analysis::concordance::index::SurfaceIndexResourceEstimate::legacy_structural_lower_bound(PROJECTED_TOKENS)
            .unwrap();

    eprintln!(
        "[collocation-scale] measured_tokens={token_count} projected_tokens={PROJECTED_TOKENS} measured_documents={document_count} tokens_per_document={tokens_per_document} vocabulary_configured={vocabulary_size} candidate_percent={candidate_percent} generation_ms={} index_ms={} common_ms={} moderate_ms={}",
        generation_elapsed.as_millis(),
        index_elapsed.as_millis(),
        common_elapsed.as_millis(),
        moderate_elapsed.as_millis(),
    );
    eprintln!(
        "[collocation-scale] index_estimated_resident_bytes={} complete_index_vocabulary={} compact_60m_token_array_floor={} legacy_60m_structural_floor={}",
        store.estimated_resident_bytes(),
        complete_vocabulary,
        compact_60m_token_array_floor,
        legacy_60m_structural_floor,
    );
    eprintln!(
        "[collocation-scale] common_f_node={} common_candidates={} common_candidate_symbol_bytes={} common_final_projected_bytes={} common_peak_projected_bytes={} common_rows={} cache_arc_strong_count={}",
        cached.f_node,
        common_stats.final_candidate_count,
        common_stats.candidate_symbol_bytes,
        common_stats.final_projected_bytes,
        common_stats.peak_projected_bytes,
        common_result.rows.len(),
        cache_arc_strong_count,
    );
    eprintln!(
        "[collocation-scale] moderate_f_node={} moderate_candidates={} moderate_candidate_symbol_bytes={} moderate_final_projected_bytes={} moderate_peak_projected_bytes={} moderate_rows={}",
        moderate_aggregate.f_node,
        moderate_stats.final_candidate_count,
        moderate_stats.candidate_symbol_bytes,
        moderate_stats.final_projected_bytes,
        moderate_stats.peak_projected_bytes,
        moderate_result.rows.len(),
    );
    eprintln!(
        "[collocation-scale] combined_index_bytes={} combined_peak_aggregate_bytes={} combined_safety_margin_bytes={} combined_projected_bytes={} combined_limit_bytes={} remaining_collocation_bytes={} projected_60m_candidate_aggregate_bytes={}",
        common_stats.estimated_resident_index_bytes,
        common_stats.peak_projected_bytes,
        common_stats.safety_margin_bytes,
        common_stats.combined_projected_bytes,
        common_stats.combined_limit_bytes,
        common_stats.remaining_collocation_bytes,
        projected_aggregate_bytes,
    );

    assert_eq!(store.total_indexed_tokens(), token_count);
    assert!(complete_vocabulary >= vocabulary_size);
    assert!(
        common_stats.final_candidate_count
            <= candidate_vocabulary + CONCORDANCE_COLLOCATION_MAX_SPAN
    );
    assert_eq!(cache_arc_strong_count, 2);
    assert!(common_stats.combined_projected_bytes <= common_stats.combined_limit_bytes);
    assert!(moderate_stats.combined_projected_bytes <= moderate_stats.combined_limit_bytes);
}

#[test]
fn surface_collocation_cancellation_interrupts_indexed_aggregation() {
    let documents = [concordance_document(0, "a.txt", "target collocate")];
    let query = collocation_query("target");
    let tokenization_config = TokenizationConfig::default();

    let inputs = [SurfaceIndexDocumentInput {
        document_id: 0,
        source_path: "a.txt".to_string(),
        display_name: "a.txt".to_string(),
        prepared_text: documents[0].text.as_str(),
    }];
    let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();

    let token = crate::analysis::concordance::index::SurfaceIndexCancellationToken::default();
    token.cancel();

    let err = aggregate_concordance_collocations_index(&store, &query, &tokenization_config, token)
        .unwrap_err();
    assert!(matches!(
        err,
        ConcordanceError::Index(crate::analysis::concordance::index::SurfaceIndexError::Cancelled)
    ));
}

#[test]
fn surface_collocation_cancellation_interrupts_streaming_fallback() {
    let documents = vec![concordance_document(0, "a.txt", "target collocate")];
    let query = collocation_query("target");
    let tokenization_config = TokenizationConfig::default();

    let token = crate::analysis::concordance::index::SurfaceIndexCancellationToken::default();
    token.cancel();

    let err = aggregate_concordance_collocations_documents(
        &documents,
        &query,
        &tokenization_config,
        token,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConcordanceError::Index(crate::analysis::concordance::index::SurfaceIndexError::Cancelled)
    ));
}
