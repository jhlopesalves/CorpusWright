//! Corpus-wide repeated artefact scanner for boilerplate, headers, footers, and graphical noise.
//!
//! - Default scan uses original extracted text for speed and avoids OCR.
//! - Exact lines, normalised lines, and known inline artefacts are enabled by default.
//! - Numeric-dominant candidates are disabled by default because statistical output can group dangerously.
//! - 2-line/3-line blocks are opt-in because they multiply the candidate space.
//! - Processed-text scans are slower because they apply extraction/cleanup settings.
//! - Uses a two-pass strategy: count first, collect examples only for survivors.
//! - Parallel workers build local maps; merged after per-file processing to avoid global lock contention.

use crate::cache::{DocumentTextMode, ExtractionCache, document_text_for_record};
use crate::clean::CleaningConfig;
use crate::scan::DocumentRecord;
use crate::structured_document::StructuredDocument;
use crate::text_normalization::normalize_line_for_repeated_artifact as normalize_line;
use crate::text_profile::{
    KNOWN_INLINE_PATTERNS, TextSignalContext, classify_text_signal, is_common_section_heading,
    is_decimal_digit, is_page_label_like, profile_text_line, text_signal_reasons,
};
use aho_corasick::AhoCorasick;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, Ordering},
};
use ts_rs::TS;

pub use crate::text_profile::{CandidateTextProfile, CandidateTextSignalLabel};

/// Maximum distinct raw variants tracked per candidate key.
/// Beyond this cap, `raw_variant_overflow` is set to true.
const RAW_VARIANT_TRACK_CAP: usize = 200;

/// Configuration options for the repeated artefact scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RepeatedArtifactKind {
    ExactLine,
    NormalizedLine,
    TwoLineBlock,
    ThreeLineBlock,
    /// Repeated inline substrings / markup tokens (e.g. `<br/>`, `&nbsp;`).
    InlineArtifact,
}

/// Risk labels that are advisory reviews rather than absolute truths.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ArtifactRiskLabel {
    StrongHeaderFooterCandidate,
    PossibleBoilerplate,
    CommonSectionHeadingReviewCarefully,
    SymbolOrNoiseCandidate,
    Ambiguous,
}

/// Content-class label for a candidate, based on character composition.
/// Used to filter candidates and warn users about risky groupings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CandidateContentClass {
    /// Predominantly alphabetic text (prose, headings).
    TextDominant,
    /// Mix of text and numbers (e.g. "Page 12", "Chapter 5").
    MixedTextNumbers,
    /// Predominantly digits and numeric punctuation (e.g. "32.01 46.83").
    NumericDominant,
    /// Predominantly symbols/noise markers (e.g. "****", "------").
    SymbolNoiseDominant,
}

/// Counts of candidate occurrences by estimated layout positions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PositionSummary {
    pub top_count: usize,
    pub middle_count: usize,
    pub bottom_count: usize,
    pub unknown_count: usize,
}

/// A specific example instance of a repeated candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RepeatedArtifactCandidate {
    pub candidate_id: String,
    pub kind: RepeatedArtifactKind,
    pub display_text: String,
    pub normalized_key: String,
    pub occurrence_count: usize,
    pub file_count: usize,
    pub example_count: usize,
    pub position_summary: PositionSummary,
    pub position_summary_is_page_based: bool,
    pub risk_label: ArtifactRiskLabel,
    /// Content classification (text, mixed, numeric, symbol).
    pub content_class: CandidateContentClass,
    /// Deterministic advisory text/noise profile used during candidate review.
    #[serde(default)]
    pub text_profile: CandidateTextProfile,
    /// Compact advisory label derived from the text profile and existing evidence.
    #[serde(default)]
    pub text_signal_label: CandidateTextSignalLabel,
    /// Stable reason codes explaining the advisory text signal.
    #[serde(default)]
    pub text_signal_reasons: Vec<String>,
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

/// Diagnostics collected during a repeated artefact scan.
/// Returned alongside candidates in `RepeatedArtifactScanReport`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
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

/// Shared cancellation flag cloned into parallel scan workers.
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
    position_summary_is_page_based: bool,
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
    position_summary_is_page_based: bool,
}

/// Structured per-file result from the parallel aggregation pass.
/// Failed extraction still produces one of these so diagnostics can account for the file.
struct Phase1Result {
    ft: FileText,
    local_map: HashMap<(RepeatedArtifactKind, String), LocalCandidateStats>,
    extraction_failed: bool,
    fatal_error: Option<String>,
}

/// Single file's pre-processed text ready for scanning.
#[derive(Debug, Clone)]
struct FileText {
    file_name: String,
    file_path: String,
    /// All raw (untrimmed, unvalidated) lines.
    raw_lines: Vec<String>,
    /// Total lines in the file.
    total_lines: usize,
    /// Page and page-local line metadata used when the source has reliable page chunks.
    document: StructuredDocument,
    /// Flat-line index to structured page/line location.
    line_locations: Vec<(usize, usize)>,
    /// Non-PDF flat text uses `document` for line storage but not as reliable page metadata.
    has_page_metadata: bool,
}

impl FileText {
    fn document_line(&self, line_idx: usize) -> Option<&crate::structured_document::DocumentLine> {
        let (page_idx, line_idx_in_page) = *self.line_locations.get(line_idx)?;
        self.document
            .pages
            .get(page_idx)?
            .lines
            .get(line_idx_in_page)
    }

    fn page_number_for_line(&self, line_idx: usize) -> Option<usize> {
        if !self.has_page_metadata {
            return None;
        }

        self.document_line(line_idx).map(|line| line.page_index + 1)
    }

    fn lines_share_page(&self, first_line_idx: usize, second_line_idx: usize) -> bool {
        if !self.has_page_metadata {
            return true;
        }

        match (
            self.document_line(first_line_idx),
            self.document_line(second_line_idx),
        ) {
            (Some(first), Some(second)) => first.page_index == second.page_index,
            _ => false,
        }
    }
}

/// Returns true if the line should be skipped before any candidate processing.
/// Filters: empty, whitespace-only, too short, too long.
#[inline]
fn should_skip_line(text: &str, min_chars: usize, max_chars: usize) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let char_count = trimmed.chars().count();
    char_count < min_chars || char_count > max_chars
}

/// Computes the layout position of a line using structured page metadata when available.
/// Short-page lines can be both top and bottom in the shared model; the scanner
/// keeps its existing single-bucket, top-first summary behaviour.
fn compute_position_for_line(ft: &FileText, line_idx: usize) -> LinePosition {
    if ft.has_page_metadata
        && let Some(line) = ft.document_line(line_idx)
    {
        if line.is_page_top {
            return LinePosition::Top;
        } else if line.is_page_bottom {
            return LinePosition::Bottom;
        } else {
            return LinePosition::Middle;
        }
    }

    let total_lines = ft.total_lines;
    let ten_percent = (total_lines / 10).max(1);
    if line_idx < ten_percent {
        LinePosition::Top
    } else if line_idx >= total_lines.saturating_sub(ten_percent) {
        LinePosition::Bottom
    } else {
        LinePosition::Middle
    }
}

/// Generates a stable, deterministic ID for a candidate.
fn compute_stable_id(kind: &RepeatedArtifactKind, key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let hash_str = format!("{:x}", hasher.finalize());
    let kind_str = match kind {
        RepeatedArtifactKind::ExactLine => "exact",
        RepeatedArtifactKind::NormalizedLine => "norm",
        RepeatedArtifactKind::TwoLineBlock => "block2",
        RepeatedArtifactKind::ThreeLineBlock => "block3",
        RepeatedArtifactKind::InlineArtifact => "inline",
    };
    format!("{}_{:.16}", kind_str, hash_str)
}

/// Safely extracts up to `max_chars` characters before `byte_pos` in `s`.
/// Walks back to the nearest char boundary, then takes up to `max_chars` chars.
/// Prefixes with `...` and trims leading whitespace.
fn safe_context_before(s: &str, byte_pos: usize, max_chars: usize) -> Option<String> {
    if byte_pos == 0 {
        return None;
    }
    // Walk back to a valid char boundary
    let end = (0..=byte_pos).rev().find(|&i| s.is_char_boundary(i))?;
    if end == 0 {
        return None;
    }
    let before = &s[..end];
    let context: String = before
        .chars()
        .rev()
        .take(max_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let trimmed = context.trim_start().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("...{}", trimmed))
    }
}

/// Safely extracts up to `max_chars` characters after `byte_pos + pattern_len` in `s`.
/// Checks `char_boundary` before slicing to avoid panic on multi-byte UTF-8.
/// Suffixes with `...` and trims trailing whitespace.
fn safe_context_after(
    s: &str,
    byte_pos: usize,
    pattern_len: usize,
    max_chars: usize,
) -> Option<String> {
    let start = byte_pos.checked_add(pattern_len)?;
    if start >= s.len() || !s.is_char_boundary(start) {
        return None;
    }
    let after = &s[start..];
    let context: String = after.chars().take(max_chars).collect();
    let trimmed = context.trim_end().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("{}...", trimmed))
    }
}

/// Computes the suspicion score used to rank candidates.
fn calculate_suspicion_score(
    kind: &RepeatedArtifactKind,
    display_text: &str,
    occurrence_count: usize,
    file_count: usize,
    top_count: usize,
    bottom_count: usize,
) -> f64 {
    let occ_factor = (occurrence_count as f64).ln_1p();
    let file_factor = (file_count as f64).ln_1p();

    let total_pos = (top_count + bottom_count) as f64;
    let pos_ratio = if occurrence_count > 0 {
        total_pos / occurrence_count as f64
    } else {
        0.0
    };
    let position_bonus = 1.0 + pos_ratio * 2.0;

    let block_multiplier = match kind {
        RepeatedArtifactKind::ExactLine => 1.0,
        RepeatedArtifactKind::NormalizedLine => 1.0,
        RepeatedArtifactKind::TwoLineBlock => 1.5,
        RepeatedArtifactKind::ThreeLineBlock => 2.0,
        RepeatedArtifactKind::InlineArtifact => 1.0,
    };

    let char_len = display_text.chars().count();
    let len_factor = if char_len <= 3 {
        0.2
    } else if char_len <= 8 {
        0.6
    } else {
        1.0
    };

    let non_alphanumeric_count = display_text
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    let symbol_ratio = if char_len > 0 {
        non_alphanumeric_count as f64 / char_len as f64
    } else {
        0.0
    };
    let symbol_bonus = 1.0 + symbol_ratio * 1.5;

    occ_factor * file_factor * position_bonus * block_multiplier * len_factor * symbol_bonus
}

/// Classifies a candidate's risk label.
fn classify_risk(
    display_text: &str,
    normalized_key: &str,
    occurrence_count: usize,
    file_count: usize,
    top_count: usize,
    bottom_count: usize,
) -> ArtifactRiskLabel {
    let lower_key = normalized_key.trim().to_lowercase();

    if is_common_section_heading(&lower_key) {
        return ArtifactRiskLabel::CommonSectionHeadingReviewCarefully;
    }

    let char_len = display_text.chars().count();
    let non_alphanumeric_count = display_text
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    let symbol_ratio = if char_len > 0 {
        non_alphanumeric_count as f64 / char_len as f64
    } else {
        0.0
    };
    if symbol_ratio >= 0.50 {
        return ArtifactRiskLabel::SymbolOrNoiseCandidate;
    }

    let pos_ratio = if occurrence_count > 0 {
        (top_count + bottom_count) as f64 / occurrence_count as f64
    } else {
        0.0
    };
    if occurrence_count >= 3 && pos_ratio >= 0.75 {
        return ArtifactRiskLabel::StrongHeaderFooterCandidate;
    }

    if is_page_label_like(&lower_key) && occurrence_count >= 3 {
        if pos_ratio >= 0.50 {
            return ArtifactRiskLabel::StrongHeaderFooterCandidate;
        }
        return ArtifactRiskLabel::PossibleBoilerplate;
    }

    if file_count >= 2 {
        return ArtifactRiskLabel::PossibleBoilerplate;
    }

    ArtifactRiskLabel::Ambiguous
}

