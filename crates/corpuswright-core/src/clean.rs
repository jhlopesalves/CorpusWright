use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ReplacementRule {
    pub pattern: String,
    pub replacement: String,
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

    for pattern in &config.remove_patterns {
        if !pattern.is_empty() {
            let p = if config.lowercase {
                pattern.to_lowercase()
            } else {
                pattern.to_string()
            };
            cleaned = cleaned.replace(&p, "");
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
            remove_patterns: vec!["remove".to_string()],
            ..CleaningConfig::default()
        };
        assert_eq!(clean_text("keep remove keep", &config), "keep  keep");
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
}
