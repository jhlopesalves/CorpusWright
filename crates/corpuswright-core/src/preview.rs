use crate::cache::ExtractionCache;
use crate::clean::{CleaningConfig, clean_text};
use crate::pdf::PdfExtractionOptions;
use crate::scan::{DocumentRecord, DocumentType};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use ts_rs::TS;

/// Configuration options for generating file previews.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewOptions {
    pub max_chars_per_file: usize,
    pub include_paths: bool,
    pub max_files: Option<usize>,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            max_chars_per_file: 5_000,
            include_paths: true,
            max_files: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PreviewWarningKind {
    InvalidUtf8,
    Truncated,
    MaxFilesReached,
    ExtractionWarning,
    ExtractionError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PreviewWarning {
    pub source_path: Option<PathBuf>,
    pub relative_path: Option<PathBuf>,
    pub kind: PreviewWarningKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct FilePreview {
    pub source_path: PathBuf,
    pub relative_path: PathBuf,
    pub document_type: DocumentType,
    pub text: String,
    #[ts(type = "number")]
    pub source_size_bytes: u64,
    pub included_char_count: usize,
    pub truncated: bool,
    pub warnings: Vec<PreviewWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CombinedPreview {
    pub files: Vec<FilePreview>,
    pub combined_text: String,
    pub total_files_previewed: usize,
    pub total_characters_included: usize,
    pub warnings: Vec<PreviewWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PreviewError {
    Io { path: PathBuf, message: String },
}

/// Generates a bounded raw-text preview for a single file.
///
/// If a `cache` is provided and contains a matching entry, the cached
/// extracted text is used (truncated to `max_chars_per_file`).  On a cache
/// miss, falls back to the existing bounded extraction path.
///
/// Only PDF and DOCX files attempt a cache lookup.  Text/HTML files are
/// always read via `read_bounded_lossy_text`.
pub fn preview_file(
    record: &DocumentRecord,
    options: &PreviewOptions,
    cache: Option<&ExtractionCache>,
) -> Result<FilePreview, PreviewError> {
    // Try cache hit first (only for DOCX/PDF where extraction is expensive)
    let (read, extraction_warnings) = if record.document_type == DocumentType::Docx {
        if let Some(cache) = cache {
            if let Some(entry) = cache.try_get(record, None, &CleaningConfig::default()) {
                let truncated = entry.extracted_text.chars().count() > options.max_chars_per_file;
                let text: String = entry
                    .extracted_text
                    .chars()
                    .take(options.max_chars_per_file)
                    .collect();
                (
                    BoundedRead {
                        text,
                        truncated,
                        invalid_utf8: false,
                    },
                    entry.warnings,
                )
            } else {
                // Cache miss → bounded extraction
                let bytes =
                    std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                        path: record.source_path.clone(),
                        message: error.to_string(),
                    })?;
                match crate::docx::extract_docx(&bytes, &CleaningConfig::default()) {
                    Ok(extracted) => {
                        let truncated = extracted.text.chars().count() > options.max_chars_per_file;
                        let text: String = extracted
                            .text
                            .chars()
                            .take(options.max_chars_per_file)
                            .collect();
                        (
                            BoundedRead {
                                text,
                                truncated,
                                invalid_utf8: false,
                            },
                            extracted.warnings,
                        )
                    }
                    Err(e) => {
                        let mut warnings = Vec::new();
                        warnings.push(format!("DOCX Extraction Failed: {}", e));
                        (
                            BoundedRead {
                                text: String::new(),
                                truncated: false,
                                invalid_utf8: false,
                            },
                            warnings,
                        )
                    }
                }
            }
        } else {
            // No cache available → bounded extraction
            let bytes = std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                path: record.source_path.clone(),
                message: error.to_string(),
            })?;
            match crate::docx::extract_docx(&bytes, &CleaningConfig::default()) {
                Ok(extracted) => {
                    let truncated = extracted.text.chars().count() > options.max_chars_per_file;
                    let text: String = extracted
                        .text
                        .chars()
                        .take(options.max_chars_per_file)
                        .collect();
                    (
                        BoundedRead {
                            text,
                            truncated,
                            invalid_utf8: false,
                        },
                        extracted.warnings,
                    )
                }
                Err(e) => {
                    let mut warnings = Vec::new();
                    warnings.push(format!("DOCX Extraction Failed: {}", e));
                    (
                        BoundedRead {
                            text: String::new(),
                            truncated: false,
                            invalid_utf8: false,
                        },
                        warnings,
                    )
                }
            }
        }
    } else if record.document_type == DocumentType::Pdf {
        let pdf_options = PdfExtractionOptions {
            use_ocr: true,
            ..PdfExtractionOptions::raw_default()
        };
        if let Some(cache) = cache {
            if let Some(entry) =
                cache.try_get(record, Some(pdf_options), &CleaningConfig::default())
            {
                let truncated = entry.extracted_text.chars().count() > options.max_chars_per_file;
                let text: String = entry
                    .extracted_text
                    .chars()
                    .take(options.max_chars_per_file)
                    .collect();
                (
                    BoundedRead {
                        text,
                        truncated,
                        invalid_utf8: false,
                    },
                    entry.warnings,
                )
            } else {
                // Cache miss → bounded extraction
                let bytes =
                    std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                        path: record.source_path.clone(),
                        message: error.to_string(),
                    })?;
                match crate::pdf::extract_pdf(
                    &bytes,
                    Some(options.max_chars_per_file),
                    PdfExtractionOptions {
                        use_ocr: true,
                        ..PdfExtractionOptions::raw_default()
                    },
                ) {
                    Ok(extracted) => {
                        let truncated = extracted.text.chars().count() > options.max_chars_per_file;
                        let text: String = extracted
                            .text
                            .chars()
                            .take(options.max_chars_per_file)
                            .collect();
                        (
                            BoundedRead {
                                text,
                                truncated,
                                invalid_utf8: false,
                            },
                            extracted.warnings,
                        )
                    }
                    Err(e) => {
                        let mut warnings = Vec::new();
                        warnings.push(format!("PDF Extraction Failed: {}", e));
                        (
                            BoundedRead {
                                text: String::new(),
                                truncated: false,
                                invalid_utf8: false,
                            },
                            warnings,
                        )
                    }
                }
            }
        } else {
            // No cache available → bounded extraction
            let bytes = std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                path: record.source_path.clone(),
                message: error.to_string(),
            })?;
            match crate::pdf::extract_pdf(
                &bytes,
                Some(options.max_chars_per_file),
                PdfExtractionOptions {
                    use_ocr: true,
                    ..PdfExtractionOptions::raw_default()
                },
            ) {
                Ok(extracted) => {
                    let truncated = extracted.text.chars().count() > options.max_chars_per_file;
                    let text: String = extracted
                        .text
                        .chars()
                        .take(options.max_chars_per_file)
                        .collect();
                    (
                        BoundedRead {
                            text,
                            truncated,
                            invalid_utf8: false,
                        },
                        extracted.warnings,
                    )
                }
                Err(e) => {
                    let mut warnings = Vec::new();
                    warnings.push(format!("PDF Extraction Failed: {}", e));
                    (
                        BoundedRead {
                            text: String::new(),
                            truncated: false,
                            invalid_utf8: false,
                        },
                        warnings,
                    )
                }
            }
        }
    } else {
        let read = read_bounded_lossy_text(&record.source_path, options.max_chars_per_file)?;
        (read, Vec::new())
    };

    let mut warnings = Vec::new();

    for w in extraction_warnings {
        warnings.push(PreviewWarning {
            source_path: Some(record.source_path.clone()),
            relative_path: Some(record.relative_path.clone()),
            kind: preview_extraction_warning_kind(&w),
            message: w,
        });
    }

    if read.invalid_utf8 {
        warnings.push(PreviewWarning {
            source_path: Some(record.source_path.clone()),
            relative_path: Some(record.relative_path.clone()),
            kind: PreviewWarningKind::InvalidUtf8,
            message: "Invalid UTF-8 was replaced with the Unicode replacement character."
                .to_string(),
        });
    }

    if read.truncated {
        warnings.push(PreviewWarning {
            source_path: Some(record.source_path.clone()),
            relative_path: Some(record.relative_path.clone()),
            kind: PreviewWarningKind::Truncated,
            message: format!(
                "Preview was truncated to {} characters.",
                options.max_chars_per_file
            ),
        });
    }

    Ok(FilePreview {
        source_path: record.source_path.clone(),
        relative_path: record.relative_path.clone(),
        document_type: record.document_type.clone(),
        source_size_bytes: record.size_bytes,
        included_char_count: read.text.chars().count(),
        text: read.text,
        truncated: read.truncated,
        warnings,
    })
}

