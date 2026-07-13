#[cfg(test)]
use corpuswright_core::analysis::aggregate_concordance_collocations_documents;
use corpuswright_core::analysis::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisBuildProgress, AnalysisCorpus,
    AnalysisDocumentPage, AnalysisDocumentSelection, AnnotatedFrequencyQuery,
    AnnotatedFrequencyResult, AnnotatedTermDistributionQuery, CompareExpressionsQuery,
    CompareExpressionsResult, ConcordanceBackend, ConcordanceBackendUsed,
    ConcordanceCollocationAggregate, ConcordanceCollocationAggregationSignature,
    ConcordanceCollocationQuery, ConcordanceCollocationResult, ConcordanceDocument,
    ConcordanceExpansionQuery, ConcordanceExpansionResult, ConcordanceFallbackReason,
    ConcordanceQuery, ConcordanceResult, ConcordanceRow, ConcordanceSearchContext, CorpusOverview,
    CorpusOverviewContext, FrequencyGroupMode, FrequencyPage, FrequencyQuery,
    OverviewDistributionEvidencePage, OverviewDistributionEvidenceQuery, OverviewDistributionQuery,
    OverviewDistributionResult, RoutedConcordanceCollocationAggregate, RoutedConcordanceResult,
    TermDistributionPage, TermDistributionQuery,
    aggregate_concordance_collocations_routed_provider,
    aggregate_concordance_collocations_scan_provider, annotation_cache_availability_for_corpus,
    build_analysis_corpus, cleaning_config_signature, compare_expressions,
    concordance_collocation_aggregation_signature, expand_concordance_context,
    query_analysis_documents, query_annotated_frequency, query_annotated_term_distribution,
    query_concordance_collocations_from_aggregate, query_frequency_list,
    query_overview_distribution, query_overview_distribution_evidence, query_term_distribution,
    resolve_analysis_input, search_concordance_documents, search_concordance_documents_routed,
    search_concordance_documents_routed_provider,
    search_concordance_documents_routed_scan_provider,
};
use corpuswright_core::cache::{
    CacheEntry, DocumentTextMode, ExtractionCache, document_text_for_record,
    document_text_from_cache_entry,
};
use corpuswright_core::clean::{CleaningConfig, PdfOcrQuality, PdfTextSource, clean_text};
use corpuswright_core::export::{ExportError, ExportOptions, ExportReport, export_corpus};
use corpuswright_core::pdf::{
    PdfExtractionOptions, PdfPageRangeResult, extract_pdf_page_range, pdf_page_count,
};
use corpuswright_core::pdf_audit::{PdfAuditResult, audit_pdf_files};
use corpuswright_core::preview::{
    CombinedPreview, PreviewOptions, preview_file_with_config, preview_files_with_config,
    preview_processed_files,
};
use corpuswright_core::project_sources::{
    ProjectSource, ProjectSourceExtractionResult, ProjectSourceExtractionRoute, ProjectSourceKind,
    ProjectSourcePdfProfile, ProjectSourcePreviewItem, ProjectSourcePreviewViewport,
    ProjectSourceProcessingResult, ProjectSourceSelectionDescriptor, ProjectSourceStatus,
    stable_project_source_id, update_project_source_extraction_results,
    update_project_source_processing_results,
};
use corpuswright_core::repeated_artifacts::{
    CancellationFlag, RepeatedArtifactScanConfig, RepeatedArtifactScanError,
    RepeatedArtifactScanReport,
};
use corpuswright_core::scan::{
    CorpusSummary, DocumentRecord, DocumentType, ScanReport, load_files, scan_directory,
};
use corpuswright_core::search::{SearchResult, search_corpus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, Window};

use rayon::prelude::*;

mod commands;
mod platform;
mod project;
mod services;
mod state;

