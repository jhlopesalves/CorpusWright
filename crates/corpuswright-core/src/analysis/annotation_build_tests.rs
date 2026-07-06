use super::*;
use crate::analysis::test_support::*;
use crate::analysis::*;
use crate::annotation::{AnnotatedToken, LemmaTable, UniversalPos, default_model_dir};
use crate::language::{ProjectLanguage, resolve_effective_document_languages};
use crate::scan::DocumentRecord;
use crate::tokenization::{TokenizationConfig, tokenize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

fn fake_annotation(surface: &str, token_index: u32) -> AnnotatedToken {
    AnnotatedToken {
        surface: surface.to_string(),
        lemma: surface.to_lowercase(),
        upos: UniversalPos::Noun,
        token_index,
        features: HashMap::new(),
    }
}

fn english_overrides(count: usize) -> HashMap<usize, String> {
    (0..count).map(|index| (index, "en".to_string())).collect()
}

fn repeated_word_text(word: &str, word_count: usize) -> String {
    std::iter::repeat_n(word, word_count)
        .collect::<Vec<_>>()
        .join(" ")
}

fn annotation_layer_result_for(
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    parallel_map: bool,
    cancel: &AtomicBool,
    docs_done: &AtomicUsize,
) -> Result<(AnalysisCorpus, AnnotationLayer), AnnotationLayerBuildError> {
    annotation_layer_result_for_with_timing(
        texts,
        language_overrides,
        parallel_map,
        cancel,
        docs_done,
        None,
    )
}

fn annotation_layer_result_for_with_timing(
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    parallel_map: bool,
    cancel: &AtomicBool,
    docs_done: &AtomicUsize,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<(AnalysisCorpus, AnnotationLayer), AnnotationLayerBuildError> {
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(texts, analysis_options, true).corpus;
    let tokens_done = AtomicU64::new(0);
    let document_token_counts = annotation_document_token_counts(&corpus);
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let layer = build_annotation_layer_with_mapping_timed(
        &corpus,
        &records,
        annotation_options(language_overrides),
        cancel,
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        parallel_map,
        AnnotationLayerBuildRuntime {
            docs_done,
            tokens_done: &tokens_done,
            document_token_counts: &document_token_counts,
            timing,
        },
    )?;
    Ok((corpus, layer))
}

fn annotation_layer_result_for_with_wave_budget(
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    cancel: &AtomicBool,
    docs_done: &AtomicUsize,
    timing: Option<&AnnotationBuildTimingCounters>,
    wave_budget: usize,
) -> Result<(AnalysisCorpus, AnnotationLayer), AnnotationLayerBuildError> {
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(texts, analysis_options, true).corpus;
    let tokens_done = AtomicU64::new(0);
    let document_token_counts = annotation_document_token_counts(&corpus);
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let mut load_text = |record: &DocumentRecord| {
        Ok(processed_resolution(
            record,
            text_by_name
                .get(record.source_path.to_str().unwrap())
                .copied()
                .unwrap_or_default(),
        ))
    };
    let inputs = load_annotation_document_inputs(
        &corpus,
        &records,
        &language_overrides,
        cancel,
        &mut load_text,
        timing,
    )?;
    let surface_term_lookup = build_surface_term_lookup(&corpus)?;
    let languages_used = inputs
        .iter()
        .map(|input| input.language.clone())
        .collect::<BTreeSet<_>>();
    let model_dir = default_model_dir();
    let worker_documents = annotate_annotation_documents_in_staged_waves_with_budget(
        &inputs,
        &surface_term_lookup,
        AnnotationModelBuildContext {
            languages_used: &languages_used,
            model_dir: &model_dir,
        },
        cancel,
        &corpus.tokenization,
        AnnotationLayerBuildRuntime {
            docs_done,
            tokens_done: &tokens_done,
            document_token_counts: &document_token_counts,
            timing,
        },
        wave_budget,
    )?;
    let layer =
        reduce_annotation_documents(worker_documents, languages_used.into_iter().collect())?;
    Ok((corpus, layer))
}

fn multi_wave_english_texts() -> (Vec<(String, String)>, usize, usize) {
    let token_budget = annotation_wave_token_budget();
    let words_per_doc = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP * 2 + 17;
    let target_words = token_budget.saturating_mul(2).saturating_add(words_per_doc);
    let mut total_words = 0usize;
    let mut texts = Vec::new();

    while total_words <= target_words {
        let words = words_per_doc + texts.len() % 5;
        texts.push((
            format!("doc-{:03}.txt", texts.len()),
            repeated_word_text("word", words),
        ));
        total_words = total_words.saturating_add(words);
    }

    assert!(total_words > token_budget.saturating_mul(2));
    (texts, total_words, token_budget)
}

fn cache_file_path(cache_root: &Path) -> PathBuf {
    cache_root.join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ))
}

fn cache_records(texts: &[(&str, &str)]) -> Vec<DocumentRecord> {
    texts.iter().map(|(name, _)| record(name)).collect()
}

fn cache_surface_corpus_for(
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
) -> AnalysisCorpus {
    let mut options = build_options();
    options.force_compute_hashes = true;
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = language_overrides;
    analysis_product_for(texts, options, true).corpus
}

fn build_cache_with_reusable_surface(
    cache_root: &Path,
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    reusable_surface_corpus: &AnalysisCorpus,
) -> (
    AnnotationCacheBuildReport,
    crate::annotation_cache::AnnotationLayerCacheEnvelope,
    Vec<String>,
) {
    let records = cache_records(texts);
    let text_by_name = texts
        .iter()
        .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
        .collect::<HashMap<_, _>>();
    let loaded_records = Arc::new(Mutex::new(Vec::new()));
    let loaded_records_for_closure = loaded_records.clone();
    let mut options = build_options();
    options.annotation_cache_root = Some(cache_root.to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = language_overrides.clone();
    let annotation_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides,
    };

    let report = build_and_write_annotation_cache_with_reusable_surface_corpus(
        &records,
        options,
        annotation_options,
        &AtomicBool::new(false),
        |record| {
            loaded_records_for_closure
                .lock()
                .unwrap()
                .push(record.relative_path.to_string_lossy().into_owned());
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .map(String::as_str)
                    .unwrap_or_default(),
            ))
        },
        |_| {},
        Some(reusable_surface_corpus),
    )
    .unwrap();
    let envelope = read_annotation_cache_envelope(cache_root);
    let loaded_records = loaded_records.lock().unwrap().clone();

    (report, envelope, loaded_records)
}

fn build_full_rebuild_cache(
    cache_root: &Path,
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    reusable_surface_corpus: &AnalysisCorpus,
) -> crate::annotation_cache::AnnotationLayerCacheEnvelope {
    build_cache_with_reusable_surface(
        cache_root,
        texts,
        language_overrides,
        reusable_surface_corpus,
    )
    .1
}

fn write_cache_envelope(
    cache_root: &Path,
    envelope: &crate::annotation_cache::AnnotationLayerCacheEnvelope,
) {
    std::fs::write(
        cache_file_path(cache_root),
        serde_json::to_vec(envelope).unwrap(),
    )
    .unwrap();
}

#[test]
fn annotation_cache_salvage_unchanged_corpus_uses_fast_path() {
    let cache_dir = tempfile::tempdir().unwrap();
    let texts = [("a.txt", "Cats chase mice."), ("b.txt", "Dogs jump high.")];
    let language_overrides = english_overrides(texts.len());
    let corpus = cache_surface_corpus_for(&texts, language_overrides.clone());

    let (first_report, _, first_loaded) = build_cache_with_reusable_surface(
        cache_dir.path(),
        &texts,
        language_overrides.clone(),
        &corpus,
    );
    assert!(first_report.cache_written);
    assert_eq!(first_loaded.len(), texts.len());

    let (second_report, _, second_loaded) =
        build_cache_with_reusable_surface(cache_dir.path(), &texts, language_overrides, &corpus);

    assert!(second_report.reused_existing);
    assert!(!second_report.cache_written);
    assert!(second_loaded.is_empty());
}

