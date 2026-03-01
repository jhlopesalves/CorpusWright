use crate::cache::ExtractionCache;
use crate::clean::{CleaningConfig, PdfEmbeddedTextStrategy, clean_text};
use crate::manifest::{ExportManifest, ManifestFileRecord};
use crate::scan::{DocumentRecord, DocumentType};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub use crate::manifest::ManifestFileRecord as ExportedFileRecord;

/// Configuration options for exporting a corpus.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportOptions {
    pub app_name: String,
    pub app_version: Option<String>,
    pub overwrite: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            app_name: "CorpusAid".to_string(),
            app_version: None,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportWarningKind {
    InvalidUtf8,
    ExtractionWarning,
    ExtractionError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportWarning {
    pub source_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub kind: ExportWarningKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportReport {
    pub output_dir: PathBuf,
    pub texts_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub warnings_path: PathBuf,
    pub config_path: PathBuf,
    pub readme_path: PathBuf,
    pub files_exported: usize,
    pub warnings_count: usize,
    pub exported_files: Vec<ExportedFileRecord>,
    pub warnings: Vec<ExportWarning>,
    pub manifest: ExportManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExportError {
    ExistingOutputDirectory { path: PathBuf },
    OutputPathIsNotDirectory { path: PathBuf },
    UnsafeOutputDirectory { path: PathBuf, message: String },
    Io { path: PathBuf, message: String },
    Json { path: PathBuf, message: String },
}

/// Safely exports the corpus to the specified output directory.
///
/// This applies cleaning rules and writes the processed texts as well as
/// a manifest, configuration log, and warnings log.
type ExportProgressCallback<'a> = &'a (dyn Fn(usize, &str) + Sync);

pub fn export_corpus(
    records: &[DocumentRecord],
    output_dir: impl AsRef<Path>,
    cleaning_config: &CleaningConfig,
    options: &ExportOptions,
    progress_callback: Option<ExportProgressCallback<'_>>,
    cache: Option<&ExtractionCache>,
) -> Result<ExportReport, ExportError> {
    let output_dir = output_dir.as_ref();
    validate_output_path(records, output_dir)?;

    if output_dir.exists() {
        if !output_dir.is_dir() {
            return Err(ExportError::OutputPathIsNotDirectory {
                path: output_dir.to_path_buf(),
            });
        }

        if !options.overwrite {
            return Err(ExportError::ExistingOutputDirectory {
                path: output_dir.to_path_buf(),
            });
        }

        fs::remove_dir_all(output_dir).map_err(|error| ExportError::Io {
            path: output_dir.to_path_buf(),
            message: error.to_string(),
        })?;
    }

    let texts_dir = output_dir.join("texts");
    fs::create_dir_all(&texts_dir).map_err(|error| ExportError::Io {
        path: texts_dir.clone(),
        message: error.to_string(),
    })?;

    // Pre-compute relative output paths, tracking collisions across all records
    let mut used_paths = HashSet::new();
    let mut relative_output_paths = Vec::with_capacity(records.len());
    for record in records.iter() {
        relative_output_paths.push(unique_output_relative_path(record, &mut used_paths));
    }

    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let processed_count = AtomicUsize::new(0);

    let results: Result<Vec<(ManifestFileRecord, Vec<ExportWarning>)>, ExportError> = records
        .par_iter()
        .zip(relative_output_paths.par_iter())
        .map(|(record, relative_path)| {
            let source_bytes = fs::read(&record.source_path).map_err(|error| ExportError::Io {
                path: record.source_path.clone(),
                message: error.to_string(),
            })?;
            let source_hash_sha256 = sha256_hex(&source_bytes);
            let invalid_utf8 = std::str::from_utf8(&source_bytes).is_err();
            let raw_text = String::from_utf8_lossy(&source_bytes).into_owned();

            let mut file_warnings = Vec::new();
            let mut warnings = Vec::new();
            let mut page_count = None;

            let extraction_method = match record.document_type {
                DocumentType::Text => Some("plain_text".to_string()),
                DocumentType::Html => Some("html2text".to_string()),
                DocumentType::Docx => Some("docx_zip_wordprocessingml".to_string()),
                DocumentType::Pdf => match cleaning_config.pdf_embedded_text_strategy {
                    PdfEmbeddedTextStrategy::PdfiumFlat => Some("pdfium_flat".to_string()),
                    PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn => {
                        Some("pdfium_visual_single_column".to_string())
                    }
                    PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => {
                        Some("pdfium_visual_columns_experimental".to_string())
                    }
                },
            };

            let manifest_output_path = PathBuf::from("texts").join(relative_path);

            let original_text = if record.document_type == DocumentType::Docx {
                if let Some(cache) = cache {
                    match cache.get_or_extract(record, None, cleaning_config) {
                        Ok(entry) => {
                            for w in &entry.warnings {
                                file_warnings.push(w.clone());
                                warnings.push(ExportWarning {
                                    source_path: Some(record.source_path.clone()),
                                    output_path: Some(manifest_output_path.clone()),
                                    kind: ExportWarningKind::ExtractionWarning,
                                    message: w.clone(),
                                });
                            }
                            entry.extracted_text
                        }
                        Err(e) => {
                            let msg = format!("DOCX Extraction Failed: {}", e);
                            file_warnings.push(msg.clone());
                            warnings.push(ExportWarning {
                                source_path: Some(record.source_path.clone()),
                                output_path: Some(manifest_output_path.clone()),
                                kind: ExportWarningKind::ExtractionError,
                                message: msg,
                            });
                            String::new()
                        }
                    }
                } else {
                    // No cache — direct extraction
                    match crate::docx::extract_docx(&source_bytes, cleaning_config) {
                        Ok(extracted) => {
                            for w in extracted.warnings {
                                file_warnings.push(w.clone());
                                warnings.push(ExportWarning {
                                    source_path: Some(record.source_path.clone()),
                                    output_path: Some(manifest_output_path.clone()),
                                    kind: ExportWarningKind::ExtractionWarning,
                                    message: w,
                                });
                            }
                            extracted.text
                        }
                        Err(e) => {
                            let msg = format!("DOCX Extraction Failed: {}", e);
                            file_warnings.push(msg.clone());
                            warnings.push(ExportWarning {
                                source_path: Some(record.source_path.clone()),
                                output_path: Some(manifest_output_path.clone()),
                                kind: ExportWarningKind::ExtractionError,
                                message: msg,
                            });
                            String::new()
                        }
                    }
                }
            } else if record.document_type == DocumentType::Pdf {
                let pdf_options = crate::pdf::PdfExtractionOptions {
                    use_ocr: true,
                    ..crate::pdf::PdfExtractionOptions::from_cleaning_config(cleaning_config)
                };
                if let Some(cache) = cache {
                    match cache.get_or_extract(record, Some(pdf_options), cleaning_config) {
                        Ok(entry) => {
                            for w in &entry.warnings {
                                file_warnings.push(w.clone());
                                warnings.push(ExportWarning {
                                    source_path: Some(record.source_path.clone()),
                                    output_path: Some(manifest_output_path.clone()),
                                    kind: ExportWarningKind::ExtractionWarning,
                                    message: w.clone(),
                                });
                            }
                            page_count = entry.page_count;
                            entry.extracted_text
                        }
                        Err(e) => {
                            let msg = format!("PDF Extraction Failed: {}", e);
                            file_warnings.push(msg.clone());
                            warnings.push(ExportWarning {
                                source_path: Some(record.source_path.clone()),
                                output_path: Some(manifest_output_path.clone()),
                                kind: ExportWarningKind::ExtractionError,
                                message: msg,
                            });
                            String::new()
                        }
                    }
                } else {
                    // No cache — direct extraction
                    match crate::pdf::extract_pdf(&source_bytes, None, pdf_options) {
                        Ok(extracted) => {
                            for w in extracted.warnings {
                                file_warnings.push(w.clone());
                                warnings.push(ExportWarning {
                                    source_path: Some(record.source_path.clone()),
                                    output_path: Some(manifest_output_path.clone()),
                                    kind: ExportWarningKind::ExtractionWarning,
                                    message: w,
                                });
                            }
                            page_count = Some(extracted.page_count);
                            extracted.text
                        }
                        Err(e) => {
                            let msg = format!("PDF Extraction Failed: {}", e);
                            file_warnings.push(msg.clone());
                            warnings.push(ExportWarning {
                                source_path: Some(record.source_path.clone()),
                                output_path: Some(manifest_output_path.clone()),
                                kind: ExportWarningKind::ExtractionError,
                                message: msg,
                            });
                            String::new()
                        }
                    }
                }
            } else {
                raw_text
            };

            let processed_text = if cleaning_config.extract_html {
                clean_text(&crate::html::extract_html(&original_text), cleaning_config)
            } else {
                clean_text(&original_text, cleaning_config)
            };
            let processed_bytes = processed_text.as_bytes();
            let processed_hash_sha256 = sha256_hex(processed_bytes);

            if invalid_utf8
                && record.document_type != DocumentType::Docx
                && record.document_type != DocumentType::Pdf
            {
                let message = "Invalid UTF-8 was replaced with the Unicode replacement character."
                    .to_string();
                file_warnings.push(message.clone());
                warnings.push(ExportWarning {
                    source_path: Some(record.source_path.clone()),
                    output_path: Some(manifest_output_path.clone()),
                    kind: ExportWarningKind::InvalidUtf8,
                    message,
                });
            }

            // Create parent directory if needed
            let output_path = texts_dir.join(relative_path);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| ExportError::Io {
                    path: parent.to_path_buf(),
                    message: error.to_string(),
                })?;
            }

            fs::write(&output_path, processed_bytes).map_err(|error| ExportError::Io {
                path: output_path.clone(),
                message: error.to_string(),
            })?;

            let manifest_record = ManifestFileRecord {
                source_path: record.source_path.clone(),
                relative_path: record.relative_path.clone(),
                document_type: record.document_type.clone(),
                output_path: manifest_output_path,
                source_size_bytes: record.size_bytes,
                original_char_count: original_text.chars().count(),
                processed_char_count: processed_text.chars().count(),
                source_hash_sha256,
                processed_hash_sha256,
                warnings: file_warnings,
                extraction_method,
                page_count,
            };

            // Emit progress if callback is provided
            let current_count = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(cb) = &progress_callback {
                let display_name = record
                    .relative_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                cb(current_count, display_name);
            }

            Ok((manifest_record, warnings))
        })
        .collect();

    let results = results?;
    let mut exported_files = Vec::with_capacity(records.len());
    let mut warnings = Vec::new();
    for (manifest_record, mut file_warnings) in results {
        exported_files.push(manifest_record);
        warnings.append(&mut file_warnings);
    }

    let manifest = ExportManifest {
        app_name: options.app_name.clone(),
        app_version: options.app_version.clone(),
        export_timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        files_exported: exported_files.len(),
        warnings_count: warnings.len(),
        cleaning_config: cleaning_config.clone(),
        files: exported_files.clone(),
    };

    let manifest_path = output_dir.join("manifest.json");
    write_json(&manifest_path, &manifest)?;

    let warnings_path = output_dir.join("warnings.json");
    write_json(&warnings_path, &warnings)?;

    let config_path = output_dir.join("config.json");
    write_json(&config_path, cleaning_config)?;

    let readme_path = output_dir.join("README.txt");
    fs::write(&readme_path, readme_text(&options.app_name)).map_err(|error| ExportError::Io {
        path: readme_path.clone(),
        message: error.to_string(),
    })?;

    Ok(ExportReport {
        output_dir: output_dir.to_path_buf(),
        texts_dir,
        manifest_path,
        warnings_path,
        config_path,
        readme_path,
        files_exported: manifest.files_exported,
        warnings_count: manifest.warnings_count,
        exported_files,
        warnings,
        manifest,
    })
}

fn validate_output_path(records: &[DocumentRecord], output_dir: &Path) -> Result<(), ExportError> {
    let output_path = normalized_absolute_path(output_dir)?;

    for source_root in infer_source_roots(records) {
        let source_root = normalized_absolute_path(&source_root)?;
        if output_path == source_root {
            return Err(ExportError::UnsafeOutputDirectory {
                path: output_dir.to_path_buf(),
                message: "Output directory must not be the same as the source root.".to_string(),
            });
        }

        if source_root.starts_with(&output_path) {
            return Err(ExportError::UnsafeOutputDirectory {
                path: output_dir.to_path_buf(),
                message: "Output directory must not contain a source root.".to_string(),
            });
        }
    }

    Ok(())
}

fn normalized_absolute_path(path: &Path) -> Result<PathBuf, ExportError> {
    if path.exists() {
        return path.canonicalize().map_err(|error| ExportError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        });
    }

    let mut parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.as_os_str().is_empty() {
        parent = Path::new(".");
    }

    let file_name = path
        .file_name()
        .ok_or_else(|| ExportError::UnsafeOutputDirectory {
            path: path.to_path_buf(),
            message: "Output directory must have a final path component.".to_string(),
        })?;
    let parent = parent.canonicalize().map_err(|error| ExportError::Io {
        path: parent.to_path_buf(),
        message: error.to_string(),
    })?;

    Ok(parent.join(file_name))
}

