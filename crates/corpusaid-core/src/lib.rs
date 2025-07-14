//! Rust core for the CorpusAid desktop application.

pub mod clean;
pub mod html;
pub mod scan;

pub use clean::{CleaningConfig, ReplacementRule, clean_text};
pub use scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanError, ScanReport,
    scan_directory,
};
