use super::*;
use crate::analysis::overview::OVERVIEW_DOCUMENT_LIMIT;
use crate::analysis::test_support::*;
use crate::analysis::*;
use crate::annotation::{UniversalPos, default_model_dir};
use crate::clean::CleaningConfig;
use crate::language::{ProjectLanguage, resolve_effective_document_languages};
use crate::tokenization::TokenizationConfig;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

#[test]
fn project_language_is_effective_for_all_documents() {
    let records = vec![record("a.txt"), record("b.txt")];
    let language_overrides = resolve_effective_document_languages(ProjectLanguage::Pt, &records);
    let mut options = build_options();
    options.language_overrides = language_overrides.clone();

    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            let text = if record.source_path == std::path::Path::new("a.txt") {
                "The quick brown fox."
            } else {
                "Le chat noir court."
            };
            Ok(processed_resolution(record, text))
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(language_overrides.len(), records.len());
    assert!(language_overrides.values().all(|language| language == "pt"));
    assert_eq!(corpus.documents.len(), 2);
    assert!(corpus.documents.iter().all(|document| {
        document.summary.language.as_deref() == Some("pt")
            && document.summary.language_is_overridden
            && document.summary.language_confidence.is_none()
    }));
}

#[test]
fn build_is_surface_only_without_udpipe_models() {
    let corpus = corpus_for(&[("surface.txt", "Cats cat ran running")]);

    assert!(!corpus.overview().annotation_available);
    assert_eq!(
        corpus.overview().build.lemmatisation.as_deref(),
        Some("Surface-only build; lemma/POS annotation is not built yet.")
    );
    assert!(
        corpus
            .documents
            .iter()
            .all(|document| !document.annotation_available)
    );
    assert!(
        corpus
            .documents
            .iter()
            .all(|document| document.annotated_term_counts.is_empty())
    );

    let page = query_frequency_list(&corpus, &base_frequency_query());
    assert_eq!(
        frequency_tokens(&page),
        [("Cats", 1), ("cat", 1), ("ran", 1), ("running", 1)]
    );
}

#[test]
fn language_override_skips_detection_in_fast_build() {
    let records = vec![record("override.txt")];
    let mut options = build_options();
    options.language_overrides.insert(0, "en".to_string());
    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Bonjour le monde")),
        |_| {},
    )
    .unwrap();
    let summary = &corpus.documents[0].summary;

    assert_eq!(summary.language.as_deref(), Some("en"));
    assert_eq!(summary.language_confidence, None);
    assert!(summary.language_is_overridden);
}

#[test]
fn parallel_map_matches_sequential_reference_build() {
    let long_text = (0..130)
        .map(|index| match index % 5 {
            0 => "alpha",
            1 => "beta",
            2 => "gamma",
            3 => "delta",
            _ => "epsilon",
        })
        .collect::<Vec<_>>()
        .join(" ");
    let texts = [
        ("z.txt", "Bonjour le monde alpha"),
        ("a.txt", long_text.as_str()),
        ("empty.txt", " \n"),
        ("b.txt", "Cats cat ran running alpha"),
    ];
    let mut options = build_options();
    options.language_overrides.insert(0, "fr".to_string());

    let sequential = analysis_product_for(&texts, options.clone(), false);
    let parallel = analysis_product_for(&texts, options, true);

    assert_eq!(
        snapshot_analysis_product(&parallel),
        snapshot_analysis_product(&sequential)
    );
}

