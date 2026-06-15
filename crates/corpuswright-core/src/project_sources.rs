use crate::clean::{PdfOcrQuality, PdfTextSource};
use crate::pdf_audit::{PdfAuditQuality, PdfAuditSuggestedProfile, audit_pdf_files};
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
        Ok(Self {
            id: helper.id,
            path: helper.path,
            file_name: helper.file_name,
            extension: helper.extension,
            kind: helper.kind,
            status: helper.status,
            reason_codes: helper.reason_codes,
            extraction_route: helper
                .extraction_route
                .unwrap_or_else(|| default_extraction_route_for_kind(helper.kind)),
            pdf_profile: helper.pdf_profile,
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

impl Default for ProjectSourceManifest {
    fn default() -> Self {
        Self {
            version: SOURCE_MANIFEST_VERSION,
            sources: Vec::new(),
        }
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
    ))
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
    let project_root = project_root.as_ref();
    let mut manifest = load_project_source_manifest(project_root)?;
    let review = build_project_source_import_review(project_root, &manifest, paths, 0);
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
            } else if result.warning_count > 0 {
                source.status = ProjectSourceStatus::ProcessedWithWarnings;
                source.reason_codes =
                    non_empty_or_default(result.reason_codes, "processed_with_warnings");
            } else if let Some(reason_code) = successful_pdf_review_reason(source) {
                source.status = ProjectSourceStatus::ProcessedWithWarnings;
                source.reason_codes = vec![reason_code.to_string()];
            } else {
                source.status = ProjectSourceStatus::Processed;
                source.reason_codes = vec!["processed_ok".to_string()];
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
            Self::OcrRescue => ProjectSourceExtractionRoute::OcrRequired,
            Self::ForceOcr => ProjectSourceExtractionRoute::ForceOcr,
        }
    }
}

pub fn default_extraction_route_for_kind(kind: ProjectSourceKind) -> ProjectSourceExtractionRoute {
    match kind {
        ProjectSourceKind::Pdf => ProjectSourceExtractionRoute::OcrIfNeeded,
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
) -> ProjectSourceImportReview {
    let mut seen_ids: HashSet<String> = manifest
        .sources
        .iter()
        .map(|source| source.id.clone())
        .collect();
    let mut candidates = Vec::new();
    let mut duplicate_count = 0usize;

    for path in paths {
        let source_path = source_path_for_storage(project_root, &path);
        let source_id = stable_project_source_id(project_root, &source_path);
        if !seen_ids.insert(source_id.clone()) {
            duplicate_count += 1;
            continue;
        }

        let mut source = new_project_source(source_id, source_path);
        preflight_project_source(&mut source);
        candidates.push(source);
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
            ProjectSourceImportPolicy::OcrIfNeeded => {
                source.extraction_route = ProjectSourceExtractionRoute::OcrIfNeeded;
            }
            ProjectSourceImportPolicy::ForceOcr => {
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
        ProjectSourcePdfProfile::OcrRescue => {
            set_source_status(source, ProjectSourceStatus::NeedsOcr, &["ocr_planned"]);
        }
        ProjectSourcePdfProfile::ForceOcr => {
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
        Some(ProjectSourcePdfProfile::OcrRescue) => Some("ocr_review_recommended"),
        Some(ProjectSourcePdfProfile::ForceOcr) => Some("ocr_generated"),
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
            source.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
            source.extraction_route = ProjectSourceExtractionRoute::OcrRequired;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["ocr_required", "low_text_density"],
            );
        }
        PdfAuditQuality::Poor => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
            source.extraction_route = ProjectSourceExtractionRoute::OcrRequired;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["ocr_required", "low_text_density"],
            );
        }
        PdfAuditQuality::Empty => {
            source.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
            source.extraction_route = ProjectSourceExtractionRoute::OcrRequired;
            set_source_status(
                source,
                ProjectSourceStatus::NeedsOcr,
                &["pdf_no_text_layer", "ocr_required"],
            );
        }
        PdfAuditQuality::Unknown => {
            if matches!(suggested_profile, PdfAuditSuggestedProfile::ForceOcr)
                || !embedded_text_detected
            {
                source.pdf_profile = Some(ProjectSourcePdfProfile::OcrRescue);
                source.extraction_route = ProjectSourceExtractionRoute::OcrRequired;
                set_source_status(
                    source,
                    ProjectSourceStatus::NeedsOcr,
                    &["pdf_no_text_layer", "ocr_required"],
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
                ProjectSourceExtractionRoute::OcrIfNeeded,
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
            ProjectSourceExtractionRoute::OcrRequired
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
        assert_eq!(
            manifest.sources[0].reason_codes,
            vec!["ocr_review_recommended"]
        );
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
    fn updating_pdf_profile_preserves_ocr_rescue_and_force_ocr_routes() {
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
            ProjectSourceExtractionRoute::OcrRequired
        );
        assert_eq!(ocr_rescue.sources[0].status, ProjectSourceStatus::NeedsOcr);
        assert_eq!(ocr_rescue.sources[0].reason_codes, vec!["ocr_planned"]);

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
        assert_eq!(ocr_source.reason_codes, vec!["ocr_planned"]);
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
            ProjectSourceExtractionRoute::OcrRequired
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
}
