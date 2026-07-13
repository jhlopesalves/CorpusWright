//! Rust-owned corpus analysis and bounded query APIs.

mod annotation_build;
mod build;
mod collocation;
mod compare;
mod concordance;
mod distribution;
mod frequency;
mod input;
mod overview;
mod overview_distribution;
mod shared;

pub(crate) use build::compare_document_records;
pub use build::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildPhase, AnalysisBuildProgress,
    AnalysisCorpus, AnalysisDocumentSelection, AnalysisDocumentSummary, build_analysis_corpus,
};

pub(crate) use annotation_build::AnnotationBuildTimingCounters;
pub use annotation_build::{
    AnnotatedTermCount, AnnotationAttachError, AnnotationCacheAvailability,
    AnnotationCacheBuildError, AnnotationCacheBuildPhase, AnnotationCacheBuildProgress,
    AnnotationCacheBuildReport, AnnotationCacheState, AnnotationLayer, AnnotationLayerBuildError,
    AnnotationLayerBuildOptions, AnnotationLayerDocument, annotation_cache_availability_for_corpus,
    build_and_write_annotation_cache,
    build_and_write_annotation_cache_with_reusable_surface_corpus, build_annotation_layer,
};

pub use input::{
    AnalysisInputProvenance, AnalysisInputResolution, AnalysisInputSource, AnalysisInputStatus,
    cleaning_config_signature, resolve_analysis_input,
};

pub use collocation::{
    CONCORDANCE_MAX_COMBINED_BYTES_ENV, ConcordanceAnalysisMemoryEstimate,
    ConcordanceAnalysisMemoryLimits, ConcordanceCollocatePositionCounts, ConcordanceCollocateRow,
    ConcordanceCollocationAggregate, ConcordanceCollocationAggregationSignature,
    ConcordanceCollocationDirection, ConcordanceCollocationMeasure, ConcordanceCollocationQuery,
    ConcordanceCollocationResult, ConcordanceCollocationSortKey,
    RoutedConcordanceCollocationAggregate, aggregate_concordance_collocations_documents,
    aggregate_concordance_collocations_routed_provider,
    aggregate_concordance_collocations_scan_provider,
    concordance_collocation_aggregation_signature, query_concordance_collocations_documents,
    query_concordance_collocations_from_aggregate,
};
pub use compare::{
    CompareDocumentCell, CompareDocumentRow, CompareError, CompareExpressionSummary,
    CompareExpressionsQuery, CompareExpressionsResult, compare_expressions,
};
pub(crate) use concordance::concordance_key;
pub use concordance::{
    ConcordanceBackend, ConcordanceBackendUsed, ConcordanceContextFilterDirection,
    ConcordanceDistributionItem, ConcordanceDistributionPage, ConcordanceDocument,
    ConcordanceError, ConcordanceExecutionStats, ConcordanceExpansionQuery,
    ConcordanceExpansionResult, ConcordanceFallbackReason, ConcordancePhase, ConcordanceProgress,
    ConcordanceQuery, ConcordanceResult, ConcordanceRow, ConcordanceSearchContext,
    ConcordanceSortKey, RoutedConcordanceResult, expand_concordance_context,
    search_concordance_documents, search_concordance_documents_routed,
    search_concordance_documents_routed_provider,
    search_concordance_documents_routed_scan_provider,
};

pub use distribution::{
    AnalysisDocumentPage, AnalysisDocumentPageItem, AnnotatedTermDistributionQuery,
    AnnotatedTermSelector, TermDistributionItem, TermDistributionPage, TermDistributionQuery,
    query_analysis_documents, query_annotated_term_distribution, query_term_distribution,
};

pub use frequency::{
    AnnotatedFrequencyMode, AnnotatedFrequencyQuery, AnnotatedFrequencyResult, FrequencyGroupMode,
    FrequencyMatchMode, FrequencyNotice, FrequencyNoticeKind, FrequencyPage, FrequencyQuery,
    FrequencySortKey, FrequencyTermItem, FrequencyTokenClass, LexFrequencyRow, PosFrequencyRow,
    SortDirection, query_annotated_frequency, query_frequency_list,
};

pub use overview::{
    CorpusOverview, CorpusOverviewBalance, CorpusOverviewBuild, CorpusOverviewContext,
    CorpusOverviewDispersion, CorpusOverviewDocument, CorpusOverviewExtractionQuality,
    CorpusOverviewFinding, CorpusOverviewFindingKind, CorpusOverviewFindingSeverity,
    CorpusOverviewFrequencyConcentration, CorpusOverviewLexicalSpectrum, CorpusOverviewPopulation,
    CorpusOverviewRankFrequency, CorpusOverviewReading, CorpusOverviewTotals,
    CorpusOverviewVocabularyGrowth, CoverageTargetCrossing, CumulativeCoveragePoint,
    FrequencySpectrumPoint, RankFrequencyPoint, UniversalType, VocabularyGrowthPoint,
};

pub use overview_distribution::{
    OverviewDistributionBin, OverviewDistributionEvidencePage, OverviewDistributionEvidenceQuery,
    OverviewDistributionEvidenceRow, OverviewDistributionMetric, OverviewDistributionQuery,
    OverviewDistributionResult, OverviewDistributionScale, OverviewDistributionWitness,
    OverviewEcdfPoint, OverviewHistogramRule, OverviewQqPoint, OverviewQqReference,
    query_overview_distribution, query_overview_distribution_evidence,
};

#[cfg(test)]
pub(crate) fn aggregate_concordance_collocations_documents_routed(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
    tokenization_config: &crate::tokenization::TokenizationConfig,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
    cancellation: &crate::index::SurfaceIndexCancellationToken,
) -> Result<RoutedConcordanceCollocationAggregate, ConcordanceError> {
    match backend {
        ConcordanceBackend::Scan => aggregate_concordance_collocations_scan_provider(
            documents.len(),
            query,
            tokenization_config,
            cancellation,
            |document_index| Some(documents[document_index].clone()),
        ),
        ConcordanceBackend::Index => {
            match aggregate_concordance_collocations_routed_provider(
                documents.len(),
                query,
                tokenization_config,
                context,
                cancellation,
                |document_index| Some(documents[document_index].clone()),
                |_| {},
            ) {
                Ok(routed) => Ok(routed),
                Err(error @ ConcordanceError::Index(_))
                    if error.prevents_unbounded_scan_fallback() =>
                {
                    Err(error)
                }
                Err(ConcordanceError::Index(_) | ConcordanceError::InvalidDocumentIdentity) => {
                    let aggregate = aggregate_concordance_collocations_documents(
                        documents,
                        query,
                        tokenization_config,
                        cancellation.clone(),
                    )?;
                    Ok(RoutedConcordanceCollocationAggregate {
                        aggregate,
                        backend_used: ConcordanceBackendUsed::ScanFallback {
                            reason: ConcordanceFallbackReason::IndexError,
                        },
                        index_rebuilt: false,
                    })
                }
                Err(error) => Err(error),
            }
        }
    }
}

#[cfg(test)]
mod test_support;
