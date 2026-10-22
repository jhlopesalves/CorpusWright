//! Capture-only adapter: synthetic demonstration text goes through the real core, never a second analysis model.
use corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken;
use corpuswright_core::analysis::{
    AnalysisBuildOptions, CorpusOverviewContext, OverviewDistributionQuery, build_analysis_corpus,
    cleaning_config_signature, query_frequency_list, query_overview_distribution,
    resolve_analysis_input,
};
use corpuswright_core::corpus::word_count::count_words_for_record;
use corpuswright_core::sources::cache::{
    DocumentTextMode, ExtractionCache, document_text_for_record,
};
use corpuswright_core::text::tokenisation::TokenizationConfig;
use corpuswright_core::{
    AnnotatedFrequencyQuery, AnnotationCacheAvailability, AnnotationCacheState, CancellationToken,
    CleaningConfig, ConcordanceCollocationQuery, ConcordanceDocument, ConcordanceExpansionQuery,
    ConcordanceQuery, FrequencyQuery, PreviewOptions, expand_concordance_context, preview_files,
    preview_processed_files, query_annotated_frequency, query_concordance_collocations_documents,
    scan_directory, search_concordance_documents,
};
use serde_json::{Value, json};
use std::{collections::HashMap, env, error::Error, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    let root = Path::new(&args[1]);
    let command = &args[2];
    let request: Value = serde_json::from_str(&args[3])?;
    let scan = scan_directory(root).map_err(|error| format!("{error:?}"))?;
    let cleaning: CleaningConfig = serde_json::from_value(
        request
            .pointer("/request/cleaningConfig")
            .or_else(|| request.get("cleaningConfig"))
            .cloned()
            .unwrap_or(serde_json::to_value(CleaningConfig::default())?),
    )?;
    let tokenization = TokenizationConfig::default();
    let cache = ExtractionCache::new();
    let mut value = if command == "bootstrap" {
        let options = PreviewOptions {
            max_chars_per_file: None,
            ..Default::default()
        };
        json!({
            "scan": scan,
            "original": preview_files(&scan.files, &options, Some(&cache))
                .map_err(|error| format!("{error:?}"))?,
            "processed": preview_processed_files(&scan.files, &options, &cleaning, Some(&cache))
                .map_err(|error| format!("{error:?}"))?,
        })
    } else if command == "compute_word_count_window_command" {
        // Match the native command's batching while sharing its core counter.
        let offset = request["offset"].as_u64().unwrap_or(0) as usize;
        let limit = request["limit"].as_u64().unwrap_or(1_000).min(1_000) as usize;
        let outcomes = scan
            .files
            .iter()
            .skip(offset)
            .take(limit)
            .map(|record| count_words_for_record(record, &cleaning, Some(&cache)))
            .collect::<Vec<_>>();
        json!({
            "totalWords": outcomes.iter().map(|outcome| outcome.count as u64).sum::<u64>(),
            "skippedOcrMode": outcomes.iter().any(|outcome| outcome.skipped_ocr_mode),
            "processedCount": outcomes.len(),
            "totalCount": scan.files.len(),
        })
    } else if command == "search_concordance_command"
        || command == "query_concordance_collocations_command"
        || command == "expand_concordance_context_command"
    {
        let documents = scan
            .files
            .iter()
            .enumerate()
            .map(|(index, record)| {
                Ok(ConcordanceDocument {
                    document_index: index,
                    source_path: record.source_path.to_string_lossy().into_owned(),
                    display_name: record.relative_path.to_string_lossy().into_owned(),
                    text: document_text_for_record(
                        record,
                        &cleaning,
                        DocumentTextMode::Processed,
                        Some(&cache),
                    )?
                    .text,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let query = request.get("query").ok_or("Missing query")?.clone();
        match command.as_str() {
            "search_concordance_command" => serde_json::to_value(search_concordance_documents(
                &documents,
                &serde_json::from_value::<ConcordanceQuery>(query)?,
                &tokenization,
            )?)?,
            "query_concordance_collocations_command" => {
                serde_json::to_value(query_concordance_collocations_documents(
                    &documents,
                    &serde_json::from_value::<ConcordanceCollocationQuery>(query)?,
                    &tokenization,
                    &SurfaceIndexCancellationToken::new(),
                )?)?
            }
            _ => {
                let query: ConcordanceExpansionQuery = serde_json::from_value(query)?;
                serde_json::to_value(expand_concordance_context(
                    &documents[query.document_index],
                    &query,
                    &tokenization,
                )?)?
            }
        }
    } else {
        let analysis = build_analysis_corpus(
            &scan.files,
            AnalysisBuildOptions {
                built_at: "2026-10-05T12:00:00Z".into(),
                build_signature: request
                    .pointer("/request/buildSignature")
                    .and_then(Value::as_str)
                    .unwrap_or("public-product-fixture")
                    .into(),
                processing_config_signature: cleaning_config_signature(&cleaning),
                overview_context: CorpusOverviewContext {
                    sources_in_project: Some(scan.files.len()),
                    sources_with_warnings: Some(0),
                    failed_sources: 0,
                    config_version: None,
                    metadata_attached: false,
                },
                udpipe_model_dir: None,
                force_compute_hashes: true,
                language_overrides: HashMap::new(),
                tokenization,
                annotation_cache_root: None,
            },
            &CancellationToken::new(),
            |record| {
                Ok(resolve_analysis_input(
                    record,
                    &cleaning,
                    |record, config| {
                        document_text_for_record(
                            record,
                            config,
                            DocumentTextMode::Processed,
                            Some(&cache),
                        )
                        .map(|document| document.text)
                    },
                    |record, config| {
                        document_text_for_record(
                            record,
                            config,
                            DocumentTextMode::Original,
                            Some(&cache),
                        )
                        .map(|document| document.text)
                    },
                ))
            },
            |_| {},
        )?;
        match command.as_str() {
            "build_analysis_corpus_command" | "compute_corpus_overview_command" => {
                serde_json::to_value(analysis.overview())?
            }
            "query_overview_distribution_command" => {
                serde_json::to_value(query_overview_distribution(
                    &analysis,
                    &serde_json::from_value::<OverviewDistributionQuery>(request["query"].clone())?,
                )?)?
            }
            "query_frequency_list_command" => serde_json::to_value(query_frequency_list(
                &analysis,
                &serde_json::from_value::<FrequencyQuery>(request["query"].clone())?,
            ))?,
            "query_annotated_frequency_command" => {
                serde_json::to_value(query_annotated_frequency(
                    &analysis,
                    &serde_json::from_value::<AnnotatedFrequencyQuery>(request["query"].clone())?,
                    AnnotationCacheAvailability {
                        state: AnnotationCacheState::Missing,
                        detail: None,
                    },
                ))?
            }
            _ => return Err(format!("Unsupported capture command: {command}").into()),
        }
    };
    // The fixture has a logical, relative root. No temporary or personal path enters the renderer.
    replace_root(&mut value, &root.to_string_lossy());
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn replace_root(value: &mut Value, root: &str) {
    match value {
        Value::String(text) => *text = text.replace(root, "Demonstration corpus"),
        Value::Array(items) => items.iter_mut().for_each(|item| replace_root(item, root)),
        Value::Object(items) => items.values_mut().for_each(|item| replace_root(item, root)),
        _ => {}
    }
}
