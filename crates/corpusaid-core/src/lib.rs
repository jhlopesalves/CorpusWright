//! Rust core for the CorpusAid desktop application.

pub mod scan;

pub use scan::{
    CorpusSummary, DocumentRecord, DocumentType, DocumentTypeCounts, ScanError, ScanReport,
    scan_directory,
};
