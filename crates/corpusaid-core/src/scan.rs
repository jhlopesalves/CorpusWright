use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Text,
    Html,
    Docx,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub source_path: PathBuf,
    pub relative_path: PathBuf,
    pub document_type: DocumentType,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DocumentTypeCounts {
    pub text: usize,
    pub html: usize,
    pub docx: usize,
    pub pdf: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorpusSummary {
    pub root: PathBuf,
    pub files_discovered: usize,
    pub files_supported: usize,
    pub files_ignored: usize,
    pub total_size_bytes: u64,
    pub document_type_counts: DocumentTypeCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanReport {
    pub root: PathBuf,
    pub files: Vec<DocumentRecord>,
    pub files_discovered: usize,
    pub files_supported: usize,
    pub files_ignored: usize,
    pub total_size_bytes: u64,
    pub summary: CorpusSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ScanError {
    MissingRoot { path: PathBuf },
    NotDirectory { path: PathBuf },
    Io { path: PathBuf, message: String },
}

/// Recursively scans a directory for supported files, returning a detailed report.
///
/// Supported files currently include `.txt`, `.html`, and `.htm`.
/// Unrecognized files are skipped but included in the ignored count.
pub fn scan_directory(root: impl AsRef<Path>) -> Result<ScanReport, ScanError> {
    let root = root.as_ref();
    if !root.exists() {
        return Err(ScanError::MissingRoot {
            path: root.to_path_buf(),
        });
    }
    if !root.is_dir() {
        return Err(ScanError::NotDirectory {
            path: root.to_path_buf(),
        });
    }

    let mut paths = Vec::new();
    get_all_paths(root, &mut paths)?;

    let files_discovered = paths.len();

    use rayon::prelude::*;
    let mut files: Vec<DocumentRecord> = paths
        .into_par_iter()
        .filter_map(|path| {
            if let Some(document_type) = document_type_for_path(&path) {
                let size_bytes = fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                let source_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

                Some(DocumentRecord {
                    source_path,
                    relative_path,
                    document_type,
                    size_bytes,
                })
            } else {
                None
            }
        })
        .collect();

    let files_ignored = files_discovered - files.len();

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let total_size_bytes = files.iter().map(|file| file.size_bytes).sum();
    let document_type_counts = count_document_types(&files);
    let root_path = root.to_path_buf();
    let files_supported = files.len();
    let summary = CorpusSummary {
        root: root_path.clone(),
        files_discovered,
        files_supported,
        files_ignored,
        total_size_bytes,
        document_type_counts,
    };

    Ok(ScanReport {
        root: root_path,
        files,
        files_discovered,
        files_supported,
        files_ignored,
        total_size_bytes,
        summary,
    })
}

/// Loads specific files, filtering for supported extensions, and returns a detailed report.
/// This allows opening specific files instead of an entire directory.
pub fn load_files(paths: Vec<PathBuf>) -> Result<ScanReport, ScanError> {
    // Use the parent of the first path as a synthetic root, or an empty path if none.
    let root_path = paths
        .first()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();

    let files_discovered = paths.len();

    use rayon::prelude::*;
    let mut files: Vec<DocumentRecord> = paths
        .into_par_iter()
        .filter_map(|path| {
            if let Some(document_type) = document_type_for_path(&path) {
                let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let source_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                let relative_path = path
                    .file_name()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| path.clone());

                Some(DocumentRecord {
                    source_path,
                    relative_path,
                    document_type,
                    size_bytes,
                })
            } else {
                None
            }
        })
        .collect();

    let files_ignored = files_discovered - files.len();

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let total_size_bytes = files.iter().map(|file| file.size_bytes).sum();
    let document_type_counts = count_document_types(&files);
    let files_supported = files.len();

    let summary = CorpusSummary {
        root: root_path.clone(),
        files_discovered,
        files_supported,
        files_ignored,
        total_size_bytes,
        document_type_counts,
    };

    Ok(ScanReport {
        root: root_path,
        files,
        files_discovered,
        files_supported,
        files_ignored,
        total_size_bytes,
        summary,
    })
}

fn get_all_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), ScanError> {
    let entries = fs::read_dir(dir).map_err(|error| ScanError::Io {
        path: dir.to_path_buf(),
        message: error.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| ScanError::Io {
            path: dir.to_path_buf(),
            message: error.to_string(),
        })?;
        let path = entry.path();

        if path.is_dir() {
            get_all_paths(&path, paths)?;
        } else {
            paths.push(path);
        }
    }

    Ok(())
}

fn document_type_for_path(path: &Path) -> Option<DocumentType> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    match extension.as_str() {
        "txt" => Some(DocumentType::Text),
        "html" | "htm" => Some(DocumentType::Html),
        "docx" => Some(DocumentType::Docx),
        "pdf" => Some(DocumentType::Pdf),
        _ => None,
    }
}

