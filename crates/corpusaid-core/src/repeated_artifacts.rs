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

// ---- Utility functions ----

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

/// Detects punctuation for candidate normalization.
fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '-' | '—'
            | '–'
            | '['
            | ']'
            | '('
            | ')'
            | '{'
            | '}'
            | '.'
            | ','
            | '*'
            | '/'
            | '\\'
            | '_'
            | '|'
            | '•'
            | '°'
    )
}

/// Normalizes a line deterministically for candidate grouping.
/// Strips surrounding punctuation, lowercases, collapses whitespace, and replaces digit runs with '#'.
fn normalize_line(s: &str) -> String {
    let trimmed = s.trim();
    let chars: Vec<char> = trimmed.chars().collect();

    let mut i = 0;
    while i < chars.len() && is_punctuation(chars[i]) {
        i += 1;
    }
    let start = i;

    let mut j = chars.len();
    while j > start && is_punctuation(chars[j - 1]) {
        j -= 1;
    }
    let end = j;

    if start >= end {
        return String::new();
    }

    let substring: String = chars[start..end].iter().collect();
    let trimmed_sub = substring.trim();
    let lower = trimmed_sub.to_lowercase();

    use regex::Regex;
    use std::sync::LazyLock;
    static RE_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let with_replaced_digits = RE_DIGITS.replace_all(&lower, "#");
    let collapsed_spaces = RE_SPACES.replace_all(&with_replaced_digits, " ");

    collapsed_spaces.trim().to_string()
}

/// Computes the layout position of a line using per-line page metadata from `FileText`.
/// For PDFs (page_num is Some): uses line-in-page index and total-lines-on-page for
/// top/bottom detection (first/last 3 lines of each page).
/// For non-PDFs: falls back to file-level 10% rule.
fn compute_position_for_line(ft: &FileText, line_idx: usize) -> LinePosition {
    if let Some(&(page_num_opt, line_in_page, page_total)) = ft.page_line_info.get(line_idx)
        && page_num_opt.is_some()
    {
        // Page-aware: first 3 / last 3 lines of this page
        if line_in_page < 3 {
            return LinePosition::Top;
        } else if line_in_page >= page_total.saturating_sub(3) {
            return LinePosition::Bottom;
        } else {
            return LinePosition::Middle;
        }
    }
    // Fallback to file-level position
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

    // 1. Common Academic Headings
    let common_headings = [
        "abstract",
        "introduction",
        "methods",
        "results",
        "discussion",
        "conclusion",
        "references",
        "bibliography",
        "appendix",
    ];
    if common_headings.contains(&lower_key.as_str()) {
        return ArtifactRiskLabel::CommonSectionHeadingReviewCarefully;
    }

    // 2. Symbol/Noise
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

    // 3. Strong Header/Footer
    let pos_ratio = if occurrence_count > 0 {
        (top_count + bottom_count) as f64 / occurrence_count as f64
    } else {
        0.0
    };
    if occurrence_count >= 3 && pos_ratio >= 0.75 {
        return ArtifactRiskLabel::StrongHeaderFooterCandidate;
    }

    // 4. Possible Boilerplate
    if file_count >= 2 {
        return ArtifactRiskLabel::PossibleBoilerplate;
    }

    ArtifactRiskLabel::Ambiguous
}

