use crate::corpus::DocumentType;
use crate::preparation::cleaning::CleaningConfig;
use crate::sources::decode::{DecodeStrategy, SourceEncoding};
use crate::text::tokenisation::TokenizationManifest;
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
    #[serde(default = "crate::sources::decode::decode_policy_version")]
    pub decode_policy_version: u32,
    #[serde(default)]
    pub detected_encoding: SourceEncoding,
    #[serde(default)]
    pub decode_strategy: DecodeStrategy,
    #[serde(default)]
    pub replacement_char_count: u32,
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
    #[serde(default = "crate::sources::decode::decode_policy_version")]
    pub decode_policy_version: u32,
    #[serde(default)]
    pub tokenization: TokenizationManifest,
    pub files: Vec<ManifestFileRecord>,
}
