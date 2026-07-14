use corpuswright_core::corpus::word_count::count_words_for_record;
use corpuswright_core::preparation::repeated_artifacts::scan_repeated_artifacts_report;
use corpuswright_core::sources::cache::apply_document_processing;
use corpuswright_core::{
    CleaningConfig, ExportOptions, PreviewOptions, PreviewWarningKind, RepeatedArtifactKind,
    RepeatedArtifactScanConfig, clean_text, export_corpus, preview_file, preview_processed_files,
    scan_directory,
};
use corpuswright_test_support::synthetic_corpus;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_end_to_end_corpus_pipeline() {
    let corpus = synthetic_corpus().expect("Failed to generate synthetic corpus");
    let expected_documents = corpus.document_count();
    let scan_report = scan_directory(corpus.root()).expect("Failed to scan directory");

    assert_eq!(scan_report.summary.files_supported, expected_documents);
    assert_eq!(scan_report.files.len(), expected_documents);

    let config = CleaningConfig {
        lowercase: true,
        remove_standalone_page_numbers: true,
        remove_page_indicators: true,
        collapse_blank_lines: true,
        normalize_irregular_line_breaks: true,
        ..CleaningConfig::default()
    };

    let preview_options = PreviewOptions {
        max_chars_per_file: Some(50),
        include_paths: true,
        max_files: None,
    };
    let preview_result =
        preview_processed_files(&scan_report.files, &preview_options, &config, None)
            .expect("Failed to preview files");

    assert_eq!(preview_result.files.len(), expected_documents);
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

    for file in &preview_result.files {
        if file.relative_path.to_string_lossy().contains("dirty.txt") {
            let processed_lower = file.text.to_lowercase();
            assert!(
                processed_lower.contains("page 99"),
                "Flat preview should keep page indicators without page metadata"
            );
        }
    }

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

    assert_eq!(export_report.files_exported, expected_documents);

    let texts_dir = temp_dir.path().join("texts");
    assert!(texts_dir.exists());

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
    assert_eq!(exported_files_count, expected_documents);

    assert!(temp_dir.path().join("manifest.json").exists());
    assert!(temp_dir.path().join("config.json").exists());
    assert!(temp_dir.path().join("warnings.json").exists());
    assert!(temp_dir.path().join("README.txt").exists());
}

#[test]
fn source_decode_policy_matches_preview_export_and_word_count() {
    let mut utf8_boundary_text = "a".repeat(8_191);
    utf8_boundary_text.push_str("ç café");

    let cases = [
        (
            "utf8-boundary.txt",
            utf8_boundary_text.as_bytes().to_vec(),
            utf8_boundary_text,
            8_192,
            2,
            false,
        ),
        (
            "portuguese-cp1252.txt",
            vec![
                b'c', b'a', b'f', 0xe9, b' ', b'a', 0xe7, 0xe3, b'o', b' ', b'J', b'o', 0xe3, b'o',
                b' ', b'c', b'o', b'r', b'a', 0xe7, 0xe3, b'o', b' ', b'n', 0xe3, b'o', b' ', b'm',
                b'a', 0xe7, 0xe3,
            ],
            "café ação João coração não maçã".to_string(),
            10,
            6,
            true,
        ),
        (
            "late-invalid-cp1252.txt",
            {
                let mut bytes = b"alpha beta gamma delta ".to_vec();
                bytes.extend_from_slice(&[b'c', b'a', b'f', 0xe9]);
                bytes
            },
            "alpha beta gamma delta café".to_string(),
            5,
            5,
            true,
        ),
    ];

    for (name, bytes, expected_text, preview_chars, expected_word_count, expect_warning) in cases {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let source_path = temp_dir.path().join(name);
        fs::write(&source_path, bytes).expect("Failed to write source fixture");
        let scan_report = scan_directory(temp_dir.path()).expect("Failed to scan source fixture");
        let record = &scan_report.files[0];
        let preview_options = PreviewOptions {
            max_chars_per_file: Some(preview_chars),
            include_paths: false,
            max_files: None,
        };

        let preview =
            preview_file(record, &preview_options, None).expect("Failed to preview source");
        let expected_preview = expected_text
            .chars()
            .take(preview_chars)
            .collect::<String>();
        assert_eq!(preview.text, expected_preview);
        assert_eq!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.kind == PreviewWarningKind::InvalidUtf8),
            expect_warning
        );
        assert!(!preview.text.contains('\u{fffd}'));

        let export_dir = temp_dir.path().join("export");
        let export_report = export_corpus(
            &scan_report.files,
            &export_dir,
            &CleaningConfig::default(),
            &ExportOptions {
                app_name: "DecodeParity".to_string(),
                app_version: None,
                overwrite: true,
            },
            None,
            None,
        )
        .expect("Failed to export source fixture");
        let exported =
            fs::read_to_string(export_dir.join(&export_report.exported_files[0].output_path))
                .expect("Failed to read exported source text");
        assert_eq!(exported, expected_text);
        assert!(!exported.contains('\u{fffd}'));

        let word_count = count_words_for_record(record, &CleaningConfig::default(), None).count;
        assert_eq!(word_count, expected_word_count);
    }
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

