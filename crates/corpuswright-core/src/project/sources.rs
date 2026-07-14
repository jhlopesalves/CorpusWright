//! Owns the versioned project-source manifest and every mutation of that persisted aggregate.
//!
//! Stable source IDs derive from project-relative identity. Queries preserve manifest order, and
//! updates write through one schema-aware boundary so import and processing state remain coherent.

use crate::corpus::preview::FilePreview;
use crate::preparation::cleaning::{PdfOcrQuality, PdfTextSource};
use crate::sources::pdf::audit::{PdfAuditQuality, PdfAuditSuggestedProfile, audit_pdf_files};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Current on-disk schema version for project source inventories.
pub const SOURCE_MANIFEST_VERSION: u32 = 1;
/// Metadata folder used by CorpusWright projects.
pub const PROJECT_METADATA_DIR: &str = ".corpuswright";
/// File name for the per-source project inventory.
pub const SOURCE_MANIFEST_FILE: &str = "sources.json";

/// Supported source categories tracked by the project source inventory.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectSourceKind {
    Pdf,
    Docx,
    Txt,
    Html,
    Unknown,
}

/// Processing stage or blocker currently known for a project source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectSourceStatus {
    Unprocessed,
    ReadyToProcess,
    NeedsOcr,
    TextExtracted,
    TextExtractedWithWarnings,
    Processed,
    ProcessedWithWarnings,
    Failed,
}

/// Extraction route chosen for a project source before cleaning runs.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectSourceExtractionRoute {
    EmbeddedText,
    OcrIfNeeded,
    ForceOcr,
    OcrRequired,
    PlainText,
    DocxExtract,
    HtmlExtract,
    Unsupported,
    #[default]
    Undecided,
}

/// Selected PDF handling for a PDF project source.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectSourcePdfProfile {
    #[default]
    Standard,
    LayoutHeavy,
    OcrRescue,
    ForceOcr,
    NoAutomaticOcr,
}

/// Batch PDF policy applied when confirming an import review.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProjectSourceImportPolicy {
    #[default]
    Recommended,
    OcrIfNeeded,
    ForceOcr,
    NoAutomaticOcr,
}

/// A single file registered with a CorpusWright project.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSource {
    pub id: String,
    pub path: PathBuf,
    pub file_name: String,
    pub extension: Option<String>,
    pub kind: ProjectSourceKind,
    pub status: ProjectSourceStatus,
    pub reason_codes: Vec<String>,
    pub extraction_route: ProjectSourceExtractionRoute,
    pub pdf_profile: Option<ProjectSourcePdfProfile>,
    pub ocr_quality: Option<PdfOcrQuality>,
    pub added_at: String,
    pub last_checked_at: Option<String>,
    pub last_processed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectSourceSerde {
    id: String,
    path: PathBuf,
    file_name: String,
    extension: Option<String>,
    kind: ProjectSourceKind,
    status: ProjectSourceStatus,
    reason_codes: Vec<String>,
    extraction_route: Option<ProjectSourceExtractionRoute>,
    pdf_profile: Option<ProjectSourcePdfProfile>,
    ocr_quality: Option<PdfOcrQuality>,
    added_at: String,
    last_checked_at: Option<String>,
    last_processed_at: Option<String>,
}

impl<'de> Deserialize<'de> for ProjectSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = ProjectSourceSerde::deserialize(deserializer)?;
        let extraction_route = helper
            .extraction_route
            .unwrap_or_else(|| default_extraction_route_for_kind(helper.kind));
        let legacy_pdf_handling =
            uses_legacy_mixed_ocr_pdf_handling(helper.kind, helper.pdf_profile, extraction_route);
        let (pdf_profile, extraction_route) =
            normalise_loaded_pdf_handling(helper.kind, helper.pdf_profile, extraction_route);
        let reason_codes = if legacy_pdf_handling {
            normalise_loaded_pdf_reason_codes(helper.reason_codes)
        } else {
            helper.reason_codes
        };

        Ok(Self {
            id: helper.id,
            path: helper.path,
            file_name: helper.file_name,
            extension: helper.extension,
            kind: helper.kind,
            status: helper.status,
            reason_codes,
            extraction_route,
            pdf_profile,
            ocr_quality: helper.ocr_quality,
            added_at: helper.added_at,
            last_checked_at: helper.last_checked_at,
            last_processed_at: helper.last_processed_at,
        })
    }
}

/// Versioned source inventory persisted inside project metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceManifest {
    pub version: u32,
    pub sources: Vec<ProjectSource>,
}

/// Counts project sources by their current processing status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceStatusCounts {
    pub unprocessed: usize,
    pub ready_to_process: usize,
    pub needs_ocr: usize,
    pub text_extracted: usize,
    pub text_extracted_with_warnings: usize,
    pub processed: usize,
    pub processed_with_warnings: usize,
    pub failed: usize,
}

/// Counts project sources by file kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceKindCounts {
    pub pdf: usize,
    pub docx: usize,
    pub txt: usize,
    pub html: usize,
    pub unknown: usize,
}

/// Aggregate information for the sources matched by an inventory query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceSummary {
    pub total_count: usize,
    pub loadable_count: usize,
    pub extracted_text_count: usize,
    pub processed_count: usize,
    pub warning_count: usize,
    pub status_counts: ProjectSourceStatusCounts,
    pub kind_counts: ProjectSourceKindCounts,
}

/// Bounded source inventory response for one query window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourcePage {
    pub total_count: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<ProjectSource>,
    pub summary: ProjectSourceSummary,
    pub inventory_summary: ProjectSourceSummary,
}

/// Compact description of the source set selected in the interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceSelectionDescriptor {
    pub all_matching: bool,
    pub query: String,
    pub ids: Vec<String>,
    pub excluded_ids: Vec<String>,
}

/// Bounded inventory window resolved from a compact source selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceSelectionViewport {
    pub total_selected: usize,
    pub viewport_start: usize,
    pub viewport_size: usize,
    pub items: Vec<ProjectSource>,
}

/// One source and its bounded text for a selected-source preview viewport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourcePreviewItem {
    pub source: ProjectSource,
    pub preview: Option<FilePreview>,
    pub preview_error: Option<String>,
}

/// Bounded preview response for a complete logical source selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourcePreviewViewport {
    pub total_selected: usize,
    pub viewport_start: usize,
    pub viewport_size: usize,
    pub items: Vec<ProjectSourcePreviewItem>,
}

/// Filters and bounds a project source inventory query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectSourceQuery {
    pub query: String,
    pub status_filter: Option<ProjectSourceStatus>,
    pub kind_filter: Option<ProjectSourceKind>,
    pub offset: usize,
    pub limit: usize,
}

/// Result of registering one or more paths with a project source inventory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceUpdate {
    pub manifest: ProjectSourceManifest,
    pub added_count: usize,
    pub duplicate_count: usize,
}

/// Source candidates inspected before they are persisted to a project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProjectSourceImportReview {
    pub candidates: Vec<ProjectSource>,
    pub duplicate_count: usize,
    pub ignored_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSourcePdfHandlingOverride {
    pub path: PathBuf,
    pub pdf_profile: ProjectSourcePdfProfile,
    pub ocr_quality: PdfOcrQuality,
}

/// Per-file result reported by the preprocessing preview path.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProjectSourceProcessingResult {
    pub path: PathBuf,
    pub failed: bool,
    pub warning_count: usize,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

/// Per-file result reported by source text extraction/materialisation.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProjectSourceExtractionResult {
    pub path: PathBuf,
    pub failed: bool,
    pub warning_count: usize,
    #[serde(default)]
    pub reason_codes: Vec<String>,
}

impl Default for ProjectSourceManifest {
    fn default() -> Self {
        Self {
            version: SOURCE_MANIFEST_VERSION,
            sources: Vec::new(),
        }
    }
}

/// Returns one bounded window from an in-memory project source inventory.
///
/// The summary describes every matching source, not only the returned window.
pub fn query_project_source_manifest(
    manifest: &ProjectSourceManifest,
    query: &ProjectSourceQuery,
) -> ProjectSourcePage {
    let query_text = query.query.trim().to_lowercase();
    let limit = query.limit.min(500);
    let mut summary = ProjectSourceSummary::default();
    let mut inventory_summary = ProjectSourceSummary::default();
    let mut items = Vec::with_capacity(limit);

    for source in &manifest.sources {
        update_project_source_summary(&mut inventory_summary, source);
        if query
            .status_filter
            .is_some_and(|status| source.status != status)
            || query.kind_filter.is_some_and(|kind| source.kind != kind)
            || !project_source_matches_query(source, &query_text)
        {
            continue;
        }

        let match_index = summary.total_count;
        update_project_source_summary(&mut summary, source);
        if match_index >= query.offset && items.len() < limit {
            items.push(source.clone());
        }
    }

    let offset = query.offset.min(summary.total_count);
    ProjectSourcePage {
        total_count: summary.total_count,
        offset,
        limit,
        items,
        summary,
        inventory_summary,
    }
}

