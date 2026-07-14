//! Finds corpus-wide boilerplate, headers, footers, and graphical noise with bounded memory.
//!
//! The first pass counts compact fingerprints; the second materialises evidence only for surviving
//! candidates. Original extracted text is the default scan input, while processed-text mode applies
//! the active preparation configuration. Candidate order remains deterministic across both passes.

pub mod promotion;

use crate::corpus::DocumentRecord;
use crate::preparation::cleaning::CleaningConfig;
use crate::preparation::repeated_artifacts::promotion::{
    RepeatedArtifactPromotion, promotion_for_candidate,
};
use crate::sources::cache::{DocumentTextMode, ExtractionCache, document_text_for_record};
use crate::sources::structured_document::StructuredDocument;
use crate::text::normalisation::normalize_line_for_repeated_artifact as normalize_line;
use crate::text::profile::{
    KNOWN_INLINE_PATTERNS, TextSignalContext, classify_text_signal, is_common_section_heading,
    is_decimal_digit, is_page_label_like, profile_text_line, text_signal_reasons,
};
use aho_corasick::AhoCorasick;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, Ordering},
};
use ts_rs::TS;

pub use crate::text::profile::{CandidateTextProfile, CandidateTextSignalLabel};

/// Maximum distinct raw variants tracked per candidate key.
/// Beyond this cap, `raw_variant_overflow` is set to true.
const RAW_VARIANT_TRACK_CAP: usize = 200;

/// Default accumulation ceiling for each repeated-artefact scan phase.
const DEFAULT_MEMORY_CEILING_BYTES: usize = 1_288_490_188;

/// Conservative pass-1 accounting for a sharded hash-map entry, including load-factor overhead.
const PASS1_ESTIMATED_BYTES_PER_KEY: usize = 80;

/// Fixed accounting before the variable strings and example payloads of a survivor.
const MATERIALIZED_ESTIMATED_BASE_BYTES_PER_KEY: usize = 512;

/// Small shards keep fallible hash-map growth allocations far below the corpus-wide map size.
const FINGERPRINT_MAP_SHARDS: usize = 256;

/// Cancellation is sampled at least this often while scanning a large file.
const CANCELLATION_CHECK_LINE_INTERVAL: usize = 1_024;

fn default_memory_ceiling_bytes() -> usize {
    DEFAULT_MEMORY_CEILING_BYTES
}

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
    /// 2-line block detection is enabled by default for recall-first review.
    pub include_two_line_blocks: bool,
    /// 3-line block detection is more expensive; disabled by default. Opt-in only.
    pub include_three_line_blocks: bool,
    /// Include candidates whose content is predominantly text.
    pub include_text_dominant: bool,
    /// Include candidates with a mix of text and numbers (e.g. "Page 12", "Chapter 5").
    pub include_mixed_text_numbers: bool,
    /// Include numeric-dominant candidates; the review UI separates these for dismissal.
    pub include_numeric_dominant: bool,
    /// Include symbol/noise-dominant candidates (extraction junk markers).
    pub include_symbol_noise: bool,
    pub min_occurrences: usize,
    pub min_files: usize,
    /// Optional returned-candidate cap. `None` returns every qualifying candidate.
    pub max_candidates: Option<usize>,
    pub max_examples_per_candidate: usize,
    pub min_line_chars: usize,
    pub max_line_chars: usize,
    /// Maximum estimated bytes used by pass-1 accounting or survivor materialisation.
    #[serde(default = "default_memory_ceiling_bytes")]
    pub memory_ceiling_bytes: usize,
}

impl Default for RepeatedArtifactScanConfig {
    fn default() -> Self {
        Self {
            analyse_processed_text: false,
            include_exact_lines: true,
            include_normalized_lines: true,
            include_inline_artifacts: true,
            include_two_line_blocks: true,
            include_three_line_blocks: false,
            include_text_dominant: true,
            include_mixed_text_numbers: true,
            include_numeric_dominant: true,
            include_symbol_noise: true,
            min_occurrences: 3,
            min_files: 1,
            max_candidates: None,
            max_examples_per_candidate: 25,
            min_line_chars: 2,
            max_line_chars: 600,
            memory_ceiling_bytes: DEFAULT_MEMORY_CEILING_BYTES,
        }
    }
}

/// Backend phases reported while a repeated-artefact scan is running.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RepeatedArtifactScanPhase {
    Pass1Scanning,
    Filtering,
    Pass2Materialising,
}

/// Honest phase and file progress for a repeated-artefact scan.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RepeatedArtifactScanProgress {
    pub phase: RepeatedArtifactScanPhase,
    pub files_completed: usize,
    pub files_total: usize,
}

/// Accumulation stage that reached the configured scan memory ceiling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RepeatedArtifactMemoryStage {
    Pass1Counting,
    SurvivorMaterialisation,
}

/// Structured failures returned through the normal command error channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum RepeatedArtifactScanError {
    Cancelled,
    CorpusTooLarge {
        stage: RepeatedArtifactMemoryStage,
        ceiling_bytes: usize,
        estimated_bytes: usize,
        suggestions: Vec<String>,
    },
    ScanFailed {
        message: String,
    },
}

impl RepeatedArtifactScanError {
    fn corpus_too_large(
        stage: RepeatedArtifactMemoryStage,
        ceiling_bytes: usize,
        estimated_bytes: usize,
    ) -> Self {
        Self::CorpusTooLarge {
            stage,
            ceiling_bytes,
            estimated_bytes,
            suggestions: vec![
                "Raise Min Files to discard corpus-specific singletons.".to_string(),
                "Disable inline artefacts or 2-line blocks to reduce candidate keys.".to_string(),
                "Lower Max Candidates to materialise fewer survivors.".to_string(),
            ],
        }
    }
}

