use crate::features::sources::modified_time_secs;
use corpuswright_core::corpus::{DocumentRecord, DocumentType};
use corpuswright_core::preparation::cleaning::{
    CleaningConfig, PdfEmbeddedTextStrategy, PdfOcrQuality, PdfTextSource, clean_text,
};
use corpuswright_core::project::sources::{ProjectSourceExtractionRoute, ProjectSourcePdfProfile};
use corpuswright_core::sources::cache::{CacheEntry, ExtractionCache};
use corpuswright_core::sources::pdf::{
    PdfExtractionOptions, PdfPageRangeResult, extract_pdf_page_range, pdf_page_count,
};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{Emitter, Window};

use super::state::PdfPageRangeCacheKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectPdfPreparationPlanItem {
    pub(crate) path: PathBuf,
    pub(crate) profile: ProjectSourcePdfProfile,
    pub(crate) profile_label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PdfIntakeMaterializationProgress {
    current_file: String,
    current_file_index: usize,
    total_files: usize,
    current_page: usize,
    total_pages: Option<usize>,
    profile: String,
    method: String,
    status: String,
    elapsed_ms: u64,
    estimated_remaining_ms: Option<u64>,
    pages_per_minute: Option<f64>,
    latest_warning: Option<String>,
}

pub(crate) struct PdfIntakePageTiming {
    page_number: usize,
    page_total_ms: u64,
    postprocess_ms: u64,
    chars_extracted: usize,
    warnings: Vec<String>,
    render_clamped: bool,
}

pub(crate) struct MaterializedOcrPage {
    page_index: usize,
    text: Option<String>,
    timing: PdfIntakePageTiming,
}

pub(crate) fn pdf_intake_method_label(options: PdfExtractionOptions) -> String {
    match options.text_source {
        PdfTextSource::EmbeddedText => match options.strategy {
            PdfEmbeddedTextStrategy::PdfiumFlat => "embedded text".to_string(),
            PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn => {
                "layout-aware embedded text".to_string()
            }
            PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => {
                "experimental visual columns".to_string()
            }
        },
        PdfTextSource::Ocr => "legacy mixed OCR route".to_string(),
        PdfTextSource::ForceOcr => "full OCR all pages".to_string(),
    }
}

pub(crate) fn pdf_profile_label(profile: ProjectSourcePdfProfile) -> &'static str {
    match profile {
        ProjectSourcePdfProfile::Standard => "standard embedded text handling",
        ProjectSourcePdfProfile::LayoutHeavy => "layout-aware embedded text handling",
        ProjectSourcePdfProfile::OcrRescue | ProjectSourcePdfProfile::ForceOcr => {
            "full OCR all pages handling"
        }
        ProjectSourcePdfProfile::NoAutomaticOcr => "no automatic OCR handling",
    }
}

pub(crate) fn canonical_pdf_profile(profile: ProjectSourcePdfProfile) -> ProjectSourcePdfProfile {
    match profile {
        ProjectSourcePdfProfile::OcrRescue => ProjectSourcePdfProfile::ForceOcr,
        profile => profile,
    }
}

pub(crate) fn pdf_profile_for_record(record: &DocumentRecord) -> ProjectSourcePdfProfile {
    if let Some(profile) = record.pdf_profile {
        return canonical_pdf_profile(profile);
    }

    match record.extraction_route {
        Some(ProjectSourceExtractionRoute::ForceOcr) => ProjectSourcePdfProfile::ForceOcr,
        Some(
            ProjectSourceExtractionRoute::OcrIfNeeded | ProjectSourceExtractionRoute::OcrRequired,
        ) => ProjectSourcePdfProfile::ForceOcr,
        Some(ProjectSourceExtractionRoute::EmbeddedText)
        | Some(ProjectSourceExtractionRoute::PlainText)
        | Some(ProjectSourceExtractionRoute::DocxExtract)
        | Some(ProjectSourceExtractionRoute::HtmlExtract)
        | Some(ProjectSourceExtractionRoute::Unsupported)
        | Some(ProjectSourceExtractionRoute::Undecided)
        | None => ProjectSourcePdfProfile::Standard,
    }
}

pub(crate) fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn selected_record_by_path(
    records: &[DocumentRecord],
) -> HashMap<PathBuf, DocumentRecord> {
    records
        .iter()
        .cloned()
        .map(|record| (canonical_or_original(&record.source_path), record))
        .collect()
}

