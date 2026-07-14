use crate::features::analysis::state::AnalysisState;
use crate::features::corpus::state::{CorpusState, project_root_cache_key};
use crate::features::sources::concordance_document_from_record;
use crate::runtime::config::DesktopRuntimeConfig;
use crate::runtime::identity::{IndexIdentity, ProjectRootIdentity};
use corpuswright_core::analysis::{
    CompareExpressionsQuery, CompareExpressionsResult, ConcordanceCollocationQuery,
    ConcordanceCollocationResult, ConcordanceExpansionQuery, ConcordanceExpansionResult,
    ConcordanceQuery, ConcordanceResult, cleaning_config_signature, compare_expressions,
    concordance_collocation_aggregation_signature, expand_concordance_context,
    query_concordance_collocations_from_aggregate,
};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::sources::cache::ExtractionCache;
use corpuswright_core::text::tokenisation::TokenizationConfig;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;

use super::service::{
    ConcordanceBackendMode, aggregate_concordance_collocations_audit_mode,
    aggregate_concordance_collocations_scan_provider,
    aggregate_concordance_collocations_with_warm_index_provider,
    concordance_default_index_disabled_for_key, log_concordance_collocation_index_error,
    log_concordance_collocation_index_result,
    log_concordance_default_index_previous_error_recovery, log_concordance_default_index_recovery,
    log_concordance_default_index_recovery_scan_error, log_concordance_index_error,
    log_concordance_index_result, record_concordance_default_index_error,
    search_concordance_documents_audit_mode, search_concordance_documents_routed_scan_provider,
    search_concordance_with_warm_index_provider,
};
use super::state::{
    ConcordanceCollocationCache, ConcordanceCollocationCacheKey, ConcordanceIndexContextKey,
    ConcordanceIndexContextState,
};

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn search_concordance_command(
    query: ConcordanceQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    runtime: tauri::State<'_, DesktopRuntimeConfig>,
    app: tauri::AppHandle,
) -> Result<ConcordanceResult, String> {
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let mode = runtime.concordance_backend();
    let context_key = ConcordanceIndexContextKey {
        index: IndexIdentity::from_ipc(corpus_version, cleaning_config_signature(&cleaning_config)),
    };
    let index_context = index_context.context_handle();
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let tokenization_config = TokenizationConfig::default();
        match mode {
            ConcordanceBackendMode::DefaultIndex | ConcordanceBackendMode::StrictIndex => {
                if mode == ConcordanceBackendMode::DefaultIndex
                    && concordance_default_index_disabled_for_key(&index_context, &context_key)
                {
                    let scan_started = Instant::now();
                    let scan = search_concordance_documents_routed_scan_provider(
                        records.len(),
                        &query,
                        &tokenization_config,
                        |document_index| {
                            concordance_document_from_record(
                                &records[document_index],
                                document_index,
                                project_root.as_deref(),
                                &cleaning_config,
                                &cache,
                            )
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    log_concordance_default_index_previous_error_recovery(
                        &query,
                        &scan.result,
                        scan_started.elapsed(),
                    );
                    return Ok(scan.result);
                }
                let started = Instant::now();
                let cancellation = {
                    let cache_guard = index_context.lock().unwrap();
                    cache_guard.cancel.begin()
                };

                let routed = search_concordance_with_warm_index_provider(
                    records.len(),
                    &query,
                    &tokenization_config,
                    &index_context,
                    context_key.clone(),
                    |document_index| {
                        concordance_document_from_record(
                            &records[document_index],
                            document_index,
                            project_root.as_deref(),
                            &cleaning_config,
                            &cache,
                        )
                    },
                    |progress| {
                        let _ = app.emit("concordance-progress", progress);
                    },
                );

                {
                    let cache_guard = index_context.lock().unwrap();
                    cache_guard.cancel.finish(&cancellation);
                }
                match routed {
                    Ok(routed) => {
                        log_concordance_index_result(mode, &query, &routed, started.elapsed());
                        Ok(routed.result)
                    }
                    Err(error) => {
                        let index_elapsed = started.elapsed();
                        log_concordance_index_error(mode, &query, index_elapsed, &error);
                        if mode == ConcordanceBackendMode::StrictIndex
                            || error.prevents_unbounded_scan_fallback()
                        {
                            return Err(error.to_string());
                        }
                        let failure_state =
                            record_concordance_default_index_error(&index_context, &context_key);
                        let scan_started = Instant::now();
                        let scan = search_concordance_documents_routed_scan_provider(
                            records.len(),
                            &query,
                            &tokenization_config,
                            |document_index| {
                                concordance_document_from_record(
                                    &records[document_index],
                                    document_index,
                                    project_root.as_deref(),
                                    &cleaning_config,
                                    &cache,
                                )
                            },
                        );
                        let scan_elapsed = scan_started.elapsed();
                        match scan {
                            Ok(scan) => {
                                log_concordance_default_index_recovery(
                                    &query,
                                    &scan.result,
                                    index_elapsed,
                                    scan_elapsed,
                                    &error,
                                    failure_state,
                                );
                                Ok(scan.result)
                            }
                            Err(scan_error) => {
                                log_concordance_default_index_recovery_scan_error(
                                    &query,
                                    index_elapsed,
                                    scan_elapsed,
                                    &error,
                                    &scan_error,
                                    failure_state,
                                );
                                Err(scan_error.to_string())
                            }
                        }
                    }
                }
            }
            ConcordanceBackendMode::Scan => search_concordance_documents_routed_scan_provider(
                records.len(),
                &query,
                &tokenization_config,
                |document_index| {
                    concordance_document_from_record(
                        &records[document_index],
                        document_index,
                        project_root.as_deref(),
                        &cleaning_config,
                        &cache,
                    )
                },
            )
            .map(|routed| routed.result)
            .map_err(|error| error.to_string()),
            ConcordanceBackendMode::Audit => {
                let mut documents = Vec::new();
                for (document_index, record) in records.iter().enumerate() {
                    if let Some(document) = concordance_document_from_record(
                        record,
                        document_index,
                        project_root.as_deref(),
                        &cleaning_config,
                        &cache,
                    ) {
                        documents.push(document);
                    }
                }
                search_concordance_documents_audit_mode(
                    &documents,
                    &query,
                    &tokenization_config,
                    &index_context,
                    context_key,
                )
            }
        }
    })
    .await
    .map_err(|error| format!("Concordance query task failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn compare_expressions_command(
    query: CompareExpressionsQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CompareExpressionsResult, String> {
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut documents = Vec::new();
        for record in &records {
            let document_index = documents.len();
            if let Some(document) = concordance_document_from_record(
                record,
                document_index,
                project_root.as_deref(),
                &cleaning_config,
                &cache,
            ) {
                documents.push(document);
            }
        }
        compare_expressions(&documents, &query, &TokenizationConfig::default())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Compare expressions task failed: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn query_concordance_collocations_command(
    query: ConcordanceCollocationQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    runtime: tauri::State<'_, DesktopRuntimeConfig>,
    app: tauri::AppHandle,
) -> Result<ConcordanceCollocationResult, String> {
    let tokenization_config = TokenizationConfig::default();
    let aggregation = concordance_collocation_aggregation_signature(&query, &tokenization_config)
        .map_err(|error| error.to_string())?;
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let mode = runtime.concordance_backend();
    let project_root_key = project_root_cache_key(project_root.as_deref());
    let cleaning_config_signature = cleaning_config_signature(&cleaning_config);
    let context_key = ConcordanceIndexContextKey {
        index: IndexIdentity::from_ipc(corpus_version, cleaning_config_signature.clone()),
    };
    let cache_key = ConcordanceCollocationCacheKey {
        index: IndexIdentity::from_ipc(corpus_version, cleaning_config_signature),
        project_root: project_root_key.map(ProjectRootIdentity::new),
        selection: query.selection.clone(),
        aggregation,
    };
    if let Some(aggregate) = collocation_cache.get(&cache_key) {
        return Ok(query_concordance_collocations_from_aggregate(
            &aggregate, &query,
        ));
    }

    let cache = cache.inner().clone();
    let index_context = index_context.context_handle();
    let query_for_aggregation = query.clone();
    let tokenization_config = TokenizationConfig::default();
    tauri::async_runtime::spawn_blocking(move || match mode {
        ConcordanceBackendMode::DefaultIndex | ConcordanceBackendMode::StrictIndex => {
            let started = Instant::now();
            let cancellation = {
                let cache_guard = index_context.lock().unwrap();
                cache_guard.cancel.begin()
            };

            let routed = aggregate_concordance_collocations_with_warm_index_provider(
                records.len(),
                &query_for_aggregation,
                &tokenization_config,
                &index_context,
                context_key,
                &cancellation,
                |document_index| {
                    concordance_document_from_record(
                        &records[document_index],
                        document_index,
                        project_root.as_deref(),
                        &cleaning_config,
                        &cache,
                    )
                },
                |progress| {
                    let _ = app.emit("concordance-progress", progress);
                },
            );

            {
                let cache_guard = index_context.lock().unwrap();
                cache_guard.cancel.finish(&cancellation);
            }

            match routed {
                Ok(routed) => {
                    log_concordance_collocation_index_result(
                        mode,
                        &query_for_aggregation,
                        &routed,
                        started.elapsed(),
                    );
                    Ok(routed.aggregate)
                }
                Err(error) => {
                    log_concordance_collocation_index_error(
                        mode,
                        &query_for_aggregation,
                        started.elapsed(),
                        &error,
                    );
                    Err(error.to_string())
                }
            }
        }
        ConcordanceBackendMode::Scan | ConcordanceBackendMode::Audit => {
            let cancellation = {
                let cache_guard = index_context.lock().unwrap();
                cache_guard.cancel.begin()
            };
            let res = if mode == ConcordanceBackendMode::Scan {
                aggregate_concordance_collocations_scan_provider(
                    records.len(),
                    &query_for_aggregation,
                    &tokenization_config,
                    &cancellation,
                    |document_index| {
                        concordance_document_from_record(
                            &records[document_index],
                            document_index,
                            project_root.as_deref(),
                            &cleaning_config,
                            &cache,
                        )
                    },
                )
                .map(|r| r.aggregate)
                .map_err(|error| error.to_string())
            } else {
                aggregate_concordance_collocations_audit_mode(
                    records.len(),
                    &query_for_aggregation,
                    &tokenization_config,
                    &index_context,
                    context_key,
                    &cancellation,
                    |document_index| {
                        concordance_document_from_record(
                            &records[document_index],
                            document_index,
                            project_root.as_deref(),
                            &cleaning_config,
                            &cache,
                        )
                    },
                    |progress| {
                        let _ = app.emit("concordance-progress", progress);
                    },
                )
            };

            {
                let cache_guard = index_context.lock().unwrap();
                cache_guard.cancel.finish(&cancellation);
            }
            res
        }
    })
    .await
    .map_err(|error| format!("Concordance collocation task failed: {error}"))?
    .map(|aggregate| {
        let aggregate = Arc::new(aggregate);
        collocation_cache.insert(cache_key, Arc::clone(&aggregate));
        query_concordance_collocations_from_aggregate(&aggregate, &query)
    })
}

#[tauri::command]
pub(crate) fn cancel_concordance_command(
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
) {
    let cache_guard = index_context.inner.lock().unwrap();
    cache_guard.cancel.cancel();
}

#[tauri::command]
pub(crate) async fn expand_concordance_context_command(
    query: ConcordanceExpansionQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    analysis: tauri::State<'_, AnalysisState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<ConcordanceExpansionResult, String> {
    let analysis_corpus = analysis.corpus(&cleaning_config)?;
    let summary = analysis_corpus
        .document_summary(query.document_index)
        .cloned()
        .ok_or(corpuswright_core::analysis::ConcordanceError::InvalidDocumentIdentity)
        .map_err(|error| error.to_string())?;
    let (record, project_root) =
        corpus.record_for_path_and_root(Path::new(&summary.source_path), corpus_version)?;
    let record = record
        .ok_or(corpuswright_core::analysis::ConcordanceError::InvalidDocumentIdentity)
        .map_err(|error| error.to_string())?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let document = concordance_document_from_record(
            &record,
            query.document_index,
            project_root.as_deref(),
            &cleaning_config,
            &cache,
        )
        .ok_or(corpuswright_core::analysis::ConcordanceError::InvalidDocumentIdentity)
        .map_err(|error| error.to_string())?;
        expand_concordance_context(&document, &query, &TokenizationConfig::default())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Concordance expansion task failed: {error}"))?
}