impl fmt::Display for RepeatedArtifactScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Scan cancelled."),
            Self::CorpusTooLarge {
                stage,
                ceiling_bytes,
                estimated_bytes,
                suggestions,
            } => write!(
                formatter,
                "Corpus too large for this configuration during {stage:?}: estimated {estimated_bytes} bytes exceeds the {ceiling_bytes}-byte ceiling. {}",
                suggestions.join(" ")
            ),
            Self::ScanFailed { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RepeatedArtifactScanError {}

impl From<String> for RepeatedArtifactScanError {
    fn from(message: String) -> Self {
        Self::ScanFailed { message }
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
    pub candidates_before_cap: usize,
    pub candidates_truncated: usize,
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
    pub promotions: Vec<RepeatedArtifactPromotion>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct CandidateFingerprint([u8; 16]);

/// Fixed-size pass-1 accounting. Candidate text never enters this structure.
#[derive(Debug, Clone, Copy)]
struct FingerprintStats {
    kind: RepeatedArtifactKind,
    occurrence_count: u32,
    file_count: u32,
    top_count: u32,
    bottom_count: u32,
    display_char_count: u32,
    display_symbol_count: u32,
    stable_id_prefix: u64,
    /// Exact-line fingerprint of the first raw variant for normalised-line de-duplication.
    first_display_fingerprint: CandidateFingerprint,
    first_display_has_exact_candidate: bool,
    has_multiple_raw_variants: bool,
}

struct ShardedFingerprintMap<Value> {
    shards: [HashMap<CandidateFingerprint, Value>; FINGERPRINT_MAP_SHARDS],
    len: usize,
}

impl<Value> ShardedFingerprintMap<Value> {
    fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| HashMap::new()),
            len: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn shard_index(fingerprint: &CandidateFingerprint) -> usize {
        usize::from(fingerprint.0[0])
    }

    fn get(&self, fingerprint: &CandidateFingerprint) -> Option<&Value> {
        self.shards[Self::shard_index(fingerprint)].get(fingerprint)
    }

    fn get_mut(&mut self, fingerprint: &CandidateFingerprint) -> Option<&mut Value> {
        self.shards[Self::shard_index(fingerprint)].get_mut(fingerprint)
    }

    fn try_insert(
        &mut self,
        fingerprint: CandidateFingerprint,
        value: Value,
    ) -> Result<Option<Value>, std::collections::TryReserveError> {
        let shard = &mut self.shards[Self::shard_index(&fingerprint)];
        if !shard.contains_key(&fingerprint) {
            shard.try_reserve(1)?;
            self.len += 1;
        }
        Ok(shard.insert(fingerprint, value))
    }

    fn iter(&self) -> impl Iterator<Item = (&CandidateFingerprint, &Value)> {
        self.shards.iter().flat_map(|shard| shard.iter())
    }

    fn retain(&mut self, mut keep: impl FnMut(&CandidateFingerprint, &mut Value) -> bool) {
        for shard in &mut self.shards {
            let before = shard.len();
            shard.retain(|fingerprint, value| keep(fingerprint, value));
            self.len -= before - shard.len();
        }
    }

    fn into_values(self) -> impl Iterator<Item = Value> {
        self.shards
            .into_iter()
            .flat_map(|shard| shard.into_values())
    }

    fn into_entries(self) -> impl Iterator<Item = (CandidateFingerprint, Value)> {
        self.shards.into_iter().flat_map(|shard| shard.into_iter())
    }
}

struct MaterializedCandidate {
    kind: RepeatedArtifactKind,
    display_text: String,
    normalized_key: String,
    candidate_key: String,
    occurrence_count: usize,
    file_count: usize,
    top_count: usize,
    middle_count: usize,
    bottom_count: usize,
    unknown_count: usize,
    raw_variants: Vec<String>,
    raw_variant_overflow: bool,
    position_summary_is_page_based: bool,
    examples: Vec<RepeatedArtifactExample>,
}

struct Pass1FileResult {
    local_counts: ShardedFingerprintMap<FingerprintStats>,
    extraction_failed: bool,
    raw_line_count: usize,
    empty_after_extraction: bool,
}

struct MaterializationBudget {
    ceiling_bytes: usize,
    estimated_bytes: usize,
}

impl MaterializationBudget {
    fn new(ceiling_bytes: usize, initial_bytes: usize) -> Result<Self, RepeatedArtifactScanError> {
        if initial_bytes > ceiling_bytes {
            return Err(RepeatedArtifactScanError::corpus_too_large(
                RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                ceiling_bytes,
                initial_bytes,
            ));
        }
        Ok(Self {
            ceiling_bytes,
            estimated_bytes: initial_bytes,
        })
    }

    fn reserve(&mut self, bytes: usize) -> Result<(), RepeatedArtifactScanError> {
        let estimated_bytes = self.estimated_bytes.saturating_add(bytes);
        if estimated_bytes > self.ceiling_bytes {
            return Err(RepeatedArtifactScanError::corpus_too_large(
                RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                self.ceiling_bytes,
                estimated_bytes,
            ));
        }
        self.estimated_bytes = estimated_bytes;
        Ok(())
    }

    fn release(&mut self, bytes: usize) {
        self.estimated_bytes = self.estimated_bytes.saturating_sub(bytes);
    }
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
    fn document_line(
        &self,
        line_idx: usize,
    ) -> Option<&crate::sources::structured_document::DocumentLine> {
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

#[derive(Clone, Copy)]
enum CandidateTextParts<'a> {
    Single(&'a str),
    Two(&'a str, &'a str),
    Three(&'a str, &'a str, &'a str),
}

impl CandidateTextParts<'_> {
    fn update_digest(self, digest: &mut impl sha2::Digest) {
        match self {
            Self::Single(text) => digest.update(text.as_bytes()),
            Self::Two(first, second) => {
                digest.update(first.as_bytes());
                digest.update(b"\n");
                digest.update(second.as_bytes());
            }
            Self::Three(first, second, third) => {
                digest.update(first.as_bytes());
                digest.update(b"\n");
                digest.update(second.as_bytes());
                digest.update(b"\n");
                digest.update(third.as_bytes());
            }
        }
    }

    fn byte_len(self) -> usize {
        match self {
            Self::Single(text) => text.len(),
            Self::Two(first, second) => first.len() + second.len() + 1,
            Self::Three(first, second, third) => first.len() + second.len() + third.len() + 2,
        }
    }

    fn matches_string(self, value: &str) -> bool {
        if value.len() != self.byte_len() {
            return false;
        }
        match self {
            Self::Single(text) => value == text,
            Self::Two(first, second) => {
                value
                    .strip_prefix(first)
                    .and_then(|rest| rest.strip_prefix('\n'))
                    == Some(second)
            }
            Self::Three(first, second, third) => {
                value
                    .strip_prefix(first)
                    .and_then(|rest| rest.strip_prefix('\n'))
                    .and_then(|rest| rest.strip_prefix(second))
                    .and_then(|rest| rest.strip_prefix('\n'))
                    == Some(third)
            }
        }
    }

    fn char_and_symbol_counts(self) -> (u32, u32) {
        let count_text = |text: &str| {
            (
                text.chars().count(),
                text.chars()
                    .filter(|character| !character.is_alphanumeric() && !character.is_whitespace())
                    .count(),
            )
        };
        let (char_count, symbol_count) = match self {
            Self::Single(text) => count_text(text),
            Self::Two(first, second) => {
                let first_counts = count_text(first);
                let second_counts = count_text(second);
                (
                    first_counts.0 + second_counts.0 + 1,
                    first_counts.1 + second_counts.1,
                )
            }
            Self::Three(first, second, third) => {
                let first_counts = count_text(first);
                let second_counts = count_text(second);
                let third_counts = count_text(third);
                (
                    first_counts.0 + second_counts.0 + third_counts.0 + 2,
                    first_counts.1 + second_counts.1 + third_counts.1,
                )
            }
        };
        (
            u32::try_from(char_count).unwrap_or(u32::MAX),
            u32::try_from(symbol_count).unwrap_or(u32::MAX),
        )
    }

    fn try_to_string(
        self,
        budget: &mut MaterializationBudget,
    ) -> Result<String, RepeatedArtifactScanError> {
        let byte_len = self.byte_len();
        budget.reserve(byte_len)?;
        let mut output = String::new();
        output.try_reserve(byte_len).map_err(|_| {
            RepeatedArtifactScanError::corpus_too_large(
                RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                budget.ceiling_bytes,
                budget.ceiling_bytes.saturating_add(1),
            )
        })?;
        match self {
            Self::Single(text) => output.push_str(text),
            Self::Two(first, second) => {
                output.push_str(first);
                output.push('\n');
                output.push_str(second);
            }
            Self::Three(first, second, third) => {
                output.push_str(first);
                output.push('\n');
                output.push_str(second);
                output.push('\n');
                output.push_str(third);
            }
        }
        Ok(output)
    }
}

fn kind_fingerprint_tag(kind: RepeatedArtifactKind) -> u8 {
    match kind {
        RepeatedArtifactKind::ExactLine => 1,
        RepeatedArtifactKind::NormalizedLine => 2,
        RepeatedArtifactKind::TwoLineBlock => 3,
        RepeatedArtifactKind::ThreeLineBlock => 4,
        RepeatedArtifactKind::InlineArtifact => 5,
    }
}

fn candidate_fingerprint(
    kind: RepeatedArtifactKind,
    key: CandidateTextParts<'_>,
) -> CandidateFingerprint {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update([kind_fingerprint_tag(kind)]);
    key.update_digest(&mut hasher);
    let digest = hasher.finalize();
    let mut fingerprint = [0_u8; 16];
    fingerprint.copy_from_slice(&digest[..16]);
    // At 128 bits, collisions remain negligible for the tens of millions of keys in scope.
    CandidateFingerprint(fingerprint)
}

fn stable_id_prefix(key: CandidateTextParts<'_>) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    key.update_digest(&mut hasher);
    let digest = hasher.finalize();
    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

fn stable_kind_order(kind: RepeatedArtifactKind) -> u8 {
    match kind {
        RepeatedArtifactKind::TwoLineBlock => 0,
        RepeatedArtifactKind::ThreeLineBlock => 1,
        RepeatedArtifactKind::ExactLine => 2,
        RepeatedArtifactKind::InlineArtifact => 3,
        RepeatedArtifactKind::NormalizedLine => 4,
    }
}

fn estimated_pass1_bytes(global_keys: usize, local_keys: usize) -> usize {
    global_keys
        .saturating_add(local_keys)
        .saturating_mul(PASS1_ESTIMATED_BYTES_PER_KEY)
}

fn ensure_pass1_memory(
    global_keys: usize,
    local_keys: usize,
    ceiling_bytes: usize,
) -> Result<(), RepeatedArtifactScanError> {
    let estimated_bytes = estimated_pass1_bytes(global_keys, local_keys);
    if estimated_bytes > ceiling_bytes {
        return Err(RepeatedArtifactScanError::corpus_too_large(
            RepeatedArtifactMemoryStage::Pass1Counting,
            ceiling_bytes,
            estimated_bytes,
        ));
    }
    Ok(())
}

fn pass1_allocation_error(ceiling_bytes: usize) -> RepeatedArtifactScanError {
    RepeatedArtifactScanError::corpus_too_large(
        RepeatedArtifactMemoryStage::Pass1Counting,
        ceiling_bytes,
        ceiling_bytes.saturating_add(1),
    )
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
    let char_len = display_text.chars().count();
    let non_alphanumeric_count = display_text
        .chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    calculate_suspicion_score_from_counts(
        *kind,
        char_len,
        non_alphanumeric_count,
        occurrence_count,
        file_count,
        top_count,
        bottom_count,
    )
}

fn calculate_suspicion_score_from_counts(
    kind: RepeatedArtifactKind,
    char_len: usize,
    non_alphanumeric_count: usize,
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

    let len_factor = if char_len <= 3 {
        0.2
    } else if char_len <= 8 {
        0.6
    } else {
        1.0
    };

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

/// Lightweight view of one valid line while pass 1 advances through a file.
struct Pass1Line<'a> {
    index: usize,
    raw: &'a str,
    normalized: String,
    content_class: CandidateContentClass,
}

struct Pass1Occurrence<'a> {
    kind: RepeatedArtifactKind,
    candidate_key: CandidateTextParts<'a>,
    display_text: CandidateTextParts<'a>,
    first_display_fingerprint: CandidateFingerprint,
    first_display_has_exact_candidate: bool,
    position: LinePosition,
}

fn increment_pass1_candidate(
    local_counts: &mut ShardedFingerprintMap<FingerprintStats>,
    global_key_count: usize,
    config: &RepeatedArtifactScanConfig,
    occurrence: Pass1Occurrence<'_>,
) -> Result<CandidateFingerprint, RepeatedArtifactScanError> {
    let fingerprint = candidate_fingerprint(occurrence.kind, occurrence.candidate_key);
    if let Some(stats) = local_counts.get_mut(&fingerprint) {
        stats.occurrence_count = stats
            .occurrence_count
            .checked_add(1)
            .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
        match occurrence.position {
            LinePosition::Top => {
                stats.top_count = stats
                    .top_count
                    .checked_add(1)
                    .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            }
            LinePosition::Bottom => {
                stats.bottom_count = stats
                    .bottom_count
                    .checked_add(1)
                    .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            }
            LinePosition::Middle => {}
        }
        if occurrence.kind == RepeatedArtifactKind::NormalizedLine
            && stats.first_display_fingerprint != occurrence.first_display_fingerprint
        {
            stats.has_multiple_raw_variants = true;
        }
        return Ok(fingerprint);
    }

    ensure_pass1_memory(
        global_key_count,
        local_counts.len().saturating_add(1),
        config.memory_ceiling_bytes,
    )?;
    let (display_char_count, display_symbol_count) =
        occurrence.display_text.char_and_symbol_counts();
    let mut stats = FingerprintStats {
        kind: occurrence.kind,
        occurrence_count: 1,
        file_count: 1,
        top_count: 0,
        bottom_count: 0,
        display_char_count,
        display_symbol_count,
        stable_id_prefix: stable_id_prefix(occurrence.candidate_key),
        first_display_fingerprint: occurrence.first_display_fingerprint,
        first_display_has_exact_candidate: occurrence.first_display_has_exact_candidate,
        has_multiple_raw_variants: false,
    };
    match occurrence.position {
        LinePosition::Top => stats.top_count = 1,
        LinePosition::Bottom => stats.bottom_count = 1,
        LinePosition::Middle => {}
    }
    local_counts
        .try_insert(fingerprint, stats)
        .map_err(|_| pass1_allocation_error(config.memory_ceiling_bytes))?;
    Ok(fingerprint)
}

fn count_file_fingerprints(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
    global_key_count: usize,
    cancel: &CancellationFlag,
) -> Result<ShardedFingerprintMap<FingerprintStats>, RepeatedArtifactScanError> {
    let mut local_counts = ShardedFingerprintMap::new();
    let mut previous_one: Option<Pass1Line<'_>> = None;
    let mut previous_two: Option<Pass1Line<'_>> = None;
    let mut inline_matches_since_cancel_check = 0usize;

    for (line_index, source_line) in ft.raw_lines.iter().enumerate() {
        if line_index % CANCELLATION_CHECK_LINE_INTERVAL == 0 && cancel.load(Ordering::Relaxed) {
            return Err(RepeatedArtifactScanError::Cancelled);
        }

        if config.include_inline_artifacts && !source_line.is_empty() {
            for inline_match in KNOWN_INLINE_AUTOMATON.find_iter(source_line) {
                inline_matches_since_cancel_check += 1;
                if inline_matches_since_cancel_check >= CANCELLATION_CHECK_LINE_INTERVAL {
                    inline_matches_since_cancel_check = 0;
                    if cancel.load(Ordering::Relaxed) {
                        return Err(RepeatedArtifactScanError::Cancelled);
                    }
                }
                let pattern = KNOWN_INLINE_PATTERNS[inline_match.pattern().as_usize()];
                let pattern_text = CandidateTextParts::Single(pattern);
                let fingerprint =
                    candidate_fingerprint(RepeatedArtifactKind::InlineArtifact, pattern_text);
                increment_pass1_candidate(
                    &mut local_counts,
                    global_key_count,
                    config,
                    Pass1Occurrence {
                        kind: RepeatedArtifactKind::InlineArtifact,
                        candidate_key: pattern_text,
                        display_text: pattern_text,
                        first_display_fingerprint: fingerprint,
                        first_display_has_exact_candidate: true,
                        position: compute_position_for_line(ft, line_index),
                    },
                )?;
            }
        }

        let raw = source_line.trim();
        if should_skip_line(raw, config.min_line_chars, config.max_line_chars) {
            previous_one = None;
            previous_two = None;
            continue;
        }

        let normalized = normalize_line(raw);
        let content_class = classify_content(raw);
        let position = compute_position_for_line(ft, line_index);
        let raw_text = CandidateTextParts::Single(raw);
        let mut exact_fingerprint = None;

        if config.include_exact_lines && should_include_content_class(content_class, config) {
            let fingerprint = candidate_fingerprint(RepeatedArtifactKind::ExactLine, raw_text);
            increment_pass1_candidate(
                &mut local_counts,
                global_key_count,
                config,
                Pass1Occurrence {
                    kind: RepeatedArtifactKind::ExactLine,
                    candidate_key: raw_text,
                    display_text: raw_text,
                    first_display_fingerprint: fingerprint,
                    first_display_has_exact_candidate: true,
                    position,
                },
            )?;
            exact_fingerprint = Some(fingerprint);
        }

        if config.include_normalized_lines
            && !normalized.is_empty()
            && (content_class != CandidateContentClass::NumericDominant
                || config.include_numeric_dominant)
        {
            let first_display_fingerprint = if config.include_exact_lines {
                exact_fingerprint.unwrap_or_else(|| {
                    candidate_fingerprint(RepeatedArtifactKind::ExactLine, raw_text)
                })
            } else {
                CandidateFingerprint([0; 16])
            };
            increment_pass1_candidate(
                &mut local_counts,
                global_key_count,
                config,
                Pass1Occurrence {
                    kind: RepeatedArtifactKind::NormalizedLine,
                    candidate_key: CandidateTextParts::Single(&normalized),
                    display_text: raw_text,
                    first_display_fingerprint,
                    first_display_has_exact_candidate: exact_fingerprint.is_some(),
                    position,
                },
            )?;
        }

        let current = Pass1Line {
            index: line_index,
            raw,
            normalized,
            content_class,
        };

        if config.include_two_line_blocks
            && let Some(previous) = previous_one.as_ref()
            && current.index == previous.index + 1
            && ft.lines_share_page(previous.index, current.index)
        {
            let block_class = dominant_content_class(previous.content_class, current.content_class);
            if should_include_content_class(block_class, config) {
                let raw_block = CandidateTextParts::Two(previous.raw, current.raw);
                let block_fingerprint =
                    candidate_fingerprint(RepeatedArtifactKind::TwoLineBlock, raw_block);
                increment_pass1_candidate(
                    &mut local_counts,
                    global_key_count,
                    config,
                    Pass1Occurrence {
                        kind: RepeatedArtifactKind::TwoLineBlock,
                        candidate_key: raw_block,
                        display_text: raw_block,
                        first_display_fingerprint: block_fingerprint,
                        first_display_has_exact_candidate: true,
                        position: compute_position_for_line(ft, previous.index),
                    },
                )?;
            }
        }

        if config.include_three_line_blocks
            && let (Some(first), Some(second)) = (previous_two.as_ref(), previous_one.as_ref())
            && second.index == first.index + 1
            && current.index == second.index + 1
            && ft.lines_share_page(first.index, second.index)
            && ft.lines_share_page(first.index, current.index)
        {
            let block_class = dominant_content_class_3(
                first.content_class,
                second.content_class,
                current.content_class,
            );
            if should_include_content_class(block_class, config) {
                let raw_block = CandidateTextParts::Three(first.raw, second.raw, current.raw);
                let block_fingerprint =
                    candidate_fingerprint(RepeatedArtifactKind::ThreeLineBlock, raw_block);
                increment_pass1_candidate(
                    &mut local_counts,
                    global_key_count,
                    config,
                    Pass1Occurrence {
                        kind: RepeatedArtifactKind::ThreeLineBlock,
                        candidate_key: raw_block,
                        display_text: raw_block,
                        first_display_fingerprint: block_fingerprint,
                        first_display_has_exact_candidate: true,
                        position: compute_position_for_line(ft, first.index),
                    },
                )?;
            }
        }

        previous_two = previous_one.take();
        previous_one = Some(current);
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    Ok(local_counts)
}

/// Counts one file into fingerprint-only per-file state. The set is discarded after merging.
fn pass1_scan_file(
    record: &DocumentRecord,
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    cancel: &CancellationFlag,
    global_key_count: usize,
) -> Result<Pass1FileResult, RepeatedArtifactScanError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    let extracted = get_document_text_with_status(
        record,
        config.analyse_processed_text,
        cleaning_config,
        cache,
    );
    if let Some(message) = extracted.fatal_error {
        return Err(RepeatedArtifactScanError::ScanFailed { message });
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }

    let extraction_failed = extracted.extraction_failed;
    let ft = build_file_text(record, &extracted.text, extracted.page_texts.as_deref());
    let raw_line_count = ft.raw_lines.len();
    let empty_after_extraction = ft.raw_lines.is_empty();
    let local_counts = count_file_fingerprints(&ft, config, global_key_count, cancel)?;
    Ok(Pass1FileResult {
        local_counts,
        extraction_failed,
        raw_line_count,
        empty_after_extraction,
    })
}

fn merge_pass1_file(
    global_counts: &mut ShardedFingerprintMap<FingerprintStats>,
    local_counts: ShardedFingerprintMap<FingerprintStats>,
    config: &RepeatedArtifactScanConfig,
) -> Result<(), RepeatedArtifactScanError> {
    let local_key_count = local_counts.len();
    for (fingerprint, local) in local_counts.into_entries() {
        if let Some(global) = global_counts.get_mut(&fingerprint) {
            global.occurrence_count = global
                .occurrence_count
                .checked_add(local.occurrence_count)
                .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            global.file_count = global
                .file_count
                .checked_add(local.file_count)
                .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            global.top_count = global
                .top_count
                .checked_add(local.top_count)
                .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            global.bottom_count = global
                .bottom_count
                .checked_add(local.bottom_count)
                .ok_or_else(|| pass1_allocation_error(config.memory_ceiling_bytes))?;
            if global.kind == RepeatedArtifactKind::NormalizedLine
                && (local.has_multiple_raw_variants
                    || global.first_display_fingerprint != local.first_display_fingerprint)
            {
                global.has_multiple_raw_variants = true;
            }
            continue;
        }

        ensure_pass1_memory(
            global_counts.len().saturating_add(1),
            local_key_count,
            config.memory_ceiling_bytes,
        )?;
        global_counts
            .try_insert(fingerprint, local)
            .map_err(|_| pass1_allocation_error(config.memory_ceiling_bytes))?;
    }
    Ok(())
}
#[derive(Debug, Clone, Copy)]
struct RankedFingerprint {
    fingerprint: CandidateFingerprint,
    stats: FingerprintStats,
    score: f64,
}

impl PartialEq for RankedFingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}

impl Eq for RankedFingerprint {}

fn compare_ranked_quality(
    first: &RankedFingerprint,
    second: &RankedFingerprint,
) -> std::cmp::Ordering {
    first
        .score
        .total_cmp(&second.score)
        .then_with(|| {
            stable_kind_order(second.stats.kind).cmp(&stable_kind_order(first.stats.kind))
        })
        .then_with(|| {
            second
                .stats
                .stable_id_prefix
                .cmp(&first.stats.stable_id_prefix)
        })
        .then_with(|| second.fingerprint.cmp(&first.fingerprint))
}

impl Ord for RankedFingerprint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap keeps the worst selected candidate at the root.
        compare_ranked_quality(other, self)
    }
}

impl PartialOrd for RankedFingerprint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct FilteredFingerprints {
    survivors: ShardedFingerprintMap<FingerprintStats>,
    candidates_after_min_occurrences: usize,
    candidates_after_min_files: usize,
    candidates_before_cap: usize,
    candidates_truncated: usize,
}

fn fingerprint_passes_min_occurrences(
    stats: &FingerprintStats,
    config: &RepeatedArtifactScanConfig,
) -> bool {
    stats.occurrence_count as usize >= config.min_occurrences
}

fn fingerprint_passes_thresholds(
    stats: &FingerprintStats,
    config: &RepeatedArtifactScanConfig,
) -> bool {
    fingerprint_passes_min_occurrences(stats, config)
        && stats.file_count as usize >= config.min_files
}

fn fingerprint_is_redundant_normalized(
    stats: &FingerprintStats,
    config: &RepeatedArtifactScanConfig,
) -> bool {
    config.include_normalized_lines
        && config.include_exact_lines
        && stats.kind == RepeatedArtifactKind::NormalizedLine
        && !stats.has_multiple_raw_variants
        && stats.first_display_has_exact_candidate
}

fn ranked_fingerprint(
    fingerprint: CandidateFingerprint,
    stats: FingerprintStats,
) -> RankedFingerprint {
    let score = calculate_suspicion_score_from_counts(
        stats.kind,
        stats.display_char_count as usize,
        stats.display_symbol_count as usize,
        stats.occurrence_count as usize,
        stats.file_count as usize,
        stats.top_count as usize,
        stats.bottom_count as usize,
    );
    RankedFingerprint {
        fingerprint,
        stats,
        score,
    }
}

fn filter_fingerprints(
    mut global_counts: ShardedFingerprintMap<FingerprintStats>,
    config: &RepeatedArtifactScanConfig,
    cancel: &CancellationFlag,
) -> Result<FilteredFingerprints, RepeatedArtifactScanError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }

    let candidates_after_min_occurrences = global_counts
        .iter()
        .filter(|(_, stats)| fingerprint_passes_min_occurrences(stats, config))
        .count();
    let candidates_after_min_files = global_counts
        .iter()
        .filter(|(_, stats)| fingerprint_passes_thresholds(stats, config))
        .count();
    let candidates_before_cap = global_counts
        .iter()
        .filter(|(_, stats)| {
            fingerprint_passes_thresholds(stats, config)
                && !fingerprint_is_redundant_normalized(stats, config)
        })
        .count();

    let Some(limit) = config.max_candidates else {
        global_counts.retain(|_, stats| {
            fingerprint_passes_thresholds(stats, config)
                && !fingerprint_is_redundant_normalized(stats, config)
        });
        return Ok(FilteredFingerprints {
            survivors: global_counts,
            candidates_after_min_occurrences,
            candidates_after_min_files,
            candidates_before_cap,
            candidates_truncated: 0,
        });
    };

    let mut selected = std::collections::BinaryHeap::<RankedFingerprint>::new();
    for (index, (fingerprint, stats)) in global_counts.iter().enumerate() {
        if index % CANCELLATION_CHECK_LINE_INTERVAL == 0 && cancel.load(Ordering::Relaxed) {
            return Err(RepeatedArtifactScanError::Cancelled);
        }
        if !fingerprint_passes_thresholds(stats, config)
            || fingerprint_is_redundant_normalized(stats, config)
            || limit == 0
        {
            continue;
        }
        let ranked = ranked_fingerprint(*fingerprint, *stats);
        if selected.len() < limit {
            let estimated_bytes = estimated_pass1_bytes(global_counts.len(), selected.len() + 1);
            if estimated_bytes > config.memory_ceiling_bytes {
                return Err(RepeatedArtifactScanError::corpus_too_large(
                    RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                    config.memory_ceiling_bytes,
                    estimated_bytes,
                ));
            }
            selected.try_reserve(1).map_err(|_| {
                RepeatedArtifactScanError::corpus_too_large(
                    RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                    config.memory_ceiling_bytes,
                    config.memory_ceiling_bytes.saturating_add(1),
                )
            })?;
            selected.push(ranked);
        } else if selected
            .peek()
            .is_some_and(|worst| compare_ranked_quality(&ranked, worst).is_gt())
        {
            selected.pop();
            selected.push(ranked);
        }
    }
    drop(global_counts);

    let selected_len = selected.len();
    ensure_pass1_memory(selected_len, 0, config.memory_ceiling_bytes)?;
    let mut survivors = ShardedFingerprintMap::new();
    for ranked in selected {
        survivors
            .try_insert(ranked.fingerprint, ranked.stats)
            .map_err(|_| {
                RepeatedArtifactScanError::corpus_too_large(
                    RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                    config.memory_ceiling_bytes,
                    config.memory_ceiling_bytes.saturating_add(1),
                )
            })?;
    }

    Ok(FilteredFingerprints {
        survivors,
        candidates_after_min_occurrences,
        candidates_after_min_files,
        candidates_before_cap,
        candidates_truncated: candidates_before_cap.saturating_sub(selected_len),
    })
}

