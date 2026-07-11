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
    ConcordanceCollocateRow, ConcordanceCollocationAggregate,
    ConcordanceCollocationAggregationSignature, ConcordanceCollocationDirection,
    ConcordanceCollocationMeasure, ConcordanceCollocationQuery, ConcordanceCollocationResult,
    ConcordanceCollocationSortKey, RoutedConcordanceCollocationAggregate,
    aggregate_concordance_collocations_documents,
    aggregate_concordance_collocations_documents_routed,
    aggregate_concordance_collocations_routed_provider,
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
    ConcordanceExpansionResult, ConcordanceFallbackReason, ConcordanceQuery, ConcordanceResult,
    ConcordanceRow, ConcordanceSearchContext, ConcordanceSortKey, RoutedConcordanceResult,
    expand_concordance_context, search_concordance_documents, search_concordance_documents_routed,
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
    CorpusOverviewVocabularyGrowth, CumulativeCoveragePoint, FrequencySpectrumPoint,
    RankFrequencyPoint, UniversalType, VocabularyGrowthPoint,
};
#[cfg(test)]
mod test_support;
