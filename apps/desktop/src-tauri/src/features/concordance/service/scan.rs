//! Bounded scan execution adapters.

use corpuswright_core::analysis::{
    ConcordanceCollocationQuery, ConcordanceDocument, ConcordanceQuery,
    RoutedConcordanceCollocationAggregate, RoutedConcordanceResult,
};

pub(crate) fn search_concordance_documents_routed_scan_provider<Resolve>(
    document_slots: usize,
    query: &ConcordanceQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    resolve: Resolve,
) -> Result<RoutedConcordanceResult, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
{
    corpuswright_core::analysis::search_concordance_documents_routed_scan_provider(
        document_slots,
        query,
        tokenization_config,
        resolve,
    )
}

pub(crate) fn aggregate_concordance_collocations_scan_provider<Resolve>(
    document_slots: usize,
    query: &ConcordanceCollocationQuery,
    tokenization_config: &corpuswright_core::text::tokenisation::TokenizationConfig,
    cancellation: &corpuswright_core::CancellationToken,
    resolve: Resolve,
) -> Result<RoutedConcordanceCollocationAggregate, corpuswright_core::analysis::ConcordanceError>
where
    Resolve: FnMut(usize) -> Option<ConcordanceDocument>,
{
    corpuswright_core::analysis::aggregate_concordance_collocations_scan_provider(
        document_slots,
        query,
        tokenization_config,
        cancellation,
        resolve,
    )
}