#[derive(Clone, Copy)]
struct MaterializedOccurrence<'a> {
    kind: RepeatedArtifactKind,
    candidate_key: CandidateTextParts<'a>,
    normalized_key: CandidateTextParts<'a>,
    display_text: CandidateTextParts<'a>,
    start_line: usize,
    end_line: usize,
    inline_byte_position: Option<usize>,
}

fn try_clone_materialized_string(
    value: &str,
    budget: &mut MaterializationBudget,
) -> Result<String, RepeatedArtifactScanError> {
    CandidateTextParts::Single(value).try_to_string(budget)
}

fn account_optional_string(
    value: &Option<String>,
    budget: &mut MaterializationBudget,
) -> Result<(), RepeatedArtifactScanError> {
    if let Some(value) = value {
        budget.reserve(value.len())?;
    }
    Ok(())
}

fn materialize_occurrence(
    ft: &FileText,
    occurrence: MaterializedOccurrence<'_>,
    position: LinePosition,
    survivors: &ShardedFingerprintMap<FingerprintStats>,
    materialized: &mut ShardedFingerprintMap<MaterializedCandidate>,
    config: &RepeatedArtifactScanConfig,
    budget: &mut MaterializationBudget,
) -> Result<(), RepeatedArtifactScanError> {
    let fingerprint = candidate_fingerprint(occurrence.kind, occurrence.candidate_key);
    if materialized.get(&fingerprint).is_none() {
        let Some(stats) = survivors.get(&fingerprint).copied() else {
            return Ok(());
        };
        budget.reserve(MATERIALIZED_ESTIMATED_BASE_BYTES_PER_KEY)?;
        let candidate_key = occurrence.candidate_key.try_to_string(budget)?;
        let normalized_key = occurrence.normalized_key.try_to_string(budget)?;
        let display_text = occurrence.display_text.try_to_string(budget)?;
        materialized
            .try_insert(
                fingerprint,
                MaterializedCandidate {
                    kind: stats.kind,
                    display_text,
                    normalized_key,
                    candidate_key,
                    occurrence_count: stats.occurrence_count as usize,
                    file_count: stats.file_count as usize,
                    top_count: 0,
                    middle_count: 0,
                    bottom_count: 0,
                    unknown_count: 0,
                    raw_variants: Vec::new(),
                    raw_variant_overflow: false,
                    position_summary_is_page_based: true,
                    examples: Vec::new(),
                },
            )
            .map_err(|_| {
                RepeatedArtifactScanError::corpus_too_large(
                    RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                    config.memory_ceiling_bytes,
                    config.memory_ceiling_bytes.saturating_add(1),
                )
            })?;
    }

    let entry = materialized
        .get_mut(&fingerprint)
        .expect("survivor entry was inserted before materialisation");
    match position {
        LinePosition::Top => entry.top_count += 1,
        LinePosition::Middle => entry.middle_count += 1,
        LinePosition::Bottom => entry.bottom_count += 1,
    }
    entry.position_summary_is_page_based &= ft.has_page_metadata;

    let variant_already_present = entry
        .raw_variants
        .iter()
        .any(|variant| occurrence.display_text.matches_string(variant));
    if !variant_already_present {
        if entry.raw_variants.len() < RAW_VARIANT_TRACK_CAP {
            entry.raw_variants.try_reserve(1).map_err(|_| {
                RepeatedArtifactScanError::corpus_too_large(
                    RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                    config.memory_ceiling_bytes,
                    config.memory_ceiling_bytes.saturating_add(1),
                )
            })?;
            entry
                .raw_variants
                .push(occurrence.display_text.try_to_string(budget)?);
        } else {
            entry.raw_variant_overflow = true;
        }
    }

    if entry.examples.len() >= config.max_examples_per_candidate {
        return Ok(());
    }
    entry.examples.try_reserve(1).map_err(|_| {
        RepeatedArtifactScanError::corpus_too_large(
            RepeatedArtifactMemoryStage::SurvivorMaterialisation,
            config.memory_ceiling_bytes,
            config.memory_ceiling_bytes.saturating_add(1),
        )
    })?;

    let (context_before, context_after) =
        if let Some(byte_position) = occurrence.inline_byte_position {
            let line = &ft.raw_lines[occurrence.start_line];
            (
                safe_context_before(line, byte_position, 80),
                safe_context_after(line, byte_position, occurrence.display_text.byte_len(), 80),
            )
        } else {
            let before = if occurrence.start_line > 0 {
                let text = ft.raw_lines[occurrence.start_line - 1].trim();
                (!text.is_empty()).then(|| text.chars().take(200).collect())
            } else {
                None
            };
            let after = if occurrence.end_line + 1 < ft.raw_lines.len() {
                let text = ft.raw_lines[occurrence.end_line + 1].trim();
                (!text.is_empty()).then(|| text.chars().take(200).collect())
            } else {
                None
            };
            (before, after)
        };
    account_optional_string(&context_before, budget)?;
    account_optional_string(&context_after, budget)?;
    budget.reserve(std::mem::size_of::<RepeatedArtifactExample>())?;
    let example = RepeatedArtifactExample {
        file_name: try_clone_materialized_string(&ft.file_name, budget)?,
        file_path: try_clone_materialized_string(&ft.file_path, budget)?,
        line_number: Some(occurrence.start_line + 1),
        page_number: ft.page_number_for_line(occurrence.start_line),
        context_before,
        matched_text: occurrence.display_text.try_to_string(budget)?,
        context_after,
    };
    entry.examples.push(example);
    Ok(())
}

