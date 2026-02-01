//! Corpus-wide repeated artifact scanner to identify boilerplate, headers, footers, or graphical noise.
//!
//! v1 design:
//! - Default scan uses original extracted text for speed and avoids OCR.
//! - Exact lines, normalised lines, and known inline artefacts are enabled by default.
//! - Numeric-dominant candidates are disabled by default because statistical output can group dangerously.
//! - 2-line/3-line blocks are opt-in because they multiply the candidate space.
//! - Processed-text scans are slower because they apply extraction/cleanup settings.
//! - Uses a two-pass strategy: count first, collect examples only for survivors.
//! - Parallel workers build local maps; merged after per-file processing to avoid global lock contention.

use crate::cache::ExtractionCache;
use crate::clean::CleaningConfig;
use crate::pdf::PdfExtractionOptions;
use crate::scan::{DocumentRecord, DocumentType};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum distinct raw variants tracked per candidate key.
/// Beyond this cap, `raw_variant_overflow` is set to true.
const RAW_VARIANT_TRACK_CAP: usize = 200;

/// Configuration options for the repeated artifact scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepeatedArtifactScanConfig {
    /// Use processed (cleaned) text instead of original extracted text.
    /// Processed scans may be slower because they apply current extraction and cleanup settings.
    pub analyse_processed_text: bool,
    pub include_exact_lines: bool,
    pub include_normalized_lines: bool,
    /// Detect repeated inline markup/conversion tokens (e.g. `<br/>`, `&nbsp;`).
    /// Bounded known-pattern scanning; enabled by default.
    pub include_inline_artifacts: bool,
    /// 2-line block detection is more expensive; disabled by default. Opt-in only.
    pub include_two_line_blocks: bool,
    /// 3-line block detection is more expensive; disabled by default. Opt-in only.
    pub include_three_line_blocks: bool,
    /// Include candidates whose content is predominantly text.
    pub include_text_dominant: bool,
    /// Include candidates with a mix of text and numbers (e.g. "Page 12", "Chapter 5").
    pub include_mixed_text_numbers: bool,
    /// Include numeric-dominant candidates (risky — may group unrelated statistical output).
    pub include_numeric_dominant: bool,
    /// Include symbol/noise-dominant candidates (extraction junk markers).
    pub include_symbol_noise: bool,
    pub min_occurrences: usize,
    pub min_files: usize,
    pub max_candidates: usize,
    pub max_examples_per_candidate: usize,
    pub min_line_chars: usize,
    pub max_line_chars: usize,
}

impl Default for RepeatedArtifactScanConfig {
    fn default() -> Self {
        Self {
            analyse_processed_text: false,
            include_exact_lines: true,
            include_normalized_lines: true,
            include_inline_artifacts: true,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            include_text_dominant: true,
            include_mixed_text_numbers: true,
            include_numeric_dominant: false,
            include_symbol_noise: true,
            min_occurrences: 5,
            min_files: 1,
            max_candidates: 100,
            max_examples_per_candidate: 25,
            min_line_chars: 4,
            max_line_chars: 300,
        }
    }
}

/// The types of candidates that can be detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RepeatedArtifactKind {
    ExactLine,
    NormalizedLine,
    TwoLineBlock,
    ThreeLineBlock,
    /// Repeated inline substrings / markup tokens (e.g. `<br/>`, `&nbsp;`).
    InlineArtifact,
}

/// Risk labels that are advisory reviews rather than absolute truths.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRiskLabel {
    StrongHeaderFooterCandidate,
    PossibleBoilerplate,
    CommonSectionHeadingReviewCarefully,
    SymbolOrNoiseCandidate,
    Ambiguous,
}

/// Content-class label for a candidate, based on character composition.
/// Used to filter candidates and warn users about risky groupings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CandidateContentClass {
    /// Predominantly alphabetic text (prose, headings).
    TextDominant,
    /// Mix of text and numbers (e.g. "Page 12", "Chapter 5").
    MixedTextNumbers,
    /// Predominantly digits and numeric punctuation (e.g. "32.01 46.83").
    NumericDominant,
    /// Predominantly symbols/noise markers (e.g. "● ● ● ●", "------").
    SymbolNoiseDominant,
}

/// Counts of candidate occurrences by estimated layout positions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PositionSummary {
    pub top_count: usize,
    pub middle_count: usize,
    pub bottom_count: usize,
    pub unknown_count: usize,
}

/// A specific example instance of a repeated candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepeatedArtifactExample {
    pub file_name: String,
    pub file_path: String,
    pub line_number: Option<usize>,
    pub page_number: Option<usize>,
    pub context_before: Option<String>,
    pub matched_text: String,
    pub context_after: Option<String>,
}

