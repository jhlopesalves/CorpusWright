use crate::cache::ExtractionCache;
use crate::clean::CleaningConfig;
use crate::pdf::{PdfExtractionOptions, clean_extracted_pdf_text};
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
    pub max_chars_per_file: Option<usize>,
    pub include_paths: bool,
    pub max_files: Option<usize>,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            max_chars_per_file: Some(5_000),
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
    pub cleaning_report: Option<crate::clean::CleaningReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CombinedPreview {
    pub files: Vec<FilePreview>,
    pub combined_text: String,
    pub total_files_previewed: usize,
    pub total_characters_included: usize,
    pub warnings: Vec<PreviewWarning>,
    pub cleaning_report: Option<crate::clean::CleaningReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PreviewError {
    Io { path: PathBuf, message: String },
}

/// Generates a raw-text preview for a single file.
///
/// If a `cache` is provided and contains a matching entry, the cached
/// extracted text is used and the configured character limit is applied. On a
/// cache miss, falls back to extraction with the same optional limit.
///
/// Only PDF and DOCX files attempt a cache lookup.  Text/HTML files are
/// always read via `read_bounded_lossy_text`.
pub fn preview_file(
    record: &DocumentRecord,
    options: &PreviewOptions,
    cache: Option<&ExtractionCache>,
) -> Result<FilePreview, PreviewError> {
    preview_file_with_config(record, options, &CleaningConfig::default(), cache)
}

pub fn preview_file_with_config(
    record: &DocumentRecord,
    options: &PreviewOptions,
    cleaning_config: &CleaningConfig,
    cache: Option<&ExtractionCache>,
) -> Result<FilePreview, PreviewError> {
    let (read, extraction_warnings) = if record.document_type == DocumentType::Docx {
        if let Some(cache) = cache {
            if let Some(entry) = cache.try_get(record, None, &CleaningConfig::default()) {
                (
                    limit_text(entry.extracted_text, options.max_chars_per_file, false),
                    entry.warnings,
                )
            } else {
                let bytes =
                    std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                        path: record.source_path.clone(),
                        message: error.to_string(),
                    })?;
                match crate::docx::extract_docx(&bytes, &CleaningConfig::default()) {
                    Ok(extracted) => (
                        limit_text(extracted.text, options.max_chars_per_file, false),
                        extracted.warnings,
                    ),
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
            let bytes = std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                path: record.source_path.clone(),
                message: error.to_string(),
            })?;
            match crate::docx::extract_docx(&bytes, &CleaningConfig::default()) {
                Ok(extracted) => (
                    limit_text(extracted.text, options.max_chars_per_file, false),
                    extracted.warnings,
                ),
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
        let pdf_options =
            PdfExtractionOptions::raw_from_record_and_cleaning_config(record, cleaning_config);
        if let Some(cache) = cache {
            if let Some(entry) = cache.try_get(record, Some(pdf_options), cleaning_config) {
                (
                    limit_text(entry.extracted_text, options.max_chars_per_file, false),
                    entry.warnings,
                )
            } else {
                let bytes =
                    std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                        path: record.source_path.clone(),
                        message: error.to_string(),
                    })?;
                match crate::pdf::extract_pdf(&bytes, options.max_chars_per_file, pdf_options) {
                    Ok(extracted) => {
                        let read = limit_text(
                            extracted.text,
                            options.max_chars_per_file,
                            extracted.truncated,
                        );
                        (read, extracted.warnings)
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
            let bytes = std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                path: record.source_path.clone(),
                message: error.to_string(),
            })?;
            match crate::pdf::extract_pdf(&bytes, options.max_chars_per_file, pdf_options) {
                Ok(extracted) => {
                    let read = limit_text(
                        extracted.text,
                        options.max_chars_per_file,
                        extracted.truncated,
                    );
                    (read, extracted.warnings)
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

    if read.truncated
        && let Some(max_chars_per_file) = options.max_chars_per_file
    {
        warnings.push(PreviewWarning {
            source_path: Some(record.source_path.clone()),
            relative_path: Some(record.relative_path.clone()),
            kind: PreviewWarningKind::Truncated,
            message: format!(
                "Preview was truncated to {} characters.",
                max_chars_per_file
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
        cleaning_report: None,
    })
}

/// Generates a raw text preview of the given files.
///
/// Applies the optional per-file character limit and combines the results.
pub fn preview_files(
    records: &[DocumentRecord],
    options: &PreviewOptions,
    cache: Option<&ExtractionCache>,
) -> Result<CombinedPreview, PreviewError> {
    preview_files_with_config(records, options, &CleaningConfig::default(), cache)
}

pub fn preview_files_with_config(
    records: &[DocumentRecord],
    options: &PreviewOptions,
    cleaning_config: &CleaningConfig,
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
        .map(|record| preview_file_with_config(record, options, cleaning_config, cache))
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
        cleaning_report: None,
    })
}

/// Generates a preview with text cleaning rules applied.
///
/// Each file is extracted once with compatible cache entries used where
/// available, then optional HTML extraction and `clean_text` are applied.
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

    let results: Result<Vec<_>, PreviewError> = records[..limit]
        .par_iter()
        .map(|record| {
            let (mut source_read, extraction_warnings) = if record.document_type
                == DocumentType::Docx
            {
                let cache_hit = cache.and_then(|c| c.try_get(record, None, cleaning_config));
                if let Some(entry) = cache_hit {
                    (
                        limit_text(
                            entry.extracted_text,
                            preview_options.max_chars_per_file,
                            false,
                        ),
                        entry.warnings,
                    )
                } else {
                    let bytes =
                        std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                            path: record.source_path.clone(),
                            message: error.to_string(),
                        })?;
                    match crate::docx::extract_docx(&bytes, cleaning_config) {
                        Ok(extracted) => (
                            limit_text(extracted.text, preview_options.max_chars_per_file, false),
                            extracted.warnings,
                        ),
                        Err(e) => (
                            empty_bounded_read(),
                            vec![format!("DOCX Extraction Failed: {}", e)],
                        ),
                    }
                }
            } else if record.document_type == DocumentType::Pdf {
                let pdf_options = PdfExtractionOptions::raw_from_record_and_cleaning_config(
                    record,
                    cleaning_config,
                );
                let cache_hit =
                    cache.and_then(|c| c.try_get(record, Some(pdf_options), cleaning_config));
                if let Some(entry) = cache_hit {
                    let (pdf_cleaned, mut cleanup_warnings) =
                        clean_extracted_pdf_text(&entry.extracted_text, cleaning_config);
                    let mut warnings = entry.warnings;
                    warnings.append(&mut cleanup_warnings);
                    (
                        limit_text(pdf_cleaned, preview_options.max_chars_per_file, false),
                        warnings,
                    )
                } else {
                    let bytes =
                        std::fs::read(&record.source_path).map_err(|error| PreviewError::Io {
                            path: record.source_path.clone(),
                            message: error.to_string(),
                        })?;
                    match crate::pdf::extract_pdf(
                        &bytes,
                        preview_options.max_chars_per_file,
                        pdf_options,
                    ) {
                        Ok(extracted) => {
                            let extraction_truncated = extracted.truncated;
                            let (pdf_cleaned, mut cleanup_warnings) =
                                clean_extracted_pdf_text(&extracted.text, cleaning_config);
                            let mut warnings = extracted.warnings;
                            warnings.append(&mut cleanup_warnings);
                            (
                                limit_text(
                                    pdf_cleaned,
                                    preview_options.max_chars_per_file,
                                    extraction_truncated,
                                ),
                                warnings,
                            )
                        }
                        Err(e) => (
                            empty_bounded_read(),
                            vec![format!("PDF Extraction Failed: {}", e)],
                        ),
                    }
                }
            } else {
                let read = read_bounded_lossy_text(
                    &record.source_path,
                    preview_options.max_chars_per_file,
                )?;
                (read, Vec::new())
            };

            if cleaning_config.extract_html {
                source_read.text = crate::html::extract_html(&source_read.text);
            }

            let (cleaned, cleaning_report) = {
                let context = crate::clean::CleaningContext {
                    source_id: Some(record.relative_path.to_str().unwrap_or("")),
                    source_label: Some(record.relative_path.to_str().unwrap_or("")),
                    page_number: None,
                };
                let (text, report) = crate::clean::clean_text_with_report(
                    &source_read.text,
                    cleaning_config,
                    Some(&context),
                );
                (text, Some(report))
            };

            let included_char_count = cleaned.chars().count();

            let mut file_warnings: Vec<PreviewWarning> = extraction_warnings
                .into_iter()
                .map(|w| PreviewWarning {
                    source_path: Some(record.source_path.clone()),
                    relative_path: Some(record.relative_path.clone()),
                    kind: preview_extraction_warning_kind(&w),
                    message: w,
                })
                .collect();

            let truncated = source_read.truncated;
            if truncated && let Some(max_chars_per_file) = preview_options.max_chars_per_file {
                file_warnings.push(PreviewWarning {
                    source_path: Some(record.source_path.clone()),
                    relative_path: Some(record.relative_path.clone()),
                    kind: PreviewWarningKind::Truncated,
                    message: format!(
                        "Preview was truncated to {} characters.",
                        max_chars_per_file
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
                cleaning_report,
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
    let cleaning_report = crate::clean::CleaningReport::merge(
        files
            .iter()
            .filter_map(|file| file.cleaning_report.as_ref()),
    );

    Ok(CombinedPreview {
        total_files_previewed: files.len(),
        files,
        combined_text,
        total_characters_included,
        warnings: combined_warnings,
        cleaning_report: Some(cleaning_report),
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

fn empty_bounded_read() -> BoundedRead {
    BoundedRead {
        text: String::new(),
        truncated: false,
        invalid_utf8: false,
    }
}

fn limit_text(text: String, max_chars: Option<usize>, extraction_truncated: bool) -> BoundedRead {
    let Some(max_chars) = max_chars else {
        return BoundedRead {
            text,
            truncated: false,
            invalid_utf8: false,
        };
    };

    let text_exceeds_limit = text.chars().count() > max_chars;
    let text = if text_exceeds_limit {
        text.chars().take(max_chars).collect()
    } else {
        text
    };

    BoundedRead {
        text,
        truncated: extraction_truncated || text_exceeds_limit,
        invalid_utf8: false,
    }
}

pub fn read_bounded_lossy_text(
    path: &std::path::Path,
    max_chars: Option<usize>,
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

    let target_chars = max_chars
        .map(|limit| limit.saturating_add(1))
        .unwrap_or(usize::MAX);
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

    let truncated = max_chars
        .is_some_and(|limit| chars_seen > limit || (!reached_eof && bytes_read < total_bytes));
    let text = match max_chars {
        Some(limit) => text.chars().take(limit).collect(),
        None => text,
    };

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
    use crate::clean::{
        CleaningConfig, CleaningOperation, RemovalMatcher, RemovalRule, RemovalRuleSource,
        RemovalScope, ReplacementRule,
    };
    use crate::scan::scan_directory;
    use std::fs;
    use tempfile::tempdir;

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
    fn exhaustive_raw_preview_returns_the_complete_file_deterministically() {
        let dir = tempdir().unwrap();
        let full_text = format!("{}TAIL_AFTER_LIMIT", "a".repeat(6_000));
        fs::write(dir.path().join("long.txt"), &full_text).unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: None,
            ..PreviewOptions::default()
        };

        let first = preview_file(&report.files[0], &options, None).unwrap();
        let second = preview_file(&report.files[0], &options, None).unwrap();

        assert_eq!(first.text, full_text);
        assert_eq!(first.included_char_count, full_text.chars().count());
        assert!(!first.truncated);
        assert!(
            first
                .warnings
                .iter()
                .all(|warning| warning.kind != PreviewWarningKind::Truncated)
        );
        assert_eq!(first, second);
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
            max_chars_per_file: Some(3),
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
    fn exact_character_limit_is_not_marked_truncated() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "abc").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: Some(3),
            ..PreviewOptions::default()
        };

        let preview = preview_file(&report.files[0], &options, None).unwrap();

        assert_eq!(preview.text, "abc");
        assert!(!preview.truncated);
        assert!(
            preview
                .warnings
                .iter()
                .all(|warning| warning.kind != PreviewWarningKind::Truncated)
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
    fn multi_file_raw_and_processed_previews_remain_bounded() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a".repeat(6_000)).unwrap();
        fs::write(dir.path().join("b.txt"), "b".repeat(6_000)).unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: Some(5_000),
            max_files: Some(1),
            ..PreviewOptions::default()
        };

        let raw = preview_files(&report.files, &options, None).unwrap();
        let processed =
            preview_processed_files(&report.files, &options, &CleaningConfig::default(), None)
                .unwrap();

        for preview in [&raw, &processed] {
            assert_eq!(preview.files.len(), 1);
            assert_eq!(preview.files[0].included_char_count, 5_000);
            assert!(preview.files[0].truncated);
            assert!(
                preview
                    .warnings
                    .iter()
                    .any(|warning| warning.kind == PreviewWarningKind::Truncated)
            );
            assert!(
                preview
                    .warnings
                    .iter()
                    .any(|warning| warning.kind == PreviewWarningKind::MaxFilesReached)
            );
        }
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
    fn exhaustive_processed_preview_cleans_text_after_the_default_limit() {
        let dir = tempdir().unwrap();
        let source_text = format!(
            "{}\nREMOVE_AFTER_LIMIT\nTAIL_AFTER_LIMIT",
            "a".repeat(6_000)
        );
        fs::write(dir.path().join("long.txt"), source_text).unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: None,
            ..PreviewOptions::default()
        };
        let cleaning = CleaningConfig {
            removal_rules: vec![anywhere_literal_rule("REMOVE_AFTER_LIMIT")],
            ..CleaningConfig::default()
        };

        let preview = preview_processed_files(&report.files, &options, &cleaning, None).unwrap();
        let file = &preview.files[0];

        assert!(!file.text.contains("REMOVE_AFTER_LIMIT"));
        assert!(file.text.contains("TAIL_AFTER_LIMIT"));
        assert!(!file.truncated);
        assert!(
            file.warnings
                .iter()
                .all(|warning| warning.kind != PreviewWarningKind::Truncated)
        );
        assert_eq!(
            file.cleaning_report.as_ref().unwrap().summaries[0].occurrence_count,
            1
        );
    }

    #[test]
    fn exhaustive_pdf_preview_is_not_limited_to_five_thousand_characters() {
        let dir = tempdir().unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("pdfs")
            .join("bert.pdf");
        fs::copy(fixture, dir.path().join("bert.pdf")).unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions {
            max_chars_per_file: None,
            ..PreviewOptions::default()
        };

        let preview = preview_file(&report.files[0], &options, None).unwrap();

        assert!(preview.included_char_count > 5_000);
        assert_eq!(preview.included_char_count, preview.text.chars().count());
        assert!(!preview.truncated);
        assert!(
            preview
                .warnings
                .iter()
                .all(|warning| warning.kind != PreviewWarningKind::Truncated)
        );

        let (pdf_cleaned, _) = clean_extracted_pdf_text(&preview.text, &CleaningConfig::default());
        let late_marker = pdf_cleaned
            .chars()
            .skip(5_000)
            .collect::<String>()
            .split_whitespace()
            .find(|word| word.chars().count() >= 10)
            .unwrap()
            .to_string();
        let processed = preview_processed_files(
            &report.files,
            &options,
            &CleaningConfig {
                removal_rules: vec![anywhere_literal_rule(&late_marker)],
                ..CleaningConfig::default()
            },
            None,
        )
        .unwrap();
        assert!(processed.files[0].included_char_count > 5_000);
        assert!(!processed.files[0].text.contains(&late_marker));
        assert!(!processed.files[0].truncated);

        let bounded = preview_file(
            &report.files[0],
            &PreviewOptions {
                max_chars_per_file: Some(5_000),
                ..PreviewOptions::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(bounded.included_char_count, 5_000);
        assert!(bounded.truncated);
        assert!(
            bounded
                .warnings
                .iter()
                .any(|warning| warning.kind == PreviewWarningKind::Truncated)
        );
    }

    #[test]
    fn processed_preview_applies_inline_repeated_artifact_rule() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "foo<br />bar").unwrap();
        fs::write(dir.path().join("b.txt"), "left<br />right<br />end").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let cleaning = CleaningConfig {
            normalize_whitespace_line_endings: true,
            removal_rules: vec![anywhere_literal_rule("<br />")],
            ..CleaningConfig::default()
        };

        let preview =
            preview_processed_files(&report.files, &PreviewOptions::default(), &cleaning, None)
                .unwrap();

        assert!(preview.combined_text.contains("foo bar"));
        assert!(preview.combined_text.contains("left right end"));
        assert!(!preview.combined_text.contains("<br />"));
        let file_summary = &preview.files[0].cleaning_report.as_ref().unwrap().summaries[0];
        assert_eq!(file_summary.rule_id.as_deref(), Some("rule-1"));
        assert_eq!(file_summary.occurrence_count, 1);
        let combined_report = preview.cleaning_report.as_ref().unwrap();
        let inline_summary = combined_report
            .summaries
            .iter()
            .find(|summary| summary.rule_id.as_deref() == Some("rule-1"))
            .unwrap();
        assert_eq!(inline_summary.occurrence_count, 3);
        assert_eq!(inline_summary.examples.len(), 3);
    }

    #[test]
    fn combined_preview_keeps_normalisation_summaries_per_source() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "\u{FB01}\u{00AD}").unwrap();
        fs::write(dir.path().join("b.txt"), "\u{FB01}\u{FB02}\u{00AD}\u{200B}").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let cleaning = CleaningConfig {
            expand_ligatures: true,
            remove_invisible_controls: true,
            ..CleaningConfig::default()
        };

        let preview =
            preview_processed_files(&report.files, &PreviewOptions::default(), &cleaning, None)
                .unwrap();
        let combined_report = preview.cleaning_report.as_ref().unwrap();
        let normalisation_summaries = combined_report
            .summaries
            .iter()
            .filter(|summary| summary.operation == CleaningOperation::Normalization)
            .collect::<Vec<_>>();

        assert_eq!(normalisation_summaries.len(), 4);
        for reason in ["ligature_expansion", "invisible_control_removal"] {
            let summaries = normalisation_summaries
                .iter()
                .filter(|summary| {
                    summary.reason_codes.len() == 1 && summary.reason_codes[0].as_str() == reason
                })
                .collect::<Vec<_>>();
            assert_eq!(summaries.len(), 2);
            assert!(summaries.iter().all(|summary| summary.source_id.is_some()));
            assert_eq!(
                summaries
                    .iter()
                    .map(|summary| summary.occurrence_count)
                    .sum::<usize>(),
                3
            );
            assert_eq!(
                combined_report
                    .totals_by_reason
                    .iter()
                    .find(|count| count.reason == reason)
                    .unwrap()
                    .count,
                3
            );
        }
    }

    #[test]
    fn public_preview_structs_serialize_to_json() {
        let preview = CombinedPreview {
            files: Vec::new(),
            combined_text: String::new(),
            total_files_previewed: 0,
            total_characters_included: 0,
            warnings: Vec::new(),
            cleaning_report: None,
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

    #[test]
    fn original_preview_output_unchanged_with_cache() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "Hello world").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();
        let cache = ExtractionCache::new();

        let without = preview_file(&report.files[0], &options, None).unwrap();
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

        let without = preview_processed_files(&report.files, &options, &cleaning, None).unwrap();
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
        // This smoke test covers both preview paths sharing a cache without valid DOCX content.
        let path = dir.path().join("test.docx");
        std::fs::write(&path, b"PK\x05\x06").unwrap();
        let report = scan_directory(dir.path()).unwrap();
        let options = PreviewOptions::default();
        let cache = ExtractionCache::new();
        let cleaning = CleaningConfig::default();

        preview_file(&report.files[0], &options, Some(&cache)).ok();

        preview_processed_files(&report.files[0..1], &options, &cleaning, Some(&cache)).ok();
    }
}