fn count_document_types(files: &[DocumentRecord]) -> DocumentTypeCounts {
    let mut counts = DocumentTypeCounts::default();
    for file in files {
        match file.document_type {
            DocumentType::Text => counts.text += 1,
            DocumentType::Html => counts.html += 1,
            DocumentType::Docx => counts.docx += 1,
            DocumentType::Pdf => counts.pdf += 1,
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_missing_root() {
        let path = PathBuf::from("does_not_exist_12345");
        let result = scan_directory(&path);
        assert!(matches!(result, Err(ScanError::MissingRoot { .. })));
    }

    #[test]
    fn test_not_directory() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        File::create(&file_path).unwrap();

        let result = scan_directory(&file_path);
        assert!(matches!(result, Err(ScanError::NotDirectory { .. })));
    }

    #[test]
    fn test_empty_directory() {
        let dir = tempdir().unwrap();
        let result = scan_directory(dir.path()).unwrap();

        assert_eq!(result.files_discovered, 0);
        assert_eq!(result.files_supported, 0);
        assert_eq!(result.files_ignored, 0);
        assert_eq!(result.total_size_bytes, 0);
        assert_eq!(result.summary.total_size_bytes, 0);
        assert!(result.files.is_empty());
    }

    #[test]
    fn test_scan_supported_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("doc1.txt")).unwrap();
        File::create(root.join("doc2.html")).unwrap();
        File::create(root.join("doc3.htm")).unwrap();

        let result = scan_directory(root).unwrap();
        assert_eq!(result.files_supported, 3);
        assert_eq!(result.files_ignored, 0);
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.summary.document_type_counts.text, 1);
        assert_eq!(result.summary.document_type_counts.html, 2);

        let extensions: Vec<_> = result
            .files
            .iter()
            .map(|file| file.relative_path.extension().unwrap().to_str().unwrap())
            .collect();

        assert!(extensions.contains(&"txt"));
        assert!(extensions.contains(&"html"));
        assert!(extensions.contains(&"htm"));
    }

    #[test]
    fn test_uppercase_extensions() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("doc1.TXT")).unwrap();
        File::create(root.join("doc2.HTML")).unwrap();
        File::create(root.join("doc3.HTM")).unwrap();

        let result = scan_directory(root).unwrap();
        assert_eq!(result.files_supported, 3);
    }

    #[test]
    fn test_unsupported_extensions_ignored() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("doc1.txt")).unwrap();
        File::create(root.join("doc2.png")).unwrap();
        File::create(root.join("doc3.docx")).unwrap();
        File::create(root.join("doc4.pdf")).unwrap();
        File::create(root.join("no_extension_file")).unwrap();

        let result = scan_directory(root).unwrap();
        assert_eq!(result.files_supported, 3);
        assert_eq!(result.files_ignored, 2);
    }

    #[test]
    fn test_recursive_scanning() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let sub1 = root.join("sub1");
        fs::create_dir(&sub1).unwrap();

        let sub2 = sub1.join("sub2");
        fs::create_dir(&sub2).unwrap();

        File::create(root.join("root.txt")).unwrap();
        File::create(sub1.join("sub1.txt")).unwrap();
        File::create(sub2.join("sub2.txt")).unwrap();

        let result = scan_directory(root).unwrap();
        assert_eq!(result.files_supported, 3);

        let names: Vec<_> = result
            .files
            .iter()
            .map(|file| file.relative_path.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"root.txt"));
        assert!(names.contains(&"sub1.txt"));
        assert!(names.contains(&"sub2.txt"));
    }

    #[test]
    fn test_deterministic_ordering() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("z.txt")).unwrap();
        File::create(root.join("a.txt")).unwrap();
        File::create(root.join("m.txt")).unwrap();

        let result = scan_directory(root).unwrap();

        let names: Vec<_> = result
            .files
            .iter()
            .map(|file| file.relative_path.file_name().unwrap().to_str().unwrap())
            .collect();

        assert_eq!(names, vec!["a.txt", "m.txt", "z.txt"]);
    }

    #[test]
    fn test_size_bytes_recorded_in_summary() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let file_path = root.join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let result = scan_directory(root).unwrap();
        assert_eq!(result.files[0].size_bytes, 11);
        assert_eq!(result.total_size_bytes, 11);
        assert_eq!(result.summary.total_size_bytes, 11);
    }

    #[test]
    fn test_load_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let f1 = root.join("doc1.txt");
        let f2 = root.join("doc2.png"); // Unsupported
        let f3 = root.join("doc3.html");

        File::create(&f1).unwrap();
        File::create(&f2).unwrap();
        File::create(&f3).unwrap();

        let paths = vec![f1, f2, f3];
        let result = load_files(paths).unwrap();

        assert_eq!(result.files_discovered, 3);
        assert_eq!(result.files_supported, 2);
        assert_eq!(result.files_ignored, 1);
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.summary.document_type_counts.text, 1);
        assert_eq!(result.summary.document_type_counts.html, 1);

        let names: Vec<_> = result
            .files
            .iter()
            .map(|file| file.relative_path.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(names.contains(&"doc1.txt"));
        assert!(names.contains(&"doc3.html"));
    }
}
