//! Product runtime settings parsed, validated, and installed once at desktop startup.

use crate::features::concordance::service::{
    ConcordanceBackendMode, concordance_backend_mode_from_env,
};
use corpuswright_core::analysis::ConcordanceAnalysisMemoryLimits;
use corpuswright_core::analysis::concordance::index::SurfaceIndexResourceLimits;
use std::path::{Path, PathBuf};

const OCR_WORKERS_ENV: &str = "CORPUSWRIGHT_OCR_WORKERS";
const ANNOTATION_TIMING_ENV: &str = "CORPUSWRIGHT_ANNOTATION_TIMING";
const OCR_DIR_ENV: &str = "CORPUSWRIGHT_OCR_DIR";
const UDPIPE_MODEL_DIR_ENV: &str = "CORPUSWRIGHT_UDPIPE_MODEL_DIR";
const VERIFY_CACHE_WRITE_ENV: &str = "CORPUSWRIGHT_VERIFY_CACHE_WRITE";

pub(crate) struct DesktopRuntimeConfig {
    concordance_backend: ConcordanceBackendMode,
    surface_index_limits: SurfaceIndexResourceLimits,
    concordance_memory_limits: ConcordanceAnalysisMemoryLimits,
    ocr_worker_count: usize,
    annotation_timing: bool,
    ocr_resource_override: Option<PathBuf>,
    udpipe_model_override: Option<PathBuf>,
    verify_cache_write: bool,
}

impl DesktopRuntimeConfig {
    pub(crate) fn from_environment() -> Self {
        let available_workers = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(crate::features::pdf::service::DEFAULT_OCR_WORKERS)
            .max(1);
        let default_workers = crate::features::pdf::service::DEFAULT_OCR_WORKERS
            .min(available_workers)
            .max(1);
        let ocr_worker_count = positive_usize(
            OCR_WORKERS_ENV,
            default_workers,
            crate::features::pdf::service::MAX_INTERNAL_OCR_WORKERS.min(available_workers),
        );

        let surface_defaults = SurfaceIndexResourceLimits::conservative();
        let surface_index_limits = SurfaceIndexResourceLimits {
            max_projected_resident_index_bytes: positive_u64(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_RESIDENT_BYTES",
                surface_defaults.max_projected_resident_index_bytes,
            ),
            max_staged_prepared_text_bytes: positive_u64(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TEXT_BYTES",
                surface_defaults.max_staged_prepared_text_bytes,
            ),
            max_staged_token_records: positive_u64(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TOKEN_RECORDS",
                surface_defaults.max_staged_token_records,
            ),
            max_retained_query_sort_records: positive_u64(
                "CORPUSWRIGHT_SURFACE_INDEX_MAX_SORT_RECORDS",
                surface_defaults.max_retained_query_sort_records,
            ),
        };

        let memory_defaults = ConcordanceAnalysisMemoryLimits::conservative();
        let concordance_memory_limits = ConcordanceAnalysisMemoryLimits {
            max_combined_bytes: positive_u64(
                corpuswright_core::analysis::CONCORDANCE_MAX_COMBINED_BYTES_ENV,
                memory_defaults.max_combined_bytes,
            ),
            safety_margin_bytes: memory_defaults.safety_margin_bytes,
        };

        Self {
            concordance_backend: concordance_backend_mode_from_env(),
            surface_index_limits,
            concordance_memory_limits,
            ocr_worker_count,
            annotation_timing: boolean_setting(ANNOTATION_TIMING_ENV, false),
            ocr_resource_override: path_setting(OCR_DIR_ENV),
            udpipe_model_override: path_setting(UDPIPE_MODEL_DIR_ENV),
            verify_cache_write: boolean_setting(VERIFY_CACHE_WRITE_ENV, false),
        }
    }

    pub(crate) fn install(&self) {
        let _ = self.surface_index_limits.install_as_default();
        let _ = self.concordance_memory_limits.install_as_default();
        let _ = corpuswright_core::annotation::cache::install_cache_write_verification(
            self.verify_cache_write,
        );
        let _ = crate::features::pdf::service::install_ocr_worker_count(self.ocr_worker_count);
    }

    pub(crate) const fn concordance_backend(&self) -> ConcordanceBackendMode {
        self.concordance_backend
    }

    pub(crate) const fn annotation_timing(&self) -> bool {
        self.annotation_timing
    }

    pub(crate) fn ocr_resource_override(&self) -> Option<&Path> {
        self.ocr_resource_override.as_deref()
    }

    pub(crate) fn udpipe_model_override(&self) -> Option<&Path> {
        self.udpipe_model_override.as_deref()
    }
}

fn positive_u64(name: &str, fallback: u64) -> u64 {
    match std::env::var(name) {
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(parsed) if parsed > 0 => parsed,
            _ => {
                rejected(name, &value, &fallback.to_string());
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => fallback,
        Err(std::env::VarError::NotUnicode(_)) => {
            rejected(name, "<non-unicode>", &fallback.to_string());
            fallback
        }
    }
}

fn positive_usize(name: &str, fallback: usize, maximum: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => match value.trim().parse::<usize>() {
            Ok(parsed) if parsed > 0 => parsed.min(maximum).max(1),
            _ => {
                rejected(name, &value, &fallback.to_string());
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => fallback,
        Err(std::env::VarError::NotUnicode(_)) => {
            rejected(name, "<non-unicode>", &fallback.to_string());
            fallback
        }
    }
}

fn boolean_setting(name: &str, fallback: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim() {
            "1" | "true" => true,
            "0" | "false" => false,
            _ => {
                rejected(name, &value, if fallback { "true" } else { "false" });
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => fallback,
        Err(std::env::VarError::NotUnicode(_)) => {
            rejected(
                name,
                "<non-unicode>",
                if fallback { "true" } else { "false" },
            );
            fallback
        }
    }
}

fn path_setting(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).and_then(|value| {
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            eprintln!("[runtime-config] setting={name} rejected=empty fallback=automatic");
            None
        } else {
            Some(path)
        }
    })
}

fn rejected(name: &str, value: &str, fallback: &str) {
    eprintln!(
        "[runtime-config] setting={} rejected={:?} fallback={}",
        name, value, fallback
    );
}