/// Classifies a text line's content into one of four categories.
///
/// Rules are applied in order: at least 50% symbols, at least 60% digits,
/// mixed text and numbers, digit-dominant, then text-dominant.
pub fn classify_content(text: &str) -> CandidateContentClass {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return CandidateContentClass::TextDominant;
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let total = chars.len() as f64;
    if total == 0.0 {
        return CandidateContentClass::TextDominant;
    }

    let alpha_count = chars.iter().filter(|c| c.is_alphabetic()).count() as f64;
    let digit_count = chars.iter().filter(|c| is_decimal_digit(**c)).count() as f64;
    let symbol_count = chars
        .iter()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count() as f64;

    let symbol_ratio = symbol_count / total;
    let digit_ratio = digit_count / total;
    let alpha_ratio = alpha_count / total;

    if symbol_ratio >= 0.50 {
        return CandidateContentClass::SymbolNoiseDominant;
    }

    if digit_ratio >= 0.60 && digit_count > 0.0 {
        return CandidateContentClass::NumericDominant;
    }

    if alpha_ratio > 0.0 && digit_count > 0.0 && alpha_ratio >= digit_ratio {
        return CandidateContentClass::MixedTextNumbers;
    }

    if digit_count > alpha_count && digit_ratio > 0.0 {
        return CandidateContentClass::NumericDominant;
    }

    CandidateContentClass::TextDominant
}

fn profile_candidate_text(display_text: &str, normalized_key: &str) -> CandidateTextProfile {
    profile_text_line(display_text, normalized_key)
}

fn classify_candidate_text_signal(
    profile: &CandidateTextProfile,
    content_class: CandidateContentClass,
    risk_label: ArtifactRiskLabel,
    position_summary: &PositionSummary,
) -> CandidateTextSignalLabel {
    classify_text_signal(
        profile,
        TextSignalContext {
            content_is_text_dominant: content_class == CandidateContentClass::TextDominant,
            has_common_section_heading_risk: risk_label
                == ArtifactRiskLabel::CommonSectionHeadingReviewCarefully,
            has_symbol_or_noise_risk: risk_label == ArtifactRiskLabel::SymbolOrNoiseCandidate,
            has_strong_header_footer_risk: risk_label
                == ArtifactRiskLabel::StrongHeaderFooterCandidate,
            page_edge_ratio: candidate_page_edge_ratio(position_summary),
        },
    )
}

fn candidate_text_signal_reasons(
    display_text: &str,
    profile: &CandidateTextProfile,
    label: CandidateTextSignalLabel,
    position_summary: &PositionSummary,
) -> Vec<String> {
    text_signal_reasons(
        display_text,
        profile,
        label,
        candidate_page_edge_ratio(position_summary),
    )
}

fn candidate_page_edge_ratio(position_summary: &PositionSummary) -> f64 {
    let known_total =
        position_summary.top_count + position_summary.middle_count + position_summary.bottom_count;
    if known_total == 0 {
        0.0
    } else {
        (position_summary.top_count + position_summary.bottom_count) as f64 / known_total as f64
    }
}

/// Returns true if a candidate with the given content class should be included
/// based on the scan config filters.
fn should_include_content_class(
    cc: CandidateContentClass,
    config: &RepeatedArtifactScanConfig,
) -> bool {
    match cc {
        CandidateContentClass::TextDominant => config.include_text_dominant,
        CandidateContentClass::MixedTextNumbers => config.include_mixed_text_numbers,
        CandidateContentClass::NumericDominant => config.include_numeric_dominant,
        CandidateContentClass::SymbolNoiseDominant => config.include_symbol_noise,
    }
}

/// Returns the dominant content class for a 2-line block.
/// If either line is NumericDominant, the block is NumericDominant.
/// If either is SymbolNoiseDominant, the block is SymbolNoiseDominant.
/// Otherwise prefer MixedTextNumbers over TextDominant when there's mixing.
fn dominant_content_class(
    a: CandidateContentClass,
    b: CandidateContentClass,
) -> CandidateContentClass {
    use CandidateContentClass::*;
    if a == NumericDominant || b == NumericDominant {
        return NumericDominant;
    }
    if a == SymbolNoiseDominant || b == SymbolNoiseDominant {
        return SymbolNoiseDominant;
    }
    if a == MixedTextNumbers || b == MixedTextNumbers {
        return MixedTextNumbers;
    }
    TextDominant
}

fn dominant_content_class_3(
    a: CandidateContentClass,
    b: CandidateContentClass,
    c: CandidateContentClass,
) -> CandidateContentClass {
    use CandidateContentClass::*;
    if a == NumericDominant || b == NumericDominant || c == NumericDominant {
        return NumericDominant;
    }
    if a == SymbolNoiseDominant || b == SymbolNoiseDominant || c == SymbolNoiseDominant {
        return SymbolNoiseDominant;
    }
    if a == MixedTextNumbers || b == MixedTextNumbers || c == MixedTextNumbers {
        return MixedTextNumbers;
    }
    TextDominant
}

static KNOWN_INLINE_AUTOMATON: LazyLock<AhoCorasick> =
    LazyLock::new(|| AhoCorasick::new(KNOWN_INLINE_PATTERNS).unwrap());

/// Detects inline artefacts in the given file text.
/// Returns a per-file local aggregated map.
fn detect_inline_artifacts(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
) -> HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> {
    if !config.include_inline_artifacts {
        return HashMap::new();
    }

    detect_inline_artifacts_with_automaton(ft, config, &KNOWN_INLINE_AUTOMATON)
}

fn detect_inline_artifacts_with_automaton(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
    automaton: &AhoCorasick,
) -> HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> {
    let mut map: HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> = HashMap::new();

    for (line_idx, line) in ft.raw_lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }

        for mat in automaton.find_iter(line) {
            let pattern = KNOWN_INLINE_PATTERNS[mat.pattern().as_usize()];
            let key = (RepeatedArtifactKind::InlineArtifact, pattern.to_string());
            let entry = map.entry(key).or_insert_with(|| LocalCandidateStats {
                occurrence_count: 0,
                top_count: 0,
                middle_count: 0,
                bottom_count: 0,
                unknown_count: 0,
                normalized_key: String::new(),
                display_text: pattern.to_string(),
                example_refs: Vec::new(),
                raw_variants: BTreeSet::from([pattern.to_string()]),
                raw_variant_overflow: false,
                position_summary_is_page_based: ft.has_page_metadata,
            });

            entry.occurrence_count += 1;
            let pos_class = compute_position_for_line(ft, line_idx);
            match pos_class {
                LinePosition::Top => entry.top_count += 1,
                LinePosition::Middle => entry.middle_count += 1,
                LinePosition::Bottom => entry.bottom_count += 1,
            }

            if entry.example_refs.len() < config.max_examples_per_candidate {
                // Inline examples use the second slot as a byte offset.
                entry.example_refs.push((line_idx, mat.start()));
            }
        }
    }

    map
}

/// Internal struct describing extraction status for a single file.
struct ExtractedScanText {
    text: String,
    page_texts: Option<Vec<String>>,
    extraction_failed: bool,
    fatal_error: Option<String>,
}