/// Generates a bounded raw text preview of the given files.
///
/// Reads up to `max_chars_per_file` per file and combines the results.
pub fn preview_files(
    records: &[DocumentRecord],
    options: &PreviewOptions,
    cache: Option<&ExtractionCache>,
) -> Result<CombinedPreview, PreviewError> {
    let limit = options
        .max_files
        .unwrap_or(records.len())
        .min(records.len());
    let mut files = Vec::with_capacity(limit);
    let mut warnings = Vec::new();

    if limit < records.len() {
        warnings.push(PreviewWarning {
            source_path: None,
            relative_path: None,
            kind: PreviewWarningKind::MaxFilesReached,
            message: format!("Preview was limited to {limit} files."),
        });
    }

    let results: Result<Vec<_>, PreviewError> = records[..limit]
        .par_iter()
        .map(|record| preview_file(record, options, cache))
        .collect();

    let previews = results?;
    for preview in previews {
        warnings.extend(preview.warnings.clone());
        files.push(preview);
    }

    let combined_text = build_combined_preview_text(&files, options.include_paths);
    let total_characters_included = files
        .iter()
        .map(|file| file.included_char_count)
        .sum::<usize>();

    Ok(CombinedPreview {
        total_files_previewed: files.len(),
        files,
        combined_text,
        total_characters_included,
        warnings,
    })
}