#[test]
fn annotation_cache_salvage_added_documents_matches_full_rebuild() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let full_dir = tempfile::tempdir().unwrap();
    let old_texts = [("a.txt", "Cats chase mice.")];
    let new_texts = [("a.txt", "Cats chase mice."), ("b.txt", "Dogs jump high.")];
    let old_overrides = english_overrides(old_texts.len());
    let new_overrides = english_overrides(new_texts.len());
    let old_corpus = cache_surface_corpus_for(&old_texts, old_overrides.clone());
    let new_corpus = cache_surface_corpus_for(&new_texts, new_overrides.clone());

    build_cache_with_reusable_surface(salvage_dir.path(), &old_texts, old_overrides, &old_corpus);
    let (_, salvaged, loaded) = build_cache_with_reusable_surface(
        salvage_dir.path(),
        &new_texts,
        new_overrides.clone(),
        &new_corpus,
    );
    let full = build_full_rebuild_cache(full_dir.path(), &new_texts, new_overrides, &new_corpus);

    assert_eq!(loaded, vec!["b.txt".to_string()]);
    assert_eq!(salvaged.layer, full.layer);
}

#[test]
fn annotation_cache_salvage_removed_documents_matches_full_rebuild() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let full_dir = tempfile::tempdir().unwrap();
    let old_texts = [("a.txt", "Cats chase mice."), ("b.txt", "Dogs jump high.")];
    let new_texts = [("a.txt", "Cats chase mice.")];
    let old_overrides = english_overrides(old_texts.len());
    let new_overrides = english_overrides(new_texts.len());
    let old_corpus = cache_surface_corpus_for(&old_texts, old_overrides.clone());
    let new_corpus = cache_surface_corpus_for(&new_texts, new_overrides.clone());

    build_cache_with_reusable_surface(salvage_dir.path(), &old_texts, old_overrides, &old_corpus);
    let (_, salvaged, loaded) = build_cache_with_reusable_surface(
        salvage_dir.path(),
        &new_texts,
        new_overrides.clone(),
        &new_corpus,
    );
    let full = build_full_rebuild_cache(full_dir.path(), &new_texts, new_overrides, &new_corpus);

    assert!(loaded.is_empty());
    assert_eq!(salvaged.layer, full.layer);
}

#[test]
fn annotation_cache_salvage_modified_document_reannotates_only_misses() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let full_dir = tempfile::tempdir().unwrap();
    let old_texts = [("a.txt", "Cats chase mice."), ("b.txt", "Dogs jump high.")];
    let new_texts = [
        ("a.txt", "Cats chase mice."),
        ("b.txt", "Birds sing softly."),
    ];
    let old_overrides = english_overrides(old_texts.len());
    let new_overrides = english_overrides(new_texts.len());
    let old_corpus = cache_surface_corpus_for(&old_texts, old_overrides.clone());
    let new_corpus = cache_surface_corpus_for(&new_texts, new_overrides.clone());

    build_cache_with_reusable_surface(salvage_dir.path(), &old_texts, old_overrides, &old_corpus);
    let (_, salvaged, loaded) = build_cache_with_reusable_surface(
        salvage_dir.path(),
        &new_texts,
        new_overrides.clone(),
        &new_corpus,
    );
    let full = build_full_rebuild_cache(full_dir.path(), &new_texts, new_overrides, &new_corpus);

    assert_eq!(loaded, vec!["b.txt".to_string()]);
    assert_eq!(salvaged.layer, full.layer);
}

#[test]
fn annotation_cache_salvage_progress_credits_reused_tokens_immediately() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let old_texts = [
        ("reused.txt", "Cats chase patient mice."),
        ("miss.txt", "Dogs jump high."),
    ];
    let new_texts = [
        ("reused.txt", "Cats chase patient mice."),
        ("miss.txt", "Birds sing very softly."),
    ];
    let old_overrides = english_overrides(old_texts.len());
    let new_overrides = english_overrides(new_texts.len());
    let old_corpus = cache_surface_corpus_for(&old_texts, old_overrides.clone());
    let new_corpus = cache_surface_corpus_for(&new_texts, new_overrides.clone());

    build_cache_with_reusable_surface(salvage_dir.path(), &old_texts, old_overrides, &old_corpus);

    let records = cache_records(&new_texts);
    let text_by_name = new_texts
        .iter()
        .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
        .collect::<HashMap<_, _>>();
    let mut options = build_options();
    options.annotation_cache_root = Some(salvage_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = new_overrides.clone();
    let annotation_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: new_overrides,
    };
    let mut progress_events = Vec::new();

    let report = build_and_write_annotation_cache_with_reusable_surface_corpus(
        &records,
        options,
        annotation_options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .map(String::as_str)
                    .unwrap_or_default(),
            ))
        },
        |progress| progress_events.push(progress),
        Some(&new_corpus),
    )
    .unwrap();

    let first_annotating = progress_events
        .iter()
        .find(|progress| {
            matches!(
                progress.current_phase,
                AnnotationCacheBuildPhase::Annotating
            )
        })
        .unwrap();
    let reused_tokens = new_corpus.documents[0].summary.token_count;
    let total_tokens = new_corpus
        .documents
        .iter()
        .map(|document| document.summary.token_count)
        .sum::<u64>();

    assert!(report.cache_written);
    assert_eq!(first_annotating.files_done, 1);
    assert_eq!(first_annotating.tokens_done, reused_tokens);
    assert_eq!(first_annotating.tokens_total, total_tokens);
}

#[test]
fn annotation_cache_salvage_renamed_document_reuses() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let full_dir = tempfile::tempdir().unwrap();
    let old_texts = [("old-name.txt", "Cats chase mice.")];
    let new_texts = [("new-name.txt", "Cats chase mice.")];
    let old_overrides = english_overrides(old_texts.len());
    let new_overrides = english_overrides(new_texts.len());
    let old_corpus = cache_surface_corpus_for(&old_texts, old_overrides.clone());
    let new_corpus = cache_surface_corpus_for(&new_texts, new_overrides.clone());

    build_cache_with_reusable_surface(salvage_dir.path(), &old_texts, old_overrides, &old_corpus);
    let (_, salvaged, loaded) = build_cache_with_reusable_surface(
        salvage_dir.path(),
        &new_texts,
        new_overrides.clone(),
        &new_corpus,
    );
    let full = build_full_rebuild_cache(full_dir.path(), &new_texts, new_overrides, &new_corpus);

    assert!(loaded.is_empty());
    assert_eq!(salvaged.layer, full.layer);
    assert_eq!(salvaged.layer.documents[0].relative_path, "new-name.txt");
}

#[test]
fn annotation_cache_salvage_model_change_invalidates_language() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let full_dir = tempfile::tempdir().unwrap();
    let texts = [
        ("en.txt", "The small cat sleeps."),
        ("pt.txt", "O gato preto correu."),
    ];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    language_overrides.insert(1, "pt".to_string());
    let corpus = cache_surface_corpus_for(&texts, language_overrides.clone());

    build_cache_with_reusable_surface(
        salvage_dir.path(),
        &texts,
        language_overrides.clone(),
        &corpus,
    );
    let mut old_envelope = read_annotation_cache_envelope(salvage_dir.path());
    let pt_model = old_envelope
        .manifest
        .models
        .iter_mut()
        .find(|model| model.language == "pt")
        .expect("test fixture should include pt model");
    pt_model.sha256 = "changed-model-sha".to_string();
    write_cache_envelope(salvage_dir.path(), &old_envelope);

    let (_, salvaged, loaded) = build_cache_with_reusable_surface(
        salvage_dir.path(),
        &texts,
        language_overrides.clone(),
        &corpus,
    );
    let full = build_full_rebuild_cache(full_dir.path(), &texts, language_overrides, &corpus);

    assert_eq!(loaded, vec!["pt.txt".to_string()]);
    assert_eq!(salvaged.layer, full.layer);
}

#[test]
fn annotation_cache_salvage_rejects_pipeline_version_mismatch() {
    let salvage_dir = tempfile::tempdir().unwrap();
    let texts = [("a.txt", "Cats chase mice."), ("b.txt", "Dogs jump high.")];
    let language_overrides = english_overrides(texts.len());
    let corpus = cache_surface_corpus_for(&texts, language_overrides.clone());

    build_cache_with_reusable_surface(
        salvage_dir.path(),
        &texts,
        language_overrides.clone(),
        &corpus,
    );
    let mut old_envelope = read_annotation_cache_envelope(salvage_dir.path());
    old_envelope.manifest.annotation_pipeline_version += 1;
    write_cache_envelope(salvage_dir.path(), &old_envelope);

    let (report, _, loaded) =
        build_cache_with_reusable_surface(salvage_dir.path(), &texts, language_overrides, &corpus);

    assert!(report.cache_written);
    assert!(!report.reused_existing);
    assert_eq!(loaded.len(), texts.len());
}

