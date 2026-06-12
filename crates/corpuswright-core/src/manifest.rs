use crate::clean::CleaningConfig;
use crate::scan::DocumentType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ManifestFileRecord {
    pub source_path: PathBuf,
    pub relative_path: PathBuf,
    pub document_type: DocumentType,
    pub output_path: PathBuf,
    #[ts(type = "number")]
    pub source_size_bytes: u64,
    pub original_char_count: usize,
    pub processed_char_count: usize,
    pub source_hash_sha256: String,
    pub processed_hash_sha256: String,
    pub warnings: Vec<String>,
    pub extraction_method: Option<String>,
    pub page_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ExportManifest {
    pub app_name: String,
    pub app_version: Option<String>,
    pub export_timestamp: String,
    pub files_exported: usize,
    pub warnings_count: usize,
    pub cleaning_config: CleaningConfig,
    pub files: Vec<ManifestFileRecord>,
}