#[test]
fn golden_fixture_pins_document_and_term_ordering() {
    let texts = [
        ("z.txt", "Beta delta alpha"),
        ("a.txt", "Beta alpha beta"),
        ("m.txt", "alpha Gamma"),
        ("empty.txt", " "),
    ];
    let mut options = build_options();
    for index in 0..texts.len() {
        options.language_overrides.insert(index, "en".to_string());
    }
    let product = analysis_product_for(&texts, options, true);
    let corpus = &product.corpus;

    let summaries = corpus
        .documents
        .iter()
        .map(|document| {
            (
                document.summary.id,
                document.summary.relative_path.as_str(),
                document.summary.token_count,
                document.summary.language.as_deref(),
                document.summary.language_confidence,
                document.summary.language_is_overridden,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summaries,
        vec![
            (0, "a.txt", 3, Some("en"), None, true),
            (1, "m.txt", 2, Some("en"), None, true),
            (2, "z.txt", 3, Some("en"), None, true),
        ]
    );

    let prepared_terms = product
        .terms
        .iter()
        .map(|term| {
            (
                term.token.as_str(),
                term.frequency,
                term.document_frequency,
                term.document_counts.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepared_terms,
        vec![
            ("beta", 3, 2, vec![(0, 2), (2, 1)]),
            ("alpha", 3, 3, vec![(0, 1), (1, 1), (2, 1)]),
            ("gamma", 1, 1, vec![(1, 1)]),
            ("delta", 1, 1, vec![(2, 1)]),
        ]
    );

    let surface_terms = corpus
        .surface_terms
        .iter()
        .map(|term| (term.token.as_str(), term.document_counts.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        surface_terms,
        vec![
            ("Beta", vec![(0, 1), (2, 1)]),
            ("alpha", vec![(0, 1), (1, 1), (2, 1)]),
            ("beta", vec![(0, 1)]),
            ("Gamma", vec![(1, 1)]),
            ("delta", vec![(2, 1)]),
        ]
    );

    let document_surface_counts = corpus
        .documents
        .iter()
        .map(|document| {
            (
                document.summary.relative_path.as_str(),
                document.surface_term_counts.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        document_surface_counts,
        vec![
            ("a.txt", vec![(0, 1), (1, 1), (2, 1)]),
            ("m.txt", vec![(1, 1), (3, 1)]),
            ("z.txt", vec![(0, 1), (1, 1), (4, 1)]),
        ]
    );
    assert_eq!(
        corpus.overview().vocabulary_growth.points,
        vec![
            VocabularyGrowthPoint {
                tokens: 1,
                types: 1,
            },
            VocabularyGrowthPoint {
                tokens: 2,
                types: 2,
            },
            VocabularyGrowthPoint {
                tokens: 4,
                types: 2,
            },
            VocabularyGrowthPoint {
                tokens: 8,
                types: 4,
            },
        ]
    );
}

#[test]
fn empty_processed_output_is_excluded_without_raw_fallback() {
    let record = record("empty.txt");
    let fallback_called = Cell::new(false);
    let resolution = resolve_analysis_input(
        &record,
        &CleaningConfig::default(),
        |_, _| Ok(" \n".to_string()),
        |_, _| {
            fallback_called.set(true);
            Ok("raw text".to_string())
        },
    );

    assert_eq!(
        resolution.provenance,
        AnalysisInputProvenance::EmptyOrFailed
    );
    assert_eq!(resolution.status, AnalysisInputStatus::Empty);
    assert_eq!(resolution.text, None);
    assert!(!fallback_called.get());
}

#[test]
fn analyse_excludes_punctuation_from_metrics() {
    let with_punctuation = corpus_for(&[("punct.txt", "Alpha, beta! Alpha? — beta.")]);
    let stripped = corpus_for(&[("plain.txt", "Alpha beta Alpha beta")]);

    assert_eq!(
        with_punctuation.overview().total_tokens,
        stripped.overview().total_tokens
    );
    assert_eq!(
        with_punctuation.overview().total_types,
        stripped.overview().total_types
    );
    assert_eq!(
        with_punctuation.overview().lexical_spectrum.hapax_count,
        stripped.overview().lexical_spectrum.hapax_count
    );
    assert_eq!(
        with_punctuation
            .overview()
            .frequency_concentration
            .shannon_entropy_bits,
        stripped
            .overview()
            .frequency_concentration
            .shannon_entropy_bits
    );

    let page = query_frequency_list(&with_punctuation, &base_frequency_query());
    assert_eq!(frequency_tokens(&page), [("Alpha", 2), ("beta", 2)]);
}

#[test]
fn prepare_analyse_token_parity() {
    let config = CleaningConfig {
        extract_html: true,
        normalize_unicode: true,
        replace_diacritics: false,
        ..CleaningConfig::default()
    };
    let fixtures = [
        "Well-being, don't stop. 3.14 and 1,000.",
        "<p>Cafe\u{301} — café! Alpha&nbsp;beta.</p>",
        "leading- -trailing alpha—beta",
    ];

    for fixture in fixtures {
        let prepare = crate::word_count::word_token_surfaces_in_processed_text(fixture, &config);
        let html = crate::html::extract_html(fixture);
        let cleaned = crate::clean::clean_text(&html, &config);
        let analyse = analysis_word_tokens(&cleaned, 0, &TokenizationConfig::default())
            .into_iter()
            .map(|token| token.surface.to_string())
            .collect::<Vec<_>>();
        assert_eq!(analyse, prepare);
    }
}

#[test]
fn provenance_counts_reconcile_for_mixed_corpus() {
    let records = vec![
        record("processed.txt"),
        record("fallback.txt"),
        record("empty.txt"),
    ];
    let mut options = build_options();
    options.overview_context.sources_in_project = Some(records.len());
    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            let name = record.relative_path.to_string_lossy();
            Ok(match name.as_ref() {
                "processed.txt" => processed_resolution(record, "processed words"),
                "fallback.txt" => resolve_analysis_input(
                    record,
                    &CleaningConfig::default(),
                    |_, _| Err("processing unavailable".to_string()),
                    |_, _| Ok("fallback words".to_string()),
                ),
                _ => processed_resolution(record, ""),
            })
        },
        |_| {},
    )
    .unwrap();
    let population = &corpus.overview().population;

    assert_eq!(population.processed_used, 1);
    assert_eq!(population.using_source_text_fallback, 1);
    assert_eq!(population.included_in_analysis, 2);
    assert_eq!(population.empty_or_failed, 1);
    assert_eq!(
        population.included_in_analysis,
        population.processed_used + population.using_source_text_fallback
    );
    assert_eq!(
        population.sources_in_project.unwrap(),
        population.included_in_analysis + population.empty_or_failed
    );
    assert_eq!(
        population.input_source,
        "Processed text; 1 using source-text fallback"
    );
    assert_eq!(
        corpus.overview().build.input_source,
        "Processed text; 1 using source-text fallback"
    );
}

#[test]
fn cleaning_rule_changes_analysed_token_count_after_rebuild() {
    let records = vec![record("rules.txt")];
    let build = |cleaning_config: &CleaningConfig| {
        let mut options = build_options();
        options.processing_config_signature = cleaning_config_signature(cleaning_config);
        build_analysis_corpus(
            &records,
            options,
            &AtomicBool::new(false),
            |record| {
                Ok(resolve_analysis_input(
                    record,
                    cleaning_config,
                    |_, config| Ok(crate::clean::clean_text("keep REMOVE", config)),
                    |_, _| unreachable!("processing succeeds"),
                ))
            },
            |_| {},
        )
        .unwrap()
    };
    let before = build(&CleaningConfig::default());
    let after_config = CleaningConfig {
        remove_patterns: vec!["REMOVE".to_string()],
        ..CleaningConfig::default()
    };
    let after = build(&after_config);

    assert_eq!(before.overview().total_tokens, 2);
    assert_eq!(after.overview().total_tokens, 1);
}

#[test]
fn inline_repeated_artifact_rule_is_absent_from_frequency_after_rebuild() {
    let records = vec![record("inline-frequency.txt")];
    let cleaning_config = CleaningConfig {
        normalize_whitespace_line_endings: true,
        removal_rules: vec![anywhere_literal_rule("<br />")],
        ..CleaningConfig::default()
    };
    let mut options = build_options();
    options.processing_config_signature = cleaning_config_signature(&cleaning_config);
    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(resolve_analysis_input(
                record,
                &cleaning_config,
                |_, config| Ok(crate::clean::clean_text("alpha<br /><br />beta", config)),
                |_, _| unreachable!("processing succeeds"),
            ))
        },
        |_| {},
    )
    .unwrap();

    assert_eq!(corpus.overview().total_tokens, 2);
    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            query: "br".to_string(),
            match_mode: FrequencyMatchMode::Contains,
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

    assert_eq!(page.total_terms, 0);
}

#[test]
fn cleaning_signature_marks_cached_analysis_stale() {
    let corpus = corpus_for(&[("stale.txt", "current text")]);
    let changed = CleaningConfig {
        remove_patterns: vec!["current".to_string()],
        ..CleaningConfig::default()
    };

    assert!(!corpus.is_stale_for_cleaning_config(&CleaningConfig::default()));
    assert!(corpus.is_stale_for_cleaning_config(&changed));
}

#[test]
fn builds_overview_and_sparse_frequency_index() {
    let corpus = corpus_for(&[("one.txt", "Cats cats dog"), ("two.txt", "dog bird")]);
    let overview = corpus.overview();

    assert_eq!(overview.document_count, 2);
    assert_eq!(overview.total_tokens, 5);
    assert_eq!(overview.total_types, 3);
    assert_eq!(overview.lexical_spectrum.hapax_count, 1);
    assert_eq!(overview.lexical_spectrum.dis_legomena_count, 2);

    let page = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            query: String::new(),
            match_mode: FrequencyMatchMode::Contains,
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
            limit: 2,
        },
    );
    assert_eq!(page.total_terms, 4);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].token, "dog");
    assert_eq!(page.items[0].frequency, 2);
    assert_eq!(page.items[1].token, "Cats");
}

