use corpuswright_core::clean::PdfOcrQuality;
use corpuswright_core::project_sources::{
    ProjectSourceImportPolicy, ProjectSourceImportReview, ProjectSourceKind, ProjectSourceManifest,
    ProjectSourcePage, ProjectSourcePdfHandlingOverride, ProjectSourcePdfProfile,
    ProjectSourceQuery, ProjectSourceSelectionDescriptor, ProjectSourceSelectionViewport,
    ProjectSourceStatus, add_project_source_paths_with_pdf_handling,
    add_project_source_paths_with_progress, load_project_source_manifest,
    project_source_matches_query, query_project_source_manifest, query_project_source_selection,
    review_project_source_paths, save_project_source_manifest, source_manifest_path,
    update_project_source_pdf_handling, update_project_source_pdf_profiles,
};
use corpuswright_core::scan::{ScanReport, load_files, scan_directory};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;
use tauri::{Emitter, Window};

const PROJECT_DIR: &str = ".corpuswright";
const MANIFEST_FILE: &str = "project.json";
const REPEATED_ARTIFACTS_FILE: &str = "repeated-artifacts.json";
const PROJECT_VERSION: u32 = 1;

struct CachedProjectSourceInventory {
    root: PathBuf,
    modified: Option<SystemTime>,
    file_size: u64,
    manifest: ProjectSourceManifest,
}

pub(crate) struct ProjectSourceInventoryState {
    inner: RwLock<Option<CachedProjectSourceInventory>>,
}

impl ProjectSourceInventoryState {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    fn with_manifest<T>(
        &self,
        root: &Path,
        read: impl FnOnce(&ProjectSourceManifest) -> T,
    ) -> Result<T, String> {
        let inventory_path = source_manifest_path(root);
        let metadata = fs::metadata(&inventory_path).ok();
        let modified = metadata.as_ref().and_then(|value| value.modified().ok());
        let file_size = metadata.as_ref().map_or(0, fs::Metadata::len);
        let mut cached = self.inner.write().unwrap();
        let needs_reload = cached.as_ref().is_none_or(|current| {
            current.root != root || current.modified != modified || current.file_size != file_size
        });
        if needs_reload {
            *cached = Some(CachedProjectSourceInventory {
                root: root.to_path_buf(),
                modified,
                file_size,
                manifest: load_project_source_manifest(root)?,
            });
        }
        Ok(read(&cached.as_ref().unwrap().manifest))
    }

    fn replace(&self, root: &Path, manifest: ProjectSourceManifest) {
        let metadata = fs::metadata(source_manifest_path(root)).ok();
        *self.inner.write().unwrap() = Some(CachedProjectSourceInventory {
            root: root.to_path_buf(),
            modified: metadata.as_ref().and_then(|value| value.modified().ok()),
            file_size: metadata.as_ref().map_or(0, fs::Metadata::len),
            manifest,
        });
    }

    pub(crate) fn source_by_id(
        &self,
        root: &Path,
        source_id: &str,
    ) -> Result<Option<corpuswright_core::project_sources::ProjectSource>, String> {
        self.with_manifest(root, |manifest| {
            manifest
                .sources
                .iter()
                .find(|source| source.id == source_id)
                .cloned()
        })
    }

    pub(crate) fn matching_paths(
        &self,
        root: &Path,
        query: &str,
        excluded_ids: &[String],
    ) -> Result<Vec<PathBuf>, String> {
        let query = query.trim().to_lowercase();
        let excluded_ids = excluded_ids
            .iter()
            .collect::<std::collections::HashSet<_>>();
        self.with_manifest(root, |manifest| {
            manifest
                .sources
                .iter()
                .filter(|source| {
                    source.kind != ProjectSourceKind::Unknown
                        && source.status != ProjectSourceStatus::Failed
                        && !excluded_ids.contains(&source.id)
                        && project_source_matches_query(source, &query)
                })
                .map(|source| source.path.clone())
                .collect()
        })
    }

