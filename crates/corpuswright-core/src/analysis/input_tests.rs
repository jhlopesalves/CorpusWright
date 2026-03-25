use super::*;
use crate::analysis::test_support::*;
use crate::clean::CleaningConfig;
use std::cell::Cell;

#[test]
fn resolver_uses_active_processed_pipeline_output() {
    let record = record("processed.txt");
    let cleaning_config = CleaningConfig {
        remove_patterns: vec!["REMOVE".to_string()],
        ..CleaningConfig::default()
    };
    let fallback_called = Cell::new(false);

    let resolution = resolve_analysis_input(
        &record,
        &cleaning_config,
        |_, config| Ok(crate::clean::clean_text("keep REMOVE", config)),
        |_, _| {
            fallback_called.set(true);
            Ok("keep REMOVE".to_string())
        },
    );

    assert_eq!(resolution.text.as_deref(), Some("keep "));
    assert_eq!(resolution.provenance, AnalysisInputProvenance::Processed);
    assert_eq!(resolution.status, AnalysisInputStatus::Ready);
    assert!(!fallback_called.get());
}

#[test]
fn resolver_uses_inline_repeated_artifact_removal_rule() {
    let record = record("inline.txt");
    let cleaning_config = CleaningConfig {
        normalize_whitespace_line_endings: true,
        removal_rules: vec![anywhere_literal_rule("<br />")],
        ..CleaningConfig::default()
    };
    let fallback_called = Cell::new(false);

    let resolution = resolve_analysis_input(
        &record,
        &cleaning_config,
        |_, config| Ok(crate::clean::clean_text("foo<br />bar", config)),
        |_, _| {
            fallback_called.set(true);
            Ok("foo<br />bar".to_string())
        },
    );

    assert_eq!(resolution.text.as_deref(), Some("foo bar"));
    assert_eq!(resolution.provenance, AnalysisInputProvenance::Processed);
    assert!(!fallback_called.get());
}

#[test]
fn resolver_falls_back_only_when_processing_fails() {
    let record = record("fallback.txt");
    let resolution = resolve_analysis_input(
        &record,
        &CleaningConfig::default(),
        |_, _| Err("processing unavailable".to_string()),
        |_, _| Ok("native source text".to_string()),
    );

    assert_eq!(
        resolution.provenance,
        AnalysisInputProvenance::SourceTextFallback
    );
    assert_eq!(resolution.text.as_deref(), Some("native source text"));
    assert!(matches!(
        resolution.status,
        AnalysisInputStatus::ReadyWithFallback { .. }
    ));
}
