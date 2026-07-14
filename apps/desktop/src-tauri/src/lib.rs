//! Composes the desktop runtime from feature-owned commands and state.
//!
//! Tauri registration remains explicit here so every IPC endpoint and managed state has one
//! auditable owner. Analytical behaviour stays in `corpuswright-core`; feature services own desktop
//! orchestration, progress events, and cache lifecycles.

use corpuswright_core::sources::cache::ExtractionCache;
use std::sync::RwLock;
use tauri::Manager;
use tauri::path::BaseDirectory;

mod features;
mod platform;
mod runtime;

use features::analysis::annotation_state::AnnotationBuildState;
use features::analysis::state::AnalysisState;
use features::concordance::state::{ConcordanceCollocationCache, ConcordanceIndexContextState};
use features::corpus::scanning_state::ScanState;
use features::corpus::state::{CorpusState, CorpusStateInner};
use features::pdf::state::{PdfIntakeState, PdfPageRangeCache};
use features::project;
use runtime::cancellation::ActiveCancellation;
use runtime::config::DesktopRuntimeConfig;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let runtime_config = DesktopRuntimeConfig::from_environment();
    runtime_config.install();
    let ocr_resource_override = runtime_config
        .ocr_resource_override()
        .map(ToOwned::to_owned);

    tauri::Builder::default()
        .manage(runtime_config)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let concordance_cache_root = app
                .path()
                .app_cache_dir()
                .ok()
                .map(|path| path.join("concordance"));
            app.manage(ConcordanceIndexContextState::new(concordance_cache_root));
            let packaged_ocr_dir = app.path().resolve("ocr", BaseDirectory::Resource).ok();
            if let Some(ocr_dir) = ocr_resource_override.clone().or(packaged_ocr_dir) {
                let _ = corpuswright_core::sources::pdf::ocr::set_ocr_resource_dir(ocr_dir);
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
            cancel: ActiveCancellation::new(),
        })
        .manage(PdfIntakeState {
            cancel: ActiveCancellation::new(),
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
            features::corpus::commands::scan_directory_command,
            features::corpus::commands::load_files_command,
            features::corpus::commands::load_project_source_records_command,
            features::pdf::commands::audit_pdf_files_command,
            features::pdf::commands::load_pdf_intake_files_command,
            features::pdf::commands::prepare_project_pdf_sources_command,
            features::pdf::commands::cancel_pdf_intake_materialization_command,
            features::corpus::commands::clear_corpus_command,
            features::corpus::commands::search_corpus_command,
            features::preview::preview_files_command,
            features::preview::preview_processed_files_command,
            features::preview::read_project_source_preview_command,
            features::preview::query_selected_source_preview_viewport_command,
            features::analysis::commands::build_annotation_cache_command,
            features::analysis::commands::cancel_annotation_cache_build_command,
            features::analysis::commands::build_analysis_corpus_command,
            features::analysis::commands::cancel_analysis_build_command,
            features::analysis::commands::get_analysis_build_status_command,
            features::analysis::commands::compute_corpus_overview_command,
            features::concordance::commands::search_concordance_command,
            features::concordance::commands::cancel_concordance_command,
            features::concordance::commands::compare_expressions_command,
            features::concordance::commands::query_concordance_collocations_command,
            features::concordance::commands::expand_concordance_context_command,
            features::analysis::commands::query_frequency_list_command,
            features::analysis::commands::query_annotated_frequency_command,
            features::analysis::commands::query_analysis_documents_command,
            features::analysis::commands::query_overview_distribution_command,
            features::analysis::commands::query_overview_distribution_evidence_command,
            features::analysis::commands::query_term_distribution_command,
            features::analysis::commands::query_annotated_term_distribution_command,
            features::pdf::commands::extract_pdf_page_range_command,
            features::processing::commands::export_corpus_command,
            features::processing::commands::process_project_sources_command,
            features::corpus::commands::compute_word_count_command,
            features::corpus::commands::compute_word_count_window_command,
            features::processing::commands::scan_repeated_artifacts_command,
            features::processing::commands::cancel_repeated_artifacts_command,
            features::files::save_config_file_command,
            features::files::save_svg_file_command,
            features::files::read_config_file_command,
            platform::commands::set_window_titlebar_color,
            features::project::create_corpus_project_command,
            features::project::open_corpus_project_command,
            features::project::initialise_folder_as_corpus_project_command,
            features::project::upgrade_project_language_command,
            features::project::query_project_sources_command,
            features::project::query_project_source_selection_action_summary_command,
            features::project::resolve_project_source_selection_pdf_paths_command,
            features::project::fast_import_project_source_files_command,
            features::project::fast_import_project_source_folder_command,
            features::project::remove_project_source_selection_command,
            features::project::review_project_source_files_import_command,
            features::project::review_project_source_folder_import_command,
            features::project::confirm_project_source_files_import_command,
            features::project::confirm_project_source_folder_import_command,
            features::project::update_project_source_pdf_handling_command,
            features::project::load_repeated_artifact_decisions_command,
            features::project::save_repeated_artifact_decisions_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