fn materialize_file_survivors(
    ft: &FileText,
    config: &RepeatedArtifactScanConfig,
    survivors: &ShardedFingerprintMap<FingerprintStats>,
    materialized: &mut ShardedFingerprintMap<MaterializedCandidate>,
    cancel: &CancellationFlag,
    budget: &mut MaterializationBudget,
) -> Result<(), RepeatedArtifactScanError> {
    let mut previous_one: Option<Pass1Line<'_>> = None;
    let mut previous_two: Option<Pass1Line<'_>> = None;
    let mut inline_matches_since_cancel_check = 0usize;

    for (line_index, source_line) in ft.raw_lines.iter().enumerate() {
        if line_index % CANCELLATION_CHECK_LINE_INTERVAL == 0 && cancel.load(Ordering::Relaxed) {
            return Err(RepeatedArtifactScanError::Cancelled);
        }

        if config.include_inline_artifacts && !source_line.is_empty() {
            for inline_match in KNOWN_INLINE_AUTOMATON.find_iter(source_line) {
                inline_matches_since_cancel_check += 1;
                if inline_matches_since_cancel_check >= CANCELLATION_CHECK_LINE_INTERVAL {
                    inline_matches_since_cancel_check = 0;
                    if cancel.load(Ordering::Relaxed) {
                        return Err(RepeatedArtifactScanError::Cancelled);
                    }
                }
                let pattern = KNOWN_INLINE_PATTERNS[inline_match.pattern().as_usize()];
                let pattern_text = CandidateTextParts::Single(pattern);
                materialize_occurrence(
                    ft,
                    MaterializedOccurrence {
                        kind: RepeatedArtifactKind::InlineArtifact,
                        candidate_key: pattern_text,
                        normalized_key: CandidateTextParts::Single(""),
                        display_text: pattern_text,
                        start_line: line_index,
                        end_line: line_index,
                        inline_byte_position: Some(inline_match.start()),
                    },
                    compute_position_for_line(ft, line_index),
                    survivors,
                    materialized,
                    config,
                    budget,
                )?;
            }
        }

        let raw = source_line.trim();
        if should_skip_line(raw, config.min_line_chars, config.max_line_chars) {
            previous_one = None;
            previous_two = None;
            continue;
        }
        let normalized = normalize_line(raw);
        let content_class = classify_content(raw);
        let position = compute_position_for_line(ft, line_index);
        let raw_text = CandidateTextParts::Single(raw);

        if config.include_exact_lines && should_include_content_class(content_class, config) {
            materialize_occurrence(
                ft,
                MaterializedOccurrence {
                    kind: RepeatedArtifactKind::ExactLine,
                    candidate_key: raw_text,
                    normalized_key: CandidateTextParts::Single(&normalized),
                    display_text: raw_text,
                    start_line: line_index,
                    end_line: line_index,
                    inline_byte_position: None,
                },
                position,
                survivors,
                materialized,
                config,
                budget,
            )?;
        }

        if config.include_normalized_lines
            && !normalized.is_empty()
            && (content_class != CandidateContentClass::NumericDominant
                || config.include_numeric_dominant)
        {
            materialize_occurrence(
                ft,
                MaterializedOccurrence {
                    kind: RepeatedArtifactKind::NormalizedLine,
                    candidate_key: CandidateTextParts::Single(&normalized),
                    normalized_key: CandidateTextParts::Single(&normalized),
                    display_text: raw_text,
                    start_line: line_index,
                    end_line: line_index,
                    inline_byte_position: None,
                },
                position,
                survivors,
                materialized,
                config,
                budget,
            )?;
        }

        let current = Pass1Line {
            index: line_index,
            raw,
            normalized,
            content_class,
        };

        if config.include_two_line_blocks
            && let Some(previous) = previous_one.as_ref()
            && current.index == previous.index + 1
            && ft.lines_share_page(previous.index, current.index)
        {
            let block_class = dominant_content_class(previous.content_class, current.content_class);
            if should_include_content_class(block_class, config) {
                materialize_occurrence(
                    ft,
                    MaterializedOccurrence {
                        kind: RepeatedArtifactKind::TwoLineBlock,
                        candidate_key: CandidateTextParts::Two(previous.raw, current.raw),
                        normalized_key: CandidateTextParts::Two(
                            &previous.normalized,
                            &current.normalized,
                        ),
                        display_text: CandidateTextParts::Two(previous.raw, current.raw),
                        start_line: previous.index,
                        end_line: current.index,
                        inline_byte_position: None,
                    },
                    compute_position_for_line(ft, previous.index),
                    survivors,
                    materialized,
                    config,
                    budget,
                )?;
            }
        }

        if config.include_three_line_blocks
            && let (Some(first), Some(second)) = (previous_two.as_ref(), previous_one.as_ref())
            && second.index == first.index + 1
            && current.index == second.index + 1
            && ft.lines_share_page(first.index, second.index)
            && ft.lines_share_page(first.index, current.index)
        {
            let block_class = dominant_content_class_3(
                first.content_class,
                second.content_class,
                current.content_class,
            );
            if should_include_content_class(block_class, config) {
                materialize_occurrence(
                    ft,
                    MaterializedOccurrence {
                        kind: RepeatedArtifactKind::ThreeLineBlock,
                        candidate_key: CandidateTextParts::Three(
                            first.raw,
                            second.raw,
                            current.raw,
                        ),
                        normalized_key: CandidateTextParts::Three(
                            &first.normalized,
                            &second.normalized,
                            &current.normalized,
                        ),
                        display_text: CandidateTextParts::Three(first.raw, second.raw, current.raw),
                        start_line: first.index,
                        end_line: current.index,
                        inline_byte_position: None,
                    },
                    compute_position_for_line(ft, first.index),
                    survivors,
                    materialized,
                    config,
                    budget,
                )?;
            }
        }

        previous_two = previous_one.take();
        previous_one = Some(current);
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    Ok(())
}

struct Pass2Accumulation<'a> {
    survivors: &'a ShardedFingerprintMap<FingerprintStats>,
    materialized: &'a mut ShardedFingerprintMap<MaterializedCandidate>,
    cancel: &'a CancellationFlag,
    budget: &'a mut MaterializationBudget,
}

