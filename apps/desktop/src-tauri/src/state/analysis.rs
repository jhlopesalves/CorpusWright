use crate::*;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AnalysisBuildStateKind {
    NotBuilt,
    Building,
    Ready,
    Stale,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisBuildProgressPayload {
    pub(crate) files_done: usize,
    pub(crate) files_total: usize,
    pub(crate) tokens_seen: u64,
    pub(crate) types_seen: usize,
    pub(crate) current_phase: corpuswright_core::analysis::AnalysisBuildPhase,
    pub(crate) elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisBuildStatus {
    pub(crate) state: AnalysisBuildStateKind,
    pub(crate) progress: Option<AnalysisBuildProgressPayload>,
    pub(crate) error: Option<String>,
    pub(crate) overview: Option<CorpusOverview>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalysisBuildRequest {
    pub(crate) indices: Option<Vec<usize>>,
    pub(crate) corpus_version: u64,
    pub(crate) build_signature: String,
    pub(crate) cleaning_config: CleaningConfig,
}

pub(crate) struct AnalysisStateInner {
    pub(crate) generation: u64,
    pub(crate) state: AnalysisBuildStateKind,
    pub(crate) progress: Option<AnalysisBuildProgressPayload>,
    pub(crate) error: Option<String>,
    pub(crate) corpus: Option<Arc<AnalysisCorpus>>,
}

pub(crate) struct AnalysisState {
    pub(crate) inner: Arc<RwLock<AnalysisStateInner>>,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl AnalysisState {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn corpus(
        &self,
        cleaning_config: &CleaningConfig,
    ) -> Result<Arc<AnalysisCorpus>, String> {
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

    pub(crate) fn reusable_corpus_for_annotation(&self) -> Option<Arc<AnalysisCorpus>> {
        let inner = self.inner.read().unwrap();
        if inner.state == AnalysisBuildStateKind::Ready {
            inner.corpus.clone()
        } else {
            None
        }
    }

    pub(crate) fn clear(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let mut inner = self.inner.write().unwrap();
        inner.generation += 1;
        inner.state = AnalysisBuildStateKind::NotBuilt;
        inner.progress = None;
        inner.error = None;
        inner.corpus = None;
    }

    pub(crate) fn status(&self, cleaning_config: &CleaningConfig) -> AnalysisBuildStatus {
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
