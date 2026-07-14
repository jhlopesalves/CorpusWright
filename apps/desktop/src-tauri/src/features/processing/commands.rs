use crate::features::analysis::state::AnalysisState;
use crate::features::concordance::state::ConcordanceIndexContextState;
use crate::features::corpus::scanning_state::ScanState;
use crate::features::corpus::state::CorpusState;
use crate::features::pdf::service::pdf_intake_file_name;
use crate::features::project;
use corpuswright_core::export::{ExportError, ExportOptions, ExportReport, export_corpus};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::preparation::repeated_artifacts::{
    RepeatedArtifactScanConfig, RepeatedArtifactScanError, RepeatedArtifactScanReport,
    scan_repeated_artifacts_report_with_cancel_cache_and_progress,
};
use corpuswright_core::project::sources::{
    ProjectSourceProcessingResult, ProjectSourceSelectionDescriptor,
    update_project_source_processing_results,
};
use corpuswright_core::sources::cache::{
    DocumentTextMode, ExtractionCache, document_text_for_record,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Window};

use super::project::{process_failure_reason, processed_text_path, records_for_project_processing};

#[derive(Clone, Serialize)]
pub(crate) struct ExportProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessSourcesProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessSourcesReport {
    total_count: usize,
    processed_count: usize,
    failed_count: usize,
    skipped_count: usize,
    output_dir: String,
}

#[tauri::command(async)]
pub(crate) fn export_corpus_command(
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
pub(crate) fn process_project_sources_command(
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

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn scan_repeated_artifacts_command(
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
    scan_repeated_artifacts_report_with_cancel_cache_and_progress(
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
pub(crate) fn cancel_repeated_artifacts_command(
    state: tauri::State<'_, ScanState>,
) -> Result<(), String> {
    state.cancel.store(true, Ordering::Relaxed);
    Ok(())
}
