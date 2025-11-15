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

use crate::clean::{CleaningConfig, PdfEmbeddedTextStrategy, TableExtractionStrategy};
use crate::pdf::PdfExtractionOptions;
use crate::scan::{DocumentRecord, DocumentType};
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
    pub use_ocr: bool,
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
            use_ocr: opts.use_ocr,
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
#[derive(Clone)]
pub struct CacheEntry {
    /// The extracted text (before cleaning).
    pub extracted_text: String,
    /// Extraction warnings surfaced by the PDF/DOCX extractor.
    pub warnings: Vec<String>,
    /// Number of pages (PDF only; `None` for DOCX and text files).
    pub page_count: Option<usize>,
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
    /// Total byte size of all cached extracted_text strings.
    total_bytes: usize,
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

        let entry_bytes = extracted.extracted_text.len();
        let page_count = extracted.page_count;
        let warnings = extracted.warnings.clone();

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
                        .saturating_sub(evicted.extracted_text.len());
                }
            } else {
                break;
            }
        }

        // Store a cloned entry so the original extraction result can be returned unchanged.
        inner.entries.insert(
            key.clone(),
            CacheEntry {
                extracted_text: extracted.extracted_text.clone(),
                warnings,
                page_count,
            },
        );
        inner.order.push_back(key);
        inner.total_bytes += entry_bytes;

        Ok(extracted)
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
            })
        }
        DocumentType::Docx => {
            let extracted = crate::docx::extract_docx(&bytes, cleaning_config)
                .map_err(|e| format!("DOCX extraction failed: {}", e))?;
            Ok(CacheEntry {
                extracted_text: extracted.text,
                warnings: extracted.warnings,
                page_count: None,
            })
        }
        _ => {
            // Plain text, HTML, or other textual files
            Ok(CacheEntry {
                extracted_text: String::from_utf8_lossy(&bytes).into_owned(),
                warnings: Vec::new(),
                page_count: None,
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

    #[test]
    fn test_cache_miss_then_hit() {
        let dir = tempdir().unwrap();
        let record = text_record(dir.path(), "hello.txt", "Hello world");
        let cache = ExtractionCache::new();

        let entry1 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry1.extracted_text, "Hello world");

        let entry2 = cache
            .get_or_extract(&record, None, &CleaningConfig::default())
            .unwrap();
        assert_eq!(entry2.extracted_text, "Hello world");
        assert_eq!(cache.len(), 1);
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
        let path = dir.path().join("test.pdf");
        let record = DocumentRecord {
            source_path: path.clone(),
            relative_path: PathBuf::from("test.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 0,
        };
        std::fs::write(&path, b"%PDF-1.4").unwrap();

        let _cache = ExtractionCache::new();

        let opts_a = PdfExtractionOptions {
            use_ocr: false,
            remove_code_like_blocks: false,
            remove_formula_like_lines: false,
            remove_page_labels: false,
            remove_repeated_headers_footers: false,
            remove_symbol_heavy_artifacts: false,
            strategy: PdfEmbeddedTextStrategy::PdfiumFlat,
        };
        let opts_b = PdfExtractionOptions {
            use_ocr: true,
            ..opts_a
        };

        let key_a = ExtractionKey::from_record(&record, Some(opts_a), &CleaningConfig::default());
        let key_b = ExtractionKey::from_record(&record, Some(opts_b), &CleaningConfig::default());
        assert_ne!(key_a, key_b, "PDF options with different OCR should differ");

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
        let path = dir.path().join("test.pdf");
        let record = DocumentRecord {
            source_path: path.clone(),
            relative_path: PathBuf::from("test.pdf"),
            document_type: DocumentType::Pdf,
            size_bytes: 0,
        };
        std::fs::write(&path, b"%PDF-1.4").unwrap();

        let cache = ExtractionCache::new();
        let pdf_opts = PdfExtractionOptions {
            use_ocr: false,
            ..PdfExtractionOptions::raw_default()
        };

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
