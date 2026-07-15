use crate::features::concordance::state::{
    ConcordanceCollocationCache, ConcordanceIndexContextState,
};
use crate::features::corpus::state::{CorpusLoadResult, CorpusState};
use crate::features::project;
use crate::features::sources::persist_project_source_text_cache;
use corpuswright_core::corpus::{DocumentType, load_files};
use corpuswright_core::preparation::cleaning::{CleaningConfig, PdfOcrQuality, PdfTextSource};
use corpuswright_core::project::sources::{
    ProjectSourceExtractionResult, ProjectSourceKind, ProjectSourcePdfProfile,
    update_project_source_extraction_results,
};
use corpuswright_core::sources::cache::ExtractionCache;
use corpuswright_core::sources::pdf::audit::{PdfAuditResult, audit_pdf_files};
use corpuswright_core::sources::pdf::{
    PdfExtractionOptions, PdfPageRangeResult, extract_pdf_page_range,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::Window;

use super::service::{
    apply_cleaning_to_page_range, canonical_or_original, extraction_reason_codes_for_warnings,
    materialize_pdf_record, pdf_intake_cancelled_message, pdf_page_range_cache_key,
    project_pdf_extraction_failure_reason, project_pdf_preparation_batch_label,
    project_pdf_preparation_plan, record_with_preparation_profile, selected_record_by_path,
};
use super::state::{PdfIntakeState, PdfPageRangeCache};

#[tauri::command(async)]
pub(crate) fn audit_pdf_files_command(paths: Vec<String>) -> Result<Vec<PdfAuditResult>, String> {
    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    Ok(audit_pdf_files(path_bufs))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn load_pdf_intake_files_command(
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

    intake_state.cancel.run(|cancellation| {
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
            if cancellation.is_cancelled() {
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
                cancellation,
            ) {
                cache.clear();
                page_cache.clear();
                return Err(error);
            }
        }

        let version = {
            let mut inner = corpus_state.inner.write().unwrap();
            inner.load(None, report.files.clone());
            inner.version.into_ipc()
        };
        Ok(CorpusLoadResult {
            report,
            corpus_version: version,
        })
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn prepare_project_pdf_sources_command(
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

    intake_state.cancel.run(|cancellation| {
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

        let mut report =
            load_files(selected_paths.clone()).map_err(|error| format!("{error:?}"))?;
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
            if cancellation.is_cancelled() {
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
                cancellation,
            ) {
                Ok(entry) => entry,
                Err(error) => {
                    let reason_code = project_pdf_extraction_failure_reason(&error, cancellation);
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
            persist_project_source_text_cache(
                &project_root,
                &prepared_record,
                pdf_options,
                &entry,
            )?;
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
            inner.version.into_ipc()
        };
        Ok(CorpusLoadResult {
            report,
            corpus_version: version,
        })
    })
}

#[tauri::command(async)]
pub(crate) fn cancel_pdf_intake_materialization_command(
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<(), String> {
    intake_state.cancel.cancel();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn extract_pdf_page_range_command(
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
