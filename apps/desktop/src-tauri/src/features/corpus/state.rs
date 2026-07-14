//! Stores one loaded corpus generation and validates every command lookup against it.
//!
//! Reload and clear operations increment the version. Asynchronous commands carry that version,
//! which prevents indices and paths from an earlier corpus reaching current state.

use crate::runtime::identity::CorpusVersion;
use corpuswright_core::corpus::{DocumentRecord, ScanReport};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

pub(crate) struct CorpusStateInner {
    pub(crate) version: CorpusVersion,
    pub(crate) root: Option<PathBuf>,
    pub(crate) records: Vec<DocumentRecord>,
    pub(crate) record_index_by_path: HashMap<String, usize>,
}

impl CorpusStateInner {
    pub(crate) fn empty() -> Self {
        Self {
            version: CorpusVersion::default(),
            root: None,
            records: vec![],
            record_index_by_path: HashMap::new(),
        }
    }

    pub(crate) fn load(&mut self, root: Option<PathBuf>, records: Vec<DocumentRecord>) {
        // A new generation invalidates every index selected from the previous record vector.
        self.version.advance();
        self.root = root;
        self.record_index_by_path = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record_path_key(&record.source_path), index))
            .collect();
        self.records = records;
    }

    pub(crate) fn clear(&mut self) {
        // Empty state still receives a new identity so in-flight work cannot match a future load.
        self.version.advance();
        self.root = None;
        self.records.clear();
        self.record_index_by_path.clear();
    }
}

pub(crate) struct CorpusState {
    pub(crate) inner: RwLock<CorpusStateInner>,
}

impl CorpusState {
    pub(crate) fn records_for_indices(
        &self,
        indices: &[usize],
        corpus_version: u64,
    ) -> Result<Vec<DocumentRecord>, String> {
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let inner = self.inner.read().unwrap();
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
        let corpus = CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        };
        corpus
            .inner
            .write()
            .unwrap()
            .load(None, vec![first.clone(), second.clone()]);
        let corpus_version = corpus.inner.read().unwrap().version.into_ipc();

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
}
