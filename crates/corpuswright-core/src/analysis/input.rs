use crate::corpus::DocumentRecord;
use crate::preparation::cleaning::CleaningConfig;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Text source used to construct an analysis index.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisInputSource {
    Processed,
    ProcessedWithSourceTextFallback,
    SourceTextFallback,
}

/// Provenance of the text offered to the analysis index for one source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisInputProvenance {
    Processed,
    SourceTextFallback,
    EmptyOrFailed,
}

/// Outcome of resolving one source for analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisInputStatus {
    Ready,
    ReadyWithFallback { processed_error: String },
    Empty,
    Failed { error: String },
}

/// One source resolved through the active processing pipeline for analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisInputResolution {
    pub text: Option<String>,
    pub provenance: AnalysisInputProvenance,
    pub source_id: String,
    pub source_name: String,
    pub status: AnalysisInputStatus,
}

/// Resolves one analysis input, preferring the active processed pipeline.
///
/// Source text is used only when processed text cannot be produced. An empty
/// processed result is intentionally excluded rather than replaced with raw
/// source text.
pub fn resolve_analysis_input<ProcessedLoader, SourceTextLoader>(
    record: &DocumentRecord,
    cleaning_config: &CleaningConfig,
    load_processed: ProcessedLoader,
    load_source_text: SourceTextLoader,
) -> AnalysisInputResolution
where
    ProcessedLoader: FnOnce(&DocumentRecord, &CleaningConfig) -> Result<String, String>,
    SourceTextLoader: FnOnce(&DocumentRecord, &CleaningConfig) -> Result<String, String>,
{
    let source_id = record.source_path.to_string_lossy().into_owned();
    let source_name = record.relative_path.to_string_lossy().into_owned();

    match load_processed(record, cleaning_config) {
        Ok(text) if !text.trim().is_empty() => AnalysisInputResolution {
            text: Some(text),
            provenance: AnalysisInputProvenance::Processed,
            source_id,
            source_name,
            status: AnalysisInputStatus::Ready,
        },
        Ok(_) => AnalysisInputResolution {
            text: None,
            provenance: AnalysisInputProvenance::EmptyOrFailed,
            source_id,
            source_name,
            status: AnalysisInputStatus::Empty,
        },
        Err(processed_error) => match load_source_text(record, cleaning_config) {
            Ok(text) if !text.trim().is_empty() => AnalysisInputResolution {
                text: Some(text),
                provenance: AnalysisInputProvenance::SourceTextFallback,
                source_id,
                source_name,
                status: AnalysisInputStatus::ReadyWithFallback { processed_error },
            },
            Ok(_) => AnalysisInputResolution {
                text: None,
                provenance: AnalysisInputProvenance::EmptyOrFailed,
                source_id,
                source_name,
                status: AnalysisInputStatus::Empty,
            },
            Err(source_error) => AnalysisInputResolution {
                text: None,
                provenance: AnalysisInputProvenance::EmptyOrFailed,
                source_id,
                source_name,
                status: AnalysisInputStatus::Failed {
                    error: format!(
                        "Processed text failed: {processed_error} Source text failed: {source_error}"
                    ),
                },
            },
        },
    }
}

/// Returns a stable signature for every active processing option and rule.
pub fn cleaning_config_signature(cleaning_config: &CleaningConfig) -> String {
    let bytes = serde_json::to_vec(cleaning_config)
        .expect("CleaningConfig serialization must remain infallible");
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
