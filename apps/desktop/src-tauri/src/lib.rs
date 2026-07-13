#[cfg(test)]
use corpuswright_core::analysis::aggregate_concordance_collocations_documents;
use corpuswright_core::analysis::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildProgress, AnalysisCorpus,
    AnalysisDocumentPage, AnalysisDocumentSelection, AnnotatedFrequencyQuery,
    AnnotatedFrequencyResult, AnnotatedTermDistributionQuery, CompareExpressionsQuery,
    CompareExpressionsResult, ConcordanceBackend, ConcordanceBackendUsed,
    ConcordanceCollocationAggregate, ConcordanceCollocationAggregationSignature,
    ConcordanceCollocationQuery, ConcordanceCollocationResult, ConcordanceDocument,
    ConcordanceExpansionQuery, ConcordanceExpansionResult, ConcordanceFallbackReason,
    ConcordanceQuery, ConcordanceResult, ConcordanceRow, ConcordanceSearchContext, CorpusOverview,
    CorpusOverviewContext, FrequencyGroupMode, FrequencyPage, FrequencyQuery,
    OverviewDistributionEvidencePage, OverviewDistributionEvidenceQuery, OverviewDistributionQuery,
    OverviewDistributionResult, RoutedConcordanceCollocationAggregate, RoutedConcordanceResult,
    TermDistributionPage, TermDistributionQuery,
    aggregate_concordance_collocations_routed_provider,
    aggregate_concordance_collocations_scan_provider, annotation_cache_availability_for_corpus,
    build_analysis_corpus, cleaning_config_signature, compare_expressions,
    concordance_collocation_aggregation_signature, expand_concordance_context,
    query_analysis_documents, query_annotated_frequency, query_annotated_term_distribution,
    query_concordance_collocations_from_aggregate, query_frequency_list,
    query_overview_distribution, query_overview_distribution_evidence, query_term_distribution,
    resolve_analysis_input, search_concordance_documents, search_concordance_documents_routed,
    search_concordance_documents_routed_provider,
    search_concordance_documents_routed_scan_provider,
};
use corpuswright_core::cache::{
    CacheEntry, DocumentTextMode, ExtractionCache, document_text_for_record,
    document_text_from_cache_entry,
};
use corpuswright_core::clean::{CleaningConfig, PdfOcrQuality, PdfTextSource, clean_text};
use corpuswright_core::export::{ExportError, ExportOptions, ExportReport, export_corpus};
use corpuswright_core::pdf::{
    PdfExtractionOptions, PdfPageRangeResult, extract_pdf_page_range, pdf_page_count,
};
use corpuswright_core::pdf_audit::{PdfAuditResult, audit_pdf_files};
use corpuswright_core::preview::{
    CombinedPreview, PreviewOptions, preview_file_with_config, preview_files_with_config,
    preview_processed_files,
};
use corpuswright_core::project_sources::{
    ProjectSource, ProjectSourceExtractionResult, ProjectSourceExtractionRoute, ProjectSourceKind,
    ProjectSourcePdfProfile, ProjectSourcePreviewItem, ProjectSourcePreviewViewport,
    ProjectSourceProcessingResult, ProjectSourceSelectionDescriptor, ProjectSourceStatus,
    stable_project_source_id, update_project_source_extraction_results,
    update_project_source_processing_results,
};
use corpuswright_core::repeated_artifacts::{
    CancellationFlag, RepeatedArtifactScanConfig, RepeatedArtifactScanError,
    RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{
    CorpusSummary, DocumentRecord, DocumentType, ScanReport, load_files, scan_directory,
};
use corpuswright_core::search::{SearchResult, search_corpus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

use rayon::prelude::*;

mod project;
mod services;
mod state;

use services::*;
use state::*;

const ANNOTATION_TIMING_ENV: &str = "CORPUSWRIGHT_ANNOTATION_TIMING";

fn annotation_timing_enabled() -> bool {
    std::env::var(ANNOTATION_TIMING_ENV).as_deref() == Ok("1")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCorpusLoadResult {
    summary: CorpusSummary,
    corpus_version: u64,
    record_count: usize,
}

#[derive(Clone, Serialize)]
struct ExportProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSourcesProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSourcesReport {
    total_count: usize,
    processed_count: usize,
    failed_count: usize,
    skipped_count: usize,
    output_dir: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordCountBatchResult {
    total_words: u64,
    skipped_ocr_mode: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordCountWindowResult {
    total_words: u64,
    skipped_ocr_mode: bool,
    processed_count: usize,
    total_count: usize,
}

#[tauri::command(async)]
fn scan_directory_command(
    path: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<CorpusLoadResult, String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    let report = scan_directory(&path).map_err(|e| format!("{:?}", e))?;
    let version = {
        let mut inner = corpus_state.inner.write().unwrap();
        inner.load(Some(PathBuf::from(&path)), report.files.clone());
        inner.version
    };
    Ok(CorpusLoadResult {
        report,
        corpus_version: version,
    })
}

#[tauri::command(async)]
fn load_files_command(
    paths: Vec<String>,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<CorpusLoadResult, String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    let report = load_files(path_bufs).map_err(|e| format!("{:?}", e))?;
    let version = {
        let mut inner = corpus_state.inner.write().unwrap();
        inner.load(None, report.files.clone());
        inner.version
    };
    Ok(CorpusLoadResult {
        report,
        corpus_version: version,
    })
}

#[tauri::command(async)]
fn load_project_source_records_command(
    root_path: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<ProjectCorpusLoadResult, String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    let report = project::load_project_source_records(root_path)?;
    hydrate_project_source_text_cache(&report.root, &report.files, &cache);
    let version = {
        let mut inner = corpus_state.inner.write().unwrap();
        inner.load(Some(report.root.clone()), report.files.clone());
        inner.version
    };
    Ok(ProjectCorpusLoadResult {
        summary: report.summary,
        corpus_version: version,
        record_count: report.files.len(),
    })
}

#[tauri::command(async)]
fn audit_pdf_files_command(paths: Vec<String>) -> Result<Vec<PdfAuditResult>, String> {
    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    Ok(audit_pdf_files(path_bufs))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn load_pdf_intake_files_command(
    window: Window,
    paths: Vec<String>,
    cleaning_config: CleaningConfig,
    profile: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<CorpusLoadResult, String> {
    if paths.is_empty() {
        return Err("Choose at least one PDF.".to_string());
    }

    intake_state.cancel.store(false, Ordering::Relaxed);
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    corpus_state.inner.write().unwrap().clear();

    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    let report = load_files(path_bufs).map_err(|e| format!("{:?}", e))?;
    if report
        .files
        .iter()
        .any(|record| record.document_type != DocumentType::Pdf)
    {
        cache.clear();
        return Err("PDF intake accepts PDF files only.".to_string());
    }

    let total_files = report.files.len();
    for (file_index, record) in report.files.iter().enumerate() {
        if intake_state.cancel.load(Ordering::Relaxed) {
            cache.clear();
            page_cache.clear();
            return Err(pdf_intake_cancelled_message());
        }

        if let Err(error) = materialize_pdf_record(
            &window,
            record,
            file_index,
            total_files,
            &profile,
            &cleaning_config,
            &cache,
            &intake_state.cancel,
        ) {
            cache.clear();
            page_cache.clear();
            return Err(error);
        }
    }

    let version = {
        let mut inner = corpus_state.inner.write().unwrap();
        inner.load(None, report.files.clone());
        inner.version
    };
    Ok(CorpusLoadResult {
        report,
        corpus_version: version,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn prepare_project_pdf_sources_command(
    window: Window,
    root_path: String,
    paths: Vec<String>,
    cleaning_config: CleaningConfig,
    profile_override: Option<ProjectSourcePdfProfile>,
    ocr_quality: PdfOcrQuality,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<CorpusLoadResult, String> {
    if paths.is_empty() {
        return Err("Choose at least one project PDF source.".to_string());
    }

    intake_state.cancel.store(false, Ordering::Relaxed);
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    corpus_state.inner.write().unwrap().clear();

    if let Some(profile) = profile_override {
        project::update_project_source_pdf_profiles_command(
            root_path.clone(),
            paths.clone(),
            profile,
            ocr_quality,
        )?;
    }

    let selected_paths = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    let (project_root, project_sources) = project::load_project_source_inventory(root_path)?;
    let project_sources_by_path = project_sources
        .sources
        .iter()
        .map(|source| (canonical_or_original(&source.path), source))
        .collect::<HashMap<_, _>>();

    let mut report = load_files(selected_paths.clone()).map_err(|error| format!("{error:?}"))?;
    for record in &mut report.files {
        let key = canonical_or_original(&record.source_path);
        let project_source = project_sources_by_path.get(&key).ok_or_else(|| {
            format!(
                "Selected PDF source is not in the project inventory: {}",
                record.source_path.display()
            )
        })?;
        if project_source.kind != ProjectSourceKind::Pdf {
            return Err(format!(
                "Selected source is not a PDF: {}",
                record.source_path.display()
            ));
        }
        record.extraction_route = Some(project_source.extraction_route);
        record.pdf_profile = project_source.pdf_profile;
        record.ocr_quality = project_source.ocr_quality;
    }

    let plan = project_pdf_preparation_plan(&report.files, &selected_paths, profile_override)?;
    let records_by_path = selected_record_by_path(&report.files);
    let mut ordered_records = Vec::with_capacity(plan.len());
    let mut extraction_results = Vec::with_capacity(plan.len());
    let total_files = plan.len();
    let batch_label = project_pdf_preparation_batch_label(&plan);
    if total_files == 0 {
        return Err(format!(
            "No selected PDFs could be prepared with {batch_label}."
        ));
    }

    for (file_index, item) in plan.iter().enumerate() {
        if intake_state.cancel.load(Ordering::Relaxed) {
            let _ = update_project_source_extraction_results(
                &project_root,
                vec![ProjectSourceExtractionResult {
                    path: item.path.clone(),
                    failed: true,
                    warning_count: 1,
                    reason_codes: vec!["extraction_cancelled".to_string()],
                }],
            );
            cache.clear();
            page_cache.clear();
            return Err(pdf_intake_cancelled_message());
        }

        let record = records_by_path
            .get(&canonical_or_original(&item.path))
            .ok_or_else(|| {
                format!(
                    "Selected PDF source is not processable: {}",
                    item.path.display()
                )
            })?;

        let prepared_record = record_with_preparation_profile(record, item);
        let pdf_options = PdfExtractionOptions::raw_from_record_and_cleaning_config(
            &prepared_record,
            &cleaning_config,
        );
        let entry = match materialize_pdf_record(
            &window,
            &prepared_record,
            file_index,
            total_files,
            &item.profile_label,
            &cleaning_config,
            &cache,
            &intake_state.cancel,
        ) {
            Ok(entry) => entry,
            Err(error) => {
                let reason_code =
                    project_pdf_extraction_failure_reason(&error, &intake_state.cancel);
                let _ = update_project_source_extraction_results(
                    &project_root,
                    vec![ProjectSourceExtractionResult {
                        path: prepared_record.source_path.clone(),
                        failed: true,
                        warning_count: 1,
                        reason_codes: vec![reason_code.to_string()],
                    }],
                );
                cache.clear();
                page_cache.clear();
                return Err(error);
            }
        };
        persist_project_source_text_cache(&project_root, &prepared_record, pdf_options, &entry)?;
        extraction_results.push(ProjectSourceExtractionResult {
            path: prepared_record.source_path.clone(),
            failed: false,
            warning_count: entry.warnings.len(),
            reason_codes: extraction_reason_codes_for_warnings(&entry.warnings),
        });
        ordered_records.push(prepared_record);
    }

    update_project_source_extraction_results(&project_root, extraction_results)?;

    report.files = ordered_records;
    let version = {
        let mut inner = corpus_state.inner.write().unwrap();
        inner.load(None, report.files.clone());
        inner.version
    };
    Ok(CorpusLoadResult {
        report,
        corpus_version: version,
    })
}

#[tauri::command(async)]
fn cancel_pdf_intake_materialization_command(
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<(), String> {
    intake_state.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command(async)]
fn clear_corpus_command(
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<(), String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
    collocation_cache.clear();
    index_context.clear();
    corpus_state.inner.write().unwrap().clear();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn search_corpus_command(
    indices: Option<Vec<usize>>,
    source_paths: Option<Vec<String>>,
    root_path: Option<String>,
    all_matching_query: Option<String>,
    excluded_source_ids: Option<Vec<String>>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    query: String,
    is_processed: bool,
    cleaning_config: CleaningConfig,
    max_hits: usize,
    cache: tauri::State<'_, ExtractionCache>,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
) -> Result<SearchResult, String> {
    let records = if let (Some(root_path), Some(selection_query)) = (root_path, all_matching_query)
    {
        let root = project::validated_project_root(&root_path)?;
        let paths = inventory.matching_paths(
            &root,
            &selection_query,
            excluded_source_ids.as_deref().unwrap_or_default(),
        )?;
        corpus.records_for_paths(paths, corpus_version)?
    } else if let Some(paths) = source_paths {
        corpus.records_for_paths(
            paths.into_iter().map(PathBuf::from).collect(),
            corpus_version,
        )?
    } else if let Some(indices) = indices {
        corpus.records_for_indices(&indices, corpus_version)?
    } else {
        corpus.all_records_and_root(corpus_version)?.0
    };
    search_corpus(
        &records,
        &query,
        is_processed,
        &cleaning_config,
        max_hits,
        Some(&*cache),
    )
}

fn preview_char_limit_for_explicit_selection(
    selected_count: usize,
    requested_limit: Option<usize>,
) -> Option<usize> {
    if selected_count == 1 {
        None
    } else {
        Some(requested_limit.unwrap_or(5_000).max(1))
    }
}

fn preview_char_limit_for_project_selection(
    selection: &ProjectSourceSelectionDescriptor,
    requested_limit: Option<usize>,
) -> Option<usize> {
    if !selection.all_matching && selection.ids.len() == 1 {
        None
    } else {
        Some(requested_limit.unwrap_or(2_000).clamp(1, 10_000))
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn preview_files_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    max_chars_per_file: Option<usize>,
    include_paths: bool,
    max_files: Option<usize>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
    let max_chars_per_file =
        preview_char_limit_for_explicit_selection(indices.len(), max_chars_per_file);
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let options = PreviewOptions {
        max_chars_per_file,
        include_paths,
        max_files,
    };
    preview_files_with_config(&records, &options, &cleaning_config, Some(&*cache))
        .map_err(|e| format!("{:?}", e))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn preview_processed_files_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    max_chars_per_file: Option<usize>,
    include_paths: bool,
    max_files: Option<usize>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
    let max_chars_per_file =
        preview_char_limit_for_explicit_selection(indices.len(), max_chars_per_file);
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let options = PreviewOptions {
        max_chars_per_file,
        include_paths,
        max_files,
    };
    preview_processed_files(&records, &options, &cleaning_config, Some(&*cache))
        .map_err(|e| format!("{:?}", e))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn read_project_source_preview_command(
    root_path: String,
    source_id: String,
    corpus_version: u64,
    processed: bool,
    max_chars: usize,
    cleaning_config: CleaningConfig,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
    let root = project::validated_project_root(&root_path)?;
    let source = inventory
        .source_by_id(&root, &source_id)?
        .ok_or_else(|| "The selected project source no longer exists.".to_string())?;
    let record = corpus
        .record_for_path(&source.path, corpus_version)?
        .ok_or_else(|| "The selected project source is not loaded.".to_string())?;
    let records = vec![record];
    let options = PreviewOptions {
        max_chars_per_file: Some(max_chars.clamp(1, 250_000)),
        include_paths: true,
        max_files: Some(1),
    };

    if processed {
        preview_processed_files(&records, &options, &cleaning_config, Some(&*cache))
            .map_err(|error| format!("{error:?}"))
    } else {
        preview_files_with_config(&records, &options, &cleaning_config, Some(&*cache))
            .map_err(|error| format!("{error:?}"))
    }
}

fn project_source_has_previewable_text(source: &ProjectSource) -> bool {
    source.kind != ProjectSourceKind::Unknown
        && source.status != ProjectSourceStatus::Failed
        && (source.kind != ProjectSourceKind::Pdf
            || matches!(
                source.status,
                ProjectSourceStatus::TextExtracted
                    | ProjectSourceStatus::TextExtractedWithWarnings
                    | ProjectSourceStatus::Processed
                    | ProjectSourceStatus::ProcessedWithWarnings
            ))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn query_selected_source_preview_viewport_command(
    root_path: String,
    selection: ProjectSourceSelectionDescriptor,
    viewport_start: usize,
    viewport_size: usize,
    preview_text_cap: Option<usize>,
    processed: bool,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<ProjectSourcePreviewViewport, String> {
    let root = project::validated_project_root(&root_path)?;
    let max_chars_per_file = preview_char_limit_for_project_selection(&selection, preview_text_cap);
    let viewport =
        inventory.selection_viewport(&root, &selection, viewport_start, viewport_size)?;
    let options = PreviewOptions {
        max_chars_per_file,
        include_paths: true,
        max_files: Some(1),
    };
    let mut items = Vec::with_capacity(viewport.items.len());

    for source in viewport.items {
        let can_preview = project_source_has_previewable_text(&source);

        let (preview, preview_error) = if can_preview {
            match corpus.record_for_path(&source.path, corpus_version)? {
                Some(record) => {
                    let result = if processed {
                        preview_processed_files(
                            std::slice::from_ref(&record),
                            &options,
                            &cleaning_config,
                            Some(&*cache),
                        )
                        .map(|combined| combined.files.into_iter().next())
                    } else {
                        preview_file_with_config(&record, &options, &cleaning_config, Some(&*cache))
                            .map(Some)
                    };
                    match result {
                        Ok(preview) => (preview, None),
                        Err(error) => (None, Some(format!("{error:?}"))),
                    }
                }
                None => (
                    None,
                    Some("The selected project source is not loaded.".to_string()),
                ),
            }
        } else {
            (None, None)
        };

        items.push(ProjectSourcePreviewItem {
            source,
            preview,
            preview_error,
        });
    }

    Ok(ProjectSourcePreviewViewport {
        total_selected: viewport.total_selected,
        viewport_start: viewport.viewport_start,
        viewport_size: viewport.viewport_size,
        items,
    })
}

fn project_effective_language_overrides(
    project_root: Option<&Path>,
    records: &[DocumentRecord],
) -> Result<HashMap<usize, String>, String> {
    let Some(project_root) = project_root else {
        return Ok(HashMap::new());
    };
    let language = project::project_language_for_root(project_root)?;
    Ok(corpuswright_core::resolve_effective_document_languages(
        language, records,
    ))
}

fn analysis_progress_payload(
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
async fn build_annotation_cache_command(
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
        tokenization: corpuswright_core::tokenization::TokenizationConfig::default(),
        annotation_cache_root: cache_dir,
    };

    let annotation_options = corpuswright_core::analysis::AnnotationLayerBuildOptions {
        udpipe_model_dir,
        language_overrides,
    };

    let cleaning_config = request.cleaning_config;
    let cache_inner = cache.inner().clone();
    let reusable_surface_corpus = analysis.reusable_corpus_for_annotation();
    let cancel = annotation.begin();
    let cancel_for_task = cancel.clone();

    let task_result = tauri::async_runtime::spawn_blocking(move || {
        corpuswright_core::analysis::build_and_write_annotation_cache_with_reusable_surface_corpus(
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
fn cancel_annotation_cache_build_command(
    annotation: tauri::State<'_, AnnotationBuildState>,
) -> Result<(), String> {
    annotation.cancel();
    Ok(())
}

#[tauri::command]
async fn build_analysis_corpus_command(
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
                tokenization: corpuswright_core::tokenization::TokenizationConfig::default(),
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
fn cancel_analysis_build_command(analysis: tauri::State<'_, AnalysisState>) {
    analysis.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command(async)]
fn get_analysis_build_status_command(
    analysis: tauri::State<'_, AnalysisState>,
    cleaning_config: CleaningConfig,
) -> AnalysisBuildStatus {
    analysis.status(&cleaning_config)
}

#[tauri::command(async)]
fn compute_corpus_overview_command(
    analysis: tauri::State<'_, AnalysisState>,
    cleaning_config: CleaningConfig,
) -> Result<CorpusOverview, String> {
    Ok(analysis.corpus(&cleaning_config)?.overview().clone())
}

#[tauri::command]
async fn search_concordance_command(
    query: ConcordanceQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    app: tauri::AppHandle,
) -> Result<ConcordanceResult, String> {
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let mode = concordance_backend_mode_from_env();
    let context_key = ConcordanceIndexContextKey {
        corpus_version,
        cleaning_config_signature: cleaning_config_signature(&cleaning_config),
    };
    let index_context = index_context.context_handle();
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
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
                let cancellation = Arc::new(AtomicBool::new(false));
                {
                    let cache_guard = index_context.lock().unwrap();
                    *cache_guard.cancel.lock().unwrap() = Some(cancellation.clone());
                }

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
                    *cache_guard.cancel.lock().unwrap() = None;
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
async fn compare_expressions_command(
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
        compare_expressions(
            &documents,
            &query,
            &corpuswright_core::tokenization::TokenizationConfig::default(),
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Compare expressions task failed: {error}"))?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn query_concordance_collocations_command(
    query: ConcordanceCollocationQuery,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    collocation_cache: tauri::State<'_, ConcordanceCollocationCache>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    app: tauri::AppHandle,
) -> Result<ConcordanceCollocationResult, String> {
    let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
    let aggregation = concordance_collocation_aggregation_signature(&query, &tokenization_config)
        .map_err(|error| error.to_string())?;
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let mode = concordance_backend_mode_from_env();
    let project_root_key = project_root_cache_key(project_root.as_deref());
    let cleaning_config_signature = cleaning_config_signature(&cleaning_config);
    let context_key = ConcordanceIndexContextKey {
        corpus_version,
        cleaning_config_signature: cleaning_config_signature.clone(),
    };
    let cache_key = ConcordanceCollocationCacheKey {
        corpus_version,
        project_root: project_root_key,
        cleaning_config_signature,
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
    let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
    tauri::async_runtime::spawn_blocking(move || match mode {
        ConcordanceBackendMode::DefaultIndex | ConcordanceBackendMode::StrictIndex => {
            let started = Instant::now();
            let cancellation = Arc::new(AtomicBool::new(false));
            {
                let cache_guard = index_context.lock().unwrap();
                *cache_guard.cancel.lock().unwrap() = Some(cancellation.clone());
            }

            let routed = aggregate_concordance_collocations_with_warm_index_provider(
                records.len(),
                &query_for_aggregation,
                &tokenization_config,
                &index_context,
                context_key,
                &cancellation.clone().into(),
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
                *cache_guard.cancel.lock().unwrap() = None;
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
            let cancellation = Arc::new(AtomicBool::new(false));
            {
                let cache_guard = index_context.lock().unwrap();
                *cache_guard.cancel.lock().unwrap() = Some(cancellation.clone());
            }
            let res = if mode == ConcordanceBackendMode::Scan {
                aggregate_concordance_collocations_scan_provider(
                    records.len(),
                    &query_for_aggregation,
                    &tokenization_config,
                    &cancellation.clone().into(),
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
                    &cancellation.clone().into(),
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
                *cache_guard.cancel.lock().unwrap() = None;
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
fn cancel_concordance_command(index_context: tauri::State<'_, ConcordanceIndexContextState>) {
    let cache_guard = index_context.inner.lock().unwrap();
    if let Some(cancel) = cache_guard.cancel.lock().unwrap().as_ref() {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
async fn expand_concordance_context_command(
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
        expand_concordance_context(
            &document,
            &query,
            &corpuswright_core::tokenization::TokenizationConfig::default(),
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Concordance expansion task failed: {error}"))?
}

#[tauri::command]
async fn query_frequency_list_command(
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
async fn query_annotated_frequency_command(
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
async fn query_analysis_documents_command(
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
async fn query_overview_distribution_command(
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
async fn query_overview_distribution_evidence_command(
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
async fn query_term_distribution_command(
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
async fn query_annotated_term_distribution_command(
    query: AnnotatedTermDistributionQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<TermDistributionPage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || query_annotated_term_distribution(&corpus, &query))
        .await
        .map_err(|error| format!("Annotated term distribution query task failed: {error}"))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn extract_pdf_page_range_command(
    index: usize,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    cleaning_config: CleaningConfig,
    start_page_index: usize,
    page_count: usize,
    pdf_text_source: PdfTextSource,
    ocr_quality: PdfOcrQuality,
    max_chars_per_page: Option<usize>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
) -> Result<PdfPageRangeResult, String> {
    if page_count == 0 {
        return Err("Page count must be greater than zero.".to_string());
    }

    let records = corpus.records_for_indices(&[index], corpus_version)?;
    let record = records
        .first()
        .ok_or_else(|| "Selected file is no longer available.".to_string())?;
    if record.document_type != corpuswright_core::DocumentType::Pdf {
        return Err("Full OCR preview is available for PDF files only.".to_string());
    }

    let mut extraction_config = cleaning_config;
    extraction_config.pdf_text_source = pdf_text_source;
    extraction_config.pdf_ocr_quality = ocr_quality;

    let key = pdf_page_range_cache_key(
        record,
        start_page_index,
        page_count,
        max_chars_per_page,
        pdf_text_source,
        ocr_quality,
        &extraction_config,
    );

    if let Some(result) = page_cache.get(&key) {
        return Ok(result);
    }

    let bytes = std::fs::read(&record.source_path)
        .map_err(|error| format!("Failed to read PDF: {error}"))?;
    let options = PdfExtractionOptions::from_cleaning_config(&extraction_config);
    let mut result = extract_pdf_page_range(
        &bytes,
        start_page_index,
        page_count,
        options,
        max_chars_per_page,
    )
    .map_err(|error| format!("PDF page extraction failed: {error}"))?;

    apply_cleaning_to_page_range(&mut result, &extraction_config, max_chars_per_page);
    page_cache.insert(key, result.clone());
    Ok(result)
}

#[tauri::command(async)]
fn export_corpus_command(
    window: Window,
    indices: Option<Vec<usize>>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    output_dir: String,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<ExportReport, String> {
    let records = match indices {
        Some(indices) => corpus.records_for_indices(&indices, corpus_version)?,
        None => corpus.all_records_and_root(corpus_version)?.0,
    };
    let total = records.len();
    let options = ExportOptions {
        app_name: "CorpusWright".to_string(),
        app_version: None,
        overwrite: false,
    };

    let progress_callback = move |current: usize, file_name: &str| {
        let _ = window.emit(
            "export-progress",
            ExportProgress {
                current,
                total,
                current_file: file_name.to_string(),
            },
        );
    };

    export_corpus(
        &records,
        output_dir,
        &cleaning_config,
        &options,
        Some(&progress_callback),
        Some(&*cache),
    )
    .map_err(|e| match e {
        ExportError::UnsafeOutputDirectory { .. } => {
            "Output directory must not be the same as or contain the source root.".to_string()
        }
        ExportError::ExistingOutputDirectory { .. } => {
            "The selected output directory already exists. Please choose an empty folder or enable overwrite.".to_string()
        }
        ExportError::OutputPathIsNotDirectory { .. } => {
            "The selected output path is not a valid directory.".to_string()
        }
        ExportError::Io { message, .. } => {
            format!("File system error during save: {}", message)
        }
        ExportError::Json { message, .. } => {
            format!("Error generating manifest: {}", message)
        }
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn process_project_sources_command(
    window: Window,
    root_path: String,
    selection: Option<ProjectSourceSelectionDescriptor>,
    corpus_version: u64,
    cleaning_config: CleaningConfig,
    corpus: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    index_context: tauri::State<'_, ConcordanceIndexContextState>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<ProcessSourcesReport, String> {
    let project_root = project::validated_project_root(&root_path)?;
    let records = records_for_project_processing(
        &project_root,
        selection,
        corpus_version,
        &corpus,
        &inventory,
    )?;
    if records.is_empty() {
        return Err("No processable project sources are available.".to_string());
    }

    let output_dir = project::processed_output_dir_for_root(&project_root)?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Could not create project processed folder: {error}"))?;

    let total = records.len();
    let mut processed_count = 0;
    let mut failed_count = 0;
    let mut skipped_count = 0;
    let mut processing_results = Vec::with_capacity(records.len());

    for (index, record) in records.iter().enumerate() {
        let current_file = pdf_intake_file_name(record);
        match document_text_for_record(
            record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&*cache),
        ) {
            Ok(document) => {
                let output_path = processed_text_path(&project_root, &output_dir, record);
                match std::fs::write(&output_path, document.text.as_bytes()) {
                    Ok(()) => {
                        let warning_count = document.warnings.len();
                        processed_count += 1;
                        processing_results.push(ProjectSourceProcessingResult {
                            path: record.source_path.clone(),
                            failed: false,
                            warning_count,
                            reason_codes: if warning_count == 0 {
                                vec!["processed_ok".to_string()]
                            } else {
                                vec!["processing_warning".to_string()]
                            },
                        });
                    }
                    Err(_) => {
                        failed_count += 1;
                        processing_results.push(ProjectSourceProcessingResult {
                            path: record.source_path.clone(),
                            failed: true,
                            warning_count: 1,
                            reason_codes: vec!["output_write_failed".to_string()],
                        });
                    }
                }
            }
            Err(error) if process_failure_reason(&error) == "ocr_required" => {
                skipped_count += 1;
            }
            Err(error) => {
                failed_count += 1;
                processing_results.push(ProjectSourceProcessingResult {
                    path: record.source_path.clone(),
                    failed: true,
                    warning_count: 1,
                    reason_codes: vec![process_failure_reason(&error).to_string()],
                });
            }
        }

        let _ = window.emit(
            "process-sources-progress",
            ProcessSourcesProgress {
                current: index + 1,
                total,
                current_file,
            },
        );
    }

    if !processing_results.is_empty() {
        update_project_source_processing_results(&project_root, processing_results)?;
    }
    analysis.clear();
    index_context.clear();

    Ok(ProcessSourcesReport {
        total_count: total,
        processed_count,
        failed_count,
        skipped_count,
        output_dir: output_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command(async)]
fn compute_word_count_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<WordCountBatchResult, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let mut total_words = 0;
    let mut skipped_ocr_mode = false;
    for record in &records {
        let outcome = corpuswright_core::word_count::count_words_for_record(
            record,
            &cleaning_config,
            Some(&*cache),
        );
        total_words += outcome.count as u64;
        skipped_ocr_mode |= outcome.skipped_ocr_mode;
    }
    Ok(WordCountBatchResult {
        total_words,
        skipped_ocr_mode,
    })
}

#[tauri::command(async)]
fn compute_word_count_window_command(
    offset: usize,
    limit: usize,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<WordCountWindowResult, String> {
    let (records, total_count) = corpus.records_window(offset, limit.min(1_000), corpus_version)?;
    let mut total_words = 0;
    let mut skipped_ocr_mode = false;
    for record in &records {
        let outcome = corpuswright_core::word_count::count_words_for_record(
            record,
            &cleaning_config,
            Some(&*cache),
        );
        total_words += outcome.count as u64;
        skipped_ocr_mode |= outcome.skipped_ocr_mode;
    }
    Ok(WordCountWindowResult {
        total_words,
        skipped_ocr_mode,
        processed_count: records.len(),
        total_count,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn scan_repeated_artifacts_command(
    indices: Option<Vec<usize>>,
    source_paths: Option<Vec<String>>,
    root_path: Option<String>,
    all_matching_query: Option<String>,
    excluded_source_ids: Option<Vec<String>>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    config: RepeatedArtifactScanConfig,
    cleaning_config: CleaningConfig,
    state: tauri::State<'_, ScanState>,
    cache: tauri::State<'_, ExtractionCache>,
    inventory: tauri::State<'_, project::ProjectSourceInventoryState>,
    window: Window,
) -> Result<RepeatedArtifactScanReport, RepeatedArtifactScanError> {
    let records = if let (Some(root_path), Some(selection_query)) = (root_path, all_matching_query)
    {
        let root = project::validated_project_root(&root_path)?;
        let paths = inventory.matching_paths(
            &root,
            &selection_query,
            excluded_source_ids.as_deref().unwrap_or_default(),
        )?;
        corpus.records_for_paths(paths, corpus_version)?
    } else if let Some(paths) = source_paths {
        corpus.records_for_paths(
            paths.into_iter().map(PathBuf::from).collect(),
            corpus_version,
        )?
    } else if let Some(indices) = indices {
        corpus.records_for_indices(&indices, corpus_version)?
    } else {
        corpus.all_records_and_root(corpus_version)?.0
    };
    // A previous cancellation must not leak into the next scan.
    state.cancel.store(false, Ordering::Relaxed);
    corpuswright_core::repeated_artifacts::scan_repeated_artifacts_report_with_cancel_cache_and_progress(
        &records,
        &config,
        &cleaning_config,
        Some(&*cache),
        &state.cancel,
        |progress| {
            let _ = window.emit("repeated-artifact-scan-progress", progress);
        },
    )
}

#[tauri::command(async)]
fn cancel_repeated_artifacts_command(state: tauri::State<'_, ScanState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

/// Checks whether the given path has a `.json` extension (case-insensitive).
fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

const MAX_CONFIG_SIZE: u64 = 1_048_576; // 1 MB

/// Reads a config JSON file from an arbitrary user-chosen path.
///
/// Validates that:
/// - The path has a `.json` extension (case-insensitive).
/// - The file exists and is a regular file.
/// - The file does not exceed the maximum config size.
/// - The content is valid JSON.
///
/// # Security note
///
/// Users still choose the path and an injected webview caller could still read
/// writable `.json` paths within process permissions. This is defence-in-depth,
/// not a complete filesystem sandbox.
#[tauri::command(async)]
fn read_config_file_command(path: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    if !is_json_path(&path_buf) {
        return Err("Only .json config files are supported.".to_string());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Cannot access file: {}", e))?;
    if !metadata.is_file() {
        return Err("Path is not a regular file.".to_string());
    }
    if metadata.len() > MAX_CONFIG_SIZE {
        return Err(format!(
            "File exceeds {} MB size limit.",
            MAX_CONFIG_SIZE / 1_048_576
        ));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read file: {}", e))?;
    let _: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("File is not valid JSON: {}", e))?;
    Ok(content)
}

/// Writes a config JSON string to an arbitrary user-chosen path.
///
/// Validates that:
/// - The path has a `.json` extension (case-insensitive).
/// - The content does not exceed the maximum config size.
/// - The content is valid JSON.
///
/// # Security note
///
/// Users still choose the path and an injected webview caller could still write
/// to writable `.json` paths within process permissions. This is defence-in-depth,
/// not a complete filesystem sandbox.
#[tauri::command(async)]
fn save_config_file_command(path: String, content: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    if !is_json_path(&path_buf) {
        return Err("Only .json config files are supported.".to_string());
    }
    if content.len() as u64 > MAX_CONFIG_SIZE {
        return Err(format!(
            "Config exceeds {} MB size limit.",
            MAX_CONFIG_SIZE / 1_048_576
        ));
    }
    let _: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Content is not valid JSON: {}", e))?;
    std::fs::write(&path, &content).map_err(|e| format!("Cannot write file: {}", e))
}

#[tauri::command(async)]
fn save_svg_file_command(path: String, content: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let is_svg = path_buf
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
    if !is_svg {
        return Err("Only .svg files are supported.".to_string());
    }
    if !content.trim_start().starts_with("<svg") {
        return Err("SVG content must have an <svg> root element.".to_string());
    }
    std::fs::write(&path_buf, content).map_err(|error| format!("Cannot write SVG file: {error}"))
}

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;

    const DWMWA_CAPTION_COLOR: u32 = 35;

    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            dwAttribute: u32,
            pvAttribute: *const c_void,
            cbAttribute: u32,
        ) -> i32;
    }

    pub fn set_titlebar_color(hwnd: *mut c_void, color: u32) {
        unsafe {
            let pv_attribute = &color as *const u32 as *const c_void;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR,
                pv_attribute,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn parse_hex_color(hex: &str) -> Option<u32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        // BGR format: 0x00BBGGRR
        Some(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
    } else {
        None
    }
}

#[tauri::command]
fn set_window_titlebar_color(window: Window, color: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(parsed_color) = parse_hex_color(&color) {
            if let Ok(hwnd) = window.hwnd() {
                win32::set_titlebar_color(hwnd.0, parsed_color);
                return Ok(());
            }
            return Err("Could not get window handle".to_string());
        }
        Err(format!("Invalid hex color: {}", color))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        let _ = color;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let concordance_cache_root = app
                .path()
                .app_cache_dir()
                .ok()
                .map(|path| path.join("concordance"));
            app.manage(ConcordanceIndexContextState::new(concordance_cache_root));
            if let Ok(pdfium_path) = app
                .path()
                .resolve("ocr/pdfium.dll", BaseDirectory::Resource)
                && let Some(ocr_dir) = pdfium_path.parent()
            {
                let _ = corpuswright_core::pdf_ocr::set_ocr_resource_dir(ocr_dir.to_path_buf());
            }

            #[cfg(target_os = "windows")]
            {
                for window in app.webview_windows().values() {
                    if let Ok(hwnd) = window.hwnd() {
                        win32::set_titlebar_color(hwnd.0, 0x00a65800);
                    }
                }
            }

            Ok(())
        })
        .manage(ScanState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(PdfIntakeState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(AnnotationBuildState::new())
        .manage(AnalysisState::new())
        .manage(CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        })
        .manage(ExtractionCache::new())
        .manage(PdfPageRangeCache::new())
        .manage(ConcordanceCollocationCache::new())
        .manage(project::ProjectSourceInventoryState::new())
        .invoke_handler(tauri::generate_handler![
            scan_directory_command,
            load_files_command,
            load_project_source_records_command,
            audit_pdf_files_command,
            load_pdf_intake_files_command,
            prepare_project_pdf_sources_command,
            cancel_pdf_intake_materialization_command,
            clear_corpus_command,
            search_corpus_command,
            preview_files_command,
            preview_processed_files_command,
            read_project_source_preview_command,
            query_selected_source_preview_viewport_command,
            build_annotation_cache_command,
            cancel_annotation_cache_build_command,
            build_analysis_corpus_command,
            cancel_analysis_build_command,
            get_analysis_build_status_command,
            compute_corpus_overview_command,
            search_concordance_command,
            cancel_concordance_command,
            compare_expressions_command,
            query_concordance_collocations_command,
            expand_concordance_context_command,
            query_frequency_list_command,
            query_annotated_frequency_command,
            query_analysis_documents_command,
            query_overview_distribution_command,
            query_overview_distribution_evidence_command,
            query_term_distribution_command,
            query_annotated_term_distribution_command,
            extract_pdf_page_range_command,
            export_corpus_command,
            process_project_sources_command,
            compute_word_count_command,
            compute_word_count_window_command,
            scan_repeated_artifacts_command,
            cancel_repeated_artifacts_command,
            save_config_file_command,
            save_svg_file_command,
            read_config_file_command,
            set_window_titlebar_color,
            project::create_corpus_project_command,
            project::open_corpus_project_command,
            project::initialise_folder_as_corpus_project_command,
            project::upgrade_project_language_command,
            project::query_project_sources_command,
            project::query_project_source_selection_action_summary_command,
            project::resolve_project_source_selection_pdf_paths_command,
            project::fast_import_project_source_files_command,
            project::fast_import_project_source_folder_command,
            project::remove_project_source_selection_command,
            project::review_project_source_files_import_command,
            project::review_project_source_folder_import_command,
            project::confirm_project_source_files_import_command,
            project::confirm_project_source_folder_import_command,
            project::update_project_source_pdf_handling_command,
            project::load_repeated_artifact_decisions_command,
            project::save_repeated_artifact_decisions_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    fn collocation_cache_key(
        corpus_version: u64,
        selection: AnalysisDocumentSelection,
        query: &str,
        case_sensitive: bool,
    ) -> ConcordanceCollocationCacheKey {
        ConcordanceCollocationCacheKey {
            corpus_version,
            project_root: Some("project".to_string()),
            cleaning_config_signature: "cleaning-v1".to_string(),
            selection,
            aggregation: ConcordanceCollocationAggregationSignature {
                query: query.to_string(),
                case_sensitive,
            },
        }
    }

    fn collocation_aggregate(
        signature: ConcordanceCollocationAggregationSignature,
    ) -> ConcordanceCollocationAggregate {
        ConcordanceCollocationAggregate {
            signature,
            candidate_counts: std::collections::HashMap::new(),
            collocate_frequencies: vec![],
            symbols: vec![],
            r1: Default::default(),
            n: 0,
            f_node: 0,
        }
    }

    fn desktop_collocation_query(
        query: &str,
        case_sensitive: bool,
        span: usize,
        direction: corpuswright_core::analysis::ConcordanceCollocationDirection,
    ) -> ConcordanceCollocationQuery {
        ConcordanceCollocationQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            case_sensitive,
            span,
            direction,
            min_cooccurrence: 1,
            min_collocate_frequency: 1,
            hide_stopwords: false,
            language: None,
            sort_measure: corpuswright_core::analysis::ConcordanceCollocationMeasure::LogDice,
            sort_key: corpuswright_core::analysis::ConcordanceCollocationSortKey::Association,
            sort_direction: corpuswright_core::analysis::SortDirection::Desc,
            limit: 100,
        }
    }

    fn query_cached_collocations_for_test(
        cache: &ConcordanceCollocationCache,
        query: &ConcordanceCollocationQuery,
        aggregate: &mut impl FnMut(
            ConcordanceCollocationAggregationSignature,
        ) -> ConcordanceCollocationAggregate,
    ) -> ConcordanceCollocationResult {
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let aggregation =
            concordance_collocation_aggregation_signature(query, &tokenization_config).unwrap();
        let cache_key = ConcordanceCollocationCacheKey {
            corpus_version: 7,
            project_root: Some("project".to_string()),
            cleaning_config_signature: "cleaning-v1".to_string(),
            selection: query.selection.clone(),
            aggregation: aggregation.clone(),
        };
        if let Some(aggregate) = cache.get(&cache_key) {
            return query_concordance_collocations_from_aggregate(&aggregate, query);
        }

        let aggregate = Arc::new(aggregate(aggregation));
        cache.insert(cache_key, Arc::clone(&aggregate));
        query_concordance_collocations_from_aggregate(&aggregate, query)
    }

    fn concordance_test_documents() -> Vec<ConcordanceDocument> {
        vec![
            ConcordanceDocument {
                document_index: 0,
                source_path: "a.txt".to_string(),
                display_name: "a.txt".to_string(),
                text: "Movie, movie. Good movie! I love it. ação coração não João.".to_string(),
            },
            ConcordanceDocument {
                document_index: 1,
                source_path: "b.txt".to_string(),
                display_name: "b.txt".to_string(),
                text: "A good movie is still a movie. Eu digo ação, coração, não, João."
                    .to_string(),
            },
        ]
    }

    fn concordance_test_query(query: &str) -> ConcordanceQuery {
        ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            context_filter_direction:
                corpuswright_core::analysis::ConcordanceContextFilterDirection::Both,
            case_sensitive: false,
            sort_keys: vec![corpuswright_core::analysis::ConcordanceSortKey::FilePosition],
            window_tokens: 5,
            offset: 0,
            limit: 100,
            include_rows: true,
            distribution_offset: 0,
            distribution_limit: 10,
            include_distribution: true,
        }
    }

    fn concordance_test_context_key(corpus_version: u64) -> ConcordanceIndexContextKey {
        ConcordanceIndexContextKey {
            corpus_version,
            cleaning_config_signature: "cleaning-v1".to_string(),
        }
    }

    fn concordance_test_context() -> ConcordanceIndexContextHandle {
        Arc::new(Mutex::new(ConcordanceIndexContextCache::default()))
    }

    fn concordance_test_index_error() -> corpuswright_core::analysis::ConcordanceError {
        corpuswright_core::analysis::ConcordanceError::Index(
            corpuswright_core::index::SurfaceIndexError::DuplicateDocumentId(0),
        )
    }

    #[test]
    fn concordance_backend_mode_parser_covers_env_contract() {
        let cases = [
            (None, ConcordanceBackendMode::DefaultIndex, None),
            (Some(""), ConcordanceBackendMode::DefaultIndex, None),
            (Some("   "), ConcordanceBackendMode::DefaultIndex, None),
            (Some("scan"), ConcordanceBackendMode::Scan, None),
            (Some("ScAn"), ConcordanceBackendMode::Scan, None),
            (Some("index"), ConcordanceBackendMode::StrictIndex, None),
            (Some(" INDEX "), ConcordanceBackendMode::StrictIndex, None),
            (Some("Audit"), ConcordanceBackendMode::Audit, None),
            (
                Some("something-else"),
                ConcordanceBackendMode::DefaultIndex,
                Some("something-else"),
            ),
        ];

        for (value, expected_mode, expected_unknown) in cases {
            let parsed = parse_concordance_backend_mode(value);
            assert_eq!(parsed.mode, expected_mode);
            assert_eq!(parsed.unknown_value.as_deref(), expected_unknown);
        }
    }

    #[test]
    fn concordance_default_index_uses_index_for_supported_query() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let outcome = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.recovery_reason, None);
        assert_eq!(outcome.index_rebuilt, Some(true));
        assert_eq!(outcome.result, scan);
    }

    #[test]
    fn concordance_default_index_success_does_not_invoke_scan_route() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let expected = search_concordance_documents(&documents, &query, &tokenization_config)
            .expect("fixture scan should succeed");
        let scan_calls = std::cell::Cell::new(0usize);
        let index_calls = std::cell::Cell::new(0usize);

        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            |_, _, _| {
                scan_calls.set(scan_calls.get() + 1);
                panic!("default index success must not invoke scan")
            },
            |_, _, _, _, _| {
                index_calls.set(index_calls.get() + 1);
                Ok(RoutedConcordanceResult {
                    result: expected.clone(),
                    backend_used: ConcordanceBackendUsed::Index,
                    index_rebuilt: false,
                    execution_stats:
                        corpuswright_core::analysis::ConcordanceExecutionStats::default(),
                })
            },
        )
        .unwrap();

        assert_eq!(index_calls.get(), 1);
        assert_eq!(scan_calls.get(), 0);
        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.result, expected);
    }

    #[test]
    fn concordance_default_index_handles_context_filter() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.context_filter = Some("good".to_string());
        query.context_filter_direction =
            corpuswright_core::analysis::ConcordanceContextFilterDirection::Right;
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let outcome = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(outcome.recovery_reason, None);
        assert_eq!(outcome.index_rebuilt, Some(true));
        assert_eq!(outcome.result, scan);
    }

    #[test]
    fn concordance_default_index_recovers_from_index_error_and_retries_clean_rebuild() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let primed = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key.clone(),
        )
        .unwrap();
        assert!(primed.index_rebuilt);
        assert!(context.lock().unwrap().entry.is_some());

        let index_error = concordance_test_index_error();
        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key.clone(),
            search_concordance_documents,
            |_, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap();

        assert_eq!(outcome.backend_used, ConcordanceBackendUsed::Scan);
        assert_eq!(
            outcome.recovery_reason,
            Some(ConcordanceIndexRecoveryReason::IndexError)
        );
        assert_eq!(outcome.index_rebuilt, None);
        assert_eq!(outcome.result, scan);
        {
            let cache = context.lock().unwrap();
            assert!(cache.entry.is_none());
            assert_eq!(
                cache
                    .default_index_failure
                    .as_ref()
                    .map(|failure| &failure.state),
                Some(&ConcordanceDefaultIndexFailureState::RetryPending)
            );
        }

        let retry = search_concordance_documents_default_index_mode(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key,
        )
        .unwrap();

        assert_eq!(retry.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(retry.index_rebuilt, Some(true));
        assert_eq!(retry.recovery_reason, None);
        assert!(context.lock().unwrap().default_index_failure.is_none());
    }

    #[test]
    fn concordance_default_index_disables_after_repeated_index_error() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        for expected_state in [
            ConcordanceDefaultIndexFailureState::RetryPending,
            ConcordanceDefaultIndexFailureState::Disabled,
        ] {
            let index_error = concordance_test_index_error();
            let outcome = search_concordance_documents_default_index_mode_with(
                &documents,
                &query,
                &tokenization_config,
                &context,
                key.clone(),
                search_concordance_documents,
                |_, _, _, _, _| Err(index_error.clone()),
            )
            .unwrap();
            assert_eq!(outcome.result, scan);
            assert_eq!(
                outcome.recovery_reason,
                Some(ConcordanceIndexRecoveryReason::IndexError)
            );
            assert_eq!(
                context
                    .lock()
                    .unwrap()
                    .default_index_failure
                    .as_ref()
                    .map(|failure| &failure.state),
                Some(&expected_state)
            );
        }

        let index_calls = std::cell::Cell::new(0usize);
        let outcome = search_concordance_documents_default_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            key,
            search_concordance_documents,
            |_, _, _, _, _| {
                index_calls.set(index_calls.get() + 1);
                Err(concordance_test_index_error())
            },
        )
        .unwrap();

        assert_eq!(index_calls.get(), 0);
        assert_eq!(outcome.result, scan);
        assert_eq!(
            outcome.recovery_reason,
            Some(ConcordanceIndexRecoveryReason::PreviousIndexError)
        );
    }

    #[test]
    fn concordance_strict_index_surfaces_injected_index_error() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let index_error = concordance_test_index_error();

        let error = search_concordance_documents_strict_index_mode_with(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            |_, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap_err();

        assert_eq!(error, index_error.to_string());
    }

    #[test]
    fn concordance_audit_difference_reports_first_row_identity_mismatch() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let mut index = scan.clone();
        index.rows.as_mut().expect("rows requested")[0].token_index += 1;

        let difference = compare_concordance_results(&scan, &index).unwrap();

        assert_eq!(difference.field, "row_identity");
        assert_eq!(difference.row_index, Some(0));
        assert_ne!(difference.scan, difference.index);
    }

    #[test]
    fn concordance_audit_compares_omitted_result_sections() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.include_rows = false;
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        let mut index = scan.clone();

        assert_eq!(compare_concordance_results(&scan, &index), None);
        index.rows = Some(Vec::new());
        assert_eq!(
            compare_concordance_results(&scan, &index)
                .expect("section mismatch")
                .field,
            "rows_included"
        );

        query.include_rows = true;
        query.include_distribution = false;
        let rows_only =
            search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
        assert_eq!(compare_concordance_results(&rows_only, &rows_only), None);
    }

    #[test]
    fn concordance_collocation_default_index_reuses_concordance_warm_context() {
        let documents = concordance_test_documents();
        let concordance_query = concordance_test_query("movie");
        let collocation_query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let key = concordance_test_context_key(7);
        let scan = aggregate_concordance_collocations_documents(
            &documents,
            &collocation_query,
            &tokenization_config,
            corpuswright_core::index::SurfaceIndexCancellationToken::default(),
        )
        .unwrap();

        let concordance = search_concordance_with_warm_index_provider(
            documents.len(),
            &concordance_query,
            &tokenization_config,
            &context,
            key.clone(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
        )
        .unwrap();
        let collocation = aggregate_concordance_collocations_default_index_mode(
            documents.len(),
            &collocation_query,
            &tokenization_config,
            &context,
            key,
            &corpuswright_core::index::SurfaceIndexCancellationToken::default(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
        )
        .unwrap();

        assert!(concordance.index_rebuilt);
        assert_eq!(collocation.backend_used, ConcordanceBackendUsed::Index);
        assert_eq!(collocation.index_rebuilt, Some(false));
        assert_eq!(collocation.recovery_reason, None);
        assert_eq!(
            compare_concordance_collocation_aggregates(&scan, &collocation.aggregate),
            None
        );
        assert_eq!(
            query_concordance_collocations_from_aggregate(
                &collocation.aggregate,
                &collocation_query
            ),
            query_concordance_collocations_from_aggregate(&scan, &collocation_query)
        );
    }

    #[test]
    fn concordance_collocation_memory_refusal_never_invokes_scan_fallback() {
        let documents = concordance_test_documents();
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan_calls = std::cell::Cell::new(0usize);
        let index_error =
            corpuswright_core::analysis::ConcordanceError::AnalysisMemoryLimitExceeded {
                estimated_resident_index_bytes: 900,
                projected_aggregate_bytes: 200,
                combined_limit_bytes: 1_000,
                safety_margin_bytes: 100,
            };

        let error = aggregate_concordance_collocations_default_index_mode_with(
            documents.len(),
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
            &corpuswright_core::index::SurfaceIndexCancellationToken::default(),
            |document_index| Some(documents[document_index].clone()),
            |_| {},
            |_, _, _, _, _, _| {
                scan_calls.set(scan_calls.get() + 1);
                Err(concordance_test_index_error())
            },
            |_, _, _, _, _, _, _, _| Err(index_error.clone()),
        )
        .unwrap_err();

        assert_eq!(scan_calls.get(), 0);
        assert!(error.contains("Narrow the file scope"));
        assert!(context.lock().unwrap().default_index_failure.is_none());
    }

    #[test]
    fn concordance_collocation_audit_difference_reports_full_aggregate_mismatch() {
        let documents = concordance_test_documents();
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let scan = aggregate_concordance_collocations_documents(
            &documents,
            &query,
            &tokenization_config,
            corpuswright_core::index::SurfaceIndexCancellationToken::default(),
        )
        .unwrap();
        let mut index = scan.clone();
        index.f_node += 1;

        let difference = compare_concordance_collocation_aggregates(&scan, &index).unwrap();

        assert_eq!(difference.field, "collocation_f_node");
        assert_ne!(difference.scan, difference.index);
    }

    #[test]
    fn concordance_warm_index_context_reuses_store_for_fixture_sequence() {
        let documents = concordance_test_documents();
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let queries = [
            "movie",
            "good movie",
            "I love it",
            "ação",
            "coração",
            "não",
            "João",
        ];
        let mut rebuild_count = 0;

        for pass in 0..2 {
            for query_text in queries.iter().copied() {
                let query = concordance_test_query(query_text);
                let scan =
                    search_concordance_documents(&documents, &query, &tokenization_config).unwrap();
                let started_at = Instant::now();
                let routed = search_concordance_documents_with_warm_index(
                    &documents,
                    &query,
                    &tokenization_config,
                    &context,
                    concordance_test_context_key(7),
                )
                .unwrap();
                eprintln!(
                    "[concordance-backend-test] pass={} query={} index_ms={} rebuilt={}",
                    pass,
                    log_string(query_text, 80),
                    duration_ms(started_at.elapsed()),
                    routed.index_rebuilt
                );

                assert_eq!(routed.backend_used, ConcordanceBackendUsed::Index);
                assert_eq!(routed.result, scan);
                if routed.index_rebuilt {
                    rebuild_count += 1;
                }
                if pass == 1 {
                    assert!(!routed.index_rebuilt);
                }
            }
        }

        assert_eq!(rebuild_count, 1);
    }

    #[test]
    fn concordance_warm_index_context_rebuilds_when_corpus_key_changes() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("movie");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();

        let first = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();
        let second = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();
        let third = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(8),
        )
        .unwrap();

        assert!(first.index_rebuilt);
        assert!(!second.index_rebuilt);
        assert!(third.index_rebuilt);
    }

    #[test]
    fn concordance_index_context_filter_uses_index() {
        let documents = concordance_test_documents();
        let mut query = concordance_test_query("movie");
        query.context_filter = Some("good".to_string());
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();
        let scan = search_concordance_documents(&documents, &query, &tokenization_config).unwrap();

        let routed = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap();

        assert_eq!(routed.backend_used, ConcordanceBackendUsed::Index);
        assert!(routed.index_rebuilt);
        assert_eq!(routed.result, scan);
    }

    #[test]
    fn concordance_index_errors_surface_from_warm_route() {
        let documents = concordance_test_documents();
        let query = concordance_test_query("   ");
        let tokenization_config = corpuswright_core::tokenization::TokenizationConfig::default();
        let context = concordance_test_context();

        let error = search_concordance_documents_with_warm_index(
            &documents,
            &query,
            &tokenization_config,
            &context,
            concordance_test_context_key(7),
        )
        .unwrap_err();

        assert_eq!(
            error,
            corpuswright_core::analysis::ConcordanceError::EmptyQuery
        );
    }

    #[test]
    fn concordance_collocation_cache_is_keyed_by_aggregation_inputs() {
        let cache = ConcordanceCollocationCache::new();
        let key = collocation_cache_key(7, AnalysisDocumentSelection::default(), "movie", false);
        cache.insert(
            key.clone(),
            Arc::new(collocation_aggregate(key.aggregation.clone())),
        );

        assert!(cache.get(&key).is_some());
        let mut other_project = key.clone();
        other_project.project_root = Some("other-project".to_string());
        assert!(cache.get(&other_project).is_none());
        let mut other_cleaning = key.clone();
        other_cleaning.cleaning_config_signature = "cleaning-v2".to_string();
        assert!(cache.get(&other_cleaning).is_none());
        assert!(
            cache
                .get(&collocation_cache_key(
                    8,
                    AnalysisDocumentSelection::default(),
                    "movie",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection {
                        all_documents: false,
                        ids: vec![1],
                        excluded_ids: Vec::new(),
                    },
                    "movie",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection::default(),
                    "film",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection::default(),
                    "movie",
                    true,
                ))
                .is_none()
        );
    }

    #[test]
    fn concordance_collocation_cache_shares_and_releases_aggregate_allocations() {
        let cache = ConcordanceCollocationCache::new();
        let key = collocation_cache_key(7, AnalysisDocumentSelection::default(), "movie", false);
        let mut owned = collocation_aggregate(key.aggregation.clone());
        owned.symbols.push("candidate".to_string());
        owned.collocate_frequencies.push(11);
        owned.candidate_counts.insert(0, Default::default());
        let aggregate = Arc::new(owned);
        let symbol_ptr = aggregate.symbols[0].as_ptr();
        cache.insert(key.clone(), Arc::clone(&aggregate));

        let first = cache.get(&key).unwrap();
        let second = cache.get(&key).unwrap();
        assert!(Arc::ptr_eq(&aggregate, &first));
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.symbols[0].as_ptr(), symbol_ptr);
        let strong_count = Arc::strong_count(&aggregate);

        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let _ = query_concordance_collocations_from_aggregate(&first, &query);
        assert_eq!(Arc::strong_count(&aggregate), strong_count);
        assert_eq!(first.symbols[0].as_ptr(), symbol_ptr);

        drop(first);
        drop(second);
        let weak = Arc::downgrade(&aggregate);
        drop(aggregate);
        let replacement = Arc::new(collocation_aggregate(key.aggregation.clone()));
        cache.insert(key.clone(), replacement);
        assert!(weak.upgrade().is_none());

        let replacement = cache.get(&key).unwrap();
        let replacement_weak = Arc::downgrade(&replacement);
        drop(replacement);
        cache.clear();
        assert!(cache.get(&key).is_none());
        assert!(replacement_weak.upgrade().is_none());
    }

    #[test]
    fn concordance_collocation_cache_hit_reuses_across_span_and_direction() {
        let cache = ConcordanceCollocationCache::new();
        let aggregation_runs = std::cell::Cell::new(0usize);
        let mut aggregate = |signature| {
            aggregation_runs.set(aggregation_runs.get() + 1);
            collocation_aggregate(signature)
        };
        let query = desktop_collocation_query(
            "movie",
            false,
            1,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );

        let first = query_cached_collocations_for_test(&cache, &query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(first.span, 1);
        assert_eq!(
            first.direction,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both
        );

        let mut span_query = query.clone();
        span_query.span = 20;
        let span_change = query_cached_collocations_for_test(&cache, &span_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(span_change.span, 20);

        let mut direction_query = query.clone();
        direction_query.direction =
            corpuswright_core::analysis::ConcordanceCollocationDirection::Left;
        let direction_change =
            query_cached_collocations_for_test(&cache, &direction_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(
            direction_change.direction,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Left
        );

        let mut frequency_query = query.clone();
        frequency_query.min_collocate_frequency = 10;
        frequency_query.min_cooccurrence = 4;
        let frequency_change =
            query_cached_collocations_for_test(&cache, &frequency_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(frequency_change.min_collocate_frequency, 10);
        assert_eq!(frequency_change.min_cooccurrence, 4);
    }

    #[test]
    fn concordance_collocation_cache_misses_for_query_or_case_change() {
        let cache = ConcordanceCollocationCache::new();
        let aggregation_runs = std::cell::Cell::new(0usize);
        let mut aggregate = |signature| {
            aggregation_runs.set(aggregation_runs.get() + 1);
            collocation_aggregate(signature)
        };
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        query_cached_collocations_for_test(&cache, &query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);

        let mut different_query = query.clone();
        different_query.query = "film".to_string();
        query_cached_collocations_for_test(&cache, &different_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 2);

        let mut different_case = query.clone();
        different_case.case_sensitive = true;
        query_cached_collocations_for_test(&cache, &different_case, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 3);
    }

    #[test]
    fn explicit_single_source_preview_is_exhaustive() {
        assert_eq!(
            preview_char_limit_for_explicit_selection(1, Some(5_000)),
            None
        );
    }

    #[test]
    fn non_single_explicit_source_previews_are_bounded() {
        assert_eq!(
            preview_char_limit_for_explicit_selection(0, None),
            Some(5_000)
        );
        assert_eq!(
            preview_char_limit_for_explicit_selection(2, Some(7_500)),
            Some(7_500)
        );
    }

    #[test]
    fn project_preview_is_exhaustive_only_for_one_explicit_id() {
        assert_eq!(
            preview_char_limit_for_project_selection(
                &ProjectSourceSelectionDescriptor::default(),
                None,
            ),
            Some(2_000)
        );

        let explicit_single = ProjectSourceSelectionDescriptor {
            ids: vec!["source-1".to_string()],
            ..ProjectSourceSelectionDescriptor::default()
        };
        assert_eq!(
            preview_char_limit_for_project_selection(&explicit_single, Some(2_000)),
            None
        );

        let explicit_multiple = ProjectSourceSelectionDescriptor {
            ids: vec!["source-1".to_string(), "source-2".to_string()],
            ..ProjectSourceSelectionDescriptor::default()
        };
        assert_eq!(
            preview_char_limit_for_project_selection(&explicit_multiple, None),
            Some(2_000)
        );

        let all_matching = ProjectSourceSelectionDescriptor {
            all_matching: true,
            ids: vec!["source-1".to_string()],
            ..ProjectSourceSelectionDescriptor::default()
        };
        assert_eq!(
            preview_char_limit_for_project_selection(&all_matching, None),
            Some(2_000)
        );
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

    fn project_source_for_preview(
        kind: ProjectSourceKind,
        status: ProjectSourceStatus,
    ) -> ProjectSource {
        ProjectSource {
            id: "source_test".to_string(),
            path: PathBuf::from("source.txt"),
            file_name: "source.txt".to_string(),
            extension: Some("txt".to_string()),
            kind,
            status,
            reason_codes: Vec::new(),
            extraction_route: ProjectSourceExtractionRoute::PlainText,
            pdf_profile: None,
            ocr_quality: None,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            last_checked_at: None,
            last_processed_at: None,
        }
    }

    #[test]
    fn native_source_preview_does_not_require_materialised_processing() {
        for status in [
            ProjectSourceStatus::Unprocessed,
            ProjectSourceStatus::ReadyToProcess,
            ProjectSourceStatus::TextExtracted,
            ProjectSourceStatus::Processed,
        ] {
            let source = project_source_for_preview(ProjectSourceKind::Txt, status);
            assert!(project_source_has_previewable_text(&source));
        }
    }

    #[test]
    fn pdf_preview_still_requires_extracted_source_text() {
        let pending =
            project_source_for_preview(ProjectSourceKind::Pdf, ProjectSourceStatus::ReadyToProcess);
        let extracted =
            project_source_for_preview(ProjectSourceKind::Pdf, ProjectSourceStatus::TextExtracted);

        assert!(!project_source_has_previewable_text(&pending));
        assert!(project_source_has_previewable_text(&extracted));
    }

    #[test]
    fn corpus_state_resolves_records_by_path_without_frontend_indices() {
        let first = pdf_record("first.pdf", Some(ProjectSourcePdfProfile::Standard), None);
        let second = pdf_record("second.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None);
        let corpus = CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        };
        corpus
            .inner
            .write()
            .unwrap()
            .load(None, vec![first.clone(), second.clone()]);
        let corpus_version = corpus.inner.read().unwrap().version;

        assert_eq!(
            corpus
                .record_for_path(Path::new("second.pdf"), corpus_version)
                .unwrap(),
            Some(second.clone())
        );
        assert_eq!(
            corpus
                .records_for_paths(
                    vec![PathBuf::from("second.pdf"), PathBuf::from("first.pdf")],
                    corpus_version,
                )
                .unwrap(),
            vec![second, first]
        );
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

    fn project_pdf_record(project_root: &Path, name: &str) -> DocumentRecord {
        let path = project_root.join(name);
        fs::write(&path, b"%PDF-1.4 synthetic").unwrap();
        DocumentRecord {
            source_path: path,
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Pdf,
            size_bytes: 18,
            extraction_route: Some(ProjectSourceExtractionRoute::EmbeddedText),
            pdf_profile: Some(ProjectSourcePdfProfile::Standard),
            ocr_quality: Some(PdfOcrQuality::Balanced),
        }
    }

    fn profile_plan(
        records: &[DocumentRecord],
        selected: &[&str],
        profile_override: Option<ProjectSourcePdfProfile>,
    ) -> Vec<ProjectSourcePdfProfile> {
        let selected_paths = selected.iter().map(PathBuf::from).collect::<Vec<_>>();
        project_pdf_preparation_plan(records, &selected_paths, profile_override)
            .unwrap()
            .into_iter()
            .map(|item| item.profile)
            .collect()
    }

    #[test]
    fn mixed_source_profiles_produce_mixed_preparation_items() {
        let records = vec![
            pdf_record("a.pdf", Some(ProjectSourcePdfProfile::Standard), None),
            pdf_record("b.pdf", Some(ProjectSourcePdfProfile::OcrRescue), None),
            pdf_record("c.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None),
        ];

        assert_eq!(
            profile_plan(&records, &["a.pdf", "b.pdf", "c.pdf"], None),
            vec![
                ProjectSourcePdfProfile::Standard,
                ProjectSourcePdfProfile::ForceOcr,
                ProjectSourcePdfProfile::ForceOcr,
            ]
        );
    }

    #[test]
    fn force_ocr_source_does_not_override_standard_sources() {
        let records = vec![
            pdf_record(
                "standard.pdf",
                Some(ProjectSourcePdfProfile::Standard),
                None,
            ),
            pdf_record("force.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None),
        ];

        assert_eq!(
            profile_plan(&records, &["standard.pdf", "force.pdf"], None),
            vec![
                ProjectSourcePdfProfile::Standard,
                ProjectSourcePdfProfile::ForceOcr,
            ]
        );
    }

    #[test]
    fn legacy_ocr_rescue_source_maps_to_full_ocr_without_overriding_standard_sources() {
        let records = vec![
            pdf_record(
                "standard.pdf",
                Some(ProjectSourcePdfProfile::Standard),
                None,
            ),
            pdf_record("ocr.pdf", Some(ProjectSourcePdfProfile::OcrRescue), None),
        ];

        assert_eq!(
            profile_plan(&records, &["standard.pdf", "ocr.pdf"], None),
            vec![
                ProjectSourcePdfProfile::Standard,
                ProjectSourcePdfProfile::ForceOcr,
            ]
        );
    }

    #[test]
    fn selected_source_count_is_respected() {
        let records = vec![
            pdf_record("a.pdf", Some(ProjectSourcePdfProfile::Standard), None),
            pdf_record("b.pdf", Some(ProjectSourcePdfProfile::OcrRescue), None),
            pdf_record("c.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None),
        ];

        let selected_paths = vec![PathBuf::from("a.pdf"), PathBuf::from("c.pdf")];
        let plan = project_pdf_preparation_plan(&records, &selected_paths, None).unwrap();

        assert_eq!(plan.len(), 2);
        assert_eq!(
            plan.iter()
                .map(|item| item.path.clone())
                .collect::<Vec<_>>(),
            vec![PathBuf::from("a.pdf"), PathBuf::from("c.pdf")]
        );
    }

    #[test]
    fn batch_override_applies_only_when_explicit() {
        let records = vec![
            pdf_record("a.pdf", Some(ProjectSourcePdfProfile::Standard), None),
            pdf_record("b.pdf", Some(ProjectSourcePdfProfile::OcrRescue), None),
            pdf_record("c.pdf", Some(ProjectSourcePdfProfile::Standard), None),
        ];

        assert_eq!(
            profile_plan(
                &records,
                &["a.pdf", "b.pdf"],
                Some(ProjectSourcePdfProfile::ForceOcr)
            ),
            vec![
                ProjectSourcePdfProfile::ForceOcr,
                ProjectSourcePdfProfile::ForceOcr,
            ]
        );
        assert_eq!(
            profile_plan(&records, &["c.pdf"], None),
            vec![ProjectSourcePdfProfile::Standard]
        );
    }

    #[test]
    fn mixed_batch_summary_does_not_report_one_global_profile() {
        let records = vec![
            pdf_record("a.pdf", Some(ProjectSourcePdfProfile::Standard), None),
            pdf_record("b.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None),
        ];
        let selected_paths = vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")];
        let plan = project_pdf_preparation_plan(&records, &selected_paths, None).unwrap();

        assert_eq!(
            project_pdf_preparation_batch_label(&plan),
            "mixed stored handling"
        );
    }

    #[test]
    fn mixed_preparation_uses_each_sources_extraction_mode() {
        let records = vec![
            pdf_record("a.pdf", Some(ProjectSourcePdfProfile::Standard), None),
            pdf_record("b.pdf", Some(ProjectSourcePdfProfile::OcrRescue), None),
            pdf_record("c.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None),
        ];
        let selected_paths = vec![
            PathBuf::from("a.pdf"),
            PathBuf::from("b.pdf"),
            PathBuf::from("c.pdf"),
        ];
        let plan = project_pdf_preparation_plan(&records, &selected_paths, None).unwrap();
        let config = CleaningConfig {
            pdf_text_source: PdfTextSource::ForceOcr,
            ..CleaningConfig::default()
        };
        let text_sources = records
            .iter()
            .zip(plan.iter())
            .map(|(record, item)| {
                let prepared = record_with_preparation_profile(record, item);
                PdfExtractionOptions::raw_from_record_and_cleaning_config(&prepared, &config)
                    .text_source
            })
            .collect::<Vec<_>>();

        assert_eq!(
            text_sources,
            vec![
                PdfTextSource::EmbeddedText,
                PdfTextSource::ForceOcr,
                PdfTextSource::ForceOcr,
            ]
        );
    }

    #[test]
    fn preparation_profile_blocks_global_config_fallback() {
        let records = vec![pdf_record(
            "undecided.pdf",
            None,
            Some(ProjectSourceExtractionRoute::Undecided),
        )];
        let selected_paths = vec![PathBuf::from("undecided.pdf")];
        let plan = project_pdf_preparation_plan(&records, &selected_paths, None).unwrap();
        let prepared = record_with_preparation_profile(&records[0], &plan[0]);
        let config = CleaningConfig {
            pdf_text_source: PdfTextSource::ForceOcr,
            ..CleaningConfig::default()
        };
        let options = PdfExtractionOptions::raw_from_record_and_cleaning_config(&prepared, &config);

        assert_eq!(
            prepared.pdf_profile,
            Some(ProjectSourcePdfProfile::Standard)
        );
        assert_eq!(
            prepared.extraction_route,
            Some(ProjectSourceExtractionRoute::EmbeddedText)
        );
        assert_eq!(options.text_source, PdfTextSource::EmbeddedText);
    }

    #[test]
    fn project_pdf_extraction_failure_reason_distinguishes_cancellation() {
        let cancel = AtomicBool::new(false);

        assert_eq!(
            project_pdf_extraction_failure_reason("PDF materialisation failed", &cancel),
            "text_extraction_failed"
        );
        assert_eq!(
            project_pdf_extraction_failure_reason("PDF intake cancelled", &cancel),
            "extraction_cancelled"
        );

        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            project_pdf_extraction_failure_reason("OCR batch stopped", &cancel),
            "extraction_cancelled"
        );
    }

    #[test]
    fn extraction_warning_messages_map_to_stable_reason_codes() {
        let warnings = vec![
            "OCR render size was clamped on page 2.".to_string(),
            "Layout-aware extraction adjusted line order.".to_string(),
            "OCR text extracted for review.".to_string(),
            "Repeated extraction artefacts were reported.".to_string(),
        ];

        assert_eq!(
            extraction_reason_codes_for_warnings(&warnings),
            vec![
                "render_size_clamped",
                "layout_warning",
                "ocr_generated",
                "many_repeated_artifacts",
            ]
        );
    }

    #[test]
    fn project_source_text_cache_hydrates_extracted_pdf_text() {
        let root = unique_temp_dir("source-text-cache");
        fs::create_dir_all(&root).unwrap();
        let record = project_pdf_record(&root, "paper.pdf");
        let options = PdfExtractionOptions::raw_from_record_and_cleaning_config(
            &record,
            &CleaningConfig::default(),
        );
        let entry = CacheEntry {
            extracted_text: "Synthetic extracted text".to_string(),
            warnings: Vec::new(),
            page_count: Some(1),
            page_texts: Some(vec!["Synthetic extracted text".to_string()]),
        };

        persist_project_source_text_cache(&root, &record, options, &entry).unwrap();

        let cache = ExtractionCache::new();
        hydrate_project_source_text_cache(&root, std::slice::from_ref(&record), &cache);
        let hydrated = cache
            .try_get(&record, Some(options), &CleaningConfig::default())
            .unwrap();

        assert_eq!(hydrated.extracted_text, "Synthetic extracted text");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_and_analyse_share_processed_input_and_token_stream() {
        let root = unique_temp_dir("analysis-processed-text");
        fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source.txt");
        fs::write(&source_path, "keep REMOVE").unwrap();
        let record = DocumentRecord {
            source_path: source_path.clone(),
            relative_path: PathBuf::from("source.txt"),
            document_type: DocumentType::Text,
            size_bytes: fs::metadata(&source_path).unwrap().len(),
            extraction_route: Some(ProjectSourceExtractionRoute::PlainText),
            pdf_profile: None,
            ocr_quality: None,
        };
        let cleaning_config = CleaningConfig {
            remove_patterns: vec!["REMOVE".to_string()],
            ..CleaningConfig::default()
        };

        let cache = ExtractionCache::new();
        let prepared = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap()
        .text;
        let resolution =
            resolve_analysis_input_for_record(&record, Some(&root), &cleaning_config, &cache);

        assert_eq!(
            resolution.provenance,
            corpuswright_core::analysis::AnalysisInputProvenance::Processed
        );
        assert_eq!(prepared, "keep ");
        assert_eq!(resolution.text.as_deref(), Some(prepared.as_str()));
        let config = corpuswright_core::tokenization::TokenizationConfig::default();
        let prepare_tokens = corpuswright_core::tokenization::tokenize(&prepared, 0, &config);
        let analysis_text = resolution.text.as_deref().unwrap();
        let analysis_tokens = corpuswright_core::tokenization::tokenize(analysis_text, 0, &config);
        assert_eq!(prepare_tokens, analysis_tokens);
        let _ = fs::remove_dir_all(root);
    }
}
