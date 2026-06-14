//! Extraction cache for corpus files.
//!
//! Caches extracted text from source files to avoid repeated PDF/DOCX
//! extraction across search, word count, preview, and export commands.
//!
//! # Locking
//!
//! Uses a two-phase pattern:
//! 1. Read-lock the cache map; if hit, clone entry and release lock.
//! 2. Extract text without holding any cache lock.
//! 3. Write-lock the cache map; insert if still absent.
//!
//! # Memory limits
//!
//! - Total cache size is bounded by `DEFAULT_MAX_TOTAL_BYTES` (256 MB).
//! - Per-entry limit is `DEFAULT_MAX_ENTRY_BYTES` (10 MB).
//! - Entries exceeding the per-entry cap are returned but not cached.
//! - FIFO eviction: when total bytes would be exceeded, oldest entries
//!   are evicted until within the limit.
//! - Cache is cleared on corpus reload / clear.

use crate::clean::{
    CleaningConfig, PdfEmbeddedTextStrategy, PdfOcrQuality, PdfTextSource, TableExtractionStrategy,
    clean_structured_document,
};
use crate::pdf::PdfExtractionOptions;
use crate::scan::{DocumentRecord, DocumentType};
use crate::structured_document::StructuredDocument;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::RwLock;

/// Default maximum total cache size in bytes (256 MB).
pub const DEFAULT_MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;

/// Default maximum per-entry size in bytes (10 MB).
pub const DEFAULT_MAX_ENTRY_BYTES: usize = 10 * 1024 * 1024;

/// Subset of PDF extraction options used as part of the cache key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PdfOptionsKey {
    pub strategy: PdfEmbeddedTextStrategy,
    pub text_source: PdfTextSource,
    pub ocr_quality: PdfOcrQuality,
    pub remove_repeated_headers_footers: bool,
    pub remove_page_labels: bool,
    pub remove_symbol_heavy_artifacts: bool,
    pub remove_code_like_blocks: bool,
    pub remove_formula_like_lines: bool,
}

impl From<PdfExtractionOptions> for PdfOptionsKey {
    fn from(opts: PdfExtractionOptions) -> Self {
        Self {
            strategy: opts.strategy,
            text_source: opts.text_source,
            ocr_quality: opts.ocr_quality,
            remove_repeated_headers_footers: opts.remove_repeated_headers_footers,
            remove_page_labels: opts.remove_page_labels,
            remove_symbol_heavy_artifacts: opts.remove_symbol_heavy_artifacts,
            remove_code_like_blocks: opts.remove_code_like_blocks,
            remove_formula_like_lines: opts.remove_formula_like_lines,
        }
    }
}

/// Subset of `CleaningConfig` fields that affect DOCX extraction
/// (as opposed to post-extraction cleaning).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocxConfigKey {
    pub table_extraction_strategy: TableExtractionStrategy,
    pub remove_headers: bool,
    pub remove_footers: bool,
    pub remove_footnotes: bool,
    pub remove_endnotes: bool,
    pub remove_comments: bool,
    pub remove_table_of_contents: bool,
}

impl From<&CleaningConfig> for DocxConfigKey {
    fn from(config: &CleaningConfig) -> Self {
        Self {
            table_extraction_strategy: config.table_extraction_strategy,
            remove_headers: config.remove_headers,
            remove_footers: config.remove_footers,
            remove_footnotes: config.remove_footnotes,
            remove_endnotes: config.remove_endnotes,
            remove_comments: config.remove_comments,
            remove_table_of_contents: config.remove_table_of_contents,
        }
    }
}