#[test]
fn shuffled_source_order_produces_identical_analysis() {
    let texts = HashMap::from([
        ("a.txt", "alpha alpha beta"),
        ("b.txt", "beta gamma"),
        ("c.txt", "gamma delta"),
    ]);
    let build = |names: &[&str]| {
        let records = names.iter().map(|name| record(name)).collect::<Vec<_>>();
        build_analysis_corpus(
            &records,
            build_options(),
            &AtomicBool::new(false),
            |record| {
                Ok(processed_resolution(
                    record,
                    texts[record.source_path.to_str().unwrap()],
                ))
            },
            |_| {},
        )
        .unwrap()
    };
    let forward = build(&["a.txt", "b.txt", "c.txt"]);
    let shuffled = build(&["c.txt", "a.txt", "b.txt"]);
    assert_eq!(forward.overview(), shuffled.overview());
    assert_eq!(
        query_frequency_list(&forward, &base_frequency_query()),
        query_frequency_list(&shuffled, &base_frequency_query())
    );
}

#[test]
fn builds_many_tiny_documents_without_a_large_fixture() {
    let records = (0..1_000)
        .map(|index| record(&format!("{index}.txt")))
        .collect::<Vec<_>>();
    let corpus = build_analysis_corpus(
        &records,
        build_options(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "shared unique")),
        |_| {},
    )
    .unwrap();

    assert_eq!(corpus.overview().document_count, 1_000);
    assert_eq!(corpus.overview().total_tokens, 2_000);
    assert_eq!(corpus.overview().total_types, 2);
    assert!(corpus.overview().documents_truncated);
    assert_eq!(corpus.overview().documents.len(), OVERVIEW_DOCUMENT_LIMIT);
}