/// Resolves one bounded viewport from a compact source selection.
///
/// The selected count covers the complete logical set, including items outside
/// the returned viewport.
pub fn query_project_source_selection(
    manifest: &ProjectSourceManifest,
    selection: &ProjectSourceSelectionDescriptor,
    viewport_start: usize,
    viewport_size: usize,
) -> ProjectSourceSelectionViewport {
    let query = selection.query.trim().to_lowercase();
    let ids = selection.ids.iter().collect::<HashSet<_>>();
    let excluded_ids = selection.excluded_ids.iter().collect::<HashSet<_>>();
    let viewport_size = viewport_size.min(50);
    let mut total_selected = 0;
    let mut items = Vec::with_capacity(viewport_size);

    for source in &manifest.sources {
        let selected = if selection.all_matching {
            !excluded_ids.contains(&source.id) && project_source_matches_query(source, &query)
        } else {
            ids.contains(&source.id)
        };
        if !selected {
            continue;
        }

        if total_selected >= viewport_start && items.len() < viewport_size {
            items.push(source.clone());
        }
        total_selected += 1;
    }

    ProjectSourceSelectionViewport {
        total_selected,
        viewport_start: viewport_start.min(total_selected),
        viewport_size,
        items,
    }
}

/// Returns whether a source matches the free-text part of an inventory query.
///
/// An empty query matches every source. Callers that already normalise input to
/// lowercase can reuse this predicate when resolving compact selections.
pub fn project_source_matches_query(source: &ProjectSource, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let status = match source.status {
        ProjectSourceStatus::Unprocessed => "unprocessed",
        ProjectSourceStatus::ReadyToProcess => "ready to process text available",
        ProjectSourceStatus::NeedsOcr => "needs ocr",
        ProjectSourceStatus::TextExtracted => "text extracted",
        ProjectSourceStatus::TextExtractedWithWarnings => "text extracted with warnings",
        ProjectSourceStatus::Processed => "processed",
        ProjectSourceStatus::ProcessedWithWarnings => "processed with warnings",
        ProjectSourceStatus::Failed => "failed",
    };
    let kind = match source.kind {
        ProjectSourceKind::Pdf => "pdf",
        ProjectSourceKind::Docx => "docx",
        ProjectSourceKind::Txt => "txt text native text",
        ProjectSourceKind::Html => "html",
        ProjectSourceKind::Unknown => "unknown",
    };
    let route = match source.extraction_route {
        ProjectSourceExtractionRoute::EmbeddedText => "embedded text",
        ProjectSourceExtractionRoute::OcrIfNeeded => "ocr if needed",
        ProjectSourceExtractionRoute::ForceOcr => "force ocr",
        ProjectSourceExtractionRoute::OcrRequired => "ocr required",
        ProjectSourceExtractionRoute::PlainText => "plain text",
        ProjectSourceExtractionRoute::DocxExtract => "docx extract",
        ProjectSourceExtractionRoute::HtmlExtract => "html extract",
        ProjectSourceExtractionRoute::Unsupported => "unsupported",
        ProjectSourceExtractionRoute::Undecided => "undecided",
    };

    source.file_name.to_lowercase().contains(query)
        || source.path.to_string_lossy().to_lowercase().contains(query)
        || source
            .extension
            .as_deref()
            .is_some_and(|extension| extension.to_lowercase().contains(query))
        || status.contains(query)
        || kind.contains(query)
        || route.contains(query)
        || source
            .reason_codes
            .iter()
            .any(|reason| reason.to_lowercase().replace('_', " ").contains(query))
}

fn update_project_source_summary(summary: &mut ProjectSourceSummary, source: &ProjectSource) {
    summary.total_count += 1;
    if source.kind != ProjectSourceKind::Unknown && source.status != ProjectSourceStatus::Failed {
        summary.loadable_count += 1;
    }
    if matches!(
        source.status,
        ProjectSourceStatus::TextExtracted
            | ProjectSourceStatus::TextExtractedWithWarnings
            | ProjectSourceStatus::Processed
            | ProjectSourceStatus::ProcessedWithWarnings
    ) {
        summary.extracted_text_count += 1;
    }
    if matches!(
        source.status,
        ProjectSourceStatus::Processed | ProjectSourceStatus::ProcessedWithWarnings
    ) {
        summary.processed_count += 1;
    }
    if matches!(
        source.status,
        ProjectSourceStatus::TextExtractedWithWarnings | ProjectSourceStatus::ProcessedWithWarnings
    ) || source.reason_codes.iter().any(|reason| {
        !matches!(
            reason.as_str(),
            "native_text_available"
                | "text_extracted"
                | "processed_ok"
                | "ocr_planned"
                | "force_ocr_planned"
        )
    }) {
        summary.warning_count += 1;
    }

    match source.status {
        ProjectSourceStatus::Unprocessed => summary.status_counts.unprocessed += 1,
        ProjectSourceStatus::ReadyToProcess => summary.status_counts.ready_to_process += 1,
        ProjectSourceStatus::NeedsOcr => summary.status_counts.needs_ocr += 1,
        ProjectSourceStatus::TextExtracted => summary.status_counts.text_extracted += 1,
        ProjectSourceStatus::TextExtractedWithWarnings => {
            summary.status_counts.text_extracted_with_warnings += 1;
        }
        ProjectSourceStatus::Processed => summary.status_counts.processed += 1,
        ProjectSourceStatus::ProcessedWithWarnings => {
            summary.status_counts.processed_with_warnings += 1;
        }
        ProjectSourceStatus::Failed => summary.status_counts.failed += 1,
    }
    match source.kind {
        ProjectSourceKind::Pdf => summary.kind_counts.pdf += 1,
        ProjectSourceKind::Docx => summary.kind_counts.docx += 1,
        ProjectSourceKind::Txt => summary.kind_counts.txt += 1,
        ProjectSourceKind::Html => summary.kind_counts.html += 1,
        ProjectSourceKind::Unknown => summary.kind_counts.unknown += 1,
    }
}

/// Returns the path to the persisted source inventory for a project root.
pub fn source_manifest_path(project_root: impl AsRef<Path>) -> PathBuf {
    project_root
        .as_ref()
        .join(PROJECT_METADATA_DIR)
        .join(SOURCE_MANIFEST_FILE)
}

/// Loads a project's source inventory, returning an empty manifest when none exists.
///
/// # Errors
///
/// Returns an error when the inventory cannot be read, parsed, or has an
/// unsupported schema version.
pub fn load_project_source_manifest(
    project_root: impl AsRef<Path>,
) -> Result<ProjectSourceManifest, String> {
    let manifest_path = source_manifest_path(project_root);
    if !manifest_path.exists() {
        return Ok(ProjectSourceManifest::default());
    }

    let content = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Could not read project source inventory: {error}"))?;
    let manifest: ProjectSourceManifest = serde_json::from_str(&content)
        .map_err(|error| format!("Project source inventory is not valid JSON: {error}"))?;
    if manifest.version != SOURCE_MANIFEST_VERSION {
        return Err(format!(
            "Unsupported CorpusWright source inventory version: {}.",
            manifest.version
        ));
    }
    Ok(manifest)
}

/// Persists a project's source inventory inside the metadata folder.
///
/// # Errors
///
/// Returns an error when the metadata directory cannot be created or the
/// inventory cannot be serialised or written.
pub fn save_project_source_manifest(
    project_root: impl AsRef<Path>,
    manifest: &ProjectSourceManifest,
) -> Result<(), String> {
    let manifest_path = source_manifest_path(project_root);
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create project metadata folder: {error}"))?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("Could not serialise project source inventory: {error}"))?;
    fs::write(&manifest_path, json)
        .map_err(|error| format!("Could not write project source inventory: {error}"))
}

/// Preflights paths and returns source candidates without writing the manifest.
///
/// # Errors
///
/// Returns an error when the existing inventory cannot be loaded.
pub fn review_project_source_paths(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
    ignored_count: usize,
) -> Result<ProjectSourceImportReview, String> {
    let project_root = project_root.as_ref();
    let manifest = load_project_source_manifest(project_root)?;
    Ok(build_project_source_import_review(
        project_root,
        &manifest,
        paths,
        ignored_count,
        None,
    ))
}

/// Registers paths while reporting how many candidates have completed preflight.
///
/// # Errors
///
/// Returns an error when the existing inventory cannot be loaded or the updated
/// inventory cannot be persisted.
pub fn add_project_source_paths_with_progress(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
    mut progress: impl FnMut(usize, usize),
) -> Result<ProjectSourceUpdate, String> {
    add_project_source_paths_with_pdf_handling_and_progress(
        project_root.as_ref(),
        paths,
        ProjectSourceImportPolicy::Recommended,
        PdfOcrQuality::Balanced,
        Vec::new(),
        Some(&mut progress),
    )
}

/// Registers paths as project sources, runs lightweight preflight, and skips duplicates.
///
/// # Errors
///
/// Returns an error when the existing inventory cannot be loaded or the updated
/// inventory cannot be persisted.
pub fn add_project_source_paths(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
) -> Result<ProjectSourceUpdate, String> {
    add_project_source_paths_with_policy(
        project_root,
        paths,
        ProjectSourceImportPolicy::Recommended,
        PdfOcrQuality::Balanced,
    )
}

