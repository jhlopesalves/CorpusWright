use crate::features::corpus::state::CorpusState;
use crate::features::pdf::service::duration_ms;
use crate::features::project;
use crate::features::sources::{project_annotation_cache_dir, resolve_analysis_input_for_record};
use corpuswright_core::analysis::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildProgress, AnalysisDocumentPage,
    AnalysisDocumentSelection, AnnotatedFrequencyQuery, AnnotatedFrequencyResult,
    AnnotatedTermDistributionQuery, AnnotationLayerBuildOptions, CorpusOverview,
    CorpusOverviewContext, FrequencyGroupMode, FrequencyPage, FrequencyQuery,
    OverviewDistributionEvidencePage, OverviewDistributionEvidenceQuery, OverviewDistributionQuery,
    OverviewDistributionResult, TermDistributionPage, TermDistributionQuery,
    annotation_cache_availability_for_corpus, build_analysis_corpus,
    build_and_write_annotation_cache_with_reusable_surface_corpus, cleaning_config_signature,
    query_analysis_documents, query_annotated_frequency, query_annotated_term_distribution,
    query_frequency_list, query_overview_distribution, query_overview_distribution_evidence,
    query_term_distribution,
};
use corpuswright_core::corpus::DocumentRecord;
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::sources::cache::ExtractionCache;
use corpuswright_core::text::language::resolve_effective_document_languages;
use corpuswright_core::text::tokenisation::TokenizationConfig;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

use super::annotation_state::AnnotationBuildState;
use super::state::{
    AnalysisBuildProgressPayload, AnalysisBuildRequest, AnalysisBuildStateKind,
    AnalysisBuildStatus, AnalysisState,
};

pub(crate) const ANNOTATION_TIMING_ENV: &str = "CORPUSWRIGHT_ANNOTATION_TIMING";

pub(crate) fn annotation_timing_enabled() -> bool {
    std::env::var(ANNOTATION_TIMING_ENV).as_deref() == Ok("1")
}

pub(crate) fn project_effective_language_overrides(
    project_root: Option<&Path>,
    records: &[DocumentRecord],
) -> Result<HashMap<usize, String>, String> {
    let Some(project_root) = project_root else {
        return Ok(HashMap::new());
    };
    let language = project::project_language_for_root(project_root)?;
    Ok(resolve_effective_document_languages(language, records))
}

pub(crate) fn analysis_progress_payload(
    progress: AnalysisBuildProgress,
    started_at: Instant,
) -> AnalysisBuildProgressPayload {
    AnalysisBuildProgressPayload {
        files_done: progress.files_done,
        files_total: progress.files_total,
        tokens_seen: progress.tokens_seen,
        types_seen: progress.types_seen,
        current_phase: progress.current_phase,
        elapsed_ms: duration_ms(started_at.elapsed()),
    }
}

#[tauri::command]
pub(crate) async fn build_annotation_cache_command(
    request: AnalysisBuildRequest,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    analysis: tauri::State<'_, AnalysisState>,
    annotation: tauri::State<'_, AnnotationBuildState>,
    window: Window,
) -> Result<
    corpuswright_core::analysis::AnnotationCacheBuildReport,
    corpuswright_core::analysis::AnnotationCacheBuildError,
