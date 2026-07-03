use super::*;
use crate::analysis::test_support::*;
use crate::index::{
    InMemoryIndexStore, SurfaceIndexDocumentInput, SurfaceIndexQuery, search_surface_index,
};
use crate::tokenization::TokenizationConfig;
use std::collections::HashMap;

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
    let mut candidate_counts = HashMap::<String, u64>::new();
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
                for candidate in &token_keys[left_start..token_index] {
                    if query_key_set.contains(candidate) {
                        continue;
                    }
                    *candidate_counts.entry(candidate.clone()).or_default() += 1;
                }
            }
            if matches!(
                direction,
                ConcordanceCollocationDirection::Right | ConcordanceCollocationDirection::Both
            ) {
                let right_end = (node_end + span).min(token_keys.len());
                r1 += (right_end - node_end) as u64;
                for candidate in &token_keys[node_end..right_end] {
                    if query_key_set.contains(candidate) {
                        continue;
                    }
                    *candidate_counts.entry(candidate.clone()).or_default() += 1;
                }
            }
        }
    }

    let limit = concordance_collocation_limit(query.limit);
    let mut rows = candidate_counts
        .into_iter()
        .filter_map(|(surface, o11)| {
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
            if o11 < query.min_cooccurrence || collocate_frequency < query.min_collocate_frequency {
                return None;
            }
            Some(collocation_row(surface, o11, collocate_frequency, r1, n))
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
            )
            .unwrap();
            let mut context = ConcordanceSearchContext::default();
            let index = aggregate_concordance_collocations_documents_routed(
                &fixture.documents,
                &query,
                &tokenization_config,
                &mut context,
                ConcordanceBackend::Index,
            )
            .unwrap();

            assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
            assert_eq!(
                index.aggregate, scan,
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

    let scan =
        aggregate_concordance_collocations_documents(&documents, &query, &tokenization_config)
            .unwrap();
    let index =
        aggregate_concordance_collocations_index(&store, &query, &tokenization_config).unwrap();

    assert_eq!(scan.f_node, 4);
    assert_eq!(index.f_node, 4);
    assert_eq!(index, scan);
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
    let concordance_first = crate::analysis::search_concordance_documents_routed(
        &documents,
        &concordance,
        &tokenization_config,
        &mut concordance_first_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    let collocation_second = aggregate_concordance_collocations_documents_routed(
        &documents,
        &collocation,
        &tokenization_config,
        &mut concordance_first_context,
        ConcordanceBackend::Index,
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
    )
    .unwrap();
    let concordance_second = crate::analysis::search_concordance_documents_routed(
        &documents,
        &concordance,
        &tokenization_config,
        &mut collocation_first_context,
        ConcordanceBackend::Index,
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
    let scan =
        aggregate_concordance_collocations_documents(&documents, &query, &tokenization_config)
            .unwrap();
    let mut context = ConcordanceSearchContext::default();

    let routed = aggregate_concordance_collocations_documents_routed(
        &documents,
        &query,
        &tokenization_config,
        &mut context,
        ConcordanceBackend::Index,
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
