//! Rust core for the CorpusAid desktop application.

pub mod cache;
pub mod clean;
pub mod docx;
pub mod html;
pub mod pdf;
pub mod pdf_ocr;
pub mod pdf_quality;
pub mod preview;
pub mod repeated_artifacts;
pub mod scan;
pub mod search;
pub mod word_count;

pub use clean::{CleaningConfig, ReplacementRule, clean_text};
pub use preview::{
    CombinedPreview, FilePreview, PreviewError, PreviewOptions, PreviewWarning, PreviewWarningKind,
    preview_file, preview_files, preview_processed_files,
};
pub use scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanError, ScanReport,
    scan_directory,
};
pub use search::{SearchHit, SearchResult, search_corpus};