> {
    let (records, project_root) = match request.indices {
        Some(indices) => corpus
            .records_and_root_for_indices(&indices, request.corpus_version)
            .map_err(|e| {
                corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed {
                    error: e,
                }
            })?,
        None => corpus
            .all_records_and_root(request.corpus_version)
            .map_err(|e| {
                corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed {
                    error: e,
                }
            })?,
    };
    if records.is_empty() {
        return Err(
            corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed {
                error: "No text is available for analysis.".to_string(),
            },
        );
    }

    let overview_context = if let Some(root) = project_root.as_deref() {
        let summary = inventory.summary(root).map_err(|e| {
            corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed { error: e }
        })?;
        CorpusOverviewContext {
            sources_in_project: Some(summary.total_count),
            sources_with_warnings: Some(summary.warning_count),
            failed_sources: summary.status_counts.failed,
            config_version: None,
            metadata_attached: false,
        }
    } else {
        CorpusOverviewContext {
            sources_in_project: Some(records.len()),
            sources_with_warnings: None,
            failed_sources: 0,
            config_version: None,
            metadata_attached: false,
        }
    };

    let cache_dir = project_root.as_deref().map(project_annotation_cache_dir);
    let language_overrides =
        project_effective_language_overrides(project_root.as_deref(), &records).map_err(|e| {
            corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed { error: e }
        })?;

    let udpipe_model_dir = window
        .app_handle()
        .path()
        .resolve("udpipe", BaseDirectory::Resource)
        .ok();

    let analysis_options = AnalysisBuildOptions {
        built_at: jiff::Timestamp::now().to_string(),
        build_signature: request.build_signature,
        processing_config_signature: cleaning_config_signature(&request.cleaning_config),
        overview_context,
        udpipe_model_dir: udpipe_model_dir.clone(),
        force_compute_hashes: false,
        language_overrides: language_overrides.clone(),
        tokenization: TokenizationConfig::default(),
        annotation_cache_root: cache_dir,
    };

    let annotation_options = AnnotationLayerBuildOptions {
        udpipe_model_dir,
        language_overrides,
    };

    let cleaning_config = request.cleaning_config;
    let cache_inner = cache.inner().clone();
    let reusable_surface_corpus = analysis.reusable_corpus_for_annotation();
    let cancel = annotation.begin();
    let cancel_for_task = cancel.clone();

    let task_result = tauri::async_runtime::spawn_blocking(move || {
        build_and_write_annotation_cache_with_reusable_surface_corpus(
            &records,
            analysis_options,
            annotation_options,
            &cancel_for_task,
            |record| {
                Ok(resolve_analysis_input_for_record(
                    record,
                    project_root.as_deref(),
                    &cleaning_config,
                    &cache_inner,
                ))
            },
            move |progress| {
                let _ = window.emit("annotation-build-progress", progress);
            },
            reusable_surface_corpus.as_deref(),
        )
    })
    .await
    .map_err(
        |e| corpuswright_core::analysis::AnnotationCacheBuildError::SurfaceBuildFailed {
            error: e.to_string(),
        },
    );

    annotation.finish(&cancel);

    let report = task_result??;

    Ok(report)
}

#[tauri::command]
pub(crate) fn cancel_annotation_cache_build_command(
    annotation: tauri::State<'_, AnnotationBuildState>,
) -> Result<(), String> {
    annotation.cancel();
    Ok(())
}

#[tauri::command]
pub(crate) async fn build_analysis_corpus_command(
    request: AnalysisBuildRequest,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    analysis: tauri::State<'_, AnalysisState>,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    window: Window,
) -> Result<CorpusOverview, String> {
    let (records, project_root) = match request.indices {
        Some(indices) => corpus.records_and_root_for_indices(&indices, request.corpus_version)?,
        None => corpus.all_records_and_root(request.corpus_version)?,
    };
    if records.is_empty() {
        return Err("No text is available for analysis.".to_string());
    }
    if analysis.inner.read().unwrap().state == AnalysisBuildStateKind::Building {
        return Err("An analysis corpus build is already running.".to_string());
    }

    let overview_context = if let Some(root) = project_root.as_deref() {
        let summary = inventory.summary(root)?;
        CorpusOverviewContext {
            sources_in_project: Some(summary.total_count),
            sources_with_warnings: Some(summary.warning_count),
            failed_sources: summary.status_counts.failed,
            config_version: None,
            metadata_attached: false,
        }
    } else {
        CorpusOverviewContext {
            sources_in_project: Some(records.len()),
            sources_with_warnings: None,
            failed_sources: 0,
            config_version: None,
            metadata_attached: false,
        }
    };
    let language_overrides =
        project_effective_language_overrides(project_root.as_deref(), &records)?;
    let annotation_cache_root = project_root.as_deref().map(project_annotation_cache_dir);

    analysis.cancel.store(false, Ordering::Relaxed);
    let build_generation = {
        let mut inner = analysis.inner.write().unwrap();
        inner.generation += 1;
        inner.state = AnalysisBuildStateKind::Building;
        inner.progress = None;
        inner.error = None;
        inner.corpus = None;
        inner.generation
    };

    let analysis_inner = analysis.inner.clone();
    let cancel = analysis.cancel.clone();
    let cache = cache.inner().clone();
    let started_at = Instant::now();
    let built_at = jiff::Timestamp::now().to_string();
    let build_signature = request.build_signature;
    let cleaning_config = request.cleaning_config;
    let udpipe_model_dir = window
        .app_handle()
        .path()
        .resolve("udpipe", BaseDirectory::Resource)
        .ok();
    let processing_config_signature = cleaning_config_signature(&cleaning_config);
    let task = tauri::async_runtime::spawn_blocking(move || {
        let mut last_emit = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        let mut last_phase = None;
        build_analysis_corpus(
            &records,
            AnalysisBuildOptions {
                built_at,
                build_signature,
                processing_config_signature,
                overview_context,
                udpipe_model_dir,
                force_compute_hashes: false,
                language_overrides,
                tokenization: TokenizationConfig::default(),
                annotation_cache_root,
            },
            &cancel,
            |record| {
                Ok(resolve_analysis_input_for_record(
                    record,
                    project_root.as_deref(),
                    &cleaning_config,
                    &cache,
                ))
            },
            |progress| {
                let phase_changed = last_phase != Some(progress.current_phase);
                let complete = progress.files_done == progress.files_total;
                if !phase_changed && !complete && last_emit.elapsed() < Duration::from_millis(100) {
                    return;
                }
                last_phase = Some(progress.current_phase);
                last_emit = Instant::now();
                let payload = analysis_progress_payload(progress, started_at);
                let mut inner = analysis_inner.write().unwrap();
                if inner.generation != build_generation {
                    return;
                }
                inner.progress = Some(payload.clone());
                drop(inner);
                let _ = window.emit("analysis-build-progress", payload);
            },
        )
    });

    let task_result = task.await;
    if analysis.inner.read().unwrap().generation != build_generation {
        return Err("Analysis build cancelled.".to_string());
    }
    match task_result {
        Ok(Ok(corpus)) => {
            let overview = corpus.overview().clone();
            if overview.document_count == 0 {
                let message = "No usable text is available for analysis.".to_string();
                let mut inner = analysis.inner.write().unwrap();
                inner.state = AnalysisBuildStateKind::Failed;
                inner.error = Some(message.clone());
                return Err(message);
            }
            let mut inner = analysis.inner.write().unwrap();
            inner.state = AnalysisBuildStateKind::Ready;
            inner.error = None;
            inner.corpus = Some(Arc::new(corpus));
            Ok(overview)
        }
        Ok(Err(AnalysisBuildError::Cancelled)) => {
            let mut inner = analysis.inner.write().unwrap();
            inner.state = AnalysisBuildStateKind::Cancelled;
            inner.error = None;
            Err("Analysis build cancelled.".to_string())
        }
        Ok(Err(error)) => {
            let message = error.to_string();
            let mut inner = analysis.inner.write().unwrap();
            inner.state = AnalysisBuildStateKind::Failed;
            inner.error = Some(message.clone());
            Err(message)
        }
        Err(error) => {
            let message = format!("Analysis build task failed: {error}");
            let mut inner = analysis.inner.write().unwrap();
            inner.state = AnalysisBuildStateKind::Failed;
            inner.error = Some(message.clone());
            Err(message)
        }
    }
}

