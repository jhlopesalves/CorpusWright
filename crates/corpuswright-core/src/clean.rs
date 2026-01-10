use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

use crate::structured_document::StructuredDocument;
use crate::text_normalization::normalize_line_for_repeated_artifact;
use crate::text_profile::{
    TextSignalContext, classify_text_signal, profile_text_line, text_signal_reasons,
};

lazy_static! {
    static ref RE_EXCESSIVE_SPACES: Regex = Regex::new(r" {2,}").unwrap();
    static ref RE_EXCESSIVE_HORIZONTAL_WHITESPACE: Regex = Regex::new(r"[ \t]{2,}").unwrap();
    static ref RE_PAGE_MARKER_ARABIC: Regex = Regex::new(r"^\d{1,5}$").unwrap();
    static ref RE_PAGE_MARKER_ROMAN: Regex =
        Regex::new(r"(?i)^M{0,4}(?:CM|CD|D?C{0,3})(?:XC|XL|L?X{0,3})(?:IX|IV|V?I{0,3})$")
            .unwrap();
    static ref RE_PAGE_MARKER_LABEL: Regex =
        Regex::new(r"(?i)^(?:page|pag\.|p\.|página)\s*(\d{1,5}|[ivxlcdm]{1,15})\.?$")
            .unwrap();
    static ref RE_URL_ONLY_LINE: Regex =
        Regex::new(r"(?i)^(?:https?://|www\.)[^\s<>]+$").unwrap();
    static ref RE_EMAIL_ONLY_LINE: Regex =
        Regex::new(r"(?i)^[^\s@<>]+@[^\s@<>]+\.[^\s@<>]+$").unwrap();
    static ref RE_DOI_ONLY_LINE: Regex =
        Regex::new(r"(?i)^(?:doi:\s*)?10\.\d{4,9}/\S+$").unwrap();
    static ref RE_FIGURE_TABLE_CAPTION_LINE: Regex = Regex::new(
        r"(?i)^(?:figure|fig\.|table|tabela|quadro|gr[aá]fico)\s+(?:\d+|[ivxlcdm]+)(?:[\s:.\-–—].*)?$"
    )
    .unwrap();
}

const MAX_ROMAN_PAGE_MARKER_VALUE: u32 = 399;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum TableExtractionStrategy {
    #[default]
    TabSeparated,
    FlattenParagraphs,
    Ignore,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum PdfEmbeddedTextStrategy {
    #[default]
    PdfiumFlat,
    PdfiumVisualSingleColumn,
    PdfiumVisualColumnsExperimental,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum PdfTextSource {
    #[default]
    EmbeddedText,
    Ocr,
    ForceOcr,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum PdfOcrQuality {
    Fast,
    #[default]
    Balanced,
    HighQuality,
}

/// Configuration options for text cleaning operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CleaningConfig {
    #[serde(default)]
    pub normalize_whitespace_line_endings: bool,
    #[serde(default)]
    pub repair_wrapped_pdf_line_breaks: bool,
    #[serde(default)]
    pub remove_isolated_page_markers: bool,
    #[serde(default)]
    pub remove_url_only_lines: bool,
    #[serde(default)]
    pub remove_email_only_lines: bool,
    #[serde(default)]
    pub remove_doi_only_lines: bool,
    #[serde(default)]
    pub remove_figure_table_captions: bool,
    pub join_line_breaks: bool,
    pub normalize_irregular_line_breaks: bool,
    pub remove_standalone_page_numbers: bool,
    pub remove_standalone_roman_page_numbers: bool,
    pub remove_page_indicators: bool,
    pub remove_page_delimiters: bool,
    pub lowercase: bool,
    pub trim_lines: bool,
    pub collapse_blank_lines: bool,
    pub normalize_line_endings: bool,
    pub normalize_unicode: bool,
    pub replace_diacritics: bool,
    pub extract_html: bool,
    /// Remove full lines that are overwhelmingly obvious extraction/OCR/markup noise.
    #[serde(default)]
    pub remove_obvious_extraction_noise: bool,
    pub table_extraction_strategy: TableExtractionStrategy,
    pub remove_headers: bool,
    pub remove_footers: bool,
    pub remove_footnotes: bool,
    pub remove_endnotes: bool,
    pub remove_comments: bool,
    pub remove_table_of_contents: bool,
    pub remove_patterns: Vec<String>,
    #[serde(default)]
    pub removal_rules: Vec<RemovalRule>,
    pub replace_patterns: Vec<ReplacementRule>,
    /// PDF text source used before normal text cleaning is applied.
    #[serde(default)]
    pub pdf_text_source: PdfTextSource,
    /// OCR render quality used when PDF OCR runs.
    #[serde(default)]
    pub pdf_ocr_quality: PdfOcrQuality,
    /// PDF extraction strategy.
    /// NOTE: This is an extraction-layer option specifying how raw PDF text
    /// is reconstructed from the character stream, NOT a text-cleaning/sanitization transformation.
    #[serde(default)]
    pub pdf_embedded_text_strategy: PdfEmbeddedTextStrategy,
    /// PDF-specific post-extraction cleanup option to remove repeated headers and footers across pages.
    #[serde(default)]
    pub remove_repeated_pdf_headers_footers: bool,
    /// PDF-specific post-extraction cleanup option to remove page label/page number lines from top/bottom zones.
    #[serde(default)]
    pub remove_pdf_page_labels: bool,
    /// PDF-specific post-extraction cleanup option to remove symbol-heavy graphical/plotting noise lines.
    #[serde(default)]
    pub remove_pdf_symbol_heavy_artifacts: bool,
    /// PDF-specific post-extraction cleanup option to remove code-like blocks.
    #[serde(default)]
    pub remove_pdf_code_like_blocks: bool,
    /// PDF-specific post-extraction cleanup option to remove formula/math-heavy lines.
    #[serde(default)]
    pub remove_pdf_formula_like_lines: bool,
}

/// Origin of a structured removal rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RemovalRuleSource {
    Manual,
    PromotedRepeatedArtifact,
    GeneratedPdfCleanup,
}

/// Text area affected by a structured removal rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RemovalScope {
    Anywhere,
    WholeLine,
    PageTop,
    PageBottom,
    PageTopOrBottom,
}

/// Matcher used by a structured removal rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum RemovalMatcher {
    Literal { text: String },
    NormalizedLine { normalized_key: String },
}

/// Structured removal rule applied during text cleaning.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RemovalRule {
    pub id: String,
    pub label: String,
    pub source: RemovalRuleSource,
    pub matcher: RemovalMatcher,
    pub scope: RemovalScope,
    pub enabled: bool,
}

#[derive(Deserialize)]
struct RawRemovalRule {
    id: String,
    label: String,
    source: RemovalRuleSource,
    matcher: RemovalMatcher,
    scope: RemovalScope,
    enabled: bool,
}

impl<'de> Deserialize<'de> for RemovalRule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawRemovalRule::deserialize(deserializer)?;
        let mut rule = Self {
            id: raw.id,
            label: raw.label,
            source: raw.source,
            matcher: raw.matcher,
            scope: raw.scope,
            enabled: raw.enabled,
        };
        if invalid_anywhere_rule_reason(&rule).is_some() {
            rule.enabled = false;
        }
        Ok(rule)
    }
}