#[test]
fn build_can_be_cancelled_between_documents() {
    let records = vec![record("one.txt"), record("two.txt")];
    let cancel = AtomicBool::new(false);
    let result = build_analysis_corpus(
        &records,
        build_options(),
        &cancel,
        |record| Ok(processed_resolution(record, "one two")),
        |progress| {
            if progress.files_done == 1 {
                cancel.store(true, AtomicOrdering::Relaxed);
            }
        },
    );

    assert_eq!(result.unwrap_err(), AnalysisBuildError::Cancelled);
}

#[test]
fn none_path_skips_processed_hash() {
    let product = analysis_product_for(
        &[("hash.txt", "The quick brown fox.")],
        build_options(),
        false,
    );

    assert_eq!(product.corpus.documents[0].processed_hash_sha256, "");
}

#[test]
fn some_path_hash_unchanged() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let product = analysis_product_for(&[("hash.txt", "The quick brown fox.")], options, false);

    assert_eq!(
        product.corpus.documents[0].processed_hash_sha256,
        crate::hash::sha256_hex("The quick brown fox.".as_bytes())
    );
}

#[test]
fn some_path_cache_still_ready() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let text = "The quick brown fox.";
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let mut corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, text)),
        |_| {},
    )
    .unwrap();
    let surface_term_id = corpus
        .surface_terms
        .iter()
        .position(|term| term.token == "The")
        .unwrap() as u32;
    let lemma_id = corpus.lemma_table.intern("the");
    let layer = AnnotationLayer {
        documents: vec![AnnotationLayerDocument {
            analysis_document_index: 0,
            source_path: "doc.txt".to_string(),
            relative_path: "doc.txt".to_string(),
            term_counts: vec![AnnotatedTermCount {
                surface_term_id,
                lemma_id,
                upos: UniversalPos::Det,
                count: 1,
            }],
        }],
        lemma_table: corpus.lemma_table.clone(),
        languages_used: Vec::new(),
    };
    let manifest = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    )
    .unwrap();
    crate::annotation_cache::write_annotation_layer_cache(
        temp_dir.path(),
        &manifest,
        &surface_term_table(&corpus),
        &layer,
    )
    .unwrap();

    let cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, text)),
        |_| {},
    )
    .unwrap();

    assert!(cached.overview().annotation_available);
    assert_eq!(
        cached.documents[0].annotated_term_counts,
        layer.documents[0].term_counts
    );
}

