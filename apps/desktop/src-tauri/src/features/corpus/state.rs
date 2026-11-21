//! Stores one loaded corpus generation and validates every command lookup against it.
//!
//! Reload and clear operations increment the version. Asynchronous commands carry that version,
//! which prevents indices and paths from an earlier corpus reaching current state.

use crate::runtime::identity::{CorpusVersion, CorpusVersionSequence};
use crate::runtime::sync::{read_checked, reset_to};
use corpuswright_core::corpus::{DocumentRecord, ScanReport};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

const CORPUS_STATE_RESOURCE: &str = "corpus state";

pub(crate) struct CorpusStateInner {
    pub(crate) version: CorpusVersion,
    pub(crate) root: Option<PathBuf>,
    pub(crate) records: Vec<DocumentRecord>,
    pub(crate) record_index_by_path: HashMap<String, usize>,
}

impl CorpusStateInner {
    pub(crate) fn empty_with_version(version: CorpusVersion) -> Self {
        Self {
            version,
            root: None,
            records: vec![],
            record_index_by_path: HashMap::new(),
        }
    }

    pub(crate) fn loaded(
        version: CorpusVersion,
        root: Option<PathBuf>,
        records: Vec<DocumentRecord>,
    ) -> Self {
        // A new generation invalidates every index selected from the previous record vector.
        let record_index_by_path = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record_path_key(&record.source_path), index))
            .collect();
        Self {
            version,
            root,
            records,
            record_index_by_path,
        }
    }
}

pub(crate) struct CorpusState {
    pub(crate) inner: RwLock<CorpusStateInner>,
    versions: CorpusVersionSequence,
}

impl CorpusState {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(CorpusStateInner::empty_with_version(
                CorpusVersion::default(),
            )),
            versions: CorpusVersionSequence::default(),
        }
    }

    pub(crate) fn load(&self, root: Option<PathBuf>, records: Vec<DocumentRecord>) -> u64 {
        let version = self.versions.next();
        reset_to(
            &self.inner,
            CORPUS_STATE_RESOURCE,
            CorpusStateInner::loaded(version, root, records),
        );
        version.into_ipc()
    }

    pub(crate) fn clear(&self) {
        let version = self.versions.next();
        reset_to(
            &self.inner,
            CORPUS_STATE_RESOURCE,
            CorpusStateInner::empty_with_version(version),
        );
    }

    pub(crate) fn records_for_indices(
        &self,
        indices: &[usize],
        corpus_version: u64,
    ) -> Result<Vec<DocumentRecord>, String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
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

    pub(crate) fn records_and_root_for_indices(
        &self,
        indices: &[usize],
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, Option<PathBuf>), String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
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

    pub(crate) fn all_records_and_root(
        &self,
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, Option<PathBuf>), String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        Ok((inner.records.clone(), inner.root.clone()))
    }

    pub(crate) fn records_window(
        &self,
        offset: usize,
        limit: usize,
        corpus_version: u64,
    ) -> Result<(Vec<DocumentRecord>, usize), String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
            return Err(
                "Corpus has been reloaded. Please re-select files and try again.".to_string(),
            );
        }
        let total = inner.records.len();
        let start = offset.min(total);
        let end = start.saturating_add(limit).min(total);
        Ok((inner.records[start..end].to_vec(), total))
    }

    pub(crate) fn record_for_path(
        &self,
        path: &Path,
        corpus_version: u64,
    ) -> Result<Option<DocumentRecord>, String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
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

    pub(crate) fn record_for_path_and_root(
        &self,
        path: &Path,
        corpus_version: u64,
    ) -> Result<(Option<DocumentRecord>, Option<PathBuf>), String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
            return Err(
                "Corpus has been reloaded. Please re-select the source and try again.".to_string(),
            );
        }
        let record = inner
            .record_index_by_path
            .get(&record_path_key(path))
            .and_then(|index| inner.records.get(*index))
            .cloned();
        Ok((record, inner.root.clone()))
    }

    pub(crate) fn records_for_paths(
        &self,
        paths: Vec<PathBuf>,
        corpus_version: u64,
    ) -> Result<Vec<DocumentRecord>, String> {
        let inner = read_checked(&self.inner, CORPUS_STATE_RESOURCE)?;
        if inner.version != CorpusVersion::from_ipc(corpus_version) {
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

pub(crate) fn record_path_key(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}

pub(crate) fn project_root_cache_key(path: Option<&Path>) -> Option<String> {
    path.map(record_path_key)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CorpusLoadResult {
    pub(crate) report: ScanReport,
    pub(crate) corpus_version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use corpuswright_core::corpus::DocumentType;
    use corpuswright_core::preparation::cleaning::PdfOcrQuality;
    use corpuswright_core::project::sources::{
        ProjectSourceExtractionRoute, ProjectSourcePdfProfile,
    };

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

    #[test]
    fn corpus_state_resolves_records_by_path_without_frontend_indices() {
        let first = pdf_record("first.pdf", Some(ProjectSourcePdfProfile::Standard), None);
        let second = pdf_record("second.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None);
        let corpus = CorpusState::new();
        let corpus_version = corpus.load(None, vec![first.clone(), second.clone()]);

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
    fn poisoned_corpus_lock_returns_error_on_read_and_recovers_on_reload() {
        let first = pdf_record("first.pdf", Some(ProjectSourcePdfProfile::Standard), None);
        let second = pdf_record("second.pdf", Some(ProjectSourcePdfProfile::ForceOcr), None);
        let corpus = CorpusState::new();
        let initial_version = corpus.load(Some(PathBuf::from("/stale")), vec![first.clone()]);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = corpus.inner.write().unwrap();
            guard.root = Some(PathBuf::from("/partial"));
            panic!("simulated panic while holding corpus write lock");
        }));
        assert!(corpus.inner.is_poisoned());

        let error = corpus.all_records_and_root(initial_version).unwrap_err();
        assert!(!error.contains(CORPUS_STATE_RESOURCE));
        assert!(error.contains("Clear or reload the corpus"));

        let next_version = corpus.load(None, vec![second.clone()]);
        assert_ne!(next_version, initial_version);
        assert!(!corpus.inner.is_poisoned());
        let (records, root) = corpus.all_records_and_root(next_version).unwrap();
        assert_eq!(records, vec![second]);
        assert!(root.is_none());
        assert_eq!(
            corpus
                .record_for_path(Path::new("first.pdf"), next_version)
                .unwrap(),
            None
        );
    }
}
