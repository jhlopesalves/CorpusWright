use crate::*;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectCorpusLoadResult {
    summary: CorpusSummary,
    corpus_version: u64,
    record_count: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WordCountBatchResult {
    total_words: u64,
    skipped_ocr_mode: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WordCountWindowResult {
    total_words: u64,
    skipped_ocr_mode: bool,
    processed_count: usize,
    total_count: usize,
}

#[tauri::command(async)]
pub(crate) fn scan_directory_command(
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
pub(crate) fn load_files_command(
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
pub(crate) fn load_project_source_records_command(
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
pub(crate) fn clear_corpus_command(
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
pub(crate) fn search_corpus_command(
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

#[tauri::command(async)]
pub(crate) fn compute_word_count_command(
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
pub(crate) fn compute_word_count_window_command(
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
