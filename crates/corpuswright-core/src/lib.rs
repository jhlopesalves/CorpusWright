//! UI-independent corpus processing and analysis for CorpusWright.
//!
//! The crate owns source extraction, deterministic preparation, project and corpus identity,
//! bounded analytical queries, and reproducible export. Tauri runtime state and frontend
//! presentation remain downstream boundaries. Root re-exports preserve the established API while
//! the domain modules provide the canonical paths for new code.

pub mod analysis;
pub mod annotation;
pub mod corpus;
pub mod export;
pub mod preparation;
pub mod project;
pub mod sources;
pub mod support;
pub mod text;
pub mod tooling;

// These aliases preserve the established module paths while the domain namespaces become the
// canonical architecture. Downstream crates can migrate deliberately without an IPC or API break.
pub use analysis::concordance::index;
pub use annotation::cache as annotation_cache;
pub use corpus as scan;
pub use corpus::{preview, search, word_count};
pub use export::manifest;
pub use preparation::cleaning as clean;
pub use preparation::repeated_artifacts;
pub use preparation::repeated_artifacts::promotion as repeated_artifact_promotion;
pub use project::sources as project_sources;
pub use sources::pdf;
pub use sources::pdf::{audit as pdf_audit, ocr as pdf_ocr, quality as pdf_quality};
pub use sources::{cache, decode, docx, html, structured_document};
pub use support::hash;
pub use text::{language, profile as text_profile, tokenisation as tokenization};
pub use tooling::documentation_corpus;