pub(crate) fn project_pdf_preparation_plan(
    records: &[DocumentRecord],
    selected_paths: &[PathBuf],
    profile_override: Option<ProjectSourcePdfProfile>,
) -> Result<Vec<ProjectPdfPreparationPlanItem>, String> {
    let records_by_path = selected_record_by_path(records);
    let mut items = Vec::with_capacity(selected_paths.len());

    for selected_path in selected_paths {
        let key = canonical_or_original(selected_path);
        let record = records_by_path.get(&key).ok_or_else(|| {
            format!(
                "Selected PDF source is not processable: {}",
                selected_path.display()
            )
        })?;

        if record.document_type != DocumentType::Pdf {
            return Err(format!(
                "Selected source is not a PDF: {}",
                selected_path.display()
            ));
        }

        let profile = profile_override
            .map(canonical_pdf_profile)
            .unwrap_or_else(|| pdf_profile_for_record(record));
        items.push(ProjectPdfPreparationPlanItem {
            path: record.source_path.clone(),
            profile,
            profile_label: pdf_profile_label(profile).to_string(),
        });
    }

    Ok(items)
}

pub(crate) fn project_pdf_preparation_batch_label(
    items: &[ProjectPdfPreparationPlanItem],
) -> String {
    let mut profiles = items.iter().map(|item| item.profile);
    let Some(first_profile) = profiles.next() else {
        return "no PDF handling".to_string();
    };

    if profiles.all(|profile| profile == first_profile) {
        pdf_profile_label(first_profile).to_string()
    } else {
        "mixed stored handling".to_string()
    }
}

pub(crate) fn record_with_preparation_profile(
    record: &DocumentRecord,
    item: &ProjectPdfPreparationPlanItem,
) -> DocumentRecord {
    let mut prepared = record.clone();
    prepared.pdf_profile = Some(item.profile);
    prepared.extraction_route = Some(item.profile.extraction_route());
    prepared
}

pub(crate) fn pdf_intake_file_name(record: &DocumentRecord) -> String {
    record
        .relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| record.relative_path.to_string_lossy().to_string())
}

// PDFium page access and OCR recognition remain guarded, but a small pool lets
// neighbouring pages overlap render work with the shared recogniser.
pub(crate) const DEFAULT_OCR_WORKERS: usize = 3;
pub(crate) const MAX_INTERNAL_OCR_WORKERS: usize = 5;