#[test]
fn manually_marked_repeated_artifact_rule_reaches_preview_processing_and_export() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let sources_dir = temp_dir.path().join("sources");
    fs::create_dir_all(&sources_dir).expect("Failed to create source directory");
    for index in 0..3 {
        fs::write(
            sources_dir.join(format!("doc-{index}.txt")),
            format!("Flat Header\nBody {index}\n"),
        )
        .expect("Failed to write source fixture");
    }

    let scan_report = scan_directory(&sources_dir).expect("Failed to scan source fixtures");
    let repeated_report = scan_repeated_artifacts_report(
        &scan_report.files,
        &RepeatedArtifactScanConfig {
            min_occurrences: 3,
            min_files: 1,
            min_line_chars: 4,
            include_inline_artifacts: false,
            ..RepeatedArtifactScanConfig::default()
        },
        &CleaningConfig::default(),
    )
    .expect("Failed to scan repeated artefacts");
    let candidate = repeated_report
        .candidates
        .iter()
        .find(|candidate| {
            candidate.kind == RepeatedArtifactKind::ExactLine
                && candidate.display_text == "Flat Header"
        })
        .expect("Expected repeated header candidate");
    let rule = repeated_report
        .promotions
        .iter()
        .find(|promotion| promotion.candidate_id == candidate.candidate_id)
        .and_then(|promotion| promotion.rule.clone())
        .expect("Expected Rust promotion rule");
    let cleaning_config = CleaningConfig {
        removal_rules: vec![rule],
        ..CleaningConfig::default()
    };

    let preview = preview_processed_files(
        &scan_report.files,
        &PreviewOptions::default(),
        &cleaning_config,
        None,
    )
    .expect("Failed to build processed preview");
    assert!(
        preview
            .files
            .iter()
            .all(|file| !file.text.contains("Flat Header"))
    );
    assert!(preview.files.iter().all(|file| file.text.contains("Body")));

    let raw_text = fs::read_to_string(&scan_report.files[0].source_path)
        .expect("Failed to read processing fixture");
    let mut warnings = Vec::new();
    let processed = apply_document_processing(
        &scan_report.files[0],
        raw_text,
        &cleaning_config,
        &mut warnings,
    );
    assert!(!processed.contains("Flat Header"));
    assert!(processed.contains("Body"));

    let export_dir = temp_dir.path().join("export");
    let export_report = export_corpus(
        &scan_report.files,
        &export_dir,
        &cleaning_config,
        &ExportOptions {
            app_name: "ManualPromotionGuardrail".to_string(),
            app_version: None,
            overwrite: true,
        },
        None,
        None,
    )
    .expect("Failed to export processed corpus");
    assert_eq!(export_report.files_exported, 3);
    for index in 0..3 {
        let exported =
            fs::read_to_string(export_dir.join("texts").join(format!("doc-{index}.txt")))
                .expect("Failed to read exported fixture");
        assert!(!exported.contains("Flat Header"));
        assert!(exported.contains("Body"));
    }
}