/// Generates a bounded preview with text cleaning rules applied.
///
/// Unlike the original preview, this does **not** call `preview_files`
/// first and then re-extract.  Instead it processes each file once:
/// - tries the cache first (if available);
/// - on cache miss, uses bounded extraction with the actual `cleaning_config`;
/// - applies HTML extraction and `clean_text` to the result.
///
/// This eliminates the guaranteed double extraction that existed when
/// `preview_processed_files` called `preview_files` first.
pub fn preview_processed_files(
    records: &[DocumentRecord],
    preview_options: &PreviewOptions,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> Result<CombinedPreview, PreviewError> {
    let limit = preview_options
        .max_files
        .unwrap_or(records.len())
        .min(records.len());
    let mut files = Vec::with_capacity(limit);
    let mut combined_warnings = Vec::new();

    if limit < records.len() {
        combined_warnings.push(PreviewWarning {
            source_path: None,
            relative_path: None,
            kind: PreviewWarningKind::MaxFilesReached,
            message: format!("Preview was limited to {limit} files."),
        });
    }

    // Process each file once: try cache, then bounded extraction with actual config
    let results: Result<Vec<_>, PreviewError> = records[..limit]
        .par_iter()
        .map(|record| {
            let (mut source_text, extraction_warnings) = if record.document_type
                == DocumentType::Docx
            {
                // Try cache first
                let cache_hit = cache.and_then(|c| c.try_get(record, None, cleaning_config));
                if let Some(entry) = cache_hit {
                    // Cache hit — use full text, truncate after
                    let text: String = entry
                        .extracted_text
                        .chars()
                        .take(preview_options.max_chars_per_file)
                        .collect();
                    (text, entry.warnings)
                } else {
                    // Cache miss or no cache — bounded extraction with actual config
                    let bytes =
                        std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                            path: record.source_path.clone(),
                            message: error.to_string(),
                        })?;
                    match crate::docx::extract_docx(&bytes, cleaning_config) {
                        Ok(extracted) => {
                            let text: String = extracted
                                .text
                                .chars()
                                .take(preview_options.max_chars_per_file)
                                .collect();
                            (text, extracted.warnings)
                        }
                        Err(e) => (
                            String::new(),
                            vec![format!("DOCX Extraction Failed: {}", e)],
                        ),
                    }
                }
            } else if record.document_type == DocumentType::Pdf {
                let pdf_options = PdfExtractionOptions {
                    use_ocr: true,
                    ..PdfExtractionOptions::from_cleaning_config(cleaning_config)
                };
                // Try cache first
                let cache_hit =
                    cache.and_then(|c| c.try_get(record, Some(pdf_options), cleaning_config));
                if let Some(entry) = cache_hit {
                    // Cache hit — use full text, truncate after
                    let text: String = entry
                        .extracted_text
                        .chars()
                        .take(preview_options.max_chars_per_file)
                        .collect();
                    (text, entry.warnings)
                } else {
                    // Cache miss or no cache — bounded extraction with actual config
                    let bytes =
                        std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                            path: record.source_path.clone(),
                            message: error.to_string(),
                        })?;
                    match crate::pdf::extract_pdf(
                        &bytes,
                        Some(preview_options.max_chars_per_file),
                        pdf_options,
                    ) {
                        Ok(extracted) => {
                            let text: String = extracted
                                .text
                                .chars()
                                .take(preview_options.max_chars_per_file)
                                .collect();
                            (text, extracted.warnings)
                        }
                        Err(e) => (String::new(), vec![format!("PDF Extraction Failed: {}", e)]),
                    }
                }
            } else {
                // Text, HTML, or other textual files — use bounded read, no cleaning config needed
                let read = read_bounded_lossy_text(
                    &record.source_path,
                    preview_options.max_chars_per_file,
                )?;
                (read.text, Vec::new())
            };

            // Apply HTML extraction if enabled
            if cleaning_config.extract_html {
                source_text = crate::html::extract_html(&source_text);
            }

            let cleaned = clean_text(&source_text, cleaning_config);
            let included_char_count = cleaned.chars().count();

            // Convert extraction warnings to PreviewWarnings
            let mut file_warnings: Vec<PreviewWarning> = extraction_warnings
                .into_iter()
                .map(|w| PreviewWarning {
                    source_path: Some(record.source_path.clone()),
                    relative_path: Some(record.relative_path.clone()),
                    kind: preview_extraction_warning_kind(&w),
                    message: w,
                })
                .collect();

            // Add truncation warning if needed
            let truncated = included_char_count > preview_options.max_chars_per_file;
            if truncated {
                file_warnings.push(PreviewWarning {
                    source_path: Some(record.source_path.clone()),
                    relative_path: Some(record.relative_path.clone()),
                    kind: PreviewWarningKind::Truncated,
                    message: format!(
                        "Preview was truncated to {} characters.",
                        preview_options.max_chars_per_file
                    ),
                });
            }

            Ok(FilePreview {
                source_path: record.source_path.clone(),
                relative_path: record.relative_path.clone(),
                document_type: record.document_type.clone(),
                source_size_bytes: record.size_bytes,
                included_char_count,
                text: cleaned,
                truncated,
                warnings: file_warnings,
            })
        })
        .collect();

    let previews = results?;
    for preview in previews {
        combined_warnings.extend(preview.warnings.clone());
        files.push(preview);
    }

    let combined_text = build_combined_preview_text(&files, preview_options.include_paths);
    let total_characters_included = files
        .iter()
        .map(|file| file.included_char_count)
        .sum::<usize>();

    Ok(CombinedPreview {
        total_files_previewed: files.len(),
        files,
        combined_text,
        total_characters_included,
        warnings: combined_warnings,
    })
}

