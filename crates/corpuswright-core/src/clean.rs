use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

use crate::structured_document::StructuredDocument;
use crate::text_normalization::normalize_line_for_repeated_artifact;

lazy_static! {
    static ref RE_STANDALONE_ARABIC: Regex =
        Regex::new(r"(?m)^[ \t]*\d+[ \t]*(?:\r?\n|$)").unwrap();
    static ref RE_STANDALONE_ROMAN: Regex =
        Regex::new(r"(?im)^[ \t]*[ivxlcdm]+[ \t]*(?:\r?\n|$)").unwrap();
    static ref RE_PAGE_INDICATORS: Regex =
        Regex::new(r"(?i)\b(?:page|pag\.)[ \t]*(?:[0-9]+|[ivxlcdm]+)\b").unwrap();
    static ref RE_PAGE_DELIMITERS: Regex = Regex::new(
        r"(?im)^[ \t]*-+[ \t]*(?:page|pag\.)[ \t]*(?:[0-9]+|[ivxlcdm]+)[ \t]*-+[ \t]*(?:\r?\n|$)"
    )
    .unwrap();
    static ref RE_JOIN_LINE_BREAKS: Regex = Regex::new(r"[ \t]*\r?\n[ \t]*").unwrap();
    static ref RE_EXCESSIVE_SPACES: Regex = Regex::new(r" {2,}").unwrap();
}

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct RemovalRule {
    pub id: String,
    pub label: String,
    pub source: RemovalRuleSource,
    pub matcher: RemovalMatcher,
    pub scope: RemovalScope,
    pub enabled: bool,
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

/// Processes a string according to the given cleaning rules.
pub fn clean_text(text: &str, config: &CleaningConfig) -> String {
    let mut cleaned = text.to_string();

    if config.normalize_line_endings {
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

    if config.remove_page_delimiters {
        cleaned = RE_PAGE_DELIMITERS.replace_all(&cleaned, "").to_string();
    }

    if config.remove_page_indicators {
        cleaned = RE_PAGE_INDICATORS.replace_all(&cleaned, "").to_string();
    }

    if config.remove_standalone_page_numbers {
        cleaned = RE_STANDALONE_ARABIC.replace_all(&cleaned, "").to_string();
    }

    if config.remove_standalone_roman_page_numbers {
        cleaned = RE_STANDALONE_ROMAN.replace_all(&cleaned, "").to_string();
    }

    if config.normalize_irregular_line_breaks {
        cleaned = normalize_irregular_line_breaks(&cleaned);
    }

    if config.join_line_breaks {
        cleaned = RE_JOIN_LINE_BREAKS.replace_all(&cleaned, " ").to_string();
        cleaned = RE_EXCESSIVE_SPACES.replace_all(&cleaned, " ").to_string();
    }

    cleaned = remove_structured_removal_rules(cleaned, config);
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

    if config.trim_lines {
        cleaned = cleaned
            .split('\n')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("\n");
    }

    if config.collapse_blank_lines {
        cleaned = collapse_blank_lines(&cleaned);
    }

    cleaned
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

    if config.remove_page_delimiters {
        cleaned = RE_PAGE_DELIMITERS.replace_all(&cleaned, "").to_string();
    }

    if config.remove_page_indicators {
        cleaned = RE_PAGE_INDICATORS.replace_all(&cleaned, "").to_string();
    }

    if config.remove_standalone_page_numbers {
        cleaned = RE_STANDALONE_ARABIC.replace_all(&cleaned, "").to_string();
    }

    if config.remove_standalone_roman_page_numbers {
        cleaned = RE_STANDALONE_ROMAN.replace_all(&cleaned, "").to_string();
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
                let effective_text = if config.lowercase {
                    text.to_lowercase()
                } else {
                    text.clone()
                };
                let trimmed_rule = effective_text.trim();
                if !trimmed_rule.is_empty() && prepared_line.trim() == trimmed_rule {
                    return true;
                }
            }
            RemovalMatcher::NormalizedLine { normalized_key } => {
                let trimmed_rule = normalized_key.trim();
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
                RemovalScope::PageTop
                    | RemovalScope::PageBottom
                    | RemovalScope::PageTopOrBottom
            )
    })
}