#[test]
fn annotation_cache_round_trips_ready_with_project_language() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let language_overrides = resolve_effective_document_languages(ProjectLanguage::En, &records);
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = language_overrides.clone();

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides,
    };

    build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    let corpus_cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(corpus_cached.overview().annotation_available);
}

#[test]
fn annotation_input_language_overrides_use_original_record_indices() {
    let texts = [("z.txt", "Bonjour le monde"), ("a.txt", "Cats run")];
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "fr".to_string());
    language_overrides.insert(1, "en".to_string());
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(&texts, analysis_options, true).corpus;
    let mut load_text = |record: &DocumentRecord| {
        Ok(processed_resolution(
            record,
            text_by_name
                .get(record.source_path.to_str().unwrap())
                .copied()
                .unwrap_or_default(),
        ))
    };

    let inputs = load_annotation_document_inputs(
        &corpus,
        &records,
        &language_overrides,
        &AtomicBool::new(false),
        &mut load_text,
        None,
    )
    .unwrap();

    assert_eq!(inputs[0].relative_path, "a.txt");
    assert_eq!(inputs[0].language, "en");
    assert_eq!(inputs[1].relative_path, "z.txt");
    assert_eq!(inputs[1].language, "fr");
}

#[test]
fn annotation_parallel_build_matches_serial_reference() {
    let texts = [("b.txt", "Dogs run quickly"), ("a.txt", "Cats chase mice")];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    language_overrides.insert(1, "en".to_string());

    let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
    let (_, parallel) = annotation_layer_for(&texts, language_overrides, true);

    assert_eq!(parallel, serial);
}

#[test]
fn annotation_chunk_parallel_matches_serial_for_skewed_documents() {
    let whale = repeated_word_text(
        "whale",
        crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP * 3 + 23,
    );
    let texts = [
        ("d-short.txt", "Cats run quickly."),
        ("b-whale.txt", whale.as_str()),
        ("e-short.txt", "Dogs jump."),
        ("a-short.txt", "Birds sing."),
    ];
    let language_overrides = english_overrides(texts.len());

    let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
    let (_, parallel) = annotation_layer_for(&texts, language_overrides, true);

    assert_eq!(parallel, serial);
    assert_eq!(
        parallel
            .documents
            .iter()
            .map(|document| document.relative_path.as_str())
            .collect::<Vec<_>>(),
        serial
            .documents
            .iter()
            .map(|document| document.relative_path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn annotation_chunk_parallel_multi_wave_matches_serial_reference() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let (owned_texts, total_words, token_budget) = multi_wave_english_texts();
        assert!(total_words > token_budget.saturating_mul(2));
        let texts = owned_texts
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let language_overrides = english_overrides(texts.len());

        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let (_, parallel) = annotation_layer_for(&texts, language_overrides, true);

        assert_eq!(parallel, serial);
        assert_eq!(
            parallel
                .documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>(),
            serial
                .documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            parallel
                .documents
                .iter()
                .map(|document| &document.term_counts)
                .collect::<Vec<_>>(),
            serial
                .documents
                .iter()
                .map(|document| &document.term_counts)
                .collect::<Vec<_>>()
        );
    });
}

#[test]
fn annotation_chunk_parallel_many_small_documents_matches_serial_reference() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let token_budget = annotation_wave_token_budget();
        let words_per_doc = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP / 2 + 1;
        let doc_count = (token_budget.saturating_mul(2) / words_per_doc + 2).max(64);
        let owned_texts = (0..doc_count)
            .map(|index| {
                (
                    format!("doc-{index:03}.txt"),
                    repeated_word_text("word", words_per_doc),
                )
            })
            .collect::<Vec<_>>();
        let total_words = doc_count.saturating_mul(words_per_doc);
        assert!(total_words > token_budget.saturating_mul(2));

        let texts = owned_texts
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let expected_paths = owned_texts
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        let language_overrides = english_overrides(texts.len());

        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let (_, parallel) = annotation_layer_for(&texts, language_overrides, true);

        assert_eq!(parallel, serial);
        assert_eq!(
            parallel
                .documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>(),
            expected_paths
        );
        assert_eq!(
            parallel
                .documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>(),
            serial
                .documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>()
        );
    });
}

#[test]
fn annotation_chunk_parallel_model_loads_are_bounded_per_worker() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let (owned_texts, total_words, token_budget) = multi_wave_english_texts();
        assert!(total_words > token_budget.saturating_mul(2));
        let texts = owned_texts
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let timing = AnnotationBuildTimingCounters::new();
        let docs_done = AtomicUsize::new(0);

        let (_, layer) = annotation_layer_result_for_with_timing(
            &texts,
            english_overrides(texts.len()),
            true,
            &AtomicBool::new(false),
            &docs_done,
            Some(&timing),
        )
        .unwrap();

        assert_eq!(layer.documents.len(), texts.len());
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());

        let worker_count = rayon::current_num_threads().max(1) as u64;
        let model_load_actual = timing.model_load_actual.load(AtomicOrdering::Relaxed);
        let manager_instances_created = timing
            .manager_instances_created
            .load(AtomicOrdering::Relaxed);
        let model_load_attempts = timing.model_load_attempts.load(AtomicOrdering::Relaxed);

        // For this same-language English fixture, actual loads are bounded by
        // workers. A multi-language fixture would be bounded by workers times
        // languages used.
        assert!(
            model_load_actual <= worker_count,
            "actual model loads {model_load_actual} exceeded worker count {worker_count}"
        );
        assert!(
            manager_instances_created <= worker_count,
            "manager instances {manager_instances_created} exceeded worker count {worker_count}"
        );
        assert!(model_load_attempts >= model_load_actual);
        assert!(
            model_load_attempts > model_load_actual,
            "multi-chunk fixture should exercise cached model-load hits"
        );
    });
}

#[test]
fn annotation_segment_waves_oversized_document_matches_serial_reference() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let wave_budget = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP;
        let whale = repeated_word_text("whale", wave_budget * 3 + 23);
        let texts = [("whale.txt", whale.as_str())];
        let language_overrides = english_overrides(texts.len());
        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let timing = AnnotationBuildTimingCounters::new();
        let docs_done = AtomicUsize::new(0);

        let (_, segmented) = annotation_layer_result_for_with_wave_budget(
            &texts,
            language_overrides,
            &AtomicBool::new(false),
            &docs_done,
            Some(&timing),
            wave_budget,
        )
        .unwrap();

        assert_eq!(segmented, serial);
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());
        let segments_processed = timing
            .annotation_segments_processed
            .load(AtomicOrdering::Relaxed);
        assert_eq!(segments_processed, 4);
        assert!(segments_processed > 1);
    });
}

#[test]
fn annotation_segment_waves_mixed_corpus_matches_serial_reference() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let wave_budget = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP;
        let whale = repeated_word_text("whale", wave_budget * 4 + 37);
        let texts = [
            ("a-small.txt", "Cats run quickly."),
            ("b-whale.txt", whale.as_str()),
            ("c-small.txt", "Dogs jump."),
        ];
        let language_overrides = english_overrides(texts.len());
        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let timing = AnnotationBuildTimingCounters::new();
        let docs_done = AtomicUsize::new(0);

        let (_, segmented) = annotation_layer_result_for_with_wave_budget(
            &texts,
            language_overrides,
            &AtomicBool::new(false),
            &docs_done,
            Some(&timing),
            wave_budget,
        )
        .unwrap();

        assert_eq!(segmented, serial);
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());
        assert!(
            timing
                .annotation_segments_processed
                .load(AtomicOrdering::Relaxed)
                > 1
        );
    });
}

#[test]
fn annotation_segment_waves_single_giant_chunk_is_own_segment() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let wave_budget = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP / 4;
        let word_count = wave_budget + 7;
        assert!(word_count < crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP);
        let giant = repeated_word_text("giant", word_count);
        let texts = [("giant.txt", giant.as_str())];
        let language_overrides = english_overrides(texts.len());
        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let timing = AnnotationBuildTimingCounters::new();
        let docs_done = AtomicUsize::new(0);

        let (_, segmented) = annotation_layer_result_for_with_wave_budget(
            &texts,
            language_overrides,
            &AtomicBool::new(false),
            &docs_done,
            Some(&timing),
            wave_budget,
        )
        .unwrap();

        assert_eq!(segmented, serial);
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());
        assert_eq!(
            timing
                .annotation_segments_processed
                .load(AtomicOrdering::Relaxed),
            1
        );
    });
}

