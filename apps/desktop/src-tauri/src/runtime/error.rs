//! Stable internal classification while this release deliberately retains string IPC errors.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopErrorClass {
    InvalidInput,
    UnsupportedInput,
    Cancelled,
    StaleResult,
    ResourceUnavailable,
    ResourceLimit,
    CorruptPersistentState,
    NativeFailure,
    InternalInvariant,
}

impl DesktopErrorClass {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::UnsupportedInput => "unsupported_input",
            Self::Cancelled => "cancelled",
            Self::StaleResult => "stale_result",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::ResourceLimit => "resource_limit",
            Self::CorruptPersistentState => "corrupt_persistent_state",
            Self::NativeFailure => "native_failure",
            Self::InternalInvariant => "internal_invariant",
        }
    }
}

pub(crate) fn classify_message(message: &str) -> DesktopErrorClass {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cancel") {
        DesktopErrorClass::Cancelled
    } else if lower.contains("reloaded") || lower.contains("stale") {
        DesktopErrorClass::StaleResult
    } else if lower.contains("unsupported") {
        DesktopErrorClass::UnsupportedInput
    } else if lower.contains("memory limit") || lower.contains("resource limit") {
        DesktopErrorClass::ResourceLimit
    } else if lower.contains("corrupt") || lower.contains("version mismatch") {
        DesktopErrorClass::CorruptPersistentState
    } else if lower.contains("pdfium") || lower.contains("ocr") || lower.contains("native") {
        DesktopErrorClass::NativeFailure
    } else if lower.contains("not available") || lower.contains("not found") {
        DesktopErrorClass::ResourceUnavailable
    } else if lower.contains("invalid") || lower.contains("choose ") || lower.contains("must ") {
        DesktopErrorClass::InvalidInput
    } else {
        DesktopErrorClass::InternalInvariant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_codes_cover_representative_existing_messages() {
        assert_eq!(
            classify_message("Analysis build cancelled.").code(),
            "cancelled"
        );
        assert_eq!(
            classify_message("Corpus has been reloaded.").code(),
            "stale_result"
        );
        assert_eq!(
            classify_message("memory limit exceeded").code(),
            "resource_limit"
        );
    }
}