pub(crate) fn configured_ocr_worker_count() -> usize {
    let available = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(DEFAULT_OCR_WORKERS)
        .max(1);
    let default_workers = DEFAULT_OCR_WORKERS.min(available).max(1);

    std::env::var("CORPUSWRIGHT_OCR_WORKERS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_INTERNAL_OCR_WORKERS).min(available).max(1))
        .unwrap_or(default_workers)
}

pub(crate) fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn intake_progress_metrics(
    started_at: Instant,
    completed_pages: usize,
    total_pages: Option<usize>,
) -> (u64, Option<u64>, Option<f64>) {
    let elapsed = started_at.elapsed();
    let elapsed_ms = duration_ms(elapsed);
    if completed_pages == 0 || elapsed_ms == 0 {
        return (elapsed_ms, None, None);
    }

    let elapsed_minutes = elapsed.as_secs_f64() / 60.0;
    let pages_per_minute = if elapsed_minutes > 0.0 {
        Some(completed_pages as f64 / elapsed_minutes)
    } else {
        None
    };
    let estimated_remaining_ms = total_pages.map(|total| {
        let remaining = total.saturating_sub(completed_pages);
        if remaining == 0 {
            0
        } else {
            let per_page = elapsed.as_secs_f64() / completed_pages as f64;
            duration_ms(Duration::from_secs_f64(per_page * remaining as f64))
        }
    });

    (elapsed_ms, estimated_remaining_ms, pages_per_minute)
}

pub(crate) fn latest_page_warning(page_timing: &PdfIntakePageTiming) -> Option<String> {
    page_timing.warnings.last().cloned().or_else(|| {
        page_timing.render_clamped.then(|| {
            format!(
                "OCR render size was clamped on page {}.",
                page_timing.page_number
            )
        })
    })
}

pub(crate) fn log_pdf_intake_timing_summary(
    record: &DocumentRecord,
    profile: &str,
    method: &str,
    worker_count: usize,
    total_elapsed: Duration,
    timings: &[PdfIntakePageTiming],
) {
    if timings.is_empty() {
        eprintln!(
            "PDF intake timing: file={} profile={} method={} workers={} pages=0 total_ms={}",
            pdf_intake_file_name(record),
            profile,
            method,
            worker_count,
            duration_ms(total_elapsed)
        );
        return;
    }

    let total_page_ms: u64 = timings.iter().map(|timing| timing.page_total_ms).sum();
    let total_chars: usize = timings.iter().map(|timing| timing.chars_extracted).sum();
    let warning_count: usize = timings.iter().map(|timing| timing.warnings.len()).sum();
    let clamped_pages = timings
        .iter()
        .filter(|timing| timing.render_clamped)
        .count();
    let average_page_ms = total_page_ms as f64 / timings.len() as f64;
    let average_chars = total_chars as f64 / timings.len() as f64;
    let pages_per_minute = timings.len() as f64 / (total_elapsed.as_secs_f64() / 60.0).max(0.001);
    let slowest_pages = {
        let mut sorted: Vec<&PdfIntakePageTiming> = timings.iter().collect();
        sorted.sort_by_key(|timing| std::cmp::Reverse(timing.page_total_ms));
        sorted
            .into_iter()
            .take(5)
            .map(|timing| {
                format!(
                    "p{}={}ms/{}chars",
                    timing.page_number, timing.page_total_ms, timing.chars_extracted
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let total_postprocess_ms: u64 = timings.iter().map(|timing| timing.postprocess_ms).sum();

    eprintln!(
        "PDF intake timing: file={} profile={} method={} workers={} pages={} total_ms={} pages_per_minute={:.2} avg_page_ms={:.1} avg_chars_per_page={:.1} postprocess_ms={} warnings={} clamped_pages={} slowest=[{}] render_ocr_split=unavailable",
        pdf_intake_file_name(record),
        profile,
        method,
        worker_count,
        timings.len(),
        duration_ms(total_elapsed),
        pages_per_minute,
        average_page_ms,
        average_chars,
        total_postprocess_ms,
        warning_count,
        clamped_pages,
        slowest_pages
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_pdf_intake_progress(
    window: &Window,
    record: &DocumentRecord,
    file_index: usize,
    total_files: usize,
    current_page: usize,
    total_pages: Option<usize>,
    profile: &str,
    method: &str,
    status: &str,
    elapsed_ms: u64,
    estimated_remaining_ms: Option<u64>,
    pages_per_minute: Option<f64>,
    latest_warning: Option<String>,
) {
    let _ = window.emit(
        "pdf-intake-materialization-progress",
        PdfIntakeMaterializationProgress {
            current_file: pdf_intake_file_name(record),
            current_file_index: file_index + 1,
            total_files,
            current_page,
            total_pages,
            profile: profile.to_string(),
            method: method.to_string(),
            status: status.to_string(),
            elapsed_ms,
            estimated_remaining_ms,
            pages_per_minute,
            latest_warning,
        },
    );
}

pub(crate) fn pdf_intake_cancelled_message() -> String {
    "PDF intake cancelled before materialisation completed. No corpus was loaded.".to_string()
}

pub(crate) fn project_pdf_extraction_failure_reason(
    error: &str,
    cancel: &AtomicBool,
) -> &'static str {
    if cancel.load(Ordering::Relaxed) || error.to_lowercase().contains("cancelled") {
        "extraction_cancelled"
    } else {
        "text_extraction_failed"
    }
}

pub(crate) fn extraction_reason_codes_for_warnings(warnings: &[String]) -> Vec<String> {
    let mut codes = Vec::new();
    for warning in warnings {
        let warning = warning.to_lowercase();
        let code = if warning.contains("clamped") || warning.contains("render size") {
            "render_size_clamped"
        } else if warning.contains("ocr") {
            "ocr_generated"
        } else if warning.contains("low-quality") || warning.contains("low quality") {
            "low_text_density"
        } else if warning.contains("layout") {
            "layout_warning"
        } else if warning.contains("repeated") {
            "many_repeated_artifacts"
        } else {
            continue;
        };
        if !codes.contains(&code.to_string()) {
            codes.push(code.to_string());
        }
    }
    codes
}

pub(crate) fn extract_materialized_ocr_page(
    bytes: &[u8],
    page_index: usize,
    options: PdfExtractionOptions,
) -> Result<MaterializedOcrPage, String> {
    let page_started_at = Instant::now();
    let result = extract_pdf_page_range(bytes, page_index, 1, options, None)
        .map_err(|error| format!("PDF materialisation failed: {error}"))?;
    let mut warnings = result.warnings;
    let page_number = page_index + 1;
    let mut render_clamped = false;
    let mut text = None;
    let mut chars_extracted = 0;

    let postprocess_started_at = Instant::now();
    match result.pages.into_iter().next() {
        Some(page) => {
            render_clamped = page.render_clamped;
            warnings.extend(page.warnings);
            if page.render_clamped {
                warnings.push(format!(
                    "OCR render size was clamped on page {}.",
                    page.page_number
                ));
            }
            if let Some(page_error) = page.error {
                let message = format!(
                    "Page {} extraction failed during materialisation: {}",
                    page.page_number, page_error
                );
                warnings.push(message);
            } else {
                let trimmed = page.text.trim();
                chars_extracted = trimmed.chars().count();
                if !trimmed.is_empty() {
                    text = Some(trimmed.to_string());
                }
            }
        }
        None => {
            let message = format!("Page {page_number} extraction returned no page result.");
            warnings.push(message);
        }
    }
    let postprocess_ms = duration_ms(postprocess_started_at.elapsed());

    Ok(MaterializedOcrPage {
        page_index,
        text,
        timing: PdfIntakePageTiming {
            page_number,
            page_total_ms: duration_ms(page_started_at.elapsed()),
            postprocess_ms,
            chars_extracted,
            warnings,
            render_clamped,
        },
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_force_ocr_pdf(
    window: &Window,
    record: &DocumentRecord,
    file_index: usize,
    total_files: usize,
    profile: &str,
    options: PdfExtractionOptions,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
    cancel: &AtomicBool,
) -> Result<CacheEntry, String> {
    let method = pdf_intake_method_label(options);
    let bytes = std::fs::read(&record.source_path)
        .map_err(|error| format!("Failed to read PDF: {error}"))?;

    let total_pages =
        pdf_page_count(&bytes).map_err(|error| format!("PDF materialisation failed: {error}"))?;
    let started_at = Instant::now();
    let worker_count = configured_ocr_worker_count();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_count)
        .build()
        .map_err(|error| format!("Failed to configure OCR workers: {error}"))?;

    let mut page_texts = Vec::with_capacity(total_pages);
    let mut warnings = Vec::new();
    let mut timings = Vec::with_capacity(total_pages);
    let mut latest_warning = None;
    let mut completed_pages = 0;

    emit_pdf_intake_progress(
        window,
        record,
        file_index,
        total_files,
        0,
        Some(total_pages),
        profile,
        &method,
        "Extracting OCR text...",
        0,
        None,
        None,
        None,
    );

    let mut next_page_index = 0;
    while next_page_index < total_pages {
        if cancel.load(Ordering::Relaxed) {
            return Err(pdf_intake_cancelled_message());
        }

        let batch_end = next_page_index
            .saturating_add(worker_count)
            .min(total_pages);
        // Check cancellation before spawning the parallel OCR batch
        if cancel.load(Ordering::Relaxed) {
            return Err(pdf_intake_cancelled_message());
        }
        let mut batch_results = pool.install(|| {
            (next_page_index..batch_end)
                .into_par_iter()
                .map(|page_index| extract_materialized_ocr_page(&bytes, page_index, options))
                .collect::<Result<Vec<_>, _>>()
        })?;
        batch_results.sort_by_key(|page| page.page_index);

        for page_result in batch_results {
            if let Some(page_warning) = latest_page_warning(&page_result.timing) {
                latest_warning = Some(page_warning);
            }
            page_texts.push(page_result.text.unwrap_or_default());
            let page_warnings = page_result.timing.warnings.clone();
            warnings.extend(page_warnings);
            timings.push(page_result.timing);
            completed_pages += 1;

            let (elapsed_ms, estimated_remaining_ms, pages_per_minute) =
                intake_progress_metrics(started_at, completed_pages, Some(total_pages));
            emit_pdf_intake_progress(
                window,
                record,
                file_index,
                total_files,
                completed_pages,
                Some(total_pages),
                profile,
                &method,
                "Extracting OCR text...",
                elapsed_ms,
                estimated_remaining_ms,
                pages_per_minute,
                latest_warning.clone(),
            );

            if cancel.load(Ordering::Relaxed) {
                return Err(pdf_intake_cancelled_message());
            }
        }

        next_page_index = batch_end;
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(pdf_intake_cancelled_message());
    }

    let extracted_text = page_texts
        .iter()
        .map(|page| page.trim())
        .filter(|page| !page.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    let entry = CacheEntry {
        extracted_text,
        warnings,
        page_count: Some(total_pages),
        page_texts: Some(page_texts),
    };
    cache.insert_extracted(record, Some(options), cleaning_config, entry.clone());

    log_pdf_intake_timing_summary(
        record,
        profile,
        &method,
        worker_count,
        started_at.elapsed(),
        &timings,
    );

    let (elapsed_ms, estimated_remaining_ms, pages_per_minute) =
        intake_progress_metrics(started_at, completed_pages, Some(total_pages));
    emit_pdf_intake_progress(
        window,
        record,
        file_index,
        total_files,
        completed_pages,
        Some(total_pages),
        profile,
        &method,
        "OCR text extracted.",
        elapsed_ms,
        estimated_remaining_ms,
        pages_per_minute,
        latest_warning,
    );

    Ok(entry)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize_pdf_record(
    window: &Window,
    record: &DocumentRecord,
    file_index: usize,
    total_files: usize,
    profile: &str,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
    cancel: &AtomicBool,
) -> Result<CacheEntry, String> {
    let options =
        PdfExtractionOptions::raw_from_record_and_cleaning_config(record, cleaning_config);
    if options.text_source == PdfTextSource::ForceOcr {
        return materialize_force_ocr_pdf(
            window,
            record,
            file_index,
            total_files,
            profile,
            options,
            cleaning_config,
            cache,
            cancel,
        );
    }

    let method = pdf_intake_method_label(options);
    let started_at = Instant::now();
    let total_pages = std::fs::read(&record.source_path)
        .ok()
        .and_then(|bytes| pdf_page_count(&bytes).ok());
    emit_pdf_intake_progress(
        window,
        record,
        file_index,
        total_files,
        0,
        total_pages,
        profile,
        &method,
        "Extracting source text...",
        0,
        None,
        None,
        None,
    );

    if cancel.load(Ordering::Relaxed) {
        return Err(pdf_intake_cancelled_message());
    }

    let entry = cache
        .get_or_extract(record, Some(options), cleaning_config)
        .map_err(|error| format!("PDF materialisation failed: {error}"))?;
    let completed_pages = entry.page_count.or(total_pages).unwrap_or(0);
    let latest_warning = entry.warnings.last().cloned();
    cache.insert_extracted(record, Some(options), cleaning_config, entry.clone());
    let (elapsed_ms, estimated_remaining_ms, pages_per_minute) =
        intake_progress_metrics(started_at, completed_pages, Some(completed_pages));

    emit_pdf_intake_progress(
        window,
        record,
        file_index,
        total_files,
        completed_pages,
        Some(completed_pages),
        profile,
        &method,
        "Source text extracted.",
        elapsed_ms,
        estimated_remaining_ms,
        pages_per_minute,
        latest_warning,
    );

    if cancel.load(Ordering::Relaxed) {
        return Err(pdf_intake_cancelled_message());
    }

    Ok(entry)
}

pub(crate) fn pdf_page_range_cache_key(
    record: &DocumentRecord,
    start_page_index: usize,
    page_count: usize,
    max_chars_per_page: Option<usize>,
    text_source: PdfTextSource,
    ocr_quality: PdfOcrQuality,
    cleaning_config: &CleaningConfig,
) -> PdfPageRangeCacheKey {
    let cleaning_config_json =
        serde_json::to_string(cleaning_config).unwrap_or_else(|_| format!("{cleaning_config:?}"));
    let ocr_model_identity = match text_source {
        PdfTextSource::EmbeddedText => None,
        PdfTextSource::Ocr | PdfTextSource::ForceOcr => {
            corpuswright_core::sources::pdf::ocr::ocr_model_identity()
        }
    };

    PdfPageRangeCacheKey {
        source_path: record.source_path.clone(),
        size_bytes: record.size_bytes,
        modified_time_secs: modified_time_secs(&record.source_path),
        start_page_index,
        page_count,
        max_chars_per_page,
        text_source,
        ocr_quality,
        cleaning_config_json,
        ocr_model_identity,
    }
}

pub(crate) fn apply_cleaning_to_page_range(
    result: &mut PdfPageRangeResult,
    cleaning_config: &CleaningConfig,
    max_chars_per_page: Option<usize>,
) {
    for page in &mut result.pages {
        if page.error.is_none() {
            page.text = clean_text(&page.text, cleaning_config);
        }

        let char_count = page.text.chars().count();
        if let Some(limit) = max_chars_per_page
            && char_count > limit
        {
            page.text = page.text.chars().take(limit).collect();
            page.warnings.push(format!(
                "Page {} text was truncated to {} characters after cleaning.",
                page.page_number, limit
            ));
        }
        page.char_count = page.text.chars().count();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