#[test]
fn annotation_segment_waves_under_budget_waves_use_existing_path() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let wave_budget = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP * 8;
        let texts = [
            ("a-small.txt", "Cats run quickly."),
            ("b-small.txt", "Dogs jump."),
            ("c-small.txt", "Birds sing."),
        ];
        let language_overrides = english_overrides(texts.len());
        let (_, serial) = annotation_layer_for(&texts, language_overrides.clone(), false);
        let timing = AnnotationBuildTimingCounters::new();
        let docs_done = AtomicUsize::new(0);

        let (_, parallel) = annotation_layer_result_for_with_wave_budget(
            &texts,
            language_overrides,
            &AtomicBool::new(false),
            &docs_done,
            Some(&timing),
            wave_budget,
        )
        .unwrap();

        assert_eq!(parallel, serial);
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());
        assert_eq!(
            timing
                .annotation_segments_processed
                .load(AtomicOrdering::Relaxed),
            0
        );
    });
}

#[test]
fn annotation_segment_waves_cancellation_between_segments() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();

    pool.install(|| {
        let wave_budget = crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP;
        let whale = repeated_word_text("whale", wave_budget * 20 + 3);
        let texts = [("whale.txt", whale.as_str())];
        let language_overrides = english_overrides(texts.len());
        let timing = AnnotationBuildTimingCounters::new();
        let cancel = AtomicBool::new(false);
        let docs_done = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                while timing
                    .annotation_segments_processed
                    .load(AtomicOrdering::Relaxed)
                    == 0
                {
                    std::thread::yield_now();
                }
                cancel.store(true, AtomicOrdering::Relaxed);
            });

            let result = annotation_layer_result_for_with_wave_budget(
                &texts,
                language_overrides,
                &cancel,
                &docs_done,
                Some(&timing),
                wave_budget,
            );

            match result {
                Err(error) => assert_eq!(error, AnnotationLayerBuildError::Cancelled),
                Ok(_) => panic!("segmented annotation build completed after cancellation"),
            }
        });
        assert_eq!(docs_done.load(AtomicOrdering::Relaxed), 0);
    });
}

#[test]
fn annotation_segment_token_credit_is_monotonic_and_exact() {
    let document_tokens = 101;
    let total_segment_tokens = 10;
    let mut processed_segment_tokens = 0usize;
    let mut credited_document_tokens = 0u64;
    let mut cumulative = Vec::new();

    for segment_tokens in [3usize, 3, 4] {
        processed_segment_tokens += segment_tokens;
        let credit = proportional_segment_token_credit(
            document_tokens,
            total_segment_tokens,
            processed_segment_tokens,
            credited_document_tokens,
        );
        credited_document_tokens += credit;
        cumulative.push(credited_document_tokens);
    }

    assert!(cumulative.windows(2).all(|window| window[0] <= window[1]));
    assert_eq!(credited_document_tokens, document_tokens);
    assert_eq!(proportional_segment_token_credit(0, 0, 0, 0), 0);
    assert_eq!(proportional_segment_token_credit(0, 10, 5, 0), 0);
    assert_eq!(proportional_segment_token_credit(10, 0, 0, 0), 0);
}

#[test]
fn annotation_chunk_parallel_completes_single_whale_document() {
    let whale = repeated_word_text(
        "word",
        crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP * 2 + 17,
    );
    let texts = [("whale.txt", whale.as_str())];
    let language_overrides = english_overrides(texts.len());
    let (corpus, layer) = annotation_layer_for(&texts, language_overrides, true);

    let annotated_token_total = layer.documents[0]
        .term_counts
        .iter()
        .map(|count| count.count)
        .sum::<u64>();

    assert_eq!(
        annotated_token_total,
        corpus.documents[0].summary.token_count
    );
    assert_eq!(
        annotated_token_total,
        (crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP * 2 + 17) as u64
    );
}

#[test]
fn annotation_chunk_parallel_preserves_punctuation_only_and_empty_documents() {
    let punctuation_texts = [("punctuation.txt", "!?...")];
    let punctuation_overrides = english_overrides(punctuation_texts.len());
    let (punctuation_corpus, punctuation_layer) =
        annotation_layer_for(&punctuation_texts, punctuation_overrides, true);

    assert_eq!(punctuation_corpus.documents[0].summary.token_count, 0);
    assert!(punctuation_layer.documents[0].term_counts.is_empty());
    assert!(punctuation_layer.lemma_table.is_empty());

    let empty_texts = [("empty.txt", "")];
    let empty_overrides = english_overrides(empty_texts.len());
    let (empty_corpus, empty_layer) = annotation_layer_for(&empty_texts, empty_overrides, true);

    assert!(empty_corpus.documents.is_empty());
    assert!(empty_layer.documents.is_empty());
}

#[test]
fn annotation_chunk_parallel_cancellation_after_input_load_reports_cancelled() {
    let texts = [("cancel.txt", "The quick brown fox jumps.")];
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let language_overrides = english_overrides(texts.len());
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(&texts, analysis_options, true).corpus;
    let cancel = AtomicBool::new(false);
    let docs_done = AtomicUsize::new(0);

    let result = build_annotation_layer_with_mapping(
        &corpus,
        &records,
        annotation_options(language_overrides),
        &cancel,
        |record| {
            cancel.store(true, AtomicOrdering::Relaxed);
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        true,
        &docs_done,
    );

    assert_eq!(result.unwrap_err(), AnnotationLayerBuildError::Cancelled);
    assert_eq!(docs_done.load(AtomicOrdering::Relaxed), 0);
}

#[test]
fn annotation_model_preflight_honours_cancel_before_loading_model() {
    let mut languages = BTreeSet::new();
    languages.insert("en".to_string());
    let cancel = AtomicBool::new(true);
    let timing = AnnotationBuildTimingCounters::new();
    let model_dir = default_model_dir();
    let mut manager = new_annotation_model_manager(&model_dir, Some(&timing));

    let result = preflight_annotation_languages(&languages, &mut manager, &cancel, Some(&timing));

    assert_eq!(result.unwrap_err(), AnnotationLayerBuildError::Cancelled);
    assert_eq!(timing.model_load_attempts.load(AtomicOrdering::Relaxed), 0);
}

#[test]
fn annotation_chunk_parallel_progress_counts_each_document_once() {
    let texts = [
        ("c.txt", "Cats run."),
        ("a.txt", "Dogs jump."),
        ("b.txt", "Birds sing."),
    ];
    let docs_done = AtomicUsize::new(0);
    let (_, layer) = annotation_layer_result_for(
        &texts,
        english_overrides(texts.len()),
        true,
        &AtomicBool::new(false),
        &docs_done,
    )
    .unwrap();

    assert_eq!(layer.documents.len(), texts.len());
    assert_eq!(docs_done.load(AtomicOrdering::Relaxed), texts.len());
}

#[test]
fn annotation_build_blocks_unsupported_language_before_tagging() {
    let texts = [("unsupported.txt", "Hallo welt")];
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "zz".to_string());
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(&texts, analysis_options, true).corpus;

    let result = build_annotation_layer_with_mapping(
        &corpus,
        &records,
        annotation_options(language_overrides),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        true,
        &std::sync::atomic::AtomicUsize::new(0),
    );

    assert_eq!(
        result.unwrap_err(),
        AnnotationLayerBuildError::UnsupportedLanguages {
            languages: vec!["zz".to_string()],
        }
    );
}

#[test]
fn annotation_identity_mismatch_is_error_not_skip() {
    let corpus = corpus_for(&[("identity.txt", "Cats run")]);
    let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
    let annotated = vec![AnnotatedToken {
        surface: "Missing".to_string(),
        lemma: "missing".to_string(),
        upos: UniversalPos::Noun,
        token_index: 0,
        features: HashMap::new(),
    }];

    let result = resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup);

    assert!(matches!(
        result,
        Err(AnnotationLayerBuildError::AlignmentOrIdentityMismatch { .. })
    ));
}

#[test]
fn annotation_surface_lookup_uses_raw_surface_terms_not_frequency_keys() {
    let corpus = corpus_for(&[("case.txt", "Beta beta")]);
    let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
    let upper_id = *surface_lookup.get("Beta").unwrap();
    let lower_id = *surface_lookup.get("beta").unwrap();
    assert_ne!(upper_id, lower_id);
    let annotated = vec![
        AnnotatedToken {
            surface: "Beta".to_string(),
            lemma: "beta".to_string(),
            upos: UniversalPos::Noun,
            token_index: 0,
            features: HashMap::new(),
        },
        AnnotatedToken {
            surface: "beta".to_string(),
            lemma: "beta".to_string(),
            upos: UniversalPos::Noun,
            token_index: 1,
            features: HashMap::new(),
        },
    ];

    let counts = resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup).unwrap();

    assert_eq!(
        counts,
        vec![
            LocalAnnotatedTermCount {
                surface_term_id: upper_id,
                lemma: "beta".to_string(),
                upos: UniversalPos::Noun,
                count: 1,
            },
            LocalAnnotatedTermCount {
                surface_term_id: lower_id,
                lemma: "beta".to_string(),
                upos: UniversalPos::Noun,
                count: 1,
            },
        ]
    );
}