    pub(crate) fn selection_viewport(
        &self,
        root: &Path,
        selection: &ProjectSourceSelectionDescriptor,
        viewport_start: usize,
        viewport_size: usize,
    ) -> Result<ProjectSourceSelectionViewport, String> {
        self.with_manifest(root, |manifest| {
            query_project_source_selection(manifest, selection, viewport_start, viewport_size)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CorpusProjectManifest {
    pub project_version: u32,
    pub name: String,
    pub root_path: PathBuf,
    pub created_at: String,
    pub updated_at: String,
    pub source_roots: Vec<PathBuf>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiveProject {
    name: String,
    root_path: String,
    manifest_path: String,
    project_version: u32,
    source_roots: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSourcePdfHandlingOverrideRequest {
    path: String,
    pdf_profile: ProjectSourcePdfProfile,
    ocr_quality: PdfOcrQuality,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSourceFastImportResult {
    imported: bool,
    requires_pdf_handling: bool,
    added_count: usize,
    duplicate_count: usize,
    ignored_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSourceImportResult {
    added_count: usize,
    duplicate_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectSourceRemovalResult {
    removed_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSourceImportProgress {
    current: usize,
    total: usize,
    status: &'static str,
}

#[tauri::command(async)]
pub(crate) fn create_corpus_project_command(root_path: String) -> Result<ActiveProject, String> {
    let root = normalise_selected_path(&root_path)?;
    if root.exists() && !root.is_dir() {
        return Err("Choose a folder for the CorpusWright project.".to_string());
    }

    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create project folder: {error}"))?;
    let root = canonical_or_original(root);
    let manifest_path = manifest_path_for_root(&root);
    if manifest_path.exists() {
        return Err("This folder already contains a CorpusWright project.".to_string());
    }

    fs::create_dir_all(root.join(PROJECT_DIR).join("cache"))
        .map_err(|error| format!("Could not create project metadata folder: {error}"))?;
    fs::create_dir_all(root.join("sources"))
        .map_err(|error| format!("Could not create project sources folder: {error}"))?;
    fs::create_dir_all(root.join("processed"))
        .map_err(|error| format!("Could not create project processed folder: {error}"))?;
    fs::create_dir_all(root.join("exports"))
        .map_err(|error| format!("Could not create project exports folder: {error}"))?;

    let manifest = new_manifest(&root, Vec::new());
    write_manifest(&manifest_path, &manifest)?;
    save_project_source_manifest(&root, &ProjectSourceManifest::default())?;
    Ok(active_project_from_manifest(&manifest, &manifest_path))
}

#[tauri::command(async)]
pub(crate) fn open_corpus_project_command(path: String) -> Result<ActiveProject, String> {
    let selected = normalise_selected_path(&path)?;
    let manifest_path = selected_manifest_path(&selected);
    let manifest = read_manifest(&manifest_path)?;
    Ok(active_project_from_manifest(&manifest, &manifest_path))
}

#[tauri::command(async)]
pub(crate) fn initialise_folder_as_corpus_project_command(
    root_path: String,
) -> Result<ActiveProject, String> {
    let root = normalise_selected_path(&root_path)?;
    if !root.exists() {
        return Err("Choose an existing folder to open as a CorpusWright project.".to_string());
    }
    if !root.is_dir() {
        return Err("Choose a folder to open as a CorpusWright project.".to_string());
    }

    let root = canonical_or_original(root);
    let manifest_path = manifest_path_for_root(&root);
    if manifest_path.exists() {
        let manifest = read_manifest(&manifest_path)?;
        return Ok(active_project_from_manifest(&manifest, &manifest_path));
    }

    fs::create_dir_all(root.join(PROJECT_DIR).join("cache"))
        .map_err(|error| format!("Could not create project metadata folder: {error}"))?;
    fs::create_dir_all(root.join(PROJECT_DIR).join("processed"))
        .map_err(|error| format!("Could not create project processed folder: {error}"))?;
    fs::create_dir_all(root.join(PROJECT_DIR).join("exports"))
        .map_err(|error| format!("Could not create project exports folder: {error}"))?;

    let manifest = new_manifest(&root, vec![root.clone()]);
    write_manifest(&manifest_path, &manifest)?;
    save_project_source_manifest(&root, &ProjectSourceManifest::default())?;
    Ok(active_project_from_manifest(&manifest, &manifest_path))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub(crate) fn query_project_sources_command(
    root_path: String,
    query: String,
    status_filter: Option<ProjectSourceStatus>,
    kind_filter: Option<ProjectSourceKind>,
    offset: usize,
    limit: usize,
    inventory: tauri::State<'_, ProjectSourceInventoryState>,
) -> Result<ProjectSourcePage, String> {
    let root = validated_project_root(&root_path)?;
    inventory.with_manifest(&root, |manifest| {
        query_project_source_manifest(
            manifest,
            &ProjectSourceQuery {
                query,
                status_filter,
                kind_filter,
                offset,
                limit,
            },
        )
    })
}

#[tauri::command(async)]
pub(crate) fn fast_import_project_source_files_command(
    root_path: String,
    paths: Vec<String>,
    window: Window,
    inventory: tauri::State<'_, ProjectSourceInventoryState>,
) -> Result<ProjectSourceFastImportResult, String> {
    let root = validated_project_root(&root_path)?;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    if paths_require_pdf_handling(&paths) {
        return Ok(ProjectSourceFastImportResult {
            imported: false,
            requires_pdf_handling: true,
            added_count: 0,
            duplicate_count: 0,
            ignored_count: 0,
        });
    }

    let update = add_project_source_paths_with_progress(&root, paths, |current, total| {
        let _ = window.emit(
            "project-source-import-progress",
            ProjectSourceImportProgress {
                current,
                total,
                status: "registering",
            },
        );
    })?;
    let result = ProjectSourceFastImportResult {
        imported: true,
        requires_pdf_handling: false,
        added_count: update.added_count,
        duplicate_count: update.duplicate_count,
        ignored_count: 0,
    };
    inventory.replace(&root, update.manifest);
    Ok(result)
}

fn paths_require_pdf_handling(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| {
        corpuswright_core::project_sources::kind_for_path(path) == ProjectSourceKind::Pdf
    })
}

#[tauri::command(async)]
pub(crate) fn fast_import_project_source_folder_command(
    root_path: String,
    folder_path: String,
    window: Window,
    inventory: tauri::State<'_, ProjectSourceInventoryState>,
) -> Result<ProjectSourceFastImportResult, String> {
    let root = validated_project_root(&root_path)?;
    let _ = window.emit(
        "project-source-import-progress",
        ProjectSourceImportProgress {
            current: 0,
            total: 0,
            status: "scanning",
        },
    );
    let report = scan_source_folder(&folder_path)?;
    if report.summary.document_type_counts.pdf > 0 {
        return Ok(ProjectSourceFastImportResult {
            imported: false,
            requires_pdf_handling: true,
            added_count: 0,
            duplicate_count: 0,
            ignored_count: report.files_ignored,
        });
    }

    let paths = report
        .files
        .into_iter()
        .map(|record| record.source_path)
        .collect();
    let update = add_project_source_paths_with_progress(&root, paths, |current, total| {
        let _ = window.emit(
            "project-source-import-progress",
            ProjectSourceImportProgress {
                current,
                total,
                status: "registering",
            },
        );
    })?;
    let result = ProjectSourceFastImportResult {
        imported: true,
        requires_pdf_handling: false,
        added_count: update.added_count,
        duplicate_count: update.duplicate_count,
        ignored_count: report.files_ignored,
    };
    inventory.replace(&root, update.manifest);
    Ok(result)
}

#[tauri::command(async)]
pub(crate) fn remove_project_source_selection_command(
    root_path: String,
    all_matching: bool,
    query: String,
    ids: Vec<String>,
    excluded_ids: Vec<String>,
    inventory: tauri::State<'_, ProjectSourceInventoryState>,
) -> Result<ProjectSourceRemovalResult, String> {
    let root = validated_project_root(&root_path)?;
    let mut manifest = load_project_source_manifest(&root)?;
    let original_count = manifest.sources.len();
    if all_matching {
        let query = query.trim().to_lowercase();
        let excluded_ids = excluded_ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        manifest.sources.retain(|source| {
            excluded_ids.contains(&source.id) || !project_source_matches_query(source, &query)
        });
    } else {
        let ids = ids.into_iter().collect::<std::collections::HashSet<_>>();
        manifest.sources.retain(|source| !ids.contains(&source.id));
    }
    let removed_count = original_count - manifest.sources.len();
    save_project_source_manifest(&root, &manifest)?;
    inventory.replace(&root, manifest);
    Ok(ProjectSourceRemovalResult { removed_count })
}

#[tauri::command(async)]
pub(crate) fn review_project_source_files_import_command(
    root_path: String,
    paths: Vec<String>,
) -> Result<ProjectSourceImportReview, String> {
    let root = validated_project_root(&root_path)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    review_project_source_paths(root, paths, 0)
}

#[tauri::command(async)]
pub(crate) fn review_project_source_folder_import_command(
    root_path: String,
    folder_path: String,
) -> Result<ProjectSourceImportReview, String> {
    let root = validated_project_root(&root_path)?;
    let report = scan_source_folder(&folder_path)?;
    let paths = report
        .files
        .into_iter()
        .map(|record| record.source_path)
        .collect();
    review_project_source_paths(root, paths, report.files_ignored)
}

#[tauri::command(async)]
pub(crate) fn confirm_project_source_files_import_command(
    root_path: String,
    paths: Vec<String>,
    pdf_handling_overrides: Vec<ProjectSourcePdfHandlingOverrideRequest>,
) -> Result<ProjectSourceImportResult, String> {
    let root = validated_project_root(&root_path)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    let update = add_project_source_paths_with_pdf_handling(
        root,
        paths,
        ProjectSourceImportPolicy::Recommended,
        PdfOcrQuality::Balanced,
        pdf_handling_overrides
            .into_iter()
            .map(pdf_handling_override_from_request)
            .collect(),
    )?;
    Ok(ProjectSourceImportResult {
        added_count: update.added_count,
        duplicate_count: update.duplicate_count,
    })
}

#[tauri::command(async)]
pub(crate) fn confirm_project_source_folder_import_command(
    root_path: String,
    folder_path: String,
    pdf_handling_overrides: Vec<ProjectSourcePdfHandlingOverrideRequest>,
) -> Result<ProjectSourceImportResult, String> {
    let root = validated_project_root(&root_path)?;
    let report = scan_source_folder(&folder_path)?;
    let paths = report
        .files
        .into_iter()
        .map(|record| record.source_path)
        .collect();
    let update = add_project_source_paths_with_pdf_handling(
        root,
        paths,
        ProjectSourceImportPolicy::Recommended,
        PdfOcrQuality::Balanced,
        pdf_handling_overrides
            .into_iter()
            .map(pdf_handling_override_from_request)
            .collect(),
    )?;
    Ok(ProjectSourceImportResult {
        added_count: update.added_count,
        duplicate_count: update.duplicate_count,
    })
}

pub(crate) fn load_project_source_records(root_path: String) -> Result<ScanReport, String> {
    let root = validated_project_root(&root_path)?;
    let manifest = load_project_source_manifest(&root)?;
    let route_by_path = manifest
        .sources
        .iter()
        .map(|source| (canonical_or_original(source.path.clone()), source))
        .collect::<HashMap<_, _>>();
    let paths = manifest
        .sources
        .iter()
        .filter(|source| source.kind != ProjectSourceKind::Unknown)
        .filter(|source| source.status != ProjectSourceStatus::Failed)
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();
    let mut report = load_files(paths).map_err(|error| format!("{error:?}"))?;

    for record in &mut report.files {
        let key = canonical_or_original(record.source_path.clone());
        if let Some(source) = route_by_path.get(&key) {
            record.extraction_route = Some(source.extraction_route);
            record.pdf_profile = source.pdf_profile;
            record.ocr_quality = source.ocr_quality;
        }
    }

    report.root = root.clone();
    report.summary.root = root;
    Ok(report)
}

pub(crate) fn load_project_source_inventory(
    root_path: String,
) -> Result<(PathBuf, ProjectSourceManifest), String> {
    let root = validated_project_root(&root_path)?;
    let manifest = load_project_source_manifest(&root)?;
    Ok((root, manifest))
}

pub(crate) fn update_project_source_pdf_profiles_command(
    root_path: String,
    paths: Vec<String>,
    profile: ProjectSourcePdfProfile,
    ocr_quality: PdfOcrQuality,
) -> Result<(), String> {
    let root = validated_project_root(&root_path)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    update_project_source_pdf_profiles(root, paths, profile, ocr_quality)?;
    Ok(())
}

/// Applies per-source PDF handling overrides to existing project sources.
/// Unlike the bulk profile command, this preserves per-source handling choices
/// so that re-extract can change handling at the row level.
#[tauri::command(async)]
pub(crate) fn update_project_source_pdf_handling_command(
    root_path: String,
    overrides: Vec<ProjectSourcePdfHandlingOverrideRequest>,
) -> Result<(), String> {
    let root = validated_project_root(&root_path)?;
    update_project_source_pdf_handling(
        root,
        overrides
            .into_iter()
            .map(pdf_handling_override_from_request)
            .collect(),
    )?;
    Ok(())
}

#[tauri::command(async)]
pub(crate) fn load_repeated_artifact_decisions_command(
    root_path: String,
) -> Result<String, String> {
    let root = validated_project_root(&root_path)?;
    let path = root.join(PROJECT_DIR).join(REPEATED_ARTIFACTS_FILE);
    if !path.exists() {
        return Ok(empty_repeated_artifact_decisions_json());
    }
    if !path.is_file() {
        return Err("Repeated artefact decisions path is not a file.".to_string());
    }
    let content =
        fs::read_to_string(&path).map_err(|error| format!("Could not read decisions: {error}"))?;
    validate_repeated_artifact_decisions_json(&content)?;
    Ok(content)
}

#[tauri::command(async)]
pub(crate) fn save_repeated_artifact_decisions_command(
    root_path: String,
    content: String,
) -> Result<(), String> {
    validate_repeated_artifact_decisions_json(&content)?;
    let root = validated_project_root(&root_path)?;
    let path = root.join(PROJECT_DIR).join(REPEATED_ARTIFACTS_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create project metadata folder: {error}"))?;
    }
    fs::write(&path, content).map_err(|error| format!("Could not write decisions: {error}"))
}

fn pdf_handling_override_from_request(
    request: ProjectSourcePdfHandlingOverrideRequest,
) -> ProjectSourcePdfHandlingOverride {
    ProjectSourcePdfHandlingOverride {
        path: PathBuf::from(request.path),
        pdf_profile: request.pdf_profile,
        ocr_quality: request.ocr_quality,
    }
}

fn scan_source_folder(folder_path: &str) -> Result<ScanReport, String> {
    let folder = normalise_selected_path(folder_path)?;
    if !folder.exists() {
        return Err("Choose an existing folder to add source files.".to_string());
    }
    if !folder.is_dir() {
        return Err("Choose a folder to add source files.".to_string());
    }

    scan_directory(&folder).map_err(|error| format!("{error:?}"))
}

fn normalise_selected_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Choose a project folder.".to_string());
    }
    Ok(PathBuf::from(trimmed))
}

fn manifest_path_for_root(root: &Path) -> PathBuf {
    root.join(PROJECT_DIR).join(MANIFEST_FILE)
}

fn selected_manifest_path(selected: &Path) -> PathBuf {
    if selected.is_file()
        && selected
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(MANIFEST_FILE))
    {
        return selected.to_path_buf();
    }
    manifest_path_for_root(selected)
}

pub(crate) fn validated_project_root(root_path: &str) -> Result<PathBuf, String> {
    let selected = normalise_selected_path(root_path)?;
    let manifest_path = manifest_path_for_root(&selected);
    let manifest = read_manifest(&manifest_path)?;
    Ok(project_root_from_manifest_path(&manifest_path)
        .map(canonical_or_original)
        .unwrap_or(manifest.root_path))
}

fn project_root_from_manifest_path(manifest_path: &Path) -> Option<PathBuf> {
    manifest_path
        .parent()
        .and_then(|metadata_dir| metadata_dir.parent())
        .map(Path::to_path_buf)
}

fn canonical_or_original(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn empty_repeated_artifact_decisions_json() -> String {
    "{\n  \"version\": 1,\n  \"decisions\": {}\n}".to_string()
}

fn validate_repeated_artifact_decisions_json(content: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|error| format!("Repeated artefact decisions are not valid JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Repeated artefact decisions must be a JSON object.".to_string())?;
    if object.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("Repeated artefact decisions must use version 1.".to_string());
    }
    if !object
        .get("decisions")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err("Repeated artefact decisions must contain a decisions object.".to_string());
    }
    Ok(())
}

fn current_timestamp() -> String {
    jiff::Timestamp::now().to_string()
}

fn project_name_from_root(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("CorpusWright Project")
        .to_string()
}

fn new_manifest(root: &Path, source_roots: Vec<PathBuf>) -> CorpusProjectManifest {
    let now = current_timestamp();
    CorpusProjectManifest {
        project_version: PROJECT_VERSION,
        name: project_name_from_root(root),
        root_path: root.to_path_buf(),
        created_at: now.clone(),
        updated_at: now,
        source_roots,
        notes: None,
    }
}

fn write_manifest(manifest_path: &Path, manifest: &CorpusProjectManifest) -> Result<(), String> {
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create project metadata folder: {error}"))?;
    }
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("Could not serialise project manifest: {error}"))?;
    fs::write(manifest_path, json)
        .map_err(|error| format!("Could not write project manifest: {error}"))
}

fn read_manifest(manifest_path: &Path) -> Result<CorpusProjectManifest, String> {
    if !manifest_path.exists() {
        return Err("This folder does not contain a CorpusWright project.".to_string());
    }
    if !manifest_path.is_file() {
        return Err("The CorpusWright project manifest is not a file.".to_string());
    }
    let content = fs::read_to_string(manifest_path)
        .map_err(|error| format!("Could not read project manifest: {error}"))?;
    let manifest: CorpusProjectManifest = serde_json::from_str(&content)
        .map_err(|error| format!("Project manifest is not valid JSON: {error}"))?;
    if manifest.project_version != PROJECT_VERSION {
        return Err(format!(
            "Unsupported CorpusWright project version: {}.",
            manifest.project_version
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err("Project manifest is missing a project name.".to_string());
    }
    Ok(manifest)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn active_project_from_manifest(
    manifest: &CorpusProjectManifest,
    manifest_path: &Path,
) -> ActiveProject {
    let actual_root = project_root_from_manifest_path(manifest_path)
        .map(canonical_or_original)
        .unwrap_or_else(|| manifest.root_path.clone());
    let source_roots = manifest
        .source_roots
        .iter()
        .map(|source_root| {
            if *source_root == manifest.root_path {
                path_to_string(&actual_root)
            } else {
                path_to_string(source_root)
            }
        })
        .collect();

    ActiveProject {
        name: manifest.name.clone(),
        root_path: path_to_string(&actual_root),
        manifest_path: path_to_string(manifest_path),
        project_version: manifest.project_version,
        source_roots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "corpuswright-project-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn creating_new_project_writes_valid_manifest() {
        let root = unique_temp_dir("create");
        let result = create_corpus_project_command(path_to_string(&root));

        assert!(result.is_ok());
        let manifest_path = manifest_path_for_root(&root);
        let manifest: CorpusProjectManifest =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();

        assert_eq!(manifest.project_version, PROJECT_VERSION);
        assert_eq!(manifest.name, root.file_name().unwrap().to_str().unwrap());
        assert!(manifest.source_roots.is_empty());
        assert!(root.join(PROJECT_DIR).join("cache").is_dir());
        assert!(root.join("sources").is_dir());
        assert!(root.join("processed").is_dir());
        assert!(root.join("exports").is_dir());

        cleanup(&root);
    }

    #[test]
    fn opening_existing_project_reads_manifest() {
        let root = unique_temp_dir("open");
        create_corpus_project_command(path_to_string(&root)).unwrap();

        let opened = open_corpus_project_command(path_to_string(&root)).unwrap();

        assert_eq!(opened.name, root.file_name().unwrap().to_str().unwrap());
        assert_eq!(opened.project_version, PROJECT_VERSION);
        assert_eq!(
            PathBuf::from(opened.manifest_path)
                .file_name()
                .and_then(|name| name.to_str()),
            Some(MANIFEST_FILE)
        );

        cleanup(&root);
    }

    #[test]
    fn missing_manifest_returns_error() {
        let root = unique_temp_dir("missing");
        fs::create_dir_all(&root).unwrap();

        let error = open_corpus_project_command(path_to_string(&root)).unwrap_err();

        assert!(error.contains("does not contain a CorpusWright project"));

        cleanup(&root);
    }

    #[test]
    fn opening_folder_as_project_preserves_existing_files() {
        let root = unique_temp_dir("folder");
        fs::create_dir_all(&root).unwrap();
        let source_file = root.join("essay.txt");
        fs::write(&source_file, "sample").unwrap();

        let project = initialise_folder_as_corpus_project_command(path_to_string(&root)).unwrap();
        let manifest_path = manifest_path_for_root(&root);
        let manifest: CorpusProjectManifest =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();

        assert!(source_file.exists());
        assert_eq!(
            manifest.source_roots,
            vec![canonical_or_original(root.clone())]
        );
        assert_eq!(
            project.source_roots,
            vec![path_to_string(&canonical_or_original(root.clone()))]
        );
        assert!(root.join(PROJECT_DIR).join("cache").is_dir());
        assert!(root.join(PROJECT_DIR).join("processed").is_dir());
        assert!(root.join(PROJECT_DIR).join("exports").is_dir());

        cleanup(&root);
    }

    #[test]
    fn reviewing_source_files_does_not_persist_sources() {
        let root = unique_temp_dir("review-files");
        create_corpus_project_command(path_to_string(&root)).unwrap();
        let source_file = root.join("essay.txt");
        fs::write(&source_file, "sample").unwrap();

        let review = review_project_source_files_import_command(
            path_to_string(&root),
            vec![path_to_string(&source_file)],
        )
        .unwrap();
        let manifest = load_project_source_manifest(&root).unwrap();

        assert_eq!(review.candidates.len(), 1);
        assert_eq!(review.duplicate_count, 0);
        assert!(manifest.sources.is_empty());

        cleanup(&root);
    }

    #[test]
    fn folder_import_review_reports_ignored_unsupported_files() {
        let root = unique_temp_dir("review-folder");
        create_corpus_project_command(path_to_string(&root)).unwrap();
        let folder = root.join("incoming");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("essay.txt"), "sample").unwrap();
        fs::write(folder.join("image.png"), "not a source").unwrap();

        let review = review_project_source_folder_import_command(
            path_to_string(&root),
            path_to_string(&folder),
        )
        .unwrap();

        assert_eq!(review.candidates.len(), 1);
        assert_eq!(review.ignored_count, 1);

        cleanup(&root);
    }

    #[test]
    fn confirming_source_file_import_persists_sources() {
        let root = unique_temp_dir("confirm-files");
        create_corpus_project_command(path_to_string(&root)).unwrap();
        let source_file = root.join("essay.txt");
        fs::write(&source_file, "sample").unwrap();

        let update = confirm_project_source_files_import_command(
            path_to_string(&root),
            vec![path_to_string(&source_file)],
            Vec::new(),
        )
        .unwrap();
        let manifest = load_project_source_manifest(&root).unwrap();

        assert_eq!(update.added_count, 1);
        assert_eq!(manifest.sources.len(), 1);

        cleanup(&root);
    }

    #[test]
    fn native_text_paths_bypass_pdf_handling() {
        assert!(!paths_require_pdf_handling(&[
            PathBuf::from("review.txt"),
            PathBuf::from("notes.html"),
            PathBuf::from("article.docx"),
        ]));
        assert!(paths_require_pdf_handling(&[
            PathBuf::from("review.txt"),
            PathBuf::from("scan.pdf"),
        ]));
    }
}