/// Classifies a text line's content into one of four categories.
///
/// Classification rules (applied in order):
/// 1. If >=50% non-alphanumeric symbols → SymbolNoiseDominant
/// 2. If >=60% digits (and >0 digits) → NumericDominant
/// 3. If both alphabetic and digits exist, and alphabetic ratio >= digit ratio → MixedTextNumbers
/// 4. If digit count > alpha count (below 60% threshold) → NumericDominant
/// 5. Default → TextDominant
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
    let digit_count = chars.iter().filter(|c| c.is_ascii_digit()).count() as f64;
    let symbol_count = chars
        .iter()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count() as f64;

    let symbol_ratio = symbol_count / total;
    let digit_ratio = digit_count / total;
    let alpha_ratio = alpha_count / total;

    // 1. Symbol/noise dominant
    if symbol_ratio >= 0.50 {
        return CandidateContentClass::SymbolNoiseDominant;
    }

    // 2. Numeric dominant
    if digit_ratio >= 0.60 && digit_count > 0.0 {
        return CandidateContentClass::NumericDominant;
    }

    // 3. Mixed text + numbers: some of both, alphabetic >= digit
    if alpha_ratio > 0.0 && digit_count > 0.0 && alpha_ratio >= digit_ratio {
        return CandidateContentClass::MixedTextNumbers;
    }

    // 4. Digit-dominant but below 60% threshold — still numeric-dominant if digits dominate alpha
    if digit_count > alpha_count && digit_ratio > 0.0 {
        return CandidateContentClass::NumericDominant;
    }

    // 5. Mostly text
    CandidateContentClass::TextDominant
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

// ---- Inline artefact detection ----

/// Known inline markup/conversion patterns.
const KNOWN_INLINE_PATTERNS: &[&str] = &[
    "<br/>", "<br>", "<br />", "<BR/>", "<BR>", "<BR />", "&nbsp;", "&amp;", "&lt;", "&gt;",
    "&quot;", "&apos;",
];

/// Detects inline artefacts in the given file text.
/// Returns a per-file local aggregated map.
fn detect_inline_artifacts(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
) -> HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> {
    let mut map: HashMap<(RepeatedArtifactKind, String), LocalCandidateStats> = HashMap::new();

    if !config.include_inline_artifacts {
        return map;
    }

    for (line_idx, line) in ft.raw_lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        for &pattern in KNOWN_INLINE_PATTERNS {
            let mut search_start = 0;
            while let Some(pos) = line[search_start..].find(pattern) {
                let abs_pos = search_start + pos;
                search_start = abs_pos + pattern.len();

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
                });

                entry.occurrence_count += 1;
                let pos_class = compute_position_for_line(ft, line_idx);
                match pos_class {
                    LinePosition::Top => entry.top_count += 1,
                    LinePosition::Middle => entry.middle_count += 1,
                    LinePosition::Bottom => entry.bottom_count += 1,
                }

                if entry.example_refs.len() < config.max_examples_per_candidate {
                    // For inline, store (line_idx, char_pos) — char_pos used as context offset
                    entry.example_refs.push((line_idx, abs_pos));
                }
            }
        }
    }

    map
}

/// Build the PDF extraction options used by repeated artefact scanning.
///
/// OCR is always `false` regardless of `cleaning_config`.
/// When `analyse_processed_text` is true, uses the user's config for cleanup flags
/// and strategy; otherwise uses PdfiumFlat and all cleanup flags disabled.
fn build_repeated_artifact_pdf_options(
    analyse_processed_text: bool,
    cleaning_config: &CleaningConfig,
) -> PdfExtractionOptions {
    if analyse_processed_text {
        PdfExtractionOptions {
            strategy: cleaning_config.pdf_embedded_text_strategy,
            use_ocr: false,
            remove_repeated_headers_footers: cleaning_config.remove_repeated_pdf_headers_footers,
            remove_page_labels: cleaning_config.remove_pdf_page_labels,
            remove_symbol_heavy_artifacts: cleaning_config.remove_pdf_symbol_heavy_artifacts,
            remove_code_like_blocks: cleaning_config.remove_pdf_code_like_blocks,
            remove_formula_like_lines: cleaning_config.remove_pdf_formula_like_lines,
        }
    } else {
        PdfExtractionOptions {
            strategy: crate::clean::PdfEmbeddedTextStrategy::PdfiumFlat,
            use_ocr: false,
            remove_repeated_headers_footers: false,
            remove_page_labels: false,
            remove_symbol_heavy_artifacts: false,
            remove_code_like_blocks: false,
            remove_formula_like_lines: false,
        }
    }
}