fn invalid_anywhere_rule_reason(rule: &RemovalRule) -> Option<&'static str> {
    if rule.scope != RemovalScope::Anywhere {
        return None;
    }

    match &rule.matcher {
        RemovalMatcher::NormalizedLine { .. } => {
            Some("Normalised-line matchers cannot use Inline literal scope.")
        }
        RemovalMatcher::Literal { text } if text.trim().is_empty() => {
            Some("Inline literal text cannot be empty.")
        }
        RemovalMatcher::Literal { text } if text.contains(['\r', '\n']) => {
            Some("Inline literal text cannot contain line breaks.")
        }
        RemovalMatcher::Literal { .. } => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ReplacementRule {
    pub pattern: String,
    pub replacement: String,
}

/// Cleaned text with optional page text metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanedStructuredDocument {
    /// The canonical flat output produced by the existing cleaning pipeline.
    pub text: String,
    /// Cleaned page text when page boundaries survive cleaning unchanged.
    pub page_texts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CleaningOperation {
    PageDelimiter,
    PageIndicator,
    StandalonePageLabel,
    ObviousExtractionNoise,
    StructuredRemovalRule,
    LiteralPattern,
    ReplacementPattern,
    TrimLines,
    CollapseBlankLines,
    LineReflow,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RemovedTextExample {
    pub removed_text: String,
    pub line_number: Option<usize>,
    pub page_number: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RemovalSummary {
    pub operation: CleaningOperation,
    pub rule_id: Option<String>,
    pub rule_source: Option<RemovalRuleSource>,
    pub reason_codes: Vec<String>,
    pub source_id: Option<String>,
    pub source_label: Option<String>,
    pub occurrence_count: usize,
    pub examples: Vec<RemovedTextExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SkippedRuleNotice {
    pub rule_id: Option<String>,
    pub rule_source: Option<RemovalRuleSource>,
    pub source_id: Option<String>,
    pub source_label: Option<String>,
    pub notice: String,
    pub occurrence_count: usize,
}

impl SkippedRuleNotice {
    /// Summarises skip notices across the corpus into a human-readable warning.
    pub fn format_corpus_summary(notices: &[SkippedRuleNotice]) -> Option<String> {
        let mut skipped_sources = std::collections::HashSet::new();
        let mut skipped_rules = std::collections::HashSet::new();

        for notice in notices {
            if notice.notice
                == "Requires reliable page metadata. Skipped on sources without page zones."
            {
                if let Some(ref source) = notice.source_id {
                    skipped_sources.insert(source.clone());
                }
                if let Some(ref rule) = notice.rule_id {
                    skipped_rules.insert(rule.clone());
                }
            }
        }

        if skipped_sources.is_empty() {
            None
        } else {
            let n = skipped_rules.len();
            let n_str = if n == 0 {
                "Some".to_string()
            } else {
                n.to_string()
            };
            Some(format!(
                "{} page-zone rules were skipped because page metadata was unreliable for {} sources.",
                n_str,
                skipped_sources.len()
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CleaningCount {
    pub operation: CleaningOperation,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ReasonCount {
    pub reason: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CleaningReport {
    pub summaries: Vec<RemovalSummary>,
    pub skipped_rules: Vec<SkippedRuleNotice>,
    pub totals_by_operation: Vec<CleaningCount>,
    pub totals_by_reason: Vec<ReasonCount>,
}

impl CleaningReport {
    pub(crate) fn merge<'a>(reports: impl IntoIterator<Item = &'a CleaningReport>) -> Self {
        use std::collections::BTreeMap;

        let mut summaries: BTreeMap<String, RemovalSummary> = BTreeMap::new();
        let mut skipped_rules: BTreeMap<String, SkippedRuleNotice> = BTreeMap::new();
        let mut totals_by_operation: BTreeMap<CleaningOperation, usize> = BTreeMap::new();
        let mut totals_by_reason: BTreeMap<String, usize> = BTreeMap::new();

        for report in reports {
            for count in &report.totals_by_operation {
                *totals_by_operation.entry(count.operation).or_default() += count.count;
            }
            for count in &report.totals_by_reason {
                *totals_by_reason.entry(count.reason.clone()).or_default() += count.count;
            }
            for notice in &report.skipped_rules {
                let key = format!(
                    "{:?}|{:?}|{:?}|{}",
                    notice.rule_id, notice.rule_source, notice.source_id, notice.notice
                );
                skipped_rules
                    .entry(key)
                    .and_modify(|existing| {
                        existing.occurrence_count += notice.occurrence_count;
                    })
                    .or_insert_with(|| notice.clone());
            }
            for item in &report.summaries {
                let aggregate_rule = item.rule_id.is_some();
                let key = format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}",
                    item.operation,
                    item.rule_id,
                    item.rule_source,
                    item.reason_codes,
                    (!aggregate_rule).then_some(&item.source_id)
                );
                let summary = summaries.entry(key).or_insert_with(|| {
                    let mut summary = item.clone();
                    summary.occurrence_count = 0;
                    summary.examples.clear();
                    if aggregate_rule {
                        summary.source_id = None;
                        summary.source_label = None;
                    }
                    summary
                });
                summary.occurrence_count += item.occurrence_count;
                for example in &item.examples {
                    if summary.examples.len() >= 20 {
                        break;
                    }
                    summary.examples.push(example.clone());
                }
            }
        }

        Self {
            summaries: summaries.into_values().collect(),
            skipped_rules: skipped_rules.into_values().collect(),
            totals_by_operation: totals_by_operation
                .into_iter()
                .map(|(operation, count)| CleaningCount { operation, count })
                .collect(),
            totals_by_reason: totals_by_reason
                .into_iter()
                .map(|(reason, count)| ReasonCount { reason, count })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CleaningContext<'a> {
    pub source_id: Option<&'a str>,
    pub source_label: Option<&'a str>,
    pub page_number: Option<usize>,
}

#[derive(Default)]
pub(crate) struct CleaningReportCollector {
    // Map to quickly group summaries: operation -> rule_source -> reason_codes -> index in summaries
    // For simplicity we can just iterate to find an existing summary or use a HashMap.
    // However, since we want to accumulate occurrences and capped examples, doing a linear scan might be fine for small numbers,
    events: Vec<RemovalEvent>,
    skipped_rules: Vec<SkippedRuleNotice>,
}

struct RemovalEvent {
    operation: CleaningOperation,
    rule_id: Option<String>,
    rule_source: Option<RemovalRuleSource>,
    reason_codes: Vec<String>,
    source_id: Option<String>,
    source_label: Option<String>,
    occurrence_count: usize,
    examples: Vec<RemovedTextExample>,
}

impl CleaningReportCollector {
    pub fn record_skipped_rule(
        &mut self,
        rule_id: Option<String>,
        rule_source: Option<RemovalRuleSource>,
        context: Option<&CleaningContext<'_>>,
        notice: String,
    ) {
        let source_id = context.and_then(|c| c.source_id).map(String::from);
        if let Some(existing) = self.skipped_rules.iter_mut().find(|r| {
            r.rule_id == rule_id
                && r.rule_source == rule_source
                && r.source_id == source_id
                && r.notice == notice
        }) {
            existing.occurrence_count += 1;
        } else {
            self.skipped_rules.push(SkippedRuleNotice {
                rule_id,
                rule_source,
                source_id,
                source_label: context.and_then(|c| c.source_label).map(String::from),
                notice,
                occurrence_count: 1,
            });
        }
    }

    pub fn record_removal(
        &mut self,
        operation: CleaningOperation,
        rule_source: Option<RemovalRuleSource>,
        reason_codes: Vec<String>,
        context: Option<&CleaningContext>,
        example: RemovedTextExample,
    ) {
        self.events.push(RemovalEvent {
            operation,
            rule_id: None,
            rule_source,
            reason_codes,
            source_id: context.and_then(|c| c.source_id.map(|s| s.to_string())),
            source_label: context.and_then(|c| c.source_label.map(|s| s.to_string())),
            occurrence_count: 1,
            examples: vec![example],
        });
    }

    fn record_rule_removals(
        &mut self,
        rule: &RemovalRule,
        reason_codes: Vec<String>,
        context: Option<&CleaningContext>,
        occurrence_count: usize,
        examples: Vec<RemovedTextExample>,
    ) {
        self.events.push(RemovalEvent {
            operation: CleaningOperation::StructuredRemovalRule,
            rule_id: Some(rule.id.clone()),
            rule_source: Some(rule.source),
            reason_codes,
            source_id: context.and_then(|c| c.source_id.map(str::to_string)),
            source_label: context.and_then(|c| c.source_label.map(str::to_string)),
            occurrence_count,
            examples,
        });
    }

    pub fn finish(self) -> CleaningReport {
        use std::collections::BTreeMap;
        let mut summary_map: BTreeMap<String, RemovalSummary> = BTreeMap::new();
        let mut totals_by_operation: BTreeMap<CleaningOperation, usize> = BTreeMap::new();
        let mut totals_by_reason: BTreeMap<String, usize> = BTreeMap::new();

        for event in self.events {
            // Update operation totals
            *totals_by_operation.entry(event.operation).or_default() += event.occurrence_count;

            // Update reason totals
            for reason in &event.reason_codes {
                *totals_by_reason.entry(reason.clone()).or_default() += event.occurrence_count;
            }

            // Group summary
            let key = format!(
                "{:?}|{:?}|{:?}|{:?}",
                event.operation, event.rule_id, event.rule_source, event.reason_codes
            );
            let summary = summary_map.entry(key).or_insert_with(|| RemovalSummary {
                operation: event.operation,
                rule_id: event.rule_id,
                rule_source: event.rule_source,
                reason_codes: event.reason_codes,
                source_id: event.source_id,
                source_label: event.source_label,
                occurrence_count: 0,
                examples: Vec::new(),
            });

            summary.occurrence_count += event.occurrence_count;
            for example in event.examples {
                if summary.examples.len() >= 20 {
                    break;
                }
                let mut truncated_text = example.removed_text;
                if truncated_text.chars().count() > 200 {
                    truncated_text = truncated_text.chars().take(197).collect::<String>() + "...";
                }
                summary.examples.push(RemovedTextExample {
                    removed_text: truncated_text,
                    line_number: example.line_number,
                    page_number: example.page_number,
                });
            }
        }

        let summaries = summary_map.into_values().collect();
        let totals_by_op = totals_by_operation
            .into_iter()
            .map(|(operation, count)| CleaningCount { operation, count })
            .collect();
        let totals_by_res = totals_by_reason
            .into_iter()
            .map(|(reason, count)| ReasonCount { reason, count })
            .collect();

        CleaningReport {
            summaries,
            skipped_rules: self.skipped_rules,
            totals_by_operation: totals_by_op,
            totals_by_reason: totals_by_res,
        }
    }
}

/// Processes a string according to the given cleaning rules.
pub fn clean_text(text: &str, config: &CleaningConfig) -> String {
    clean_text_impl(text, config, None, None)
}

/// Processes a string and returns countable deterministic cleaning effects.
pub(crate) fn clean_text_impl(
    text: &str,
    config: &CleaningConfig,
    context: Option<&CleaningContext>,
    mut collector: Option<&mut CleaningReportCollector>,
) -> String {
    let mut cleaned = text.to_string();

    if config.normalize_whitespace_line_endings || config.normalize_line_endings {
        cleaned = cleaned.replace("\r\n", "\n").replace('\r', "\n");
    }

    if config.replace_diacritics {
        // Decompose to NFD before stripping combining diacritical marks.
        cleaned = cleaned
            .nfd()
            .filter(|c| !matches!(*c, '\u{0300}'..='\u{036f}'))
            .collect::<String>();
    }

    if config.normalize_unicode {
        // NFC is the most common composed Unicode form.
        cleaned = cleaned.nfc().collect::<String>();
    }

    if config.lowercase {
        cleaned = cleaned.to_lowercase();
    }

    if config.remove_url_only_lines {
        cleaned = apply_line_removal(
            &cleaned,
            &mut collector,
            context,
            CleaningOperation::Other,
            |line| is_url_only_line(line).then(|| vec!["remove_url_only_lines".to_string()]),
        );
    }

    if config.remove_email_only_lines {
        cleaned = apply_line_removal(
            &cleaned,
            &mut collector,
            context,
            CleaningOperation::Other,
            |line| is_email_only_line(line).then(|| vec!["remove_email_only_lines".to_string()]),
        );
    }

    if config.remove_doi_only_lines {
        cleaned = apply_line_removal(
            &cleaned,
            &mut collector,
            context,
            CleaningOperation::Other,
            |line| is_doi_only_line(line).then(|| vec!["remove_doi_only_lines".to_string()]),
        );
    }

    if config.remove_figure_table_captions {
        cleaned = apply_line_removal(
            &cleaned,
            &mut collector,
            context,
            CleaningOperation::Other,
            |line| {
                is_figure_table_caption_line(line)
                    .then(|| vec!["remove_figure_table_captions".to_string()])
            },
        );
    }

    if config.remove_obvious_extraction_noise {
        cleaned = apply_line_removal(
            &cleaned,
            &mut collector,
            context,
            CleaningOperation::ObviousExtractionNoise,
            should_remove_obvious_extraction_noise_line,
        );
    }

    if config.repair_wrapped_pdf_line_breaks
        || config.normalize_irregular_line_breaks
        || config.join_line_breaks
    {
        cleaned = normalize_irregular_line_breaks(&cleaned);
    }

    cleaned = remove_structured_removal_rules(cleaned, config, &mut collector, context);
    cleaned = remove_literal_patterns(cleaned, config);

    for rule in &config.replace_patterns {
        if !rule.pattern.is_empty() {
            let p = if config.lowercase {
                rule.pattern.to_lowercase()
            } else {
                rule.pattern.to_string()
            };
            cleaned = cleaned.replace(&p, &rule.replacement);
        }
    }

    if config.normalize_whitespace_line_endings {
        cleaned = normalize_horizontal_whitespace(&cleaned);
    }

    if config.normalize_whitespace_line_endings || config.trim_lines {
        cleaned = cleaned
            .split('\n')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
    }

    if config.normalize_whitespace_line_endings || config.collapse_blank_lines {
        cleaned = collapse_blank_lines(&cleaned);
    }

    cleaned
}

pub fn clean_text_with_report(
    text: &str,
    config: &CleaningConfig,
    context: Option<&CleaningContext>,
) -> (String, CleaningReport) {
    let mut collector = CleaningReportCollector::default();
    let cleaned = clean_text_impl(text, config, context, Some(&mut collector));
    (cleaned, collector.finish())
}

/// Cleans a page-aware document without exposing stale page metadata.
///
/// The flat output is always the same result as `clean_text` on the document's
/// deterministic flat text. Cleaned page text is returned only when cleaning
/// each page independently and joining the cleaned pages produces exactly the
/// same flat output.
fn prepare_line_for_rule_matching(line_text: &str, config: &CleaningConfig) -> String {
    let mut cleaned = line_text.to_string();

    if config.replace_diacritics {
        cleaned = cleaned
            .nfd()
            .filter(|c| !matches!(*c, '\u{0300}'..='\u{036f}'))
            .collect::<String>();
    }

    if config.normalize_unicode {
        cleaned = cleaned.nfc().collect::<String>();
    }

    if config.lowercase {
        cleaned = cleaned.to_lowercase();
    }

    cleaned
}

fn prepare_rule_text_for_matching(text: &str, config: &CleaningConfig) -> String {
    let mut cleaned = text.to_string();

    if config.replace_diacritics {
        cleaned = cleaned
            .nfd()
            .filter(|c| !matches!(*c, '\u{0300}'..='\u{036f}'))
            .collect::<String>();
    }

    if config.normalize_unicode {
        cleaned = cleaned.nfc().collect::<String>();
    }

    if config.lowercase {
        cleaned = cleaned.to_lowercase();
    }

    cleaned
}

fn prepare_normalized_key_for_rule_matching(key: &str, config: &CleaningConfig) -> String {
    let mut cleaned = key.to_string();

    if config.replace_diacritics {
        cleaned = cleaned
            .nfd()
            .filter(|c| !matches!(*c, '\u{0300}'..='\u{036f}'))
            .collect::<String>();
    }

    if config.normalize_unicode {
        cleaned = cleaned.nfc().collect::<String>();
    }

    cleaned
}

fn should_remove_line_by_rules(
    prepared_line: &str,
    is_top: bool,
    is_bottom: bool,
    config: &CleaningConfig,
) -> bool {
    if config.removal_rules.is_empty() {
        return false;
    }

    for rule in &config.removal_rules {
        if !rule.enabled {
            continue;
        }

        let scope_applies = match rule.scope {
            RemovalScope::WholeLine => true,
            RemovalScope::PageTop => is_top,
            RemovalScope::PageBottom => is_bottom,
            RemovalScope::PageTopOrBottom => is_top || is_bottom,
            RemovalScope::Anywhere => false,
        };

        if !scope_applies {
            continue;
        }

        match &rule.matcher {
            RemovalMatcher::Literal { text } => {
                let effective_text = prepare_rule_text_for_matching(text, config);
                let trimmed_rule = effective_text.trim();
                if !trimmed_rule.is_empty() && prepared_line.trim() == trimmed_rule {
                    return true;
                }
            }
            RemovalMatcher::NormalizedLine { normalized_key } => {
                let effective_key =
                    prepare_normalized_key_for_rule_matching(normalized_key, config);
                let trimmed_rule = effective_key.trim();
                if !trimmed_rule.is_empty() {
                    let normalized = normalize_line_for_repeated_artifact(prepared_line.trim());
                    if normalized.as_str() == trimmed_rule {
                        return true;
                    }
                }
            }
        }
    }

    false
}

pub(crate) fn has_page_zone_rules(config: &CleaningConfig) -> bool {
    config.removal_rules.iter().any(|rule| {
        rule.enabled
            && matches!(
                rule.scope,
                RemovalScope::PageTop | RemovalScope::PageBottom | RemovalScope::PageTopOrBottom
            )
    })
}

fn has_page_label_removal(config: &CleaningConfig) -> bool {
    config.remove_standalone_page_numbers
        || config.remove_standalone_roman_page_numbers
        || config.remove_page_indicators
        || config.remove_page_delimiters
        || config.remove_isolated_page_markers
}

pub(crate) fn requires_reliable_page_metadata(config: &CleaningConfig) -> bool {
    has_page_zone_rules(config) || has_page_label_removal(config)
}

fn without_page_label_removal(config: &CleaningConfig) -> CleaningConfig {
    let mut config = config.clone();
    config.remove_standalone_page_numbers = false;
    config.remove_standalone_roman_page_numbers = false;
    config.remove_page_indicators = false;
    config.remove_page_delimiters = false;
    config.remove_isolated_page_markers = false;
    config
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageMarkerNumber {
    Arabic(u32),
    Roman(u32),
}

impl PageMarkerNumber {
    fn value(self) -> u32 {
        match self {
            PageMarkerNumber::Arabic(value) | PageMarkerNumber::Roman(value) => value,
        }
    }

    fn same_numbering_system(self, other: Self) -> bool {
        matches!(
            (self, other),
            (PageMarkerNumber::Arabic(_), PageMarkerNumber::Arabic(_))
                | (PageMarkerNumber::Roman(_), PageMarkerNumber::Roman(_))
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageMarkerCandidate {
    number: PageMarkerNumber,
    is_decorated: bool,
    is_labelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageMarkerLineCandidate {
    page_position: usize,
    line_position: usize,
    is_top: bool,
    is_bottom: bool,
    marker: PageMarkerCandidate,
}

fn page_label_removal_decisions(
    document: &StructuredDocument,
    config: &CleaningConfig,
) -> Vec<Vec<bool>> {
    let mut decisions = document
        .pages
        .iter()
        .map(|page| vec![false; page.lines.len()])
        .collect::<Vec<_>>();

    if !has_page_label_removal(config) {
        return decisions;
    }

    let candidates = page_marker_line_candidates(document, config);
    for candidate in &candidates {
        if candidate.marker.is_decorated
            || candidate.marker.is_labelled
            || has_bare_page_marker_evidence(candidate, &candidates, document)
        {
            decisions[candidate.page_position][candidate.line_position] = true;
        }
    }

    decisions
}

fn page_marker_line_candidates(
    document: &StructuredDocument,
    config: &CleaningConfig,
) -> Vec<PageMarkerLineCandidate> {
    let mut candidates = Vec::new();

    for (page_position, page) in document.pages.iter().enumerate() {
        for (line_position, line) in page.lines.iter().enumerate() {
            if !line.is_page_top && !line.is_page_bottom {
                continue;
            }

            let Some(marker) = page_marker_candidate_for_line(&line.text) else {
                continue;
            };

            if !page_marker_allowed_by_config(marker, config) {
                continue;
            }

            candidates.push(PageMarkerLineCandidate {
                page_position,
                line_position,
                is_top: line.is_page_top,
                is_bottom: line.is_page_bottom,
                marker,
            });
        }
    }

    candidates
}

fn page_marker_allowed_by_config(marker: PageMarkerCandidate, config: &CleaningConfig) -> bool {
    config.remove_isolated_page_markers
        || (marker.is_labelled && config.remove_page_indicators)
        || (marker.is_decorated && config.remove_page_delimiters)
        || (!marker.is_labelled
            && match marker.number {
                PageMarkerNumber::Arabic(_) => config.remove_standalone_page_numbers,
                PageMarkerNumber::Roman(_) => config.remove_standalone_roman_page_numbers,
            })
}

fn has_bare_page_marker_evidence(
    candidate: &PageMarkerLineCandidate,
    candidates: &[PageMarkerLineCandidate],
    document: &StructuredDocument,
) -> bool {
    has_monotonic_page_marker_neighbour(candidate, candidates)
        || has_arabic_page_ordinal_evidence(candidate, document)
}

fn has_monotonic_page_marker_neighbour(
    candidate: &PageMarkerLineCandidate,
    candidates: &[PageMarkerLineCandidate],
) -> bool {
    candidates.iter().any(|other| {
        candidate.page_position.abs_diff(other.page_position) == 1
            && page_marker_edges_overlap(candidate, other)
            && candidate
                .marker
                .number
                .same_numbering_system(other.marker.number)
            && candidate
                .marker
                .number
                .value()
                .abs_diff(other.marker.number.value())
                == 1
    })
}

fn page_marker_edges_overlap(
    left: &PageMarkerLineCandidate,
    right: &PageMarkerLineCandidate,
) -> bool {
    (left.is_top && right.is_top) || (left.is_bottom && right.is_bottom)
}

fn has_arabic_page_ordinal_evidence(
    candidate: &PageMarkerLineCandidate,
    document: &StructuredDocument,
) -> bool {
    let PageMarkerNumber::Arabic(value) = candidate.marker.number else {
        return false;
    };

    value > 1 && value as usize == candidate.page_position + 1 && document.pages.len() > 1
}

fn page_marker_candidate_for_line(line: &str) -> Option<PageMarkerCandidate> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 40 {
        return None;
    }

    let (unframed, is_decorated) = trim_page_marker_decoration(trimmed);
    if unframed.is_empty() {
        return None;
    }

    if let Some(number) = labelled_page_marker_number(unframed) {
        return Some(PageMarkerCandidate {
            number,
            is_decorated,
            is_labelled: true,
        });
    }

    let number = bare_page_marker_number(unframed)?;
    Some(PageMarkerCandidate {
        number,
        is_decorated,
        is_labelled: false,
    })
}

fn trim_page_marker_decoration(line: &str) -> (&str, bool) {
    let mut trimmed = line.trim();
    let mut is_decorated = false;

    loop {
        let next = trim_one_page_marker_decoration(trimmed);
        if next == trimmed {
            return (trimmed, is_decorated);
        }
        trimmed = next;
        is_decorated = true;
    }
}

fn trim_one_page_marker_decoration(line: &str) -> &str {
    let dash_trimmed = line
        .trim_start_matches(is_page_marker_dash)
        .trim()
        .trim_end_matches(is_page_marker_dash)
        .trim();
    if dash_trimmed != line {
        return dash_trimmed;
    }

    if let Some(inner) = line
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    {
        return inner.trim();
    }

    let bar_trimmed = line
        .trim_start_matches('|')
        .trim()
        .trim_end_matches('|')
        .trim();
    if bar_trimmed != line {
        return bar_trimmed;
    }

    if let Some(without_period) = line.strip_suffix('.') {
        return without_period.trim_end();
    }

    line
}

fn is_page_marker_dash(character: char) -> bool {
    matches!(character, '-' | '–' | '—')
}

fn labelled_page_marker_number(marker: &str) -> Option<PageMarkerNumber> {
    let captures = RE_PAGE_MARKER_LABEL.captures(marker)?;
    let marker = captures.get(1).map(|value| value.as_str()).unwrap_or("");
    bare_page_marker_number(marker)
}

fn bare_page_marker_number(marker: &str) -> Option<PageMarkerNumber> {
    let marker = marker.trim();
    if RE_PAGE_MARKER_ARABIC.is_match(marker) {
        let value = marker.parse::<u32>().ok()?;
        if value > 0 {
            return Some(PageMarkerNumber::Arabic(value));
        }
    }

    roman_page_marker_value(marker).map(PageMarkerNumber::Roman)
}

fn roman_page_marker_value(marker: &str) -> Option<u32> {
    let trimmed = marker.trim();
    let roman_len = trimmed.chars().count();
    if roman_len == 0 || roman_len > 15 {
        return None;
    }
    if !trimmed.chars().all(is_ascii_roman_marker_char) {
        return None;
    }
    if !RE_PAGE_MARKER_ROMAN.is_match(trimmed) {
        return None;
    }

    let value = roman_numeral_value(trimmed)?;
    (value > 0 && value <= MAX_ROMAN_PAGE_MARKER_VALUE).then_some(value)
}

fn is_ascii_roman_marker_char(character: char) -> bool {
    matches!(
        character,
        'i' | 'I' | 'v' | 'V' | 'x' | 'X' | 'l' | 'L' | 'c' | 'C' | 'd' | 'D' | 'm' | 'M'
    )
}

fn roman_numeral_value(marker: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut previous = 0u32;

    for character in marker.chars().rev() {
        let value = match character.to_ascii_uppercase() {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        };

        if value < previous {
            total = total.saturating_sub(value);
        } else {
            total = total.saturating_add(value);
            previous = value;
        }
    }

    Some(total)
}

/// Cleans a page-aware document without exposing stale page metadata.
///
/// Metadata-aware removals are applied before each page is cleaned. Without
/// such removals, the flat output matches `clean_text` on the document's
/// deterministic flat text.
pub fn clean_structured_document(
    document: &StructuredDocument,
    config: &CleaningConfig,
) -> CleanedStructuredDocument {
    clean_structured_document_impl(document, config, None, None)
}

pub(crate) fn clean_structured_document_impl(
    document: &StructuredDocument,
    config: &CleaningConfig,
    mut collector: Option<&mut CleaningReportCollector>,
    context: Option<&CleaningContext<'_>>,
) -> CleanedStructuredDocument {
    if !requires_reliable_page_metadata(config) || !document.has_reliable_page_metadata {
        if !document.has_reliable_page_metadata {
            #[allow(clippy::collapsible_if)]
            if let Some(col) = collector.as_deref_mut() {
                for rule in &config.removal_rules {
                    if matches!(
                        rule.scope,
                        RemovalScope::PageTop
                            | RemovalScope::PageBottom
                            | RemovalScope::PageTopOrBottom
                    ) && rule.enabled
                    {
                        col.record_skipped_rule(
                            Some(rule.id.clone()),
                            Some(rule.source),
                            context,
                            "Requires reliable page metadata. Skipped on sources without page zones."
                                .to_string(),
                        );
                    }
                }
            }
        }

        let text = clean_text_impl(&document.to_flat_text(), config, context, collector);
        let page_texts = document
            .pages
            .iter()
            .map(|page| clean_text(&page.to_text(), config))
            .collect::<Vec<_>>();

        let joined_page_texts = page_texts.join("\n\n");
        let page_texts = (joined_page_texts == text).then_some(page_texts);

        CleanedStructuredDocument { text, page_texts }
    } else {
        let page_label_decisions = page_label_removal_decisions(document, config);
        let mut filtered_pages = Vec::with_capacity(document.pages.len());
        for (page_position, page) in document.pages.iter().enumerate() {
            let mut filtered_lines = Vec::with_capacity(page.lines.len());
            for (line_position, line) in page.lines.iter().enumerate() {
                let prepared = prepare_line_for_rule_matching(&line.text, config);
                if !page_label_decisions[page_position][line_position]
                    && !should_remove_line_by_rules(
                        &prepared,
                        line.is_page_top,
                        line.is_page_bottom,
                        config,
                    )
                {
                    filtered_lines.push(line.clone());
                }
            }
            filtered_pages.push(crate::structured_document::DocumentPage {
                page_index: page.page_index,
                lines: filtered_lines,
            });
        }
        let filtered_document = StructuredDocument {
            pages: filtered_pages,
            has_reliable_page_metadata: document.has_reliable_page_metadata,
        };

        let page_cleaning_config = without_page_label_removal(config);
        let mut page_texts = Vec::with_capacity(filtered_document.pages.len());
        for page in &filtered_document.pages {
            let page_text = page.to_text();
            let cleaned = if let Some(collector) = collector.as_deref_mut() {
                let page_context = CleaningContext {
                    source_id: context.and_then(|value| value.source_id),
                    source_label: context.and_then(|value| value.source_label),
                    page_number: Some(page.page_index + 1),
                };
                clean_text_impl(
                    &page_text,
                    &page_cleaning_config,
                    Some(&page_context),
                    Some(collector),
                )
            } else {
                clean_text(&page_text, &page_cleaning_config)
            };
            page_texts.push(cleaned);
        }

        // The authoritative flat text is derived from the cleaned page representation.
        let text = page_texts.join("\n\n");
        CleanedStructuredDocument {
            text,
            page_texts: Some(page_texts),
        }
    }
}

fn normalize_irregular_line_breaks(text: &str) -> String {
    let norm_text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraphs = Vec::new();

    for paragraph in norm_text.split("\n\n") {
        let lines = paragraph
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();

        if !lines.is_empty() {
            paragraphs.push(reflow_paragraph_lines(&lines));
        }
    }

    paragraphs.join("\n\n")
}

fn reflow_paragraph_lines(lines: &[&str]) -> String {
    let mut reflowed = String::new();

    for line in lines {
        if reflowed.is_empty() {
            reflowed.push_str(line);
        } else if should_dehyphenate_line_break(&reflowed, line) {
            reflowed.pop();
            reflowed.push_str(line);
        } else if reflowed.ends_with('-') {
            reflowed.push_str(line);
        } else {
            reflowed.push(' ');
            reflowed.push_str(line);
        }
    }

    RE_EXCESSIVE_HORIZONTAL_WHITESPACE
        .replace_all(&reflowed, " ")
        .to_string()
}

fn should_dehyphenate_line_break(prefix: &str, suffix: &str) -> bool {
    let Some(prefix_without_hyphen) = prefix.trim_end().strip_suffix('-') else {
        return false;
    };

    let Some(prefix_fragment) = prefix_without_hyphen.split_whitespace().last() else {
        return false;
    };
    let Some(suffix_fragment) = suffix.split_whitespace().next() else {
        return false;
    };

    let prefix_len = prefix_fragment.chars().count();
    let suffix_len = suffix_fragment.chars().count();
    if prefix_len < 5 || suffix_len < 4 {
        return false;
    }

    if !prefix_fragment.chars().all(char::is_alphabetic)
        || !suffix_fragment.chars().all(char::is_alphabetic)
    {
        return false;
    }

    let Some(first_suffix_char) = suffix_fragment.chars().next() else {
        return false;
    };
    if !first_suffix_char.is_lowercase() {
        return false;
    }

    // Short prefixes such as "well-" are often compounds rather than line-wrap
    // splits, and all-caps prefixes are often identifiers or acronyms.
    prefix_fragment.chars().any(char::is_lowercase)
}

fn normalize_horizontal_whitespace(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            RE_EXCESSIVE_HORIZONTAL_WHITESPACE
                .replace_all(line, " ")
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collapse_blank_lines(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut newline_count = 0usize;

    for character in text.chars() {
        if character == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                collapsed.push(character);
            }
        } else {
            newline_count = 0;
            collapsed.push(character);
        }
    }

    collapsed
}

fn apply_line_removal(
    text: &str,
    collector: &mut Option<&mut CleaningReportCollector>,
    context: Option<&CleaningContext>,
    operation: CleaningOperation,
    should_remove: impl Fn(&str) -> Option<Vec<String>>,
) -> String {
    let mut cleaned = String::with_capacity(text.len());
    let mut line_number = 1;

    for line in text.split_inclusive('\n') {
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        let content = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);

        if let Some(reasons) = should_remove(content) {
            if let Some(coll) = collector.as_deref_mut() {
                coll.record_removal(
                    operation,
                    None,
                    reasons,
                    context,
                    RemovedTextExample {
                        removed_text: content.trim().to_string(),
                        line_number: Some(line_number),
                        page_number: context.and_then(|value| value.page_number),
                    },
                );
            }
            line_number += 1;
            continue;
        }

        cleaned.push_str(line);
        line_number += 1;
    }

    cleaned
}

pub(crate) fn is_isolated_page_marker_line(line: &str) -> bool {
    page_marker_candidate_for_line(line).is_some()
}

fn is_url_only_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && RE_URL_ONLY_LINE.is_match(trimmed)
}

fn is_email_only_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && RE_EMAIL_ONLY_LINE.is_match(trimmed)
}

fn is_doi_only_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && RE_DOI_ONLY_LINE.is_match(trimmed)
}

fn is_figure_table_caption_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && RE_FIGURE_TABLE_CAPTION_LINE.is_match(trimmed)
}

fn should_remove_obvious_extraction_noise_line(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let profile = profile_text_line(trimmed, "");
    let label = classify_text_signal(&profile, TextSignalContext::default());
    let reasons = text_signal_reasons(trimmed, &profile, label, 0.0);

    crate::text_profile::obvious_extraction_noise_decisive_reasons(&profile, &reasons)
}

fn effective_remove_patterns(config: &CleaningConfig) -> Vec<String> {
    config
        .remove_patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            if config.lowercase {
                pattern.to_lowercase()
            } else {
                pattern.clone()
            }
        })
        .collect()
}

fn effective_whole_line_literals(config: &CleaningConfig) -> Vec<String> {
    config
        .removal_rules
        .iter()
        .filter(|rule| rule.enabled && rule.scope == RemovalScope::WholeLine)
        .filter_map(|rule| match &rule.matcher {
            RemovalMatcher::Literal { text } => {
                let effective_text = prepare_rule_text_for_matching(text, config);
                let trimmed = effective_text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            RemovalMatcher::NormalizedLine { .. } => None,
        })
        .collect()
}

fn effective_whole_line_normalized_keys(config: &CleaningConfig) -> Vec<String> {
    config
        .removal_rules
        .iter()
        .filter(|rule| rule.enabled && rule.scope == RemovalScope::WholeLine)
        .filter_map(|rule| match &rule.matcher {
            RemovalMatcher::NormalizedLine { normalized_key } => {
                let effective_key =
                    prepare_normalized_key_for_rule_matching(normalized_key, config);
                let trimmed = effective_key.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            RemovalMatcher::Literal { .. } => None,
        })
        .collect()
}

struct EffectiveAnywhereLiteral<'a> {
    rule: &'a RemovalRule,
    text: String,
}

fn effective_anywhere_literals(config: &CleaningConfig) -> Vec<EffectiveAnywhereLiteral<'_>> {
    config
        .removal_rules
        .iter()
        .filter(|rule| rule.enabled && rule.scope == RemovalScope::Anywhere)
        .filter_map(|rule| match &rule.matcher {
            RemovalMatcher::Literal { text } => {
                let effective_text = prepare_rule_text_for_matching(text, config);
                (!effective_text.trim().is_empty() && !effective_text.contains(['\r', '\n']))
                    .then_some(EffectiveAnywhereLiteral {
                        rule,
                        text: effective_text,
                    })
            }
            RemovalMatcher::NormalizedLine { .. } => None,
        })
        .collect()
}

fn remove_structured_removal_rules(
    text: String,
    config: &CleaningConfig,
    collector: &mut Option<&mut CleaningReportCollector>,
    context: Option<&CleaningContext>,
) -> String {
    let whole_line_literals = effective_whole_line_literals(config);
    let whole_line_normalized_keys = effective_whole_line_normalized_keys(config);
    let anywhere_literals = effective_anywhere_literals(config);
    if let Some(collector) = collector.as_deref_mut() {
        for rule in config.removal_rules.iter().filter(|rule| rule.enabled) {
            if let Some(notice) = invalid_anywhere_rule_reason(rule) {
                collector.record_skipped_rule(
                    Some(rule.id.clone()),
                    Some(rule.source),
                    context,
                    notice.to_string(),
                );
            }
        }
    }
    if whole_line_literals.is_empty()
        && whole_line_normalized_keys.is_empty()
        && anywhere_literals.is_empty()
    {
        return text;
    }

    let mut cleaned = text;
    if !whole_line_literals.is_empty() || !whole_line_normalized_keys.is_empty() {
        cleaned = remove_whole_lines_matching_structured(
            &cleaned,
            &whole_line_literals,
            &whole_line_normalized_keys,
        );
    }
    if !anywhere_literals.is_empty() {
        if let Some(collector) = collector.as_deref_mut() {
            cleaned = replace_anywhere_literals_with_report(
                cleaned,
                &anywhere_literals,
                collector,
                context,
            );
        } else {
            let patterns = anywhere_literals
                .iter()
                .map(|literal| literal.text.clone())
                .collect::<Vec<_>>();
            cleaned = replace_anywhere_literals(cleaned, &patterns);
        }
    }
    cleaned
}

fn replace_anywhere_literals_with_report(
    mut text: String,
    literals: &[EffectiveAnywhereLiteral<'_>],
    collector: &mut CleaningReportCollector,
    context: Option<&CleaningContext>,
) -> String {
    for literal in literals {
        let mut cleaned = String::with_capacity(text.len());
        let mut last_end = 0;
        let mut line_number = 1;
        let mut occurrence_count = 0;
        let mut examples = Vec::new();

        for (start, matched) in text.match_indices(&literal.text) {
            line_number += text[last_end..start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            cleaned.push_str(&text[last_end..start]);
            cleaned.push(' ');
            last_end = start + matched.len();
            occurrence_count += 1;
            if examples.len() < 20 {
                examples.push(RemovedTextExample {
                    removed_text: matched.to_string(),
                    line_number: Some(line_number),
                    page_number: context.and_then(|value| value.page_number),
                });
            }
        }

        if occurrence_count == 0 {
            continue;
        }
        cleaned.push_str(&text[last_end..]);
        collector.record_rule_removals(
            literal.rule,
            vec!["inline_literal".to_string()],
            context,
            occurrence_count,
            examples,
        );
        text = cleaned;
    }

    text
}

fn remove_whole_lines_matching_structured(
    text: &str,
    literals: &[String],
    normalized_keys: &[String],
) -> String {
    let mut cleaned = String::with_capacity(text.len());

    for line in text.split_inclusive('\n') {
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        let content = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);
        if literals.iter().any(|literal| content.trim() == literal) {
            continue;
        }
        if !normalized_keys.is_empty() {
            let normalized = normalize_line_for_repeated_artifact(content.trim());
            if normalized_keys.iter().any(|key| normalized.as_str() == key) {
                continue;
            }
        }
        cleaned.push_str(line);
    }

    cleaned
}

fn remove_literal_patterns(text: String, config: &CleaningConfig) -> String {
    let patterns = effective_remove_patterns(config);
    if patterns.is_empty() {
        return text;
    }

    if patterns.len() > 1
        && can_batch_literal_removals(&patterns)
        && let Some(cleaned) = remove_literal_patterns_batched(&text, &patterns)
    {
        return cleaned;
    }

    remove_literal_patterns_sequential(text, &patterns)
}

fn remove_literal_patterns_sequential(text: String, patterns: &[String]) -> String {
    replace_literal_patterns_sequential(text, patterns, "")
}

fn replace_anywhere_literals(text: String, patterns: &[String]) -> String {
    if patterns.len() > 1
        && can_batch_literal_removals(patterns)
        && let Some(cleaned) = replace_literal_patterns_batched(&text, patterns, " ")
    {
        return cleaned;
    }

    replace_literal_patterns_sequential(text, patterns, " ")
}

fn replace_literal_patterns_sequential(
    mut text: String,
    patterns: &[String],
    replacement: &str,
) -> String {
    for pattern in patterns {
        text = text.replace(pattern, replacement);
    }
    text
}

fn remove_literal_patterns_batched(text: &str, patterns: &[String]) -> Option<String> {
    replace_literal_patterns_batched(text, patterns, "")
}

fn replace_literal_patterns_batched(
    text: &str,
    patterns: &[String],
    replacement: &str,
) -> Option<String> {
    let automaton = literal_remove_automaton(patterns);
    let mut cleaned = String::with_capacity(text.len());
    let mut last_end = 0;
    let mut found_match = false;

    for mat in automaton.find_iter(text) {
        found_match = true;
        cleaned.push_str(&text[last_end..mat.start()]);
        cleaned.push_str(replacement);
        last_end = mat.end();
    }

    if !found_match {
        return Some(text.to_string());
    }

    cleaned.push_str(&text[last_end..]);

    // Rewriting can expose a later pattern. The sequential path is the
    // reference behaviour for those cascading cases.
    if automaton.is_match(&cleaned) {
        None
    } else {
        Some(cleaned)
    }
}

fn literal_remove_automaton(patterns: &[String]) -> AhoCorasick {
    AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostFirst)
        .build(patterns)
        .expect("effective removal patterns are non-empty")
}

fn can_batch_literal_removals(patterns: &[String]) -> bool {
    for i in 0..patterns.len() {
        for j in (i + 1)..patterns.len() {
            let left = patterns[i].as_str();
            let right = patterns[j].as_str();
            if left == right
                || left.contains(right)
                || right.contains(left)
                || literals_can_overlap(left, right)
            {
                return false;
            }
        }
    }

    true
}

fn literals_can_overlap(left: &str, right: &str) -> bool {
    let max_overlap_len = left.len().min(right.len());
    (1..max_overlap_len).any(|len| {
        let left_suffix_matches = left.is_char_boundary(left.len() - len)
            && right.is_char_boundary(len)
            && left[left.len() - len..] == right[..len];
        let right_suffix_matches = right.is_char_boundary(right.len() - len)
            && left.is_char_boundary(len)
            && right[right.len() - len..] == left[..len];

        left_suffix_matches || right_suffix_matches
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structured_document::StructuredDocument;
    use serde_json::json;

    fn whole_line_rule(text: &str) -> RemovalRule {
        RemovalRule {
            id: "rule-1".to_string(),
            label: text.to_string(),
            source: RemovalRuleSource::PromotedRepeatedArtifact,
            matcher: RemovalMatcher::Literal {
                text: text.to_string(),
            },
            scope: RemovalScope::WholeLine,
            enabled: true,
        }
    }

    fn normalized_line_rule(normalized_key: &str) -> RemovalRule {
        RemovalRule {
            id: "rule-1".to_string(),
            label: format!("Normalised whole line: {normalized_key}"),
            source: RemovalRuleSource::PromotedRepeatedArtifact,
            matcher: RemovalMatcher::NormalizedLine {
                normalized_key: normalized_key.to_string(),
            },
            scope: RemovalScope::WholeLine,
            enabled: true,
        }
    }

    fn anywhere_literal_rule(text: &str) -> RemovalRule {
        RemovalRule {
            id: "rule-1".to_string(),
            label: format!("Anywhere: {text}"),
            source: RemovalRuleSource::PromotedRepeatedArtifact,
            matcher: RemovalMatcher::Literal {
                text: text.to_string(),
            },
            scope: RemovalScope::Anywhere,
            enabled: true,
        }
    }

    fn normalized_anywhere_rule(normalized_key: &str) -> RemovalRule {
        RemovalRule {
            id: "normalised-anywhere".to_string(),
            label: normalized_key.to_string(),
            source: RemovalRuleSource::PromotedRepeatedArtifact,
            matcher: RemovalMatcher::NormalizedLine {
                normalized_key: normalized_key.to_string(),
            },
            scope: RemovalScope::Anywhere,
            enabled: true,
        }
    }

    fn page_texts(result: CleanedStructuredDocument) -> Vec<String> {
        result
            .page_texts
            .expect("page-aware cleaning should preserve page text")
    }

    #[test]
    fn clean_structured_document_preserves_page_count_for_page_local_config() {
        let document = StructuredDocument::from_pages(["Title\nBody", "Title\nMore body"]);
        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "title\nbody\n\ntitle\nmore body");
        let pages = page_texts(cleaned);
        assert_eq!(pages, vec!["title\nbody", "title\nmore body"]);
    }

    #[test]
    fn clean_structured_document_flat_text_matches_clean_text_for_representative_config() {
        let document =
            StructuredDocument::from_pages(["HEADER\n Body old drop ", "More old\nHEADER"]);
        let config = CleaningConfig {
            lowercase: true,
            trim_lines: true,
            remove_patterns: vec!["drop".to_string()],
            removal_rules: vec![whole_line_rule("HEADER")],
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "new".to_string(),
            }],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, clean_text(&document.to_flat_text(), &config));
        assert_eq!(
            cleaned.page_texts,
            Some(vec!["body new".to_string(), "more new\n".to_string()])
        );
    }

    #[test]
    fn clean_structured_document_flat_text_matches_joined_cleaned_pages() {
        let document = StructuredDocument::from_pages(["Alpha\nBeta", "Gamma"]);
        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);
        let pages = page_texts(cleaned.clone());

        assert_eq!(cleaned.text, pages.join("\n\n"));
    }

    #[test]
    fn clean_structured_document_literal_whole_line_rules_apply_within_pages() {
        let document = StructuredDocument::from_pages(["Header\nBody", "Header\nMore body"]);
        let config = CleaningConfig {
            removal_rules: vec![whole_line_rule("Header")],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "Body\n\nMore body");
        assert_eq!(page_texts(cleaned), vec!["Body", "More body"]);
    }

    #[test]
    fn clean_structured_document_normalized_whole_line_rules_apply_within_pages() {
        let document = StructuredDocument::from_pages(["Page 1\nBody", "Page 42\nMore body"]);
        let config = CleaningConfig {
            removal_rules: vec![normalized_line_rule("page #")],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "Body\n\nMore body");
        assert_eq!(page_texts(cleaned), vec!["Body", "More body"]);
    }

    #[test]
    fn clean_structured_document_preserves_mid_sentence_occurrences() {
        let document = StructuredDocument::from_pages([
            "Header\nThis sentence mentions Header in the body.",
            "Another Header sentence.",
        ]);
        let config = CleaningConfig {
            removal_rules: vec![whole_line_rule("Header")],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(
            page_texts(cleaned),
            vec![
                "This sentence mentions Header in the body.",
                "Another Header sentence."
            ]
        );
    }

    #[test]
    fn clean_structured_document_legacy_remove_patterns_apply_within_pages() {
        let document = StructuredDocument::from_pages(["Keep DROP keep", "DROP body"]);
        let config = CleaningConfig {
            remove_patterns: vec!["DROP".to_string()],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(page_texts(cleaned), vec!["Keep  keep", " body"]);
    }

    #[test]
    fn clean_structured_document_replacement_rules_apply_within_pages() {
        let document = StructuredDocument::from_pages(["old value", "second old value"]);
        let config = CleaningConfig {
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "new".to_string(),
            }],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(page_texts(cleaned), vec!["new value", "second new value"]);
    }

    #[test]
    fn clean_structured_document_disabled_rules_do_nothing() {
        let mut rule = whole_line_rule("Header");
        rule.enabled = false;
        let document = StructuredDocument::from_pages(["Header\nBody", "Header\nMore body"]);
        let config = CleaningConfig {
            removal_rules: vec![rule],
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(
            page_texts(cleaned),
            vec!["Header\nBody", "Header\nMore body"]
        );
    }

    #[test]
    fn clean_structured_document_preserves_page_texts_when_reflow_keeps_boundaries() {
        let document = StructuredDocument::from_pages(["Alpha", "Beta"]);
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, clean_text(&document.to_flat_text(), &config));
        assert_eq!(cleaned.text, "Alpha\n\nBeta");
        assert_eq!(
            cleaned.page_texts,
            Some(vec!["Alpha".to_string(), "Beta".to_string()])
        );
    }

    #[test]
    fn lowercase_text() {
        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("HELLO Mixed", &config), "hello mixed");
    }

    #[test]
    fn normalizes_line_endings() {
        let config = CleaningConfig {
            normalize_line_endings: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("a\r\nb\rc", &config), "a\nb\nc");
    }

    #[test]
    fn trims_lines() {
        let config = CleaningConfig {
            trim_lines: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("  a  \n\tb\t", &config), "a\nb");
    }

    #[test]
    fn collapses_blank_lines() {
        let config = CleaningConfig {
            collapse_blank_lines: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("a\n\n\n\nb", &config), "a\n\nb");
    }

    #[test]
    fn normalizes_grouped_whitespace_and_line_endings() {
        let config = CleaningConfig {
            normalize_whitespace_line_endings: true,
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("  a  \r\n\r\n\r\n b\t\tc  ", &config),
            "a\n\nb c"
        );
    }

    fn obvious_noise_config() -> CleaningConfig {
        CleaningConfig {
            remove_obvious_extraction_noise: true,
            ..CleaningConfig::default()
        }
    }

    #[test]
    fn obvious_extraction_noise_default_config_does_not_remove_noise_lines() {
        let config = CleaningConfig::default();
        assert_eq!(
            clean_text("Alpha\n------\nBeta", &config),
            "Alpha\n------\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_removes_repeated_symbol_lines() {
        let config = obvious_noise_config();

        assert_eq!(
            clean_text("Alpha\n------\n________\n••••••••\nBeta", &config),
            "Alpha\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_removes_cid_marker_lines() {
        let config = obvious_noise_config();

        assert_eq!(
            clean_text("Alpha\ncid:14\ncid:105\nBeta", &config),
            "Alpha\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_removes_markup_and_entity_lines() {
        let config = obvious_noise_config();

        assert_eq!(
            clean_text("Alpha\n<br/>\n<br>\n&nbsp;\n&amp;\nBeta", &config),
            "Alpha\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_removes_replacement_character_junk_lines() {
        let config = obvious_noise_config();

        assert_eq!(clean_text("Alpha\n����\nBeta", &config), "Alpha\nBeta");
    }

    #[test]
    fn obvious_extraction_noise_keeps_section_headings() {
        let config = obvious_noise_config();

        let text = "Introduction\nMethods\nResults\nDiscussion\nConclusion";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_portuguese_and_french_headings() {
        let config = obvious_noise_config();

        let text = "Resumo\nIntrodução\nRéférences";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_page_labels() {
        let config = obvious_noise_config();

        let text = "Page 12\nPágina 3 de 10";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_formula_and_code_like_lines() {
        let config = obvious_noise_config();

        let text = "p < .05\nχ² = 5.32\ny = mx + b\nif (x > 0) { return x; }";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_table_and_statistical_rows() {
        let config = obvious_noise_config();

        let text = "Table 3\nMean SD N\n12.4 15.8 99.2";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_natural_prose() {
        let config = obvious_noise_config();

        let text = "The participants answered the questionnaire.";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_keeps_mixed_text_with_one_number() {
        let config = obvious_noise_config();

        let text = "Chapter 3";
        assert_eq!(clean_text(text, &config), text);
    }

    #[test]
    fn obvious_extraction_noise_is_stable_with_join_line_breaks() {
        let config = CleaningConfig {
            remove_obvious_extraction_noise: true,
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Alpha\n------\nBeta", &config), "Alpha Beta");
    }

    #[test]
    fn obvious_extraction_noise_is_stable_with_trim_lines() {
        let config = CleaningConfig {
            remove_obvious_extraction_noise: true,
            trim_lines: true,
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("  Alpha  \n   ________   \n  Beta  ", &config),
            "Alpha\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_is_stable_with_collapse_blank_lines() {
        let config = CleaningConfig {
            remove_obvious_extraction_noise: true,
            collapse_blank_lines: true,
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("Alpha\n------\n\n\nBeta", &config),
            "Alpha\n\nBeta"
        );
    }

    #[test]
    fn obvious_extraction_noise_structured_document_stays_page_consistent() {
        let document =
            StructuredDocument::from_pages(["Alpha\n------\nBeta", "Gamma\ncid:14\nDelta"]);
        let config = obvious_noise_config();

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "Alpha\nBeta\n\nGamma\nDelta");
        assert_eq!(
            cleaned.page_texts,
            Some(vec!["Alpha\nBeta".to_string(), "Gamma\nDelta".to_string()])
        );
    }

    #[test]
    fn removes_literal_patterns() {
        let config = CleaningConfig {
            remove_patterns: vec!["remove".to_string(), "drop".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("keep remove keep drop keep", &config),
            "keep  keep  keep"
        );
    }

    #[test]
    fn remove_patterns_ignore_empty_patterns() {
        let config = CleaningConfig {
            remove_patterns: vec!["".to_string(), "drop".to_string(), "".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("keep drop keep", &config), "keep  keep");
    }

    #[test]
    fn remove_patterns_are_literal_not_regex() {
        let config = CleaningConfig {
            remove_patterns: vec!["a.c".to_string(), "[x]".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("a.c abc [x] x", &config), " abc  x");
    }

    #[test]
    fn remove_patterns_follow_lowercase_option() {
        let config = CleaningConfig {
            lowercase: true,
            remove_patterns: vec!["HEADER".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("Header body HEADER", &config), " body ");
    }

    #[test]
    fn removal_rule_serializes_and_deserializes() {
        let rule = whole_line_rule("Journal of Corpus Linguistics");

        let value = serde_json::to_value(&rule).unwrap();
        assert_eq!(value["source"], json!("promoted_repeated_artifact"));
        assert_eq!(value["scope"], json!("whole_line"));
        assert_eq!(
            value["matcher"],
            json!({
                "kind": "literal",
                "text": "Journal of Corpus Linguistics"
            })
        );

        let decoded: RemovalRule = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, rule);
    }

    #[test]
    fn normalized_line_matcher_serializes_and_deserializes() {
        let matcher = RemovalMatcher::NormalizedLine {
            normalized_key: "page #".to_string(),
        };

        let value = serde_json::to_value(&matcher).unwrap();
        assert_eq!(
            value,
            json!({
                "kind": "normalized_line",
                "normalized_key": "page #"
            })
        );

        let decoded: RemovalMatcher = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, matcher);
    }

    #[test]
    fn cleaning_config_deserializes_missing_removal_rules() {
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("removal_rules");

        let config: CleaningConfig = serde_json::from_value(value).unwrap();
        assert!(config.removal_rules.is_empty());
    }

    #[test]
    fn cleaning_config_deserializes_missing_obvious_noise_option() {
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("remove_obvious_extraction_noise");

        let config: CleaningConfig = serde_json::from_value(value).unwrap();
        assert!(!config.remove_obvious_extraction_noise);
    }

    #[test]
    fn cleaning_config_deserializes_missing_consolidated_options() {
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("normalize_whitespace_line_endings");
        object.remove("repair_wrapped_pdf_line_breaks");
        object.remove("remove_isolated_page_markers");
        object.remove("remove_url_only_lines");
        object.remove("remove_email_only_lines");
        object.remove("remove_doi_only_lines");
        object.remove("remove_figure_table_captions");

        let config: CleaningConfig = serde_json::from_value(value).unwrap();
        assert!(!config.normalize_whitespace_line_endings);
        assert!(!config.repair_wrapped_pdf_line_breaks);
        assert!(!config.remove_isolated_page_markers);
        assert!(!config.remove_url_only_lines);
        assert!(!config.remove_email_only_lines);
        assert!(!config.remove_doi_only_lines);
        assert!(!config.remove_figure_table_captions);
    }

    #[test]
    fn cleaning_config_deserializes_normalized_line_rule() {
        let rule = normalized_line_rule("page #");
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("removal_rules".to_string(), json!([rule]));

        let config: CleaningConfig = serde_json::from_value(value).unwrap();

        assert_eq!(config.removal_rules, vec![normalized_line_rule("page #")]);
    }

    #[test]
    fn whole_line_literal_rule_removes_exact_line() {
        let config = CleaningConfig {
            removal_rules: vec![whole_line_rule("Journal of Corpus Linguistics")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text(
                "This is body text.\nJournal of Corpus Linguistics\nThis is more body text.",
                &config
            ),
            "This is body text.\nThis is more body text."
        );
    }

    #[test]
    fn whole_line_literal_rule_does_not_remove_mid_sentence_text() {
        let config = CleaningConfig {
            removal_rules: vec![whole_line_rule("Journal of Corpus Linguistics")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text(
                "This article was published in Journal of Corpus Linguistics in 2020.",
                &config
            ),
            "This article was published in Journal of Corpus Linguistics in 2020."
        );
    }

    #[test]
    fn whole_line_literal_rule_matches_trimmed_line_text() {
        let config = CleaningConfig {
            removal_rules: vec![whole_line_rule("Journal of Corpus Linguistics")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text(
                "Body\n   Journal of Corpus Linguistics\t\nMore body",
                &config
            ),
            "Body\nMore body"
        );
    }

    #[test]
    fn disabled_whole_line_literal_rule_does_nothing() {
        let mut rule = whole_line_rule("Journal of Corpus Linguistics");
        rule.enabled = false;
        let config = CleaningConfig {
            removal_rules: vec![rule],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("Body\nJournal of Corpus Linguistics\nMore body", &config),
            "Body\nJournal of Corpus Linguistics\nMore body"
        );
    }

    #[test]
    fn legacy_remove_patterns_and_structured_rules_can_coexist() {
        let config = CleaningConfig {
            remove_patterns: vec!["<br/>".to_string()],
            removal_rules: vec![whole_line_rule("Journal of Corpus Linguistics")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text(
                "Body<br/>\nJournal of Corpus Linguistics\nMore<br/> body",
                &config
            ),
            "Body\nMore body"
        );
    }

    #[test]
    fn anywhere_literal_rule_removes_inline_repeated_artifact() {
        let config = CleaningConfig {
            normalize_whitespace_line_endings: true,
            removal_rules: vec![anywhere_literal_rule("<br />")],
            ..CleaningConfig::default()
        };

        let cleaned = clean_text("alpha<br /><br />beta", &config);

        assert_eq!(cleaned, "alpha beta");
        assert!(!cleaned.contains("<br />"));
    }

    #[test]
    fn anywhere_literal_rule_is_literal_not_regex() {
        let config = CleaningConfig {
            removal_rules: vec![anywhere_literal_rule("a.c[1]")],
            ..CleaningConfig::default()
        };

        let cleaned = clean_text("keep a.c[1] keep abc1 keep", &config);

        assert_eq!(cleaned, "keep   keep abc1 keep");
    }

    #[test]
    fn disabled_anywhere_literal_rule_does_nothing() {
        let mut rule = anywhere_literal_rule("<br />");
        rule.enabled = false;
        let config = CleaningConfig {
            removal_rules: vec![rule],
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("alpha<br />beta", &config), "alpha<br />beta");
    }

    #[test]
    fn removing_anywhere_literal_rule_restores_inline_text() {
        let with_rule = CleaningConfig {
            removal_rules: vec![anywhere_literal_rule("<br />")],
            ..CleaningConfig::default()
        };
        let without_rule = CleaningConfig::default();

        assert!(!clean_text("alpha<br />beta", &with_rule).contains("<br />"));
        assert_eq!(
            clean_text("alpha<br />beta", &without_rule),
            "alpha<br />beta"
        );
    }

    #[test]
    fn whole_line_normalized_rule_removes_matching_line_family() {
        let config = CleaningConfig {
            removal_rules: vec![normalized_line_rule("page #")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text(
                "Page 1\nThis is body text.\nPage 2\nThis mentions page 3 inside a sentence.\n   Page 44   \nMore body.",
                &config
            ),
            "This is body text.\nThis mentions page 3 inside a sentence.\nMore body."
        );
    }

    #[test]
    fn disabled_whole_line_normalized_rule_does_nothing() {
        let mut rule = normalized_line_rule("page #");
        rule.enabled = false;
        let config = CleaningConfig {
            removal_rules: vec![rule],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("Page 1\nBody\nPage 2", &config),
            "Page 1\nBody\nPage 2"
        );
    }

    #[test]
    fn normalized_line_rules_and_legacy_remove_patterns_can_coexist() {
        let config = CleaningConfig {
            remove_patterns: vec!["<br/>".to_string()],
            removal_rules: vec![normalized_line_rule("page #")],
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("Body<br/>\nPage 12\nMore<br/> body", &config),
            "Body\nMore body"
        );
    }

    #[test]
    fn whole_line_literal_rules_follow_lowercase_option() {
        let config = CleaningConfig {
            lowercase: true,
            removal_rules: vec![whole_line_rule("HEADER")],
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Header\nBody", &config), "body");
    }

    #[test]
    fn whole_line_literal_rules_match_without_diacritics_when_disabled() {
        let config = CleaningConfig {
            replace_diacritics: false,
            removal_rules: vec![whole_line_rule("Página 1")],
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Página 1\nBody", &config), "Body");
        assert_eq!(clean_text("Pagina 1\nBody", &config), "Pagina 1\nBody");
    }

    #[test]
    fn whole_line_literal_rules_match_with_diacritics_when_enabled() {
        let config = CleaningConfig {
            replace_diacritics: true,
            removal_rules: vec![whole_line_rule("Página 1")],
            ..CleaningConfig::default()
        };

        // The input has an accent, the rule has an accent.
        assert_eq!(clean_text("Página 1\nBody", &config), "Body");
        // The input has no accent, the rule has an accent. Both become "Pagina 1".
        assert_eq!(clean_text("Pagina 1\nBody", &config), "Body");
    }

    #[test]
    fn normalized_line_rules_match_with_diacritics_when_enabled() {
        let config = CleaningConfig {
            replace_diacritics: true,
            removal_rules: vec![normalized_line_rule("página #")],
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Página 1\nBody\nPágina 2", &config), "Body\n");
        assert_eq!(clean_text("Pagina 1\nBody\nPágina 2", &config), "Body\n");
    }

    #[test]
    fn clean_structured_document_applies_diacritic_rules_to_page_zones() {
        let config = CleaningConfig {
            replace_diacritics: true,
            removal_rules: vec![page_zone_rule("Página #", RemovalScope::PageBottom)],
            ..CleaningConfig::default()
        };

        let doc = StructuredDocument::from_pages(["L1\nL2\nL3\nMid\nL5\nL6\nPágina #"]);
        let cleaned = clean_structured_document(&doc, &config);
        assert_eq!(cleaned.text, "L1\nL2\nL3\nMid\nL5\nL6");
    }

    #[test]
    fn clean_structured_document_ignores_page_zones_when_metadata_is_unreliable() {
        let config = CleaningConfig {
            removal_rules: vec![page_zone_rule("Header", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };

        let doc = StructuredDocument::from_flat_text_as_single_page("Header\nBody\nFooter");
        // It should NOT remove the header because it's a synthetic page.
        let cleaned = clean_structured_document(&doc, &config);
        assert_eq!(cleaned.text, "Header\nBody\nFooter");

        let reliable_doc = StructuredDocument::from_pages(["Header\nBody\nFooter"]);
        let cleaned_reliable = clean_structured_document(&reliable_doc, &config);
        assert_eq!(cleaned_reliable.text, "Body\nFooter");
    }

    #[test]
    fn clean_structured_document_keeps_page_labels_when_metadata_is_unreliable() {
        let config = all_page_label_config();
        let doc = StructuredDocument::from_flat_text_as_single_page("1\nBody\nPage 2");

        let cleaned = clean_structured_document(&doc, &config);

        assert_eq!(cleaned.text, "1\nBody\nPage 2");
    }

    #[test]
    fn remove_patterns_run_after_diacritic_replacement_and_lowercase() {
        let config = CleaningConfig {
            replace_diacritics: true,
            lowercase: true,
            remove_patterns: vec!["CAFE".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("Café remains", &config), " remains");
    }

    #[test]
    fn remove_patterns_preserve_sequential_subset_semantics() {
        let config = CleaningConfig {
            remove_patterns: vec!["aba".to_string(), "ba".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("ababa", &config), "");
    }

    #[test]
    fn remove_patterns_preserve_sequential_cascading_semantics() {
        let config = CleaningConfig {
            remove_patterns: vec!["X".to_string(), "ab".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("aXb", &config), "");
    }

    #[test]
    fn replaces_literal_patterns() {
        let config = CleaningConfig {
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "new".to_string(),
            }],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("old value", &config), "new value");
    }

    #[test]
    fn normalizes_unicode_to_nfc() {
        let config = CleaningConfig {
            normalize_unicode: true,
            replace_diacritics: false,
            ..CleaningConfig::default()
        };
        let nfd_string = "e\u{0301}";
        let nfc_string = "é";
        assert_eq!(clean_text(nfd_string, &config), nfc_string);
    }

    #[test]
    fn replaces_diacritics() {
        let config = CleaningConfig {
            replace_diacritics: true,
            normalize_unicode: false,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("áéíóúçü", &config), "aeioucu");
        assert_eq!(clean_text("ÁÉÍÓÚÇÜ", &config), "AEIOUCU");
    }

    fn all_page_label_config() -> CleaningConfig {
        CleaningConfig {
            remove_standalone_page_numbers: true,
            remove_standalone_roman_page_numbers: true,
            remove_page_indicators: true,
            remove_page_delimiters: true,
            remove_isolated_page_markers: true,
            ..CleaningConfig::default()
        }
    }

    #[test]
    fn clean_text_keeps_page_marker_candidates_without_metadata() {
        let config = all_page_label_config();
        let input = [
            "12",
            "- 12 -",
            "[12]",
            "12.",
            "p. 12",
            "Page 12",
            "i",
            "ii",
            "iv",
            "--- Page 12 ---",
        ]
        .join("\n");

        assert_eq!(clean_text(&input, &config), input);
        assert_eq!(
            clean_text_with_report(&input, &config, None).1.summaries,
            vec![]
        );
    }

    #[test]
    fn removes_arabic_page_labels_at_page_edges() {
        let config = all_page_label_config();
        let document = StructuredDocument::from_pages([
            "- 12 -\nBody A",
            "[12]\nBody B",
            "12.\nBody C",
            "p. 12\nBody D",
            "Page 12\nBody E",
            "pag. 12\nBody F",
            "| 12\nBody G",
            "12 |\nBody H",
            "--- Page 12 ---\nBody I",
        ]);

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(
            cleaned.text,
            "Body A\n\nBody B\n\nBody C\n\nBody D\n\nBody E\n\nBody F\n\nBody G\n\nBody H\n\nBody I"
        );
    }

    #[test]
    fn removes_bare_arabic_page_numbers_with_sequence_evidence() {
        let config = CleaningConfig {
            remove_standalone_page_numbers: true,
            ..CleaningConfig::default()
        };
        let document =
            StructuredDocument::from_pages(["1\nAlpha", "2\nBeta", "3\nGamma", "4\nDelta"]);

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "Alpha\n\nBeta\n\nGamma\n\nDelta");
    }

    #[test]
    fn removes_bare_arabic_page_number_with_page_ordinal_evidence() {
        let config = CleaningConfig {
            remove_standalone_page_numbers: true,
            ..CleaningConfig::default()
        };
        let mut pages = (1..=12)
            .map(|index| format!("Body {index}"))
            .collect::<Vec<_>>();
        pages[11] = "12\nBody 12".to_string();
        let document = StructuredDocument::from_pages(pages);

        let cleaned = clean_structured_document(&document, &config);

        assert!(cleaned.text.contains("Body 12"));
        assert!(!cleaned.text.contains("\n12\n"));
    }

    #[test]
    fn removes_roman_page_labels_at_page_edges() {
        let config = CleaningConfig {
            remove_standalone_roman_page_numbers: true,
            ..CleaningConfig::default()
        };
        let document =
            StructuredDocument::from_pages(["i\nPreface", "ii\nMore", "iii\nMore", "iv\nBody"]);

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, "Preface\n\nMore\n\nMore\n\nBody");
    }

    #[test]
    fn keeps_roman_like_body_words() {
        let config = all_page_label_config();
        let input = ["mix", "did", "civic", "dim", "lid", "mid", "mill"].join("\n");

        assert_eq!(clean_text(&input, &config), input);
    }

    #[test]
    fn keeps_standalone_body_numeric_lines_without_page_evidence() {
        let config = all_page_label_config();
        let input = ["1984", "42", "2020"].join("\n");

        assert_eq!(clean_text(&input, &config), input);

        let document =
            StructuredDocument::from_pages(["Top\nIntro\nMore\n1984\n42\n2020\nEnd\nTail\nDone"]);
        let cleaned = clean_structured_document(&document, &config);
        assert_eq!(
            cleaned.text,
            "Top\nIntro\nMore\n1984\n42\n2020\nEnd\nTail\nDone"
        );
    }

    #[test]
    fn keeps_numbered_list_and_body_content_lines() {
        let config = all_page_label_config();
        let input = [
            "1. Introduction",
            "2. Methods",
            "12. Results",
            "1) Introduction",
            "(1) First point",
        ]
        .join("\n");

        assert_eq!(clean_text(&input, &config), input);
    }

    #[test]
    fn keeps_numeric_table_rows_away_from_page_edges() {
        let config = all_page_label_config();
        let input = "Top 1\nTop 2\nTop 3\n42\n1984\n2020\nBottom 1\nBottom 2\nBottom 3";
        let document = StructuredDocument::from_pages([input]);

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, input);
    }

    #[test]
    fn keeps_mid_sentence_page_references() {
        let config = all_page_label_config();
        let input = [
            "see page 5 for details",
            "as shown on p. 12",
            "turn to Page 7 before continuing",
        ]
        .join("\n");

        assert_eq!(clean_text(&input, &config), input);
    }

    #[test]
    fn keeps_body_single_letter_lines_without_page_evidence() {
        let config = all_page_label_config();
        let input = ["a", "e", "o", "I"].join("\n");

        assert_eq!(clean_text(&input, &config), input);
    }

    #[test]
    fn removes_url_email_and_doi_only_lines() {
        let config = CleaningConfig {
            remove_url_only_lines: true,
            remove_email_only_lines: true,
            remove_doi_only_lines: true,
            ..CleaningConfig::default()
        };

        let input = [
            "Before",
            "https://example.com",
            "www.example.org/path",
            "user@example.com",
            "doi:10.1234/example",
            "10.1000/xyz123",
            "See https://example.com inline.",
            "Contact user@example.com for details.",
            "The DOI is 10.1000/xyz123.",
            "After",
        ]
        .join("\n");

        let expected = [
            "Before",
            "See https://example.com inline.",
            "Contact user@example.com for details.",
            "The DOI is 10.1000/xyz123.",
            "After",
        ]
        .join("\n");

        assert_eq!(clean_text(&input, &config), expected);
    }

    #[test]
    fn removes_figure_and_table_caption_lines() {
        let config = CleaningConfig {
            remove_figure_table_captions: true,
            ..CleaningConfig::default()
        };

        let input = [
            "Before",
            "Figure 1. Distribution of tokens",
            "Fig. 2: Regression line",
            "Table 3 - Coefficients",
            "Tabela 4 Resultados",
            "The table 3 result is discussed here.",
            "After",
        ]
        .join("\n");

        assert_eq!(
            clean_text(&input, &config),
            "Before\nThe table 3 result is discussed here.\nAfter"
        );
    }

    #[test]
    fn joins_line_breaks_preserves_paragraph_boundaries() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };
        let input = "This is line one\nof the same paragraph.\n\nThis is another\nparagraph.";
        let expected = "This is line one of the same paragraph.\n\nThis is another paragraph.";

        assert_eq!(clean_text(input, &config), expected);
    }

    #[test]
    fn normalizes_irregular_line_breaks_preserves_single_character_lines() {
        let config = CleaningConfig {
            normalize_irregular_line_breaks: true,
            ..CleaningConfig::default()
        };
        let input = "Paragraph one\ncontinued.\n\nParagraph two\na\ncontinued.";
        let expected = "Paragraph one continued.\n\nParagraph two a continued.";

        assert_eq!(clean_text(input, &config), expected);
    }

    #[test]
    fn dehyphenates_simple_wrapped_words() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("inter-\nnational\ninfor-\nmation", &config),
            "international information"
        );
    }

    #[test]
    fn keeps_uppercase_hyphenated_continuations() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Newton-\nRaphson", &config), "Newton-Raphson");
    }

    #[test]
    fn keeps_numeric_or_symbolic_hyphenated_continuations() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("COVID-\n19", &config), "COVID-19");
        assert_eq!(clean_text("value-\n% change", &config), "value-% change");
    }

    #[test]
    fn keeps_short_ambiguous_hyphenated_compounds() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("well-\nbeing", &config), "well-being");
    }

    #[test]
    fn reflow_removes_excessive_spaces() {
        let config = CleaningConfig {
            normalize_irregular_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(clean_text("Alpha  \n  beta", &config), "Alpha beta");
    }

    #[test]
    fn reflow_keeps_terminal_hyphen_and_hyphen_before_blank_line() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        assert_eq!(
            clean_text("This remains inter-\n\nnational", &config),
            "This remains inter-\n\nnational"
        );
        assert_eq!(
            clean_text("This remains inter-", &config),
            "This remains inter-"
        );
    }

    #[test]
    fn tests_combined_dirty_fixture() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("dirty.txt");

        // Some ad-hoc test runs omit fixtures; this assertion only runs when the file exists.
        if !fixture_path.exists() {
            return;
        }

        let dirty_content = std::fs::read_to_string(fixture_path).unwrap();

        let config = CleaningConfig {
            lowercase: true,
            replace_diacritics: true,
            remove_standalone_page_numbers: true,
            remove_standalone_roman_page_numbers: true,
            remove_page_indicators: true,
            remove_page_delimiters: true,
            normalize_irregular_line_breaks: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_text(&dirty_content, &config);

        assert!(!cleaned.contains("THIS IS UPPERCASE TEXT."));
        assert!(cleaned.contains("this is uppercase text."));
        assert!(cleaned.contains("page 99"));
        assert!(cleaned.contains("--- page 99 ---"));
        assert!(cleaned.contains("ix"));
        assert!(cleaned.contains("1234"));
        assert!(!cleaned.contains("áéíóú"));
        assert!(cleaned.contains("aeiou"));
    }

    #[test]
    fn tests_default_config_leaves_clean_text_unchanged() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("clean.txt");

        if !fixture_path.exists() {
            return;
        }

        let clean_content = std::fs::read_to_string(fixture_path).unwrap();
        let config = CleaningConfig::default();
        let cleaned = clean_text(&clean_content, &config);

        assert_eq!(cleaned, clean_content);
    }

    #[test]
    fn test_cleaning_config_defaults() {
        let config = CleaningConfig::default();
        assert!(!config.normalize_whitespace_line_endings);
        assert!(!config.repair_wrapped_pdf_line_breaks);
        assert!(!config.remove_isolated_page_markers);
        assert!(!config.remove_url_only_lines);
        assert!(!config.remove_email_only_lines);
        assert!(!config.remove_doi_only_lines);
        assert!(!config.remove_figure_table_captions);
        assert!(!config.join_line_breaks);
        assert!(!config.normalize_irregular_line_breaks);
        assert!(!config.remove_standalone_page_numbers);
        assert!(!config.remove_standalone_roman_page_numbers);
        assert!(!config.remove_page_indicators);
        assert!(!config.remove_page_delimiters);
        assert!(!config.lowercase);
        assert!(!config.trim_lines);
        assert!(!config.collapse_blank_lines);
        assert!(!config.normalize_line_endings);
        assert!(!config.normalize_unicode);
        assert!(!config.replace_diacritics);
        assert!(!config.extract_html);
        assert!(!config.remove_headers);
        assert!(!config.remove_footers);
        assert!(!config.remove_footnotes);
        assert!(!config.remove_endnotes);
        assert!(!config.remove_comments);
        assert!(!config.remove_table_of_contents);
        assert!(config.removal_rules.is_empty());
        assert!(!config.remove_repeated_pdf_headers_footers);
        assert!(!config.remove_pdf_page_labels);
        assert!(!config.remove_pdf_symbol_heavy_artifacts);
        assert!(!config.remove_pdf_code_like_blocks);
        assert!(!config.remove_pdf_formula_like_lines);
        assert_eq!(
            config.table_extraction_strategy,
            TableExtractionStrategy::TabSeparated
        );
        assert_eq!(config.pdf_text_source, PdfTextSource::EmbeddedText);
        assert_eq!(config.pdf_ocr_quality, PdfOcrQuality::Balanced);
        assert_eq!(
            config.pdf_embedded_text_strategy,
            PdfEmbeddedTextStrategy::PdfiumFlat
        );
    }

    #[test]
    fn test_cleaning_config_deserializes_missing_pdf_text_source() {
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("pdf_text_source");

        let config: CleaningConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.pdf_text_source, PdfTextSource::EmbeddedText);
    }

    #[test]
    fn test_cleaning_config_deserializes_missing_pdf_ocr_quality() {
        let mut value = serde_json::to_value(CleaningConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("pdf_ocr_quality");

        let config: CleaningConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.pdf_ocr_quality, PdfOcrQuality::Balanced);
    }

    #[test]
    fn tests_default_processing_leaves_simple_text_unchanged() {
        let input = "  Some Text with Mixed CASE, \n  newlines, and áéíóú diacritics.  \n";
        let config = CleaningConfig::default();
        let cleaned = clean_text(input, &config);
        assert_eq!(cleaned, input);
    }

    #[test]
    fn tests_enabling_option_changes_text_as_expected() {
        let input = "HELLO WORLD";
        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text(input, &config), "hello world");

        let config = CleaningConfig {
            trim_lines: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("  hello  \n  world  ", &config), "hello\nworld");
    }

    fn page_zone_rule(text: &str, scope: RemovalScope) -> RemovalRule {
        RemovalRule {
            id: "rule-page-zone".to_string(),
            label: format!("Page-zone rule: {text}"),
            source: RemovalRuleSource::Manual,
            matcher: RemovalMatcher::Literal {
                text: text.to_string(),
            },
            scope,
            enabled: true,
        }
    }

    fn page_zone_normalized_rule(normalized_key: &str, scope: RemovalScope) -> RemovalRule {
        RemovalRule {
            id: "rule-page-zone-norm".to_string(),
            label: format!("Page-zone normalized: {normalized_key}"),
            source: RemovalRuleSource::Manual,
            matcher: RemovalMatcher::NormalizedLine {
                normalized_key: normalized_key.to_string(),
            },
            scope,
            enabled: true,
        }
    }

    #[test]
    fn page_zone_scopes_serialize_and_deserialize() {
        let rule_top = page_zone_rule("Header", RemovalScope::PageTop);
        let value_top = serde_json::to_value(&rule_top).unwrap();
        assert_eq!(value_top["scope"], json!("page_top"));
        let decoded_top: RemovalRule = serde_json::from_value(value_top).unwrap();
        assert_eq!(decoded_top, rule_top);

        let rule_bottom = page_zone_rule("Footer", RemovalScope::PageBottom);
        let value_bottom = serde_json::to_value(&rule_bottom).unwrap();
        assert_eq!(value_bottom["scope"], json!("page_bottom"));
        let decoded_bottom: RemovalRule = serde_json::from_value(value_bottom).unwrap();
        assert_eq!(decoded_bottom, rule_bottom);

        let rule_top_or_bottom = page_zone_rule("HeaderOrFooter", RemovalScope::PageTopOrBottom);
        let value_tob = serde_json::to_value(&rule_top_or_bottom).unwrap();
        assert_eq!(value_tob["scope"], json!("page_top_or_bottom"));
        let decoded_tob: RemovalRule = serde_json::from_value(value_tob).unwrap();
        assert_eq!(decoded_tob, rule_top_or_bottom);
    }

    #[test]
    fn clean_text_applies_whole_line_but_ignores_page_zones() {
        let config_wl = CleaningConfig {
            removal_rules: vec![whole_line_rule("MatchMe")],
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("Line1\nMatchMe\nLine3", &config_wl),
            "Line1\nLine3"
        );

        let config_top = CleaningConfig {
            removal_rules: vec![page_zone_rule("MatchMe", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("Line1\nMatchMe\nLine3", &config_top),
            "Line1\nMatchMe\nLine3"
        );

        let config_bottom = CleaningConfig {
            removal_rules: vec![page_zone_rule("MatchMe", RemovalScope::PageBottom)],
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("Line1\nMatchMe\nLine3", &config_bottom),
            "Line1\nMatchMe\nLine3"
        );

        let config_tob = CleaningConfig {
            removal_rules: vec![page_zone_rule("MatchMe", RemovalScope::PageTopOrBottom)],
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("Line1\nMatchMe\nLine3", &config_tob),
            "Line1\nMatchMe\nLine3"
        );
    }

    #[test]
    fn clean_structured_document_applies_page_zone_rules_correctly() {
        let config_top = CleaningConfig {
            removal_rules: vec![page_zone_rule("Target", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };

        let doc_match_top = StructuredDocument::from_pages(["Target\nL2\nL3\nMid\nL5\nL6\nL7"]);
        let cleaned_top = clean_structured_document(&doc_match_top, &config_top);
        assert_eq!(cleaned_top.text, "L2\nL3\nMid\nL5\nL6\nL7");

        let doc_match_mid = StructuredDocument::from_pages(["L1\nL2\nL3\nTarget\nL5\nL6\nL7"]);
        let cleaned_mid = clean_structured_document(&doc_match_mid, &config_top);
        assert_eq!(cleaned_mid.text, "L1\nL2\nL3\nTarget\nL5\nL6\nL7");

        let doc_match_bot = StructuredDocument::from_pages(["L1\nL2\nL3\nMid\nL5\nL6\nTarget"]);
        let cleaned_bot = clean_structured_document(&doc_match_bot, &config_top);
        assert_eq!(cleaned_bot.text, "L1\nL2\nL3\nMid\nL5\nL6\nTarget");

        let config_bot = CleaningConfig {
            removal_rules: vec![page_zone_rule("Target", RemovalScope::PageBottom)],
            ..CleaningConfig::default()
        };

        let cleaned_bot_applied = clean_structured_document(&doc_match_bot, &config_bot);
        assert_eq!(cleaned_bot_applied.text, "L1\nL2\nL3\nMid\nL5\nL6");

        let config_tob = CleaningConfig {
            removal_rules: vec![page_zone_rule("Target", RemovalScope::PageTopOrBottom)],
            ..CleaningConfig::default()
        };

        let cleaned_tob_top = clean_structured_document(&doc_match_top, &config_tob);
        assert_eq!(cleaned_tob_top.text, "L2\nL3\nMid\nL5\nL6\nL7");

        let cleaned_tob_bot = clean_structured_document(&doc_match_bot, &config_tob);
        assert_eq!(cleaned_tob_bot.text, "L1\nL2\nL3\nMid\nL5\nL6");

        let cleaned_tob_mid = clean_structured_document(&doc_match_mid, &config_tob);
        assert_eq!(cleaned_tob_mid.text, "L1\nL2\nL3\nTarget\nL5\nL6\nL7");

        let doc_short = StructuredDocument::from_pages(["Target\nOther"]);
        let cleaned_short_top = clean_structured_document(&doc_short, &config_top);
        assert_eq!(cleaned_short_top.text, "Other");

        let cleaned_short_bot = clean_structured_document(&doc_short, &config_bot);
        assert_eq!(cleaned_short_bot.text, "Other");
    }

    #[test]
    fn matchers_and_disabled_rules_with_page_zones() {
        let doc = StructuredDocument::from_pages(["Page 1\nBody", "Page 2\nBody"]);

        let config_norm = CleaningConfig {
            removal_rules: vec![page_zone_normalized_rule("page #", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };
        let cleaned_norm = clean_structured_document(&doc, &config_norm);
        assert_eq!(cleaned_norm.text, "Body\n\nBody");

        let mut disabled_rule = page_zone_rule("Page 1", RemovalScope::PageTop);
        disabled_rule.enabled = false;
        let config_disabled = CleaningConfig {
            removal_rules: vec![disabled_rule],
            ..CleaningConfig::default()
        };
        let cleaned_disabled = clean_structured_document(&doc, &config_disabled);
        assert_eq!(cleaned_disabled.text, "Page 1\nBody\n\nPage 2\nBody");

        let config_coexist = CleaningConfig {
            remove_patterns: vec!["Body".to_string()],
            removal_rules: vec![page_zone_rule("Page 1", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };
        let cleaned_coexist = clean_structured_document(&doc, &config_coexist);
        assert_eq!(cleaned_coexist.text, "\n\nPage 2\n");
    }

    #[test]
    fn clean_structured_document_contract_equivalence() {
        let doc = StructuredDocument::from_pages(["Header\nBody", "Header\nMore body"]);

        let config_no_pz = CleaningConfig {
            removal_rules: vec![whole_line_rule("Header")],
            ..CleaningConfig::default()
        };
        let cleaned_no_pz = clean_structured_document(&doc, &config_no_pz);
        assert_eq!(
            cleaned_no_pz.text,
            clean_text(&doc.to_flat_text(), &config_no_pz)
        );

        let config_with_pz = CleaningConfig {
            removal_rules: vec![page_zone_rule("Header", RemovalScope::PageTop)],
            ..CleaningConfig::default()
        };
        let cleaned_with_pz = clean_structured_document(&doc, &config_with_pz);
        assert_eq!(cleaned_with_pz.text, "Body\n\nMore body");
        assert_ne!(
            cleaned_with_pz.text,
            clean_text(&doc.to_flat_text(), &config_with_pz)
        );
        assert_eq!(
            clean_text(&doc.to_flat_text(), &config_with_pz),
            "Header\nBody\n\nHeader\nMore body"
        );
    }

    #[test]
    fn clean_text_characterisation_test_locks_output_byte_for_byte() {
        let input = "Page 1

Introduction
This is a line of natural text.
cid:12
Some other line with a DROP pattern inside it.
Another line with a replacement old_word inside.
A line with   too    much   space.
-
14
DROP_LITERAL
A line that might be reflowed-
across to the next line.
";

        let config = CleaningConfig {
            remove_obvious_extraction_noise: true,
            remove_standalone_page_numbers: true,
            remove_isolated_page_markers: true,
            remove_patterns: vec!["DROP".to_string(), "DROP_LITERAL".to_string()],
            replace_patterns: vec![ReplacementRule {
                pattern: "old_word".to_string(),
                replacement: "new_word".to_string(),
            }],
            normalize_whitespace_line_endings: true,
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_text(input, &config);

        let expected = "Page 1\n\nIntroduction This is a line of natural text. Some other line with a pattern inside it. Another line with a replacement new_word inside. A line with too much space. -14 _LITERAL A line that might be reflowedacross to the next line.";

        assert_eq!(
            cleaned, expected,
            "Cleaned text output must remain byte-for-byte identical before and after audit refactor."
        );
    }

    #[test]
    fn clean_structured_document_characterisation_test_locks_output_byte_for_byte() {
        let pages = [
            "Page 1\nHeader A\nSome content here.\ncid:12\nFooter A\n",
            "Page 2\nHeader B\nMore text.\n- 2 -\nDROP_ME\n",
        ];
        let doc = StructuredDocument::from_pages(pages);

        let config = CleaningConfig {
            remove_obvious_extraction_noise: true,
            remove_standalone_page_numbers: true,
            remove_isolated_page_markers: true,
            remove_patterns: vec!["DROP_ME".to_string()],
            removal_rules: vec![RemovalRule {
                id: "header".to_string(),
                label: "header".to_string(),
                source: RemovalRuleSource::Manual,
                scope: RemovalScope::PageTop,
                matcher: RemovalMatcher::Literal {
                    text: "Header A".to_string(),
                },
                enabled: true,
            }],
            normalize_whitespace_line_endings: true,
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        let cleaned_without_audit = clean_structured_document_impl(&doc, &config, None, None);

        let mut collector = CleaningReportCollector::default();
        let cleaned_with_audit =
            clean_structured_document_impl(&doc, &config, Some(&mut collector), None);

        assert_eq!(
            cleaned_without_audit.text, cleaned_with_audit.text,
            "Cleaned structured document text output must remain byte-for-byte identical with or without audit collection."
        );

        // Secondary guard against regressions
        let expected = "Some content here. Footer A\n\nHeader B More text.";
        assert_eq!(
            cleaned_with_audit.text, expected,
            "Cleaned structured document text output differs from expected characterisation payload."
        );
    }
}
