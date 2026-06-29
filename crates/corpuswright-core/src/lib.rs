//! Rust core for the CorpusWright desktop application.
//!
//! This crate is UI-agnostic and Tauri-ready. It provides:
//!
//! - **scanning**: recursive directory scanning with metadata collection;
//! - **cleaning**: configurable text cleaning with replacement rules;
//! - **plain-text extraction**: `.txt` file handling;
//! - **DOCX extraction**: main document body extraction from `.docx` files;
//! - **HTML extraction**: plain-text extraction from `.html` and `.htm` files;
//! - **PDF extraction**: embedded-text extraction from born-digital PDFs;
//! - **PDF audit**: lightweight diagnostics for selected PDFs;
//! - **PDF OCR**: optical character recognition for scanned PDFs;
//! - **PDF quality assessment**: quality metrics for PDF pages;
//! - **repeated-artefact detection**: identification of recurring boilerplate;
//! - **preview**: bounded text preview for individual and multiple files;
//! - **search**: text search across corpus documents;
//! - **word counting**: word-level statistics;
//! - **export**: export processed UTF-8 `.txt` files with `manifest.json` and
//!   metadata artefacts.

pub mod analysis;
pub mod annotation;
pub mod annotation_cache;
pub mod cache;
pub mod clean;
pub mod docx;
pub mod export;
pub mod hash;
pub mod html;
pub mod language;
pub mod manifest;
pub mod pdf;
pub mod pdf_audit;
pub mod pdf_ocr;
pub mod pdf_quality;
pub mod preview;
pub mod project;
pub mod project_sources;
pub mod repeated_artifact_promotion;
pub mod repeated_artifacts;
pub mod scan;
pub mod search;
pub mod structured_document;
mod text_normalization;
pub mod text_profile;
pub mod tokenization;
pub mod word_count;

pub use analysis::{
    AnalysisDocumentSelection, AnnotatedFrequencyMode, AnnotatedFrequencyQuery,
    AnnotatedFrequencyResult, AnnotatedTermCount, AnnotatedTermDistributionQuery,
    AnnotatedTermSelector, AnnotationCacheAvailability, AnnotationCacheBuildPhase,
    AnnotationCacheBuildProgress, AnnotationCacheState, AnnotationLayer, AnnotationLayerBuildError,
    AnnotationLayerBuildOptions, AnnotationLayerDocument, ConcordanceDistributionItem,
    ConcordanceDistributionPage, ConcordanceDocument, ConcordanceError, ConcordanceQuery,
    ConcordanceResult, ConcordanceRow, ConcordanceSortKey, FrequencyGroupMode, FrequencyMatchMode,
    FrequencyNotice, FrequencyNoticeKind, FrequencyPage, FrequencyQuery, FrequencySortKey,
    FrequencyTermItem, FrequencyTokenClass, LexFrequencyRow, PosFrequencyRow, SortDirection,
    annotation_cache_availability_for_corpus, build_annotation_layer, query_annotated_frequency,
    query_annotated_term_distribution, search_concordance_documents,
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
    RepeatedArtifactScanConfig, classify_content, no_cancellation, scan_repeated_artifacts,
    scan_repeated_artifacts_with_cancel,
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