#[tauri::command(async)]
pub(crate) fn cancel_analysis_build_command(analysis: tauri::State<'_, AnalysisState>) {
    analysis.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command(async)]
pub(crate) fn get_analysis_build_status_command(
    analysis: tauri::State<'_, AnalysisState>,
    cleaning_config: CleaningConfig,
) -> AnalysisBuildStatus {
    analysis.status(&cleaning_config)
}

#[tauri::command(async)]
pub(crate) fn compute_corpus_overview_command(
    analysis: tauri::State<'_, AnalysisState>,
    cleaning_config: CleaningConfig,
) -> Result<CorpusOverview, String> {
    Ok(analysis.corpus(&cleaning_config)?.overview().clone())
}

#[tauri::command]
pub(crate) async fn query_frequency_list_command(
    query: FrequencyQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<FrequencyPage, String> {
    let timing_enabled = annotation_timing_enabled();
    let started_at = Instant::now();
    if timing_enabled {
        eprintln!(
            "annotation-frequency-query entry offset={} limit={} group_by={:?} case_fold={} pos_filter={:?}",
            query.offset, query.limit, query.group_by, query.case_fold, query.pos_filter
        );
    }

    let corpus = match analysis.corpus(&cleaning_config) {
        Ok(corpus) => corpus,
        Err(error) => {
            if timing_enabled {
                eprintln!(
                    "annotation-frequency-query exit duration_ms={} error={:?}",
                    duration_ms(started_at.elapsed()),
                    error
                );
            }
            return Err(error);
        }
    };
    let result =
        tauri::async_runtime::spawn_blocking(move || query_frequency_list(&corpus, &query))
            .await
            .map_err(|error| format!("Frequency query task failed: {error}"));

    if timing_enabled {
        match &result {
            Ok(page) => {
                if let Some(error) = &page.query_error {
                    eprintln!(
                        "annotation-frequency-query exit duration_ms={} error={:?}",
                        duration_ms(started_at.elapsed()),
                        error
                    );
                } else {
                    eprintln!(
                        "annotation-frequency-query exit duration_ms={} rows={} total_terms={}",
                        duration_ms(started_at.elapsed()),
                        page.items.len(),
                        page.total_terms
                    );
                }
            }
            Err(error) => {
                eprintln!(
                    "annotation-frequency-query exit duration_ms={} error={:?}",
                    duration_ms(started_at.elapsed()),
                    error
                );
            }
        }
    }

    result
}

#[tauri::command]
pub(crate) async fn query_annotated_frequency_command(
    query: AnnotatedFrequencyQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    analysis: tauri::State<'_, AnalysisState>,
    window: Window,
) -> Result<AnnotatedFrequencyResult, String> {
    let analysis_corpus = analysis.corpus(&cleaning_config)?;
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let language_overrides =
        project_effective_language_overrides(project_root.as_deref(), &records)?;
    let cache_root = project_root.as_deref().map(project_annotation_cache_dir);
    let udpipe_model_dir = window
        .app_handle()
        .path()
        .resolve("udpipe", BaseDirectory::Resource)
        .ok();
    tauri::async_runtime::spawn_blocking(move || {
        let cache = annotation_cache_availability_for_corpus(
            &analysis_corpus,
            &records,
            cache_root.as_deref(),
            udpipe_model_dir.as_deref(),
            &language_overrides,
        );
        query_annotated_frequency(&analysis_corpus, &query, cache)
    })
    .await
    .map_err(|error| format!("Annotated frequency query task failed: {error}"))
}

#[tauri::command]
pub(crate) async fn query_analysis_documents_command(
    selection: AnalysisDocumentSelection,
    query: String,
    offset: usize,
    limit: usize,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<AnalysisDocumentPage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || {
        query_analysis_documents(&corpus, &selection, &query, offset, limit)
    })
    .await
    .map_err(|error| format!("Analysis document query task failed: {error}"))
}

