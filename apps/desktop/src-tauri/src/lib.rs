use corpuswright_core::analysis::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildProgress, AnalysisCorpus,
    AnalysisDocumentPage, AnalysisDocumentSelection, AnnotatedFrequencyQuery,
    AnnotatedFrequencyResult, ConcordanceDocument, ConcordanceQuery, ConcordanceResult,
    CorpusOverview, CorpusOverviewContext, FrequencyGroupMode, FrequencyPage, FrequencyQuery,
    TermDistributionPage, TermDistributionQuery, annotation_cache_availability_for_corpus,
    build_analysis_corpus, cleaning_config_signature, query_analysis_documents,
    query_annotated_frequency, query_frequency_list, query_term_distribution,
    resolve_analysis_input, search_concordance_documents,
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
    CancellationFlag, RepeatedArtifactScanConfig, RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{
    CorpusSummary, DocumentRecord, DocumentType, ScanReport, load_files, scan_directory,
};
use corpuswright_core::search::{SearchResult, search_corpus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

use rayon::prelude::*;

mod project;

struct ScanState {
    cancel: CancellationFlag,
}

struct PdfIntakeState {
    cancel: Arc<AtomicBool>,
}

struct AnnotationBuildState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl AnnotationBuildState {
    fn new() -> Self {
        Self {
            cancel: Mutex::new(None),
        }
    }

    fn begin(&self) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut active = self.cancel.lock().unwrap();
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
        cancel
    }

    fn cancel(&self) {
        if let Some(cancel) = self.cancel.lock().unwrap().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    fn finish(&self, token: &Arc<AtomicBool>) {
        let mut active = self.cancel.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|cancel| Arc::ptr_eq(cancel, token))
        {
            *active = None;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum AnalysisBuildStateKind {
    NotBuilt,
    Building,
    Ready,
    Stale,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisBuildProgressPayload {
    files_done: usize,
    files_total: usize,
    tokens_seen: u64,
    types_seen: usize,
    current_phase: corpuswright_core::analysis::AnalysisBuildPhase,
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisBuildStatus {
    state: AnalysisBuildStateKind,
    progress: Option<AnalysisBuildProgressPayload>,
    error: Option<String>,
    overview: Option<CorpusOverview>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisBuildRequest {
    indices: Option<Vec<usize>>,
    corpus_version: u64,
    build_signature: String,
    cleaning_config: CleaningConfig,
}

struct AnalysisStateInner {
    generation: u64,
    state: AnalysisBuildStateKind,
    progress: Option<AnalysisBuildProgressPayload>,
    error: Option<String>,
    corpus: Option<Arc<AnalysisCorpus>>,
}

struct AnalysisState {
    inner: Arc<RwLock<AnalysisStateInner>>,
    cancel: Arc<AtomicBool>,
}

impl AnalysisState {
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AnalysisStateInner {
                generation: 0,
                state: AnalysisBuildStateKind::NotBuilt,
                progress: None,
                error: None,
                corpus: None,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn corpus(&self, cleaning_config: &CleaningConfig) -> Result<Arc<AnalysisCorpus>, String> {
        let corpus = self
            .inner
            .read()
            .unwrap()
            .corpus
            .clone()
            .ok_or_else(|| "Build the analysis corpus before querying it.".to_string())?;
        if corpus.is_stale_for_cleaning_config(cleaning_config) {
            return Err(
                "The analysis corpus is stale because processing settings changed. Rebuild it before querying."
                    .to_string(),
            );
        }
        Ok(corpus)
    }

    fn clear(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let mut inner = self.inner.write().unwrap();
        inner.generation += 1;
        inner.state = AnalysisBuildStateKind::NotBuilt;
        inner.progress = None;
        inner.error = None;
        inner.corpus = None;
    }

    fn status(&self, cleaning_config: &CleaningConfig) -> AnalysisBuildStatus {
        let inner = self.inner.read().unwrap();
        let stale = inner
            .corpus
            .as_ref()
            .is_some_and(|corpus| corpus.is_stale_for_cleaning_config(cleaning_config));
        AnalysisBuildStatus {
            state: if stale {
                AnalysisBuildStateKind::Stale
            } else {
                inner.state
            },
            progress: inner.progress.clone(),
            error: inner.error.clone(),
            overview: (!stale)
                .then(|| {
                    inner
                        .corpus
                        .as_ref()
                        .map(|corpus| corpus.overview().clone())
                })
                .flatten(),
        }
    }
}

const PROJECT_SOURCE_TEXT_CACHE_VERSION: u32 = 1;
const PROJECT_SOURCE_TEXT_CACHE_DIR: &str = "source-text";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectPdfPreparationPlanItem {
    path: PathBuf,
    profile: ProjectSourcePdfProfile,
    profile_label: String,
}

struct CorpusStateInner {
    version: u64,
    root: Option<PathBuf>,
    records: Vec<DocumentRecord>,
    record_index_by_path: HashMap<String, usize>,
}

impl CorpusStateInner {
    fn empty() -> Self {
        Self {
            version: 0,
            root: None,
            records: vec![],
            record_index_by_path: HashMap::new(),
        }
    }

    fn load(&mut self, root: Option<PathBuf>, records: Vec<DocumentRecord>) {
        self.version += 1;
        self.root = root;
        self.record_index_by_path = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record_path_key(&record.source_path), index))
            .collect();
        self.records = records;
    }

    fn clear(&mut self) {
        self.version += 1;
        self.root = None;
        self.records.clear();
        self.record_index_by_path.clear();
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

    fn records_and_root_for_indices(
        &self,
        indices: &[usize],
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, Option<PathBuf>), String> {
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
        Ok((result, inner.root.clone()))
    }

    fn all_records_and_root(
        &self,
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, Option<PathBuf>), String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        Ok((inner.records.clone(), inner.root.clone()))
    }

    fn records_window(
        &self,
        offset: usize,
        limit: usize,
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, usize), String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        let total = inner.records.len();
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        Ok((inner.records[start..end].to_vec(), total))
    }

    fn record_for_path(
        &self,
        path: &Path,
        corpus_version: u64,
    ) -> Result<Option<DocumentRecord>, String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
            return Err(
                "Corpus has been reloaded. Please re-select the source and try again.".to_string(),
            );
        }
        Ok(inner
            .record_index_by_path
            .get(&record_path_key(path))
            .and_then(|index| inner.records.get(*index))
            .cloned())
    }

    fn records_for_paths(
        &self,
        paths: Vec<PathBuf>,
        corpus_version: u64,
    ) -> Result<Vec<DocumentRecord>, String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        Ok(paths
            .into_iter()
            .map(|path| record_path_key(&path))
            .filter_map(|path| inner.record_index_by_path.get(&path))
            .filter_map(|index| inner.records.get(*index))
            .cloned()
            .collect())
    }
}

fn record_path_key(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
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
struct ProjectCorpusLoadResult {
    summary: CorpusSummary,
    corpus_version: u64,
    record_count: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedProjectSourceText {
    version: u32,
    source_id: String,
    size_bytes: u64,
    modified_time_secs: Option<u64>,
    pdf_options: PdfExtractionOptions,
    entry: CacheEntry,
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
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<CorpusLoadResult, String> {
    analysis.clear();
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
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<CorpusLoadResult, String> {
    analysis.clear();
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
fn load_project_source_records_command(
    root_path: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
    page_cache: tauri::State<'_, PdfPageRangeCache>,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<ProjectCorpusLoadResult, String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
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

fn pdf_intake_method_label(options: PdfExtractionOptions) -> String {
    match options.text_source {
        PdfTextSource::EmbeddedText => match options.strategy {
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumFlat => {
                "embedded text".to_string()
            }
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn => {
                "layout-aware embedded text".to_string()
            }
            corpuswright_core::clean::PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => {
                "experimental visual columns".to_string()
            }
        },
        PdfTextSource::Ocr => "legacy mixed OCR route".to_string(),
        PdfTextSource::ForceOcr => "full OCR all pages".to_string(),
    }
}

fn pdf_profile_label(profile: ProjectSourcePdfProfile) -> &'static str {
    match profile {
        ProjectSourcePdfProfile::Standard => "standard embedded text handling",
        ProjectSourcePdfProfile::LayoutHeavy => "layout-aware embedded text handling",
        ProjectSourcePdfProfile::OcrRescue | ProjectSourcePdfProfile::ForceOcr => {
            "full OCR all pages handling"
        }
        ProjectSourcePdfProfile::NoAutomaticOcr => "no automatic OCR handling",
    }
}

fn canonical_pdf_profile(profile: ProjectSourcePdfProfile) -> ProjectSourcePdfProfile {
    match profile {
        ProjectSourcePdfProfile::OcrRescue => ProjectSourcePdfProfile::ForceOcr,
        profile => profile,
    }
}

fn pdf_profile_for_record(record: &DocumentRecord) -> ProjectSourcePdfProfile {
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

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn selected_record_by_path(records: &[DocumentRecord]) -> HashMap<PathBuf, DocumentRecord> {
    records
        .iter()
        .cloned()
        .map(|record| (canonical_or_original(&record.source_path), record))
        .collect()
}

fn project_pdf_preparation_plan(
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

fn project_pdf_preparation_batch_label(items: &[ProjectPdfPreparationPlanItem]) -> String {
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

fn record_with_preparation_profile(
    record: &DocumentRecord,
    item: &ProjectPdfPreparationPlanItem,
) -> DocumentRecord {
    let mut prepared = record.clone();
    prepared.pdf_profile = Some(item.profile);
    prepared.extraction_route = Some(item.profile.extraction_route());
    prepared
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

fn project_pdf_extraction_failure_reason(error: &str, cancel: &AtomicBool) -> &'static str {
    if cancel.load(Ordering::Relaxed) || error.to_lowercase().contains("cancelled") {
        "extraction_cancelled"
    } else {
        "text_extraction_failed"
    }
}

fn extraction_reason_codes_for_warnings(warnings: &[String]) -> Vec<String> {
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
fn materialize_pdf_record(
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
    intake_state: tauri::State<'_, PdfIntakeState>,
) -> Result<CorpusLoadResult, String> {
    if paths.is_empty() {
        return Err("Choose at least one project PDF source.".to_string());
    }

    intake_state.cancel.store(false, Ordering::Relaxed);
    cache.clear();
    page_cache.clear();
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
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<(), String> {
    analysis.clear();
    cache.clear();
    page_cache.clear();
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

fn project_source_text_cache_matches_record(
    persisted: &PersistedProjectSourceText,
    project_root: &Path,
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
) -> bool {
    let source_id = project_source_id_for_record(project_root, record);
    let expected_options =
        PdfExtractionOptions::raw_from_record_and_cleaning_config(record, cleaning_config);

    persisted.version == PROJECT_SOURCE_TEXT_CACHE_VERSION
        && persisted.source_id == source_id
        && persisted.size_bytes == record.size_bytes
        && persisted.modified_time_secs == modified_time_secs(&record.source_path)
        && persisted.pdf_options.text_source == expected_options.text_source
        && persisted.pdf_options.strategy == expected_options.strategy
        && persisted.pdf_options.ocr_quality == expected_options.ocr_quality
}

fn read_project_source_text_cache(
    project_root: &Path,
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
) -> Option<CacheEntry> {
    let source_id = project_source_id_for_record(project_root, record);
    let cache_path = project_source_text_cache_path(project_root, &source_id);
    let json = std::fs::read_to_string(cache_path).ok()?;
    let persisted = serde_json::from_str::<PersistedProjectSourceText>(&json).ok()?;
    if project_source_text_cache_matches_record(&persisted, project_root, record, cleaning_config) {
        Some(persisted.entry)
    } else {
        None
    }
}

fn analysis_document_text(
    record: &DocumentRecord,
    project_root: Option<&Path>,
    cleaning_config: &CleaningConfig,
    mode: DocumentTextMode,
    cache: &ExtractionCache,
) -> Result<String, String> {
    if record.document_type == DocumentType::Pdf {
        let pdf_options =
            PdfExtractionOptions::raw_from_record_and_cleaning_config(record, cleaning_config);
        if let Some(entry) = project_root
            .and_then(|root| read_project_source_text_cache(root, record, cleaning_config))
            .or_else(|| cache.try_get(record, Some(pdf_options), cleaning_config))
        {
            return Ok(document_text_from_cache_entry(record, cleaning_config, mode, entry).text);
        }
    }

    document_text_for_record(record, cleaning_config, mode, Some(cache))
        .map(|document| document.text)
}

fn resolve_analysis_input_for_record(
    record: &DocumentRecord,
    project_root: Option<&Path>,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
) -> corpuswright_core::analysis::AnalysisInputResolution {
    resolve_analysis_input(
        record,
        cleaning_config,
        |record, cleaning_config| {
            analysis_document_text(
                record,
                project_root,
                cleaning_config,
                DocumentTextMode::Processed,
                cache,
            )
        },
        |record, cleaning_config| {
            analysis_document_text(
                record,
                project_root,
                cleaning_config,
                DocumentTextMode::Original,
                cache,
            )
        },
    )
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
    let cancel = annotation.begin();
    let cancel_for_task = cancel.clone();

    let task_result = tauri::async_runtime::spawn_blocking(move || {
        corpuswright_core::analysis::build_and_write_annotation_cache(
            &records,
            analysis_options,
            annotation_options,
            &cancel_for_task,
            move |progress| {
                let _ = window.emit("annotation-build-progress", progress);
            },
            |record| {
                Ok(resolve_analysis_input_for_record(
                    record,
                    project_root.as_deref(),
                    &cleaning_config,
                    &cache_inner,
                ))
            },
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
) -> Result<ConcordanceResult, String> {
    let (records, project_root) = corpus.all_records_and_root(corpus_version)?;
    let cache = cache.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut documents = Vec::new();
        for (document_index, record) in records.iter().enumerate() {
            let resolution = resolve_analysis_input_for_record(
                record,
                project_root.as_deref(),
                &cleaning_config,
                &cache,
            );
            let text = match resolution.provenance {
                corpuswright_core::analysis::AnalysisInputProvenance::Processed
                | corpuswright_core::analysis::AnalysisInputProvenance::SourceTextFallback => {
                    resolution.text
                }
                corpuswright_core::analysis::AnalysisInputProvenance::EmptyOrFailed => None,
            };
            let Some(text) = text.filter(|value| !value.trim().is_empty()) else {
                continue;
            };
            let source_path = record.source_path.to_string_lossy().into_owned();
            let relative_path = record.relative_path.to_string_lossy().into_owned();
            documents.push(ConcordanceDocument {
                document_index,
                source_path: source_path.clone(),
                display_name: if relative_path.is_empty() {
                    source_path
                } else {
                    relative_path
                },
                text,
            });
        }
        search_concordance_documents(
            &documents,
            &query,
            &corpuswright_core::tokenization::TokenizationConfig::default(),
        )
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Concordance query task failed: {error}"))?
}

#[tauri::command]
async fn query_frequency_list_command(
    query: FrequencyQuery,
    cleaning_config: CleaningConfig,
    analysis: tauri::State<'_, AnalysisState>,
) -> Result<FrequencyPage, String> {
    let corpus = analysis.corpus(&cleaning_config)?;
    tauri::async_runtime::spawn_blocking(move || query_frequency_list(&corpus, &query))
        .await
        .map_err(|error| format!("Frequency query task failed: {error}"))
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

fn project_source_text_cache_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".corpuswright")
        .join("cache")
        .join(PROJECT_SOURCE_TEXT_CACHE_DIR)
}

fn project_annotation_cache_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".corpuswright")
        .join("cache")
        .join("annotation")
}

fn project_source_text_cache_path(project_root: &Path, source_id: &str) -> PathBuf {
    project_source_text_cache_dir(project_root).join(format!("{source_id}.json"))
}

fn project_source_id_for_record(project_root: &Path, record: &DocumentRecord) -> String {
    stable_project_source_id(project_root, &record.source_path)
}

fn persist_project_source_text_cache(
    project_root: &Path,
    record: &DocumentRecord,
    pdf_options: PdfExtractionOptions,
    entry: &CacheEntry,
) -> Result<(), String> {
    let source_id = project_source_id_for_record(project_root, record);
    let cache_path = project_source_text_cache_path(project_root, &source_id);
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create project source text cache: {error}"))?;
    }

    let persisted = PersistedProjectSourceText {
        version: PROJECT_SOURCE_TEXT_CACHE_VERSION,
        source_id,
        size_bytes: record.size_bytes,
        modified_time_secs: modified_time_secs(&record.source_path),
        pdf_options,
        entry: entry.clone(),
    };
    let json = serde_json::to_string_pretty(&persisted)
        .map_err(|error| format!("Could not serialise extracted source text: {error}"))?;
    std::fs::write(cache_path, json)
        .map_err(|error| format!("Could not write extracted source text cache: {error}"))
}

fn hydrate_project_source_text_cache(
    project_root: &Path,
    records: &[DocumentRecord],
    cache: &ExtractionCache,
) {
    for record in records {
        if record.document_type != DocumentType::Pdf {
            continue;
        }

        let source_id = project_source_id_for_record(project_root, record);
        let cache_path = project_source_text_cache_path(project_root, &source_id);
        let Ok(json) = std::fs::read_to_string(&cache_path) else {
            continue;
        };
        let Ok(persisted) = serde_json::from_str::<PersistedProjectSourceText>(&json) else {
            eprintln!(
                "Ignoring invalid project source text cache: {}",
                cache_path.display()
            );
            continue;
        };
        if persisted.version != PROJECT_SOURCE_TEXT_CACHE_VERSION
            || persisted.source_id != source_id
            || persisted.size_bytes != record.size_bytes
            || persisted.modified_time_secs != modified_time_secs(&record.source_path)
        {
            continue;
        }

        cache.insert_extracted(
            record,
            Some(persisted.pdf_options),
            &CleaningConfig::default(),
            persisted.entry,
        );
    }
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

fn records_for_project_processing(
    project_root: &Path,
    selection: Option<ProjectSourceSelectionDescriptor>,
    corpus_version: u64,
    corpus: &CorpusState,
    inventory: &project::ProjectSourceInventoryState,
) -> Result<Vec<DocumentRecord>, String> {
    match selection {
        Some(selection) if selection.all_matching => {
            let paths = inventory.matching_paths(
                project_root,
                &selection.query,
                selection.excluded_ids.as_slice(),
            )?;
            corpus.records_for_paths(paths, corpus_version)
        }
        Some(selection) => {
            let mut paths = Vec::with_capacity(selection.ids.len());
            for source_id in selection.ids {
                if let Some(source) = inventory.source_by_id(project_root, &source_id)?
                    && source.kind != ProjectSourceKind::Unknown
                    && source.status != ProjectSourceStatus::Failed
                {
                    paths.push(source.path);
                }
            }
            corpus.records_for_paths(paths, corpus_version)
        }
        None => Ok(corpus.all_records_and_root(corpus_version)?.0),
    }
}

fn processed_text_path(project_root: &Path, output_dir: &Path, record: &DocumentRecord) -> PathBuf {
    let source_id = project_source_id_for_record(project_root, record);
    output_dir.join(format!("{source_id}.txt"))
}

fn process_failure_reason(error: &str) -> &'static str {
    if error.contains("OCR text is not materialised") {
        "ocr_required"
    } else {
        "parse_failed"
    }
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
) -> Result<RepeatedArtifactScanReport, String> {
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
            query_frequency_list_command,
            query_annotated_frequency_command,
            query_analysis_documents_command,
            query_term_distribution_command,
            extract_pdf_page_range_command,
            export_corpus_command,
            process_project_sources_command,
            compute_word_count_command,
            compute_word_count_window_command,
            scan_repeated_artifacts_command,
            cancel_repeated_artifacts_command,
            save_config_file_command,
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
