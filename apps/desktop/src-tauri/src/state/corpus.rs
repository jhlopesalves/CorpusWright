use crate::*;

pub(crate) struct CorpusStateInner {
    pub(crate) version: u64,
    pub(crate) root: Option<PathBuf>,
    pub(crate) records: Vec<DocumentRecord>,
    pub(crate) record_index_by_path: HashMap<String, usize>,
}

impl CorpusStateInner {
    pub(crate) fn empty() -> Self {
        Self {
            version: 0,
            root: None,
            records: vec![],
            record_index_by_path: HashMap::new(),
        }
    }

    pub(crate) fn load(&mut self, root: Option<PathBuf>, records: Vec<DocumentRecord>) {
        self.version += 1;
        self.root = root;
        self.record_index_by_path = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record_path_key(&record.source_path), index))
            .collect();
        self.records = records;
    }

    pub(crate) fn clear(&mut self) {
        self.version += 1;
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

    pub(crate) fn records_and_root_for_indices(
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

    pub(crate) fn all_records_and_root(
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

    pub(crate) fn records_window(
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

    pub(crate) fn record_for_path(
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

    pub(crate) fn record_for_path_and_root(
        &self,
        path: &Path,
        corpus_version: u64,
    ) -> Result<(Option<DocumentRecord>, Option<PathBuf>), String> {
        let inner = self.inner.read().unwrap();
        if inner.version != corpus_version {
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