#[tauri::command]
pub(crate) async fn query_overview_distribution_command(
    query: OverviewDistributionQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<OverviewDistributionResult, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || query_overview_distribution(&corpus, &query))
        .await
        .map_err(|error| format!("Overview distribution query task failed: {error}"))?
}

#[tauri::command]
pub(crate) async fn query_overview_distribution_evidence_command(
    query: OverviewDistributionEvidenceQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<OverviewDistributionEvidencePage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || {
        query_overview_distribution_evidence(&corpus, &query)
    })
    .await
    .map_err(|error| format!("Overview distribution evidence task failed: {error}"))?
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn query_term_distribution_command(
    token: String,
    selection: AnalysisDocumentSelection,
    group_by: FrequencyGroupMode,
    case_fold: bool,
    language: Option<String>,
    offset: usize,
    limit: usize,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<TermDistributionPage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || {
        query_term_distribution(
            &corpus,
            &token,
            &TermDistributionQuery {
                selection,
                group_by,
                case_fold,
                language,
                offset,
                limit,
            },
        )
    })
    .await
    .map_err(|error| format!("Term distribution query task failed: {error}"))
}

#[tauri::command]
pub(crate) async fn query_annotated_term_distribution_command(
    query: AnnotatedTermDistributionQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<TermDistributionPage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || query_annotated_term_distribution(&corpus, &query))
        .await
        .map_err(|error| format!("Annotated term distribution query task failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use corpuswright_core::corpus::DocumentType;
    use corpuswright_core::preparation::cleaning::PdfOcrQuality;
    use corpuswright_core::project::sources::{
        ProjectSourceExtractionRoute, ProjectSourcePdfProfile,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "corpuswright-desktop-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn pdf_record(
        name: &str,
        profile: Option<ProjectSourcePdfProfile>,
        route: Option<ProjectSourceExtractionRoute>,
    ) -> DocumentRecord {
        DocumentRecord {
            source_path: PathBuf::from(name),
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Pdf,
            size_bytes: 0,
            extraction_route: route,
            pdf_profile: profile,
            ocr_quality: Some(PdfOcrQuality::Balanced),
        }
    }

    #[test]
    fn analysis_request_uses_project_language() {
        let root = unique_temp_dir("analysis-project-language");
        let project =
            project::create_corpus_project_command(corpuswright_core::CreateCorpusProjectRequest {
                root_path: root.to_string_lossy().into_owned(),
                name: "Project Language".to_string(),
                language: corpuswright_core::ProjectLanguage::Pt,
                description: None,
            })
            .unwrap();
        let records = vec![
            pdf_record("first.txt", None, None),
            pdf_record("second.txt", None, None),
        ];

        let overrides =
            project_effective_language_overrides(Some(Path::new(&project.root_path)), &records)
                .unwrap();

        assert_eq!(overrides.len(), records.len());
        assert!(overrides.values().all(|language| language == "pt"));
        let _ = fs::remove_dir_all(root);
    }
}