fn materialize_file_record(
    record: &DocumentRecord,
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    accumulation: &mut Pass2Accumulation<'_>,
) -> Result<(), RepeatedArtifactScanError> {
    if accumulation.cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    let extracted = get_document_text_with_status(
        record,
        config.analyse_processed_text,
        cleaning_config,
        cache,
    );
    if let Some(message) = extracted.fatal_error {
        return Err(RepeatedArtifactScanError::ScanFailed { message });
    }
    if accumulation.cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    let ft = build_file_text(record, &extracted.text, extracted.page_texts.as_deref());
    materialize_file_survivors(
        &ft,
        config,
        accumulation.survivors,
        accumulation.materialized,
        accumulation.cancel,
        accumulation.budget,
    )
}

fn finalize_materialized_candidates(
    materialized: ShardedFingerprintMap<MaterializedCandidate>,
    budget: &mut MaterializationBudget,
) -> Result<Vec<RepeatedArtifactCandidate>, RepeatedArtifactScanError> {
    let candidate_count = materialized.len();
    budget.reserve(candidate_count.saturating_mul(
        std::mem::size_of::<RepeatedArtifactCandidate>() + std::mem::size_of::<f64>(),
    ))?;
    let mut scored = Vec::new();
    scored.try_reserve(candidate_count).map_err(|_| {
        RepeatedArtifactScanError::corpus_too_large(
            RepeatedArtifactMemoryStage::SurvivorMaterialisation,
            budget.ceiling_bytes,
            budget.ceiling_bytes.saturating_add(1),
        )
    })?;

    for mut entry in materialized.into_values() {
        entry.raw_variants.sort();
        let score = calculate_suspicion_score(
            &entry.kind,
            &entry.display_text,
            entry.occurrence_count,
            entry.file_count,
            entry.top_count,
            entry.bottom_count,
        );
        let risk_label = classify_risk(
            &entry.display_text,
            &entry.normalized_key,
            entry.occurrence_count,
            entry.file_count,
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
        scored.push((
            score,
            RepeatedArtifactCandidate {
                candidate_id,
                kind: entry.kind,
                display_text: entry.display_text,
                normalized_key: entry.normalized_key,
                occurrence_count: entry.occurrence_count,
                file_count: entry.file_count,
                example_count: entry.examples.len(),
                position_summary,
                position_summary_is_page_based: entry.position_summary_is_page_based,
                risk_label,
                content_class,
                text_profile,
                text_signal_label,
                text_signal_reasons,
                raw_variant_count: entry.raw_variants.len(),
                raw_variant_count_is_capped: entry.raw_variant_overflow,
                raw_variants: entry.raw_variants,
                examples: entry.examples,
            },
        ));
    }

    scored.sort_by(|first, second| {
        second
            .0
            .partial_cmp(&first.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| first.1.candidate_id.cmp(&second.1.candidate_id))
    });
    Ok(scored.into_iter().map(|(_, candidate)| candidate).collect())
}

/// Performs a corpus-wide scan for repeated lines and blocks.
pub fn scan_repeated_artifacts_report(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
) -> Result<RepeatedArtifactScanReport, String> {
    scan_repeated_artifacts_report_with_cancel(records, config, cleaning_config, &no_cancellation())
}

/// Performs a corpus-wide scan with cooperative cancellation.
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
    .map_err(|error| error.to_string())
}