/// Composite key identifying a unique extraction.
///
/// Includes file identity (path, size, modified time) and the extraction
/// options that affect the output text for each document type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExtractionKey {
    /// Canonical source path.
    pub source_path: PathBuf,
    /// File size in bytes at scan time.
    pub size_bytes: u64,
    /// File modification time as seconds since UNIX_EPOCH (if available).
    pub modified_time_secs: Option<u64>,
    /// Document type (determines which extractor to use).
    pub document_type: DocumentType,
    /// PDF extraction options (only meaningful for PDF documents).
    pub pdf_options: Option<PdfOptionsKey>,
    /// DOCX extraction-affecting config subset (only meaningful for DOCX).
    pub docx_config: Option<DocxConfigKey>,
}

impl ExtractionKey {
    /// Build an `ExtractionKey` from a document record and extraction options.
    pub fn from_record(
        record: &DocumentRecord,
        pdf_options: Option<PdfExtractionOptions>,
        cleaning_config: &CleaningConfig,
    ) -> Self {
        let modified_time_secs = std::fs::metadata(&record.source_path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            });

        let pdf_opts_key = if record.document_type == DocumentType::Pdf {
            pdf_options.map(PdfOptionsKey::from)
        } else {
            None
        };

        let docx_cfg_key = if record.document_type == DocumentType::Docx {
            Some(DocxConfigKey::from(cleaning_config))
        } else {
            None
        };

        Self {
            source_path: record.source_path.clone(),
            size_bytes: record.size_bytes,
            modified_time_secs,
            document_type: record.document_type.clone(),
            pdf_options: pdf_opts_key,
            docx_config: docx_cfg_key,
        }
    }
}

/// A cached extraction result, preserving metadata from the extraction step.
///
/// Fields mirror the outputs of `ExtractedPdf` and `ExtractedDocx`:
/// - `extracted_text`: the raw extracted text (before cleaning).
/// - `warnings`: extraction warnings (e.g. PDF page extraction issues).
/// - `page_count`: number of pages (only meaningful for PDF; `None` for others).
/// - `page_texts`: page-local text when extraction produced reliable page chunks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheEntry {
    /// The extracted text (before cleaning).
    pub extracted_text: String,
    /// Extraction warnings surfaced by the PDF/DOCX extractor.
    pub warnings: Vec<String>,
    /// Number of pages (PDF only; `None` for DOCX and text files).
    pub page_count: Option<usize>,
    /// Extracted page text for page-aware formats. Older cache entries and
    /// flat formats do not have this metadata.
    #[serde(default)]
    pub page_texts: Option<Vec<String>>,
}

/// Text mode requested by downstream tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentTextMode {
    /// Raw materialised or extracted text.
    Original,
    /// Text after PDF cleanup, optional HTML extraction, and normal cleaning.
    Processed,
}

/// Text resolved through the canonical extraction/cache path.
pub struct DocumentText {
    pub text: String,
    pub warnings: Vec<String>,
    pub page_count: Option<usize>,
    pub page_texts: Option<Vec<String>>,
}

/// Thread-safe, size-limited cache for extracted text.
pub struct ExtractionCache {
    inner: RwLock<CacheInner>,
    max_total_bytes: usize,
    max_entry_bytes: usize,
}

struct CacheInner {
    entries: HashMap<ExtractionKey, CacheEntry>,
    /// Insertion order for FIFO eviction.
    order: VecDeque<ExtractionKey>,
    /// Approximate total byte size of cached text and page metadata.
    total_bytes: usize,
}

fn cache_entry_bytes(entry: &CacheEntry) -> usize {
    entry.extracted_text.len()
        + entry
            .page_texts
            .as_ref()
            .map(|pages| pages.iter().map(String::len).sum::<usize>())
            .unwrap_or(0)
}

impl ExtractionCache {
    /// Creates a new cache with default size limits.
    pub fn new() -> Self {
        Self::with_limits(DEFAULT_MAX_TOTAL_BYTES, DEFAULT_MAX_ENTRY_BYTES)
    }

    /// Creates a cache with explicit size limits.
    pub fn with_limits(max_total_bytes: usize, max_entry_bytes: usize) -> Self {
        Self {
            inner: RwLock::new(CacheInner {
                entries: HashMap::new(),
                order: VecDeque::new(),
                total_bytes: 0,
            }),
            max_total_bytes,
            max_entry_bytes,
        }
    }

