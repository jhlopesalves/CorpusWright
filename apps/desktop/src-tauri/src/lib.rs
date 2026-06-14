use corpuswright_core::cache::{CacheEntry, ExtractionCache};
use corpuswright_core::clean::{CleaningConfig, PdfOcrQuality, PdfTextSource, clean_text};
use corpuswright_core::export::{ExportError, ExportOptions, ExportReport, export_corpus};
use corpuswright_core::pdf::{
    PdfExtractionOptions, PdfPageRangeResult, extract_pdf_page_range, pdf_page_count,
};
use corpuswright_core::pdf_audit::{PdfAuditResult, audit_pdf_files};
use corpuswright_core::preview::{
    CombinedPreview, PreviewOptions, preview_files_with_config, preview_processed_files,
};
use corpuswright_core::repeated_artifacts::{
    CancellationFlag, RepeatedArtifactScanConfig, RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{
    DocumentRecord, DocumentType, ScanReport, load_files, scan_directory,
};
use corpuswright_core::search::{SearchResult, search_corpus};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

use rayon::prelude::*;

struct ScanState {
    cancel: CancellationFlag,
}

struct PdfIntakeState {
    cancel: Arc<AtomicBool>,
}

struct CorpusStateInner {
    version: u64,
    root: Option<PathBuf>,
    records: Vec<DocumentRecord>,
}

impl CorpusStateInner {
    fn empty() -> Self {
        Self {
            version: 0,
            root: None,
            records: vec![],
        }
    }

    fn load(&mut self, root: Option<PathBuf>, records: Vec<DocumentRecord>) {
        self.version += 1;
        self.root = root;
        self.records = records;
    }

    fn clear(&mut self) {
        self.version += 1;
        self.root = None;
        self.records.clear();
    }
}

struct CorpusState {
    inner: RwLock<CorpusStateInner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PdfPageRangeCacheKey {
    source_path: PathBuf,
    size_bytes: u64,
    modified_time_secs: Option<u64>,
    start_page_index: usize,
    page_count: usize,
    max_chars_per_page: Option<usize>,
    text_source: PdfTextSource,
    ocr_quality: PdfOcrQuality,
    cleaning_config_json: String,
    ocr_model_identity: Option<String>,
}

struct PdfPageRangeCache {
    inner: RwLock<HashMap<PdfPageRangeCacheKey, PdfPageRangeResult>>,
}

impl PdfPageRangeCache {
    fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    fn get(&self, key: &PdfPageRangeCacheKey) -> Option<PdfPageRangeResult> {
        self.inner.read().unwrap().get(key).cloned()
    }

    fn insert(&self, key: PdfPageRangeCacheKey, result: PdfPageRangeResult) {
        self.inner.write().unwrap().insert(key, result);
    }

    fn clear(&self) {
        self.inner.write().unwrap().clear();
    }
}

impl CorpusState {
    fn records_for_indices(
        &self,
        indices: &[usize],
        corpus_version: u64,
    ) -> Result<Vec<DocumentRecord>, String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        let records = &inner.records;
        let mut result = Vec::with_capacity(indices.len());
        for &i in indices {
            let record = records.get(i).ok_or_else(|| {
                format!(
                    "Index {} is out of bounds (corpus has {} records).",
                    i,
                    records.len()
                )
            })?;
            result.push(record.clone());
        }
        Ok(result)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusLoadResult {
    report: ScanReport,
    corpus_version: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PdfIntakeMaterializationProgress {
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

struct PdfIntakePageTiming {
    page_number: usize,
    page_total_ms: u64,
    postprocess_ms: u64,
    chars_extracted: usize,
    warnings: Vec<String>,
    render_clamped: bool,
}

struct MaterializedOcrPage {
    page_index: usize,
    text: Option<String>,
    timing: PdfIntakePageTiming,
}

#[derive(Clone, Serialize)]
struct ExportProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordCountBatchResult {
    total_words: u64,
    skipped_ocr_mode: bool,
}

#[tauri::command(async)]
fn scan_directory_command(
    path: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
) -> Result<CorpusLoadResult, String> {
    cache.clear();
    page_cache.clear();
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
) -> Result<CorpusLoadResult, String> {
    cache.clear();
    page_cache.clear();
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
fn audit_pdf_files_command(paths: Vec<String>) -> Result<Vec<PdfAuditResult>, String> {
    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    Ok(audit_pdf_files(path_bufs))
}

fn pdf_intake_method_label(options: PdfExtractionOptions) -> String {
    match options.text_source {
        PdfTextSource::EmbeddedText => match options.strategy {
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumFlat => {
                "embedded text".to_string()
            }
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn => {
                "layout-heavy embedded text".to_string()
            }
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => {
                "experimental visual columns".to_string()
            }
        },
        PdfTextSource::Ocr => "OCR rescue".to_string(),
        PdfTextSource::ForceOcr => "force OCR".to_string(),
    }
}

fn pdf_intake_file_name(record: &DocumentRecord) -> String {
    record
        .relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| record.relative_path.to_string_lossy().to_string())
}

// PDFium page access and OCR recognition remain guarded, but a small pool lets
// neighbouring pages overlap render work with the shared recogniser.
const DEFAULT_OCR_WORKERS: usize = 3;
const MAX_INTERNAL_OCR_WORKERS: usize = 5;

fn configured_ocr_worker_count() -> usize {
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

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn intake_progress_metrics(
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

fn latest_page_warning(page_timing: &PdfIntakePageTiming) -> Option<String> {
    page_timing.warnings.last().cloned().or_else(|| {
        page_timing.render_clamped.then(|| {
            format!(
                "OCR render size was clamped on page {}.",
                page_timing.page_number
            )
        })
    })
}

fn log_pdf_intake_timing_summary(
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
fn emit_pdf_intake_progress(
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

fn pdf_intake_cancelled_message() -> String {
    "PDF intake cancelled before materialisation completed. No corpus was loaded.".to_string()
}

fn extract_materialized_ocr_page(
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
fn materialize_force_ocr_pdf(
    window: &Window,
    record: &DocumentRecord,
    file_index: usize,
    total_files: usize,
    profile: &str,
    options: PdfExtractionOptions,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
    cancel: &AtomicBool,
) -> Result<(), String> {
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

    let mut page_texts = Vec::new();
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
        "Materialising raw OCR text...",
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
            if let Some(text) = page_result.text {
                page_texts.push(text);
            }
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
                "Materialising raw OCR text...",
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

    cache.insert_extracted(
        record,
        Some(options),
        cleaning_config,
        CacheEntry {
            extracted_text: page_texts.join("\n\n"),
            warnings,
            page_count: Some(total_pages),
        },
    );

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
        "Materialised raw OCR text.",
        elapsed_ms,
        estimated_remaining_ms,
        pages_per_minute,
        latest_warning,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn materialize_pdf_record(
    window: &Window,
    record: &DocumentRecord,
    file_index: usize,
    total_files: usize,
    profile: &str,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let options = PdfExtractionOptions::raw_from_cleaning_config(cleaning_config);
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
        "Materialising raw extracted text...",
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
    cache.insert_extracted(record, Some(options), cleaning_config, entry);
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
        "Materialised raw extracted text.",
        elapsed_ms,
        estimated_remaining_ms,
        pages_per_minute,
        latest_warning,
    );

    if cancel.load(Ordering::Relaxed) {
        return Err(pdf_intake_cancelled_message());
    }

    Ok(())
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
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<CorpusLoadResult, String> {
    if paths.is_empty() {
        return Err("Choose at least one PDF.".to_string());
    }

    intake_state.cancel.store(false, Ordering::Relaxed);
    cache.clear();
    page_cache.clear();
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
) -> Result<(), String> {
    cache.clear();
    page_cache.clear();
    corpus_state.inner.write().unwrap().clear();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn search_corpus_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    query: String,
    is_processed: bool,
    cleaning_config: CleaningConfig,
    max_hits: usize,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<SearchResult, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    search_corpus(
        &records,
        &query,
        is_processed,
        &cleaning_config,
        max_hits,
        Some(&*cache),
    )
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn preview_files_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    max_chars_per_file: usize,
    include_paths: bool,
    max_files: Option<usize>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
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
    max_chars_per_file: usize,
    include_paths: bool,
    max_files: Option<usize>,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let options = PreviewOptions {
        max_chars_per_file,
        include_paths,
        max_files,
    };
    preview_processed_files(&records, &options, &cleaning_config, Some(&*cache))
        .map_err(|e| format!("{:?}", e))
}

fn modified_time_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_secs())
        })
}

fn pdf_page_range_cache_key(
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
            corpuswright_core::pdf_ocr::ocr_model_identity()
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

fn apply_cleaning_to_page_range(
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
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    output_dir: String,
    cleaning_config: CleaningConfig,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<ExportReport, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
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
fn scan_repeated_artifacts_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    config: RepeatedArtifactScanConfig,
    cleaning_config: CleaningConfig,
    state: tauri::State<'_, ScanState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<RepeatedArtifactScanReport, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    // A previous cancellation must not leak into the next scan.
    state.cancel.store(false, Ordering::Relaxed);
    corpuswright_core::repeated_artifacts::scan_repeated_artifacts_report_with_cancel_and_cache(
        &records,
        &config,
        &cleaning_config,
        Some(&*cache),
        &state.cancel,
    )
    .map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Ok(pdfium_path) = app
                .path()
                .resolve("ocr/pdfium.dll", BaseDirectory::Resource)
                && let Some(ocr_dir) = pdfium_path.parent()
            {
                let _ = corpuswright_core::pdf_ocr::set_ocr_resource_dir(ocr_dir.to_path_buf());
            }
            Ok(())
        })
        .manage(ScanState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(PdfIntakeState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        })
        .manage(ExtractionCache::new())
        .manage(PdfPageRangeCache::new())
        .invoke_handler(tauri::generate_handler![
            scan_directory_command,
            load_files_command,
            audit_pdf_files_command,
            load_pdf_intake_files_command,
            cancel_pdf_intake_materialization_command,
            clear_corpus_command,
            search_corpus_command,
            preview_files_command,
            preview_processed_files_command,
            extract_pdf_page_range_command,
            export_corpus_command,
            compute_word_count_command,
            scan_repeated_artifacts_command,
            cancel_repeated_artifacts_command,
            save_config_file_command,
            read_config_file_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