/// Registers paths as project sources with a batch PDF import policy.
///
/// # Errors
///
/// Returns an error when the existing inventory cannot be loaded or the updated
/// inventory cannot be persisted.
pub fn add_project_source_paths_with_policy(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
    pdf_policy: ProjectSourceImportPolicy,
    ocr_quality: PdfOcrQuality,
) -> Result<ProjectSourceUpdate, String> {
    add_project_source_paths_with_pdf_handling(
        project_root,
        paths,
        pdf_policy,
        ocr_quality,
        Vec::new(),
    )
}

/// Registers paths as project sources with source-level PDF handling choices.
///
/// # Errors
///
/// Returns an error when the existing inventory cannot be loaded or the updated
/// inventory cannot be persisted.
pub fn add_project_source_paths_with_pdf_handling(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
    pdf_policy: ProjectSourceImportPolicy,
    ocr_quality: PdfOcrQuality,
    pdf_handling_overrides: Vec<ProjectSourcePdfHandlingOverride>,
) -> Result<ProjectSourceUpdate, String> {
    add_project_source_paths_with_pdf_handling_and_progress(
        project_root.as_ref(),
        paths,
        pdf_policy,
        ocr_quality,
        pdf_handling_overrides,
        None,
    )
}

fn add_project_source_paths_with_pdf_handling_and_progress(
    project_root: &Path,
    paths: Vec<PathBuf>,
    pdf_policy: ProjectSourceImportPolicy,
    ocr_quality: PdfOcrQuality,
    pdf_handling_overrides: Vec<ProjectSourcePdfHandlingOverride>,
    progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Result<ProjectSourceUpdate, String> {
    let mut manifest = load_project_source_manifest(project_root)?;
    let review = build_project_source_import_review(project_root, &manifest, paths, 0, progress);
    let duplicate_count = review.duplicate_count;
    let added_count = review.candidates.len();
    let handling_by_source_id = pdf_handling_overrides
        .into_iter()
        .map(|pdf_handling| {
            let source_path = source_path_for_storage(project_root, &pdf_handling.path);
            let source_id = stable_project_source_id(project_root, &source_path);
            (source_id, pdf_handling)
        })
        .collect::<HashMap<_, _>>();

    for mut source in review.candidates {
        let source_id = source.id.clone();
        let pdf_handling = handling_by_source_id.get(&source_id);
        apply_import_policy(&mut source, pdf_policy, ocr_quality, pdf_handling);
        manifest.sources.push(source);
    }

    manifest.sources.sort_by(|a, b| a.path.cmp(&b.path));
    save_project_source_manifest(project_root, &manifest)?;

    Ok(ProjectSourceUpdate {
        manifest,
        added_count,
        duplicate_count,
    })
}

/// Applies per-file preprocessing results to the persisted source inventory.
///
/// # Errors
///
/// Returns an error when the inventory cannot be loaded or saved.
pub fn update_project_source_processing_results(
    project_root: impl AsRef<Path>,
    results: Vec<ProjectSourceProcessingResult>,
) -> Result<ProjectSourceManifest, String> {
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;
    let now = current_timestamp();

    for result in results {
        let source_path = source_path_for_storage(project_root, &result.path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if let Some(source) = manifest
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
        {
            source.last_processed_at = Some(now.clone());
            if result.failed {
                source.status = ProjectSourceStatus::Failed;
                source.reason_codes = non_empty_or_default(result.reason_codes, "parse_failed");
            } else {
                let mut concrete_reasons: Vec<String> = result
                    .reason_codes
                    .into_iter()
                    .filter(|code| {
                        code != "text_extracted_with_warnings"
                            && code != "processed_with_warnings"
                            && code != "review_recommended"
                            && code != "text_extracted"
                            && code != "processed_ok"
                    })
                    .collect();

                if let Some(reason_code) = successful_pdf_review_reason(source) {
                    append_reason_code_once(&mut concrete_reasons, reason_code);
                }

                if !concrete_reasons.is_empty() {
                    source.status = ProjectSourceStatus::ProcessedWithWarnings;
                    source.reason_codes = concrete_reasons;
                } else {
                    source.status = ProjectSourceStatus::Processed;
                    source.reason_codes = Vec::new();
                }
            }
        }
    }

    save_project_source_manifest(project_root, &manifest)?;
    Ok(manifest)
}

/// Applies per-file source text extraction results to the persisted source inventory.
///
/// # Errors
///
/// Returns an error when the inventory cannot be loaded or saved.
pub fn update_project_source_extraction_results(
    project_root: impl AsRef<Path>,
    results: Vec<ProjectSourceExtractionResult>,
) -> Result<ProjectSourceManifest, String> {
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;
    let now = current_timestamp();

    for result in results {
        let source_path = source_path_for_storage(project_root, &result.path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if let Some(source) = manifest
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
        {
            source.last_checked_at = Some(now.clone());
            if result.failed {
                source.status = ProjectSourceStatus::Failed;
                source.reason_codes =
                    non_empty_or_default(result.reason_codes, "text_extraction_failed");
            } else {
                let mut concrete_reasons: Vec<String> = result
                    .reason_codes
                    .into_iter()
                    .filter(|code| {
                        code != "text_extracted_with_warnings"
                            && code != "processed_with_warnings"
                            && code != "review_recommended"
                            && code != "text_extracted"
                            && code != "processed_ok"
                    })
                    .collect();

                if let Some(reason_code) = successful_pdf_review_reason(source) {
                    append_reason_code_once(&mut concrete_reasons, reason_code);
                }

                if !concrete_reasons.is_empty() {
                    source.status = ProjectSourceStatus::TextExtractedWithWarnings;
                    source.reason_codes = concrete_reasons;
                } else {
                    source.status = ProjectSourceStatus::TextExtracted;
                    source.reason_codes = Vec::new();
                }
            }
        }
    }

    save_project_source_manifest(project_root, &manifest)?;
    Ok(manifest)
}

/// Updates the PDF profile and corresponding extraction route for specific sources.
///
/// # Errors
///
/// Returns an error when the inventory cannot be loaded or saved.
pub fn update_project_source_pdf_profiles(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
    profile: ProjectSourcePdfProfile,
    ocr_quality: PdfOcrQuality,
) -> Result<ProjectSourceManifest, String> {
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;
    let now = current_timestamp();

    for path in paths {
        let source_path = source_path_for_storage(project_root, &path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if let Some(source) = manifest
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .filter(|source| source.kind == ProjectSourceKind::Pdf)
        {
            source.pdf_profile = Some(profile);
            source.extraction_route = profile.extraction_route();
            source.ocr_quality = Some(ocr_quality);
            source.last_checked_at = Some(now.clone());
            set_pdf_source_planned_status_for_profile(source, profile);
        }
    }

    save_project_source_manifest(project_root, &manifest)?;
    Ok(manifest)
}

/// Updates stored per-source PDF handling from per-source overrides,
/// persisting each source's pdf_profile and ocr_quality individually.
///
/// Unlike `update_project_source_pdf_profiles` which applies a single profile
/// to all selected paths, this function processes per-source overrides so that
/// row-level handling changes are preserved during re-extract.
///
/// # Errors
///
/// Returns an error when the inventory cannot be loaded or saved.
pub fn update_project_source_pdf_handling(
    project_root: impl AsRef<Path>,
    overrides: Vec<ProjectSourcePdfHandlingOverride>,
) -> Result<ProjectSourceManifest, String> {
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;
    let now = current_timestamp();

    for pdf_override in overrides {
        let source_path = source_path_for_storage(project_root, &pdf_override.path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if let Some(source) = manifest
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .filter(|source| source.kind == ProjectSourceKind::Pdf)
        {
            source.pdf_profile = Some(pdf_override.pdf_profile);
            source.extraction_route = pdf_override.pdf_profile.extraction_route();
            source.ocr_quality = Some(pdf_override.ocr_quality);
            source.last_checked_at = Some(now.clone());
            set_pdf_source_planned_status_for_profile(source, pdf_override.pdf_profile);
        }
    }

    save_project_source_manifest(project_root, &manifest)?;
    Ok(manifest)
}

/// Removes specific source paths from the project inventory without touching disk files.
///
/// # Errors
///
/// Returns an error when the inventory cannot be loaded or saved.
pub fn remove_project_source_paths(
    project_root: impl AsRef<Path>,
    paths: Vec<PathBuf>,
) -> Result<ProjectSourceManifest, String> {
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;

    let remove_ids: HashSet<String> = paths
        .iter()
        .map(|path| {
            let source_path = source_path_for_storage(project_root, path);
            stable_project_source_id(project_root, &source_path)
        })
        .collect();

    manifest
        .sources
        .retain(|source| !remove_ids.contains(&source.id));

    save_project_source_manifest(project_root, &manifest)?;
    Ok(manifest)
}

/// Recomputes a source's kind, availability, and initial processing route.
pub fn preflight_project_source(source: &mut ProjectSource) {
    source.file_name = file_name_for_path(&source.path);
    source.extension = extension_for_path(&source.path);
    source.kind = kind_for_path(&source.path);
    source.last_checked_at = Some(current_timestamp());

    if !source.path.exists() {
        source.extraction_route = default_extraction_route_for_kind(source.kind);
        set_source_status(source, ProjectSourceStatus::Failed, &["missing_file"]);
        return;
    }

    if !source.path.is_file() {
        source.extraction_route = ProjectSourceExtractionRoute::Unsupported;
        set_source_status(
            source,
            ProjectSourceStatus::Failed,
            &["unsupported_file_type"],
        );
        return;
    }

    if source.kind == ProjectSourceKind::Unknown {
        source.extraction_route = ProjectSourceExtractionRoute::Unsupported;
        set_source_status(
            source,
            ProjectSourceStatus::Failed,
            &["unsupported_file_type"],
        );
        return;
    }

    match fs::File::open(&source.path) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            source.extraction_route = default_extraction_route_for_kind(source.kind);
            set_source_status(source, ProjectSourceStatus::Failed, &["permission_denied"]);
            return;
        }
        Err(_) => {
            source.extraction_route = default_extraction_route_for_kind(source.kind);
            set_source_status(source, ProjectSourceStatus::Failed, &["parse_failed"]);
            return;
        }
    }

    if source.kind == ProjectSourceKind::Pdf {
        preflight_pdf_source(source);
    } else {
        source.extraction_route = default_extraction_route_for_kind(source.kind);
        set_source_status(
            source,
            ProjectSourceStatus::ReadyToProcess,
            &["native_text_available"],
        );
    }
}

/// Builds a stable source ID from the project-relative path when possible.
pub fn stable_project_source_id(
    project_root: impl AsRef<Path>,
    source_path: impl AsRef<Path>,
) -> String {
    let project_root = canonical_or_original(project_root.as_ref().to_path_buf());
    let source_path = source_path_for_storage(&project_root, source_path.as_ref());
    let basis = match source_path.strip_prefix(&project_root) {
        Ok(relative) => format!("project:{}", normalise_path_for_id(relative)),
        Err(_) => format!("absolute:{}", normalise_path_for_id(&source_path)),
    };

    let mut hasher = Sha256::new();
    hasher.update(basis.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("source_{}", &digest[..24])
}

/// Classifies a path by its extension without opening the file.
pub fn kind_for_path(path: impl AsRef<Path>) -> ProjectSourceKind {
    match extension_for_path(path).as_deref() {
        Some("pdf") => ProjectSourceKind::Pdf,
        Some("docx") => ProjectSourceKind::Docx,
        Some("txt") => ProjectSourceKind::Txt,
        Some("html" | "htm") => ProjectSourceKind::Html,
        _ => ProjectSourceKind::Unknown,
    }
}

impl ProjectSourceExtractionRoute {
    /// Returns the PDF text source implied by this route, if it applies to PDF extraction.
    pub fn pdf_text_source(self) -> Option<PdfTextSource> {
        match self {
            Self::EmbeddedText => Some(PdfTextSource::EmbeddedText),
            Self::OcrIfNeeded | Self::OcrRequired => Some(PdfTextSource::Ocr),
            Self::ForceOcr => Some(PdfTextSource::ForceOcr),
            Self::PlainText
            | Self::DocxExtract
            | Self::HtmlExtract
            | Self::Unsupported
            | Self::Undecided => None,
        }
    }
}

impl ProjectSourcePdfProfile {
    pub fn extraction_route(self) -> ProjectSourceExtractionRoute {
        match self {
            Self::Standard | Self::LayoutHeavy | Self::NoAutomaticOcr => {
                ProjectSourceExtractionRoute::EmbeddedText
            }
            Self::OcrRescue | Self::ForceOcr => ProjectSourceExtractionRoute::ForceOcr,
        }
    }
}

fn normalise_loaded_pdf_handling(
    kind: ProjectSourceKind,
    pdf_profile: Option<ProjectSourcePdfProfile>,
    extraction_route: ProjectSourceExtractionRoute,
) -> (
    Option<ProjectSourcePdfProfile>,
    ProjectSourceExtractionRoute,
) {
    if kind != ProjectSourceKind::Pdf {
        return (pdf_profile, extraction_route);
    }

    match pdf_profile {
        Some(ProjectSourcePdfProfile::OcrRescue) => (
            Some(ProjectSourcePdfProfile::ForceOcr),
            ProjectSourceExtractionRoute::ForceOcr,
        ),
        Some(ProjectSourcePdfProfile::ForceOcr) => (
            Some(ProjectSourcePdfProfile::ForceOcr),
            ProjectSourceExtractionRoute::ForceOcr,
        ),
        Some(profile) => (Some(profile), extraction_route),
        None => match extraction_route {
            ProjectSourceExtractionRoute::OcrIfNeeded
            | ProjectSourceExtractionRoute::OcrRequired => (
                Some(ProjectSourcePdfProfile::ForceOcr),
                ProjectSourceExtractionRoute::ForceOcr,
            ),
            _ => (None, extraction_route),
        },
    }
}

fn uses_legacy_mixed_ocr_pdf_handling(
    kind: ProjectSourceKind,
    pdf_profile: Option<ProjectSourcePdfProfile>,
    extraction_route: ProjectSourceExtractionRoute,
) -> bool {
    kind == ProjectSourceKind::Pdf
        && (matches!(pdf_profile, Some(ProjectSourcePdfProfile::OcrRescue))
            || matches!(
                (pdf_profile, extraction_route),
                (
                    None,
                    ProjectSourceExtractionRoute::OcrIfNeeded
                        | ProjectSourceExtractionRoute::OcrRequired
                )
            ))
}

fn normalise_loaded_pdf_reason_codes(reason_codes: Vec<String>) -> Vec<String> {
    reason_codes
        .into_iter()
        .map(|code| match code.as_str() {
            "ocr_planned" | "ocr_required" => "force_ocr_planned".to_string(),
            "ocr_review_recommended" => "ocr_generated".to_string(),
            _ => code,
        })
        .collect()
}

pub fn default_extraction_route_for_kind(kind: ProjectSourceKind) -> ProjectSourceExtractionRoute {
    match kind {
        ProjectSourceKind::Pdf => ProjectSourceExtractionRoute::EmbeddedText,
        ProjectSourceKind::Docx => ProjectSourceExtractionRoute::DocxExtract,
        ProjectSourceKind::Txt => ProjectSourceExtractionRoute::PlainText,
        ProjectSourceKind::Html => ProjectSourceExtractionRoute::HtmlExtract,
        ProjectSourceKind::Unknown => ProjectSourceExtractionRoute::Unsupported,
    }
}

fn build_project_source_import_review(
    project_root: &Path,
    manifest: &ProjectSourceManifest,
    paths: Vec<PathBuf>,
    ignored_count: usize,
    mut progress: Option<&mut dyn FnMut(usize, usize)>,
) -> ProjectSourceImportReview {
    let mut seen_ids: HashSet<String> = manifest
        .sources
        .iter()
        .map(|source| source.id.clone())
        .collect();
    let mut candidates = Vec::new();
    let mut duplicate_count = 0usize;

    let total = paths.len();
    for (index, path) in paths.into_iter().enumerate() {
        let source_path = source_path_for_storage(project_root, &path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if seen_ids.insert(source_id.clone()) {
            let mut source = new_project_source(source_id, source_path);
            preflight_project_source(&mut source);
            candidates.push(source);
        } else {
            duplicate_count += 1;
        }

        let current = index + 1;
        if (current % 250 == 0 || current == total)
            && let Some(report) = progress.as_deref_mut()
        {
            report(current, total);
        }
    }

    candidates.sort_by(|a, b| a.path.cmp(&b.path));

    ProjectSourceImportReview {
        candidates,
        duplicate_count,
        ignored_count,
    }
}

fn new_project_source(source_id: String, source_path: PathBuf) -> ProjectSource {
    let kind = kind_for_path(&source_path);
    ProjectSource {
        id: source_id,
        path: source_path.clone(),
        file_name: file_name_for_path(&source_path),
        extension: extension_for_path(&source_path),
        kind,
        status: ProjectSourceStatus::Unprocessed,
        reason_codes: Vec::new(),
        extraction_route: default_extraction_route_for_kind(kind),
        pdf_profile: None,
        ocr_quality: None,
        added_at: current_timestamp(),
        last_checked_at: None,
        last_processed_at: None,
    }
}

fn apply_import_policy(
    source: &mut ProjectSource,
    pdf_policy: ProjectSourceImportPolicy,
    ocr_quality: PdfOcrQuality,
    pdf_handling_override: Option<&ProjectSourcePdfHandlingOverride>,
) {
    if source.kind != ProjectSourceKind::Pdf || source.status == ProjectSourceStatus::Failed {
        return;
    }

    if let Some(pdf_handling) = pdf_handling_override {
        source.ocr_quality = Some(pdf_handling.ocr_quality);
        source.pdf_profile = Some(pdf_handling.pdf_profile);
        source.extraction_route = pdf_handling.pdf_profile.extraction_route();
        set_pdf_source_planned_status_for_profile(source, pdf_handling.pdf_profile);
    } else {
        source.ocr_quality = Some(ocr_quality);
        match pdf_policy {
            ProjectSourceImportPolicy::Recommended => {}
            ProjectSourceImportPolicy::OcrIfNeeded | ProjectSourceImportPolicy::ForceOcr => {
                source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
                source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
                set_pdf_source_planned_status_for_profile(
                    source,
                    ProjectSourcePdfProfile::ForceOcr,
                );
            }
            ProjectSourceImportPolicy::NoAutomaticOcr => {
                source.pdf_profile = Some(ProjectSourcePdfProfile::NoAutomaticOcr);
                source.extraction_route = ProjectSourceExtractionRoute::EmbeddedText;
                set_pdf_source_planned_status_for_profile(
                    source,
                    ProjectSourcePdfProfile::NoAutomaticOcr,
                );
            }
        };
    }
}

fn set_pdf_source_planned_status_for_profile(
    source: &mut ProjectSource,
    profile: ProjectSourcePdfProfile,
) {
    match profile {
        ProjectSourcePdfProfile::OcrRescue | ProjectSourcePdfProfile::ForceOcr => {
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["force_ocr_planned"],
            );
        }
        ProjectSourcePdfProfile::Standard
        | ProjectSourcePdfProfile::LayoutHeavy
        | ProjectSourcePdfProfile::NoAutomaticOcr => {
            set_source_status(
                source,
                ProjectSourceStatus::ReadyToProcess,
                &["native_text_available"],
            );
        }
    }
}

fn successful_pdf_review_reason(source: &ProjectSource) -> Option<&'static str> {
    if source.kind != ProjectSourceKind::Pdf {
        return None;
    }

    match source.pdf_profile {
        Some(ProjectSourcePdfProfile::LayoutHeavy) => Some("layout_heavy_profile"),
        Some(ProjectSourcePdfProfile::OcrRescue | ProjectSourcePdfProfile::ForceOcr) => {
            Some("ocr_generated")
        }
        Some(ProjectSourcePdfProfile::Standard | ProjectSourcePdfProfile::NoAutomaticOcr) => None,
        None => match source.extraction_route {
            ProjectSourceExtractionRoute::OcrRequired | ProjectSourceExtractionRoute::ForceOcr => {
                Some("ocr_review_recommended")
            }
            _ => None,
        },
    }
}

fn preflight_pdf_source(source: &mut ProjectSource) {
    let results = audit_pdf_files(vec![source.path.clone()]);
    let Some(result) = results.into_iter().next() else {
        source.extraction_route = ProjectSourceExtractionRoute::Undecided;
        set_source_status(source, ProjectSourceStatus::Failed, &["pdf_open_failed"]);
        return;
    };

    let warning_text = result.warnings.join("\n").to_lowercase();
    if warning_text.contains("encrypted") || warning_text.contains("password") {
        source.extraction_route = ProjectSourceExtractionRoute::Undecided;
        set_source_status(source, ProjectSourceStatus::Failed, &["pdf_encrypted"]);
        return;
    }

    if warning_text.contains("could not parse pdf") || warning_text.contains("could not read pdf") {
        source.extraction_route = ProjectSourceExtractionRoute::Undecided;
        set_source_status(source, ProjectSourceStatus::Failed, &["pdf_open_failed"]);
        return;
    }

    apply_pdf_audit_result_to_source(
        source,
        result.quality,
        result.suggested_profile,
        result.embedded_text_detected,
    );
}

fn apply_pdf_audit_result_to_source(
    source: &mut ProjectSource,
    quality: PdfAuditQuality,
    suggested_profile: PdfAuditSuggestedProfile,
    embedded_text_detected: bool,
) {
    match quality {
        PdfAuditQuality::Good => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
            source.extraction_route = ProjectSourceExtractionRoute::EmbeddedText;
            set_source_status(
                source,
                ProjectSourceStatus::ReadyToProcess,
                &["native_text_available"],
            );
        }
        PdfAuditQuality::Suspicious => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
            source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["force_ocr_planned", "low_text_density"],
            );
        }
        PdfAuditQuality::Poor => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
            source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["force_ocr_planned", "low_text_density"],
            );
        }
        PdfAuditQuality::Empty => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
            source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["pdf_no_text_layer", "force_ocr_planned"],
            );
        }
        PdfAuditQuality::Unknown => {
            if matches!(suggested_profile, PdfAuditSuggestedProfile::ForceOcr)
                || !embedded_text_detected
            {
                source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
                source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
                set_source_status(
                    source,
                    ProjectSourceStatus::NeedsOcr,
                    &["pdf_no_text_layer", "force_ocr_planned"],
                );
            } else {
                source.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
                source.extraction_route = ProjectSourceExtractionRoute::Undecided;
                set_source_status(
                    source,
                    ProjectSourceStatus::ReadyToProcess,
                    &["pdf_route_undecided"],
                );
            }
        }
    }
}