use services::*;
use state::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let concordance_cache_root = app
                .path()
                .app_cache_dir()
                .ok()
                .map(|path| path.join("concordance"));
            app.manage(ConcordanceIndexContextState::new(concordance_cache_root));
            if let Ok(pdfium_path) = app
                .path()
                .resolve("ocr/pdfium.dll", BaseDirectory::Resource)
                && let Some(ocr_dir) = pdfium_path.parent()
            {
                let _ = corpuswright_core::pdf_ocr::set_ocr_resource_dir(ocr_dir.to_path_buf());
            }

            #[cfg(target_os = "windows")]
            {
                for window in app.webview_windows().values() {
                    if let Ok(hwnd) = window.hwnd() {
                        platform::window::set_titlebar_color_raw(hwnd.0, 0x00a65800);
                    }
                }
            }

            Ok(())
        })
        .manage(ScanState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(PdfIntakeState {
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .manage(AnnotationBuildState::new())
        .manage(AnalysisState::new())
        .manage(CorpusState {
            inner: RwLock::new(CorpusStateInner::empty()),
        })
        .manage(ExtractionCache::new())
        .manage(PdfPageRangeCache::new())
        .manage(ConcordanceCollocationCache::new())
        .manage(project::ProjectSourceInventoryState::new())
        .invoke_handler(tauri::generate_handler![
            commands::corpus::scan_directory_command,
            commands::corpus::load_files_command,
            commands::corpus::load_project_source_records_command,
            commands::pdf::audit_pdf_files_command,
            commands::pdf::load_pdf_intake_files_command,
            commands::pdf::prepare_project_pdf_sources_command,
            commands::pdf::cancel_pdf_intake_materialization_command,
            commands::corpus::clear_corpus_command,
            commands::corpus::search_corpus_command,
            commands::preview::preview_files_command,
            commands::preview::preview_processed_files_command,
            commands::preview::read_project_source_preview_command,
            commands::preview::query_selected_source_preview_viewport_command,
            commands::analysis::build_annotation_cache_command,
            commands::analysis::cancel_annotation_cache_build_command,
            commands::analysis::build_analysis_corpus_command,
            commands::analysis::cancel_analysis_build_command,
            commands::analysis::get_analysis_build_status_command,
            commands::analysis::compute_corpus_overview_command,
            commands::concordance::search_concordance_command,
            commands::concordance::cancel_concordance_command,
            commands::concordance::compare_expressions_command,
            commands::concordance::query_concordance_collocations_command,
            commands::concordance::expand_concordance_context_command,
            commands::analysis::query_frequency_list_command,
            commands::analysis::query_annotated_frequency_command,
            commands::analysis::query_analysis_documents_command,
            commands::analysis::query_overview_distribution_command,
            commands::analysis::query_overview_distribution_evidence_command,
            commands::analysis::query_term_distribution_command,
            commands::analysis::query_annotated_term_distribution_command,
            commands::pdf::extract_pdf_page_range_command,
            commands::processing::export_corpus_command,
            commands::processing::process_project_sources_command,
            commands::corpus::compute_word_count_command,
            commands::corpus::compute_word_count_window_command,
            commands::processing::scan_repeated_artifacts_command,
            commands::processing::cancel_repeated_artifacts_command,
            commands::files::save_config_file_command,
            commands::files::save_svg_file_command,
            commands::files::read_config_file_command,
            commands::window::set_window_titlebar_color,
            project::create_corpus_project_command,
            project::open_corpus_project_command,
            project::initialise_folder_as_corpus_project_command,
            project::upgrade_project_language_command,
            project::query_project_sources_command,
            project::query_project_source_selection_action_summary_command,
            project::resolve_project_source_selection_pdf_paths_command,
            project::fast_import_project_source_files_command,
            project::fast_import_project_source_folder_command,
            project::remove_project_source_selection_command,
            project::review_project_source_files_import_command,
            project::review_project_source_folder_import_command,
            project::confirm_project_source_files_import_command,
            project::confirm_project_source_folder_import_command,
            project::update_project_source_pdf_handling_command,
            project::load_repeated_artifact_decisions_command,
            project::save_repeated_artifact_decisions_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
