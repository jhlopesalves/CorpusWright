use corpuswright_core::project_sources::{
    ProjectSourceManifest, ProjectSourceProcessingResult, ProjectSourceUpdate,
    add_project_source_paths, load_project_source_manifest, save_project_source_manifest,
    update_project_source_processing_results,
};
use corpuswright_core::scan::scan_directory;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROJECT_DIR: &str = ".corpuswright";
const MANIFEST_FILE: &str = "project.json";
const PROJECT_VERSION: u32 = 1;

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

#[tauri::command(async)]
pub(crate) fn load_project_sources_command(
    root_path: String,
) -> Result<ProjectSourceManifest, String> {
    let root = validated_project_root(&root_path)?;
    load_project_source_manifest(root)
}

#[tauri::command(async)]
pub(crate) fn add_project_source_files_command(
    root_path: String,
    paths: Vec<String>,
) -> Result<ProjectSourceUpdate, String> {
    let root = validated_project_root(&root_path)?;
    let paths = paths.into_iter().map(PathBuf::from).collect();
    add_project_source_paths(root, paths)
}

#[tauri::command(async)]
pub(crate) fn add_project_source_folder_command(
    root_path: String,
    folder_path: String,
) -> Result<ProjectSourceUpdate, String> {
    let root = validated_project_root(&root_path)?;
    let folder = normalise_selected_path(&folder_path)?;
    if !folder.exists() {
        return Err("Choose an existing folder to add source files.".to_string());
    }
    if !folder.is_dir() {
        return Err("Choose a folder to add source files.".to_string());
    }

    let report = scan_directory(&folder).map_err(|error| format!("{error:?}"))?;
    let paths = report
        .files
        .into_iter()
        .map(|record| record.source_path)
        .collect();
    add_project_source_paths(root, paths)
}

#[tauri::command(async)]
pub(crate) fn update_project_source_processing_results_command(
    root_path: String,
    results: Vec<ProjectSourceProcessingResult>,
) -> Result<ProjectSourceManifest, String> {
    let root = validated_project_root(&root_path)?;
    update_project_source_processing_results(root, results)
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

fn validated_project_root(root_path: &str) -> Result<PathBuf, String> {
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
}
