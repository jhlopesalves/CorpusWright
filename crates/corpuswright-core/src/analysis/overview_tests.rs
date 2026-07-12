use super::*;
use crate::analysis::test_support::*;
use crate::analysis::*;
use crate::annotation::UniversalPos;
use std::sync::atomic::AtomicBool;

fn assert_dispersion_buckets_partition(overview: &CorpusOverview) {
    let dispersion = &overview.dispersion;
    let exclusive_count_total = dispersion.df_in_1_count
        + dispersion.df_in_2_to_9_count
        + dispersion.df_in_10_to_99_count
        + dispersion.df_in_100_plus_count;
    assert_eq!(exclusive_count_total, overview.total_types);
    assert_eq!(
        dispersion
            .profile_buckets
            .iter()
            .map(|bucket| bucket.count)
            .sum::<usize>(),
        overview.total_types
    );

    if overview.total_types > 0 {
        let exclusive_share_total = [
            dispersion.df_in_1_share,
            dispersion.df_in_2_to_9_share,
            dispersion.df_in_10_to_99_share,
            dispersion.df_in_100_plus_share,
        ]
        .into_iter()
        .map(Option::unwrap)
        .sum::<f64>();
        assert!((exclusive_share_total - 1.0).abs() < 1e-12);
        let profile_share_total = dispersion
            .profile_buckets
            .iter()
            .map(|bucket| bucket.share.unwrap())
            .sum::<f64>();
        assert!((profile_share_total - 1.0).abs() < 1e-12);
    }

    assert_dispersion_sample_lengths(overview);
}

#[test]
fn exact_coverage_targets_use_first_full_rank_crossing() {
    let crossings = exact_coverage_target_crossings(&[40, 20, 20, 10, 5, 5], 100);
    assert_eq!(
        crossings,
        vec![
            CoverageTargetCrossing {
                target_share: 0.50,
                first_rank: 2,
                actual_cumulative_share: 0.60,
            },
            CoverageTargetCrossing {
                target_share: 0.80,
                first_rank: 3,
                actual_cumulative_share: 0.80,
            },
            CoverageTargetCrossing {
                target_share: 0.90,
                first_rank: 4,
                actual_cumulative_share: 0.90,
            },
            CoverageTargetCrossing {
                target_share: 0.95,
                first_rank: 5,
                actual_cumulative_share: 0.95,
            },
        ]
    );
}

fn sample_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn assert_dispersion_sample_lengths(overview: &CorpusOverview) {
    let dispersion = &overview.dispersion;
    for (label, count, sample_types) in [
        (
            "df_in_1",
            dispersion.df_in_1_count,
            &dispersion.df_in_1_sample_types,
        ),
        (
            "df_in_2_to_9",
            dispersion.df_in_2_to_9_count,
            &dispersion.df_in_2_to_9_sample_types,
        ),
        (
            "df_in_10_to_99",
            dispersion.df_in_10_to_99_count,
            &dispersion.df_in_10_to_99_sample_types,
        ),
        (
            "df_in_100_plus",
            dispersion.df_in_100_plus_count,
            &dispersion.df_in_100_plus_sample_types,
        ),
        (
            "df_in_all",
            dispersion.df_in_all_count,
            &dispersion.df_in_all_sample_types,
        ),
        (
            "df_at_least_1pct",
            dispersion.df_at_least_1pct_count,
            &dispersion.df_at_least_1pct_sample_types,
        ),
        (
            "df_at_least_10pct",
            dispersion.df_at_least_10pct_count,
            &dispersion.df_at_least_10pct_sample_types,
        ),
        (
            "df_at_least_50pct",
            dispersion.df_at_least_50pct_count,
            &dispersion.df_at_least_50pct_sample_types,
        ),
        (
            "df_at_least_90pct",
            dispersion.df_at_least_90pct_count,
            &dispersion.df_at_least_90pct_sample_types,
        ),
    ] {
        assert_eq!(
            sample_types.len(),
            count.min(DISPERSION_SAMPLE_TYPE_LIMIT),
            "{label} sample length should be complete or capped"
        );
    }

    for bucket in &dispersion.profile_buckets {
        assert_eq!(
            bucket.sample_types.len(),
            bucket.count.min(DISPERSION_SAMPLE_TYPE_LIMIT),
            "{} profile sample length should be complete or capped",
            bucket.label
        );
    }
}