/// Build the DOCX extraction config used by repeated artefact scanning.
fn build_repeated_artifact_docx_config(
    analyse_processed_text: bool,
    cleaning_config: &CleaningConfig,
) -> CleaningConfig {
    if analyse_processed_text {
        cleaning_config.clone()
    } else {
        CleaningConfig::default()
    }
}

/// Apply post-extraction processing (HTML extraction + cleaning) that is
/// currently done after text extraction when `analyse_processed_text` is true.
/// This preserves the exact same ordering as the non-cache path.
fn apply_scan_post_processing(raw_text: &str, cleaning_config: &CleaningConfig) -> String {
    let temp = if cleaning_config.extract_html {
        crate::html::extract_html(raw_text)
    } else {
        raw_text.to_string()
    };
    crate::clean::clean_text(&temp, cleaning_config)
}

/// Internal struct describing extraction status for a single file.
struct ExtractedScanText {
    text: String,
    extraction_failed: bool,
}

/// Extracts text and records whether extraction failed (vs. succeeded with possibly empty text).
///
/// If a cache is provided, uses `cache.get_or_extract` to avoid re-extracting documents
/// that have been previously extracted with compatible options. Post-extraction cleaning
/// is applied *after* cache retrieval to preserve the existing order (cache stores raw
/// extracted text only).
fn get_document_text_with_status(
    record: &DocumentRecord,
    analyse_processed_text: bool,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> ExtractedScanText {
    // ── Cache path ───────────────────────────────────────────────────────
    if let Some(cache) = cache {
        let pdf_opts = if record.document_type == DocumentType::Pdf {
            Some(build_repeated_artifact_pdf_options(
                analyse_processed_text,
                cleaning_config,
            ))
        } else {
            None
        };
        let docx_config =
            build_repeated_artifact_docx_config(analyse_processed_text, cleaning_config);

        match cache.get_or_extract(record, pdf_opts, &docx_config) {
            Ok(entry) => {
                let processed_text = if analyse_processed_text {
                    apply_scan_post_processing(&entry.extracted_text, cleaning_config)
                } else {
                    entry.extracted_text
                };
                return ExtractedScanText {
                    text: processed_text,
                    extraction_failed: false,
                };
            }
            Err(_) => {
                return ExtractedScanText {
                    text: String::new(),
                    extraction_failed: true,
                };
            }
        }
    }

    // ── Direct extraction path (no cache) ────────────────────────────────
    let source_bytes = match std::fs::read(&record.source_path) {
        Ok(b) => b,
        Err(_) => {
            return ExtractedScanText {
                text: String::new(),
                extraction_failed: true,
            };
        }
    };

    let (raw_text, extraction_failed) = match record.document_type {
        DocumentType::Docx => {
            let cfg = build_repeated_artifact_docx_config(analyse_processed_text, cleaning_config);
            match crate::docx::extract_docx(&source_bytes, &cfg) {
                Ok(extracted) => (extracted.text, false),
                Err(_) => (String::new(), true),
            }
        }
        DocumentType::Pdf => {
            let pdf_opts =
                build_repeated_artifact_pdf_options(analyse_processed_text, cleaning_config);
            match crate::pdf::extract_pdf(&source_bytes, None, pdf_opts) {
                Ok(extracted) => (extracted.text, false),
                Err(_) => (String::new(), true),
            }
        }
        _ => (String::from_utf8_lossy(&source_bytes).into_owned(), false),
    };

    let processed_text = if analyse_processed_text {
        apply_scan_post_processing(&raw_text, cleaning_config)
    } else {
        raw_text
    };

    ExtractedScanText {
        text: processed_text,
        extraction_failed,
    }
}

/// Build a per-file `FileText` from extracted text and page info.
fn build_file_text(file_idx: usize, record: &DocumentRecord, text: &str) -> FileText {
    let file_name = record
        .source_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    let file_path = record.relative_path.to_string_lossy().into_owned();

    let is_pdf = record.document_type == DocumentType::Pdf;

    // Build raw lines and page boundary info.
    // For PDFs we split on "\n\n" to approximate pages.
    let mut raw_lines: Vec<String> = Vec::new();
    let mut page_nums: Vec<Option<usize>> = Vec::new();

    if is_pdf {
        let page_chunks: Vec<&str> = text.split("\n\n").collect();
        for (p_idx, chunk) in page_chunks.iter().enumerate() {
            for line in chunk.lines() {
                raw_lines.push(line.to_string());
                page_nums.push(Some(p_idx + 1));
            }
        }
    } else {
        for line in text.lines() {
            raw_lines.push(line.to_string());
        }
        page_nums.resize(raw_lines.len(), None);
    }

    let total_lines = raw_lines.len();

    // Build page_line_info aligned with raw_lines
    let mut page_line_info: Vec<(Option<usize>, usize, usize)> =
        Vec::with_capacity(raw_lines.len());
    if is_pdf {
        let page_chunks: Vec<&str> = text.split("\n\n").collect();
        for (p_idx, chunk) in page_chunks.iter().enumerate() {
            let page_lines: Vec<&str> = chunk.lines().collect();
            let total_on_page = page_lines.len();
            for (line_in_page, _line) in page_lines.iter().enumerate() {
                page_line_info.push((Some(p_idx + 1), line_in_page, total_on_page));
            }
        }
    } else {
        for idx in 0..raw_lines.len() {
            page_line_info.push((None, idx, raw_lines.len()));
        }
    }

    FileText {
        file_idx,
        file_name,
        file_path,
        raw_lines,
        total_lines,
        page_nums,
        page_line_info,
    }
}

/// Phase 1 (parallel): each file builds an aggregated local map.
/// Returns Phase1Result (never None — always produces a structured result).
fn phase1_scan_file(
    record: &DocumentRecord,
    file_idx: usize,
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    cancel: &CancellationFlag,
) -> Phase1Result {
    // Check cancellation before extraction
    if cancel.load(Ordering::Relaxed) {
        return Phase1Result {
            ft: build_file_text(file_idx, record, ""),
            local_map: HashMap::new(),
            extraction_failed: false,
        };
    }

    let extracted = get_document_text_with_status(
        record,
        config.analyse_processed_text,
        cleaning_config,
        cache,
    );

    // Check cancellation after extraction
    if cancel.load(Ordering::Relaxed) {
        return Phase1Result {
            ft: build_file_text(file_idx, record, ""),
            local_map: HashMap::new(),
            extraction_failed: false,
        };
    }

    let ft = build_file_text(file_idx, record, &extracted.text);

    let mut map = phase1_aggregate(&ft, config);

    // Detect inline artefacts and merge into the same local map
    let inline_map = detect_inline_artifacts(&ft, config);
    for ((kind, key), inline_stats) in inline_map {
        map.insert((kind, key), inline_stats);
    }

    Phase1Result {
        ft,
        local_map: map,
        extraction_failed: extracted.extraction_failed,
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

    // First pass: pre-compute valid entries with content class.
    // Normalise once per line; reuse for exact + normalised.
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

    // Helper to insert/update a local aggregated entry.
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
        });
        entry.occurrence_count += 1;
        match pos {
            LinePosition::Top => entry.top_count += 1,
            LinePosition::Middle => entry.middle_count += 1,
            LinePosition::Bottom => entry.bottom_count += 1,
        }
        // Set display text on first occurrence
        if entry.display_text.is_empty() {
            entry.display_text = display.clone();
        }
        // Bounded examples
        if entry.example_refs.len() < config.max_examples_per_candidate {
            entry.example_refs.push((rs, re));
        }
        // Track distinct raw variants using a bounded set.
        if entry.raw_variants.len() < RAW_VARIANT_TRACK_CAP {
            entry.raw_variants.insert(display.clone());
        } else if !entry.raw_variants.contains(&display) {
            entry.raw_variant_overflow = true;
        }
    };

    // --- Exact lines (filter by content class) ---
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

    // --- Normalised lines (reuse norm from valid_entries) ---
    // SAFETY: NumericDominant lines are NOT used for normalised grouping
    // unless explicitly enabled (include_numeric_dominant).
    if config.include_normalized_lines {
        for &(idx, ref line, ref norm, cc) in &valid_entries {
            if norm.is_empty() {
                continue;
            }
            // Skip numeric-dominant lines for normalised grouping unless enabled
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

    // --- 2-line blocks (opt-in) ---
    if config.include_two_line_blocks && valid_entries.len() >= 2 {
        for w in valid_entries.windows(2) {
            let (i0, ref l0, ref n0, cc0) = w[0];
            let (i1, ref l1, ref n1, cc1) = w[1];
            // Must be contiguous in raw lines and same page
            if i1 != i0 + 1 {
                continue;
            }
            if ft.page_nums.get(i0).copied().flatten() != ft.page_nums.get(i1).copied().flatten() {
                continue;
            }
            let text_block = format!("{}\n{}", l0, l1);
            let norm_block = format!("{}\n{}", n0, n1);
            // Classify block: use the dominant class of its lines
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

    // --- 3-line blocks (opt-in) ---
    if config.include_three_line_blocks && valid_entries.len() >= 3 {
        for w in valid_entries.windows(3) {
            let (i0, ref l0, ref n0, cc0) = w[0];
            let (i1, ref l1, ref n1, cc1) = w[1];
            let (i2, ref l2, ref n2, cc2) = w[2];
            if i1 != i0 + 1 || i2 != i1 + 1 {
                continue;
            }
            if ft.page_nums.get(i0).copied().flatten() != ft.page_nums.get(i1).copied().flatten()
                || ft.page_nums.get(i0).copied().flatten()
                    != ft.page_nums.get(i2).copied().flatten()
            {
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

/// Phase 2: merge local aggregated maps into global CountEntry map.
fn phase2_merge(
    all_maps: Vec<HashMap<(RepeatedArtifactKind, String), LocalCandidateStats>>,
    config: &RepeatedArtifactScanConfig,
) -> HashMap<(RepeatedArtifactKind, String), CountEntry> {
    let mut global: HashMap<(RepeatedArtifactKind, String), CountEntry> = HashMap::new();

    // Track file index for compact file IDs
    // Because each local map belongs to exactly one file, we can
    // increment file_count once per key per local map without per-occurrence checks.
    for (_file_idx, local_map) in all_maps.into_iter().enumerate() {
        for ((kind, key), stats) in local_map {
            // Only one entry per key per file now — merge directly.
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
                });

            entry.occurrence_count += stats.occurrence_count;
            entry.top_count += stats.top_count;
            entry.middle_count += stats.middle_count;
            entry.bottom_count += stats.bottom_count;
            entry.unknown_count += stats.unknown_count;

            // File counting: each local map = one file. Just push if not already tracked.
            let fid = _file_idx as u32;
            // Since keys appear at most once per file, we can just push unconditionally
            // because each local_map corresponds to a different _file_idx.
            // But to be safe, avoid duplicates (shouldn't happen, but be robust).
            if !entry.file_ids.contains(&fid) {
                entry.file_ids.push(fid);
            }

            // Merge example refs (bounded)
            for (rs, re) in &stats.example_refs {
                if entry.example_refs.len() < config.max_examples_per_candidate {
                    entry.example_refs.push((fid, *rs, *re));
                }
            }

            // Merge distinct raw variants from local stats into global entry.
            // Use capped set semantics: insert up to RAW_VARIANT_TRACK_CAP, then set overflow.
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

/// Phase 3: filter, score, rank, deduplicate, collect final examples.
/// Returns (candidates, diagnostics_counts).
fn phase3_finalize(
    global: HashMap<(RepeatedArtifactKind, String), CountEntry>,
    file_texts: &[FileText],
    config: &RepeatedArtifactScanConfig,
) -> Vec<RepeatedArtifactCandidate> {
    let mut scored: Vec<(f64, RepeatedArtifactCandidate)> = Vec::new();

    // Collect all entries first, then deduplicate norm vs exact.
    let mut entries: Vec<CountEntry> = global
        .into_values()
        .filter(|e| {
            e.occurrence_count >= config.min_occurrences && e.file_ids.len() >= config.min_files
        })
        .collect();

    // Dedup: remove NormalizedLine entries that add no value.
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

    // Now score and build candidates
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

        // Determine content class from the display text
        let content_class = classify_content(&entry.display_text);

        let raw_variant_count = entry.raw_variants.len();
        let raw_variant_count_is_capped = entry.raw_variant_overflow;

        // Collect examples from file_texts using stored refs.
        // Different handling for inline vs line/block candidates.
        let is_inline = entry.kind == RepeatedArtifactKind::InlineArtifact;
        let examples: Vec<RepeatedArtifactExample> = entry
            .example_refs
            .iter()
            .filter_map(|&(fid, rs, re)| {
                let ft = file_texts.get(fid as usize)?;

                if is_inline {
                    // rs = line_idx, re = char position within line
                    let line_idx = rs;
                    let char_pos = re;
                    let line = ft.raw_lines.get(line_idx)?;
                    let pattern = &entry.display_text;
                    let plen = pattern.len();

                    let context_before = safe_context_before(line, char_pos, 80);
                    let context_after = safe_context_after(line, char_pos, plen, 80);
                    let matched_text = pattern.to_string();

                    Some(RepeatedArtifactExample {
                        file_name: ft.file_name.clone(),
                        file_path: ft.file_path.clone(),
                        line_number: Some(line_idx + 1),
                        page_number: ft.page_nums.get(line_idx).copied().flatten(),
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
                        page_number: ft.page_nums.get(rs).copied().flatten(),
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
                position_summary: PositionSummary {
                    top_count: entry.top_count,
                    middle_count: entry.middle_count,
                    bottom_count: entry.bottom_count,
                    unknown_count: entry.unknown_count,
                },
                risk_label,
                content_class,
                raw_variant_count,
                raw_variant_count_is_capped,
                raw_variants: entry.raw_variants.into_iter().collect(),
                examples,
            },
        ));
    }

    // Sort by suspicion score descending
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
    let custom_removals_active = cleaning_config.remove_patterns.len();
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

    // Phase 1: parallel extraction + per-file aggregation.
    // Every file produces a Phase1Result (no filter_map, no dropping).
    let phase_results: Vec<Phase1Result> = records
        .par_iter()
        .enumerate()
        .map(|(idx, record)| phase1_scan_file(record, idx, config, cleaning_config, cache, cancel))
        .collect();

    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".to_string());
    }

    // Count extraction diagnostics
    let files_scanned = phase_results.len();
    let files_failed_extraction = phase_results.iter().filter(|r| r.extraction_failed).count();

    let file_texts: Vec<FileText> = phase_results
        .iter()
        .map(|pr| FileText {
            file_idx: pr.ft.file_idx,
            file_name: pr.ft.file_name.clone(),
            file_path: pr.ft.file_path.clone(),
            raw_lines: pr.ft.raw_lines.clone(),
            total_lines: pr.ft.total_lines,
            page_nums: pr.ft.page_nums.clone(),
            page_line_info: pr.ft.page_line_info.clone(),
        })
        .collect();

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

    // Phase 2: merge
    let global = phase2_merge(all_maps, config);

    if cancel.load(Ordering::Relaxed) {
        return Err("Scan cancelled.".to_string());
    }

    // --- Filtering stage counts for diagnostics ---
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

    // Phase 3: finalize
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