/// Extracts text and records whether extraction failed (vs. succeeded with possibly empty text).
///
/// If a cache is provided, uses the shared document text helper so repeated
/// artefact scans see the same materialised OCR text as preview, search, word
/// count, and export.
fn get_document_text_with_status(
    record: &DocumentRecord,
    analyse_processed_text: bool,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> ExtractedScanText {
    let mode = if analyse_processed_text {
        DocumentTextMode::Processed
    } else {
        DocumentTextMode::Original
    };

    match document_text_for_record(record, cleaning_config, mode, cache) {
        Ok(document_text) => ExtractedScanText {
            text: document_text.text,
            page_texts: document_text.page_texts,
            extraction_failed: false,
            fatal_error: None,
        },
        Err(error) => {
            let fatal_error = if error.contains("OCR text is not materialised") {
                Some(error)
            } else {
                None
            };
            ExtractedScanText {
                text: String::new(),
                page_texts: None,
                extraction_failed: true,
                fatal_error,
            }
        }
    }
}

/// Build a per-file `FileText` from extracted text and optional page metadata.
fn build_file_text(record: &DocumentRecord, text: &str, page_texts: Option<&[String]>) -> FileText {
    let file_name = record
        .source_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    let file_path = record.relative_path.to_string_lossy().into_owned();

    let document = if let Some(page_texts) = page_texts {
        StructuredDocument::from_pages(page_texts)
    } else {
        StructuredDocument::from_flat_text_as_single_page(text)
    };
    let raw_lines = document
        .iter_lines()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>();
    let line_locations = document
        .pages
        .iter()
        .flat_map(|page| {
            page.lines
                .iter()
                .map(|line| (page.page_index, line.line_index_in_page))
        })
        .collect::<Vec<_>>();
    let total_lines = raw_lines.len();

    FileText {
        file_name,
        file_path,
        raw_lines,
        total_lines,
        document,
        line_locations,
        has_page_metadata: page_texts.is_some(),
    }
}

/// Builds the per-file scan result used by the parallel aggregation pass.
///
/// A result is returned even when extraction fails so diagnostics stay aligned
/// with the requested records.
fn phase1_scan_file(
    record: &DocumentRecord,
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    cancel: &CancellationFlag,
) -> Phase1Result {
    if cancel.load(Ordering::Relaxed) {
        return Phase1Result {
            ft: build_file_text(record, "", None),
            local_map: HashMap::new(),
            extraction_failed: false,
            fatal_error: None,
        };
    }

    let extracted = get_document_text_with_status(
        record,
        config.analyse_processed_text,
        cleaning_config,
        cache,
    );

    if cancel.load(Ordering::Relaxed) {
        return Phase1Result {
            ft: build_file_text(record, "", None),
            local_map: HashMap::new(),
            extraction_failed: false,
            fatal_error: None,
        };
    }

    let ft = build_file_text(record, &extracted.text, extracted.page_texts.as_deref());

    let mut map = phase1_aggregate(&ft, config);

    let inline_map = detect_inline_artifacts(&ft, config);
    for ((kind, key), inline_stats) in inline_map {
        map.insert((kind, key), inline_stats);
    }

    Phase1Result {
        ft,
        local_map: map,
        extraction_failed: extracted.extraction_failed,
        fatal_error: extracted.fatal_error,
    }
}

/// Aggregate occurrences into a local stats map (one entry per key, not per occurrence).
fn phase1_aggregate(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
) -> HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> {
    let mut map: HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> = HashMap::new();
    let total = ft.total_lines;
    let min_c = config.min_line_chars;
    let max_c = config.max_line_chars;

    if total == 0 {
        return map;
    }

    // Normalising once per line avoids repeated work across candidate paths.
    let mut valid_entries: Vec<(
        usize,
        String,
        /* norm */ String,
        /* content_class */ CandidateContentClass,
    )> = Vec::new();
    for idx in 0..total {
        let raw = ft.raw_lines[idx].trim();
        if should_skip_line(raw, min_c, max_c) {
            continue;
        }
        let norm = normalize_line(raw);
        let cc = classify_content(raw);
        valid_entries.push((idx, raw.to_string(), norm, cc));
    }

    let mut upsert = |kind: RepeatedArtifactKind,
                      key: String,
                      norm_key: String,
                      display: String,
                      _cc: CandidateContentClass,
                      pos: LinePosition,
                      rs: usize,
                      re: usize| {
        let map_key = (kind, key);
        let entry = map.entry(map_key).or_insert_with(|| LocalCandidateStats {
            occurrence_count: 0,
            top_count: 0,
            middle_count: 0,
            bottom_count: 0,
            unknown_count: 0,
            normalized_key: norm_key,
            display_text: String::new(),
            example_refs: Vec::new(),
            raw_variants: BTreeSet::new(),
            raw_variant_overflow: false,
            position_summary_is_page_based: ft.has_page_metadata,
        });
        entry.occurrence_count += 1;
        match pos {
            LinePosition::Top => entry.top_count += 1,
            LinePosition::Middle => entry.middle_count += 1,
            LinePosition::Bottom => entry.bottom_count += 1,
        }
        if entry.display_text.is_empty() {
            entry.display_text = display.clone();
        }
        if entry.example_refs.len() < config.max_examples_per_candidate {
            entry.example_refs.push((rs, re));
        }
        // Raw variants are capped to keep large normalised groups bounded.
        if entry.raw_variants.len() < RAW_VARIANT_TRACK_CAP {
            entry.raw_variants.insert(display.clone());
        } else if !entry.raw_variants.contains(&display) {
            entry.raw_variant_overflow = true;
        }
    };

    if config.include_exact_lines {
        for &(idx, ref line, ref norm, cc) in &valid_entries {
            if !should_include_content_class(cc, config) {
                continue;
            }
            let pos = compute_position_for_line(ft, idx);
            upsert(
                RepeatedArtifactKind::ExactLine,
                line.clone(),
                norm.clone(),
                line.clone(),
                cc,
                pos,
                idx,
                idx,
            );
        }
    }

    // SAFETY: NumericDominant lines are NOT used for normalised grouping
    // unless explicitly enabled (include_numeric_dominant).
    if config.include_normalized_lines {
        for &(idx, ref line, ref norm, cc) in &valid_entries {
            if norm.is_empty() {
                continue;
            }
            if cc == CandidateContentClass::NumericDominant && !config.include_numeric_dominant {
                continue;
            }
            let pos = compute_position_for_line(ft, idx);
            upsert(
                RepeatedArtifactKind::NormalizedLine,
                norm.clone(),
                norm.clone(),
                line.clone(),
                cc,
                pos,
                idx,
                idx,
            );
        }
    }

    if config.include_two_line_blocks && valid_entries.len() >= 2 {
        for w in valid_entries.windows(2) {
            let (i0, ref l0, ref n0, cc0) = w[0];
            let (i1, ref l1, ref n1, cc1) = w[1];
            // Must be contiguous in raw lines and same page
            if i1 != i0 + 1 {
                continue;
            }
            if !ft.lines_share_page(i0, i1) {
                continue;
            }
            let text_block = format!("{}\n{}", l0, l1);
            let norm_block = format!("{}\n{}", n0, n1);
            let block_cc = dominant_content_class(cc0, cc1);
            if !should_include_content_class(block_cc, config) {
                continue;
            }
            let pos = compute_position_for_line(ft, i0);
            upsert(
                RepeatedArtifactKind::TwoLineBlock,
                text_block.clone(),
                norm_block,
                text_block,
                block_cc,
                pos,
                i0,
                i1,
            );
        }
    }

    if config.include_three_line_blocks && valid_entries.len() >= 3 {
        for w in valid_entries.windows(3) {
            let (i0, ref l0, ref n0, cc0) = w[0];
            let (i1, ref l1, ref n1, cc1) = w[1];
            let (i2, ref l2, ref n2, cc2) = w[2];
            if i1 != i0 + 1 || i2 != i1 + 1 {
                continue;
            }
            if !ft.lines_share_page(i0, i1) || !ft.lines_share_page(i0, i2) {
                continue;
            }
            let text_block = format!("{}\n{}\n{}", l0, l1, l2);
            let norm_block = format!("{}\n{}\n{}", n0, n1, n2);
            let block_cc = dominant_content_class_3(cc0, cc1, cc2);
            if !should_include_content_class(block_cc, config) {
                continue;
            }
            let pos = compute_position_for_line(ft, i0);
            upsert(
                RepeatedArtifactKind::ThreeLineBlock,
                text_block.clone(),
                norm_block,
                text_block,
                block_cc,
                pos,
                i0,
                i2,
            );
        }
    }

    map
}

/// Merges per-file candidate maps into global candidate counts.
fn phase2_merge(
    all_maps: Vec<HashMap<(RepeatedArtifactKind, String), LocalCandidateStats>>,
    config: &RepeatedArtifactScanConfig,
) -> HashMap<(RepeatedArtifactKind, String), CountEntry> {
    let mut global: HashMap<(RepeatedArtifactKind, String), CountEntry> = HashMap::new();

    // Each local map belongs to one file, so a key is counted once per map.
    for (_file_idx, local_map) in all_maps.into_iter().enumerate() {
        for ((kind, key), stats) in local_map {
            let entry = global
                .entry((kind, key.clone()))
                .or_insert_with(|| CountEntry {
                    kind,
                    display_text: stats.display_text.clone(),
                    normalized_key: stats.normalized_key.clone(),
                    candidate_key: key.clone(),
                    occurrence_count: 0,
                    top_count: 0,
                    middle_count: 0,
                    bottom_count: 0,
                    unknown_count: 0,
                    file_ids: Vec::new(),
                    example_refs: Vec::new(),
                    raw_variants: BTreeSet::new(),
                    raw_variant_overflow: false,
                    position_summary_is_page_based: stats.position_summary_is_page_based,
                });

            entry.occurrence_count += stats.occurrence_count;
            entry.top_count += stats.top_count;
            entry.middle_count += stats.middle_count;
            entry.bottom_count += stats.bottom_count;
            entry.unknown_count += stats.unknown_count;
            entry.position_summary_is_page_based &= stats.position_summary_is_page_based;

            let fid = _file_idx as u32;
            if !entry.file_ids.contains(&fid) {
                entry.file_ids.push(fid);
            }

            for (rs, re) in &stats.example_refs {
                if entry.example_refs.len() < config.max_examples_per_candidate {
                    entry.example_refs.push((fid, *rs, *re));
                }
            }

            // Merge distinct raw variants from local stats into global entry.
            // The overflow flag preserves "more existed" without retaining every variant.
            for variant in &stats.raw_variants {
                if entry.raw_variants.len() < RAW_VARIANT_TRACK_CAP {
                    entry.raw_variants.insert(variant.clone());
                } else if !entry.raw_variants.contains(variant) {
                    entry.raw_variant_overflow = true;
                }
            }
            if stats.raw_variant_overflow {
                entry.raw_variant_overflow = true;
            }
        }
    }

    global
}

/// Filters, scores, ranks, deduplicates, and attaches examples.
fn phase3_finalize(
    global: HashMap<(RepeatedArtifactKind, String), CountEntry>,
    file_texts: &[FileText],
    config: &RepeatedArtifactScanConfig,
) -> Vec<RepeatedArtifactCandidate> {
    let mut scored: Vec<(f64, RepeatedArtifactCandidate)> = Vec::new();

    let mut entries: Vec<CountEntry> = global
        .into_values()
        .filter(|e| {
            e.occurrence_count >= config.min_occurrences && e.file_ids.len() >= config.min_files
        })
        .collect();

    // Keep normalised entries only when they add variants beyond exact matches.
    if config.include_normalized_lines && config.include_exact_lines {
        let mut i = 0;
        while i < entries.len() {
            if entries[i].kind != RepeatedArtifactKind::NormalizedLine {
                i += 1;
                continue;
            }
            let norm_display = &entries[i].display_text;
            let norm_occ = entries[i].occurrence_count;
            let norm_files = entries[i].file_ids.len();
            let norm_variants = entries[i].raw_variants.len();

            if norm_variants > 1 {
                i += 1;
                continue;
            }

            let redundant = entries.iter().any(|e| {
                e.kind == RepeatedArtifactKind::ExactLine
                    && e.display_text == *norm_display
                    && e.occurrence_count == norm_occ
                    && e.file_ids.len() == norm_files
            });

            if redundant {
                entries.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    for entry in entries {
        let file_count = entry.file_ids.len();
        let score = calculate_suspicion_score(
            &entry.kind,
            &entry.display_text,
            entry.occurrence_count,
            file_count,
            entry.top_count,
            entry.bottom_count,
        );

        let risk_label = classify_risk(
            &entry.display_text,
            &entry.normalized_key,
            entry.occurrence_count,
            file_count,
            entry.top_count,
            entry.bottom_count,
        );

        let candidate_id = compute_stable_id(&entry.kind, &entry.candidate_key);

        let content_class = classify_content(&entry.display_text);
        let position_summary = PositionSummary {
            top_count: entry.top_count,
            middle_count: entry.middle_count,
            bottom_count: entry.bottom_count,
            unknown_count: entry.unknown_count,
        };
        let text_profile = profile_candidate_text(&entry.display_text, &entry.normalized_key);
        let text_signal_label = classify_candidate_text_signal(
            &text_profile,
            content_class,
            risk_label,
            &position_summary,
        );
        let text_signal_reasons = candidate_text_signal_reasons(
            &entry.display_text,
            &text_profile,
            text_signal_label,
            &position_summary,
        );

        let raw_variant_count = entry.raw_variants.len();
        let raw_variant_count_is_capped = entry.raw_variant_overflow;

        let is_inline = entry.kind == RepeatedArtifactKind::InlineArtifact;
        let examples: Vec<RepeatedArtifactExample> = entry
            .example_refs
            .iter()
            .filter_map(|&(fid, rs, re)| {
                let ft = file_texts.get(fid as usize)?;

                if is_inline {
                    let line_idx = rs;
                    let byte_pos = re;
                    let line = ft.raw_lines.get(line_idx)?;
                    let pattern = &entry.display_text;
                    let plen = pattern.len();

                    let context_before = safe_context_before(line, byte_pos, 80);
                    let context_after = safe_context_after(line, byte_pos, plen, 80);
                    let matched_text = pattern.to_string();

                    Some(RepeatedArtifactExample {
                        file_name: ft.file_name.clone(),
                        file_path: ft.file_path.clone(),
                        line_number: Some(line_idx + 1),
                        page_number: ft.page_number_for_line(line_idx),
                        context_before,
                        matched_text,
                        context_after,
                    })
                } else {
                    let context_before = if rs > 0 {
                        let t = ft.raw_lines[rs - 1].trim();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t.chars().take(200).collect())
                        }
                    } else {
                        None
                    };
                    let context_after = if re + 1 < ft.raw_lines.len() {
                        let t = ft.raw_lines[re + 1].trim();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t.chars().take(200).collect())
                        }
                    } else {
                        None
                    };
                    let matched: Vec<&str> =
                        ft.raw_lines[rs..=re].iter().map(|s| s.trim()).collect();
                    let matched_text = matched.join("\n");

                    Some(RepeatedArtifactExample {
                        file_name: ft.file_name.clone(),
                        file_path: ft.file_path.clone(),
                        line_number: Some(rs + 1),
                        page_number: ft.page_number_for_line(rs),
                        context_before,
                        matched_text,
                        context_after,
                    })
                }
            })
            .collect();

        scored.push((
            score,
            RepeatedArtifactCandidate {
                candidate_id,
                kind: entry.kind,
                display_text: entry.display_text,
                normalized_key: entry.normalized_key,
                occurrence_count: entry.occurrence_count,
                file_count,
                example_count: examples.len(),
                position_summary,
                position_summary_is_page_based: entry.position_summary_is_page_based,
                risk_label,
                content_class,
                text_profile,
                text_signal_label,
                text_signal_reasons,
                raw_variant_count,
                raw_variant_count_is_capped,
                raw_variants: entry.raw_variants.into_iter().collect(),
                examples,
            },
        ));
    }

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .into_iter()
        .take(config.max_candidates)
        .map(|(_, c)| c)
        .collect()
}

/// Performs a corpus-wide scan for repeated lines and blocks, with optional cache.
///
/// Returns both candidates and diagnostics.
pub fn scan_repeated_artifacts_report(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
) -> Result<RepeatedArtifactScanReport, String> {
    scan_repeated_artifacts_report_with_cancel(records, config, cleaning_config, &no_cancellation())
}

