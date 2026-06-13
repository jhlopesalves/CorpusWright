//! Word-count helper for configured text extraction and cleaning.
//!
//! Counts follow the active `CleaningConfig` so the Tauri layer aligns with
//! export and preview processing.
//!
//! OCR modes only use matching cached extraction results. Automatic word counts
//! skip uncached OCR PDFs instead of counting a different text source.

use crate::cache::ExtractionCache;
use crate::clean::{CleaningConfig, PdfTextSource, clean_text};
use crate::scan::{DocumentRecord, DocumentType};

pub const OCR_WORD_COUNT_SKIPPED_STATUS: &str =
    "Token count skipped for OCR mode until OCR text is generated.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordCountOutcome {
    pub count: usize,
    pub skipped_ocr_mode: bool,
}

impl WordCountOutcome {
    fn count(count: usize) -> Self {
        Self {
            count,
            skipped_ocr_mode: false,
        }
    }

    fn skipped_ocr_mode() -> Self {
        Self {
            count: 0,
            skipped_ocr_mode: true,
        }
    }
}

/// Count words in a single document after applying the configured extraction
/// and post-extraction cleaning.
///
/// I/O and extraction errors yield a zero count. OCR-mode PDFs return a skipped
/// result unless a matching extraction is already cached.
pub fn count_words_for_record(
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> WordCountOutcome {
    let pdf_options = if record.document_type == DocumentType::Pdf {
        Some(crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(
            cleaning_config,
        ))
    } else {
        None
    };

    if record.document_type == DocumentType::Pdf
        && cleaning_config.pdf_text_source != PdfTextSource::EmbeddedText
    {
        if let Some(entry) =
            cache.and_then(|cache| cache.try_get(record, pdf_options, cleaning_config))
        {
            let source_text =
                apply_pdf_cleaning_for_word_count(&entry.extracted_text, cleaning_config);
            return WordCountOutcome::count(count_words_in_processed_text(
                &source_text,
                cleaning_config,
            ));
        }
        return WordCountOutcome::skipped_ocr_mode();
    }

    let source_text = if let Some(cache) = cache {
        match cache.get_or_extract(record, pdf_options, cleaning_config) {
            Ok(entry) => entry.extracted_text,
            Err(_) => return WordCountOutcome::count(0),
        }
    } else {
        if record.document_type == DocumentType::Docx {
            if let Ok(bytes) = std::fs::read(&record.source_path)
                && let Ok(extracted) = crate::docx::extract_docx(&bytes, cleaning_config)
            {
                extracted.text
            } else {
                return WordCountOutcome::count(0);
            }
        } else if record.document_type == DocumentType::Pdf {
            if let Ok(bytes) = std::fs::read(&record.source_path)
                && let Ok(extracted) = crate::pdf::extract_pdf(
                    &bytes,
                    None,
                    crate::pdf::PdfExtractionOptions::raw_from_cleaning_config(cleaning_config),
                )
            {
                extracted.text
            } else {
                return WordCountOutcome::count(0);
            }
        } else {
            // Plain text, HTML, or other textual files
            if let Ok(bytes) = std::fs::read(&record.source_path) {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                return WordCountOutcome::count(0);
            }
        }
    };

    let source_text = if record.document_type == DocumentType::Pdf {
        apply_pdf_cleaning_for_word_count(&source_text, cleaning_config)
    } else {
        source_text
    };

    WordCountOutcome::count(count_words_in_processed_text(&source_text, cleaning_config))
}

fn apply_pdf_cleaning_for_word_count(raw: &str, cleaning_config: &CleaningConfig) -> String {
    let (text, _) = crate::pdf::clean_extracted_pdf_text(raw, cleaning_config);
    text
}

/// Counts words after optional HTML extraction and `clean_text`.
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
    use crate::clean::{CleaningConfig, PdfTextSource, ReplacementRule};
    use crate::scan::DocumentRecord;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

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

    #[test]
    fn custom_removal_reduces_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello REMOVE world");

        let config = CleaningConfig {
            remove_patterns: vec!["REMOVE".to_string()],
            ..CleaningConfig::default()
        };

        let count = count_words_for_record(&record, &config, None).count;
        assert_eq!(
            count, 2,
            "expected 2 words after removing REMOVE, got {count}"
        );
    }

    #[test]
    fn default_config_gives_raw_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello REMOVE world");

        let config = CleaningConfig::default();
        let count = count_words_for_record(&record, &config, None).count;
        assert_eq!(
            count, 3,
            "expected 3 words with default config, got {count}"
        );
    }

    #[test]
    fn lowercase_preserves_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "Hello WORLD");

        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let count = count_words_for_record(&record, &config, None).count;
        assert_eq!(count, 2, "lowercase should not change the word count");
    }

    #[test]
    fn replace_pattern_affects_word_count() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello old world");

        let config = CleaningConfig {
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "".to_string(),
            }],
            ..CleaningConfig::default()
        };
        let count = count_words_for_record(&record, &config, None).count;
        assert_eq!(count, 2, "expected 2 words after replacing 'old' with ''");
    }

    #[test]
    fn missing_file_returns_zero() {
        let record = DocumentRecord {
            source_path: PathBuf::from("/tmp/does_not_exist_12345.txt"),
            relative_path: PathBuf::from("does_not_exist_12345.txt"),
            document_type: DocumentType::Text,
            size_bytes: 0,
        };
        let config = CleaningConfig::default();
        let count = count_words_for_record(&record, &config, None).count;
        assert_eq!(count, 0, "missing file should yield 0");
    }

    #[test]
    fn uncached_ocr_pdf_word_count_is_skipped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("scan.pdf");
        fs::write(&path, b"%PDF-1.4").unwrap();
        let record = DocumentRecord {
            source_path: path,
            relative_path: PathBuf::from("scan.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 8,
        };
        let config = CleaningConfig {
            pdf_text_source: PdfTextSource::Ocr,
            ..CleaningConfig::default()
        };

        let outcome = count_words_for_record(&record, &config, None);

        assert_eq!(outcome.count, 0);
        assert!(outcome.skipped_ocr_mode);
    }
}