#[test]
fn annotation_local_counts_group_duplicates_by_surface_lemma_and_upos() {
    let corpus = corpus_for(&[("grouping.txt", "Alpha alpha Alpha")]);
    let surface_lookup = build_surface_term_lookup(&corpus).unwrap();
    let upper_id = *surface_lookup.get("Alpha").unwrap();
    let lower_id = *surface_lookup.get("alpha").unwrap();
    let annotated = vec![
        AnnotatedToken {
            surface: "Alpha".to_string(),
            lemma: "alpha".to_string(),
            upos: UniversalPos::Noun,
            token_index: 0,
            features: HashMap::new(),
        },
        AnnotatedToken {
            surface: "alpha".to_string(),
            lemma: "alpha".to_string(),
            upos: UniversalPos::Noun,
            token_index: 1,
            features: HashMap::new(),
        },
        AnnotatedToken {
            surface: "Alpha".to_string(),
            lemma: "alpha".to_string(),
            upos: UniversalPos::Noun,
            token_index: 2,
            features: HashMap::new(),
        },
        AnnotatedToken {
            surface: "Alpha".to_string(),
            lemma: "alpha".to_string(),
            upos: UniversalPos::Verb,
            token_index: 3,
            features: HashMap::new(),
        },
    ];

    let counts = resolve_annotated_tokens_to_local_counts(0, &annotated, &surface_lookup).unwrap();
    let mut expected = vec![
        LocalAnnotatedTermCount {
            surface_term_id: upper_id,
            lemma: "alpha".to_string(),
            upos: UniversalPos::Noun,
            count: 2,
        },
        LocalAnnotatedTermCount {
            surface_term_id: upper_id,
            lemma: "alpha".to_string(),
            upos: UniversalPos::Verb,
            count: 1,
        },
        LocalAnnotatedTermCount {
            surface_term_id: lower_id,
            lemma: "alpha".to_string(),
            upos: UniversalPos::Noun,
            count: 1,
        },
    ];
    expected.sort_by(|left, right| {
        left.surface_term_id
            .cmp(&right.surface_term_id)
            .then_with(|| left.lemma.cmp(&right.lemma))
            .then_with(|| left.upos.as_upos_str().cmp(right.upos.as_upos_str()))
    });

    assert_eq!(counts, expected);
}

#[test]
fn annotation_projection_discards_punctuation_and_renumbers_word_tokens() {
    let input = AnnotationDocumentInput {
        analysis_document_index: 0,
        source_path: "projection.txt".to_string(),
        relative_path: "projection.txt".to_string(),
        text: "Hello, world!".to_string(),
        language: "en".to_string(),
    };
    let tokens = tokenize(&input.text, 0, &TokenizationConfig::default());
    let annotations = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| fake_annotation(token.surface, index as u32))
        .collect::<Vec<_>>();

    let projected = project_chunk_annotations_to_word_tokens(
        &input,
        &tokens,
        vec![(0..tokens.len(), annotations)],
    )
    .unwrap();

    assert_eq!(
        projected
            .iter()
            .map(|token| token.surface.as_str())
            .collect::<Vec<_>>(),
        vec!["Hello", "world"]
    );
    assert_eq!(
        projected
            .iter()
            .map(|token| token.token_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn annotation_projection_allows_punctuation_only_documents() {
    let input = AnnotationDocumentInput {
        analysis_document_index: 0,
        source_path: "punctuation.txt".to_string(),
        relative_path: "punctuation.txt".to_string(),
        text: "!?...".to_string(),
        language: "en".to_string(),
    };
    let tokens = tokenize(&input.text, 0, &TokenizationConfig::default());

    let projected = project_chunk_annotations_to_word_tokens(&input, &tokens, vec![]).unwrap();

    assert!(projected.is_empty());
}

#[test]
fn annotation_layer_keeps_punctuation_out_of_counts_and_lemmas() {
    let texts = [("multi.txt", "Hello, world! Dogs run. \"Cats?\"")];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    let (corpus, layer) = annotation_layer_for(&texts, language_overrides, true);

    let annotated_token_total = layer.documents[0]
        .term_counts
        .iter()
        .map(|count| count.count)
        .sum::<u64>();
    assert_eq!(
        annotated_token_total,
        corpus.documents[0].summary.token_count
    );

    let punctuation = [",", "!", ".", "\"", "?"];
    let counted_surfaces = layer.documents[0]
        .term_counts
        .iter()
        .map(|count| {
            corpus.surface_terms[count.surface_term_id as usize]
                .token
                .as_str()
        })
        .collect::<Vec<_>>();
    assert!(
        counted_surfaces
            .iter()
            .all(|surface| !punctuation.contains(surface))
    );
    assert!(
        layer
            .lemma_table
            .entries()
            .iter()
            .all(|lemma| !punctuation.contains(&lemma.as_str()))
    );
}

#[test]
fn annotation_layer_allows_punctuation_only_documents() {
    let texts = [("punctuation.txt", "!?...")];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    let (corpus, layer) = annotation_layer_for(&texts, language_overrides, false);

    assert_eq!(corpus.documents[0].summary.token_count, 0);
    assert!(layer.documents[0].term_counts.is_empty());
    assert!(layer.lemma_table.is_empty());
}

#[test]
fn annotation_layer_completes_document_larger_than_sentence_cap() {
    let text = std::iter::repeat_n(
        "word",
        crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP + 3,
    )
    .collect::<Vec<_>>()
    .join(" ");
    let texts = [("long.txt", text.as_str())];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    let (corpus, layer) = annotation_layer_for(&texts, language_overrides, false);

    let annotated_token_total = layer.documents[0]
        .term_counts
        .iter()
        .map(|count| count.count)
        .sum::<u64>();

    assert_eq!(
        annotated_token_total,
        corpus.documents[0].summary.token_count
    );
    assert_eq!(
        annotated_token_total,
        (crate::annotation::ANNOTATION_SENTENCE_CHUNK_WORD_CAP + 3) as u64
    );
}

#[test]
fn annotation_layer_happy_path_populates_lemma_upos_and_surface_ids() {
    let texts = [("english.txt", "Dogs run")];
    let mut language_overrides = HashMap::new();
    language_overrides.insert(0, "en".to_string());
    let (corpus, layer) = annotation_layer_for(&texts, language_overrides, true);

    assert_eq!(layer.languages_used, vec!["en"]);
    assert_eq!(layer.documents.len(), corpus.documents.len());
    assert_eq!(layer.documents[0].analysis_document_index, 0);
    assert_eq!(layer.documents[0].relative_path, "english.txt");
    assert!(!layer.lemma_table.is_empty());

    let annotated_token_total = layer.documents[0]
        .term_counts
        .iter()
        .map(|count| count.count)
        .sum::<u64>();
    assert_eq!(
        annotated_token_total,
        corpus.documents[0].summary.token_count
    );
    assert!(
        layer.documents[0]
            .term_counts
            .iter()
            .any(|count| count.upos != UniversalPos::X)
    );
    for count in &layer.documents[0].term_counts {
        assert!((count.surface_term_id as usize) < corpus.surface_terms.len());
        assert!(layer.lemma_table.lookup(count.lemma_id).is_some());
    }
}

#[test]
fn test_annotation_cache_seam_none_root() {
    let corpus = corpus_for(&[("doc.txt", "The quick brown fox jumps over the lazy dog.")]);
    assert!(!corpus.overview().annotation_available);
    assert!(corpus.documents.iter().all(|doc| !doc.annotation_available));
    assert!(
        corpus
            .documents
            .iter()
            .all(|doc| doc.annotated_term_counts.is_empty())
    );
}

#[test]
fn test_annotation_cache_seam_missing_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus.overview().annotation_available);
    assert!(corpus.documents.iter().all(|doc| !doc.annotation_available));
}