/// Performs the memory-bounded two-pass scan with optional extraction cache.
pub fn scan_repeated_artifacts_report_with_cancel_and_cache(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    cancel: &CancellationFlag,
) -> Result<RepeatedArtifactScanReport, RepeatedArtifactScanError> {
    scan_repeated_artifacts_report_with_cancel_cache_and_progress(
        records,
        config,
        cleaning_config,
        cache,
        cancel,
        |_| {},
    )
}

/// Performs the memory-bounded two-pass scan and reports backend phase progress.
pub fn scan_repeated_artifacts_report_with_cancel_cache_and_progress<Progress>(
    records: &[DocumentRecord],
    config: &RepeatedArtifactScanConfig,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
    cancel: &CancellationFlag,
    mut report_progress: Progress,
) -> Result<RepeatedArtifactScanReport, RepeatedArtifactScanError>
where
    Progress: FnMut(RepeatedArtifactScanProgress),
{
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
            promotions: Vec::new(),
            diagnostics: RepeatedArtifactScanDiagnostics {
                files_requested: 0,
                files_scanned: 0,
                files_failed_extraction: 0,
                files_empty_after_extraction: 0,
                total_raw_lines: 0,
                total_candidate_keys_before_filtering: 0,
                candidates_after_min_occurrences: 0,
                candidates_after_min_files: 0,
                candidates_before_cap: 0,
                candidates_truncated: 0,
                final_candidates: 0,
                analysed_processed_text,
                custom_removals_active,
                max_examples_per_candidate,
            },
        });
    }

    let mut ordered_records = Vec::new();
    ordered_records.try_reserve(records.len()).map_err(|_| {
        RepeatedArtifactScanError::corpus_too_large(
            RepeatedArtifactMemoryStage::Pass1Counting,
            config.memory_ceiling_bytes,
            config.memory_ceiling_bytes.saturating_add(1),
        )
    })?;
    ordered_records.extend(records.iter());
    ordered_records.sort_by(|first, second| {
        first
            .relative_path
            .to_string_lossy()
            .cmp(&second.relative_path.to_string_lossy())
            .then_with(|| {
                first
                    .source_path
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .cmp(
                        &second
                            .source_path
                            .file_name()
                            .map(|name| name.to_string_lossy()),
                    )
            })
    });

    report_progress(RepeatedArtifactScanProgress {
        phase: RepeatedArtifactScanPhase::Pass1Scanning,
        files_completed: 0,
        files_total: files_requested,
    });
    let mut global_counts = ShardedFingerprintMap::new();
    let mut files_failed_extraction = 0usize;
    let mut files_empty_after_extraction = 0usize;
    let mut total_raw_lines = 0usize;

    for (file_index, record) in ordered_records.iter().enumerate() {
        let file_result = pass1_scan_file(
            record,
            config,
            cleaning_config,
            cache,
            cancel,
            global_counts.len(),
        )?;
        files_failed_extraction += usize::from(file_result.extraction_failed);
        files_empty_after_extraction += usize::from(file_result.empty_after_extraction);
        total_raw_lines = total_raw_lines.saturating_add(file_result.raw_line_count);
        merge_pass1_file(&mut global_counts, file_result.local_counts, config)?;
        report_progress(RepeatedArtifactScanProgress {
            phase: RepeatedArtifactScanPhase::Pass1Scanning,
            files_completed: file_index + 1,
            files_total: files_requested,
        });
    }

    if cancel.load(Ordering::Relaxed) {
        return Err(RepeatedArtifactScanError::Cancelled);
    }
    let total_candidate_keys_before_filtering = global_counts.len();
    report_progress(RepeatedArtifactScanProgress {
        phase: RepeatedArtifactScanPhase::Filtering,
        files_completed: 0,
        files_total: files_requested,
    });
    let filtered = filter_fingerprints(global_counts, config, cancel)?;
    let candidates_after_min_occurrences = filtered.candidates_after_min_occurrences;
    let candidates_after_min_files = filtered.candidates_after_min_files;
    let candidates_before_cap = filtered.candidates_before_cap;
    let candidates_truncated = filtered.candidates_truncated;
    let survivors = filtered.survivors;

    report_progress(RepeatedArtifactScanProgress {
        phase: RepeatedArtifactScanPhase::Pass2Materialising,
        files_completed: 0,
        files_total: files_requested,
    });
    let survivor_accounting_bytes = survivors
        .len()
        .saturating_mul(PASS1_ESTIMATED_BYTES_PER_KEY);
    let mut budget =
        MaterializationBudget::new(config.memory_ceiling_bytes, survivor_accounting_bytes)?;
    let mut materialized = ShardedFingerprintMap::new();

    if survivors.is_empty() {
        report_progress(RepeatedArtifactScanProgress {
            phase: RepeatedArtifactScanPhase::Pass2Materialising,
            files_completed: files_requested,
            files_total: files_requested,
        });
    } else {
        let mut accumulation = Pass2Accumulation {
            survivors: &survivors,
            materialized: &mut materialized,
            cancel,
            budget: &mut budget,
        };
        for (file_index, record) in ordered_records.iter().enumerate() {
            materialize_file_record(record, config, cleaning_config, cache, &mut accumulation)?;
            report_progress(RepeatedArtifactScanProgress {
                phase: RepeatedArtifactScanPhase::Pass2Materialising,
                files_completed: file_index + 1,
                files_total: files_requested,
            });
        }
    }

    if materialized.len() != survivors.len() {
        return Err(RepeatedArtifactScanError::ScanFailed {
            message: "A survivor could not be materialised consistently in pass 2.".to_string(),
        });
    }
    drop(survivors);
    budget.release(survivor_accounting_bytes);
    let candidates = finalize_materialized_candidates(materialized, &mut budget)?;
    let final_candidates = candidates.len();

    let mut promotions = Vec::new();
    promotions.try_reserve(final_candidates).map_err(|_| {
        RepeatedArtifactScanError::corpus_too_large(
            RepeatedArtifactMemoryStage::SurvivorMaterialisation,
            config.memory_ceiling_bytes,
            config.memory_ceiling_bytes.saturating_add(1),
        )
    })?;
    promotions.extend(candidates.iter().map(promotion_for_candidate));

    Ok(RepeatedArtifactScanReport {
        candidates,
        promotions,
        diagnostics: RepeatedArtifactScanDiagnostics {
            files_requested,
            files_scanned: files_requested,
            files_failed_extraction,
            files_empty_after_extraction,
            total_raw_lines,
            total_candidate_keys_before_filtering,
            candidates_after_min_occurrences,
            candidates_after_min_files,
            candidates_before_cap,
            candidates_truncated,
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
    use crate::corpus::DocumentType;
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
    fn test_classify_risk_header_footer_boundary() {
        assert_eq!(
            classify_risk("Running Header", "running header", 3, 1, 2, 1),
            ArtifactRiskLabel::StrongHeaderFooterCandidate
        );
        assert_ne!(
            classify_risk("Running Header", "running header", 2, 1, 2, 0),
            ArtifactRiskLabel::StrongHeaderFooterCandidate
        );
        assert_ne!(
            classify_risk("Running Header", "running header", 4, 1, 2, 0),
            ArtifactRiskLabel::StrongHeaderFooterCandidate
        );
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
            extraction_route: None,
            pdf_profile: None,
            ocr_quality: None,
        }
    }

    fn relaxed_config() -> RepeatedArtifactScanConfig {
        RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            max_candidates: Some(100),
            min_line_chars: 3,
            include_two_line_blocks: false,
            ..RepeatedArtifactScanConfig::default()
        }
    }

    fn repeated_distinct_exact_lines(count: usize, repeats: usize) -> String {
        let mut lines = Vec::new();
        for index in 0..count {
            let line = format!("Repeated artifact key {index:03}");
            for _ in 0..repeats {
                lines.push(line.clone());
            }
        }
        lines.join("\n")
    }

    #[test]
    fn scan_report_exposes_manual_promotions_without_applying_them() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = (0..4)
            .map(|index| {
                make_text_record(
                    &format!("doc-{index}.txt"),
                    &format!(
                        "Running Header\nBody {index} a\nBody {index} b\nBody {index} c\nBody {index} d\nBody {index} e\nBody {index} f\nBody {index} g\nBody {index} h\nBody {index} i"
                    ),
                    temp_dir.path(),
                )
            })
            .collect::<Vec<_>>();
        let scan_config = RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 4,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&records, &scan_config, &CleaningConfig::default())
                .unwrap();
        assert_eq!(report.promotions.len(), report.candidates.len());
        let strong_candidate = report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.risk_label == ArtifactRiskLabel::StrongHeaderFooterCandidate
                    && candidate.display_text == "Running Header"
            })
            .unwrap();
        let promotion = report
            .promotions
            .iter()
            .find(|promotion| promotion.candidate_id == strong_candidate.candidate_id)
            .unwrap();

        let input = "Running Header\nBody";
        assert_eq!(
            crate::preparation::cleaning::clean_text(input, &CleaningConfig::default()),
            input
        );

        let cleaning_config = CleaningConfig {
            removal_rules: vec![promotion.rule.clone().unwrap()],
            ..CleaningConfig::default()
        };
        assert_eq!(
            crate::preparation::cleaning::clean_text(input, &cleaning_config),
            "Body"
        );
    }

    #[test]
    fn unbounded_scan_returns_candidates_past_old_cap() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = repeated_distinct_exact_lines(150, 3);
        let doc = make_text_record("uncapped.txt", &content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            min_occurrences: 3,
            min_files: 1,
            max_candidates: None,
            min_line_chars: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[doc], &config, &CleaningConfig::default()).unwrap();

        assert_eq!(report.candidates.len(), 150);
        assert_eq!(
            report.diagnostics.total_candidate_keys_before_filtering,
            150
        );
        assert_eq!(report.diagnostics.candidates_after_min_occurrences, 150);
        assert_eq!(report.diagnostics.candidates_after_min_files, 150);
        assert_eq!(report.diagnostics.candidates_before_cap, 150);
        assert_eq!(report.diagnostics.candidates_truncated, 0);
        assert_eq!(report.diagnostics.final_candidates, 150);
    }

    #[test]
    fn explicit_candidate_cap_is_reported_in_diagnostics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = repeated_distinct_exact_lines(150, 3);
        let doc = make_text_record("capped.txt", &content, temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            min_occurrences: 3,
            min_files: 1,
            max_candidates: Some(100),
            min_line_chars: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[doc], &config, &CleaningConfig::default()).unwrap();

        assert_eq!(report.candidates.len(), 100);
        assert_eq!(report.diagnostics.candidates_before_cap, 150);
        assert_eq!(report.diagnostics.candidates_truncated, 50);
        assert_eq!(report.diagnostics.final_candidates, 100);
    }

    #[test]
    fn numeric_dominant_exact_line_surfaces_by_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "10\nbody a\n10\nbody b\n10\n";
        let doc = make_text_record("numeric-default.txt", content, temp_dir.path());

        let report = scan_repeated_artifacts_report(
            &[doc],
            &RepeatedArtifactScanConfig::default(),
            &CleaningConfig::default(),
        )
        .unwrap();

        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine
                && candidate.display_text == "10"
                && candidate.content_class == CandidateContentClass::NumericDominant
        }));
    }

    #[test]
    fn two_line_running_head_surfaces_by_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let block = "Chapter 5\nRecurring Title";
        let content = [block, "Body a", block, "Body b", block].join("\n");
        let doc = make_text_record("two-line-default.txt", &content, temp_dir.path());

        let report = scan_repeated_artifacts_report(
            &[doc],
            &RepeatedArtifactScanConfig::default(),
            &CleaningConfig::default(),
        )
        .unwrap();

        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::TwoLineBlock && candidate.display_text == block
        }));
    }

    #[test]
    fn default_thresholds_surface_three_two_character_hits_only() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = "**\nAB\n**\nAB\n**\n";
        let doc = make_text_record("thresholds.txt", content, temp_dir.path());

        let report = scan_repeated_artifacts_report(
            &[doc],
            &RepeatedArtifactScanConfig::default(),
            &CleaningConfig::default(),
        )
        .unwrap();

        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine && candidate.display_text == "**"
        }));
        assert!(!report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine && candidate.display_text == "AB"
        }));
    }

    #[test]
    fn default_thresholds_surface_long_repeated_boilerplate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let boilerplate = "L".repeat(450);
        let content = (0..3)
            .map(|_| boilerplate.clone())
            .collect::<Vec<_>>()
            .join("\n");
        let doc = make_text_record("long-boilerplate.txt", &content, temp_dir.path());

        let report = scan_repeated_artifacts_report(
            &[doc],
            &RepeatedArtifactScanConfig::default(),
            &CleaningConfig::default(),
        )
        .unwrap();

        assert!(report.candidates.iter().any(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine
                && candidate.display_text == boilerplate
        }));
    }

    #[test]
    fn candidate_order_is_stable_across_input_order_and_thread_count() {
        let temp_dir = tempfile::tempdir().unwrap();
        let doc_a = make_text_record(
            "a.txt",
            "Shared Header\nAlpha body\nShared Footer\nShared Header\nShared Footer\n",
            temp_dir.path(),
        );
        let doc_b = make_text_record(
            "b.txt",
            "Shared Footer\nBeta body\nShared Header\nShared Footer\nShared Header\n",
            temp_dir.path(),
        );
        let config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            min_occurrences: 3,
            min_files: 1,
            max_candidates: None,
            ..RepeatedArtifactScanConfig::default()
        };
        let one_thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let four_threads = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();

        let candidates_one_thread = one_thread.install(|| {
            scan_repeated_artifacts_report(
                &[doc_a.clone(), doc_b.clone()],
                &config,
                &CleaningConfig::default(),
            )
            .unwrap()
            .candidates
        });
        let candidates_four_threads_reversed = four_threads.install(|| {
            scan_repeated_artifacts_report(&[doc_b, doc_a], &config, &CleaningConfig::default())
                .unwrap()
                .candidates
        });

        assert_eq!(candidates_one_thread, candidates_four_threads_reversed);
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
    fn scanner_promotion_matches_the_legacy_pdf_header_footer_fixture() {
        let temp_dir = tempfile::tempdir().unwrap();
        let record = make_pdf_record("promotion-parity.pdf", temp_dir.path());
        let pages = repeated_cached_pdf_pages();
        let raw_text = pages.join("\n\n");
        let scan_cleaning_config = CleaningConfig::default();
        let cache = ExtractionCache::new();
        cache.insert_extracted(
            &record,
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &scan_cleaning_config,
                ),
            ),
            &scan_cleaning_config,
            crate::sources::cache::CacheEntry {
                extracted_text: raw_text.clone(),
                warnings: Vec::new(),
                page_count: Some(pages.len()),
                page_texts: Some(pages.clone()),
            },
        );
        let scan_config = RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 3,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        };

        let report = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[record],
            &scan_config,
            &scan_cleaning_config,
            Some(&cache),
            &no_cancellation(),
        )
        .unwrap();
        let manually_marked_candidate_ids = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.risk_label == ArtifactRiskLabel::StrongHeaderFooterCandidate
            })
            .map(|candidate| candidate.candidate_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let rule_config = CleaningConfig {
            removal_rules: report
                .promotions
                .into_iter()
                .filter(|promotion| {
                    manually_marked_candidate_ids.contains(promotion.candidate_id.as_str())
                })
                .filter_map(|promotion| promotion.rule)
                .collect(),
            ..CleaningConfig::default()
        };
        let structured = StructuredDocument::from_pages(pages);
        let promoted =
            crate::preparation::cleaning::clean_structured_document(&structured, &rule_config).text;
        let fallback_config = CleaningConfig {
            remove_repeated_pdf_headers_footers: true,
            ..CleaningConfig::default()
        };
        let (legacy, _) =
            crate::sources::pdf::clean_extracted_pdf_text(&raw_text, &fallback_config);

        for artifact in ["Header", "Footer"] {
            assert!(!promoted.contains(artifact));
            assert!(!legacy.contains(artifact));
        }
        assert!(promoted.contains("Middle prose"));
        assert!(legacy.contains("Middle prose"));
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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
            max_candidates: Some(50),
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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
            max_candidates: Some(50),
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
        if !crate::sources::pdf::ocr::pdfium_available() {
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
            extraction_route: None,
            pdf_profile: None,
            ocr_quality: None,
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
            max_candidates: Some(100),
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
            max_line_chars: 300,
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
        if !crate::sources::pdf::ocr::pdfium_available() {
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
            extraction_route: None,
            pdf_profile: None,
            ocr_quality: None,
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
    fn test_default_max_line_chars_allows_long_boilerplate() {
        assert!(
            RepeatedArtifactScanConfig::default().max_line_chars >= 600,
            "default max_line_chars should allow long repeated boilerplate"
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
    fn test_numeric_dominant_normalised_surface_by_default() {
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
            norm_numeric,
            "numeric-dominant lines should produce NormalizedLine by default"
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
            include_numeric_dominant: false,
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
        assert!(cfg.include_numeric_dominant);
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
            extraction_route: None,
            pdf_profile: None,
            ocr_quality: None,
        }
    }

    fn force_ocr_cleaning_config() -> CleaningConfig {
        CleaningConfig {
            pdf_text_source: crate::preparation::cleaning::PdfTextSource::ForceOcr,
            pdf_ocr_quality: crate::preparation::cleaning::PdfOcrQuality::HighQuality,
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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

        let extracted = get_document_text_with_status(
            &doc,
            config.analyse_processed_text,
            &cleaning_config,
            Some(&cache),
        );
        let ft = build_file_text(&doc, &extracted.text, extracted.page_texts.as_deref());

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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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

        let extracted = get_document_text_with_status(
            &doc,
            config.analyse_processed_text,
            &cleaning_config,
            Some(&cache),
        );
        let ft = build_file_text(&doc, &extracted.text, extracted.page_texts.as_deref());

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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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
            Some(
                crate::sources::pdf::PdfExtractionOptions::raw_from_cleaning_config(
                    &cleaning_config,
                ),
            ),
            &cleaning_config,
            crate::sources::cache::CacheEntry {
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

    #[test]
    fn two_pass_matches_filter_late_reference_on_mixed_synthetic_corpus() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![
            make_text_record(
                "a.txt",
                "Running Header\nPage 1\nBoilerplate line\nRepeated block first\nRepeated block second\nBody A <br/>",
                temp_dir.path(),
            ),
            make_text_record(
                "b.txt",
                "Running Header\nPage 2\nBoilerplate line\nRepeated block first\nRepeated block second\nBody B <br/>",
                temp_dir.path(),
            ),
            make_text_record(
                "c.txt",
                "Unique C\nPage 3\nBoilerplate line\nRepeated block first\nRepeated block second\nBody C <br/>\nLate Survivor",
                temp_dir.path(),
            ),
            make_text_record(
                "d.txt",
                "Unique D\nPage 4\nBoilerplate line\nRepeated block first\nRepeated block second\nBody D <br/>\nLate Survivor",
                temp_dir.path(),
            ),
        ];
        let config = RepeatedArtifactScanConfig {
            include_three_line_blocks: false,
            min_occurrences: 2,
            min_files: 2,
            max_candidates: None,
            max_examples_per_candidate: 2,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&records, &config, &CleaningConfig::default()).unwrap();

        // This is the complete filter-late reference snapshot for the fixture.
        let actual = report
            .candidates
            .iter()
            .map(|candidate| {
                (
                    format!("{:?}|{}", candidate.kind, candidate.display_text),
                    (candidate.occurrence_count, candidate.file_count),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = [
            ("ExactLine|Running Header", (2, 2)),
            ("ExactLine|Boilerplate line", (4, 4)),
            ("ExactLine|Repeated block first", (4, 4)),
            ("ExactLine|Repeated block second", (4, 4)),
            ("ExactLine|Late Survivor", (2, 2)),
            ("NormalizedLine|Page 1", (4, 4)),
            (
                "TwoLineBlock|Boilerplate line\nRepeated block first",
                (4, 4),
            ),
            (
                "TwoLineBlock|Repeated block first\nRepeated block second",
                (4, 4),
            ),
            ("InlineArtifact|<br/>", (4, 4)),
        ]
        .into_iter()
        .map(|(key, counts)| (key.to_string(), counts))
        .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(actual, expected);

        let normalized_page = report
            .candidates
            .iter()
            .find(|candidate| {
                candidate.kind == RepeatedArtifactKind::NormalizedLine
                    && candidate.normalized_key == "page #"
            })
            .unwrap();
        assert_eq!(
            normalized_page.raw_variants,
            vec!["Page 1", "Page 2", "Page 3", "Page 4"]
        );
        assert!(report.candidates.iter().all(|candidate| {
            candidate.example_count == 2
                && candidate.position_summary.top_count
                    + candidate.position_summary.middle_count
                    + candidate.position_summary.bottom_count
                    + candidate.position_summary.unknown_count
                    == candidate.occurrence_count
        }));
    }

    #[test]
    fn mostly_unique_hundred_thousand_line_corpus_returns_only_small_survivor_set() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut lines = Vec::with_capacity(100_003);
        lines.push("Shared survivor".to_string());
        for index in 0..100_000 {
            lines.push(format!("Unique synthetic line {index:06}"));
            if index == 49_999 {
                lines.push("Shared survivor".to_string());
            }
        }
        lines.push("Shared survivor".to_string());
        let record = make_text_record("mostly-unique.txt", &lines.join("\n"), temp_dir.path());
        let config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            min_occurrences: 3,
            min_files: 1,
            max_candidates: None,
            ..RepeatedArtifactScanConfig::default()
        };

        let report =
            scan_repeated_artifacts_report(&[record], &config, &CleaningConfig::default()).unwrap();

        assert_eq!(
            report.diagnostics.total_candidate_keys_before_filtering,
            100_001
        );
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].display_text, "Shared survivor");
        assert_eq!(report.candidates[0].occurrence_count, 3);
    }

    #[test]
    fn cancellation_interrupts_fingerprint_counting_inside_large_file_promptly() {
        let temp_dir = tempfile::tempdir().unwrap();
        let content = (0..200_000)
            .map(|index| format!("Unique cancellation line {index:06}"))
            .collect::<Vec<_>>()
            .join("\n");
        let record = make_text_record("cancel-mid-file.txt", &content, temp_dir.path());
        let file_text = build_file_text(&record, &content, None);
        let cancel = no_cancellation();
        let cancel_from_thread = cancel.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            cancel_from_thread.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();

        let result = count_file_fingerprints(
            &file_text,
            &RepeatedArtifactScanConfig::default(),
            0,
            &cancel,
        );
        canceller.join().unwrap();

        assert!(matches!(result, Err(RepeatedArtifactScanError::Cancelled)));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "mid-file cancellation took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn scan_reports_real_two_pass_phase_progress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let records = vec![
            make_text_record("a.txt", "Shared line\nUnique A", temp_dir.path()),
            make_text_record("b.txt", "Shared line\nUnique B", temp_dir.path()),
        ];
        let config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            min_occurrences: 2,
            min_files: 2,
            ..RepeatedArtifactScanConfig::default()
        };
        let mut progress = Vec::new();

        let report = scan_repeated_artifacts_report_with_cancel_cache_and_progress(
            &records,
            &config,
            &CleaningConfig::default(),
            None,
            &no_cancellation(),
            |event| progress.push(event),
        )
        .unwrap();

        assert_eq!(report.candidates.len(), 1);
        let mut phases = progress.iter().map(|event| event.phase).collect::<Vec<_>>();
        phases.dedup();
        assert_eq!(
            phases,
            vec![
                RepeatedArtifactScanPhase::Pass1Scanning,
                RepeatedArtifactScanPhase::Filtering,
                RepeatedArtifactScanPhase::Pass2Materialising,
            ]
        );
        assert!(progress.iter().any(|event| {
            event.phase == RepeatedArtifactScanPhase::Pass1Scanning
                && event.files_completed == 2
                && event.files_total == 2
        }));
        assert!(progress.iter().any(|event| {
            event.phase == RepeatedArtifactScanPhase::Pass2Materialising
                && event.files_completed == 2
                && event.files_total == 2
        }));
    }

    #[test]
    fn configured_ceiling_returns_structured_errors_for_both_accumulation_stages() {
        let temp_dir = tempfile::tempdir().unwrap();
        let unique_content = (0..100)
            .map(|index| format!("Unique ceiling line {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let unique_record = make_text_record("pass1-limit.txt", &unique_content, temp_dir.path());
        let pass1_config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            min_occurrences: 1,
            memory_ceiling_bytes: PASS1_ESTIMATED_BYTES_PER_KEY * 10,
            ..RepeatedArtifactScanConfig::default()
        };

        let pass1_error = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[unique_record],
            &pass1_config,
            &CleaningConfig::default(),
            None,
            &no_cancellation(),
        )
        .unwrap_err();
        assert!(matches!(
            pass1_error,
            RepeatedArtifactScanError::CorpusTooLarge {
                stage: RepeatedArtifactMemoryStage::Pass1Counting,
                ..
            }
        ));

        let survivor_record = make_text_record(
            "materialisation-limit.txt",
            "Repeated survivor\nRepeated survivor\nRepeated survivor",
            temp_dir.path(),
        );
        let materialisation_config = RepeatedArtifactScanConfig {
            include_normalized_lines: false,
            include_inline_artifacts: false,
            include_two_line_blocks: false,
            include_three_line_blocks: false,
            min_occurrences: 3,
            min_files: 1,
            memory_ceiling_bytes: 200,
            ..RepeatedArtifactScanConfig::default()
        };

        let materialisation_error = scan_repeated_artifacts_report_with_cancel_and_cache(
            &[survivor_record],
            &materialisation_config,
            &CleaningConfig::default(),
            None,
            &no_cancellation(),
        )
        .unwrap_err();
        assert!(matches!(
            materialisation_error,
            RepeatedArtifactScanError::CorpusTooLarge {
                stage: RepeatedArtifactMemoryStage::SurvivorMaterialisation,
                ..
            }
        ));
        if let RepeatedArtifactScanError::CorpusTooLarge { suggestions, .. } = materialisation_error
        {
            assert_eq!(suggestions.len(), 3);
        }
    }

    #[test]
    fn pass1_accounting_entries_are_fixed_size_and_string_free() {
        assert!(
            std::mem::size_of::<(CandidateFingerprint, FingerprintStats)>()
                <= PASS1_ESTIMATED_BYTES_PER_KEY
        );
    }
}