/// Performs a corpus-wide scan for repeated lines and blocks, with optional cache
/// and cancellation.
///
/// Returns both candidates and diagnostics.
pub fn scan_repeated_artifacts_report_with_cancel(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cancel: &CancellationFlag,
) -> Result<RepeatedArtifactScanReport, String> {
    scan_repeated_artifacts_report_with_cancel_and_cache(
        records,
        config,
        cleaning_config,
        None,
        cancel,
    )
}

/// Performs a corpus-wide scan for repeated lines and blocks, with optional cache
/// and cancellation.
///
/// Returns both candidates and diagnostics.
pub fn scan_repeated_artifacts_report_with_cancel_and_cache<'a>(
    records: &'a [DocumentRecord],
    config: &'a RepeatedArtifactScanConfig,
    cleaning_config: &'a CleaningConfig,
    cache: Option<&'a ExtractionCache>,
    cancel: &'a CancellationFlag,
) -> Result<RepeatedArtifactScanReport, String> {
    let files_requested = records.len();
    let analysed_processed_text = config.analyse_processed_text;
    let custom_removals_active = cleaning_config.remove_patterns.len()
        + cleaning_config
            .removal_rules
            .iter()
            .filter(|rule| rule.enabled)
            .count();
    let max_examples_per_candidate = config.max_examples_per_candidate;

    if records.is_empty() {
        return Ok(RepeatedArtifactScanReport {
            candidates: Vec::new(),
            diagnostics: RepeatedArtifactScanDiagnostics {
                files_requested: 0,
                files_scanned: 0,
                files_failed_extraction: 0,
                files_empty_after_extraction: 0,
                total_raw_lines: 0,
                total_candidate_keys_before_filtering: 0,
                candidates_after_min_occurrences: 0,
                candidates_after_min_files: 0,
                final_candidates: 0,
                analysed_processed_text,
                custom_removals_active,
                max_examples_per_candidate,
            },
        });
    }

    // Every record produces a Phase1Result so diagnostics stay aligned.
    let phase_results: Vec<Phase1Result> = records
        .par_iter()
        .map(|record| phase1_scan_file(record, config, cleaning_config, cache, cancel))
        .collect();

    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".to_string());
    }

    if let Some(error) = phase_results
        .iter()
        .find_map(|result| result.fatal_error.as_ref())
    {
        return Err(error.clone());
    }

    let files_scanned = phase_results.len();
    let files_failed_extraction = phase_results.iter().filter(|r| r.extraction_failed).count();

    let file_texts: Vec<FileText> = phase_results.iter().map(|pr| pr.ft.clone()).collect();

    let total_raw_lines: usize = file_texts.iter().map(|ft| ft.raw_lines.len()).sum();
    let files_empty_after_extraction = file_texts
        .iter()
        .filter(|ft| ft.raw_lines.is_empty())
        .count();

    let all_maps: Vec<HashMap<(RepeatedArtifactKind, String), LocalCandidateStats>> =
        phase_results.into_iter().map(|pr| pr.local_map).collect();

    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".to_string());
    }

    let global = phase2_merge(all_maps, config);

    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".to_string());
    }

    let total_candidate_keys_before_filtering = global.len();

    let after_min_occurrences: Vec<(RepeatedArtifactKind, String)> = global
        .iter()
        .filter(|(_, e)| e.occurrence_count >= config.min_occurrences)
        .map(|(k, _)| k.clone())
        .collect();
    let candidates_after_min_occurrences = after_min_occurrences.len();

    let after_min_files: Vec<(RepeatedArtifactKind, String)> = global
        .iter()
        .filter(|(_, e)| {
            e.occurrence_count >= config.min_occurrences && e.file_ids.len() >= config.min_files
        })
        .map(|(k, _)| k.clone())
        .collect();
    let candidates_after_min_files = after_min_files.len();

    let candidates = phase3_finalize(global, &file_texts, config);
    let final_candidates = candidates.len();

    Ok(RepeatedArtifactScanReport {
        candidates,
        diagnostics: RepeatedArtifactScanDiagnostics {
            files_requested,
            files_scanned,
            files_failed_extraction,
            files_empty_after_extraction,
            total_raw_lines,
            total_candidate_keys_before_filtering,
            candidates_after_min_occurrences,
            candidates_after_min_files,
            final_candidates,
            analysed_processed_text,
            custom_removals_active,
            max_examples_per_candidate,
        },
    })
}

/// Performs a corpus-wide scan for repeated lines and blocks.
///
/// Three-phase scan with cancellation support.
///
/// `cancel` is an `AtomicBool`. The scanner checks it:
/// - before extracting each file
/// - after extracting each file
/// - after counting each file
/// - between phases
///
/// Returns `Err("Scan cancelled.")` if cancelled.
pub fn scan_repeated_artifacts(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
) -> Result<Vec<RepeatedArtifactCandidate>, String> {
    let report = scan_repeated_artifacts_report(records, config, cleaning_config)?;
    Ok(report.candidates)
}