#[test]
fn test_annotation_cache_seam_stale_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    // 1. Build a corpus to generate expected manifest
    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    let mut manifest = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    )
    .unwrap();
    // Make it stale
    manifest.schema_version += 1;

    let layer = AnnotationLayer {
        documents: vec![],
        lemma_table: LemmaTable::new(),
        languages_used: vec![],
    };

    crate::annotation_cache::write_annotation_layer_cache(
        temp_dir.path(),
        &manifest,
        &surface_term_table(&corpus),
        &layer,
    )
    .unwrap();

    // 2. Build with cache root
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus2 = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus2.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_invalid_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    let final_path = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    std::fs::write(&final_path, b"{ corrupt json").unwrap();

    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());

    let corpus = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_valid_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    // 1. Build unannotated corpus
    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    options.force_compute_hashes = true;
    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    // 2. Call build_annotation_layer to create a valid layer
    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };
    let layer = build_annotation_layer(
        &corpus,
        &records,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();

    // 3. Build manifest and write to cache
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

    // 4. Run fresh analysis build with cache root
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus_cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    // 5. Assertions
    assert!(corpus_cached.overview().annotation_available);
    assert!(
        corpus_cached
            .documents
            .iter()
            .all(|doc| doc.annotation_available)
    );
    assert!(!corpus_cached.documents[0].annotated_term_counts.is_empty());
    assert_eq!(corpus_cached.lemma_table, layer.lemma_table);

    // Verify query lights up
    let mut query = base_frequency_query();
    query.group_by = FrequencyGroupMode::Lemma;
    let page = query_frequency_list(&corpus_cached, &query);
    assert!(!page.items.is_empty());
    // Verify no AnnotationUnavailable notices
    assert!(
        page.notices
            .iter()
            .all(|n| n.kind != FrequencyNoticeKind::AnnotationUnavailable)
    );
}

#[test]
fn test_annotation_cache_seam_validation_reject_bad_layer_docs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc1.txt"), record("doc2.txt")];

    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };
    let mut layer = build_annotation_layer(
        &corpus,
        &records,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();

    // Mess up the documents in layer: make mismatched path/source
    layer.documents[1].analysis_document_index = 1;
    layer.documents[1].source_path = "mismatched".to_string();

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

    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus2 = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus2.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_validation_reject_bad_ids() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };
    let mut layer = build_annotation_layer(
        &corpus,
        &records,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();

    // Mess up term/lemma IDs: make surface_term_id out of range
    if !layer.documents[0].term_counts.is_empty() {
        layer.documents[0].term_counts[0].surface_term_id = 99999;
    }

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

    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus2 = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick brown fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus2.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_tokenisation_threading() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    let custom_config = TokenizationConfig {
        retain_ascii_hyphen_compounds: false,
        ..Default::default()
    };

    let mut options_custom = build_options();
    options_custom.tokenization = custom_config;
    options_custom.udpipe_model_dir = Some(default_model_dir());

    let corpus = build_analysis_corpus(
        &records,
        options_custom.clone(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick-brown-fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    // Expect custom tokenization config to flow into manifest
    let manifest = build_expected_manifest(
        &corpus,
        &records,
        options_custom.udpipe_model_dir.as_deref(),
        &options_custom.language_overrides,
    )
    .unwrap();
    assert!(!manifest.tokenization.config.retain_ascii_hyphen_compounds);

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options_custom.udpipe_model_dir.clone(),
        language_overrides: options_custom.language_overrides.clone(),
    };
    let layer = build_annotation_layer(
        &corpus,
        &records,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick-brown-fox jumps over the lazy dog.",
            ))
        },
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();

    crate::annotation_cache::write_annotation_layer_cache(
        temp_dir.path(),
        &manifest,
        &surface_term_table(&corpus),
        &layer,
    )
    .unwrap();

    // Build with default tokenization config (should read Stale)
    let mut options_default = build_options();
    options_default.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options_default.udpipe_model_dir = Some(default_model_dir());

    let corpus_default = build_analysis_corpus(
        &records,
        options_default,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                "The quick-brown-fox jumps over the lazy dog.",
            ))
        },
        |_| {},
    )
    .unwrap();

    assert!(!corpus_default.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_mixed_en_pt() {
    let temp_dir = tempfile::tempdir().unwrap();
    // doc1 is Portuguese, doc2 is English
    let records = vec![record("pt.txt"), record("en.txt")];
    let mut overrides = HashMap::new();
    overrides.insert(0, "pt".to_string());
    overrides.insert(1, "en".to_string());

    let mut options = build_options();
    options.language_overrides = overrides;
    options.udpipe_model_dir = Some(default_model_dir());
    options.force_compute_hashes = true;

    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("pt") {
                Ok(processed_resolution(record, "O gato preto correu."))
            } else {
                Ok(processed_resolution(record, "The quick brown fox jumps."))
            }
        },
        |_| {},
    )
    .unwrap();

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };
    let layer = build_annotation_layer(
        &corpus,
        &records,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("pt") {
                Ok(processed_resolution(record, "O gato preto correu."))
            } else {
                Ok(processed_resolution(record, "The quick brown fox jumps."))
            }
        },
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();

    let manifest = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    )
    .unwrap();
    // Assert English and Portuguese models both present in manifest
    assert!(manifest.models.iter().any(|m| m.language == "pt"));
    assert!(manifest.models.iter().any(|m| m.language == "en"));

    crate::annotation_cache::write_annotation_layer_cache(
        temp_dir.path(),
        &manifest,
        &surface_term_table(&corpus),
        &layer,
    )
    .unwrap();

    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus_cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("pt") {
                Ok(processed_resolution(record, "O gato preto correu."))
            } else {
                Ok(processed_resolution(record, "The quick brown fox jumps."))
            }
        },
        |_| {},
    )
    .unwrap();

    assert!(corpus_cached.overview().annotation_available);
}

#[test]
fn test_annotation_cache_seam_unsupported_language_preflight() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut overrides = HashMap::new();
    overrides.insert(0, "unsupported_lang".to_string());

    let mut options = build_options();
    options.language_overrides = overrides;
    options.udpipe_model_dir = Some(default_model_dir());

    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {},
    )
    .unwrap();

    // Attempting to build expected manifest should fail (ValidationError because unsupported_lang model is not found)
    let manifest_res = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    );
    assert!(manifest_res.is_err());

    // Run analysis build with cache root (should skip attach cleanly, leaving build successful)
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let corpus_res = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {},
    );

    assert!(corpus_res.is_ok());
    let corpus_built = corpus_res.unwrap();
    assert!(!corpus_built.overview().annotation_available);
}

#[test]
fn test_explicit_build_writes_ready_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let report = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(report.cache_written);
    assert!(!report.reused_existing);
    assert_eq!(report.documents_annotated, 1);
    assert!(report.total_annotated_term_count_rows > 0);

    // Assert we can read it and it's Ready
    let mut manifest_options = build_options();
    manifest_options.udpipe_model_dir = options.udpipe_model_dir.clone();
    manifest_options.force_compute_hashes = true;
    let corpus = build_analysis_corpus(
        &records,
        manifest_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();
    let manifest = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    )
    .unwrap();
    let load = crate::annotation_cache::load_annotation_layer_cache(temp_dir.path(), &manifest);
    assert!(matches!(
        load,
        crate::annotation_cache::AnnotationLayerCacheLoad::Ready(_)
    ));
}

#[test]
fn test_fresh_analysis_attaches_after_explicit_build() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    let corpus_cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(corpus_cached.overview().annotation_available);
}

#[test]
fn test_persisted_manifest_matches_reader() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    let bytes = std::fs::read(&cache_file).unwrap();
    let envelope: crate::annotation_cache::AnnotationLayerCacheEnvelope =
        serde_json::from_slice(&bytes).unwrap();

    let mut manifest_options = build_options();
    manifest_options.udpipe_model_dir = options.udpipe_model_dir.clone();
    manifest_options.force_compute_hashes = true;
    let corpus = build_analysis_corpus(
        &records,
        manifest_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();
    let manifest = build_expected_manifest(
        &corpus,
        &records,
        options.udpipe_model_dir.as_deref(),
        &options.language_overrides,
    )
    .unwrap();

    assert_eq!(envelope.manifest, manifest);
}

