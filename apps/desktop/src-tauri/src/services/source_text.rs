use crate::*;

pub(crate) const PROJECT_SOURCE_TEXT_CACHE_VERSION: u32 = 1;
pub(crate) const PROJECT_SOURCE_TEXT_CACHE_DIR: &str = "source-text";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedProjectSourceText {
    version: u32,
    source_id: String,
    size_bytes: u64,
    modified_time_secs: Option<u64>,
    pdf_options: PdfExtractionOptions,
    entry: CacheEntry,
}

pub(crate) fn project_source_text_cache_matches_record(
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

pub(crate) fn read_project_source_text_cache(
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

pub(crate) fn analysis_document_text(
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

pub(crate) fn resolve_analysis_input_for_record(
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

pub(crate) fn text_for_concordance_resolution(
    resolution: corpuswright_core::analysis::AnalysisInputResolution,
) -> Option<String> {
    match resolution.provenance {
        corpuswright_core::analysis::AnalysisInputProvenance::Processed
        | corpuswright_core::analysis::AnalysisInputProvenance::SourceTextFallback => {
            resolution.text
        }
        corpuswright_core::analysis::AnalysisInputProvenance::EmptyOrFailed => None,
    }
    .filter(|value| !value.trim().is_empty())
}

pub(crate) fn concordance_document_from_record(
    record: &DocumentRecord,
    document_index: usize,
    project_root: Option<&Path>,
    cleaning_config: &CleaningConfig,
    cache: &ExtractionCache,
) -> Option<ConcordanceDocument> {
    let resolution =
        resolve_analysis_input_for_record(record, project_root, cleaning_config, cache);
    let text = text_for_concordance_resolution(resolution)?;
    let source_path = record.source_path.to_string_lossy().into_owned();
    let relative_path = record.relative_path.to_string_lossy().into_owned();
    Some(ConcordanceDocument {
        document_index,
        source_path: source_path.clone(),
        display_name: if relative_path.is_empty() {
            source_path
        } else {
            relative_path
        },
        text,
    })
}

pub(crate) fn modified_time_secs(path: &Path) -> Option<u64> {
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

pub(crate) fn project_source_text_cache_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".corpuswright")
        .join("cache")
        .join(PROJECT_SOURCE_TEXT_CACHE_DIR)
}

pub(crate) fn project_annotation_cache_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".corpuswright")
        .join("cache")
        .join("annotation")
}

pub(crate) fn project_source_text_cache_path(project_root: &Path, source_id: &str) -> PathBuf {
    project_source_text_cache_dir(project_root).join(format!("{source_id}.json"))
}

pub(crate) fn project_source_id_for_record(project_root: &Path, record: &DocumentRecord) -> String {
    stable_project_source_id(project_root, &record.source_path)
}

pub(crate) fn persist_project_source_text_cache(
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

pub(crate) fn hydrate_project_source_text_cache(
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
