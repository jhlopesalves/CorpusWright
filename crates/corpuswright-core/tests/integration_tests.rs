use corpuswright_core::{
    CleaningConfig, ExportOptions, PreviewOptions, clean_text, export_corpus,
    preview_processed_files, scan_directory,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_end_to_end_corpus_pipeline() {
    // 1. Setup paths
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixtures_dir = Path::new(manifest_dir).join("tests").join("fixtures");

    // 2. Scan the directory
    let mut scan_report = scan_directory(&fixtures_dir).expect("Failed to scan directory");
    scan_report
        .files
        .retain(|f| !f.source_path.to_string_lossy().contains("non-tracked"));
    scan_report.summary.files_supported = scan_report.files.len();

    assert_eq!(scan_report.summary.files_supported, 20);
    assert_eq!(scan_report.files.len(), 20);

    // 3. Setup aggressive cleaning config for the dirty text
    let config = CleaningConfig {
        lowercase: true,
        remove_standalone_page_numbers: true,
        remove_page_indicators: true,
        collapse_blank_lines: true,
        normalize_irregular_line_breaks: true,
        ..CleaningConfig::default()
    };

    // 4. Preview files
    let preview_options = PreviewOptions {
        max_chars_per_file: 50,
        include_paths: true,
        max_files: None,
    };
    let preview_result =
        preview_processed_files(&scan_report.files, &preview_options, &config, None)
            .expect("Failed to preview files");

    assert_eq!(preview_result.files.len(), 20);
    assert!(
        preview_result
            .files
            .iter()
            .any(|p| p.relative_path.to_string_lossy().contains("clean.txt"))
    );
    assert!(
        preview_result
            .files
            .iter()
            .any(|p| p.relative_path.to_string_lossy().contains("dirty.txt"))
    );

    // Ensure the dirty preview shows the text has been cleaned (e.g. no PAGE artifacts)
    for file in &preview_result.files {
        if file.relative_path.to_string_lossy().contains("dirty.txt") {
            let processed_lower = file.text.to_lowercase();
            assert!(
                !processed_lower.contains("page "),
                "Preview should not contain page indicators"
            );
        }
    }

    // 5. Export Corpus
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let export_options = ExportOptions {
        app_name: "IntegrationTest".to_string(),
        app_version: Some("1.0.0".to_string()),
        overwrite: true,
    };

    let export_report = export_corpus(
        &scan_report.files,
        temp_dir.path(),
        &config,
        &export_options,
        None,
        None,
    )
    .expect("Failed to export corpus");

    assert_eq!(export_report.files_exported, 20);

    // 6. Verify exported structure and content
    let texts_dir = temp_dir.path().join("texts");
    assert!(texts_dir.exists());

    // Count all files recursively under texts_dir
    fn count_files_recursively(dir: &Path) -> usize {
        let mut count = 0;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_type().unwrap().is_dir() {
                    count += count_files_recursively(&entry.path());
                } else {
                    count += 1;
                }
            }
        }
        count
    }
    let exported_files_count = count_files_recursively(&texts_dir);
    assert_eq!(exported_files_count, 20);

    assert!(temp_dir.path().join("manifest.json").exists());
    assert!(temp_dir.path().join("config.json").exists());
    assert!(temp_dir.path().join("warnings.json").exists());
    assert!(temp_dir.path().join("README.txt").exists());
}

#[test]
fn test_clean_text_direct_api() {
    let config = CleaningConfig {
        lowercase: true,
        ..CleaningConfig::default()
    };

    let original = "HELLO WORLD\nThis is normal.\n";
    let cleaned = clean_text(original, &config);

    assert_eq!(cleaned, "hello world\nthis is normal.\n");
}
