use crate::features::corpus::state::CorpusState;
use crate::features::project;
use crate::features::sources::project_source_id_for_record;
use corpuswright_core::corpus::DocumentRecord;
use corpuswright_core::project::sources::{
    ProjectSourceKind, ProjectSourceSelectionDescriptor, ProjectSourceStatus,
};
use std::path::{Path, PathBuf};

pub(crate) fn records_for_project_processing(
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

pub(crate) fn processed_text_path(
    project_root: &Path,
    output_dir: &Path,
    record: &DocumentRecord,
) -> PathBuf {
    let source_id = project_source_id_for_record(project_root, record);
    output_dir.join(format!("{source_id}.txt"))
}

pub(crate) fn process_failure_reason(error: &str) -> &'static str {
    if error.contains("OCR text is not materialised") {
        "ocr_required"
    } else {
        "parse_failed"
    }
}
