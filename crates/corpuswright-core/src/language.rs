//! Per-document language detection and override resolution.
//!
//! Detection uses lingua-rs; the threshold is tunable and defaults to 0.5.
//! Languages below the confidence threshold require an explicit override;
//! if no override is set the document is flagged as low-confidence.

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use ts_rs::TS;

/// Confidence threshold below which detection is considered low-confidence.
/// This is tunable — change the constant to adjust sensitivity.
pub const DEFAULT_LOW_CONFIDENCE_THRESHOLD: f64 = 0.5;

/// A detected language with its confidence score (0.0–1.0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DetectedLanguage {
    /// ISO 639-1 two-letter code (e.g., "en", "pt", "fr").
    pub iso639_1: String,
    /// Confidence score 0.0–1.0.
    #[ts(type = "number")]
    pub confidence: f64,
}

/// The fully-resolved language for one document after detection and override.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LanguageResolution {
    /// Effective ISO 639-1 code used for annotation/stemming.
    pub effective_language: String,
    /// Whether the language was set by manual override.
    pub is_overridden: bool,
    /// Whether detection confidence was below the threshold.
    pub is_low_confidence: bool,
    /// Confidence from detection (None if overridden without detection).
    #[ts(optional)]
    pub confidence: Option<f64>,
}

/// Holds the detection result alongside any manual override for one document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentLanguage {
    /// The raw detected language (always computed).
    pub detected: DetectedLanguage,
    /// Manual override ISO 639-1 code, if set.
    #[ts(optional)]
    pub override_language: Option<String>,
    /// The resolved language after override logic.
    pub resolution: LanguageResolution,
}

static DETECTOR: LazyLock<LanguageDetector> = LazyLock::new(|| {
    LanguageDetectorBuilder::from_languages(&[
        Language::English,
        Language::Portuguese,
        Language::French,
    ])
    .build()
});

/// Runs language detection on arbitrary text, returning the best-matching
/// language and its confidence (0.0–1.0).
pub fn detect_language(text: &str) -> DetectedLanguage {
    if text.trim().is_empty() {
        return DetectedLanguage {
            iso639_1: "en".to_string(),
            confidence: 0.0,
        };
    }
    // lingua 1.x `compute_language_confidence` returns f64 directly
    // (0.0 = not confident, 1.0 = highly confident).
    let conf_en = DETECTOR.compute_language_confidence(text, Language::English);
    let conf_pt = DETECTOR.compute_language_confidence(text, Language::Portuguese);
    let conf_fr = DETECTOR.compute_language_confidence(text, Language::French);

    let (best_conf, best_lang) = if conf_en >= conf_pt && conf_en >= conf_fr {
        (conf_en, "en")
    } else if conf_pt >= conf_fr {
        (conf_pt, "pt")
    } else {
        (conf_fr, "fr")
    };

    DetectedLanguage {
        iso639_1: best_lang.to_string(),
        confidence: best_conf,
    }
}

/// Resolves a DetectedLanguage against an optional manual override and a
/// tunable confidence threshold.
///
/// When confidence is below `threshold` and no override is provided, the
/// returned resolution still has `effective_language` set to the (low-confidence)
/// detection result, but `is_low_confidence` is `true` — callers must flag this
/// rather than silently using the model.
pub fn resolve_language(
    detected: &DetectedLanguage,
    override_language: Option<&str>,
    threshold: f64,
) -> LanguageResolution {
    let override_lang = override_language
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase());

    if let Some(ref lang) = override_lang {
        LanguageResolution {
            effective_language: lang.clone(),
            is_overridden: true,
            is_low_confidence: false,
            confidence: Some(detected.confidence),
        }
    } else {
        LanguageResolution {
            effective_language: detected.iso639_1.clone(),
            is_overridden: false,
            is_low_confidence: detected.confidence < threshold,
            confidence: Some(detected.confidence),
        }
    }
}

/// Convenience: detect + resolve in one call using the default threshold.
pub fn detect_and_resolve(text: &str, override_language: Option<&str>) -> DocumentLanguage {
    let detected = detect_language(text);
    let resolution = resolve_language(
        &detected,
        override_language,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );
    DocumentLanguage {
        detected,
        override_language: override_language.map(str::to_string),
        resolution,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection_known_samples() {
        // Short but distinct samples for the three supported languages.
        let en = detect_language("The government announced new policies today");
        let pt = detect_language("O governo anunciou novas políticas hoje");
        let fr = detect_language("Le gouvernement a annoncé de nouvelles politiques");

        assert_eq!(en.iso639_1, "en");
        assert_eq!(pt.iso639_1, "pt");
        assert_eq!(fr.iso639_1, "fr");
        // All should have non-trivial confidence on clean samples.
        assert!(en.confidence > 0.5);
        assert!(pt.confidence > 0.5);
        assert!(fr.confidence > 0.5);
    }

    #[test]
    fn language_override_takes_precedence() {
        let detected = DetectedLanguage {
            iso639_1: "fr".to_string(),
            confidence: 0.9,
        };
        let resolution = resolve_language(&detected, Some("en"), DEFAULT_LOW_CONFIDENCE_THRESHOLD);
        assert_eq!(resolution.effective_language, "en");
        assert!(resolution.is_overridden);
        assert!(!resolution.is_low_confidence);
    }

    #[test]
    fn low_confidence_requires_or_flags_override() {
        let detected = DetectedLanguage {
            iso639_1: "en".to_string(),
            confidence: 0.3,
        };
        // No override — should flag low confidence.
        let resolution = resolve_language(&detected, None, DEFAULT_LOW_CONFIDENCE_THRESHOLD);
        assert_eq!(resolution.effective_language, "en");
        assert!(!resolution.is_overridden);
        assert!(resolution.is_low_confidence);

        // With override — low confidence flag is cleared.
        let resolved = resolve_language(&detected, Some("fr"), DEFAULT_LOW_CONFIDENCE_THRESHOLD);
        assert_eq!(resolved.effective_language, "fr");
        assert!(resolved.is_overridden);
        assert!(!resolved.is_low_confidence);
    }

    #[test]
    fn empty_text_defaults_to_english_zero_confidence() {
        let detected = detect_language("");
        assert_eq!(detected.iso639_1, "en");
        assert_eq!(detected.confidence, 0.0);
    }

    #[test]
    fn resolution_is_deterministic() {
        let a = detect_and_resolve("Bonjour le monde", None);
        let b = detect_and_resolve("Bonjour le monde", None);
        assert_eq!(a.resolution, b.resolution);
    }

    #[test]
    fn threshold_is_tunable() {
        let detected = DetectedLanguage {
            iso639_1: "pt".to_string(),
            confidence: 0.6,
        };
        // At default 0.5 threshold this is not low-confidence.
        let r = resolve_language(&detected, None, 0.5);
        assert!(!r.is_low_confidence);
        // At a stricter 0.7 threshold it becomes low-confidence.
        let r = resolve_language(&detected, None, 0.7);
        assert!(r.is_low_confidence);
    }
}
