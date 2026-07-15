//! Warm index contexts, indexed search, indexed collocation aggregation, and rebuild state.

use super::super::state::{
    ConcordanceIndexContextEntry, ConcordanceIndexContextHandle, ConcordanceIndexContextKey,
};
use corpuswright_core::analysis::{
    ConcordanceBackend, ConcordanceBackendUsed, ConcordanceCollocationQuery, ConcordanceDocument,
    ConcordanceQuery, ConcordanceSearchContext, RoutedConcordanceCollocationAggregate,
    RoutedConcordanceResult, aggregate_concordance_collocations_routed_provider,
    search_concordance_documents_routed, search_concordance_documents_routed_provider,
};

pub(crate) fn search_concordance_documents_with_warm_index(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
) -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError> {
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache
        .entry
        .as_mut()
        .expect("concordance index context exists after insertion");
    let routed = search_concordance_documents_routed(
        documents,
        query,
        tokenization_config,
        &mut entry.context,
        ConcordanceBackend::Index,
    )?;
    if routed.backend_used == ConcordanceBackendUsed::Index {
        cache.clear_default_index_failure_for(&key);
    }
    Ok(routed)
}

pub(crate) fn search_concordance_with_warm_index_provider<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress),
{
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache.entry.as_mut().expect("inserted above");
    let routed = search_concordance_documents_routed_provider(
        document_slots,
        query,
        tokenization_config,
        &mut entry.context,
        resolve,
        report_progress,
    )?;
    cache.clear_default_index_failure_for(&key);
    Ok(routed)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn aggregate_concordance_collocations_with_warm_index_provider<Resolve, Progress>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    context_state: &ConcordanceIndexContextHandle,
    key: ConcordanceIndexContextKey,
    cancellation: &corpuswright_core::analysis::concordance::index::SurfaceIndexCancellationToken,
    resolve: Resolve,
    report_progress: Progress,
) -> Result<RoutedConcordanceCollocationAggregate, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
    Progress: FnMut(corpuswright_core::analysis::ConcordanceProgress),
{
    let mut cache = context_state.lock().unwrap();
    if cache
        .entry
        .as_ref()
        .is_none_or(|cached_entry| cached_entry.key != key)
    {
        let context = cache
            .cache_root
            .clone()
            .map(ConcordanceSearchContext::with_cache_root)
            .unwrap_or_default();
        cache.entry = Some(ConcordanceIndexContextEntry {
            key: key.clone(),
            context,
        });
    }
    let entry = cache.entry.as_mut().expect("inserted above");
    let routed = aggregate_concordance_collocations_routed_provider(
        document_slots,
        query,
        tokenization_config,
        &mut entry.context,
        cancellation,
        resolve,
        report_progress,
    )?;
    cache.clear_default_index_failure_for(&key);
    Ok(routed)
}
