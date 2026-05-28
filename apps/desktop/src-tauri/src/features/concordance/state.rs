use corpuswright_core::analysis::{
    AnalysisDocumentSelection, ConcordanceCollocationAggregate,
    ConcordanceCollocationAggregationSignature, ConcordanceSearchContext,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

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

#[cfg(test)]
mod tests {
    use super::*;
    use corpuswright_core::analysis::{
        ConcordanceCollocationQuery, ConcordanceCollocationResult,
        concordance_collocation_aggregation_signature,
        query_concordance_collocations_from_aggregate,
    };

    fn collocation_cache_key(
        corpus_version: u64,
        selection: AnalysisDocumentSelection,
        query: &str,
        case_sensitive: bool,
    ) -> ConcordanceCollocationCacheKey {
        ConcordanceCollocationCacheKey {
            corpus_version,
            project_root: Some("project".to_string()),
            cleaning_config_signature: "cleaning-v1".to_string(),
            selection,
            aggregation: ConcordanceCollocationAggregationSignature {
                query: query.to_string(),
                case_sensitive,
            },
        }
    }

    fn collocation_aggregate(
        signature: ConcordanceCollocationAggregationSignature,
    ) -> ConcordanceCollocationAggregate {
        ConcordanceCollocationAggregate {
            signature,
            candidate_counts: std::collections::HashMap::new(),
            collocate_frequencies: vec![],
            symbols: vec![],
            r1: Default::default(),
            n: 0,
            f_node: 0,
        }
    }

    fn desktop_collocation_query(
        query: &str,
        case_sensitive: bool,
        span: usize,
        direction: corpuswright_core::analysis::ConcordanceCollocationDirection,
    ) -> ConcordanceCollocationQuery {
        ConcordanceCollocationQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            case_sensitive,
            span,
            direction,
            min_cooccurrence: 1,
            min_collocate_frequency: 1,
            hide_stopwords: false,
            language: None,
            sort_measure: corpuswright_core::analysis::ConcordanceCollocationMeasure::LogDice,
            sort_key: corpuswright_core::analysis::ConcordanceCollocationSortKey::Association,
            sort_direction: corpuswright_core::analysis::SortDirection::Desc,
            limit: 100,
        }
    }

    fn query_cached_collocations_for_test(
        cache: &ConcordanceCollocationCache,
        query: &ConcordanceCollocationQuery,
        aggregate: &mut impl FnMut(
            ConcordanceCollocationAggregationSignature,
        ) -> ConcordanceCollocationAggregate,
    ) -> ConcordanceCollocationResult {
        let tokenization_config =
            corpuswright_core::text::tokenisation::TokenizationConfig::default();
        let aggregation =
            concordance_collocation_aggregation_signature(query, &tokenization_config).unwrap();
        let cache_key = ConcordanceCollocationCacheKey {
            corpus_version: 7,
            project_root: Some("project".to_string()),
            cleaning_config_signature: "cleaning-v1".to_string(),
            selection: query.selection.clone(),
            aggregation: aggregation.clone(),
        };
        if let Some(aggregate) = cache.get(&cache_key) {
            return query_concordance_collocations_from_aggregate(&aggregate, query);
        }

        let aggregate = Arc::new(aggregate(aggregation));
        cache.insert(cache_key, Arc::clone(&aggregate));
        query_concordance_collocations_from_aggregate(&aggregate, query)
    }

    #[test]
    fn concordance_collocation_cache_is_keyed_by_aggregation_inputs() {
        let cache = ConcordanceCollocationCache::new();
        let key = collocation_cache_key(7, AnalysisDocumentSelection::default(), "movie", false);
        cache.insert(
            key.clone(),
            Arc::new(collocation_aggregate(key.aggregation.clone())),
        );

        assert!(cache.get(&key).is_some());
        let mut other_project = key.clone();
        other_project.project_root = Some("other-project".to_string());
        assert!(cache.get(&other_project).is_none());
        let mut other_cleaning = key.clone();
        other_cleaning.cleaning_config_signature = "cleaning-v2".to_string();
        assert!(cache.get(&other_cleaning).is_none());
        assert!(
            cache
                .get(&collocation_cache_key(
                    8,
                    AnalysisDocumentSelection::default(),
                    "movie",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection {
                        all_documents: false,
                        ids: vec![1],
                        excluded_ids: Vec::new(),
                    },
                    "movie",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection::default(),
                    "film",
                    false,
                ))
                .is_none()
        );
        assert!(
            cache
                .get(&collocation_cache_key(
                    7,
                    AnalysisDocumentSelection::default(),
                    "movie",
                    true,
                ))
                .is_none()
        );
    }

    #[test]
    fn concordance_collocation_cache_shares_and_releases_aggregate_allocations() {
        let cache = ConcordanceCollocationCache::new();
        let key = collocation_cache_key(7, AnalysisDocumentSelection::default(), "movie", false);
        let mut owned = collocation_aggregate(key.aggregation.clone());
        owned.symbols.push("candidate".to_string());
        owned.collocate_frequencies.push(11);
        owned.candidate_counts.insert(0, Default::default());
        let aggregate = Arc::new(owned);
        let symbol_ptr = aggregate.symbols[0].as_ptr();
        cache.insert(key.clone(), Arc::clone(&aggregate));

        let first = cache.get(&key).unwrap();
        let second = cache.get(&key).unwrap();
        assert!(Arc::ptr_eq(&aggregate, &first));
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.symbols[0].as_ptr(), symbol_ptr);
        let strong_count = Arc::strong_count(&aggregate);

        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        let _ = query_concordance_collocations_from_aggregate(&first, &query);
        assert_eq!(Arc::strong_count(&aggregate), strong_count);
        assert_eq!(first.symbols[0].as_ptr(), symbol_ptr);

        drop(first);
        drop(second);
        let weak = Arc::downgrade(&aggregate);
        drop(aggregate);
        let replacement = Arc::new(collocation_aggregate(key.aggregation.clone()));
        cache.insert(key.clone(), replacement);
        assert!(weak.upgrade().is_none());

        let replacement = cache.get(&key).unwrap();
        let replacement_weak = Arc::downgrade(&replacement);
        drop(replacement);
        cache.clear();
        assert!(cache.get(&key).is_none());
        assert!(replacement_weak.upgrade().is_none());
    }

    #[test]
    fn concordance_collocation_cache_hit_reuses_across_span_and_direction() {
        let cache = ConcordanceCollocationCache::new();
        let aggregation_runs = std::cell::Cell::new(0usize);
        let mut aggregate = |signature| {
            aggregation_runs.set(aggregation_runs.get() + 1);
            collocation_aggregate(signature)
        };
        let query = desktop_collocation_query(
            "movie",
            false,
            1,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );

        let first = query_cached_collocations_for_test(&cache, &query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(first.span, 1);
        assert_eq!(
            first.direction,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both
        );

        let mut span_query = query.clone();
        span_query.span = 20;
        let span_change = query_cached_collocations_for_test(&cache, &span_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(span_change.span, 20);

        let mut direction_query = query.clone();
        direction_query.direction =
            corpuswright_core::analysis::ConcordanceCollocationDirection::Left;
        let direction_change =
            query_cached_collocations_for_test(&cache, &direction_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(
            direction_change.direction,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Left
        );

        let mut frequency_query = query.clone();
        frequency_query.min_collocate_frequency = 10;
        frequency_query.min_cooccurrence = 4;
        let frequency_change =
            query_cached_collocations_for_test(&cache, &frequency_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);
        assert_eq!(frequency_change.min_collocate_frequency, 10);
        assert_eq!(frequency_change.min_cooccurrence, 4);
    }

    #[test]
    fn concordance_collocation_cache_misses_for_query_or_case_change() {
        let cache = ConcordanceCollocationCache::new();
        let aggregation_runs = std::cell::Cell::new(0usize);
        let mut aggregate = |signature| {
            aggregation_runs.set(aggregation_runs.get() + 1);
            collocation_aggregate(signature)
        };
        let query = desktop_collocation_query(
            "movie",
            false,
            5,
            corpuswright_core::analysis::ConcordanceCollocationDirection::Both,
        );
        query_cached_collocations_for_test(&cache, &query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 1);

        let mut different_query = query.clone();
        different_query.query = "film".to_string();
        query_cached_collocations_for_test(&cache, &different_query, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 2);

        let mut different_case = query.clone();
        different_case.case_sensitive = true;
        query_cached_collocations_for_test(&cache, &different_case, &mut aggregate);
        assert_eq!(aggregation_runs.get(), 3);
    }
}