fn set_source_status(
    source: &mut ProjectSource,
    status: ProjectSourceStatus,
    reason_codes: &[&str],
) {
    source.status = status;
    source.reason_codes = reason_codes
        .iter()
        .map(|code| (*code).to_string())
        .collect();
}

fn non_empty_or_default(mut reason_codes: Vec<String>, default_code: &str) -> Vec<String> {
    reason_codes.retain(|code| !code.trim().is_empty());
    if reason_codes.is_empty() {
        vec![default_code.to_string()]
    } else {
        reason_codes
    }
}

fn append_reason_code_once(reason_codes: &mut Vec<String>, reason_code: &str) {
    if !reason_codes.iter().any(|code| code == reason_code) {
        reason_codes.push(reason_code.to_string());
    }
}

fn current_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn canonical_or_original(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn source_path_for_storage(project_root: &Path, source_path: &Path) -> PathBuf {
    let path = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        project_root.join(source_path)
    };
    canonical_or_original(path)
}

fn normalise_path_for_id(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_name_for_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn extension_for_path(path: impl AsRef<Path>) -> Option<String> {
    path.as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn source_for_test(path: PathBuf, kind: ProjectSourceKind) -> ProjectSource {
        ProjectSource {
            id: "source_test".to_string(),
            path: path.clone(),
            file_name: file_name_for_path(&path),
            extension: extension_for_path(&path),
            kind,
            status: ProjectSourceStatus::ReadyToProcess,
            reason_codes: vec!["native_text_available".to_string()],
            extraction_route: default_extraction_route_for_kind(kind),
            pdf_profile: None,
            ocr_quality: None,
            added_at: "2026-01-01T00:00:00Z".to_string(),
            last_checked_at: Some("2026-01-01T00:00:00Z".to_string()),
            last_processed_at: None,
        }
    }

    #[test]
    fn source_query_returns_bounded_window_and_full_match_summary() {
        let sources = (0..1_000)
            .map(|index| {
                let kind = if index % 2 == 0 {
                    ProjectSourceKind::Txt
                } else {
                    ProjectSourceKind::Html
                };
                source_for_test(PathBuf::from(format!("document-{index:04}.txt")), kind)
            })
            .collect();
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources,
        };

        let page = query_project_source_manifest(
            &manifest,
            &ProjectSourceQuery {
                query: "document".to_string(),
                kind_filter: Some(ProjectSourceKind::Txt),
                offset: 200,
                limit: 75,
                ..ProjectSourceQuery::default()
            },
        );

        assert_eq!(page.total_count, 500);
        assert_eq!(page.offset, 200);
        assert_eq!(page.items.len(), 75);
        assert_eq!(page.summary.kind_counts.txt, 500);
        assert_eq!(page.summary.kind_counts.html, 0);
        assert_eq!(page.summary.status_counts.ready_to_process, 500);
        assert_eq!(page.summary.warning_count, 0);
        assert_eq!(page.inventory_summary.total_count, 1_000);
    }

    #[test]
    fn source_query_matches_native_text_status_language() {
        let source = source_for_test(PathBuf::from("notes.txt"), ProjectSourceKind::Txt);
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources: vec![source],
        };

        let page = query_project_source_manifest(
            &manifest,
            &ProjectSourceQuery {
                query: "text available".to_string(),
                limit: 20,
                ..ProjectSourceQuery::default()
            },
        );

        assert_eq!(page.total_count, 1);
        assert_eq!(page.items[0].kind, ProjectSourceKind::Txt);
    }

    #[test]
    fn source_selection_query_keeps_full_count_with_bounded_items() {
        let sources = (0..50_000)
            .map(|index| {
                let mut source = source_for_test(
                    PathBuf::from(format!("Linha_{index}.txt")),
                    ProjectSourceKind::Txt,
                );
                source.id = format!("source-{index}");
                source
            })
            .collect();
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources,
        };
        let excluded_id = manifest.sources[10].id.clone();

        let viewport = query_project_source_selection(
            &manifest,
            &ProjectSourceSelectionDescriptor {
                all_matching: true,
                query: "linha".to_string(),
                excluded_ids: vec![excluded_id],
                ..ProjectSourceSelectionDescriptor::default()
            },
            40_000,
            12,
        );

        assert_eq!(viewport.total_selected, 49_999);
        assert_eq!(viewport.viewport_start, 40_000);
        assert_eq!(viewport.viewport_size, 12);
        assert_eq!(viewport.items.len(), 12);
        assert_eq!(viewport.items[0].file_name, "Linha_40001.txt");
    }

    #[test]
    fn source_registration_reports_preflight_progress() {
        let temp = tempdir().unwrap();
        let project_root = temp.path().join("project");
        let source_root = temp.path().join("sources");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&source_root).unwrap();
        let paths = (0..3)
            .map(|index| {
                let path = source_root.join(format!("source-{index}.txt"));
                fs::write(&path, format!("synthetic source {index}")).unwrap();
                path
            })
            .collect::<Vec<_>>();
        let mut progress = Vec::new();

        let update =
            add_project_source_paths_with_progress(&project_root, paths, |current, total| {
                progress.push((current, total))
            })
            .unwrap();

        assert_eq!(update.added_count, 3);
        assert_eq!(progress, vec![(3, 3)]);
        assert!(update.manifest.sources.iter().all(|source| {
            source.kind == ProjectSourceKind::Txt
                && source.status == ProjectSourceStatus::ReadyToProcess
        }));
    }

    #[test]
    fn source_manifest_can_be_saved_and_loaded() {
        let dir = tempdir().unwrap();
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources: Vec::new(),
        };

        save_project_source_manifest(dir.path(), &manifest).unwrap();
        let loaded = load_project_source_manifest(dir.path()).unwrap();

        assert_eq!(loaded, manifest);
        assert!(source_manifest_path(dir.path()).is_file());
    }

    #[test]
    fn source_manifest_persists_extraction_route_and_ocr_quality() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path, ProjectSourceKind::Pdf);
        source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
        source.ocr_quality = Some(PdfOcrQuality::HighQuality);
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources: vec![source],
        };

        save_project_source_manifest(dir.path(), &manifest).unwrap();
        let loaded = load_project_source_manifest(dir.path()).unwrap();

        assert_eq!(
            loaded.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(
            loaded.sources[0].ocr_quality,
            Some(PdfOcrQuality::HighQuality)
        );
    }

    #[test]
    fn pdf_profile_serializes_and_deserializes() {
        let json = serde_json::to_string(&ProjectSourcePdfProfile::LayoutHeavy).unwrap();
        let profile: ProjectSourcePdfProfile = serde_json::from_str("\"ocr_rescue\"").unwrap();

        assert_eq!(json, "\"layout_heavy\"");
        assert_eq!(profile, ProjectSourcePdfProfile::OcrRescue);
    }

    #[test]
    fn legacy_ocr_rescue_source_records_load_as_force_ocr() {
        let dir = tempdir().unwrap();
        let manifest_path = source_manifest_path(dir.path());
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(
            &manifest_path,
            r#"{
  "version": 1,
  "sources": [
    {
      "id": "source_pdf",
      "path": "scan.pdf",
      "file_name": "scan.pdf",
      "extension": "pdf",
      "kind": "pdf",
      "status": "needs_ocr",
      "reason_codes": ["ocr_planned"],
      "extraction_route": "ocr_required",
      "pdf_profile": "ocr_rescue",
      "ocr_quality": "Balanced",
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    }
  ]
}"#,
        )
        .unwrap();

        let loaded = load_project_source_manifest(dir.path()).unwrap();

        assert_eq!(
            loaded.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::ForceOcr)
        );
        assert_eq!(
            loaded.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(loaded.sources[0].reason_codes, vec!["force_ocr_planned"]);
    }

    #[test]
    fn legacy_mixed_ocr_routes_without_profile_load_as_force_ocr() {
        let dir = tempdir().unwrap();
        let manifest_path = source_manifest_path(dir.path());
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(
            &manifest_path,
            r#"{
  "version": 1,
  "sources": [
    {
      "id": "source_pdf",
      "path": "scan.pdf",
      "file_name": "scan.pdf",
      "extension": "pdf",
      "kind": "pdf",
      "status": "needs_ocr",
      "reason_codes": ["ocr_required"],
      "extraction_route": "ocr_if_needed",
      "ocr_quality": "Balanced",
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    }
  ]
}"#,
        )
        .unwrap();

        let loaded = load_project_source_manifest(dir.path()).unwrap();

        assert_eq!(
            loaded.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::ForceOcr)
        );
        assert_eq!(
            loaded.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(loaded.sources[0].reason_codes, vec!["force_ocr_planned"]);
    }

    #[test]
    fn legacy_ocr_rescue_review_reason_loads_as_generated_ocr() {
        assert_eq!(
            normalise_loaded_pdf_reason_codes(vec![
                "ocr_review_recommended".to_string(),
                "low_text_density".to_string(),
            ]),
            vec!["ocr_generated", "low_text_density"]
        );
    }

    #[test]
    fn source_manifest_persists_pdf_profile() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path, ProjectSourceKind::Pdf);
        source.pdf_profile = Some(ProjectSourcePdfProfile::LayoutHeavy);
        source.extraction_route = ProjectSourcePdfProfile::LayoutHeavy.extraction_route();
        let manifest = ProjectSourceManifest {
            version: SOURCE_MANIFEST_VERSION,
            sources: vec![source],
        };

        save_project_source_manifest(dir.path(), &manifest).unwrap();
        let loaded = load_project_source_manifest(dir.path()).unwrap();

        assert_eq!(
            loaded.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::LayoutHeavy)
        );
        assert_eq!(
            loaded.sources[0].extraction_route,
            ProjectSourceExtractionRoute::EmbeddedText
        );
    }

    #[test]
    fn old_source_manifest_without_routes_loads_with_kind_defaults() {
        let dir = tempdir().unwrap();
        let manifest_path = source_manifest_path(dir.path());
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(
            &manifest_path,
            r#"{
  "version": 1,
  "sources": [
    {
      "id": "source_pdf",
      "path": "paper.pdf",
      "file_name": "paper.pdf",
      "extension": "pdf",
      "kind": "pdf",
      "status": "ready_to_process",
      "reason_codes": ["native_text_available"],
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    },
    {
      "id": "source_txt",
      "path": "notes.txt",
      "file_name": "notes.txt",
      "extension": "txt",
      "kind": "txt",
      "status": "ready_to_process",
      "reason_codes": ["native_text_available"],
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    },
    {
      "id": "source_docx",
      "path": "essay.docx",
      "file_name": "essay.docx",
      "extension": "docx",
      "kind": "docx",
      "status": "ready_to_process",
      "reason_codes": ["native_text_available"],
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    },
    {
      "id": "source_html",
      "path": "page.html",
      "file_name": "page.html",
      "extension": "html",
      "kind": "html",
      "status": "ready_to_process",
      "reason_codes": ["native_text_available"],
      "added_at": "2026-01-01T00:00:00Z",
      "last_checked_at": null,
      "last_processed_at": null
    }
  ]
}"#,
        )
        .unwrap();

        let loaded = load_project_source_manifest(dir.path()).unwrap();
        let routes = loaded
            .sources
            .iter()
            .map(|source| source.extraction_route)
            .collect::<Vec<_>>();

        assert_eq!(
            routes,
            vec![
                ProjectSourceExtractionRoute::EmbeddedText,
                ProjectSourceExtractionRoute::PlainText,
                ProjectSourceExtractionRoute::DocxExtract,
                ProjectSourceExtractionRoute::HtmlExtract,
            ]
        );
        assert!(
            loaded
                .sources
                .iter()
                .all(|source| source.pdf_profile.is_none())
        );
    }

    #[test]
    fn pdf_profile_to_route_mapping_is_stable() {
        assert_eq!(
            ProjectSourcePdfProfile::Standard.extraction_route(),
            ProjectSourceExtractionRoute::EmbeddedText
        );
        assert_eq!(
            ProjectSourcePdfProfile::LayoutHeavy.extraction_route(),
            ProjectSourceExtractionRoute::EmbeddedText
        );
        assert_eq!(
            ProjectSourcePdfProfile::OcrRescue.extraction_route(),
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(
            ProjectSourcePdfProfile::ForceOcr.extraction_route(),
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(
            ProjectSourcePdfProfile::NoAutomaticOcr.extraction_route(),
            ProjectSourceExtractionRoute::EmbeddedText
        );
    }

    #[test]
    fn successful_pdf_preparation_uses_profile_status_mapping() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
        source.extraction_route = ProjectSourcePdfProfile::OcrRescue.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_processing_results(
            dir.path(),
            vec![ProjectSourceProcessingResult {
                path,
                failed: false,
                warning_count: 0,
                reason_codes: Vec::new(),
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::ProcessedWithWarnings
        );
        assert_eq!(manifest.sources[0].reason_codes, vec!["ocr_generated"]);
    }

    #[test]
    fn source_text_extraction_marks_text_extracted_not_processed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        source.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: false,
                warning_count: 0,
                reason_codes: Vec::new(),
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::TextExtracted
        );
        assert_eq!(manifest.sources[0].reason_codes, Vec::<String>::new());
        assert_eq!(manifest.sources[0].last_processed_at, None);
    }

    #[test]
    fn source_text_extraction_warnings_do_not_claim_processing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("scan.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
        source.extraction_route = ProjectSourcePdfProfile::ForceOcr.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: false,
                warning_count: 1,
                reason_codes: vec!["ocr_generated".to_string()],
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::TextExtractedWithWarnings
        );
        assert_eq!(manifest.sources[0].reason_codes, vec!["ocr_generated"]);
        assert_eq!(manifest.sources[0].last_processed_at, None);
    }

    #[test]
    fn source_text_extraction_no_concrete_warnings_does_not_show_warning_banner() {
        // Standard embedded text extraction with warning_count > 0 but no concrete
        // reason codes should NOT produce a generic warning — it should show as
        // plain TextExtracted with no warning banner.
        let dir = tempdir().unwrap();
        let path = dir.path().join("warning.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        source.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: false,
                warning_count: 1,
                reason_codes: Vec::new(),
            }],
        )
        .unwrap();

        // No concrete reason + Standard profile = TextExtracted, no warning
        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::TextExtracted
        );
        assert_eq!(manifest.sources[0].reason_codes, Vec::<String>::new());
    }

    #[test]
    fn source_text_extraction_warnings_keep_pdf_review_reason() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clamped.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
        source.extraction_route = ProjectSourcePdfProfile::ForceOcr.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: false,
                warning_count: 1,
                reason_codes: vec!["render_size_clamped".to_string()],
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::TextExtractedWithWarnings
        );
        assert_eq!(
            manifest.sources[0].reason_codes,
            vec!["render_size_clamped", "ocr_generated"]
        );
    }

    #[test]
    fn source_text_extraction_layout_profile_records_review_reason() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("layout.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::LayoutHeavy);
        source.extraction_route = ProjectSourcePdfProfile::LayoutHeavy.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: false,
                warning_count: 0,
                reason_codes: Vec::new(),
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::TextExtractedWithWarnings
        );
        assert_eq!(
            manifest.sources[0].reason_codes,
            vec!["layout_heavy_profile"]
        );
    }

    #[test]
    fn source_text_extraction_can_record_cancellation_reason() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cancelled.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
        source.extraction_route = ProjectSourcePdfProfile::ForceOcr.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_extraction_results(
            dir.path(),
            vec![ProjectSourceExtractionResult {
                path,
                failed: true,
                warning_count: 1,
                reason_codes: vec!["extraction_cancelled".to_string()],
            }],
        )
        .unwrap();

        assert_eq!(manifest.sources[0].status, ProjectSourceStatus::Failed);
        assert_eq!(
            manifest.sources[0].reason_codes,
            vec!["extraction_cancelled"]
        );
        assert_eq!(manifest.sources[0].last_processed_at, None);
    }

    #[test]
    fn mixed_pdf_processing_results_update_sources_independently() {
        let dir = tempdir().unwrap();
        let standard_path = dir.path().join("standard.pdf");
        let ocr_path = dir.path().join("ocr.pdf");
        let force_path = dir.path().join("force.pdf");

        let mut standard = source_for_test(standard_path.clone(), ProjectSourceKind::Pdf);
        standard.id = stable_project_source_id(dir.path(), &standard_path);
        standard.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        standard.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();

        let mut ocr = source_for_test(ocr_path.clone(), ProjectSourceKind::Pdf);
        ocr.id = stable_project_source_id(dir.path(), &ocr_path);
        ocr.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
        ocr.extraction_route = ProjectSourcePdfProfile::OcrRescue.extraction_route();

        let mut force = source_for_test(force_path.clone(), ProjectSourceKind::Pdf);
        force.id = stable_project_source_id(dir.path(), &force_path);
        force.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
        force.extraction_route = ProjectSourcePdfProfile::ForceOcr.extraction_route();

        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![standard, ocr, force],
            },
        )
        .unwrap();

        let manifest = update_project_source_processing_results(
            dir.path(),
            vec![
                ProjectSourceProcessingResult {
                    path: standard_path,
                    failed: false,
                    warning_count: 0,
                    reason_codes: Vec::new(),
                },
                ProjectSourceProcessingResult {
                    path: ocr_path,
                    failed: false,
                    warning_count: 0,
                    reason_codes: Vec::new(),
                },
                ProjectSourceProcessingResult {
                    path: force_path,
                    failed: false,
                    warning_count: 0,
                    reason_codes: Vec::new(),
                },
            ],
        )
        .unwrap();

        assert_eq!(manifest.sources[0].status, ProjectSourceStatus::Processed);
        assert_eq!(manifest.sources[0].reason_codes, Vec::<String>::new());
        assert_eq!(
            manifest.sources[1].status,
            ProjectSourceStatus::ProcessedWithWarnings
        );
        assert_eq!(manifest.sources[1].reason_codes, vec!["ocr_generated"]);
        assert_eq!(
            manifest.sources[2].status,
            ProjectSourceStatus::ProcessedWithWarnings
        );
        assert_eq!(manifest.sources[2].reason_codes, vec!["ocr_generated"]);
    }

    #[test]
    fn layout_heavy_preparation_preserves_profile_separately_from_route() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.pdf_profile = Some(ProjectSourcePdfProfile::LayoutHeavy);
        source.extraction_route = ProjectSourcePdfProfile::LayoutHeavy.extraction_route();
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let manifest = update_project_source_processing_results(
            dir.path(),
            vec![ProjectSourceProcessingResult {
                path,
                failed: false,
                warning_count: 0,
                reason_codes: Vec::new(),
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::LayoutHeavy)
        );
        assert_eq!(
            manifest.sources[0].extraction_route,
            ProjectSourceExtractionRoute::EmbeddedText
        );
        assert_eq!(
            manifest.sources[0].status,
            ProjectSourceStatus::ProcessedWithWarnings
        );
        assert_eq!(
            manifest.sources[0].reason_codes,
            vec!["layout_heavy_profile"]
        );
    }

    #[test]
    fn updating_pdf_profile_maps_legacy_ocr_rescue_to_force_ocr_route() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let ocr_rescue = update_project_source_pdf_profiles(
            dir.path(),
            vec![path.clone()],
            ProjectSourcePdfProfile::OcrRescue,
            PdfOcrQuality::Balanced,
        )
        .unwrap();

        assert_eq!(
            ocr_rescue.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::OcrRescue)
        );
        assert_eq!(
            ocr_rescue.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(ocr_rescue.sources[0].status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(
            ocr_rescue.sources[0].reason_codes,
            vec!["force_ocr_planned"]
        );

        let force_ocr = update_project_source_pdf_profiles(
            dir.path(),
            vec![path],
            ProjectSourcePdfProfile::ForceOcr,
            PdfOcrQuality::HighQuality,
        )
        .unwrap();

        assert_eq!(
            force_ocr.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::ForceOcr)
        );
        assert_eq!(
            force_ocr.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(force_ocr.sources[0].status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(force_ocr.sources[0].reason_codes, vec!["force_ocr_planned"]);
        assert_eq!(
            force_ocr.sources[0].ocr_quality,
            Some(PdfOcrQuality::HighQuality)
        );
    }

    #[test]
    fn imported_pdf_ocr_overrides_remain_unprocessed() {
        let path = PathBuf::from("paper.pdf");
        let mut ocr_source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        let ocr_override = ProjectSourcePdfHandlingOverride {
            path: path.clone(),
            pdf_profile: ProjectSourcePdfProfile::OcrRescue,
            ocr_quality: PdfOcrQuality::Balanced,
        };
        apply_import_policy(
            &mut ocr_source,
            ProjectSourceImportPolicy::Recommended,
            PdfOcrQuality::Fast,
            Some(&ocr_override),
        );

        assert_eq!(ocr_source.status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(ocr_source.reason_codes, vec!["force_ocr_planned"]);
        assert_eq!(ocr_source.last_processed_at, None);

        let mut force_source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        let force_override = ProjectSourcePdfHandlingOverride {
            path,
            pdf_profile: ProjectSourcePdfProfile::ForceOcr,
            ocr_quality: PdfOcrQuality::HighQuality,
        };
        apply_import_policy(
            &mut force_source,
            ProjectSourceImportPolicy::Recommended,
            PdfOcrQuality::Fast,
            Some(&force_override),
        );

        assert_eq!(force_source.status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(force_source.reason_codes, vec!["force_ocr_planned"]);
        assert_eq!(force_source.last_processed_at, None);
    }

    #[test]
    fn legacy_ocr_if_needed_import_policy_maps_to_force_ocr() {
        let mut source = source_for_test(PathBuf::from("paper.pdf"), ProjectSourceKind::Pdf);

        apply_import_policy(
            &mut source,
            ProjectSourceImportPolicy::OcrIfNeeded,
            PdfOcrQuality::Balanced,
            None,
        );

        assert_eq!(source.pdf_profile, Some(ProjectSourcePdfProfile::ForceOcr));
        assert_eq!(
            source.extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(source.reason_codes, vec!["force_ocr_planned"]);
    }

    #[test]
    fn pdf_audit_quality_maps_to_recommended_routes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut good = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        apply_pdf_audit_result_to_source(
            &mut good,
            PdfAuditQuality::Good,
            PdfAuditSuggestedProfile::Standard,
            true,
        );

        let mut scanned = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        apply_pdf_audit_result_to_source(
            &mut scanned,
            PdfAuditQuality::Empty,
            PdfAuditSuggestedProfile::ForceOcr,
            false,
        );

        let mut unclear = source_for_test(path, ProjectSourceKind::Pdf);
        apply_pdf_audit_result_to_source(
            &mut unclear,
            PdfAuditQuality::Unknown,
            PdfAuditSuggestedProfile::Standard,
            true,
        );

        assert_eq!(
            good.extraction_route,
            ProjectSourceExtractionRoute::EmbeddedText
        );
        assert_eq!(good.status, ProjectSourceStatus::ReadyToProcess);
        assert_eq!(
            scanned.extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(scanned.status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(
            unclear.extraction_route,
            ProjectSourceExtractionRoute::Undecided
        );
        assert_eq!(unclear.reason_codes, vec!["pdf_route_undecided"]);
    }

    #[test]
    fn duplicate_source_paths_are_deduplicated() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("essay.txt");
        fs::write(&source, "Synthetic text").unwrap();

        let update = add_project_source_paths(dir.path(), vec![source.clone(), source]).unwrap();

        assert_eq!(update.added_count, 1);
        assert_eq!(update.duplicate_count, 1);
        assert_eq!(update.manifest.sources.len(), 1);
    }

    #[test]
    fn duplicate_source_import_preserves_existing_route() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("paper.pdf");
        let mut source = source_for_test(path.clone(), ProjectSourceKind::Pdf);
        source.id = stable_project_source_id(dir.path(), &path);
        source.extraction_route = ProjectSourceExtractionRoute::ForceOcr;
        source.ocr_quality = Some(PdfOcrQuality::HighQuality);
        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source],
            },
        )
        .unwrap();

        let update = add_project_source_paths_with_policy(
            dir.path(),
            vec![path],
            ProjectSourceImportPolicy::NoAutomaticOcr,
            PdfOcrQuality::Fast,
        )
        .unwrap();

        assert_eq!(update.added_count, 0);
        assert_eq!(update.duplicate_count, 1);
        assert_eq!(
            update.manifest.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(
            update.manifest.sources[0].ocr_quality,
            Some(PdfOcrQuality::HighQuality)
        );
    }

    #[test]
    fn unsupported_extension_is_marked_failed() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("image.png");
        fs::write(&source, "not a corpus document").unwrap();

        let update = add_project_source_paths(dir.path(), vec![source]).unwrap();
        let source = &update.manifest.sources[0];

        assert_eq!(source.kind, ProjectSourceKind::Unknown);
        assert_eq!(source.status, ProjectSourceStatus::Failed);
        assert_eq!(
            source.extraction_route,
            ProjectSourceExtractionRoute::Unsupported
        );
        assert_eq!(source.reason_codes, vec!["unsupported_file_type"]);
    }

    #[test]
    fn missing_file_is_marked_failed() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("missing.txt");

        let update = add_project_source_paths(dir.path(), vec![source]).unwrap();
        let source = &update.manifest.sources[0];

        assert_eq!(source.status, ProjectSourceStatus::Failed);
        assert_eq!(
            source.extraction_route,
            ProjectSourceExtractionRoute::PlainText
        );
        assert_eq!(source.reason_codes, vec!["missing_file"]);
    }

    #[test]
    fn removing_failed_source_persists_after_reload() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("missing.pdf");

        let update = add_project_source_paths(dir.path(), vec![source.clone()]).unwrap();
        assert_eq!(
            update.manifest.sources[0].status,
            ProjectSourceStatus::Failed
        );

        let manifest = remove_project_source_paths(dir.path(), vec![source]).unwrap();
        assert!(manifest.sources.is_empty());

        let reloaded = load_project_source_manifest(dir.path()).unwrap();
        assert!(reloaded.sources.is_empty());
    }

    #[test]
    fn readable_txt_and_docx_paths_are_ready_to_process() {
        let dir = tempdir().unwrap();
        let txt = dir.path().join("essay.txt");
        let docx = dir.path().join("essay.docx");
        fs::write(&txt, "Synthetic text").unwrap();
        fs::write(&docx, "PK synthetic placeholder").unwrap();

        let update = add_project_source_paths(dir.path(), vec![txt, docx]).unwrap();

        assert_eq!(update.manifest.sources.len(), 2);
        assert!(update.manifest.sources.iter().all(|source| {
            source.status == ProjectSourceStatus::ReadyToProcess
                && source.reason_codes == vec!["native_text_available"]
        }));
        let routes = update
            .manifest
            .sources
            .iter()
            .map(|source| source.extraction_route)
            .collect::<Vec<_>>();
        assert!(routes.contains(&ProjectSourceExtractionRoute::PlainText));
        assert!(routes.contains(&ProjectSourceExtractionRoute::DocxExtract));
    }

    #[test]
    fn project_source_ids_are_stable_across_load_save() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("stable.txt");
        fs::write(&source, "Synthetic text").unwrap();

        let update = add_project_source_paths(dir.path(), vec![source.clone()]).unwrap();
        let first_id = update.manifest.sources[0].id.clone();
        let loaded = load_project_source_manifest(dir.path()).unwrap();
        let loaded_id = loaded.sources[0].id.clone();
        let recomputed_id = stable_project_source_id(dir.path(), source);

        assert_eq!(first_id, loaded_id);
        assert_eq!(first_id, recomputed_id);
    }

    #[test]
    fn update_project_source_pdf_handling_preserves_per_source_overrides() {
        let dir = tempdir().unwrap();
        let standard_path = dir.path().join("standard.pdf");
        let ocr_path = dir.path().join("ocr.pdf");

        let mut standard = source_for_test(standard_path.clone(), ProjectSourceKind::Pdf);
        standard.id = stable_project_source_id(dir.path(), &standard_path);
        standard.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        standard.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();

        let mut ocr = source_for_test(ocr_path.clone(), ProjectSourceKind::Pdf);
        ocr.id = stable_project_source_id(dir.path(), &ocr_path);
        ocr.pdf_profile = Some(ProjectSourcePdfProfile::ForceOcr);
        ocr.extraction_route = ProjectSourcePdfProfile::ForceOcr.extraction_route();

        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![standard, ocr],
            },
        )
        .unwrap();

        // Update only the standard source to layout_heavy, leave OCR source unchanged
        let manifest = update_project_source_pdf_handling(
            dir.path(),
            vec![ProjectSourcePdfHandlingOverride {
                path: standard_path,
                pdf_profile: ProjectSourcePdfProfile::LayoutHeavy,
                ocr_quality: PdfOcrQuality::Balanced,
            }],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::LayoutHeavy)
        );
        assert_eq!(
            manifest.sources[0].extraction_route,
            ProjectSourceExtractionRoute::EmbeddedText
        );
        // OCR source should be unchanged
        assert_eq!(
            manifest.sources[1].pdf_profile,
            Some(ProjectSourcePdfProfile::ForceOcr)
        );
        assert_eq!(
            manifest.sources[1].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
    }

    #[test]
    fn update_project_source_pdf_handling_allows_row_level_overrides() {
        let dir = tempdir().unwrap();
        let path_a = dir.path().join("a.pdf");
        let path_b = dir.path().join("b.pdf");

        let mut source_a = source_for_test(path_a.clone(), ProjectSourceKind::Pdf);
        source_a.id = stable_project_source_id(dir.path(), &path_a);
        source_a.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        source_a.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();

        let mut source_b = source_for_test(path_b.clone(), ProjectSourceKind::Pdf);
        source_b.id = stable_project_source_id(dir.path(), &path_b);
        source_b.pdf_profile = Some(ProjectSourcePdfProfile::Standard);
        source_b.extraction_route = ProjectSourcePdfProfile::Standard.extraction_route();

        save_project_source_manifest(
            dir.path(),
            &ProjectSourceManifest {
                version: SOURCE_MANIFEST_VERSION,
                sources: vec![source_a, source_b],
            },
        )
        .unwrap();

        // Override each source with a different profile
        let manifest = update_project_source_pdf_handling(
            dir.path(),
            vec![
                ProjectSourcePdfHandlingOverride {
                    path: path_a,
                    pdf_profile: ProjectSourcePdfProfile::ForceOcr,
                    ocr_quality: PdfOcrQuality::HighQuality,
                },
                ProjectSourcePdfHandlingOverride {
                    path: path_b,
                    pdf_profile: ProjectSourcePdfProfile::LayoutHeavy,
                    ocr_quality: PdfOcrQuality::Balanced,
                },
            ],
        )
        .unwrap();

        assert_eq!(
            manifest.sources[0].pdf_profile,
            Some(ProjectSourcePdfProfile::ForceOcr)
        );
        assert_eq!(
            manifest.sources[0].extraction_route,
            ProjectSourceExtractionRoute::ForceOcr
        );
        assert_eq!(
            manifest.sources[0].ocr_quality,
            Some(PdfOcrQuality::HighQuality)
        );

        assert_eq!(
            manifest.sources[1].pdf_profile,
            Some(ProjectSourcePdfProfile::LayoutHeavy)
        );
        assert_eq!(
            manifest.sources[1].extraction_route,
            ProjectSourceExtractionRoute::EmbeddedText
        );
        assert_eq!(
            manifest.sources[1].ocr_quality,
            Some(PdfOcrQuality::Balanced)
        );
    }
}
