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

pub mod cache;
pub mod clean;
pub mod docx;
pub mod export;
pub mod html;
pub mod manifest;
pub mod pdf;
pub mod pdf_audit;
pub mod pdf_ocr;
pub mod pdf_quality;
pub mod preview;
pub mod repeated_artifacts;
pub mod scan;
pub mod search;
pub mod structured_document;
mod text_normalization;
pub mod word_count;

pub use clean::{
    CleaningConfig, PdfOcrQuality, PdfTextSource, RemovalMatcher, RemovalRule, RemovalRuleSource,
    RemovalScope, ReplacementRule, clean_text,
};
pub use export::{
    ExportError, ExportOptions, ExportReport, ExportWarning, ExportWarningKind, ExportedFileRecord,
    export_corpus,
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
pub use repeated_artifacts::{
    ArtifactRiskLabel, CancellationFlag, CandidateContentClass, CandidateTextProfile,
    CandidateTextSignalLabel, PositionSummary, RepeatedArtifactCandidate, RepeatedArtifactExample,
    RepeatedArtifactKind, RepeatedArtifactScanConfig, classify_content, no_cancellation,
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
