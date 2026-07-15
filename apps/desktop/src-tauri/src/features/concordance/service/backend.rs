//! Backend selection and stable route labels.

use corpuswright_core::analysis::{ConcordanceBackendUsed, ConcordanceFallbackReason};

use super::telemetry::log_string;

pub(crate) const CONCORDANCE_BACKEND_ENV: &str = "CORPUSWRIGHT_CONCORDANCE_BACKEND";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConcordanceBackendMode {
    DefaultIndex,
    Scan,
    StrictIndex,
    Audit,
}

impl ConcordanceBackendMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DefaultIndex => "default_index",
            Self::Scan => "scan",
            Self::StrictIndex => "strict_index",
            Self::Audit => "audit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConcordanceBackendModeParse {
    pub(crate) mode: ConcordanceBackendMode,
    pub(crate) unknown_value: Option<String>,
}

pub(crate) fn parse_concordance_backend_mode(value: Option<&str>) -> ConcordanceBackendModeParse {
    let Some(trimmed) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return ConcordanceBackendModeParse {
            mode: ConcordanceBackendMode::DefaultIndex,
            unknown_value: None,
        };
    };

    let mode = match trimmed.to_ascii_lowercase().as_str() {
        "scan" => ConcordanceBackendMode::Scan,
        "index" => ConcordanceBackendMode::StrictIndex,
        "audit" => ConcordanceBackendMode::Audit,
        _ => {
            return ConcordanceBackendModeParse {
                mode: ConcordanceBackendMode::DefaultIndex,
                unknown_value: Some(trimmed.to_string()),
            };
        }
    };

    ConcordanceBackendModeParse {
        mode,
        unknown_value: None,
    }
}

pub(crate) fn concordance_backend_mode_from_env() -> ConcordanceBackendMode {
    match std::env::var(CONCORDANCE_BACKEND_ENV) {
        Ok(value) => {
            let parsed = parse_concordance_backend_mode(Some(&value));
            if let Some(unknown_value) = parsed.unknown_value.as_deref() {
                eprintln!(
                    "[concordance-backend] mode=default_index warning=unknown_env env={} value={}",
                    CONCORDANCE_BACKEND_ENV,
                    log_string(unknown_value, 80)
                );
            }
            parsed.mode
        }
        Err(std::env::VarError::NotPresent) => ConcordanceBackendMode::DefaultIndex,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[concordance-backend] mode=default_index warning=invalid_env_unicode env={}",
                CONCORDANCE_BACKEND_ENV
            );
            ConcordanceBackendMode::DefaultIndex
        }
    }
}

pub(crate) fn concordance_backend_used_label(backend: ConcordanceBackendUsed) -> &'static str {
    match backend {
        ConcordanceBackendUsed::Scan => "scan",
        ConcordanceBackendUsed::Index => "index",
        ConcordanceBackendUsed::ScanFallback { .. } => "scan_fallback",
    }
}

fn concordance_fallback_reason_label(reason: ConcordanceFallbackReason) -> &'static str {
    match reason {
        ConcordanceFallbackReason::IndexError => "index_error",
    }
}

pub(crate) fn concordance_fallback_label(backend: ConcordanceBackendUsed) -> &'static str {
    match backend {
        ConcordanceBackendUsed::ScanFallback { reason } => {
            concordance_fallback_reason_label(reason)
        }
        ConcordanceBackendUsed::Scan | ConcordanceBackendUsed::Index => "none",
    }
}

#[cfg(test)]
pub(crate) fn concordance_backend_used_is_index_error_fallback(
    backend: ConcordanceBackendUsed,
) -> bool {
    matches!(
        backend,
        ConcordanceBackendUsed::ScanFallback {
            reason: ConcordanceFallbackReason::IndexError,
        }
    )
}
