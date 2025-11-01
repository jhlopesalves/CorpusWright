//! Rust core for the CorpusAid desktop application.

pub mod clean;
pub mod docx;
pub mod html;
pub mod pdf_quality;
pub mod scan;

pub use clean::{CleaningConfig, ReplacementRule, clean_text};
pub use scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanError, ScanReport,
    scan_directory,
};
