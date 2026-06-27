//! Per-document language detection and override resolution.
//!
//! Detection uses lingua-rs; the threshold is tunable and defaults to 0.5.
//! Languages below the confidence threshold require an explicit override;
//! if no override is set the document is flagged as low-confidence.

use crate::scan::DocumentRecord;
use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use ts_rs::TS;

/// Confidence threshold below which detection is considered low-confidence.
/// This is tunable — change the constant to adjust sensitivity.
pub const DEFAULT_LOW_CONFIDENCE_THRESHOLD: f64 = 0.5;
const CONFIDENCE_STABILITY_SCALE: f64 = 1_000_000_000_000.0;

/// Project-level language supported by CorpusWright annotation resources.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ProjectLanguage {
    En,
    Fr,
    Pt,
}

impl ProjectLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
            Self::Pt => "pt",
        }
    }
}

/// Resolves the monolingual project language to every source record.
pub fn resolve_effective_document_languages(
    project_language: ProjectLanguage,
    records: &[DocumentRecord],
) -> HashMap<usize, String> {
    let language = project_language.code();
    records
        .iter()
        .enumerate()
        .map(|(index, _)| (index, language.to_string()))
        .collect()
}

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
    /// The raw detected language, absent when an override short-circuits detection.
    #[ts(optional)]
    pub detected: Option<DetectedLanguage>,
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

fn stable_confidence(confidence: f64) -> f64 {
    if confidence.is_finite() {
        (confidence * CONFIDENCE_STABILITY_SCALE).round() / CONFIDENCE_STABILITY_SCALE
    } else {
        confidence
    }
}

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
        confidence: stable_confidence(best_conf),
    }
}

fn normalised_override_language(override_language: Option<&str>) -> Option<String> {
    override_language
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
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
    if let Some(lang) = normalised_override_language(override_language) {
        LanguageResolution {
            effective_language: lang,
            is_overridden: true,
            is_low_confidence: false,
            confidence: None,
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
    if let Some(lang) = normalised_override_language(override_language) {
        return DocumentLanguage {
            detected: None,
            override_language: Some(lang.clone()),
            resolution: LanguageResolution {
                effective_language: lang,
                is_overridden: true,
                is_low_confidence: false,
                confidence: None,
            },
        };
    }

    let detected = detect_language(text);
    let resolution = resolve_language(&detected, None, DEFAULT_LOW_CONFIDENCE_THRESHOLD);
    DocumentLanguage {
        detected: Some(detected),
        override_language: None,
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
        assert_eq!(resolution.confidence, None);
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
        assert_eq!(resolved.confidence, None);
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
        assert_eq!(
            a.resolution.effective_language,
            b.resolution.effective_language
        );
        assert_eq!(a.resolution.is_overridden, b.resolution.is_overridden);
        assert_eq!(
            a.resolution.is_low_confidence,
            b.resolution.is_low_confidence
        );

        let a_confidence = a.resolution.confidence.unwrap();
        let b_confidence = b.resolution.confidence.unwrap();
        assert_eq!(a_confidence, b_confidence);
    }

    #[test]
    fn override_short_circuits_detection_result() {
        let resolved = detect_and_resolve("Bonjour le monde", Some("EN"));
        assert_eq!(resolved.detected, None);
        assert_eq!(resolved.override_language.as_deref(), Some("en"));
        assert_eq!(resolved.resolution.effective_language, "en");
        assert!(resolved.resolution.is_overridden);
        assert!(!resolved.resolution.is_low_confidence);
        assert_eq!(resolved.resolution.confidence, None);
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