/// Cleans a page-aware document without exposing stale page metadata.
///
/// The flat output is always the same result as `clean_text` on the document's
/// deterministic flat text. Cleaned page text is returned only when cleaning
/// each page independently and joining the cleaned pages produces exactly the
/// same flat output.
pub fn clean_structured_document(
    document: &StructuredDocument,
    config: &CleaningConfig,
) -> CleanedStructuredDocument {
    if !has_page_zone_rules(config) {
        let text = clean_text(&document.to_flat_text(), config);
        let page_texts = document
            .pages
            .iter()
            .map(|page| clean_text(&page.to_text(), config))
            .collect::<Vec<_>>();

        let joined_page_texts = page_texts.join("\n\n");
        let page_texts = (joined_page_texts == text).then_some(page_texts);

        CleanedStructuredDocument { text, page_texts }
    } else {
        // Pre-filter the document lines by applying structured removal rules (including page-zone scopes).
        let mut filtered_pages = Vec::with_capacity(document.pages.len());
        for page in &document.pages {
            let mut filtered_lines = Vec::with_capacity(page.lines.len());
            for line in &page.lines {
                let prepared = prepare_line_for_rule_matching(&line.text, config);
                if !should_remove_line_by_rules(
                    &prepared,
                    line.is_page_top,
                    line.is_page_bottom,
                    config,
                ) {
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
        };

        // Clean each page of the filtered document independently.
        let page_texts = filtered_document
            .pages
            .iter()
            .map(|page| clean_text(&page.to_text(), config))
            .collect::<Vec<_>>();

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
    for para in norm_text.split("\n\n") {
        let mut valid_lines = Vec::new();
        for line in para.split('\n') {
            let trimmed = line.trim();
            if trimmed.chars().count() == 1 {
                continue;
            }
            if !trimmed.is_empty() {
                valid_lines.push(trimmed);
            }
        }
        if !valid_lines.is_empty() {
            let joined = valid_lines.join(" ");
            let collapsed = RE_EXCESSIVE_SPACES.replace_all(&joined, " ").to_string();
            paragraphs.push(collapsed);
        }
    }
    paragraphs.join("\n\n")
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
                let effective_text = if config.lowercase {
                    text.to_lowercase()
                } else {
                    text.clone()
                };
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
                let trimmed = normalized_key.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            RemovalMatcher::Literal { .. } => None,
        })
        .collect()
}

fn remove_structured_removal_rules(text: String, config: &CleaningConfig) -> String {
    let whole_line_literals = effective_whole_line_literals(config);
    let whole_line_normalized_keys = effective_whole_line_normalized_keys(config);
    if whole_line_literals.is_empty() && whole_line_normalized_keys.is_empty() {
        return text;
    }

    remove_whole_lines_matching_structured(&text, &whole_line_literals, &whole_line_normalized_keys)
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

fn remove_literal_patterns_sequential(mut text: String, patterns: &[String]) -> String {
    for pattern in patterns {
        text = text.replace(pattern, "");
    }
    text
}

fn remove_literal_patterns_batched(text: &str, patterns: &[String]) -> Option<String> {
    let automaton = literal_remove_automaton(patterns);
    let mut cleaned = String::with_capacity(text.len());
    let mut last_end = 0;
    let mut found_match = false;

    for mat in automaton.find_iter(text) {
        found_match = true;
        cleaned.push_str(&text[last_end..mat.start()]);
        last_end = mat.end();
    }

    if !found_match {
        return Some(text.to_string());
    }

    cleaned.push_str(&text[last_end..]);

    // Deletions can join surrounding text into a later pattern. The sequential
    // path is the reference behaviour for those cascading cases.
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
    fn clean_structured_document_omits_page_texts_when_boundaries_are_not_preserved() {
        let document = StructuredDocument::from_pages(["Alpha", "Beta"]);
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };

        let cleaned = clean_structured_document(&document, &config);

        assert_eq!(cleaned.text, clean_text(&document.to_flat_text(), &config));
        assert_eq!(cleaned.text, "Alpha Beta");
        assert!(cleaned.page_texts.is_none());
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

    #[test]
    fn removes_standalone_arabic_page_numbers() {
        let config = CleaningConfig {
            remove_standalone_page_numbers: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("hello\n12\nworld", &config), "hello\nworld");
        assert_eq!(clean_text("12\nworld", &config), "world");
        assert_eq!(clean_text("hello\n1042", &config), "hello\n");
        assert_eq!(clean_text("hello 12 world", &config), "hello 12 world"); // preserved
    }

    #[test]
    fn removes_standalone_roman_page_numbers() {
        let config = CleaningConfig {
            remove_standalone_roman_page_numbers: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("hello\niv\nworld", &config), "hello\nworld");
        assert_eq!(clean_text("hello\nXII\nworld", &config), "hello\nworld");
        assert_eq!(clean_text("hello\nxxi\nworld", &config), "hello\nworld");
        assert_eq!(clean_text("civic duty", &config), "civic duty"); // ordinary word not removed
        assert_eq!(clean_text("I\nam", &config), "am");
    }

    #[test]
    fn removes_page_indicators() {
        let config = CleaningConfig {
            remove_page_indicators: true,
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("hello Page 12 world", &config), "hello  world");
        assert_eq!(clean_text("hello pag. xvi world", &config), "hello  world");
        assert_eq!(clean_text("Page IV", &config), "");
    }

    #[test]
    fn removes_page_delimiters() {
        let config = CleaningConfig {
            remove_page_delimiters: true,
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("hello\n--- Page 12 ---\nworld", &config),
            "hello\nworld"
        );
        assert_eq!(clean_text("--- pag. xvi ---", &config), "");
        assert_eq!(
            clean_text("This is --- page 12 --- test", &config),
            "This is --- page 12 --- test"
        ); // only removes whole line
    }

    #[test]
    fn joins_line_breaks() {
        let config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };
        assert_eq!(
            clean_text("hello\nworld\n\nagain", &config),
            "hello world again"
        );
    }

    #[test]
    fn normalizes_irregular_line_breaks() {
        let config = CleaningConfig {
            normalize_irregular_line_breaks: true,
            ..CleaningConfig::default()
        };
        let input = "Paragraph one\ncontinued.\n\nParagraph two\na\ncontinued.";
        // Single-character lines are dropped by irregular line-break normalisation.
        let expected = "Paragraph one continued.\n\nParagraph two continued.";
        assert_eq!(clean_text(input, &config), expected);
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
        assert!(!cleaned.contains("page 99"));
        assert!(!cleaned.contains("--- page 99 ---"));
        assert!(!cleaned.contains("ix\n"));
        assert!(!cleaned.contains("1234\n"));
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
        let doc = StructuredDocument::from_pages(["L1\nL2\nL3\nMid\nL5\nL6\nL7"]);

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
}