fn infer_source_roots(records: &[DocumentRecord]) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for record in records {
        let Some(mut root) = record.source_path.parent().map(Path::to_path_buf) else {
            continue;
        };

        for component in record
            .relative_path
            .parent()
            .into_iter()
            .flat_map(Path::components)
        {
            if matches!(component, Component::Normal(_)) {
                root.pop();
            }
        }

        if !roots.contains(&root) {
            roots.push(root);
        }
    }

    roots
}

/// Windows-reserved filenames that cannot be used as bare path components.
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Sanitise a single path component (a filename or directory name) for safe
/// cross-platform use, preserving Unicode readability.
///
/// * Replaces Windows-invalid filename characters (`< > : " / \ | ? *`) and
///   control characters with `_`.
/// * Trims leading/trailing whitespace and trailing dots.
/// * Collapses consecutive `_` characters.
/// * If the component (case-insensitively) matches a Windows reserved name
///   (CON, PRN, AUX, NUL, COM1–COM9, LPT1–LPT9) a trailing `_` is added.
/// * Caps at 128 Unicode characters.
/// * Falls back to `fallback` if the result would be empty.
fn sanitize_path_component(component: &str, fallback: &str) -> String {
    let trimmed = component.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    let mut sanitized = String::new();
    let mut previous_was_separator = false;

    for ch in trimmed.chars() {
        if sanitized.len() >= 128 {
            break;
        }
        let ch_is_bad = ch.is_ascii_control()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*');
        let replacement = if ch_is_bad { Some('_') } else { Some(ch) };

        if let Some(c) = replacement {
            if c == '_' {
                if previous_was_separator {
                    continue;
                }
                previous_was_separator = true;
            } else {
                previous_was_separator = false;
            }
            sanitized.push(c);
        }
    }

    // Trim trailing dots (Windows restriction) and whitespace
    let trimmed_sanitized = sanitized.trim_end_matches('.').trim().to_string();
    let result = if trimmed_sanitized.is_empty() {
        fallback.to_string()
    } else {
        trimmed_sanitized
    };

    // Check for Windows reserved names (case-insensitive)
    let lower = result.to_lowercase();
    if WINDOWS_RESERVED_NAMES.contains(&lower.as_str()) {
        format!("{}_", result)
    } else {
        result
    }
}