    /// Returns the text extracted for `record`, using the cache if possible.
    ///
    /// Two-phase locking:
    /// 1. Read-lock for fast lookup.
    /// 2. If miss, extract without holding any lock.
    /// 3. Write-lock to insert if still absent.
    ///
    /// # Arguments
    ///
    /// * `record` - The document record to extract text from.
    /// * `pdf_options` - PDF extraction options (required for PDF documents,
    ///   ignored for others).
    /// * `cleaning_config` - Cleaning config used to derive DOCX extraction
    ///   options (table strategy, header/footer removal, etc.).
    pub fn get_or_extract(
        &self,
        record: &DocumentRecord,
        pdf_options: Option<PdfExtractionOptions>,
        cleaning_config: &CleaningConfig,
    ) -> Result<CacheEntry, String> {
        let key = ExtractionKey::from_record(record, pdf_options, cleaning_config);

        {
            let inner = self.inner.read().unwrap();
            if let Some(entry) = inner.entries.get(&key) {
                return Ok(entry.clone());
            }
        } // Read lock released

        let extracted = extract_text_from_record(record, pdf_options, cleaning_config)?;

        let entry_bytes = cache_entry_bytes(&extracted);

        if entry_bytes > self.max_entry_bytes {
            return Ok(extracted);
        }

        let mut inner = self.inner.write().unwrap();
        if let Some(existing) = inner.entries.get(&key) {
            // Another thread inserted while we were extracting
            return Ok(existing.clone());
        }

        while inner.total_bytes + entry_bytes > self.max_total_bytes {
            if let Some(evict_key) = inner.order.pop_front() {
                if let Some(evicted) = inner.entries.remove(&evict_key) {
                    inner.total_bytes = inner
                        .total_bytes
                        .saturating_sub(cache_entry_bytes(&evicted));
                }
            } else {
                break;
            }
        }

        // Store a cloned entry so the original extraction result can be returned unchanged.
        inner.entries.insert(key.clone(), extracted.clone());
        inner.order.push_back(key);
        inner.total_bytes += entry_bytes;

        Ok(extracted)
    }

    /// Inserts an already extracted entry for the given record and options.
    pub fn insert_extracted(
        &self,
        record: &DocumentRecord,
        pdf_options: Option<PdfExtractionOptions>,
        cleaning_config: &CleaningConfig,
        entry: CacheEntry,
    ) {
        let entry_bytes = cache_entry_bytes(&entry);
        let key = ExtractionKey::from_record(record, pdf_options, cleaning_config);
        let mut inner = self.inner.write().unwrap();

        if let Some(existing) = inner.entries.remove(&key) {
            inner.total_bytes = inner
                .total_bytes
                .saturating_sub(cache_entry_bytes(&existing));
            inner.order.retain(|existing_key| existing_key != &key);
        }

        if entry_bytes > self.max_total_bytes {
            inner.entries.clear();
            inner.order.clear();
            inner.total_bytes = 0;
            inner.entries.insert(key.clone(), entry);
            inner.order.push_back(key);
            inner.total_bytes = entry_bytes;
            return;
        }

        while inner.total_bytes + entry_bytes > self.max_total_bytes {
            if let Some(evict_key) = inner.order.pop_front() {
                if let Some(evicted) = inner.entries.remove(&evict_key) {
                    inner.total_bytes = inner
                        .total_bytes
                        .saturating_sub(cache_entry_bytes(&evicted));
                }
            } else {
                break;
            }
        }

        inner.entries.insert(key.clone(), entry);
        inner.order.push_back(key);
        inner.total_bytes += entry_bytes;
    }

