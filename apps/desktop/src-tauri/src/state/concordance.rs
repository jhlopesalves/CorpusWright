use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceCollocationCacheKey {
    pub(crate) corpus_version: u64,
    pub(crate) project_root: Option<String>,
    pub(crate) cleaning_config_signature: String,
    pub(crate) selection: AnalysisDocumentSelection,
    pub(crate) aggregation: ConcordanceCollocationAggregationSignature,
}

pub(crate) struct ConcordanceCollocationCache {
    // Last-one cache for the expensive corpus scan. Presentation filters are
    // intentionally excluded from the key so measure/sort/floor changes reuse it.
    pub(crate) inner: Mutex<
        Option<(
            ConcordanceCollocationCacheKey,
            Arc<ConcordanceCollocationAggregate>,
        )>,
    >,
}

impl ConcordanceCollocationCache {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub(crate) fn get(
        &self,
        key: &ConcordanceCollocationCacheKey,
    ) -> Option<Arc<ConcordanceCollocationAggregate>> {
        let inner = self.inner.lock().unwrap();
        inner
            .as_ref()
            .and_then(|(cached_key, aggregate)| (cached_key == key).then(|| Arc::clone(aggregate)))
    }

    pub(crate) fn insert(
        &self,
        key: ConcordanceCollocationCacheKey,
        aggregate: Arc<ConcordanceCollocationAggregate>,
    ) {
        *self.inner.lock().unwrap() = Some((key, aggregate));
    }

    pub(crate) fn clear(&self) {
        *self.inner.lock().unwrap() = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceIndexContextKey {
    pub(crate) corpus_version: u64,
    pub(crate) cleaning_config_signature: String,
}

pub(crate) struct ConcordanceIndexContextEntry {
    pub(crate) key: ConcordanceIndexContextKey,
    pub(crate) context: ConcordanceSearchContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConcordanceDefaultIndexFailureState {
    RetryPending,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceDefaultIndexFailure {
    pub(crate) key: ConcordanceIndexContextKey,
    pub(crate) state: ConcordanceDefaultIndexFailureState,
}

#[derive(Default)]
pub(crate) struct ConcordanceIndexContextCache {
    pub(crate) entry: Option<ConcordanceIndexContextEntry>,
    pub(crate) default_index_failure: Option<ConcordanceDefaultIndexFailure>,
    pub(crate) cache_root: Option<PathBuf>,
    pub(crate) cancel: Mutex<Option<Arc<AtomicBool>>>,
}

pub(crate) type ConcordanceIndexContextHandle = Arc<Mutex<ConcordanceIndexContextCache>>;

pub(crate) struct ConcordanceIndexContextState {
    pub(crate) inner: ConcordanceIndexContextHandle,
}

impl ConcordanceIndexContextState {
    pub(crate) fn new(cache_root: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ConcordanceIndexContextCache {
                entry: None,
                default_index_failure: None,
                cache_root,
                cancel: Mutex::new(None),
            })),
        }
    }

    pub(crate) fn context_handle(&self) -> ConcordanceIndexContextHandle {
        Arc::clone(&self.inner)
    }

    pub(crate) fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        let cache_root = inner.cache_root.clone();
        *inner = ConcordanceIndexContextCache {
            cache_root,
            ..ConcordanceIndexContextCache::default()
        };
    }
}

impl ConcordanceIndexContextCache {
    pub(crate) fn default_index_disabled_for(&self, key: &ConcordanceIndexContextKey) -> bool {
        self.default_index_failure.as_ref().is_some_and(|failure| {
            failure.key == *key && failure.state == ConcordanceDefaultIndexFailureState::Disabled
        })
    }

    pub(crate) fn clear_default_index_failure_for(&mut self, key: &ConcordanceIndexContextKey) {
        if self
            .default_index_failure
            .as_ref()
            .is_some_and(|failure| failure.key == *key)
        {
            self.default_index_failure = None;
        }
    }

    pub(crate) fn record_default_index_error(
        &mut self,
        key: &ConcordanceIndexContextKey,
    ) -> ConcordanceDefaultIndexFailureState {
        if self
            .entry
            .as_ref()
            .is_some_and(|cached_entry| cached_entry.key == *key)
        {
            self.entry = None;
        }

        let next_state = if self.default_index_failure.as_ref().is_some_and(|failure| {
            failure.key == *key
                && failure.state == ConcordanceDefaultIndexFailureState::RetryPending
        }) {
            ConcordanceDefaultIndexFailureState::Disabled
        } else {
            ConcordanceDefaultIndexFailureState::RetryPending
        };
        self.default_index_failure = Some(ConcordanceDefaultIndexFailure {
            key: key.clone(),
            state: next_state.clone(),
        });
        next_state
    }
}