/// Build a relative output path under `texts/` for the given record.
///
/// * Preserves the relative directory structure (sanitised) from `record.relative_path.parent()`.
/// * Uses the original file stem (sanitised) and forces `.txt` extension.
/// * No index prefix or hash by default.
/// * On collision against `used_paths`, appends `__N` before `.txt`.
/// * If collisions exceed 100, falls back to a short hash suffix.
fn unique_output_relative_path(
    record: &DocumentRecord,
    used_paths: &mut HashSet<PathBuf>,
) -> PathBuf {
    // --- Build output subdirectory from relative_path.parent() ---
    let parent = record
        .relative_path
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let sub_dir: PathBuf = parent
        .components()
        .filter_map(|c| {
            if let std::path::Component::Normal(name) = c {
                let s = name.to_str().map(|s| sanitize_path_component(s, "folder"));
                s.filter(|s| !s.is_empty())
            } else {
                None
            }
        })
        .collect();

    // --- Build the output file stem (sanitised, then force .txt) ---
    let raw_stem = record
        .relative_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let sane_stem = sanitize_path_component(raw_stem, "document");

    // --- Assemble relative output path (relative to texts_dir) ---
    let base_path: PathBuf = if sub_dir.as_os_str().is_empty() {
        PathBuf::from(format!("{}.txt", sane_stem))
    } else {
        sub_dir.join(format!("{}.txt", sane_stem))
    };

    // --- Attempt to use the base path (no suffix needed) ---
    if !used_paths.contains(&base_path) {
        used_paths.insert(base_path.clone());
        return base_path;
    }

    // --- Collision: try `stem__2.txt`, `stem__3.txt`, etc. ---
    let dir_empty = sub_dir.as_os_str().is_empty();
    for n in 2..=100usize {
        let candidate = if dir_empty {
            PathBuf::from(format!("{}__{}.txt", sane_stem, n))
        } else {
            sub_dir.join(format!("{}__{}.txt", sane_stem, n))
        };
        if !used_paths.contains(&candidate) {
            used_paths.insert(candidate.clone());
            return candidate;
        }
    }

    // --- Safety valve: use a short hash when collisions exceed 100 ---
    let hash: String = sha256_hex(
        format!(
            "{}\n{}",
            record.source_path.display(),
            record.relative_path.display()
        )
        .as_bytes(),
    )
    .chars()
    .take(8)
    .collect();

    let candidate = if dir_empty {
        PathBuf::from(format!("{}__{}.txt", sane_stem, hash))
    } else {
        sub_dir.join(format!("{}__{}.txt", sane_stem, hash))
    };
    used_paths.insert(candidate.clone());
    candidate
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), ExportError> {
    let json = serde_json::to_string_pretty(value).map_err(|error| ExportError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    fs::write(path, format!("{json}\n")).map_err(|error| ExportError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn readme_text(app_name: &str) -> String {
    format!(
        "{app_name} processed corpus export\n\n\
         This directory contains processed UTF-8 .txt files under texts/.\n\
         manifest.json records source paths, output paths, hashes, character counts, and warnings.\n\
         config.json records the cleaning configuration used for this export.\n\
         warnings.json records export warnings and is present even when empty.\n\
         Source files were not modified.\n"
    )
}