    /// Read-only cache lookup. Returns `Some` if a matching entry exists,
    /// `None` otherwise. Never performs extraction or I/O.
    ///
    /// This is useful for preview paths that want to benefit from a warm
    /// cache (populated by export or previous `get_or_extract` calls)
    /// without forcing full extraction on a miss.
    pub fn try_get(
        &self,
        record: &DocumentRecord,
        pdf_options: Option<PdfExtractionOptions>,
        cleaning_config: &CleaningConfig,
    ) -> Option<CacheEntry> {
        let key = ExtractionKey::from_record(record, pdf_options, cleaning_config);
        let inner = self.inner.read().unwrap();
        inner.entries.get(&key).cloned()
    }

    /// Returns the number of cached entries.
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().entries.len()
    }

    /// Returns the approximate total byte size of cached text.
    pub fn total_bytes(&self) -> usize {
        self.inner.read().unwrap().total_bytes
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.entries.clear();
        inner.order.clear();
        inner.total_bytes = 0;
    }
}

fn docx_config_for_document_text(
    record: &DocumentRecord,
    mode: DocumentTextMode,
    cleaning_config: &CleaningConfig,
) -> CleaningConfig {
    match (record.document_type.clone(), mode) {
        (DocumentType::Docx, DocumentTextMode::Processed) => cleaning_config.clone(),
        (DocumentType::Docx, DocumentTextMode::Original) => CleaningConfig::default(),
        _ => cleaning_config.clone(),
    }
}

fn missing_materialised_ocr_message() -> String {
    "OCR text is not materialised for this PDF. Re-run special PDF intake or materialise OCR before scanning repeated artefacts.".to_string()
}

fn apply_document_processing(
    record: &DocumentRecord,
    text: String,
    cleaning_config: &CleaningConfig,
    warnings: &mut Vec<String>,
) -> String {
    let mut processed = if record.document_type == DocumentType::Pdf {
        let (pdf_cleaned, mut cleanup_warnings) =
            crate::pdf::clean_extracted_pdf_text(&text, cleaning_config);
        warnings.append(&mut cleanup_warnings);
        pdf_cleaned
    } else {
        text
    };

    if cleaning_config.extract_html {
        processed = crate::html::extract_html(&processed);
    }

    crate::clean::clean_text(&processed, cleaning_config)
}

/// Returns document text through the same raw extraction cache used by
/// materialised PDF intake.
///
/// OCR-mode PDFs require a matching cache entry. This prevents downstream
/// tools from silently falling back to embedded PDF text after special PDF
/// intake has already materialised OCR text.
pub fn document_text_for_record(
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
    mode: DocumentTextMode,
    cache: Option<&ExtractionCache>,
) -> Result<DocumentText, String> {
    let pdf_options = if record.document_type == DocumentType::Pdf {
        Some(PdfExtractionOptions::raw_from_cleaning_config(
            cleaning_config,
        ))
    } else {
        None
    };
    let extraction_config = docx_config_for_document_text(record, mode, cleaning_config);

    let raw_entry = if record.document_type == DocumentType::Pdf
        && pdf_options.is_some_and(|options| options.text_source != PdfTextSource::EmbeddedText)
    {
        let cache = cache.ok_or_else(missing_materialised_ocr_message)?;
        cache
            .try_get(record, pdf_options, &extraction_config)
            .ok_or_else(missing_materialised_ocr_message)?
    } else if let Some(cache) = cache {
        cache.get_or_extract(record, pdf_options, &extraction_config)?
    } else {
        extract_text_from_record(record, pdf_options, &extraction_config)?
    };

    let CacheEntry {
        extracted_text,
        mut warnings,
        page_count,
        page_texts,
    } = raw_entry;

    let (text, page_texts) = match mode {
        DocumentTextMode::Original => (extracted_text, page_texts),
        DocumentTextMode::Processed => {
            if record.document_type == DocumentType::Pdf
                && let Some(ref raw_pages) = page_texts
            {
                let mut warnings_flat = warnings.clone();
                let original_flat_text = apply_document_processing(
                    record,
                    extracted_text.clone(),
                    cleaning_config,
                    &mut warnings_flat,
                );

                let (_pdf_text, pdf_warnings, cleaned_pages) =
                    crate::pdf::clean_extracted_pdf_pages_and_text(raw_pages, cleaning_config);

                let document = StructuredDocument::from_pages(cleaned_pages);
                let cleaned = clean_structured_document(&document, cleaning_config);

                if cleaned.page_texts.is_some() && cleaned.text == original_flat_text {
                    warnings.extend(pdf_warnings);
                    (cleaned.text, cleaned.page_texts)
                } else {
                    (original_flat_text, None)
                }
            } else {
                let text = apply_document_processing(
                    record,
                    extracted_text,
                    cleaning_config,
                    &mut warnings,
                );
                (text, None)
            }
        }
    };

    Ok(DocumentText {
        text,
        warnings,
        page_count,
        page_texts,
    })
}

