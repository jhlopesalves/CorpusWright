//! Installed-package workflow compiled only for the dedicated `native-smoke` test package.

use corpuswright_core::analysis::{
    AnalysisBuildOptions, AnalysisDocumentSelection, ConcordanceContextFilterDirection,
    ConcordanceDocument, ConcordanceQuery, ConcordanceSortKey, CorpusOverviewContext,
    FrequencyGroupMode, FrequencyMatchMode, FrequencyQuery, FrequencySortKey, FrequencyTokenClass,
    SortDirection, build_analysis_corpus, cleaning_config_signature, query_frequency_list,
    resolve_analysis_input, search_concordance_documents,
};
use corpuswright_core::corpus::preview::{PreviewOptions, preview_files};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::sources::cache::{
    DocumentTextMode, ExtractionCache, document_text_for_record,
};
use corpuswright_core::text::tokenisation::TokenizationConfig;
use corpuswright_core::{CancellationToken, ExportOptions, export_corpus, scan_directory};
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

static STARTED: AtomicBool = AtomicBool::new(false);

pub(crate) fn start(app: AppHandle) {
    if env::var_os("CORPUSWRIGHT_NATIVE_SMOKE_CORPUS").is_none()
        || STARTED.swap(true, Ordering::AcqRel)
    {
        return;
    }
    std::thread::spawn(move || {
        let result_path = env::var_os("CORPUSWRIGHT_NATIVE_SMOKE_RESULT").map(PathBuf::from);
        let result = run(&app);
        if let Some(path) = result_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let payload = match &result {
                Ok(evidence) => json!({ "status": "passed", "evidence": evidence }),
                Err(error) => json!({ "status": "failed", "error": error }),
            };
            let _ = fs::write(path, format!("{}\n", payload));
        }
        app.exit(if result.is_ok() { 0 } else { 1 });
    });
}

fn run(app: &AppHandle) -> Result<serde_json::Value, String> {
    let corpus_dir = required_env_path("CORPUSWRIGHT_NATIVE_SMOKE_CORPUS")?;
    let export_dir = required_env_path("CORPUSWRIGHT_NATIVE_SMOKE_EXPORT")?;
    let ocr_dir = app
        .path()
        .resolve("ocr", BaseDirectory::Resource)
        .map_err(|error| format!("resolve installed OCR resources: {error}"))?;
    let udpipe_dir = app
        .path()
        .resolve("udpipe", BaseDirectory::Resource)
        .map_err(|error| format!("resolve installed UDPipe resources: {error}"))?;
    verify_resources(&ocr_dir, &udpipe_dir)?;
    corpuswright_core::sources::pdf::ocr::set_ocr_resource_dir(ocr_dir.clone())
        .map_err(|error| format!("configure installed OCR resources: {error}"))?;
    let pdfium_path = corpuswright_core::sources::pdf::ocr::pdfium_library_path_for_dir(&ocr_dir);
    corpuswright_core::sources::pdf::ocr::init_pdfium()
        .map_err(|error| format!("load installed PDFium library: {error}"))?;

    let scan = scan_directory(&corpus_dir).map_err(|error| format!("scan failed: {error:?}"))?;
    if scan.files.is_empty() {
        return Err("generated corpus scan returned no documents".to_string());
    }
    let cache = ExtractionCache::new();
    let preview = preview_files(&scan.files, &PreviewOptions::default(), Some(&cache))
        .map_err(|error| format!("preview failed: {error:?}"))?;
    if preview.total_files_previewed == 0 {
        return Err("preview returned no documents".to_string());
    }

    let cleaning = CleaningConfig::default();
    let cancellation = CancellationToken::new();
    let corpus = build_analysis_corpus(
        &scan.files,
        AnalysisBuildOptions {
            built_at: "native-package-smoke".to_string(),
            build_signature: "native-package-smoke-v1".to_string(),
            processing_config_signature: cleaning_config_signature(&cleaning),
            overview_context: CorpusOverviewContext::default(),
            udpipe_model_dir: Some(udpipe_dir.clone()),
            force_compute_hashes: true,
            language_overrides: HashMap::new(),
            tokenization: TokenizationConfig::default(),
            annotation_cache_root: None,
        },
        &cancellation,
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
    )
    .map_err(|error| format!("analysis build failed: {error}"))?;
    let frequency = query_frequency_list(
        &corpus,
        &FrequencyQuery {
            selection: AnalysisDocumentSelection::default(),
            query: "synthetic".to_string(),
            match_mode: FrequencyMatchMode::Contains,
            minimum_frequency: 1,
            token_class: FrequencyTokenClass::All,
            hide_stopwords: false,
            group_by: FrequencyGroupMode::Surface,
            case_fold: false,
            language: None,
            pos_filter: None,
            sort_key: FrequencySortKey::Frequency,
            sort_direction: SortDirection::Desc,
            offset: 0,
            limit: 25,
        },
    );
    if frequency.items.is_empty() {
        return Err("Frequency smoke query returned no rows".to_string());
    }

    let documents = scan
        .files
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let text = document_text_for_record(
                record,
                &cleaning,
                DocumentTextMode::Processed,
                Some(&cache),
            )
            .map_err(|error| format!("Concordance input failed: {error}"))?
            .text;
            Ok(ConcordanceDocument {
                document_index: index,
                source_path: record.source_path.to_string_lossy().into_owned(),
                display_name: record.relative_path.to_string_lossy().into_owned(),
                text,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let concordance = search_concordance_documents(
        &documents,
        &ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: "synthetic".to_string(),
            context_filter: None,
            context_filter_direction: ConcordanceContextFilterDirection::Both,
            case_sensitive: false,
            sort_keys: vec![ConcordanceSortKey::FilePosition],
            window_tokens: 5,
            offset: 0,
            limit: 25,
            include_rows: true,
            distribution_offset: 0,
            distribution_limit: 25,
            include_distribution: true,
        },
        &TokenizationConfig::default(),
    )
    .map_err(|error| format!("Concordance smoke query failed: {error}"))?;
    if concordance.total_hits == 0 {
        return Err("Concordance smoke query returned no hits".to_string());
    }

    if export_dir.exists() {
        fs::remove_dir_all(&export_dir).map_err(|error| format!("clear smoke export: {error}"))?;
    }
    let exported = export_corpus(
        &scan.files,
        &export_dir,
        &cleaning,
        &ExportOptions {
            app_name: "CorpusWright native package smoke".to_string(),
            app_version: None,
            overwrite: true,
        },
        None,
        Some(&cache),
    )
    .map_err(|error| format!("export smoke failed: {error:?}"))?;

    Ok(json!({
        "documentsScanned": scan.files.len(),
        "documentsPreviewed": preview.total_files_previewed,
        "analysisTokens": corpus.overview().totals.tokens,
        "frequencyRows": frequency.items.len(),
        "concordanceHits": concordance.total_hits,
        "exportedFiles": exported.files_exported,
        "pdfiumLibrary": pdfium_path,
        "ocrResourceDirectory": ocr_dir,
        "udpipeResourceDirectory": udpipe_dir,
    }))
}

fn required_env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| format!("{name} must be an absolute path"))
}

fn verify_resources(ocr_dir: &Path, udpipe_dir: &Path) -> Result<(), String> {
    let pdfium = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    for path in [
        ocr_dir.join(pdfium),
        ocr_dir.join("text-detection.rten"),
        ocr_dir.join("text-recognition.rten"),
        udpipe_dir.join("english-ud-2.1-20180111.udpipe"),
    ] {
        if !path.is_file() {
            return Err(format!("installed resource is missing: {}", path.display()));
        }
    }
    Ok(())
}
