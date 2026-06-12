//! Word-count helper that applies the configured cleaning pipeline.
//!
//! This module exists so `compute_word_count_command` in the Tauri layer
//! can produce a **configured/processed** word count that matches export
//! and preview processing as closely as feasible, rather than a raw word
//! count that ignores the active `CleaningConfig`.
//!
//! OCR is deliberately **disabled** for word-count (matches the pre-existing
//! raw-count behaviour) to keep the operation fast.  If export/preview
//! enable OCR for PDFs, the word count may slightly undercount those files,
//! though this is typically negligible.

use crate::cache::ExtractionCache;
use crate::clean::{CleaningConfig, clean_text};
use crate::scan::{DocumentRecord, DocumentType};

/// Count words in a single document **after** applying the configured
/// `CleaningConfig` to both extraction and post-extraction cleaning.
///
/// Pipeline (mirrors `export_corpus` and `preview_processed_files` closely):
/// 1. Read source bytes.
/// 2. Extract structured text (PDF/DOCX) with config-aware options.
/// 3. If `cleaning_config.extract_html` is set, run HTML-to-text extraction.
/// 4. Run `clean_text` with the full config.
/// 5. Return `text.split_whitespace().count()`.
///
/// Errors (I/O, extraction failure, etc.) silently yield `0`, preserving
/// the pre-existing error-handling behaviour of the raw word-count command.
pub fn count_words_for_record(
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> usize {
    let source_text = if let Some(cache) = cache {
        let pdf_options = if record.document_type == DocumentType::Pdf {
            Some(crate::pdf::PdfExtractionOptions::from_cleaning_config(
                cleaning_config,
            ))
        } else {
            None
        };
        match cache.get_or_extract(record, pdf_options, cleaning_config) {
            Ok(entry) => entry.extracted_text,
            Err(_) => return 0,
        }
    } else {
        // Fallback: direct extraction (no cache available)
        if record.document_type == DocumentType::Docx {
            if let Ok(bytes) = std::fs::read(&record.source_path)
                && let Ok(extracted) = crate::docx::extract_docx(&bytes, cleaning_config)
            {
                extracted.text
            } else {
                return 0;
            }
        } else if record.document_type == DocumentType::Pdf {
            if let Ok(bytes) = std::fs::read(&record.source_path)
                && let Ok(extracted) = crate::pdf::extract_pdf(
                    &bytes,
                    None,
                    crate::pdf::PdfExtractionOptions::from_cleaning_config(cleaning_config),
                )
            {
                extracted.text
            } else {
                return 0;
            }
        } else {
            // Plain text, HTML, or other textual files
            if let Ok(bytes) = std::fs::read(&record.source_path) {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                return 0;
            }
        }
    };

    count_words_in_processed_text(&source_text, cleaning_config)
}

/// Apply HTML extraction (if enabled) and `clean_text`, then count words.
fn count_words_in_processed_text(raw: &str, cleaning_config: &CleaningConfig) -> usize {
    let text = if cleaning_config.extract_html {
        crate::html::extract_html(raw)
    } else {
        raw.to_string()
    };
    let cleaned = clean_text(&text, cleaning_config);
    cleaned.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{CleaningConfig, ReplacementRule};
    use crate::scan::DocumentRecord;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Helper: build a minimal DocumentRecord backed by a real temp file.
    fn text_record(source_dir: &std::path::Path, content: &str) -> DocumentRecord {
        let path = source_dir.join("test.txt");
        fs::write(&path, content).unwrap();
        DocumentRecord {
            source_path: path.clone(),
            relative_path: PathBuf::from("test.txt"),
            document_type: DocumentType::Text,
            size_bytes: content.len() as u64,
        }
    }

    /// Raw word count on "hello REMOVE world" is 3.
    /// With custom removal "REMOVE" the configured count should be 2.
    #[test]
    fn custom_removal_reduces_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello REMOVE world");

        let config = CleaningConfig {
            remove_patterns: vec!["REMOVE".to_string()],
            ..CleaningConfig::default()
        };

        let count = count_words_for_record(&record, &config, None);
        assert_eq!(
            count, 2,
            "expected 2 words after removing REMOVE, got {count}"
        );
    }

    /// Default config should produce a raw count of 3.
    #[test]
    fn default_config_gives_raw_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello REMOVE world");

        let config = CleaningConfig::default();
        let count = count_words_for_record(&record, &config, None);
        assert_eq!(
            count, 3,
            "expected 3 words with default config, got {count}"
        );
    }

    /// Lowercase config should not change word count.
    #[test]
    fn lowercase_preserves_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "Hello WORLD");

        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let count = count_words_for_record(&record, &config, None);
        assert_eq!(count, 2, "lowercase should not change the word count");
    }

    /// Replace pattern should change word count when the pattern matches whole words.
    #[test]
    fn replace_pattern_affects_word_count() {
        let dir = tempdir().unwrap();
        // "old" becomes "" — effectively removes "old"
        let record = text_record(dir.path(), "hello old world");

        let config = CleaningConfig {
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "".to_string(),
            }],
            ..CleaningConfig::default()
        };
        let count = count_words_for_record(&record, &config, None);
        assert_eq!(count, 2, "expected 2 words after replacing 'old' with ''");
    }

    /// Non-existent files yield 0 (preserving the pre-existing error behaviour).
    #[test]
    fn missing_file_returns_zero() {
        let record = DocumentRecord {
            source_path: PathBuf::from("/tmp/does_not_exist_12345.txt"),
            relative_path: PathBuf::from("does_not_exist_12345.txt"),
            document_type: DocumentType::Text,
            size_bytes: 0,
        };
        let config = CleaningConfig::default();
        let count = count_words_for_record(&record, &config, None);
        assert_eq!(count, 0, "missing file should yield 0");
    }
}
