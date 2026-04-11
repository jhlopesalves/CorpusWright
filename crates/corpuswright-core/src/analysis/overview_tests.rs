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
    }

    assert_dispersion_sample_lengths(overview);
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
    let (points, truncated) = frequency_spectrum(&[5, 2, 2, 1]);
    assert!(!truncated);
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
    let summary = rank_frequency_summary(&frequencies, frequencies.iter().sum());
    assert!((summary.zipf_exponent.unwrap() - 2.0).abs() < 0.02);
    assert!(summary.r_squared.unwrap() > 0.999);
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
