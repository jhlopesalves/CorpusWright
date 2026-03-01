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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{CleaningConfig, ReplacementRule};
    use crate::scan::scan_directory;
    use std::fs;
    use tempfile::tempdir;

    // ----------------------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------------------
    /// Build a DocumentRecord manually for testing naming/export logic.
    fn make_record(
        source_dir: &Path,
        relative: &str,
        doc_type: DocumentType,
        content: &str,
    ) -> DocumentRecord {
        let source_path = source_dir.join(relative);
        // Ensure parent directory exists
        if let Some(parent) = source_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&source_path, content).unwrap();
        DocumentRecord {
            source_path,
            relative_path: PathBuf::from(relative),
            document_type: doc_type,
            size_bytes: content.len() as u64,
        }
    }

    // ----------------------------------------------------------------------
    // sanitize_path_component unit tests
    // ----------------------------------------------------------------------
    #[test]
    fn sanitize_keeps_valid_unicode() {
        assert_eq!(sanitize_path_component("Linha_10", "document"), "Linha_10");
        assert_eq!(sanitize_path_component("café", "document"), "café");
        // Spaces are valid on modern filesystems and preserved
        assert_eq!(sanitize_path_component("a b.txt", "document"), "a b.txt");
    }

    #[test]
    fn sanitize_replaces_invalid_chars() {
        let s = sanitize_path_component("a<b>c:d\"e/f\\g|h?i*j", "document");
        assert_eq!(s, "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn sanitize_handles_windows_reserved_names() {
        assert_eq!(sanitize_path_component("CON", "document"), "CON_");
        assert_eq!(sanitize_path_component("con", "document"), "con_");
        assert_eq!(sanitize_path_component("nul", "document"), "nul_");
        assert_eq!(sanitize_path_component("COM1", "document"), "COM1_");
        assert_eq!(sanitize_path_component("lpt9", "document"), "lpt9_");
    }

    #[test]
    fn sanitize_empty_falls_back() {
        assert_eq!(sanitize_path_component("", "document"), "document");
        assert_eq!(sanitize_path_component("   ", "document"), "document");
    }

    #[test]
    fn sanitize_trims_trailing_dots() {
        let s = sanitize_path_component("file...", "document");
        assert!(!s.ends_with('.'));
    }

    #[test]
    fn sanitize_caps_at_128_chars() {
        let long = "a".repeat(200);
        let s = sanitize_path_component(&long, "document");
        assert!(s.len() <= 128);
    }

    // ----------------------------------------------------------------------
    // unique_output_relative_path unit tests
    // ----------------------------------------------------------------------
    #[test]
    fn relative_path_plain_txt_file() {
        let rec = DocumentRecord {
            source_path: PathBuf::from("/tmp/Linha_10.txt"),
            relative_path: PathBuf::from("Linha_10.txt"),
            document_type: DocumentType::Text,
            size_bytes: 10,
        };
        let mut used = HashSet::new();
        let out = unique_output_relative_path(&rec, &mut used);
        assert_eq!(out, PathBuf::from("Linha_10.txt"));
    }

    #[test]
    fn relative_path_pdf_extension_changed() {
        let rec = DocumentRecord {
            source_path: PathBuf::from("/tmp/paper.pdf"),
            relative_path: PathBuf::from("paper.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 100,
        };
        let mut used = HashSet::new();
        let out = unique_output_relative_path(&rec, &mut used);
        assert_eq!(out, PathBuf::from("paper.txt"));
    }

    #[test]
    fn relative_path_nested_subdirectory() {
        let rec = DocumentRecord {
            source_path: PathBuf::from("/tmp/sub/a.txt"),
            relative_path: PathBuf::from("sub/a.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let mut used = HashSet::new();
        let out = unique_output_relative_path(&rec, &mut used);
        assert_eq!(out, PathBuf::from("sub/a.txt"));
    }

    #[test]
    fn relative_path_collision_creates_suffixed_name() {
        let mut used = HashSet::new();
        let rec1 = DocumentRecord {
            source_path: PathBuf::from("/tmp/one/name.txt"),
            relative_path: PathBuf::from("one/name.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let rec2 = DocumentRecord {
            source_path: PathBuf::from("/tmp/two/name.txt"),
            relative_path: PathBuf::from("two/name.txt"),
            // Different subdirs -> no collision
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let out1 = unique_output_relative_path(&rec1, &mut used);
        assert_eq!(out1, PathBuf::from("one/name.txt"));
        let out2 = unique_output_relative_path(&rec2, &mut used);
        assert_eq!(out2, PathBuf::from("two/name.txt"));
    }

    #[test]
    fn relative_path_collision_same_subdir() {
        let mut used = HashSet::new();
        // Same subdir and same stem -> collision
        let rec1 = DocumentRecord {
            source_path: PathBuf::from("/tmp/sub/name.txt"),
            relative_path: PathBuf::from("sub/name.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        // Second with same relative_path but different source (can happen with symlinks etc.)
        let rec2 = DocumentRecord {
            source_path: PathBuf::from("/tmp/sub/name_other.txt"),
            relative_path: PathBuf::from("sub/name.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let out1 = unique_output_relative_path(&rec1, &mut used);
        assert_eq!(out1, PathBuf::from("sub/name.txt"));
        let out2 = unique_output_relative_path(&rec2, &mut used);
        assert_eq!(out2, PathBuf::from("sub/name__2.txt"));
    }

    #[test]
    fn relative_path_unsafe_characters_sanitised() {
        let rec = DocumentRecord {
            source_path: PathBuf::from("/tmp/file<bad>.txt"),
            relative_path: PathBuf::from("file<bad>.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let mut used = HashSet::new();
        let out = unique_output_relative_path(&rec, &mut used);
        assert_eq!(out, PathBuf::from("file_bad_.txt"));
    }

    #[test]
    fn relative_path_empty_stem_falls_back() {
        // If file_stem returns empty or None, should fallback to "document"
        let rec = DocumentRecord {
            source_path: PathBuf::from("/tmp/.hidden"),
            relative_path: PathBuf::from(".hidden"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let mut used = HashSet::new();
        let out = unique_output_relative_path(&rec, &mut used);
        assert!(!out.as_os_str().is_empty());
    }

    // ----------------------------------------------------------------------
    // Integration tests for export_corpus
    // ----------------------------------------------------------------------
    #[test]
    fn exports_one_file() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "Hello").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        // Output file should be texts/a.txt
        let expected_output = export.texts_dir.join("a.txt");
        assert_eq!(fs::read_to_string(&expected_output).unwrap(), "Hello");
        // Manifest output_path should match
        assert_eq!(
            export.exported_files[0].output_path,
            PathBuf::from("texts").join("a.txt")
        );
    }

    #[test]
    fn exports_multiple_files() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        fs::write(source.path().join("b.html"), "B").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 2);
        assert_eq!(fs::read_dir(&export.texts_dir).unwrap().count(), 2);
        // Check filenames are source-like
        let mut names: Vec<String> = fs::read_dir(&export.texts_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn duplicate_basenames_do_not_collide() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::create_dir(source.path().join("one")).unwrap();
        fs::create_dir(source.path().join("two")).unwrap();
        fs::write(source.path().join("one").join("same.txt"), "one").unwrap();
        fs::write(source.path().join("two").join("same.txt"), "two").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        // Different subdirs -> no collision, both are in separate folders
        assert_eq!(export.files_exported, 2);
        assert!(
            export
                .exported_files
                .iter()
                .any(|f| f.output_path == *"texts/one/same.txt")
        );
        assert!(
            export
                .exported_files
                .iter()
                .any(|f| f.output_path == *"texts/two/same.txt")
        );
        assert_eq!(
            fs::read_to_string(export.texts_dir.join("one/same.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(export.texts_dir.join("two/same.txt")).unwrap(),
            "two"
        );
    }

    #[test]
    fn writes_only_txt_files_under_texts() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        // Create a subdirectory to test recursive check
        fs::create_dir(source.path().join("sub")).unwrap();
        fs::write(source.path().join("sub").join("a.html"), "<h1>A</h1>").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        // Walk all files under texts/ and verify .txt extension
        for entry in walkdir(export.texts_dir.as_path()) {
            let path = entry.path();
            if path.is_file() {
                assert_eq!(
                    path.extension().unwrap(),
                    "txt",
                    "Non-txt file found: {:?}",
                    path
                );
            }
        }
    }

    fn walkdir(dir: &Path) -> Vec<std::fs::DirEntry> {
        let mut entries = Vec::new();
        if dir.is_dir()
            && let Ok(read_dir) = fs::read_dir(dir)
        {
            for entry in read_dir.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    entries.extend(walkdir(&entry.path()));
                } else {
                    entries.push(entry);
                }
            }
        }
        entries
    }

    #[test]
    fn writes_manifest_warnings_config_and_readme() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert!(export.manifest_path.exists());
        assert!(export.warnings_path.exists());
        assert!(export.config_path.exists());
        assert!(export.readme_path.exists());
    }

    #[test]
    fn manifest_contains_source_and_processed_hashes() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        let manifest: ExportManifest =
            serde_json::from_str(&fs::read_to_string(export.manifest_path).unwrap()).unwrap();
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].source_hash_sha256.len(), 64);
        assert_eq!(manifest.files[0].processed_hash_sha256.len(), 64);
    }

    #[test]
    fn refuses_existing_output_directory_unless_overwrite_is_true() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        let output = output_parent.path().join("export");
        fs::create_dir(&output).unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let result = export_corpus(
            &report.files,
            &output,
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        );
        assert!(matches!(
            result,
            Err(ExportError::ExistingOutputDirectory { .. })
        ));

        let options = ExportOptions {
            overwrite: true,
            ..ExportOptions::default()
        };
        assert!(
            export_corpus(
                &report.files,
                &output,
                &CleaningConfig::default(),
                &options,
                None,
                None,
            )
            .is_ok()
        );
    }

    #[test]
    fn source_files_are_unchanged_after_export() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        let source_path = source.path().join("a.txt");
        fs::write(&source_path, "Original").unwrap();
        let before = fs::read(&source_path).unwrap();
        let report = scan_directory(source.path()).unwrap();
        let config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };

        export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &config,
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(fs::read(&source_path).unwrap(), before);
    }

    #[test]
    fn allows_output_inside_source_root_subdirectory() {
        let source = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let result = export_corpus(
            &report.files,
            source.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        );

        // Should succeed — subdirectories inside the corpus root are valid export targets
        assert!(result.is_ok());
        let export = result.unwrap();
        assert_eq!(export.files_exported, 1);
        assert!(export.texts_dir.join("a.txt").exists());
    }

    #[test]
    fn applies_cleaning_during_export() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "HELLO old").unwrap();
        let report = scan_directory(source.path()).unwrap();
        let config = CleaningConfig {
            lowercase: true,
            replace_patterns: vec![ReplacementRule {
                pattern: "old".to_string(),
                replacement: "new".to_string(),
            }],
            ..CleaningConfig::default()
        };

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &config,
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        let output_file = export.texts_dir.join("a.txt");
        assert_eq!(fs::read_to_string(output_file).unwrap(), "hello new");
    }

    #[test]
    fn warnings_json_is_written_even_when_empty() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(fs::read_to_string(export.warnings_path).unwrap(), "[]\n");
    }

    #[test]
    fn public_export_structs_serialize_to_json() {
        let options = ExportOptions::default();
        let json = serde_json::to_string(&options).unwrap();
        assert!(json.contains("app_name"));
    }

    #[test]
    fn exports_to_bare_relative_output_directory() {
        let source = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let output_dir_name = "test_bare_relative_output_dir_12345";
        let output_path = std::path::PathBuf::from(output_dir_name);
        if output_path.exists() {
            fs::remove_dir_all(&output_path).unwrap();
        }

        let export = export_corpus(
            &report.files,
            &output_path,
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        assert!(export.manifest_path.exists());
        fs::remove_dir_all(&output_path).unwrap();
    }

    #[test]
    fn exports_to_dot_slash_relative_output_directory() {
        let source = tempdir().unwrap();
        fs::write(source.path().join("a.txt"), "A").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let output_dir_name = "./test_dot_slash_relative_output_dir_12345";
        let output_path = std::path::PathBuf::from(output_dir_name);
        if output_path.exists() {
            fs::remove_dir_all(&output_path).unwrap();
        }

        let export = export_corpus(
            &report.files,
            &output_path,
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        assert!(export.manifest_path.exists());
        fs::remove_dir_all(&output_path).unwrap();
    }

    // ----------------------------------------------------------------------
    // New naming tests
    // ----------------------------------------------------------------------
    #[test]
    fn exports_text_file_with_source_like_name() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();
        fs::write(source.path().join("Linha_10.txt"), "Hello").unwrap();
        let report = scan_directory(source.path()).unwrap();

        let export = export_corpus(
            &report.files,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        // Output should be texts/Linha_10.txt
        let output_path = export.texts_dir.join("Linha_10.txt");
        assert!(output_path.exists());
        assert_eq!(fs::read_to_string(output_path).unwrap(), "Hello");
        // Manifest must match
        assert_eq!(
            export.exported_files[0].output_path,
            PathBuf::from("texts").join("Linha_10.txt")
        );
    }

    #[test]
    fn exports_non_txt_changes_extension_to_txt() {
        // For a PDF, the output should have .txt extension
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        // We can test naming directly without needing actual PDF extraction
        // by using a DocumentRecord with DocumentType::Pdf
        let record = make_record(
            source.path(),
            "paper.pdf",
            DocumentType::Pdf,
            "dummy content",
        );
        let records = vec![record];

        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        assert_eq!(
            export.exported_files[0].output_path,
            PathBuf::from("texts").join("paper.txt")
        );
        // The file on disk should also be .txt
        assert!(export.texts_dir.join("paper.txt").exists());
    }

    #[test]
    fn exports_html_changes_extension_to_txt() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        let record = make_record(source.path(), "page.html", DocumentType::Html, "<h1>A</h1>");
        let records = vec![record];

        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        assert_eq!(
            export.exported_files[0].output_path,
            PathBuf::from("texts").join("page.txt")
        );
        assert!(export.texts_dir.join("page.txt").exists());
    }

    #[test]
    fn exports_docx_changes_extension_to_txt() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        let record = make_record(source.path(), "essay.docx", DocumentType::Docx, "dummy");
        let records = vec![record];

        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        assert_eq!(
            export.exported_files[0].output_path,
            PathBuf::from("texts").join("essay.txt")
        );
        assert!(export.texts_dir.join("essay.txt").exists());
    }

    #[test]
    fn preserves_relative_subdirectories() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        let record = make_record(
            source.path(),
            "CoreReviews/IMBD Corpus/Linha_10.txt",
            DocumentType::Text,
            "Hello",
        );
        let records = vec![record];

        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        // Output should be texts/CoreReviews/IMBD Corpus/Linha_10.txt
        let expected_rel = PathBuf::from("texts/CoreReviews/IMBD Corpus/Linha_10.txt");
        assert_eq!(export.exported_files[0].output_path, expected_rel);
        // The file must actually exist on disk at that relative path under texts_dir
        assert!(
            export
                .texts_dir
                .join("CoreReviews/IMBD Corpus/Linha_10.txt")
                .exists()
        );
    }

    #[test]
    fn collision_handling_integration() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        // Create two files with same relative_path (same directory, same name)
        fs::create_dir_all(source.path().join("sub")).unwrap();
        fs::write(source.path().join("sub").join("name.txt"), "first").unwrap();
        // Second file is different source but same relative path
        fs::write(source.path().join("sub").join("name_other.txt"), "second").unwrap();

        // We need to manually construct records because scan will give different relative_paths
        let rec1 = DocumentRecord {
            source_path: source.path().join("sub").join("name.txt"),
            relative_path: PathBuf::from("sub").join("name.txt"),
            document_type: DocumentType::Text,
            size_bytes: 5,
        };
        let rec2 = DocumentRecord {
            source_path: source.path().join("sub").join("name_other.txt"),
            relative_path: PathBuf::from("sub").join("name.txt"), // Same relative path intentionally
            document_type: DocumentType::Text,
            size_bytes: 6,
        };

        let records = vec![rec1, rec2];
        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 2);

        // First should be name.txt, second should be name__2.txt
        let paths: Vec<&PathBuf> = export
            .exported_files
            .iter()
            .map(|f| &f.output_path)
            .collect();
        assert!(paths.contains(&&PathBuf::from("texts/sub/name.txt")));
        assert!(paths.contains(&&PathBuf::from("texts/sub/name__2.txt")));

        // Check on disk
        assert!(export.texts_dir.join("sub/name.txt").exists());
        assert!(export.texts_dir.join("sub/name__2.txt").exists());
        assert_eq!(
            fs::read_to_string(export.texts_dir.join("sub/name.txt")).unwrap(),
            "first"
        );
        assert_eq!(
            fs::read_to_string(export.texts_dir.join("sub/name__2.txt")).unwrap(),
            "second"
        );
    }

    #[test]
    fn unsafe_path_characters_are_sanitised_in_export() {
        let source = tempdir().unwrap();
        let output_parent = tempdir().unwrap();

        // On Windows we cannot create a file with '<' or ':' in the name,
        // so we use a safe source filename but override the relative_path
        // to contain unsafe characters, simulating what would happen on Linux.
        let source_path = source.path().join("source.txt");
        fs::write(&source_path, "content").unwrap();
        let record = DocumentRecord {
            source_path,
            relative_path: PathBuf::from("file<bad>:name.txt"),
            document_type: DocumentType::Text,
            size_bytes: 7,
        };
        let records = vec![record];

        let export = export_corpus(
            &records,
            output_parent.path().join("export"),
            &CleaningConfig::default(),
            &ExportOptions::default(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(export.files_exported, 1);
        // The output must have sanitised name
        let rel = export.exported_files[0].output_path.clone();
        assert_eq!(rel, PathBuf::from("texts/file_bad_name.txt"));
        assert!(export.texts_dir.join("file_bad_name.txt").exists());
    }
}