#[test]
fn manifest_builder_rejects_too_short_document_hash_input() {
    let corpus = corpus_for(&[("doc1.txt", "hello")]);
    let result = crate::annotation_cache::build_annotation_manifest(
        &corpus,
        &[],
        crate::tokenization::TokenizationManifest {
            id: "".into(),
            version: String::new(),
            unicode_segmentation_version: "".into(),
            config: crate::tokenization::TokenizationConfig::default(),
        },
        &std::collections::HashMap::new(),
        vec![],
        &[], // too few
        &["en".to_string()],
    );
    assert!(matches!(
        result,
        Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
    ));
}

#[test]
fn manifest_builder_rejects_too_long_document_hash_input() {
    let corpus = corpus_for(&[]); // count = 0
    let result = crate::annotation_cache::build_annotation_manifest(
        &corpus,
        &[],
        crate::tokenization::TokenizationManifest {
            id: "".into(),
            version: String::new(),
            unicode_segmentation_version: "".into(),
            config: crate::tokenization::TokenizationConfig::default(),
        },
        &std::collections::HashMap::new(),
        vec![],
        &["hash1".to_string()], // too many
        &[],
    );
    assert!(matches!(
        result,
        Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
    ));
}

#[test]
fn manifest_builder_rejects_too_short_document_language_input() {
    let corpus = corpus_for(&[("doc1.txt", "hello")]);
    let result = crate::annotation_cache::build_annotation_manifest(
        &corpus,
        &[],
        crate::tokenization::TokenizationManifest {
            id: "".into(),
            version: String::new(),
            unicode_segmentation_version: "".into(),
            config: crate::tokenization::TokenizationConfig::default(),
        },
        &std::collections::HashMap::new(),
        vec![],
        &["hash1".to_string()],
        &[], // too few
    );
    assert!(matches!(
        result,
        Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
    ));
}

#[test]
fn manifest_builder_rejects_too_long_document_language_input() {
    let corpus = corpus_for(&[]); // count = 0
    let result = crate::annotation_cache::build_annotation_manifest(
        &corpus,
        &[],
        crate::tokenization::TokenizationManifest {
            id: "".into(),
            version: String::new(),
            unicode_segmentation_version: "".into(),
            config: crate::tokenization::TokenizationConfig::default(),
        },
        &std::collections::HashMap::new(),
        vec![],
        &[],
        &["en".to_string()], // too many
    );
    assert!(matches!(
        result,
        Err(crate::annotation_cache::AnnotationLayerCacheError::ValidationError(_))
    ));
}

#[test]
fn test_build_is_deterministic() {
    let temp_dir1 = tempfile::tempdir().unwrap();
    let temp_dir2 = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    let mut options1 = build_options();
    options1.annotation_cache_root = Some(temp_dir1.path().to_path_buf());
    options1.udpipe_model_dir = Some(default_model_dir());
    let build_ann_options1 = AnnotationLayerBuildOptions {
        udpipe_model_dir: options1.udpipe_model_dir.clone(),
        language_overrides: options1.language_overrides.clone(),
    };
    build_and_write_annotation_cache(
        &records,
        options1,
        build_ann_options1,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    let mut options2 = build_options();
    options2.annotation_cache_root = Some(temp_dir2.path().to_path_buf());
    options2.udpipe_model_dir = Some(default_model_dir());
    let build_ann_options2 = AnnotationLayerBuildOptions {
        udpipe_model_dir: options2.udpipe_model_dir.clone(),
        language_overrides: options2.language_overrides.clone(),
    };
    build_and_write_annotation_cache(
        &records,
        options2,
        build_ann_options2,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    let cache_file1 = temp_dir1.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    let cache_file2 = temp_dir2.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));

    let bytes1 = std::fs::read(&cache_file1).unwrap();
    let bytes2 = std::fs::read(&cache_file2).unwrap();

    // Use semantic envelope equality
    let envelope1: crate::annotation_cache::AnnotationLayerCacheEnvelope =
        serde_json::from_slice(&bytes1).unwrap();
    let envelope2: crate::annotation_cache::AnnotationLayerCacheEnvelope =
        serde_json::from_slice(&bytes2).unwrap();

    assert_eq!(envelope1, envelope2);
}