/// Like `scan_repeated_artifacts` but accepts an external cancellation flag.
///
/// Pass an `Arc<AtomicBool>` set to `true` to request cancellation.
/// The scanner checks it cooperatively between operations.
pub fn scan_repeated_artifacts_with_cancel(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cancel: &CancellationFlag,
) -> Result<Vec<RepeatedArtifactCandidate>, String> {
    let report =
        scan_repeated_artifacts_report_with_cancel(records, config, cleaning_config, cancel)?;
    Ok(report.candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::DocumentType;
    use std::path::PathBuf;

    #[test]
    fn test_normalize_line() {
        assert_eq!(normalize_line("--- Page 12 ---"), "page #");
        assert_eq!(normalize_line("Chapter 1: Intro"), "chapter #: intro");
        assert_eq!(normalize_line("   Some    spaces   "), "some spaces");
    }

    #[test]
    fn test_compute_stable_id() {
        let id1 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "test key");
        let id2 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "test key");
        let id3 = compute_stable_id(&RepeatedArtifactKind::NormalizedLine, "test key");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.starts_with("exact_"));
        assert!(id3.starts_with("norm_"));
    }

    #[test]
    fn test_classify_risk_headings() {
        let risk = classify_risk("Introduction", "introduction", 5, 2, 0, 0);
        assert_eq!(risk, ArtifactRiskLabel::CommonSectionHeadingReviewCarefully);
    }

    fn assert_common_headings(headings: &[&str]) {
        for heading in headings {
            let normalized = normalize_line(heading);
            assert!(
                is_common_section_heading(&normalized),
                "expected {heading:?} to normalise to a common section heading"
            );
        }
    }

    #[test]
    fn test_common_section_heading_helper_recognises_english_headings() {
        assert_common_headings(&[
            "abstract",
            "introduction",
            "method",
            "methods",
            "methodology",
            "materials and methods",
            "results",
            "discussion",
            "conclusion",
            "conclusions",
            "references",
            "bibliography",
            "appendix",
            "appendices",
            "acknowledgements",
            "acknowledgments",
        ]);
    }

    #[test]
    fn test_common_section_heading_helper_recognises_portuguese_headings() {
        assert_common_headings(&[
            "introdução",
            "metodologia",
            "resultados",
            "discussão",
            "conclusão",
            "referências",
        ]);
    }

    #[test]
    fn test_common_section_heading_helper_recognises_unaccented_portuguese_headings() {
        assert_common_headings(&["introducao", "discussao", "conclusao", "referencias"]);
    }

    #[test]
    fn test_common_section_heading_helper_recognises_french_headings() {
        assert_common_headings(&["résumé", "méthodes", "résultats", "références"]);
    }

    #[test]
    fn test_common_section_heading_helper_recognises_unaccented_french_headings() {
        assert_common_headings(&["resume", "methodes", "resultats", "references"]);
    }

    #[test]
    fn test_page_label_helper_recognises_conservative_page_forms() {
        for normalized_key in [
            "page #",
            "page # of #",
            "p #",
            "p. #",
            "pag #",
            "pag. #",
            "pág #",
            "pág. #",
            "pagina #",
            "página #",
            "pagina # de #",
            "página # de #",
        ] {
            assert!(
                is_page_label_like(normalized_key),
                "expected {normalized_key:?} to be page-label-like"
            );
        }
    }

    #[test]
    fn test_page_label_helper_rejects_ordinary_prose() {
        for normalized_key in [
            "this is page #",
            "chapter #",
            "page one",
            "pagina de resultados",
            "the top # lines",
        ] {
            assert!(
                !is_page_label_like(normalized_key),
                "expected {normalized_key:?} to stay outside page-label matching"
            );
        }
    }

    #[test]
    fn test_classify_risk_page_labels_use_repetition_evidence() {
        let risk = classify_risk("Página 2", "página #", 3, 1, 0, 0);
        assert_eq!(risk, ArtifactRiskLabel::PossibleBoilerplate);

        let risk = classify_risk("Pág. 2", "pág. #", 4, 1, 2, 0);
        assert_eq!(risk, ArtifactRiskLabel::StrongHeaderFooterCandidate);
    }

    #[test]
    fn test_classify_risk_noise() {
        let risk = classify_risk("### ||| ***", "### ||| ***", 5, 2, 0, 0);
        assert_eq!(risk, ArtifactRiskLabel::SymbolOrNoiseCandidate);
    }

    #[test]
    fn test_classify_risk_header_footer() {
        let risk = classify_risk("Running Header", "running header", 10, 3, 8, 2); // 100% top/bottom concentration
        assert_eq!(risk, ArtifactRiskLabel::StrongHeaderFooterCandidate);
    }

    #[test]
    fn test_classify_risk_boilerplate() {
        let risk = classify_risk("Copyright notice info", "copyright notice info", 5, 3, 1, 1); // < 75% top/bottom concentration
        assert_eq!(risk, ArtifactRiskLabel::PossibleBoilerplate);
    }

    fn empty_position_summary() -> PositionSummary {
        PositionSummary {
            top_count: 0,
            middle_count: 0,
            bottom_count: 0,
            unknown_count: 0,
        }
    }

    fn signal_for_text(
        text: &str,
    ) -> (CandidateTextProfile, CandidateTextSignalLabel, Vec<String>) {
        let normalized = normalize_line(text);
        let position_summary = empty_position_summary();
        let profile = profile_candidate_text(text, &normalized);
        let risk_label = classify_risk(text, &normalized, 1, 1, 0, 0);
        let signal_label = classify_candidate_text_signal(
            &profile,
            classify_content(text),
            risk_label,
            &position_summary,
        );
        let reasons =
            candidate_text_signal_reasons(text, &profile, signal_label, &position_summary);

        (profile, signal_label, reasons)
    }

    #[test]
    fn test_candidate_text_signal_recognises_natural_prose_without_noise_label() {
        for text in [
            "This is a normal sentence.",
            "The participants answered the questionnaire.",
            "Corpus linguistics provides empirical evidence.",
        ] {
            let (_, signal_label, _) = signal_for_text(text);
            assert!(
                matches!(
                    signal_label,
                    CandidateTextSignalLabel::LikelyNaturalText
                        | CandidateTextSignalLabel::Ambiguous
                ),
                "expected prose-like text to stay natural or ambiguous: {text:?}"
            );
            assert_ne!(
                signal_label,
                CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise
            );
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_english_section_headings() {
        for text in [
            "Introduction",
            "Methods",
            "Results",
            "Discussion",
            "Conclusion",
        ] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(signal_label, CandidateTextSignalLabel::LikelySectionHeading);
            assert!(reasons.contains(&"common_section_heading".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_portuguese_section_headings() {
        for text in ["Resumo", "Introdução", "Métodos", "Resultados", "Conclusão"] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(signal_label, CandidateTextSignalLabel::LikelySectionHeading);
            assert!(reasons.contains(&"common_section_heading".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_french_section_headings() {
        for text in ["Résumé", "Méthode", "Résultats", "Références"] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(signal_label, CandidateTextSignalLabel::LikelySectionHeading);
            assert!(reasons.contains(&"common_section_heading".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_page_labels() {
        for text in [
            "Page 1",
            "Page 12 of 42",
            "p. 5",
            "pag. 7",
            "pág. 9",
            "Página 3 de 10",
        ] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(signal_label, CandidateTextSignalLabel::LikelyPageLabel);
            assert!(reasons.contains(&"page_label_pattern".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_table_or_statistical_rows() {
        for text in [
            "12.4 15.8 99.2",
            "2020 2021 2022 2023",
            "p < .05   F = 7.92",
            "Mean SD N",
        ] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(
                signal_label,
                CandidateTextSignalLabel::LikelyTableOrNumericRow,
                "expected table/statistical row label for {text:?}"
            );
            assert!(reasons.contains(&"table_or_numeric_row".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_formula_or_code_like_lines() {
        for text in [
            "y = mx + b",
            "χ² = 5.32",
            "if (x > 0) { return x; }",
            "foo_bar = value + 1;",
        ] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(
                signal_label,
                CandidateTextSignalLabel::LikelyFormulaOrCode,
                "expected formula/code label for {text:?}"
            );
            assert!(reasons.contains(&"formula_or_code_symbols".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_markup_and_entities() {
        for text in ["<br/>", "<br>", "&nbsp;", "&amp;"] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(
                signal_label,
                CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise
            );
            assert!(reasons.contains(&"markup_entity_or_tag".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_cid_markers() {
        for text in ["cid:14", "cid:105"] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(
                signal_label,
                CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise
            );
            assert!(reasons.contains(&"cid_marker".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_recognises_symbol_heavy_junk() {
        for text in ["------", "••••••", "______"] {
            let (_, signal_label, reasons) = signal_for_text(text);
            assert_eq!(
                signal_label,
                CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise
            );
            assert!(reasons.contains(&"long_repeated_character_run".to_string()));
        }
    }

    #[test]
    fn test_candidate_text_signal_keeps_short_text_ambiguous() {
        let (_, signal_label, reasons) = signal_for_text("Header");
        assert_eq!(signal_label, CandidateTextSignalLabel::Ambiguous);
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_candidate_text_profile_counts_unicode_decimal_digits() {
        let (profile, signal_label, reasons) = signal_for_text("１２ ３４ ٥٦");
        assert!(
            profile.digit_ratio > 0.60,
            "unicode decimal digits should count in digit_ratio"
        );
        assert_eq!(
            signal_label,
            CandidateTextSignalLabel::LikelyTableOrNumericRow
        );
        assert!(reasons.contains(&"high_digit_ratio".to_string()));
    }

    #[test]
    fn test_candidate_text_signal_leaves_content_class_behaviour_stable() {
        assert_eq!(
            classify_content("Page ٣"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("32.01 46.83"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("Introduction"),
            CandidateContentClass::TextDominant
        );
        assert_eq!(
            classify_content("------"),
            CandidateContentClass::SymbolNoiseDominant
        );
    }

    #[test]
    fn test_candidate_text_signal_reasons_include_page_edge_repetition() {
        let normalized = normalize_line("Page 7");
        let profile = profile_candidate_text("Page 7", &normalized);
        let position_summary = PositionSummary {
            top_count: 0,
            middle_count: 0,
            bottom_count: 3,
            unknown_count: 0,
        };
        let signal_label = classify_candidate_text_signal(
            &profile,
            classify_content("Page 7"),
            ArtifactRiskLabel::StrongHeaderFooterCandidate,
            &position_summary,
        );
        let reasons =
            candidate_text_signal_reasons("Page 7", &profile, signal_label, &position_summary);

        assert_eq!(signal_label, CandidateTextSignalLabel::LikelyPageLabel);
        assert!(reasons.contains(&"page_edge_repetition".to_string()));
    }

    fn create_test_pdf(pages_content: &[Vec<&str>]) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{Document, Object, Stream, StringFormat, dictionary};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary!(
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica"
        ));

        let resources_id = doc.add_object(dictionary!(
            "Font" => dictionary!(
                "F1" => font_id
            )
        ));

        let mut kids = Vec::new();
        for page_lines in pages_content {
            let mut ops = vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
            ];

            let mut first = true;
            for line in page_lines {
                if first {
                    ops.push(Operation::new("Td", vec![100.into(), 750.into()]));
                    first = false;
                } else {
                    ops.push(Operation::new("Td", vec![0.into(), (-30.0).into()]));
                }
                ops.push(Operation::new(
                    "Tj",
                    vec![Object::String(
                        line.as_bytes().to_vec(),
                        StringFormat::Literal,
                    )],
                ));
            }
            ops.push(Operation::new("ET", vec![]));

            let content = Content { operations: ops };
            let content_id = doc.add_object(Stream::new(dictionary!(), content.encode().unwrap()));

            let page_id = doc.add_object(dictionary!(
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
            ));
            kids.push(page_id.into());
        }

        let pages = dictionary!(
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages_content.len() as i32
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary!(
            "Type" => "Catalog",
            "Pages" => pages_id
        ));
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn make_text_record(name: &str, content: &str, temp_dir: &std::path::Path) -> DocumentRecord {
        let file_path = temp_dir.join(name);
        std::fs::write(&file_path, content).unwrap();
        DocumentRecord {
            source_path: file_path.clone(),
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Text,
            size_bytes: content.len() as u64,
        }
    }

    fn relaxed_config() -> RepeatedArtifactScanConfig {
        RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            max_candidates: 100,
            min_line_chars: 3,
            ..RepeatedArtifactScanConfig::default()
        }
    }

    fn repeated_cached_pdf_text() -> String {
        repeated_cached_pdf_pages().join("\n\n")
    }

    fn repeated_cached_pdf_pages() -> Vec<String> {
        (1..=3)
            .map(|page_number| {
                [
                    "Header".to_string(),
                    "Body top".to_string(),
                    "Body lead".to_string(),
                    "Middle prose".to_string(),
                    "Body tail".to_string(),
                    format!("Page {page_number}"),
                    "Footer".to_string(),
                ]
                .join("\n")
            })
            .collect::<Vec<_>>()
    }

    #[test]
    fn test_file_text_uses_structured_document_for_pdf_pages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("structured-pages.pdf", temp_dir.path());
        let pages = repeated_cached_pdf_pages();
        let text = pages.join("\n\n");

        let ft = build_file_text(&doc, &text, Some(&pages));

        assert_eq!(ft.document.pages.len(), 3);
        assert_eq!(ft.raw_lines.len(), 21);
        assert_eq!(ft.line_locations.len(), ft.raw_lines.len());
        assert_eq!(ft.page_number_for_line(0), Some(1));
        assert_eq!(ft.page_number_for_line(7), Some(2));
        assert_eq!(ft.page_number_for_line(14), Some(3));

        assert_eq!(compute_position_for_line(&ft, 0), LinePosition::Top);
        assert_eq!(compute_position_for_line(&ft, 3), LinePosition::Middle);
        assert_eq!(compute_position_for_line(&ft, 5), LinePosition::Bottom);
        assert!(ft.lines_share_page(0, 6));
        assert!(!ft.lines_share_page(6, 7));
    }

    #[test]
    fn test_file_text_does_not_invent_pdf_pages_without_metadata() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("flat-cached.pdf", temp_dir.path());
        let text = repeated_cached_pdf_text();

        let ft = build_file_text(&doc, &text, None);

        assert_eq!(ft.document.pages.len(), 1);
        assert_eq!(ft.raw_lines.len(), 23);
        assert_eq!(ft.page_number_for_line(0), None);
        assert!(ft.lines_share_page(6, 7));
        assert_eq!(compute_position_for_line(&ft, 0), LinePosition::Top);
    }

    #[test]
    fn test_scanner_preserves_cached_pdf_exact_and_normalized_page_positions() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("cached-structured-pages.pdf", temp_dir.path());
        let cleaning_config = CleaningConfig::default();
        let cache = ExtractionCache::new();
        let page_texts = repeated_cached_pdf_pages();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: page_texts.join("\n\n"),
                warnings: Vec::new(),
                page_count: Some(3),
                page_texts: Some(page_texts),
            },
        );

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            max_candidates: 50,
            ..RepeatedArtifactScanConfig::default()
        };

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc],
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .expect("cached PDF scan should succeed");

        let header = report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::ExactLine
                    && candidate.display_text == "Header"
            })
            .expect("expected exact Header candidate");
        assert_eq!(header.occurrence_count, 3);
        assert_eq!(header.file_count, 1);
        assert_eq!(
            header.position_summary,
            PositionSummary {
                top_count: 3,
                middle_count: 0,
                bottom_count: 0,
                unknown_count: 0,
            }
        );
        assert_eq!(
            header.risk_label,
            ArtifactRiskLabel::StrongHeaderFooterCandidate
        );
        assert_eq!(header.content_class, CandidateContentClass::TextDominant);
        assert_eq!(header.raw_variant_count, 1);
        assert_eq!(header.raw_variants, vec!["Header".to_string()]);
        assert_eq!(
            header
                .examples
                .iter()
                .map(|example| example.page_number)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(3)]
        );

        let page_norm = report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::NormalizedLine
                    && candidate.normalized_key == "page #"
            })
            .expect("expected normalised page # candidate");
        assert_eq!(page_norm.occurrence_count, 3);
        assert_eq!(
            page_norm.position_summary,
            PositionSummary {
                top_count: 0,
                middle_count: 0,
                bottom_count: 3,
                unknown_count: 0,
            }
        );
        assert_eq!(
            page_norm.risk_label,
            ArtifactRiskLabel::StrongHeaderFooterCandidate
        );
        assert_eq!(
            page_norm.content_class,
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(page_norm.raw_variant_count, 3);
        assert!(!page_norm.raw_variant_count_is_capped);
        assert_eq!(
            page_norm.raw_variants,
            vec![
                "Page 1".to_string(),
                "Page 2".to_string(),
                "Page 3".to_string(),
            ]
        );
    }

    #[test]
    fn test_scanner_falls_back_to_flat_text_without_cached_pages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("old-flat-cache.pdf", temp_dir.path());
        let cleaning_config = CleaningConfig::default();
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: repeated_cached_pdf_text(),
                warnings: Vec::new(),
                page_count: Some(3),
                page_texts: None,
            },
        );

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            max_candidates: 50,
            ..RepeatedArtifactScanConfig::default()
        };

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc],
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .expect("cached PDF scan should fall back to flat text");

        let header = report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::ExactLine
                    && candidate.display_text == "Header"
            })
            .expect("expected exact Header candidate");
        assert_eq!(header.occurrence_count, 3);
        assert!(
            header
                .examples
                .iter()
                .all(|example| example.page_number.is_none())
        );
    }

    #[test]
    fn test_example_capping() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = vec!["Header Line"; 20].join("\n");
        let doc = make_text_record("dummy.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            max_examples_per_candidate: 5,
            ..relaxed_config()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        assert!(!result.is_empty());
        let cand = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::ExactLine)
            .unwrap();
        assert_eq!(cand.occurrence_count, 20);
        assert_eq!(cand.examples.len(), 5);
    }

    #[test]
    fn test_approximate_positions_for_inferred_pdf() {
        if !crate::pdf_ocr::pdfium_available() {
            return;
        }
        let pages = vec![
            vec![
                "Page1Line1",
                "Page1Line2",
                "Page1Line3",
                "Page1Line4",
                "Page1Line5",
            ],
            vec![
                "Page2Line1",
                "Page2Line2",
                "Page2Line3",
                "Page2Line4",
                "Page2Line5",
            ],
        ];
        let bytes = create_test_pdf(&pages);

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("dummy.pdf");
        std::fs::write(&file_path, bytes).unwrap();

        let doc = DocumentRecord {
            source_path: file_path.clone(),
            relative_path: PathBuf::from("dummy.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 1000,
        };

        let config = RepeatedArtifactScanConfig {
            analyse_processed_text: false,
            min_files: 1,
            min_occurrences: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let total_bottom: usize = result.iter().map(|c| c.position_summary.bottom_count).sum();
        let total_top: usize = result.iter().map(|c| c.position_summary.top_count).sum();
        assert!(total_bottom > 0);
        assert!(total_top > 0);
    }

    #[test]
    fn test_deterministic_candidate_ids() {
        let id1 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "Same Key");
        let id2 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "Same Key");
        let id3 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "Different Key");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(id1.starts_with("exact_"));
    }

    #[test]
    fn test_normalisation_self_contained() {
        assert_eq!(normalize_line("  --- test 123 --- "), "test #");
        assert_eq!(normalize_line("Another Line"), "another line");
        assert_eq!(normalize_line("Another [Line]"), "another [line");
    }

    #[test]
    fn test_headings_not_hidden() {
        let risk = classify_risk("Introduction", "introduction", 5, 2, 0, 0);
        assert_eq!(risk, ArtifactRiskLabel::CommonSectionHeadingReviewCarefully);
    }

    #[test]
    fn test_many_unique_lines_no_explosion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut lines = Vec::new();
        for i in 0..2000 {
            lines.push(format!("Unique line number {}", i));
        }
        let content = lines.join("\n");
        let doc = make_text_record("many_unique.txt", &content, temp_dir.path());

        let result =
            scan_repeated_artifacts(&[doc], &relaxed_config(), &CleaningConfig::default()).unwrap();
        assert!(
            result.len() <= 10,
            "expected <=10 candidates, got {}",
            result.len()
        );
    }

    #[test]
    fn test_repeated_header_across_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let header = "Copyright (c) 2024 Example Publishing Co.";
        let mut records = Vec::new();
        for i in 0..5 {
            let body: Vec<String> = (0..50)
                .map(|j| format!("Body line {} in file {}", j, i))
                .collect();
            let content = format!("{}\n{}", header, body.join("\n"));
            records.push(make_text_record(
                &format!("file{}.txt", i),
                &content,
                temp_dir.path(),
            ));
        }

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 4,
            min_files: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let result =
            scan_repeated_artifacts(&records, &config, &CleaningConfig::default()).unwrap();
        assert!(!result.is_empty(), "should find the repeated header");
        let header_cand = result.iter().find(|c| c.display_text == header);
        assert!(header_cand.is_some(), "should find the exact header line");
        let h = header_cand.unwrap();
        assert!(h.file_count >= 2, "should appear in multiple files");
    }

    #[test]
    fn test_examples_capped_stress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = vec!["Repeated line here"; 100].join("\n");
        let doc = make_text_record("many_occurrences.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            max_examples_per_candidate: 3,
            ..relaxed_config()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        for cand in &result {
            assert!(cand.examples.len() <= 3, "examples should be capped at 3");
        }
    }

    #[test]
    fn test_blocks_disabled_does_not_produce_blocks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "Line one\nLine two\nLine three\nLine one\nLine two\nLine three\n";
        let doc = make_text_record("block_test.txt", content, temp_dir.path());

        let config = relaxed_config();
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        for cand in &result {
            assert_ne!(cand.kind, RepeatedArtifactKind::TwoLineBlock);
            assert_ne!(cand.kind, RepeatedArtifactKind::ThreeLineBlock);
        }
    }

    #[test]
    fn test_blocks_enabled_produces_blocks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let block = "Header A\nHeader B";
        let parts = vec![block; 20];
        let content = parts.join("\n");
        let doc = make_text_record("blocks.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            include_two_line_blocks: true,
            min_occurrences: 5,
            min_files: 1,
            max_candidates: 100,
            min_line_chars: 3,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let has_block = result
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::TwoLineBlock);
        assert!(has_block, "should detect 2-line blocks when enabled");
    }

    #[test]
    fn test_long_lines_skipped() {}

    #[test]
    fn test_long_lines_skipped_proper() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repeated = "hello";
        let long = "X".repeat(500);
        let mut parts = Vec::new();
        for _ in 0..10 {
            parts.push(long.clone());
            parts.push(repeated.to_string());
        }
        let content = parts.join("\n");
        let doc = make_text_record("long_lines2.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let found_hello = result.iter().any(|c| c.display_text == "hello");
        assert!(found_hello, "should find the short repeated line");
        for cand in &result {
            assert!(
                cand.display_text.len() < 500,
                "long line should not appear as candidate"
            );
        }
    }

    #[test]
    fn test_min_files_filtering() {
        let temp_dir = tempfile::tempdir().unwrap();
        let shared = "Shared header line here";
        let records = vec![
            make_text_record("f1.txt", shared, temp_dir.path()),
            make_text_record("f2.txt", "Only in file 2", temp_dir.path()),
            make_text_record("f3.txt", "Only in file 3", temp_dir.path()),
        ];

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let result =
            scan_repeated_artifacts(&records, &config, &CleaningConfig::default()).unwrap();
        let found_shared = result.iter().any(|c| c.display_text == shared);
        assert!(
            !found_shared,
            "single-file occurrence should not pass min_files=2"
        );
    }

    #[test]
    fn test_high_occurrence_does_not_store_unlimited_examples() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = vec!["Repeated artefact line"; 500].join("\n");
        let doc = make_text_record("high_occ.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            max_examples_per_candidate: 4,
            ..relaxed_config()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        for cand in &result {
            assert!(cand.examples.len() <= 4, "examples must be capped");
            assert_eq!(cand.example_count, cand.examples.len());
        }
    }

    #[test]
    fn test_should_skip_line() {
        assert!(should_skip_line("", 3, 300));
        assert!(should_skip_line("   ", 3, 300));
        assert!(should_skip_line("ab", 3, 300));
        assert!(should_skip_line(&"X".repeat(500), 3, 300));
        assert!(!should_skip_line("hello", 3, 300));
        assert!(!should_skip_line("  hello  ", 3, 300));
    }

    #[test]
    fn test_norm_dedup_removes_single_variant_norm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = ["Hello World"; 10].join("\n");
        let doc = make_text_record("dedup.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let exact = result
            .iter()
            .filter(|c| c.kind == RepeatedArtifactKind::ExactLine)
            .count();
        let norm = result
            .iter()
            .filter(|c| c.kind == RepeatedArtifactKind::NormalizedLine)
            .count();
        assert_eq!(exact, 1, "should have one exact candidate");
        if norm > 0 {
            let nc = result
                .iter()
                .find(|c| c.kind == RepeatedArtifactKind::NormalizedLine)
                .unwrap();
            let ec = result
                .iter()
                .find(|c| c.kind == RepeatedArtifactKind::ExactLine)
                .unwrap();
            assert!(
                !(nc.display_text == ec.display_text && nc.occurrence_count == ec.occurrence_count),
                "normalized candidate should not be a duplicate of exact candidate"
            );
        }
    }

    #[test]
    fn test_norm_dedup_preserves_multi_variant_norm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut content_parts = Vec::new();
        for i in 0..10 {
            content_parts.push(format!("Page {}", (i % 3) + 1));
        }
        let content = content_parts.join("\n");
        let doc = make_text_record("multi_norm.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 3,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let norm_page = result.iter().find(|c| {
            c.normalized_key == "page #" && c.kind == RepeatedArtifactKind::NormalizedLine
        });
        assert!(
            norm_page.is_some(),
            "normalised 'page #' should be present (multi-variant)"
        );
    }

    #[test]
    fn test_pdf_no_ocr_by_default() {
        if !crate::pdf_ocr::pdfium_available() {
            return;
        }
        let pages = vec![vec!["Header", "Body line 1", "Body line 2", "Footer"]];
        let bytes = create_test_pdf(&pages);

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("minimal.pdf");
        std::fs::write(&file_path, bytes).unwrap();

        let doc = DocumentRecord {
            source_path: file_path.clone(),
            relative_path: PathBuf::from("minimal.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 200,
        };

        let config = RepeatedArtifactScanConfig {
            analyse_processed_text: false,
            min_files: 1,
            min_occurrences: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        assert!(result.len() <= 10, "pdf scan should complete without OCR");
    }

    #[test]
    fn test_cancellation_returns_err() {
        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_text_record("cancel_test.txt", "Some text", temp_dir.path());

        let result = scan_repeated_artifacts_with_cancel(
            &[doc],
            &RepeatedArtifactScanConfig::default(),
            &CleaningConfig::default(),
            &cancel,
        );
        assert!(result.is_err(), "cancelled scan should return error");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("cancelled"),
            "error should mention cancellation"
        );
    }

    #[test]
    fn test_default_min_files_is_one() {
        let cfg = RepeatedArtifactScanConfig::default();
        assert_eq!(cfg.min_files, 1, "default min_files should be 1");
    }

    #[test]
    fn test_long_lines_filtered_by_short_default() {
        assert!(
            RepeatedArtifactScanConfig::default().max_line_chars <= 300,
            "default max_line_chars should be <= 300"
        );
    }

    #[test]
    fn test_classify_numeric_decimal() {
        assert_eq!(
            classify_content("32.01 46.83"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("0.2 0.1 0.3 0.4 0.5"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("[0.386, 1.378]"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("4.10"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("42 41 40 39"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("-5.81 28.93"),
            CandidateContentClass::NumericDominant
        );
    }

    #[test]
    fn test_classify_numeric_unicode_decimal_digits() {
        assert_eq!(
            classify_content("12 34"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("１２ ３４"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("٣٤ ٥٦"),
            CandidateContentClass::NumericDominant
        );
        assert_eq!(
            classify_content("Page ٣"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Página ４"),
            CandidateContentClass::MixedTextNumbers
        );
    }

    #[test]
    fn test_classify_mixed_text_numbers() {
        assert_eq!(
            classify_content("Page 12"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Chapter 5"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Figure 9.6"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Table 3.2"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Section 4.10"),
            CandidateContentClass::MixedTextNumbers
        );
        assert_eq!(
            classify_content("Exercise 3.14"),
            CandidateContentClass::MixedTextNumbers
        );
    }

    #[test]
    fn test_classify_text_dominant() {
        assert_eq!(
            classify_content("This book has been published by Cambridge University Press"),
            CandidateContentClass::TextDominant
        );
        assert_eq!(
            classify_content("Introduction"),
            CandidateContentClass::TextDominant
        );
        assert_eq!(
            classify_content("Methods"),
            CandidateContentClass::TextDominant
        );
    }

    #[test]
    fn test_classify_symbol_noise() {
        assert_eq!(
            classify_content("● ● ● ● ● ● ●"),
            CandidateContentClass::SymbolNoiseDominant
        );
        assert_eq!(
            classify_content("------"),
            CandidateContentClass::SymbolNoiseDominant
        );
        assert_eq!(
            classify_content("********"),
            CandidateContentClass::SymbolNoiseDominant
        );
        assert_eq!(
            classify_content("||| ||| |||"),
            CandidateContentClass::SymbolNoiseDominant
        );
    }

    #[test]
    fn test_normalised_content_filter_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "32.01 46.83\n31.28 20.91\n40.68 1.05\n";
        let doc = make_text_record("numeric_norm.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let norm_numeric = result
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::NormalizedLine);
        assert!(
            !norm_numeric,
            "numeric-dominant lines should not produce NormalizedLine by default"
        );
    }

    #[test]
    fn test_numeric_dominant_enabled_shows_norm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "32.01 46.83\n31.28 20.91\n40.68 1.05\n";
        let doc = make_text_record("numeric_on2.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            include_numeric_dominant: true,
            min_occurrences: 2,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let has_norm = result
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::NormalizedLine);
        assert!(
            has_norm,
            "numeric-dominant should produce NormalizedLine when enabled"
        );
    }

    #[test]
    fn test_exact_numeric_obey_content_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "42.5\n42.5\n42.5\n42.5\n42.5\n";
        let doc = make_text_record("numeric_exact2.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 4,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(
            std::slice::from_ref(&doc),
            &config,
            &CleaningConfig::default(),
        )
        .unwrap();
        let exact_numeric = result
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::ExactLine && c.display_text == "42.5");
        assert!(
            !exact_numeric,
            "exact numeric hidden when numeric filter off"
        );
        let config2 = RepeatedArtifactScanConfig {
            include_numeric_dominant: true,
            min_occurrences: 4,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result2 = scan_repeated_artifacts(
            std::slice::from_ref(&doc),
            &config2,
            &CleaningConfig::default(),
        )
        .unwrap();
        let exact_numeric2 = result2
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::ExactLine && c.display_text == "42.5");
        assert!(
            exact_numeric2,
            "exact numeric should appear when numeric filter on"
        );
    }

    #[test]
    fn test_page_chapter_still_produce_norm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "Page 1\nPage 2\nPage 3\nPage 4\nPage 5\nPage 6\n";
        let doc = make_text_record("pages2.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let page_norm = result.iter().any(|c| {
            c.kind == RepeatedArtifactKind::NormalizedLine && c.normalized_key == "page #"
        });
        assert!(
            page_norm,
            "Page patterns should still produce normalised candidates"
        );
    }

    #[test]
    fn test_chapter_still_produce_norm() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "Chapter 1\nChapter 2\nChapter 3\nChapter 4\nChapter 5\nChapter 6\n";
        let doc = make_text_record("chapters2.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let ch_norm = result.iter().any(|c| {
            c.kind == RepeatedArtifactKind::NormalizedLine && c.normalized_key == "chapter #"
        });
        assert!(
            ch_norm,
            "Chapter patterns should still produce normalised candidates"
        );
    }

    #[test]
    fn test_numeric_dominant_content_filter_default() {
        let cfg = RepeatedArtifactScanConfig::default();
        assert!(!cfg.include_numeric_dominant);
        assert!(cfg.include_text_dominant);
        assert!(cfg.include_mixed_text_numbers);
        assert!(cfg.include_symbol_noise);
    }

    #[test]
    fn test_content_class_on_candidate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "Header\nHeader\nHeader\n";
        let doc = make_text_record("content_cand.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        for cand in &result {
            if cand.display_text == "Header" {
                assert_eq!(cand.content_class, CandidateContentClass::TextDominant);
            }
        }
    }

    #[test]
    fn test_scan_candidates_include_text_signal_fields_for_exact_normalized_and_inline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = [
            "Repeated Header",
            "Page 1",
            "body<br/>text",
            "Repeated Header",
            "Page 2",
            "more<br/>text",
            "Repeated Header",
            "Page 3",
        ]
        .join("\n");
        let doc = make_text_record("candidate_text_signal.txt", &content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();

        let exact = result
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::ExactLine
                    && candidate.display_text == "Repeated Header"
            })
            .expect("expected repeated exact-line candidate");
        assert_eq!(exact.text_profile.token_count, 2);
        assert_eq!(exact.text_signal_label, CandidateTextSignalLabel::Ambiguous);

        let normalized = result
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::NormalizedLine
                    && candidate.normalized_key == "page #"
            })
            .expect("expected repeated normalised page candidate");
        assert_eq!(
            normalized.text_signal_label,
            CandidateTextSignalLabel::LikelyPageLabel
        );
        assert!(
            normalized
                .text_signal_reasons
                .contains(&"page_label_pattern".to_string())
        );

        let inline = result
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::InlineArtifact
                    && candidate.display_text == "<br/>"
            })
            .expect("expected repeated inline artefact candidate");
        assert_eq!(
            inline.text_signal_label,
            CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise
        );
        assert!(
            inline
                .text_signal_reasons
                .contains(&"markup_entity_or_tag".to_string())
        );
    }

    #[test]
    fn test_br_tag_detected_as_inline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "some text.<br/>more text\nother <br/> stuff\n";
        let doc = make_text_record("br_test.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let br = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "<br/>");
        assert!(br.is_some(), "<br/> should be detected as InlineArtifact");
        assert_eq!(br.unwrap().occurrence_count, 2);
    }

    #[test]
    fn test_nbsp_detected_as_inline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "word1&nbsp;word2\nword3&nbsp;word4&nbsp;word5\n";
        let doc = make_text_record("nbsp_test.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let nbsp = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "&nbsp;");
        assert!(
            nbsp.is_some(),
            "&nbsp; should be detected as InlineArtifact"
        );
        assert_eq!(nbsp.unwrap().occurrence_count, 3);
    }

    #[test]
    fn test_multiple_inline_artifacts_in_one_line() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "first <br/> second &nbsp; third <br /> fourth\n";
        let doc = make_text_record("inline_multi.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();

        for expected in ["<br/>", "&nbsp;", "<br />"] {
            let candidate = result
                .iter()
                .find(|c| {
                    c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == expected
                })
                .unwrap_or_else(|| panic!("{} should be detected as InlineArtifact", expected));
            assert_eq!(candidate.occurrence_count, 1);
            assert_eq!(candidate.normalized_key, "");
            assert_eq!(candidate.raw_variants, vec![expected.to_string()]);
            assert_eq!(candidate.examples.len(), 1);
            assert_eq!(candidate.examples[0].line_number, Some(1));
            assert_eq!(candidate.examples[0].matched_text, expected);
        }
    }

    #[test]
    fn test_inline_examples_are_capped() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parts = vec!["<br/>"; 100];
        let content = parts.join(" ");
        let doc = make_text_record("br_many.txt", &content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            max_examples_per_candidate: 3,
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let br = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "<br/>");
        assert!(br.is_some());
        let br = br.unwrap();
        assert!(br.examples.len() <= 3, "inline examples must be capped");
        assert_eq!(br.occurrence_count, 100);
    }

    #[test]
    fn test_inline_has_context() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "awful movie.<br/>the acting was great\n";
        let doc = make_text_record("context_test.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let br = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "<br/>");
        assert!(br.is_some());
        let ex = &br.unwrap().examples[0];
        assert!(
            ex.context_before
                .as_ref()
                .is_some_and(|c| c.contains("awful")),
            "context_before should contain text before match"
        );
        assert!(
            ex.context_after
                .as_ref()
                .is_some_and(|c| c.contains("acting")),
            "context_after should contain text after match"
        );
    }

    #[test]
    fn test_inline_across_multiple_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![
            make_text_record("f1.txt", "x &nbsp; y", temp_dir.path()),
            make_text_record("f2.txt", "a &nbsp; b", temp_dir.path()),
            make_text_record("f3.txt", "c &nbsp; d", temp_dir.path()),
        ];
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 2,
            ..RepeatedArtifactScanConfig::default()
        };
        let result =
            scan_repeated_artifacts(&records, &config, &CleaningConfig::default()).unwrap();
        let nbsp = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "&nbsp;");
        assert!(nbsp.is_some());
        assert_eq!(nbsp.unwrap().file_count, 3);
    }

    #[test]
    fn test_inline_candidate_ids_distinct() {
        let id1 = compute_stable_id(&RepeatedArtifactKind::InlineArtifact, "<br />");
        let id2 = compute_stable_id(&RepeatedArtifactKind::InlineArtifact, "&nbsp;");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_exact_lines_same_norm_different_ids() {
        let id1 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "Hello World 1");
        let id2 = compute_stable_id(&RepeatedArtifactKind::ExactLine, "Hello World 2");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_inline_candidate_ids_distinct_in_pipeline() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "café naïve <br /> texto &nbsp; mais\noutra linha &nbsp; aqui <br /> ali\n";
        let doc = make_text_record("inline_ids_pipeline.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let br_cand = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "<br />");
        let nbsp_cand = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "&nbsp;");
        assert!(br_cand.is_some(), "<br /> candidate should exist");
        assert!(nbsp_cand.is_some(), "&nbsp; candidate should exist");
        assert_ne!(
            br_cand.unwrap().candidate_id,
            nbsp_cand.unwrap().candidate_id,
            "distinct inline patterns must produce distinct candidate IDs"
        );
    }

    #[test]
    fn test_safe_context_before_non_ascii() {
        let text = "café naïve façade <br /> texto";
        let pos = text.find("<br />").unwrap();
        let ctx = safe_context_before(text, pos, 80);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("café"));
    }

    #[test]
    fn test_safe_context_after_non_ascii() {
        let text = "ação é ótima <br /> continuação";
        let pos = text.find("<br />").unwrap();
        let ctx = safe_context_after(text, pos, "<br />".len(), 80);
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("continuação"));
    }

    #[test]
    fn test_inline_context_utf8_safe() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "café naïve façade <br /> texto\n";
        let doc = make_text_record("utf8_inline.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let br = result
            .iter()
            .find(|c| c.kind == RepeatedArtifactKind::InlineArtifact && c.display_text == "<br />");
        assert!(br.is_some());
        let ex = &br.unwrap().examples[0];
        assert!(ex.context_before.is_some() || ex.context_after.is_some());
    }

    #[test]
    fn test_inline_disabled_does_not_detect() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "text <br/> more text\n";
        let doc = make_text_record("inline_off.txt", content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            include_inline_artifacts: false,
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };
        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();
        let has_inline = result
            .iter()
            .any(|c| c.kind == RepeatedArtifactKind::InlineArtifact);
        assert!(
            !has_inline,
            "inline detection should produce nothing when disabled"
        );
    }

    #[test]
    fn test_normalized_raw_variant_count_is_distinct() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = ["Page 1", "Page 2", "Page 2", "Page 2", "Page 2", "Page 1"].join("\n");
        let doc = make_text_record("variants.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            include_normalized_lines: true,
            include_exact_lines: true,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();

        let page_norm = result
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::NormalizedLine
                    && candidate.normalized_key == "page #"
            })
            .expect("expected page # normalised candidate");

        assert_eq!(page_norm.raw_variant_count, 2);
        assert!(!page_norm.raw_variant_count_is_capped);
    }

    #[test]
    fn test_raw_variant_count_capped_flag() {
        let temp_dir = tempfile::tempdir().unwrap();
        // The overflow branch needs more variants than RAW_VARIANT_TRACK_CAP can retain.
        let variant_count = RAW_VARIANT_TRACK_CAP + 10;
        let mut lines = Vec::with_capacity(variant_count);
        for i in 1..=variant_count {
            lines.push(format!("Page {}", i));
        }
        let content = lines.join("\n");
        let doc = make_text_record("capped.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            min_line_chars: 3,
            include_normalized_lines: true,
            include_exact_lines: false,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();

        let page_norm = result
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::NormalizedLine
                    && candidate.normalized_key == "page #"
            })
            .expect("expected page # normalised candidate");

        assert_eq!(page_norm.raw_variant_count, RAW_VARIANT_TRACK_CAP);
        assert!(page_norm.raw_variant_count_is_capped);
    }

    #[test]
    fn test_exact_line_raw_variant_count_one() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = ["Repeated Header"; 10].join("\n");
        let doc = make_text_record("exact_one.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            min_line_chars: 3,
            ..RepeatedArtifactScanConfig::default()
        };

        let result = scan_repeated_artifacts(&[doc], &config, &CleaningConfig::default()).unwrap();

        let exact_cand = result
            .iter()
            .find(|c| {
                c.kind == RepeatedArtifactKind::ExactLine && c.display_text == "Repeated Header"
            })
            .expect("expected exact line candidate 'Repeated Header'");

        assert_eq!(exact_cand.raw_variant_count, 1);
        assert!(!exact_cand.raw_variant_count_is_capped);
    }

    #[test]
    fn test_scan_report_contains_diagnostics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = ["Repeated Line"; 10].join("\n");
        let doc = make_text_record("diag.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 5,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[doc], &config, &CleaningConfig::default()).unwrap();

        assert_eq!(report.diagnostics.files_requested, 1);
        assert_eq!(report.diagnostics.files_scanned, 1);
        assert!(report.diagnostics.total_raw_lines > 0);
        assert!(report.diagnostics.total_candidate_keys_before_filtering > 0);
        assert_eq!(report.diagnostics.final_candidates, report.candidates.len());
    }

    #[test]
    fn test_scan_report_min_files_filter_diagnostics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = ["Repeated Line"; 10].join("\n");
        let doc = make_text_record("diag2.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[doc], &config, &CleaningConfig::default()).unwrap();

        assert!(report.diagnostics.candidates_after_min_occurrences > 0);
        assert_eq!(report.diagnostics.candidates_after_min_files, 0);
        assert_eq!(report.diagnostics.final_candidates, 0);
    }

    #[test]
    fn test_scan_report_empty_extraction_diagnostics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_text_record("empty.txt", "", temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 1,
            min_files: 1,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[doc], &config, &CleaningConfig::default()).unwrap();

        assert_eq!(report.diagnostics.files_empty_after_extraction, 1);
        assert_eq!(report.diagnostics.total_raw_lines, 0);
    }

    #[test]
    fn test_repeated_artifacts_with_cache_equals_without_cache() {
        let temp_dir = tempfile::tempdir().unwrap();

        let content1 =
            "Header\nBody line A\nBody line B\nFooter\nHeader\nBody line C\nBody line D\nFooter\n";
        let content2 = "Header\nBody line X\nBody line Y\nFooter\nHeader\nBody line Z\nFooter\n";

        let records = vec![
            make_text_record("file1.txt", content1, temp_dir.path()),
            make_text_record("file2.txt", content2, temp_dir.path()),
        ];

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            ..RepeatedArtifactScanConfig::default()
        };

        let cleaning_config = CleaningConfig::default();
        let cancel = no_cancellation();

        let report_no_cache = scan_repeated_artifacts_report_with_cancel_and_cache(
            &records,
            &config,
            &cleaning_config,
            None,
            &cancel,
        )
        .expect("scan without cache should succeed");

        let cache = ExtractionCache::new();
        let report_with_cache_first = scan_repeated_artifacts_report_with_cancel_and_cache(
            &records,
            &config,
            &cleaning_config,
            Some(&cache),
            &cancel,
        )
        .expect("scan with cache (first) should succeed");

        let report_with_cache_second = scan_repeated_artifacts_report_with_cancel_and_cache(
            &records,
            &config,
            &cleaning_config,
            Some(&cache),
            &cancel,
        )
        .expect("scan with cache (second) should succeed");

        assert_eq!(
            report_no_cache.candidates.len(),
            report_with_cache_first.candidates.len(),
            "candidate count should match between cache and no-cache (first)"
        );
        assert_eq!(
            report_no_cache.candidates.len(),
            report_with_cache_second.candidates.len(),
            "candidate count should match between cache and no-cache (second)"
        );

        // Rayon can produce different candidate order, so compare by stable ID.
        let mut no_cache_sorted: Vec<_> = report_no_cache.candidates.iter().collect();
        no_cache_sorted.sort_by(|a, b| a.candidate_id.cmp(&b.candidate_id));

        for (report_candidates, label) in [
            (&report_with_cache_first.candidates, "first cache pass"),
            (&report_with_cache_second.candidates, "second cache pass"),
        ] {
            let mut cache_sorted: Vec<_> = report_candidates.iter().collect();
            cache_sorted.sort_by(|a, b| a.candidate_id.cmp(&b.candidate_id));

            assert_eq!(
                no_cache_sorted.len(),
                cache_sorted.len(),
                "candidate count mismatch for {}",
                label
            );

            for (nc, cc) in no_cache_sorted.iter().zip(cache_sorted.iter()) {
                assert_eq!(
                    nc.candidate_id, cc.candidate_id,
                    "candidate_id mismatch for {}",
                    label
                );
                assert_eq!(
                    nc.kind, cc.kind,
                    "kind mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.display_text, cc.display_text,
                    "display_text mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.normalized_key, cc.normalized_key,
                    "normalized_key mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.occurrence_count, cc.occurrence_count,
                    "occurrence_count mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.file_count, cc.file_count,
                    "file_count mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.position_summary, cc.position_summary,
                    "position_summary mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.risk_label, cc.risk_label,
                    "risk_label mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.content_class, cc.content_class,
                    "content_class mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.raw_variants, cc.raw_variants,
                    "raw_variants mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.raw_variant_count, cc.raw_variant_count,
                    "raw_variant_count mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
                assert_eq!(
                    nc.raw_variant_count_is_capped, cc.raw_variant_count_is_capped,
                    "raw_variant_count_is_capped mismatch for candidate {} in {}",
                    nc.candidate_id, label
                );
            }
        }

        assert!(!cache.is_empty(), "cache should have entries after scan");

        assert_eq!(
            report_no_cache.diagnostics.files_failed_extraction,
            report_with_cache_first.diagnostics.files_failed_extraction
        );
        assert_eq!(
            report_no_cache.diagnostics.files_scanned,
            report_with_cache_first.diagnostics.files_scanned
        );
    }

    #[test]
    fn test_repeated_artifacts_with_cache_preserves_raw_variants() {
        let temp_dir = tempfile::tempdir().unwrap();

        let content_parts = ["Page 1", "Page 2", "Page 1", "Page 3", "Page 2", "Page 1"];
        let content = content_parts.join("\n");
        let doc = make_text_record("multivariant.txt", &content, temp_dir.path());

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            include_exact_lines: true,
            include_normalized_lines: true,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let cleaning_config = CleaningConfig::default();
        let cancel = no_cancellation();
        let cache = ExtractionCache::new();

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc],
            &config,
            &cleaning_config,
            Some(&cache),
            &cancel,
        )
        .expect("scan should succeed");

        let page_norm = report.candidates.iter().find(|c| {
            c.kind == RepeatedArtifactKind::NormalizedLine && c.normalized_key == "page #"
        });

        assert!(
            page_norm.is_some(),
            "normalised page # candidate should exist"
        );
        let page_norm = page_norm.unwrap();
        assert!(
            page_norm.raw_variant_count >= 2,
            "should have at least 2 raw variants, got {}",
            page_norm.raw_variant_count
        );
        assert!(
            !page_norm.raw_variants.is_empty(),
            "raw_variants should not be empty"
        );
        for expected_variant in &["Page 1", "Page 2", "Page 3"] {
            assert!(
                page_norm
                    .raw_variants
                    .contains(&expected_variant.to_string()),
                "raw_variants should contain '{}', got {:?}",
                expected_variant,
                page_norm.raw_variants
            );
        }
    }

    fn make_pdf_record(name: &str, temp_dir: &std::path::Path) -> DocumentRecord {
        let bytes = b"%PDF-1.4\n% synthetic placeholder for cached OCR regression\n";
        let file_path = temp_dir.join(name);
        std::fs::write(&file_path, bytes).unwrap();
        DocumentRecord {
            source_path: file_path,
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Pdf,
            size_bytes: bytes.len() as u64,
        }
    }

    fn force_ocr_cleaning_config() -> CleaningConfig {
        CleaningConfig {
            pdf_text_source: crate::clean::PdfTextSource::ForceOcr,
            pdf_ocr_quality: crate::clean::PdfOcrQuality::HighQuality,
            ..CleaningConfig::default()
        }
    }

    #[test]
    fn test_repeated_artifacts_original_mode_uses_materialised_ocr_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("cached-ocr.pdf", temp_dir.path());
        let cleaning_config = force_ocr_cleaning_config();
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "Header\nbody line\nHeader\nother body line\nHeader".to_string(),
                warnings: Vec::new(),
                page_count: Some(1),
                page_texts: None,
            },
        );

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc],
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .expect("scan should use cached OCR text");

        assert_eq!(report.diagnostics.files_empty_after_extraction, 0);
        assert!(report.diagnostics.total_raw_lines > 0);
        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine && candidate.display_text == "Header"
        }));
    }

    #[test]
    fn test_repeated_artifacts_processed_mode_cleans_materialised_ocr_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("processed-cached-ocr.pdf", temp_dir.path());
        let mut cleaning_config = force_ocr_cleaning_config();
        cleaning_config.remove_patterns = vec!["Header".to_string()];
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "Header\nKept line\nHeader\nKept line\nHeader".to_string(),
                warnings: Vec::new(),
                page_count: Some(1),
                page_texts: None,
            },
        );

        let config = RepeatedArtifactScanConfig {
            analyse_processed_text: true,
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc],
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .expect("processed scan should use cleaned cached OCR text");

        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine
                && candidate.display_text == "Kept line"
        }));
        assert!(!report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine && candidate.display_text == "Header"
        }));
    }

    #[test]
    fn test_scanner_uses_cleaned_page_texts_when_available() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("cleaned-pages-scan.pdf", temp_dir.path());
        let cleaning_config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "HEADER\n\nBODY".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["HEADER".to_string(), "BODY".to_string()]),
            },
        );

        let config = RepeatedArtifactScanConfig {
            analyse_processed_text: true,
            min_occurrences: 1,
            min_files: 1,
            min_line_chars: 3,
            include_exact_lines: true,
            ..RepeatedArtifactScanConfig::default()
        };

        let ft = phase1_scan_file(
            &doc,
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .ft;

        assert!(ft.has_page_metadata);
        assert_eq!(ft.document.pages.len(), 2);
        assert_eq!(ft.document.pages[0].lines[0].text, "header");
        assert_eq!(ft.document.pages[1].lines[0].text, "body");
    }

    #[test]
    fn test_scanner_falls_back_to_flat_text_when_cleaned_page_texts_are_unavailable() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc = make_pdf_record("no-cleaned-pages-scan.pdf", temp_dir.path());
        let cleaning_config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &doc,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "Alpha\nBeta".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["Alpha".to_string(), "Beta".to_string()]),
            },
        );

        let config = RepeatedArtifactScanConfig {
            analyse_processed_text: true,
            min_occurrences: 1,
            min_files: 1,
            min_line_chars: 3,
            include_exact_lines: true,
            ..RepeatedArtifactScanConfig::default()
        };

        let ft = phase1_scan_file(
            &doc,
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .ft;

        assert!(!ft.has_page_metadata);
        assert_eq!(ft.document.pages.len(), 1);
    }

    #[test]
    fn test_position_summary_is_page_based_flag() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cleaning_config = CleaningConfig::default();
        let cache = ExtractionCache::new();

        // Document 1: has page texts (page based)
        let doc_page_based = make_pdf_record("page-based.pdf", temp_dir.path());
        cache.insert_extracted(
            &doc_page_based,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "Header line\nbody line\nHeader line\nother body\nHeader line"
                    .to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec![
                    "Header line\nbody line".to_string(),
                    "Header line\nother body\nHeader line".to_string(),
                ]),
            },
        );

        // Document 2: fallback (flat text only, not page based)
        let doc_flat = make_pdf_record("flat-based.pdf", temp_dir.path());
        cache.insert_extracted(
            &doc_flat,
            Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                &cleaning_config,
            )),
            &cleaning_config,
            crate::cache::CacheEntry {
                extracted_text: "Header line\nbody line\nHeader line\nother body\nHeader line"
                    .to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: None, // No page texts
            },
        );

        let config = RepeatedArtifactScanConfig {
            min_occurrences: 2,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        // Scan page-based document only
        let report_page_based = scan_repeated_artifacts_report_with_cancel_and_cache(
            std::slice::from_ref(&doc_page_based),
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .unwrap();

        let cand_page_based = report_page_based
            .candidates
            .iter()
            .find(|c| c.display_text == "Header line")
            .unwrap();
        assert!(cand_page_based.position_summary_is_page_based);

        // Scan flat document only
        let report_flat = scan_repeated_artifacts_report_with_cancel_and_cache(
            std::slice::from_ref(&doc_flat),
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .unwrap();

        let cand_flat = report_flat
            .candidates
            .iter()
            .find(|c| c.display_text == "Header line")
            .unwrap();
        assert!(!cand_flat.position_summary_is_page_based);

        // Scan both mixed: page-based and flat. The flag must logically AND to false.
        let report_mixed = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[doc_page_based, doc_flat],
            &config,
            &cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .unwrap();

        let cand_mixed = report_mixed
            .candidates
            .iter()
            .find(|c| c.display_text == "Header line")
            .unwrap();
        assert!(!cand_mixed.position_summary_is_page_based);
    }
}