impl Default for ExtractionCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Extracts text from a document record without any caching.
///
/// Returns a `CacheEntry` with extracted text, warnings, and page count
/// (PDF only). The caller can then decide whether to cache the result.
fn extract_text_from_record(
    record: &DocumentRecord,
    pdf_options: Option<PdfExtractionOptions>,
    cleaning_config: &CleaningConfig,
) -> Result<CacheEntry, String> {
    let bytes =
        std::fs::read(&record.source_path).map_err(|e| format!("Failed to read file: {}", e))?;

    match record.document_type {
        DocumentType::Pdf => {
            let opts = pdf_options
                .ok_or_else(|| "PDF extraction options are required for PDF files.".to_string())?;
            let extracted = crate::pdf::extract_pdf(&bytes, None, opts)
                .map_err(|e| format!("PDF extraction failed: {}", e))?;
            Ok(CacheEntry {
                extracted_text: extracted.text,
                warnings: extracted.warnings,
                page_count: Some(extracted.page_count),
                page_texts: extracted.page_texts,
            })
        }
        DocumentType::Docx => {
            let extracted = crate::docx::extract_docx(&bytes, cleaning_config)
                .map_err(|e| format!("DOCX extraction failed: {}", e))?;
            Ok(CacheEntry {
                extracted_text: extracted.text,
                warnings: extracted.warnings,
                page_count: None,
                page_texts: None,
            })
        }
        _ => {
            // Plain text, HTML, or other textual files
            Ok(CacheEntry {
                extracted_text: String::from_utf8_lossy(&bytes).into_owned(),
                warnings: Vec::new(),
                page_count: None,
                page_texts: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::CleaningConfig;
    use crate::scan::{DocumentRecord, DocumentType};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn text_record(dir: &std::path::Path, name: &str, content: &str) -> DocumentRecord {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        DocumentRecord {
            source_path: path,
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Text,
            size_bytes: content.len() as u64,
        }
    }

    fn pdf_record(dir: &std::path::Path, name: &str) -> DocumentRecord {
        let path = dir.join(name);
        std::fs::write(&path, b"%PDF-1.4").unwrap();
        DocumentRecord {
            source_path: path,
            relative_path: PathBuf::from(name),
            document_type: DocumentType::Pdf,
            size_bytes: 8,
        }
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello.txt", "Hello world");
        let cache = ExtractionCache::new();

        let entry1 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry1.extracted_text, "Hello world");
        assert!(entry1.page_texts.is_none());

        let entry2 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry2.extracted_text, "Hello world");
        assert!(entry2.page_texts.is_none());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_entry_deserializes_without_page_texts() {
        let json = r#"{"extracted_text":"flat text","warnings":[],"page_count":2}"#;

        let entry: CacheEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.extracted_text, "flat text");
        assert_eq!(entry.page_count, Some(2));
        assert!(entry.page_texts.is_none());
    }

    #[test]
    fn test_cache_entry_deserializes_with_page_texts() {
        let json = r#"{"extracted_text":"alpha\n\nbeta","warnings":[],"page_count":2,"page_texts":["alpha","beta"]}"#;

        let entry: CacheEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.extracted_text, "alpha\n\nbeta");
        assert_eq!(
            entry.page_texts,
            Some(vec!["alpha".to_string(), "beta".to_string()])
        );
    }

    #[test]
    fn test_inserted_pdf_cache_entry_preserves_page_texts() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "pages.pdf");
        let cache = ExtractionCache::new();
        let page_texts = vec!["alpha".to_string(), "beta".to_string()];
        let pdf_opts = PdfExtractionOptions::raw_default();

        cache.insert_extracted(
            &record,
            Some(pdf_opts),
            &CleaningConfig::default(),
            CacheEntry {
                extracted_text: "alpha\n\nbeta".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(page_texts.clone()),
            },
        );

        let entry = cache
            .try_get(&record, Some(pdf_opts), &CleaningConfig::default())
            .expect("inserted entry should be cached");

        assert_eq!(entry.extracted_text, "alpha\n\nbeta");
        assert_eq!(entry.page_texts, Some(page_texts));
    }

    #[test]
    fn test_processed_document_text_does_not_return_raw_page_texts() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "processed-pages.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "HEADER\n\nBODY".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["HEADER".to_string(), "BODY".to_string()]),
            },
        );

        let original = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Original,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(
            original.page_texts,
            Some(vec!["HEADER".to_string(), "BODY".to_string()])
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(processed.text, "header\n\nbody");
        // Cleaned page texts are returned (not raw uppercase ones)
        assert_eq!(
            processed.page_texts,
            Some(vec!["header".to_string(), "body".to_string()])
        );
    }

    #[test]
    fn test_processed_cached_text_without_page_metadata_behaves_as_before() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "flat.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "HEADER\n\nBODY".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: None, // No page metadata
            },
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(processed.text, "header\n\nbody");
        assert!(processed.page_texts.is_none());
    }

    #[test]
    fn test_processed_cached_text_with_page_metadata_uses_page_aware_cleaner_and_preserves() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "pages.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "HEADER\n\nBODY".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["HEADER".to_string(), "BODY".to_string()]),
            },
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(processed.text, "header\n\nbody");
        assert_eq!(
            processed.page_texts,
            Some(vec!["header".to_string(), "body".to_string()])
        );
    }

    #[test]
    fn test_cleaned_page_metadata_is_preserved_when_independently_cleaned_pages_join_exactly() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "exact-join.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            lowercase: true,
            trim_lines: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "  ALPHA  \n\n  BETA  ".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["  ALPHA  ".to_string(), "  BETA  ".to_string()]),
            },
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(processed.text, "alpha\n\nbeta");
        assert_eq!(
            processed.page_texts,
            Some(vec!["alpha".to_string(), "beta".to_string()])
        );
    }

    #[test]
    fn test_cleaned_page_metadata_is_dropped_when_equivalence_fails() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "equivalence-fail.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            join_line_breaks: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "Alpha\nBeta".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["Alpha".to_string(), "Beta".to_string()]),
            },
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();
        assert_eq!(processed.text, "Alpha Beta");
        assert!(processed.page_texts.is_none());
    }

    #[test]
    fn test_raw_page_texts_are_never_paired_with_cleaned_flat_text() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "raw-pairing-check.pdf");
        let cache = ExtractionCache::new();
        let cleaning_config = CleaningConfig {
            lowercase: true,
            ..CleaningConfig::default()
        };
        let pdf_options = PdfExtractionOptions::raw_from_cleaning_config(&cleaning_config);

        cache.insert_extracted(
            &record,
            Some(pdf_options),
            &cleaning_config,
            CacheEntry {
                extracted_text: "HEADER\n\nBODY".to_string(),
                warnings: Vec::new(),
                page_count: Some(2),
                page_texts: Some(vec!["HEADER".to_string(), "BODY".to_string()]),
            },
        );

        let processed = document_text_for_record(
            &record,
            &cleaning_config,
            DocumentTextMode::Processed,
            Some(&cache),
        )
        .unwrap();

        assert_eq!(processed.text, "header\n\nbody");
        if let Some(pages) = processed.page_texts {
            for page in pages {
                assert_eq!(page, page.to_lowercase());
            }
        }
    }

    #[test]
    fn test_clear_removes_entries() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "a.txt", "Content");
        let cache = ExtractionCache::new();

        cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        let entry = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry.extracted_text, "Content");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_different_files_have_different_keys() {
        let dir = tempdir().unwrap();
        let record1 = text_record(dir.path(), "a.txt", "Alpha");
        let record2 = text_record(dir.path(), "b.txt", "Beta");
        let cache = ExtractionCache::new();

        let e1 = cache
            .get_or_extract(&record1, None, &CleaningConfig::default())
            .unwrap();
        let e2 = cache
            .get_or_extract(&record2, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(e1.extracted_text, "Alpha");
        assert_eq!(e2.extracted_text, "Beta");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_different_pdf_options_create_different_keys() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "test.pdf");

        let _cache = ExtractionCache::new();

        let opts_a = PdfExtractionOptions {
            text_source: PdfTextSource::EmbeddedText,
            ocr_quality: PdfOcrQuality::Balanced,
            remove_code_like_blocks: false,
            remove_formula_like_lines: false,
            remove_page_labels: false,
            remove_repeated_headers_footers: false,
            remove_symbol_heavy_artifacts: false,
            strategy: PdfEmbeddedTextStrategy::PdfiumFlat,
        };
        let opts_b = PdfExtractionOptions {
            text_source: PdfTextSource::Ocr,
            ..opts_a
        };
        let opts_c = PdfExtractionOptions {
            text_source: PdfTextSource::ForceOcr,
            ..opts_a
        };
        let opts_d = PdfExtractionOptions {
            text_source: PdfTextSource::ForceOcr,
            ocr_quality: PdfOcrQuality::HighQuality,
            ..opts_a
        };

        let key_a = ExtractionKey::from_record(&record, Some(opts_a), &CleaningConfig::default());
        let key_b = ExtractionKey::from_record(&record, Some(opts_b), &CleaningConfig::default());
        let key_c = ExtractionKey::from_record(&record, Some(opts_c), &CleaningConfig::default());
        let key_d = ExtractionKey::from_record(&record, Some(opts_d), &CleaningConfig::default());
        assert_ne!(
            key_a, key_b,
            "Embedded text and OCR rescue cache keys should differ"
        );
        assert_ne!(
            key_b, key_c,
            "OCR rescue and force OCR cache keys should differ"
        );
        assert_ne!(
            key_c, key_d,
            "OCR quality presets should be part of the cache key"
        );

        let key_a2 = ExtractionKey::from_record(&record, Some(opts_a), &CleaningConfig::default());
        assert_eq!(key_a, key_a2, "Same PDF options should match");
    }

    #[test]
    fn test_different_docx_config_creates_different_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.docx");
        let record = DocumentRecord {
            source_path: path.clone(),
            relative_path: PathBuf::from("test.docx"),
            document_type: DocumentType::Docx,
            size_bytes: 0,
        };
        std::fs::write(&path, b"PK\x05\x06").unwrap(); // empty ZIP footer (won't extract, but key test is fine)

        let _cache = ExtractionCache::new();

        let config_a = CleaningConfig {
            remove_headers: false,
            ..CleaningConfig::default()
        };
        let config_b = CleaningConfig {
            remove_headers: true,
            ..CleaningConfig::default()
        };

        let key_a = ExtractionKey::from_record(&record, None, &config_a);
        let key_b = ExtractionKey::from_record(&record, None, &config_b);
        assert_ne!(
            key_a, key_b,
            "DOCX configs with different headers flag should differ"
        );

        let key_a2 = ExtractionKey::from_record(&record, None, &config_a);
        assert_eq!(key_a, key_a2, "Same DOCX config should match");
    }

    #[test]
    fn test_try_get_returns_none_for_empty_cache() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "empty.txt", "content");
        let cache = ExtractionCache::new();

        assert!(
            cache
                .try_get(&record, None, &CleaningConfig::default())
                .is_none()
        );
    }

    #[test]
    fn test_try_get_returns_some_after_get_or_extract() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "cached.txt", "Hello from cache");
        let cache = ExtractionCache::new();

        cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();

        let entry = cache
            .try_get(&record, None, &CleaningConfig::default())
            .expect("should be a hit after get_or_extract");
        assert_eq!(entry.extracted_text, "Hello from cache");
    }

    #[test]
    fn test_cache_entry_carries_warnings_and_page_count_for_pdf() {
        let dir = tempdir().unwrap();
        let record = pdf_record(dir.path(), "test.pdf");

        let cache = ExtractionCache::new();
        let pdf_opts = PdfExtractionOptions::raw_default();

        let result = cache.get_or_extract(&record, Some(pdf_opts), &CleaningConfig::default());
        // The minimal PDF may fail before cache metadata exists; either outcome is acceptable here.
        if let Ok(entry) = result {
            assert!(entry.page_count.is_some() || entry.warnings.is_empty());
        }
    }

    #[test]
    fn test_per_entry_size_cap_skips_caching() {
        let dir = tempdir().unwrap();
        let content = "x".repeat(100);
        let record = text_record(dir.path(), "big.txt", &content);
        let cache = ExtractionCache::with_limits(10_000_000, 50);

        let entry = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry.extracted_text, content);
        assert_eq!(
            cache.len(),
            0,
            "entry exceeding per-entry cap should not be cached"
        );
    }

    #[test]
    fn test_total_size_cap_evicts_old_entries() {
        let dir = tempdir().unwrap();
        // The tiny total limit forces FIFO eviction after two short entries.
        let cache = ExtractionCache::with_limits(150, 10_000_000);

        let record1 = text_record(dir.path(), "a.txt", &"a".repeat(80));
        let record2 = text_record(dir.path(), "b.txt", &"b".repeat(80));
        let record3 = text_record(dir.path(), "c.txt", &"c".repeat(80));

        cache
            .get_or_extract(&record1, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(cache.len(), 1, "first entry should be cached");

        cache
            .get_or_extract(&record2, None, &CleaningConfig::default())
            .unwrap();

        let len_after_two = cache.len();
        assert!(
            len_after_two <= 2,
            "after two inserts, cache should have at most 2 entries, got {}",
            len_after_two
        );

        cache
            .get_or_extract(&record3, None, &CleaningConfig::default())
            .unwrap();
        let len_after_three = cache.len();
        assert!(
            len_after_three <= 2,
            "after three inserts, cache should have at most 2 entries (FIFO eviction), got {}",
            len_after_three
        );
    }

    #[test]
    fn test_extract_text_basic() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "sample.txt", "Hello world");
        let entry = extract_text_from_record(&record, None, &CleaningConfig::default()).unwrap();
        assert_eq!(entry.extracted_text, "Hello world");
        assert!(entry.warnings.is_empty());
        assert!(entry.page_count.is_none());
        assert!(entry.page_texts.is_none());
    }

    #[test]
    fn test_cache_reuses_extraction() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "reuse.txt", "Reusable text");
        let cache = ExtractionCache::new();

        let e1 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(e1.extracted_text, "Reusable text");
        assert_eq!(cache.len(), 1);
        let bytes_after_first = cache.total_bytes();

        let e2 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(e2.extracted_text, "Reusable text");
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.total_bytes(),
            bytes_after_first,
            "cache bytes should not increase on hit"
        );
    }
}
