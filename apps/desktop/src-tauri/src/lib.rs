use corpuswright_core::cache::ExtractionCache;
use corpuswright_core::clean::CleaningConfig;
use corpuswright_core::export::{ExportError, ExportOptions, ExportReport, export_corpus};
use corpuswright_core::preview::{
    CombinedPreview, PreviewOptions, preview_files, preview_processed_files,
};
use corpuswright_core::repeated_artifacts::{
    CancellationFlag, RepeatedArtifactScanConfig, RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{DocumentRecord, ScanReport, load_files, scan_directory};
use corpuswright_core::search::{SearchResult, search_corpus};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

struct ScanState {
    cancel: CancellationFlag,
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
struct ExportProgress {
    current: usize,
    total: usize,
    current_file: String,
}

#[tauri::command(async)]
fn scan_directory_command(
    path: String,
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CorpusLoadResult, String> {
    cache.clear();
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
) -> Result<CorpusLoadResult, String> {
    cache.clear();
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
fn clear_corpus_command(
    corpus_state: tauri::State<'_, CorpusState>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<(), String> {
    cache.clear();
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

#[tauri::command(async)]
fn preview_files_command(
    indices: Vec<usize>,
    corpus_version: u64,
    corpus: tauri::State<'_, CorpusState>,
    max_chars_per_file: usize,
    include_paths: bool,
    max_files: Option<usize>,
    cache: tauri::State<'_, ExtractionCache>,
) -> Result<CombinedPreview, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let options = PreviewOptions {
        max_chars_per_file,
        include_paths,
        max_files,
    };
    preview_files(&records, &options, Some(&*cache)).map_err(|e| format!("{:?}", e))
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
) -> Result<u64, String> {
    let records = corpus.records_for_indices(&indices, corpus_version)?;
    let total_words: u64 = records
        .iter()
        .map(|record| {
            corpuswright_core::word_count::count_words_for_record(
                record,
                &cleaning_config,
                Some(&*cache),
            ) as u64
        })
        .sum();
    Ok(total_words)
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
        .manage(CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        })
        .manage(ExtractionCache::new())
        .invoke_handler(tauri::generate_handler![
            scan_directory_command,
            load_files_command,
            clear_corpus_command,
            search_corpus_command,
            preview_files_command,
            preview_processed_files_command,
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
