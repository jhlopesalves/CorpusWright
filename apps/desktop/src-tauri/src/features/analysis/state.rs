use crate::runtime::cancellation::ActiveCancellation;
use crate::runtime::identity::AnalysisGeneration;
use crate::runtime::sync::{read_checked, write_reset};
use corpuswright_core::analysis::{AnalysisCorpus, CorpusOverview};
use corpuswright_core::preparation::cleaning::CleaningConfig;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub(crate) const ANALYSIS_STATE_RESOURCE: &str = "analysis state";

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
    pub(crate) generation: AnalysisGeneration,
    pub(crate) state: AnalysisBuildStateKind,
    pub(crate) progress: Option<AnalysisBuildProgressPayload>,
    pub(crate) error: Option<String>,
    pub(crate) corpus: Option<Arc<AnalysisCorpus>>,
}

pub(crate) struct AnalysisState {
    pub(crate) inner: Arc<RwLock<AnalysisStateInner>>,
    pub(crate) cancel: ActiveCancellation,
}

impl AnalysisState {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(AnalysisStateInner {
                generation: AnalysisGeneration::default(),
                state: AnalysisBuildStateKind::NotBuilt,
                progress: None,
                error: None,
                corpus: None,
            })),
            cancel: ActiveCancellation::new(),
        }
    }

    pub(crate) fn corpus(
        &self,
        cleaning_config: &CleaningConfig,
    ) -> Result<Arc<AnalysisCorpus>, String> {
        let corpus = read_checked(&self.inner, ANALYSIS_STATE_RESOURCE)?
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
        let Ok(inner) = read_checked(&self.inner, ANALYSIS_STATE_RESOURCE) else {
            return None;
        };
        if inner.state == AnalysisBuildStateKind::Ready {
            inner.corpus.clone()
        } else {
            None
        }
    }

    pub(crate) fn clear(&self) {
        self.cancel.cancel();
        let mut inner = write_reset(&self.inner, ANALYSIS_STATE_RESOURCE);
        inner.generation.advance();
        inner.state = AnalysisBuildStateKind::NotBuilt;
        inner.progress = None;
        inner.error = None;
        inner.corpus = None;
    }

    pub(crate) fn status(&self, cleaning_config: &CleaningConfig) -> AnalysisBuildStatus {
        let inner = match read_checked(&self.inner, ANALYSIS_STATE_RESOURCE) {
            Ok(inner) => inner,
            Err(error) => {
                return AnalysisBuildStatus {
                    state: AnalysisBuildStateKind::Failed,
                    progress: None,
                    error: Some(error.into_command_error()),
                    overview: None,
                };
            }
        };
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
