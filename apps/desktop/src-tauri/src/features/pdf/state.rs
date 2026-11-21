use crate::runtime::cancellation::ActiveCancellation;
use crate::runtime::sync::{read_recover, write_recover};
use corpuswright_core::preparation::cleaning::{PdfOcrQuality, PdfTextSource};
use corpuswright_core::sources::pdf::PdfPageRangeResult;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

const PDF_PAGE_RANGE_CACHE_RESOURCE: &str = "PDF page-range cache";

pub(crate) struct PdfIntakeState {
    pub(crate) cancel: ActiveCancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PdfPageRangeCacheKey {
    pub(crate) source_path: PathBuf,
    pub(crate) size_bytes: u64,
    pub(crate) modified_time_secs: Option<u64>,
    pub(crate) start_page_index: usize,
    pub(crate) page_count: usize,
    pub(crate) max_chars_per_page: Option<usize>,
    pub(crate) text_source: PdfTextSource,
    pub(crate) ocr_quality: PdfOcrQuality,
    pub(crate) cleaning_config_json: String,
    pub(crate) ocr_model_identity: Option<String>,
}

pub(crate) struct PdfPageRangeCache {
    pub(crate) inner: RwLock<HashMap<PdfPageRangeCacheKey, PdfPageRangeResult>>,
}

impl PdfPageRangeCache {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn get(&self, key: &PdfPageRangeCacheKey) -> Option<PdfPageRangeResult> {
        read_recover(&self.inner, PDF_PAGE_RANGE_CACHE_RESOURCE)
            .get(key)
            .cloned()
    }

    pub(crate) fn insert(&self, key: PdfPageRangeCacheKey, result: PdfPageRangeResult) {
        write_recover(&self.inner, PDF_PAGE_RANGE_CACHE_RESOURCE).insert(key, result);
    }

    pub(crate) fn clear(&self) {
        write_recover(&self.inner, PDF_PAGE_RANGE_CACHE_RESOURCE).clear();
    }
}
