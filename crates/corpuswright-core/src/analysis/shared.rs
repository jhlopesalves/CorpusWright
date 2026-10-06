use std::collections::HashSet;
use std::sync::LazyLock;

pub(super) const MAX_QUERY_WINDOW: usize = 500;
/// Lower fixed DP reference used for literal interval labels.
///
/// Plain Gries-style DP is unnormalised here: its upper bound depends on the
/// document-size shares, so these references are implementation heuristics,
/// not validated qualitative categories or cross-corpus cut-offs.
pub(super) const ANNOTATED_DP_BROADLY_DISPERSED_MAX: f64 = 0.30;
/// Upper fixed DP reference used for literal interval labels.
pub(super) const ANNOTATED_DP_HIGHLY_CONCENTRATED_MIN: f64 = 0.70;

use crate::analysis::frequency::case_fold_surface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StopwordLanguage {
    English,
    French,
    Portuguese,
}

impl StopwordLanguage {
    pub(super) fn from_subtag(subtag: Option<&str>) -> Self {
        let Some(tag) = subtag else {
            return Self::English;
        };
        let tag = tag.trim().to_lowercase();
        let primary = tag.split('-').next().unwrap_or(&tag);
        match primary {
            "fr" => Self::French,
            "pt" => Self::Portuguese,
            _ => Self::English,
        }
    }
}

fn load_stopword_set(raw: &str) -> HashSet<String> {
    raw.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(case_fold_surface)
        .collect()
}

pub(super) static STOPWORDS_EN: LazyLock<HashSet<String>> =
    LazyLock::new(|| load_stopword_set(include_str!("../../resources/stopwords/en.txt")));

pub(super) static STOPWORDS_FR: LazyLock<HashSet<String>> =
    LazyLock::new(|| load_stopword_set(include_str!("../../resources/stopwords/fr.txt")));

pub(super) static STOPWORDS_PT: LazyLock<HashSet<String>> =
    LazyLock::new(|| load_stopword_set(include_str!("../../resources/stopwords/pt.txt")));

pub(super) fn stopword_matches(value: &str, language: StopwordLanguage) -> bool {
    let folded = case_fold_surface(value);
    match language {
        StopwordLanguage::English => STOPWORDS_EN.contains(&folded),
        StopwordLanguage::French => STOPWORDS_FR.contains(&folded),
        StopwordLanguage::Portuguese => STOPWORDS_PT.contains(&folded),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_from_subtag() {
        assert_eq!(
            StopwordLanguage::from_subtag(None),
            StopwordLanguage::English
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("en")),
            StopwordLanguage::English
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("EN-us")),
            StopwordLanguage::English
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("fr")),
            StopwordLanguage::French
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("FR-ca")),
            StopwordLanguage::French
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("pt-br")),
            StopwordLanguage::Portuguese
        );
        // Fallback to English
        assert_eq!(
            StopwordLanguage::from_subtag(Some("de")),
            StopwordLanguage::English
        );
        assert_eq!(
            StopwordLanguage::from_subtag(Some("")),
            StopwordLanguage::English
        );
    }

    #[test]
    fn filters_stopwords_by_language() {
        assert!(stopword_matches("the", StopwordLanguage::English));
        assert!(!stopword_matches("the", StopwordLanguage::French));
        assert!(!stopword_matches("the", StopwordLanguage::Portuguese));

        assert!(stopword_matches("le", StopwordLanguage::French));
        assert!(!stopword_matches("le", StopwordLanguage::English));

        assert!(stopword_matches("como", StopwordLanguage::Portuguese));

        // Case folding works
        assert!(stopword_matches("THE", StopwordLanguage::English));
        assert!(stopword_matches("Le", StopwordLanguage::French));
    }
}