#[test]
fn test_annotation_timing_does_not_change_cache_output() {
    struct TimingReset;
    impl Drop for TimingReset {
        fn drop(&mut self) {
            TEST_ANNOTATION_TIMING_ENABLED.store(false, AtomicOrdering::Relaxed);
        }
    }

    let _reset = TimingReset;
    let records = vec![record("doc.txt")];

    let build_cache = |cache_root: &std::path::Path| {
        let mut options = build_options();
        options.annotation_cache_root = Some(cache_root.to_path_buf());
        options.udpipe_model_dir = Some(default_model_dir());
        let build_ann_options = AnnotationLayerBuildOptions {
            udpipe_model_dir: options.udpipe_model_dir.clone(),
            language_overrides: options.language_overrides.clone(),
        };

        build_and_write_annotation_cache(
            &records,
            options,
            build_ann_options,
            &AtomicBool::new(false),
            |record| Ok(processed_resolution(record, "The quick brown fox.")),
            |_| {},
        )
        .unwrap();

        let cache_file = cache_root.join(format!(
            "annotation-layer.v{}.json",
            crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
        ));
        let bytes = std::fs::read(cache_file).unwrap();
        serde_json::from_slice::<crate::annotation_cache::AnnotationLayerCacheEnvelope>(&bytes)
            .unwrap()
    };

    let untimed_dir = tempfile::tempdir().unwrap();
    TEST_ANNOTATION_TIMING_ENABLED.store(false, AtomicOrdering::Relaxed);
    let untimed = build_cache(untimed_dir.path());

    let timed_dir = tempfile::tempdir().unwrap();
    TEST_ANNOTATION_TIMING_ENABLED.store(true, AtomicOrdering::Relaxed);
    let timed = build_cache(timed_dir.path());

    assert_eq!(untimed, timed);
}

#[test]
fn test_reused_surface_corpus_skips_rebuild_and_matches_rebuild_output() {
    let records = vec![record("doc.txt")];
    let text = "The quick brown fox.";
    let mut surface_options = build_options();
    surface_options.udpipe_model_dir = Some(default_model_dir());
    surface_options.force_compute_hashes = true;
    let active_corpus = build_analysis_corpus(
        &records,
        surface_options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, text)),
        |_| {},
    )
    .unwrap();

    let reuse_dir = tempfile::tempdir().unwrap();
    let mut reuse_options = surface_options.clone();
    reuse_options.annotation_cache_root = Some(reuse_dir.path().to_path_buf());
    let reuse_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: reuse_options.udpipe_model_dir.clone(),
        language_overrides: reuse_options.language_overrides.clone(),
    };
    let reuse_load_calls = Arc::new(AtomicUsize::new(0));
    let reuse_load_calls_for_closure = reuse_load_calls.clone();
    let mut reuse_progress = Vec::new();
    let reuse_report = build_and_write_annotation_cache_with_reusable_surface_corpus(
        &records,
        reuse_options,
        reuse_ann_options,
        &AtomicBool::new(false),
        |record| {
            reuse_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(processed_resolution(record, text))
        },
        |progress| reuse_progress.push(progress),
        Some(&active_corpus),
    )
    .unwrap();
    assert!(reuse_report.cache_written);
    assert_eq!(reuse_load_calls.load(AtomicOrdering::Relaxed), 1);
    assert!(reuse_progress.iter().all(|progress| {
        !matches!(
            progress.current_phase,
            AnnotationCacheBuildPhase::BuildingSurface { .. }
        )
    }));

    let ready_load_calls = Arc::new(AtomicUsize::new(0));
    let ready_load_calls_for_closure = ready_load_calls.clone();
    let mut ready_options = surface_options.clone();
    ready_options.annotation_cache_root = Some(reuse_dir.path().to_path_buf());
    let ready_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: ready_options.udpipe_model_dir.clone(),
        language_overrides: ready_options.language_overrides.clone(),
    };
    let ready_report = build_and_write_annotation_cache_with_reusable_surface_corpus(
        &records,
        ready_options,
        ready_ann_options,
        &AtomicBool::new(false),
        |record| {
            ready_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(processed_resolution(record, text))
        },
        |_| {},
        Some(&active_corpus),
    )
    .unwrap();
    assert!(ready_report.reused_existing);
    assert_eq!(ready_load_calls.load(AtomicOrdering::Relaxed), 0);

    let rebuild_dir = tempfile::tempdir().unwrap();
    let mut rebuild_options = surface_options.clone();
    rebuild_options.annotation_cache_root = Some(rebuild_dir.path().to_path_buf());
    let rebuild_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: rebuild_options.udpipe_model_dir.clone(),
        language_overrides: rebuild_options.language_overrides.clone(),
    };
    let rebuild_load_calls = Arc::new(AtomicUsize::new(0));
    let rebuild_load_calls_for_closure = rebuild_load_calls.clone();
    let rebuild_report = build_and_write_annotation_cache(
        &records,
        rebuild_options,
        rebuild_ann_options,
        &AtomicBool::new(false),
        |record| {
            rebuild_load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(processed_resolution(record, text))
        },
        |_| {},
    )
    .unwrap();
    assert!(rebuild_report.cache_written);
    assert_eq!(rebuild_load_calls.load(AtomicOrdering::Relaxed), 2);
    assert_eq!(
        read_annotation_cache_envelope(reuse_dir.path()),
        read_annotation_cache_envelope(rebuild_dir.path())
    );
}

#[test]
fn test_reusable_surface_corpus_gate_rejects_identity_mismatches() {
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    options.force_compute_hashes = true;
    let corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();
    assert!(
        validate_reusable_surface_corpus(&corpus, &records, &options, &options.language_overrides)
            .is_ok()
    );

    let other_records = vec![record("other.txt")];
    assert!(
        validate_reusable_surface_corpus(
            &corpus,
            &other_records,
            &options,
            &options.language_overrides
        )
        .is_err()
    );

    let mut build_signature_mismatch = options.clone();
    build_signature_mismatch.build_signature = "different".to_string();
    assert!(
        validate_reusable_surface_corpus(
            &corpus,
            &records,
            &build_signature_mismatch,
            &options.language_overrides
        )
        .is_err()
    );

    let mut missing_hash = corpus.clone();
    missing_hash.documents[0].processed_hash_sha256.clear();
    assert!(
        validate_reusable_surface_corpus(
            &missing_hash,
            &records,
            &options,
            &options.language_overrides
        )
        .is_err()
    );

    let mut invalid_hash = corpus.clone();
    invalid_hash.documents[0].processed_hash_sha256 = "z".repeat(64);
    assert!(
        validate_reusable_surface_corpus(
            &invalid_hash,
            &records,
            &options,
            &options.language_overrides
        )
        .is_err()
    );

    let mut cleaning_mismatch = options.clone();
    cleaning_mismatch.processing_config_signature = "different".to_string();
    assert!(
        validate_reusable_surface_corpus(
            &corpus,
            &records,
            &cleaning_mismatch,
            &options.language_overrides
        )
        .is_err()
    );

    let mut tokenisation_mismatch = options.clone();
    tokenisation_mismatch
        .tokenization
        .retain_ascii_hyphen_compounds = !tokenisation_mismatch
        .tokenization
        .retain_ascii_hyphen_compounds;
    assert!(
        validate_reusable_surface_corpus(
            &corpus,
            &records,
            &tokenisation_mismatch,
            &options.language_overrides
        )
        .is_err()
    );

    let mut override_mismatch = HashMap::new();
    override_mismatch.insert(0, "pt".to_string());
    assert!(
        validate_reusable_surface_corpus(&corpus, &records, &options, &override_mismatch).is_err()
    );

    let mut missing_language = corpus.clone();
    missing_language.documents[0].summary.language = None;
    assert!(
        validate_reusable_surface_corpus(
            &missing_language,
            &records,
            &options,
            &options.language_overrides
        )
        .is_err()
    );
}

#[test]
fn test_rejected_surface_corpus_reuse_falls_back_to_rebuild() {
    let records = vec![record("doc.txt")];
    let text = "The quick brown fox.";
    let mut options = build_options();
    options.udpipe_model_dir = Some(default_model_dir());
    options.force_compute_hashes = true;
    let mut invalid_reuse_corpus = build_analysis_corpus(
        &records,
        options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, text)),
        |_| {},
    )
    .unwrap();
    invalid_reuse_corpus.documents[0]
        .processed_hash_sha256
        .clear();

    let temp_dir = tempfile::tempdir().unwrap();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };
    let load_calls = Arc::new(AtomicUsize::new(0));
    let load_calls_for_closure = load_calls.clone();
    let mut progress = Vec::new();
    let report = build_and_write_annotation_cache_with_reusable_surface_corpus(
        &records,
        options,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            load_calls_for_closure.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(processed_resolution(record, text))
        },
        |event| progress.push(event),
        Some(&invalid_reuse_corpus),
    )
    .unwrap();

    assert!(report.cache_written);
    assert_eq!(load_calls.load(AtomicOrdering::Relaxed), 2);
    assert!(progress.iter().any(|event| matches!(
        event.current_phase,
        AnnotationCacheBuildPhase::BuildingSurface { .. }
    )));
}

