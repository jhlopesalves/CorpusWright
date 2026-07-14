use crate::features::corpus::state::CorpusState;
use crate::features::project;
use corpuswright_core::corpus::preview::{
    CombinedPreview, PreviewOptions, preview_file_with_config, preview_files_with_config,
    preview_processed_files,
};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use corpuswright_core::project::sources::{
    ProjectSource, ProjectSourceKind, ProjectSourcePreviewItem, ProjectSourcePreviewViewport,
    ProjectSourceSelectionDescriptor, ProjectSourceStatus,
};
use corpuswright_core::sources::cache::ExtractionCache;

pub(crate) fn preview_char_limit_for_explicit_selection(
    selected_count: usize,
    requested_limit: Option<usize>,
) -> Option<usize> {
    if selected_count == 1 {
        None
    } else {
        Some(requested_limit.unwrap_or(5_000).max(1))
    }
}

pub(crate) fn preview_char_limit_for_project_selection(
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
pub(crate) fn preview_files_command(
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
pub(crate) fn preview_processed_files_command(
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
pub(crate) fn read_project_source_preview_command(
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

pub(crate) fn project_source_has_previewable_text(source: &ProjectSource) -> bool {
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
pub(crate) fn query_selected_source_preview_viewport_command(
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

#[cfg(test)]
mod tests {
    use super::*;
    use corpuswright_core::project::sources::ProjectSourceExtractionRoute;
    use std::path::PathBuf;

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
}