/// A candidate group returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepeatedArtifactCandidate {
    pub candidate_id: String,
    pub kind: RepeatedArtifactKind,
    pub display_text: String,
    pub normalized_key: String,
    pub occurrence_count: usize,
    pub file_count: usize,
    pub example_count: usize,
    pub position_summary: PositionSummary,
    pub risk_label: ArtifactRiskLabel,
    /// Content classification (text, mixed, numeric, symbol).
    pub content_class: CandidateContentClass,
    /// How many distinct raw text variants appear under this candidate's grouping key.
    /// For normalised candidates this shows how many distinct lines were grouped.
    pub raw_variant_count: usize,
    /// True if the raw_variant_count is capped at RAW_VARIANT_TRACK_CAP and may be higher.
    pub raw_variant_count_is_capped: bool,
    /// The actual distinct raw text variants tracked for this candidate.
    /// For exact-line candidates this contains the single literal string.
    /// For normalised candidates this contains all distinct raw lines that
    /// normalise to the same grouping key (up to RAW_VARIANT_TRACK_CAP).
    pub raw_variants: Vec<String>,
    pub examples: Vec<RepeatedArtifactExample>,
}

/// Diagnostics collected during a repeated artifact scan.
/// Returned alongside candidates in `RepeatedArtifactScanReport`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepeatedArtifactScanDiagnostics {
    pub files_requested: usize,
    pub files_scanned: usize,
    pub files_failed_extraction: usize,
    pub files_empty_after_extraction: usize,
    pub total_raw_lines: usize,
    pub total_candidate_keys_before_filtering: usize,
    pub candidates_after_min_occurrences: usize,
    pub candidates_after_min_files: usize,
    pub final_candidates: usize,
    pub analysed_processed_text: bool,
    pub custom_removals_active: usize,
    pub max_examples_per_candidate: usize,
}

/// Scan report containing both candidates and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepeatedArtifactScanReport {
    pub candidates: Vec<RepeatedArtifactCandidate>,
    pub diagnostics: RepeatedArtifactScanDiagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LinePosition {
    Top,
    Middle,
    Bottom,
}

// ---- Internal structures for the two-pass scan ----

/// Shared cancellation flag type. Wraps an `AtomicBool` behind `Arc`.
/// Set to `true` to request cancellation.
pub type CancellationFlag = std::sync::Arc<AtomicBool>;

/// No-op cancellation flag that never triggers.
pub fn no_cancellation() -> CancellationFlag {
    std::sync::Arc::new(AtomicBool::new(false))
}

/// Per-file aggregated candidate stats (one struct per candidate key per file,
/// NOT one per occurrence).
#[derive(Debug, Clone)]
struct LocalCandidateStats {
    occurrence_count: usize,
    top_count: usize,
    middle_count: usize,
    bottom_count: usize,
    unknown_count: usize,
    normalized_key: String,
    display_text: String,
    /// Bounded example references: (raw_start_idx, raw_end_idx).
    example_refs: Vec<(usize, usize)>,
    /// Distinct raw text variants seen for this key (capped at RAW_VARIANT_TRACK_CAP).
    raw_variants: BTreeSet<String>,
    /// True if more than RAW_VARIANT_TRACK_CAP distinct variants were seen.
    raw_variant_overflow: bool,
}

/// Lightweight stats gathered during the merge phase.
#[derive(Debug, Clone)]
struct CountEntry {
    kind: RepeatedArtifactKind,
    display_text: String,
    normalized_key: String,
    candidate_key: String,
    occurrence_count: usize,
    top_count: usize,
    middle_count: usize,
    bottom_count: usize,
    unknown_count: usize,
    /// Set of file IDs where this candidate appears.
    file_ids: Vec<u32>,
    /// Bounded example references: (file_idx, raw_start_idx, raw_end_idx).
    example_refs: Vec<(u32, usize, usize)>,
    /// Distinct raw variants merged across files (capped at RAW_VARIANT_TRACK_CAP).
    raw_variants: BTreeSet<String>,
    /// True if more than RAW_VARIANT_TRACK_CAP distinct variants exist.
    raw_variant_overflow: bool,
}

/// Structured per-file result from phase 1.
/// Every file produces one of these (no filter_map dropping).
struct Phase1Result {
    ft: FileText,
    local_map: HashMap<(RepeatedArtifactKind, String), LocalCandidateStats>,
    extraction_failed: bool,
}

/// Single file's pre-processed text ready for scanning.
struct FileText {
    file_idx: usize,
    file_name: String,
    file_path: String,
    /// All raw (untrimmed, unvalidated) lines.
    raw_lines: Vec<String>,
    /// Total lines in the file.
    total_lines: usize,
    /// Page boundaries: for each raw line, which page (None for non-PDF).
    page_nums: Vec<Option<usize>>,
    /// Per-line page metadata: (page_number, line_index_within_page, total_lines_on_page).
    /// Always aligned with raw_lines[i].
    page_line_info: Vec<(Option<usize>, usize, usize)>,
}