#[test]
fn test_ready_cache_skips_rebuild() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let report1 = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options.clone(),
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(report1.cache_written);
    assert!(!report1.reused_existing);

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    let mtime1 = std::fs::metadata(&cache_file).unwrap().modified().unwrap();

    let report2 = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(!report2.cache_written);
    assert!(report2.reused_existing);
    assert_eq!(report1.documents_annotated, report2.documents_annotated);
    assert_eq!(
        report1.total_annotated_term_count_rows,
        report2.total_annotated_term_count_rows
    );

    let mtime2 = std::fs::metadata(&cache_file).unwrap().modified().unwrap();
    assert_eq!(mtime1, mtime2);
}

#[test]
fn test_annotation_cache_build_reports_phase_progress() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc1.txt"), record("doc2.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let mut progress_events = Vec::new();
    let report = build_and_write_annotation_cache(
        &records,
        options,
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("doc1") {
                Ok(processed_resolution(record, "The quick brown fox."))
            } else {
                Ok(processed_resolution(record, "The slow red fox."))
            }
        },
        |progress| progress_events.push(progress),
    )
    .unwrap();

    assert_eq!(report.documents_annotated, 2);
    assert!(progress_events.iter().any(|progress| matches!(
        progress.current_phase,
        AnnotationCacheBuildPhase::BuildingSurface { .. }
    )));
    assert!(progress_events.iter().any(|progress| matches!(
        progress.current_phase,
        AnnotationCacheBuildPhase::LoadingModel
    )));
    assert!(progress_events.iter().any(|progress| matches!(
        progress.current_phase,
        AnnotationCacheBuildPhase::Annotating
    )));
    assert!(progress_events.iter().any(|progress| matches!(
        progress.current_phase,
        AnnotationCacheBuildPhase::WritingCache
    )));
    assert!(
        progress_events
            .iter()
            .any(|progress| matches!(progress.current_phase, AnnotationCacheBuildPhase::Completed))
    );
    let completed = progress_events
        .iter()
        .find(|progress| matches!(progress.current_phase, AnnotationCacheBuildPhase::Completed))
        .unwrap();
    assert!(completed.tokens_total > 0);
    assert_eq!(completed.tokens_done, completed.tokens_total);

    let annotating_done = progress_events
        .iter()
        .filter_map(|progress| match progress.current_phase {
            AnnotationCacheBuildPhase::Annotating => Some(progress.files_done),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!annotating_done.is_empty());
    assert!(
        annotating_done
            .windows(2)
            .all(|window| window[0] <= window[1])
    );
    assert_eq!(annotating_done.last(), Some(&2));

    let annotating_tokens_done = progress_events
        .iter()
        .filter_map(|progress| match progress.current_phase {
            AnnotationCacheBuildPhase::Annotating => Some(progress.tokens_done),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        annotating_tokens_done
            .windows(2)
            .all(|window| window[0] <= window[1])
    );
    assert_eq!(annotating_tokens_done.last(), Some(&completed.tokens_total));
}

#[test]
fn annotation_cache_progress_handles_zero_token_documents() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("punctuation.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = english_overrides(records.len());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let mut progress_events = Vec::new();
    let report = build_and_write_annotation_cache(
        &records,
        options,
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "!?...")),
        |progress| progress_events.push(progress),
    )
    .unwrap();

    let completed = progress_events
        .iter()
        .find(|progress| matches!(progress.current_phase, AnnotationCacheBuildPhase::Completed))
        .unwrap();
    assert_eq!(report.documents_annotated, 1);
    assert_eq!(completed.tokens_done, 0);
    assert_eq!(completed.tokens_total, 0);
}

#[test]
fn test_stale_or_invalid_replaced_after_full_build() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    std::fs::create_dir_all(temp_dir.path()).unwrap();
    std::fs::write(&cache_file, "INVALID JSON DATA").unwrap();

    let report = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "The quick brown fox.")),
        |_| {},
    )
    .unwrap();

    assert!(report.cache_written);
    assert!(!report.reused_existing);

    let bytes = std::fs::read(&cache_file).unwrap();
    assert!(
        serde_json::from_slice::<crate::annotation_cache::AnnotationLayerCacheEnvelope>(&bytes)
            .is_ok()
    );
}

#[test]
fn test_unsupported_language_writes_no_partial_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut overrides = HashMap::new();
    overrides.insert(0, "unsupported_lang".to_string());

    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = overrides;

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let res = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {},
    );

    assert!(matches!(
        res,
        Err(AnnotationCacheBuildError::ManifestError { .. })
    ));

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    assert!(!cache_file.exists());
}

#[test]
fn test_unsupported_language_does_not_overwrite_existing_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];

    // 1. Build a valid cache first
    let mut options_valid = build_options();
    options_valid.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options_valid.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options_valid = AnnotationLayerBuildOptions {
        udpipe_model_dir: options_valid.udpipe_model_dir.clone(),
        language_overrides: options_valid.language_overrides.clone(),
    };

    build_and_write_annotation_cache(
        &records,
        options_valid,
        build_ann_options_valid,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Valid content.")),
        |_| {},
    )
    .unwrap();

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));

    // Save the valid cache content
    let original_cache_content = std::fs::read(&cache_file).unwrap();

    // 2. Attempt a build with unsupported language
    let mut overrides = HashMap::new();
    overrides.insert(0, "unsupported_lang".to_string());

    let mut options_invalid = build_options();
    options_invalid.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options_invalid.udpipe_model_dir = Some(default_model_dir());
    options_invalid.language_overrides = overrides;

    let build_ann_options_invalid = AnnotationLayerBuildOptions {
        udpipe_model_dir: options_invalid.udpipe_model_dir.clone(),
        language_overrides: options_invalid.language_overrides.clone(),
    };

    let res = build_and_write_annotation_cache(
        &records,
        options_invalid,
        build_ann_options_invalid,
        &AtomicBool::new(false),
        |record| Ok(processed_resolution(record, "Hello world.")),
        |_| {},
    );

    assert!(matches!(
        res,
        Err(AnnotationCacheBuildError::ManifestError { .. })
    ));

    // 3. Verify the cache file is untouched
    let new_cache_content = std::fs::read(&cache_file).unwrap();
    assert_eq!(original_cache_content, new_cache_content);
}

#[test]
fn test_cancelled_build_writes_no_final_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("doc.txt")];
    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    let mut progress_events = Vec::new();
    let cancel = AtomicBool::new(true);
    let res = build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &cancel,
        |record| Ok(processed_resolution(record, "Hello world.")),
        |progress| progress_events.push(progress),
    );

    assert!(matches!(res, Err(AnnotationCacheBuildError::Cancelled)));
    assert!(
        progress_events
            .iter()
            .any(|progress| matches!(progress.current_phase, AnnotationCacheBuildPhase::Cancelled))
    );

    let cache_file = temp_dir.path().join(format!(
        "annotation-layer.v{}.json",
        crate::annotation_cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    assert!(!cache_file.exists());
}

#[test]
fn test_mixed_en_pt_round_trips_explicit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let records = vec![record("pt.txt"), record("en.txt")];
    let mut overrides = HashMap::new();
    overrides.insert(0, "pt".to_string());
    overrides.insert(1, "en".to_string());

    let mut options = build_options();
    options.annotation_cache_root = Some(temp_dir.path().to_path_buf());
    options.udpipe_model_dir = Some(default_model_dir());
    options.language_overrides = overrides;

    let build_ann_options = AnnotationLayerBuildOptions {
        udpipe_model_dir: options.udpipe_model_dir.clone(),
        language_overrides: options.language_overrides.clone(),
    };

    build_and_write_annotation_cache(
        &records,
        options.clone(),
        build_ann_options,
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("pt") {
                Ok(processed_resolution(record, "O gato preto correu."))
            } else {
                Ok(processed_resolution(record, "The quick brown fox jumps."))
            }
        },
        |_| {},
    )
    .unwrap();

    let corpus_cached = build_analysis_corpus(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            if record.source_path.to_string_lossy().contains("pt") {
                Ok(processed_resolution(record, "O gato preto correu."))
            } else {
                Ok(processed_resolution(record, "The quick brown fox jumps."))
            }
        },
        |_| {},
    )
    .unwrap();

    assert!(corpus_cached.overview().annotation_available);
}
