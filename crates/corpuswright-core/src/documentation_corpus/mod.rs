//! Reproducible qualification helpers for the documentation books corpus.
//!
//! This module deliberately contains no live-network tests. Catalogue bytes and
//! downloaded ebook bytes are supplied by the caller so parsing, scoring,
//! cleaning, hashing, duplicate detection, and manifest identity remain fully
//! deterministic in ordinary CI.

mod catalogue;
mod gutenberg;
mod qualification;

pub use catalogue::{
    CandidateEvaluation, CandidateScore, CandidateScoringInput, GutenbergCatalogRecord,
    parse_gutenberg_catalog, score_candidate,
};
pub use gutenberg::{
    GutenbergCleanError, GutenbergCleanReport, GutenbergCleanResult, GutenbergMarker,
    GutenbergMarkerKind, clean_gutenberg_text,
};
pub use qualification::{
    DuplicateEvidence, RetrievalPolicy, SeedTechnicalAudit, corpus_checksum,
    detect_duplicate_content, is_publishable_status, manifest_paths_are_strictly_ordered,
    sha256_hex, technical_audit,
};