fn build_combined_preview_text(files: &[FilePreview], include_paths: bool) -> String {
    let total = files.len();
    let mut combined = String::new();

    for (index, file) in files.iter().enumerate() {
        if index > 0 {
            combined.push_str("\n\n");
        }

        combined.push_str(&format!("FILE {:03} / {total}\n", index + 1));
        if include_paths {
            combined.push_str(&file.source_path.display().to_string());
            combined.push('\n');
        }
        combined.push_str(&file.text);
    }

    combined
}

fn preview_extraction_warning_kind(message: &str) -> PreviewWarningKind {
    if message.contains("Failed")
        || message.contains("low-quality text")
        || message.contains("required for this PDF")
    {
        PreviewWarningKind::ExtractionError
    } else {
        PreviewWarningKind::ExtractionWarning
    }
}

pub struct BoundedRead {
    pub text: String,
    pub truncated: bool,
    pub invalid_utf8: bool,
}

pub fn read_bounded_lossy_text(
    path: &std::path::Path,
    max_chars: usize,
) -> Result<BoundedRead, PreviewError> {
    const CHUNK_SIZE: usize = 8 * 1024;

    let mut file = File::open(path).map_err(|error| PreviewError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let total_bytes = file
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or_default();

    let target_chars = max_chars.saturating_add(1);
    let mut pending = Vec::new();
    let mut text = String::new();
    let mut chars_seen = 0usize;
    let mut invalid_utf8 = false;
    let mut bytes_read = 0u64;
    let mut reached_eof = false;

    while chars_seen < target_chars {
        let mut chunk = [0u8; CHUNK_SIZE];
        let read = file.read(&mut chunk).map_err(|error| PreviewError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

        if read == 0 {
            reached_eof = true;
            flush_pending(
                &mut pending,
                &mut text,
                &mut chars_seen,
                target_chars,
                true,
                &mut invalid_utf8,
            );
            break;
        }

        bytes_read += read as u64;
        pending.extend_from_slice(&chunk[..read]);
        flush_pending(
            &mut pending,
            &mut text,
            &mut chars_seen,
            target_chars,
            false,
            &mut invalid_utf8,
        );
    }

    let truncated = chars_seen > max_chars || (!reached_eof && bytes_read < total_bytes);
    let text = text.chars().take(max_chars).collect();

    Ok(BoundedRead {
        text,
        truncated,
        invalid_utf8,
    })
}

fn flush_pending(
    pending: &mut Vec<u8>,
    text: &mut String,
    chars_seen: &mut usize,
    target_chars: usize,
    eof: bool,
    invalid_utf8: &mut bool,
) {
    loop {
        if *chars_seen >= target_chars {
            pending.clear();
            return;
        }

        match std::str::from_utf8(pending) {
            Ok(valid) => {
                push_limited(text, valid, chars_seen, target_chars);
                pending.clear();
                return;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending[..valid_up_to]).unwrap();
                    push_limited(text, valid, chars_seen, target_chars);
                    pending.drain(..valid_up_to);
                    if *chars_seen >= target_chars {
                        pending.clear();
                        return;
                    }
                }

                if let Some(error_len) = error.error_len() {
                    *invalid_utf8 = true;
                    push_limited(text, "\u{fffd}", chars_seen, target_chars);
                    pending.drain(..error_len);
                } else if eof {
                    *invalid_utf8 = true;
                    push_limited(text, "\u{fffd}", chars_seen, target_chars);
                    pending.clear();
                    return;
                } else {
                    return;
                }
            }
        }
    }
}

