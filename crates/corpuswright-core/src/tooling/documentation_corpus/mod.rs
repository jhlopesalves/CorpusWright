//! Reproducible qualification helpers for the documentation books corpus.
//!
//! Callers supply catalogue and ebook bytes. Parsing, scoring, cleaning, hashing, duplicate
//! detection, and manifest identity are therefore deterministic in ordinary CI and operate solely
//! on the supplied inputs.

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