pub use analysis::{
    AnalysisDocumentSelection, AnnotatedFrequencyMode, AnnotatedFrequencyQuery,
    AnnotatedFrequencyResult, AnnotatedTermCount, AnnotatedTermDistributionQuery,
    AnnotatedTermSelector, AnnotationCacheAvailability, AnnotationCacheBuildPhase,
    AnnotationCacheBuildProgress, AnnotationCacheState, AnnotationLayer, AnnotationLayerBuildError,
    AnnotationLayerBuildOptions, AnnotationLayerDocument, CompareDocumentCell, CompareDocumentRow,
    CompareExpressionSummary, CompareExpressionsQuery, CompareExpressionsResult,
    ConcordanceAnalysisMemoryEstimate, ConcordanceAnalysisMemoryLimits,
    ConcordanceCollocatePositionCounts, ConcordanceCollocateRow, ConcordanceCollocationAggregate,
    ConcordanceCollocationAggregationSignature, ConcordanceCollocationDirection,
    ConcordanceCollocationMeasure, ConcordanceCollocationQuery, ConcordanceCollocationResult,
    ConcordanceCollocationSortKey, ConcordanceContextFilterDirection, ConcordanceDistributionItem,
    ConcordanceDistributionPage, ConcordanceDocument, ConcordanceError, ConcordanceExpansionQuery,
    ConcordanceExpansionResult, ConcordanceQuery, ConcordanceResult, ConcordanceRow,
    ConcordanceSortKey, FrequencyGroupMode, FrequencyMatchMode, FrequencyNotice,
    FrequencyNoticeKind, FrequencyPage, FrequencyQuery, FrequencySortKey, FrequencyTermItem,
    FrequencyTokenClass, LexFrequencyRow, PosFrequencyRow, SortDirection,
    aggregate_concordance_collocations_documents, annotation_cache_availability_for_corpus,
    build_annotation_layer, compare_expressions, concordance_collocation_aggregation_signature,
    expand_concordance_context, query_annotated_frequency, query_annotated_term_distribution,
    query_concordance_collocations_documents, query_concordance_collocations_from_aggregate,
    search_concordance_documents,
};
pub use annotation_cache::{
    ANNOTATION_LAYER_CACHE_FILE_VERSION, ANNOTATION_LAYER_SCHEMA_VERSION, AnnotationCorpusIdentity,
    AnnotationDocumentIdentity, AnnotationDocumentLanguage, AnnotationLanguageIdentity,
    AnnotationLanguageOverrideInput, AnnotationLayerCacheError, AnnotationLayerCacheLoad,
    AnnotationManifest, AnnotationModelIdentity, build_annotation_manifest,
    load_annotation_layer_cache, write_annotation_layer_cache,
};
pub use clean::{
    CleaningConfig, PdfOcrQuality, PdfTextSource, RemovalMatcher, RemovalRule, RemovalRuleSource,
    RemovalScope, ReplacementRule, clean_text,
};
pub use decode::{
    DECODE_POLICY_VERSION, DecodeStrategy, DecodedText, SourceEncoding, decode_source_bytes,
};
pub use export::{
    ExportError, ExportOptions, ExportReport, ExportWarning, ExportWarningKind, ExportedFileRecord,
    export_corpus,
};
pub use language::{
    DEFAULT_LOW_CONFIDENCE_THRESHOLD, DetectedLanguage, DocumentLanguage, LanguageResolution,
    ProjectLanguage, detect_and_resolve, detect_language, resolve_effective_document_languages,
    resolve_language,
};
pub use manifest::{ExportManifest, ManifestFileRecord};
pub use pdf::{
    PdfPageExtractionMethod, PdfPageRangePage, PdfPageRangeResult, extract_pdf_page_range,
};
pub use pdf_audit::{PdfAuditQuality, PdfAuditResult, PdfAuditSuggestedProfile, audit_pdf_files};
pub use preview::{
    CombinedPreview, FilePreview, PreviewError, PreviewOptions, PreviewWarning, PreviewWarningKind,
    preview_file, preview_files, preview_processed_files,
};
pub use project::{ActiveProject, CreateCorpusProjectRequest};
pub use project_sources::{
    ProjectSource, ProjectSourceExtractionResult, ProjectSourceExtractionRoute,
    ProjectSourceImportPolicy, ProjectSourceImportReview, ProjectSourceKind,
    ProjectSourceKindCounts, ProjectSourceManifest, ProjectSourcePage, ProjectSourcePreviewItem,
    ProjectSourcePreviewViewport, ProjectSourceProcessingResult, ProjectSourceQuery,
    ProjectSourceSelectionDescriptor, ProjectSourceSelectionViewport, ProjectSourceStatus,
    ProjectSourceStatusCounts, ProjectSourceSummary, ProjectSourceUpdate, add_project_source_paths,
    add_project_source_paths_with_policy, add_project_source_paths_with_progress, kind_for_path,
    load_project_source_manifest, project_source_matches_query, query_project_source_manifest,
    query_project_source_selection, review_project_source_paths, save_project_source_manifest,
    update_project_source_extraction_results, update_project_source_processing_results,
};
pub use repeated_artifact_promotion::{
    RepeatedArtifactMatcherKind, RepeatedArtifactPromotion, can_promote, candidate_matcher_kind,
    candidate_rule_text, candidate_scope, promote_candidate, promotion_for_candidate, rule_id,
};
pub use repeated_artifacts::{
    ArtifactRiskLabel, CancellationFlag, CandidateContentClass, PositionSummary,
    RepeatedArtifactCandidate, RepeatedArtifactExample, RepeatedArtifactKind,
    RepeatedArtifactMemoryStage, RepeatedArtifactScanConfig, RepeatedArtifactScanError,
    RepeatedArtifactScanPhase, RepeatedArtifactScanProgress, classify_content, no_cancellation,
    scan_repeated_artifacts, scan_repeated_artifacts_with_cancel,
};
pub use scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanError, ScanReport,
    scan_directory,
};
pub use search::{SearchHit, SearchResult, search_corpus};
pub use structured_document::{
    DocumentLine, DocumentPage, PAGE_EDGE_LINE_COUNT, StructuredDocument,
};
pub use text_profile::{CandidateTextProfile, CandidateTextSignalLabel};
pub use tokenization::{
    TOKENIZER_ID, TOKENIZER_VERSION, Token, TokenKind, TokenizationConfig, TokenizationManifest,
    UnicodeNormalizationForm, tokenize,
};