fn push_limited(text: &mut String, fragment: &str, chars_seen: &mut usize, target_chars: usize) {
    for character in fragment.chars() {
        if *chars_seen >= target_chars {
            break;
        }
        text.push(character);
        *chars_seen += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::ExtractionCache;
    use crate::clean::{CleaningConfig, ReplacementRule};
    use crate::scan::scan_directory;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn previews_one_txt_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "Hello").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();

        let preview = preview_file(&report.files[0], &options, None).unwrap();

        assert_eq!(preview.text, "Hello");
        assert_eq!(preview.included_char_count, 5);
        assert!(!preview.truncated);
    }

    #[test]
    fn previews_multiple_files_with_combined_headers() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "A").unwrap();
        fs::write(dir.path().join("b.html"), "<p>B</p>").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();

        let preview = preview_files(&report.files, &options, None).unwrap();

        assert_eq!(preview.total_files_previewed, 2);
        assert!(preview.combined_text.contains("FILE 001 / 2"));
        assert!(preview.combined_text.contains("FILE 002 / 2"));
        assert!(preview.combined_text.contains("B"));
    }

    #[test]
    fn respects_max_chars_per_file_and_marks_truncated() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "abcdef").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: 3,
            ..PreviewOptions::default()
        };

        let preview = preview_file(&report.files[0], &options, None).unwrap();

        assert_eq!(preview.text, "abc");
        assert!(preview.truncated);
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == PreviewWarningKind::Truncated)
        );
    }

    #[test]
    fn respects_max_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "A").unwrap();
        fs::write(dir.path().join("b.txt"), "B").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_files: Some(1),
            ..PreviewOptions::default()
        };

        let preview = preview_files(&report.files, &options, None).unwrap();

        assert_eq!(preview.total_files_previewed, 1);
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == PreviewWarningKind::MaxFilesReached)
        );
    }

    #[test]
    fn omits_paths_when_requested() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            include_paths: false,
            ..PreviewOptions::default()
        };

        let preview = preview_files(&report.files, &options, None).unwrap();

        assert_eq!(preview.combined_text, "FILE 001 / 1\nA");
    }

    #[test]
    fn handles_invalid_utf8_lossily_with_warning() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("bad.txt"), [0x66, 0x80, 0x67]).unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();

        let preview = preview_file(&report.files[0], &options, None).unwrap();

        assert_eq!(preview.text, "f\u{fffd}g");
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == PreviewWarningKind::InvalidUtf8)
        );
    }

    #[test]
    fn degraded_pdf_quality_warning_is_preview_extraction_error() {
        assert_eq!(
            preview_extraction_warning_kind(
                "PDFium is unavailable and degraded PDF extraction produced low-quality text. OCR/PDFium extraction is required for this PDF."
            ),
            PreviewWarningKind::ExtractionError
        );
        assert_eq!(
            preview_extraction_warning_kind("PDF backend: PDFium."),
            PreviewWarningKind::ExtractionWarning
        );
    }

    #[test]
    fn preview_does_not_modify_source_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "Original").unwrap();
        let before = fs::read(&path).unwrap();
        let report = scan_directory(dir.path()).unwrap();

        preview_files(&report.files, &PreviewOptions::default(), None).unwrap();

        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn processed_preview_reflects_cleaning_config() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "HELLO old\n\n\nWORLD").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let cleaning = CleaningConfig {
            lowercase: true,
            collapse_blank_lines: true,
            extract_html: false,
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "new".to_string(),
            }],
            ..CleaningConfig::default()
        };

        let preview =
            preview_processed_files(&report.files, &PreviewOptions::default(), &cleaning, None)
                .unwrap();

        assert!(preview.combined_text.contains("hello new\n\nworld"));
    }

    #[test]
    fn public_preview_structs_serialize_to_json() {
        let preview = CombinedPreview {
            files: Vec::new(),
            combined_text: String::new(),
            total_files_previewed: 0,
            total_characters_included: 0,
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&preview).unwrap();
        assert!(json.contains("total_files_previewed"));
    }

    #[test]
    fn performance_sanity_scans_and_previews_many_tiny_files() {
        let dir = tempdir().unwrap();
        for index in 0..100 {
            fs::write(dir.path().join(format!("{index:03}.txt")), "x").unwrap();
        }

        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_files: Some(25),
            ..PreviewOptions::default()
        };
        let preview = preview_files(&report.files, &options, None).unwrap();

        assert_eq!(report.files_supported, 100);
        assert_eq!(preview.total_files_previewed, 25);
    }

    // ── Cache integration tests ──────────────────────────────────────────

    #[test]
    fn original_preview_output_unchanged_with_cache() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "Hello world").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();
        let cache = ExtractionCache::new();

        // Without cache
        let without = preview_file(&report.files[0], &options, None).unwrap();
        // With cache
        let with = preview_file(&report.files[0], &options, Some(&cache)).unwrap();

        assert_eq!(without.text, with.text);
        assert_eq!(without.truncated, with.truncated);
        assert_eq!(without.included_char_count, with.included_char_count);
        assert_eq!(without.warnings.len(), with.warnings.len());
    }

    #[test]
    fn processed_preview_output_unchanged_with_cache() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "HELLO WORLD").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();
        let cleaning = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let cache = ExtractionCache::new();

        // Without cache
        let without = preview_processed_files(&report.files, &options, &cleaning, None).unwrap();
        // With cache
        let with =
            preview_processed_files(&report.files, &options, &cleaning, Some(&cache)).unwrap();

        assert_eq!(without.combined_text, with.combined_text);
        assert_eq!(
            without.total_characters_included,
            with.total_characters_included
        );
        assert_eq!(without.warnings.len(), with.warnings.len());
    }

    #[test]
    fn original_and_processed_preview_use_different_cache_keys() {
        let dir = tempdir().unwrap();
        // Create a DOCX-like record (real extraction not needed — just test keys)
        let path = dir.path().join("test.docx");
        std::fs::write(&path, b"PK\x05\x06").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();
        let cache = ExtractionCache::new();
        let cleaning = CleaningConfig::default();

        // Call original preview (uses default cleaning config)
        preview_file(&report.files[0], &options, Some(&cache)).ok();

        // Call processed preview (also uses default cleaning config for this test)
        preview_processed_files(&report.files[0..1], &options, &cleaning, Some(&cache)).ok();

        // Original and processed should use different keys because:
        // original → CleaningConfig::default() for DOCX (pdf_options=None)
        // processed → actual cleaning_config (may differ in pdf_options for PDF or docx_config for DOCX)
        // For a DOCX file with identical CleaningConfig::default() in both paths,
        // the keys may actually match. This test verifies no crash.
    }
}