fn assert_sample_document_frequencies<F, P>(
    sample_types: &[String],
    document_frequency_for: F,
    predicate: P,
) where
    F: Fn(&str) -> usize,
    P: Fn(usize) -> bool,
{
    for sample_type in sample_types {
        let document_frequency = document_frequency_for(sample_type);
        assert!(
            predicate(document_frequency),
            "{sample_type} has document frequency {document_frequency}"
        );
    }
}

#[test]
fn single_document_corpus_has_zero_dp_and_full_top_doc_share() {
    let mut corpus = corpus_for(&[("one.txt", "Cats run")]);
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
        ready_annotation_cache(),
    );

    assert!(result.pos_rows.iter().all(|row| row.dp == 0.0));
    assert!(result.pos_rows.iter().all(|row| row.top_doc_share == 1.0));
}

#[test]
fn empty_documents_are_skipped_and_single_document_dispersion_is_safe() {
    let corpus = corpus_for(&[("empty.txt", "  \n"), ("one.txt", "word word")]);
    assert_eq!(corpus.overview().document_count, 1);
    assert_eq!(corpus.overview().skipped_documents, 1);
    assert_eq!(corpus.overview().dispersion.median_dp, None);
}

#[test]
fn balance_statistics_match_known_distributions() {
    assert_eq!(gini_coefficient(&[1, 1]), Some(0.0));
    assert_eq!(gini_coefficient(&[0, 10]), Some(0.5));
    assert_eq!(gini_coefficient(&[0, 0]), Some(0.0));

    assert_eq!(coefficient_of_variation(&[1, 1]), Some(0.0));
    assert_eq!(coefficient_of_variation(&[0, 2]), Some(1.0));
    assert_eq!(coefficient_of_variation(&[0, 0]), None);

    assert_eq!(effective_document_count(&[1, 1], 2), Some(2.0));
    assert_eq!(effective_document_count(&[0, 2], 2), Some(1.0));
    assert_eq!(effective_document_count(&[], 0), None);
}

#[test]
fn mattr_averages_known_window_ttrs() {
    let tokens = ["a", "b", "a", "c"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let value = mattr(&tokens, 3).unwrap();
    assert!((value - (5.0 / 6.0)).abs() < 1e-12);
    assert_eq!(mattr(&tokens, 5), None);
}

#[test]
fn mtld_averages_forward_and_backward_runs() {
    let tokens = ["a", "b", "a", "c", "a", "b", "d", "a", "b", "c"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let value = mtld(&tokens, 0.72).unwrap();
    assert!((value - (545.0 / 106.0)).abs() < 1e-12);
}

#[test]
fn length_histogram_sums_single_document_and_power_edges() {
    let bins = document_length_histogram(&[0, 1, 2, 3, 4, 8]);
    assert_eq!(bins.iter().map(|bin| bin.count).sum::<usize>(), 6);
    assert_eq!(
        bins,
        vec![
            LengthHistogramBin {
                lower_tokens: 1,
                upper_tokens: Some(2),
                count: 2,
            },
            LengthHistogramBin {
                lower_tokens: 2,
                upper_tokens: Some(4),
                count: 2,
            },
            LengthHistogramBin {
                lower_tokens: 4,
                upper_tokens: Some(8),
                count: 1,
            },
            LengthHistogramBin {
                lower_tokens: 8,
                upper_tokens: Some(16),
                count: 1,
            },
        ]
    );

    assert_eq!(
        document_length_histogram(&[7]),
        vec![LengthHistogramBin {
            lower_tokens: 4,
            upper_tokens: Some(8),
            count: 1,
        }]
    );
}

fn overview_test_document(name: &str, tokens: u64, mattr: Option<f64>) -> AnalysisDocument {
    AnalysisDocument {
        summary: AnalysisDocumentSummary {
            id: 0,
            source_path: name.to_string(),
            relative_path: name.to_string(),
            token_count: tokens,
            language: None,
            language_confidence: None,
            language_is_overridden: false,
        },
        surface_term_counts: Vec::new(),
        annotated_term_counts: Vec::new(),
        annotation_available: false,
        mattr,
        mtld: None,
        processed_hash_sha256: String::new(),
    }
}

#[test]
fn mattr_distribution_bins_sum_witnesses_cap_and_one_clamps_to_final_bin() {
    let documents = vec![
        overview_test_document("zero.txt", 100, Some(0.0)),
        overview_test_document("low.txt", 100, Some(0.024)),
        overview_test_document("edge.txt", 100, Some(0.025)),
        overview_test_document("middle.txt", 100, Some(0.4)),
        overview_test_document("high.txt", 100, Some(0.8)),
        overview_test_document("one.txt", 100, Some(1.0)),
        overview_test_document("short.txt", 99, None),
    ];

    let distribution = mattr_distribution(&documents).unwrap();
    assert_eq!(distribution.bin_width, 0.025);
    assert_eq!(distribution.counts.len(), 40);
    assert_eq!(distribution.counts.iter().sum::<u32>(), 6);
    assert_eq!(distribution.used_documents, 6);
    assert_eq!(distribution.excluded_documents, 1);
    assert_eq!(distribution.counts[0], 2);
    assert_eq!(distribution.counts[1], 1);
    assert_eq!(distribution.counts[39], 1);
    assert_eq!(distribution.low_witnesses.len(), 5);
    assert_eq!(distribution.high_witnesses.len(), 5);
    assert_eq!(distribution.low_witnesses.first().unwrap().name, "zero.txt");
    assert_eq!(distribution.high_witnesses.first().unwrap().name, "one.txt");
}

#[test]
fn lexical_diversity_excludes_short_documents() {
    let short = std::iter::repeat_n("short", 99)
        .collect::<Vec<_>>()
        .join(" ");
    let eligible = std::iter::repeat_n("eligible", 100)
        .collect::<Vec<_>>()
        .join(" ");
    let corpus = corpus_for(&[
        ("short.txt", short.as_str()),
        ("eligible.txt", eligible.as_str()),
    ]);
    let lexical = &corpus.overview().lexical_spectrum;

    assert_eq!(lexical.mattr_docs_used, 1);
    assert_eq!(lexical.mattr_docs_excluded, 1);
    assert_eq!(
        lexical
            .mattr_distribution
            .as_ref()
            .map(|distribution| distribution.used_documents),
        Some(1)
    );
    assert_eq!(lexical.mtld_docs_used, 1);
    assert_eq!(lexical.mtld_docs_excluded, 1);
    assert_eq!(lexical.mattr_median, Some(0.01));
    assert!(lexical.mtld_median.is_some());
}

#[test]
fn entropy_metrics_match_uniform_and_effective_vocabularies() {
    let (entropy, evenness, effective_vocabulary) = entropy_metrics(&[1, 1, 1, 1], 4);
    assert_eq!(entropy, Some(2.0));
    assert_eq!(evenness, Some(1.0));
    assert_eq!(effective_vocabulary, Some(4.0));

    let (entropy, _, effective_vocabulary) = entropy_metrics(&[3, 1], 4);
    let entropy = entropy.unwrap();
    assert!((effective_vocabulary.unwrap() - 2.0f64.powf(entropy)).abs() < 1e-12);
}

#[test]
fn entropy_metrics_omit_single_type_and_empty_distributions() {
    assert_eq!(entropy_metrics(&[10], 10), (None, None, None));
    assert_eq!(entropy_metrics(&[], 0), (None, None, None));
}

#[test]
fn frequency_spectrum_counts_exact_frequency_classes() {
    let (points, truncated, total) = frequency_spectrum(&[5, 2, 2, 1]);
    assert!(!truncated);
    assert_eq!(total, 3);
    assert_eq!(
        points,
        vec![
            FrequencySpectrumPoint {
                frequency: 1,
                type_count: 1,
            },
            FrequencySpectrumPoint {
                frequency: 2,
                type_count: 2,
            },
            FrequencySpectrumPoint {
                frequency: 5,
                type_count: 1,
            },
        ]
    );
}

#[test]
fn zipf_fit_recovers_known_synthetic_exponent() {
    let frequencies = (1..=20u64)
        .map(|rank| 1_000_000 / rank.pow(2))
        .collect::<Vec<_>>();
    let tokens = (1..=20u64)
        .map(|rank| format!("type-{rank}"))
        .collect::<Vec<_>>();
    let summary = rank_frequency_summary(&frequencies, &tokens, frequencies.iter().sum());
    assert!((summary.zipf_exponent.unwrap() - 2.0).abs() < 0.02);
    assert!(summary.r_squared.unwrap() > 0.999);
    assert_eq!(summary.fitted_rank_start, Some(4));
    assert_eq!(summary.fitted_rank_end, Some(20));
    assert_eq!(
        summary.points.first().unwrap().token.as_deref(),
        Some("type-1")
    );
}

#[test]
fn cumulative_coverage_uses_125_ladder_and_full_vocabulary() {
    assert!(cumulative_coverage_checkpoints(&[], 0).is_empty());

    let single = cumulative_coverage_checkpoints(&[7], 7);
    assert_eq!(single, vec![CumulativeCoveragePoint { k: 1, share: 1.0 }]);

    let frequencies = vec![50, 25, 10, 5, 5, 3, 1, 1];
    let points = cumulative_coverage_checkpoints(&frequencies, 100);
    let ks = points.iter().map(|point| point.k).collect::<Vec<_>>();
    assert_eq!(ks, vec![1, 2, 5, 8]);
    assert_eq!(points.last().unwrap().share, 1.0);
}

#[test]
fn universal_types_are_thresholded_sorted_and_capped() {
    let terms = (0..14usize)
        .map(|index| AnalysisTerm {
            token: format!("u{index:02}"),
            frequency: 100 - index as u64,
            document_frequency: if index == 13 { 8 } else { 9 },
            document_counts: Vec::new(),
            dispersion: 0.0,
        })
        .collect::<Vec<_>>();
    let refs = terms.iter().collect::<Vec<_>>();

    let universal = universal_types(&refs, 9);
    assert_eq!(universal.len(), 12);
    assert_eq!(universal.first().unwrap().token, "u00");
    assert_eq!(universal.last().unwrap().token, "u11");
    assert!(universal.iter().all(|term| term.document_frequency >= 9));
    assert!(universal_types(&refs, 0).is_empty());
}

fn overview_test_term(
    token: &str,
    frequency: u64,
    document_frequency: usize,
    dispersion: f64,
) -> AnalysisTerm {
    AnalysisTerm {
        token: token.to_string(),
        frequency,
        document_frequency,
        document_counts: Vec::new(),
        dispersion,
    }
}

#[test]
fn dispersion_plane_grid_witnesses_and_loud_count_are_deterministic() {
    let mut terms = vec![
        overview_test_term("core-local", 6_000, 2, 0.95),
        overview_test_term("core-even", 5_000, 5, 0.05),
        overview_test_term("core-rare", 4_000, 1, 0.70),
    ];
    for index in 0..10 {
        terms.push(overview_test_term(
            &format!("loud-{index}"),
            1_500 - index,
            2,
            0.90 - index as f64 * 0.01,
        ));
    }
    for (index, dispersion) in [0.01, 0.02, 0.03, 0.04].into_iter().enumerate() {
        terms.push(overview_test_term(
            &format!("quiet-{index}"),
            500 - index as u64,
            5,
            dispersion,
        ));
    }
    terms.push(overview_test_term("rare-a", 900, 1, 0.80));
    terms.push(overview_test_term("rare-b", 800, 1, 0.75));
    terms.push(overview_test_term("rare-c", 700, 1, 0.60));

    assert!(dispersion_plane(&terms, 1).is_none());
    let plane = dispersion_plane(&terms, 5).unwrap();
    assert_eq!(plane.x_bins, 48);
    assert_eq!(plane.y_bins, 32);
    assert_eq!(plane.counts.len(), 48 * 32);
    assert_eq!(plane.counts.iter().sum::<u32>(), terms.len() as u32);
    assert_eq!(plane.loud_but_local_count, 12);
    assert!(plane.max_cell_count > 0);
    assert_eq!(plane.witnesses.len(), 16);
    assert_eq!(
        plane
            .witnesses
            .iter()
            .filter(|witness| witness.kind == DispersionWitnessKind::Core)
            .count(),
        3
    );
    assert_eq!(
        plane
            .witnesses
            .iter()
            .filter(|witness| witness.kind == DispersionWitnessKind::LoudButLocal)
            .count(),
        8
    );
    assert_eq!(
        plane
            .witnesses
            .iter()
            .filter(|witness| witness.kind == DispersionWitnessKind::QuietAndEven)
            .count(),
        3
    );
    assert_eq!(
        plane
            .witnesses
            .iter()
            .filter(|witness| witness.kind == DispersionWitnessKind::RareAndLocal)
            .count(),
        2
    );
    let unique_tokens = plane
        .witnesses
        .iter()
        .map(|witness| witness.token.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(unique_tokens.len(), plane.witnesses.len());
    assert_eq!(plane.witnesses[0].token, "core-local");
    assert_eq!(plane.witnesses[0].kind, DispersionWitnessKind::Core);
}

#[test]
fn dispersion_plane_witnesses_apply_document_floor_before_kind_ranking() {
    let terms = vec![
        overview_test_term("noise-high", 12_000, 2, 0.10),
        overview_test_term("core-wide", 9_000, 7, 0.12),
        overview_test_term("core-tie-a", 8_000, 5, 0.11),
        overview_test_term("core-tie-b", 8_000, 5, 0.11),
        overview_test_term("loud-low-df", 5_000, 2, 0.98),
        overview_test_term("loud-wide", 4_000, 9, 0.90),
        overview_test_term("quiet-low-df", 500, 2, 0.01),
        overview_test_term("quiet-wide", 400, 6, 0.02),
        overview_test_term("rare-local", 300, 1, 0.80),
    ];

    let plane = dispersion_plane(&terms, 10).unwrap();
    let core_tokens = plane
        .witnesses
        .iter()
        .filter(|witness| witness.kind == DispersionWitnessKind::Core)
        .map(|witness| witness.token.as_str())
        .collect::<Vec<_>>();
    assert_eq!(core_tokens, vec!["core-wide", "core-tie-a", "core-tie-b"]);
    assert!(plane.witnesses.iter().any(|witness| {
        witness.kind == DispersionWitnessKind::LoudButLocal && witness.token == "loud-wide"
    }));
    assert!(plane.witnesses.iter().any(|witness| {
        witness.kind == DispersionWitnessKind::QuietAndEven && witness.token == "quiet-wide"
    }));
    assert!(!plane.witnesses.iter().any(|witness| {
        matches!(
            witness.token.as_str(),
            "noise-high" | "loud-low-df" | "quiet-low-df"
        )
    }));
    assert!(plane.witnesses.iter().any(|witness| {
        witness.kind == DispersionWitnessKind::RareAndLocal && witness.token == "rare-local"
    }));
}

#[test]
fn dispersion_roster_is_capped_ordered_and_bounded() {
    let terms = (0..125)
        .map(|index| {
            overview_test_term(
                &format!("term-{index:02}"),
                1_000 - index as u64,
                2,
                index as f64 / 44.0,
            )
        })
        .collect::<Vec<_>>();
    let mut ranked_terms = terms.iter().collect::<Vec<_>>();
    ranked_terms.sort_by(|left, right| {
        compare_type_sample(left.frequency, &left.token, right.frequency, &right.token)
    });

    let roster = dispersion_roster(&ranked_terms);
    assert_eq!(roster.len(), 120);
    assert_eq!(roster.first().unwrap().token, "term-00");
    assert!(
        roster
            .windows(2)
            .all(|window| window[0].frequency >= window[1].frequency)
    );
    assert!(roster.iter().all(|entry| (0.0..=1.0).contains(&entry.dp)));
}

#[test]
fn heaps_fit_recovers_known_synthetic_shape() {
    let points = vec![
        VocabularyGrowthPoint {
            tokens: 100,
            types: 100,
        },
        VocabularyGrowthPoint {
            tokens: 400,
            types: 200,
        },
        VocabularyGrowthPoint {
            tokens: 900,
            types: 300,
        },
        VocabularyGrowthPoint {
            tokens: 1_600,
            types: 400,
        },
    ];
    let summary = vocabulary_growth_summary(&points);
    assert!((summary.heaps_k.unwrap() - 10.0).abs() < 1e-10);
    assert!((summary.heaps_beta.unwrap() - 0.5).abs() < 1e-12);
    assert!((summary.r_squared.unwrap() - 1.0).abs() < 1e-12);
}

#[test]
fn lexical_bands_partition_types_and_match_frequent_types() {
    let corpus = corpus_for(&[("bands.txt", "a b b c c c d d d d d d d d d d")]);
    let overview = corpus.overview();
    let lexical = &overview.lexical_spectrum;

    assert_eq!(lexical.hapax_count, 1);
    assert_eq!(lexical.dis_legomena_count, 1);
    assert_eq!(lexical.band_3_to_9_count, 1);
    assert_eq!(lexical.band_10plus_count, 1);
    assert_eq!(
        lexical.hapax_count
            + lexical.dis_legomena_count
            + lexical.band_3_to_9_count
            + lexical.band_10plus_count,
        lexical.types
    );
    assert_eq!(
        lexical.band_10plus_count,
        overview.dispersion.frequent_types
    );
}

#[test]
fn severity_thresholds_use_the_documented_boundaries() {
    use CorpusOverviewFindingSeverity::{NeedsFixing, NoIssue, WorthALook};

    assert_eq!(balance_severity(10, 100, Some(0.149), Some(10.0)), NoIssue);
    assert_eq!(
        balance_severity(10, 100, Some(0.15), Some(10.0)),
        WorthALook
    );
    assert_eq!(
        balance_severity(10, 100, Some(0.25), Some(10.0)),
        WorthALook
    );
    assert_eq!(
        balance_severity(10, 100, Some(0.251), Some(10.0)),
        NeedsFixing
    );
    assert_eq!(balance_severity(10, 100, Some(0.10), Some(6.9)), WorthALook);

    assert_eq!(extraction_severity(100, Some(0.049)), NoIssue);
    assert_eq!(extraction_severity(100, Some(0.05)), WorthALook);
    assert_eq!(extraction_severity(100, Some(0.20)), WorthALook);
    assert_eq!(extraction_severity(100, Some(0.201)), NeedsFixing);

    assert_eq!(dispersion_severity(3, 10, Some(0.299)), NoIssue);
    assert_eq!(dispersion_severity(3, 10, Some(0.30)), WorthALook);
    assert_eq!(dispersion_severity(3, 10, Some(0.60)), WorthALook);
    assert_eq!(dispersion_severity(3, 10, Some(0.601)), NeedsFixing);
    assert_eq!(dispersion_severity(3, 0, None), NoIssue);
}

#[test]
fn empty_corpus_overview_has_no_non_finite_values() {
    let corpus = build_analysis_corpus(
        &[],
        build_options(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "")),
        |_| {},
    )
    .unwrap();
    let overview = corpus.overview();

    assert_eq!(overview.totals.tokens, 0);
    assert_eq!(overview.totals.types, 0);
    assert_eq!(overview.totals.documents, 0);
    assert_eq!(overview.balance.gini, None);
    assert_eq!(overview.balance.coefficient_of_variation, None);
    assert_eq!(overview.balance.effective_documents, None);
    assert_eq!(overview.lexical_spectrum.mattr_median, None);
    assert_eq!(overview.lexical_spectrum.mtld_median, None);
    assert_eq!(overview.frequency_concentration.shannon_entropy_bits, None);
    assert_eq!(overview.frequency_concentration.evenness_j, None);
    assert_eq!(
        overview
            .frequency_concentration
            .effective_vocabulary_entropy,
        None
    );
    let json = serde_json::to_string(overview).unwrap();
    assert!(!json.contains("rawTtr"));
    assert!(!json.contains("perplexity"));
    assert!(!json.contains("NaN"));
    assert!(!json.contains("Infinity"));
    assert_dispersion_buckets_partition(overview);
}

#[test]
fn document_frequency_buckets_partition_type_count_on_fixtures() {
    for corpus in [
        corpus_for(&[("one.txt", "alpha beta beta")]),
        corpus_for(&[
            ("one.txt", "alpha shared common"),
            ("two.txt", "beta shared common"),
            ("three.txt", "gamma common"),
        ]),
        corpus_for(&[]),
    ] {
        assert_dispersion_buckets_partition(corpus.overview());
    }
}

#[test]
fn dispersion_profile_omits_ladder_rungs_above_document_count() {
    let corpus = corpus_for(&[
        ("one.txt", "alpha shared common"),
        ("two.txt", "beta shared common"),
        ("three.txt", "gamma common"),
    ]);
    let profile = &corpus.overview().dispersion.profile_buckets;

    assert_eq!(
        profile
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "2", "3–4"]
    );
    assert_eq!(
        profile
            .iter()
            .map(|bucket| bucket.count)
            .collect::<Vec<_>>(),
        vec![3, 1, 1]
    );
    assert!(profile.iter().all(|bucket| bucket.lower_bound <= 3));
    assert_eq!(profile.iter().map(|bucket| bucket.count).sum::<usize>(), 5);
}

#[test]
fn document_frequency_buckets_and_thresholds_match_hand_computed_fixture() {
    let texts = (0..100)
        .map(|index| {
            let mut tokens = vec!["alltype"];
            if index == 0 {
                tokens.push("single");
            }
            if index < 2 {
                tokens.push("pairtype");
            }
            if index < 10 {
                tokens.push("tentype");
            }
            (format!("doc-{index}.txt"), tokens.join(" "))
        })
        .collect::<Vec<_>>();
    let borrowed_texts = texts
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let corpus = corpus_for(&borrowed_texts);
    let overview = corpus.overview();
    let dispersion = &overview.dispersion;

    assert_eq!(overview.total_types, 4);
    assert_eq!(dispersion.df_in_1_count, 1);
    assert_eq!(dispersion.df_in_2_to_9_count, 1);
    assert_eq!(dispersion.df_in_10_to_99_count, 1);
    assert_eq!(dispersion.df_in_100_plus_count, 1);
    assert_eq!(dispersion.df_in_all_count, 1);
    assert_eq!(dispersion.df_at_least_1pct_count, 4);
    assert_eq!(dispersion.df_at_least_10pct_count, 2);
    assert_eq!(dispersion.df_at_least_50pct_count, 1);
    assert_eq!(dispersion.df_at_least_90pct_count, 1);
    assert_eq!(dispersion.df_at_least_1pct_share, Some(1.0));
    assert_eq!(dispersion.df_at_least_10pct_share, Some(0.5));
    assert_eq!(dispersion.df_at_least_50pct_share, Some(0.25));
    assert_eq!(dispersion.df_at_least_90pct_share, Some(0.25));
    assert_eq!(dispersion.df_in_1_sample_types, sample_strings(&["single"]));
    assert_eq!(
        dispersion.df_in_2_to_9_sample_types,
        sample_strings(&["pairtype"])
    );
    assert_eq!(
        dispersion.df_in_10_to_99_sample_types,
        sample_strings(&["tentype"])
    );
    assert_eq!(
        dispersion.df_in_100_plus_sample_types,
        sample_strings(&["alltype"])
    );
    assert_eq!(
        dispersion.df_in_all_sample_types,
        sample_strings(&["alltype"])
    );
    assert_eq!(
        dispersion.df_at_least_1pct_sample_types,
        sample_strings(&["alltype", "tentype", "pairtype", "single"])
    );
    assert_eq!(
        dispersion.df_at_least_10pct_sample_types,
        sample_strings(&["alltype", "tentype"])
    );
    assert_eq!(
        dispersion.df_at_least_50pct_sample_types,
        sample_strings(&["alltype"])
    );
    assert_eq!(
        dispersion.df_at_least_90pct_sample_types,
        sample_strings(&["alltype"])
    );

    let document_frequency_for = |sample_type: &str| match sample_type {
        "single" => 1,
        "pairtype" => 2,
        "tentype" => 10,
        "alltype" => 100,
        other => panic!("unexpected sample type {other}"),
    };
    assert_sample_document_frequencies(
        &dispersion.df_in_1_sample_types,
        document_frequency_for,
        |df| df == 1,
    );
    assert_sample_document_frequencies(
        &dispersion.df_in_2_to_9_sample_types,
        document_frequency_for,
        |df| (2..=9).contains(&df),
    );
    assert_sample_document_frequencies(
        &dispersion.df_in_10_to_99_sample_types,
        document_frequency_for,
        |df| (10..=99).contains(&df),
    );
    assert_sample_document_frequencies(
        &dispersion.df_in_100_plus_sample_types,
        document_frequency_for,
        |df| df >= 100,
    );
    assert_sample_document_frequencies(
        &dispersion.df_in_all_sample_types,
        document_frequency_for,
        |df| df == 100,
    );
    assert_sample_document_frequencies(
        &dispersion.df_at_least_1pct_sample_types,
        document_frequency_for,
        |df| df >= 1,
    );
    assert_sample_document_frequencies(
        &dispersion.df_at_least_10pct_sample_types,
        document_frequency_for,
        |df| df >= 10,
    );
    assert_sample_document_frequencies(
        &dispersion.df_at_least_50pct_sample_types,
        document_frequency_for,
        |df| df >= 50,
    );
    assert_sample_document_frequencies(
        &dispersion.df_at_least_90pct_sample_types,
        document_frequency_for,
        |df| df >= 90,
    );

    let repeated = corpus_for(&borrowed_texts);
    assert_eq!(dispersion, &repeated.overview().dispersion);
    assert_dispersion_buckets_partition(overview);
}

#[test]
fn single_document_document_frequency_thresholds_clamp_to_one_document() {
    let corpus = corpus_for(&[("one.txt", "alpha beta beta")]);
    let overview = corpus.overview();
    let dispersion = &overview.dispersion;

    assert_eq!(overview.total_types, 2);
    assert_eq!(dispersion.df_in_1_count, 2);
    assert_eq!(dispersion.df_in_all_count, 2);
    assert_eq!(dispersion.df_at_least_1pct_count, overview.total_types);
    assert_eq!(dispersion.df_at_least_10pct_count, overview.total_types);
    assert_eq!(dispersion.df_at_least_50pct_count, overview.total_types);
    assert_eq!(dispersion.df_at_least_90pct_count, overview.total_types);
    assert_eq!(dispersion.df_at_least_1pct_share, Some(1.0));
    assert_eq!(dispersion.df_at_least_10pct_share, Some(1.0));
    assert_eq!(dispersion.df_at_least_50pct_share, Some(1.0));
    assert_eq!(dispersion.df_at_least_90pct_share, Some(1.0));
    assert_dispersion_buckets_partition(overview);
}

#[test]
fn document_frequency_samples_are_capped_and_tiebroken_by_type() {
    let mut tokens = Vec::new();
    for suffix in b'a'..=b'y' {
        let token = format!("word{}", suffix as char);
        tokens.extend(std::iter::repeat_n(token, 2));
    }
    let text = tokens.join(" ");
    let corpus = corpus_for(&[("one.txt", text.as_str())]);
    let overview = corpus.overview();
    let dispersion = &overview.dispersion;
    let expected = (b'a'..=b't')
        .map(|suffix| format!("word{}", suffix as char))
        .collect::<Vec<_>>();

    assert_eq!(overview.total_types, 25);
    assert_eq!(dispersion.df_in_1_count, 25);
    assert_eq!(dispersion.df_in_all_count, 25);
    assert_eq!(dispersion.df_at_least_1pct_count, 25);
    assert_eq!(dispersion.df_in_1_sample_types, expected);
    assert_eq!(dispersion.df_in_all_sample_types, expected);
    assert_eq!(dispersion.df_at_least_1pct_sample_types, expected);
    assert_dispersion_buckets_partition(overview);
}
