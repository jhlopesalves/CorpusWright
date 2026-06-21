//! Shared Unicode tokenisation for Prepare and Analyse.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Stable identifier recorded with exported corpora.
pub const TOKENIZER_ID: &str = "corpuswright-uax29";
/// Increment when a boundary rule or aggregation key changes.
pub const TOKENIZER_VERSION: &str = "1";

/// Unicode normalisation applied to aggregation keys.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UnicodeNormalizationForm {
    Nfc,
}

/// The explicit token boundary and aggregation policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TokenizationConfig {
    pub normalization: UnicodeNormalizationForm,
    // Preserve Letter/Number + U+002D + Letter/Number chains as one token.
    pub retain_ascii_hyphen_compounds: bool,
    // Preserve contractions containing U+0027 or U+2019.
    pub retain_apostrophe_contractions: bool,
    // Split decimal points and thousands separators out of numeric tokens.
    pub split_numeric_separators: bool,
    // Emit non-whitespace punctuation as tokens rather than discarding it.
    pub include_punctuation: bool,
    // NFC-normalise and Unicode-lowercase aggregation keys.
    pub fold_aggregation_keys: bool,
}

impl Default for TokenizationConfig {
    fn default() -> Self {
        Self {
            normalization: UnicodeNormalizationForm::Nfc,
            retain_ascii_hyphen_compounds: true,
            retain_apostrophe_contractions: true,
            split_numeric_separators: true,
            include_punctuation: true,
            fold_aggregation_keys: true,
        }
    }
}

/// Tokeniser identity and policy written to the export manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TokenizationManifest {
    pub id: String,
    pub version: String,
    pub unicode_segmentation_version: String,
    pub config: TokenizationConfig,
}

impl Default for TokenizationManifest {
    fn default() -> Self {
        Self {
            id: TOKENIZER_ID.to_string(),
            version: TOKENIZER_VERSION.to_string(),
            unicode_segmentation_version: "1.13.3".to_string(),
            config: TokenizationConfig::default(),
        }
    }
}

/// Broad token class retained in the shared token stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Word,
    Number,
    Punctuation,
    Other,
}

/// One surface token with offsets into the original, unnormalised input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'text> {
    pub surface: &'text str,
    pub byte_start: usize,
    pub byte_end: usize,
    pub document_id: u32,
    pub kind: TokenKind,
}

impl Token<'_> {
    /// Returns the stable type key used by frequency and diversity measures.
    pub fn aggregation_key(&self, config: &TokenizationConfig) -> String {
        if config.fold_aggregation_keys {
            match config.normalization {
                UnicodeNormalizationForm::Nfc => {
                    self.surface.nfc().collect::<String>().to_lowercase()
                }
            }
        } else {
            self.surface.to_string()
        }
    }

    /// Prepare's user-facing word count excludes standalone punctuation.
    pub fn counts_as_word(&self) -> bool {
        matches!(self.kind, TokenKind::Word | TokenKind::Number)
    }
}

#[derive(Debug, Clone, Copy)]
struct Boundary {
    start: usize,
    end: usize,
}

fn is_numeric_separator(text: &str, byte_index: usize, ch: char) -> bool {
    if !matches!(ch, '.' | ',') {
        return false;
    }
    let previous = text[..byte_index].chars().next_back();
    let next_index = byte_index + ch.len_utf8();
    let next = text[next_index..].chars().next();
    previous.is_some_and(char::is_numeric) && next.is_some_and(char::is_numeric)
}

fn split_tailored_boundaries(
    text: &str,
    start: usize,
    end: usize,
    config: &TokenizationConfig,
    output: &mut Vec<Boundary>,
) {
    let slice = &text[start..end];
    let mut piece_start = start;
    for (relative, ch) in slice.char_indices() {
        let split_numeric =
            config.split_numeric_separators && is_numeric_separator(slice, relative, ch);
        let split_apostrophe = !config.retain_apostrophe_contractions && matches!(ch, '\'' | '’');
        if split_numeric || split_apostrophe {
            let separator_start = start + relative;
            if piece_start < separator_start {
                output.push(Boundary {
                    start: piece_start,
                    end: separator_start,
                });
            }
            output.push(Boundary {
                start: separator_start,
                end: separator_start + ch.len_utf8(),
            });
            piece_start = separator_start + ch.len_utf8();
        }
    }
    if piece_start < end {
        output.push(Boundary {
            start: piece_start,
            end,
        });
    }
}

fn boundary_is_word_component(text: &str, boundary: Boundary) -> bool {
    let surface = &text[boundary.start..boundary.end];
    surface.chars().next().is_some_and(char::is_alphanumeric)
        && surface
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
}

fn token_kind(surface: &str) -> TokenKind {
    if surface.chars().any(char::is_alphabetic) {
        TokenKind::Word
    } else if surface.chars().all(char::is_numeric) {
        TokenKind::Number
    } else if surface.chars().all(|ch| !ch.is_alphanumeric()) {
        TokenKind::Punctuation
    } else {
        TokenKind::Other
    }
}

/// Tokenises original text with UAX #29 boundaries and documented compatibility tailorings.
pub fn tokenize<'text>(
    text: &'text str,
    document_id: u32,
    config: &TokenizationConfig,
) -> Vec<Token<'text>> {
    let mut boundaries = Vec::new();
    for (start, segment) in text.split_word_bound_indices() {
        if segment.chars().all(char::is_whitespace) {
            continue;
        }
        let end = start + segment.len();
        split_tailored_boundaries(text, start, end, config, &mut boundaries);
    }

    let mut joined = Vec::with_capacity(boundaries.len());
    let mut index = 0;
    while index < boundaries.len() {
        let mut boundary = boundaries[index];
        if config.retain_ascii_hyphen_compounds && boundary_is_word_component(text, boundary) {
            while index + 2 < boundaries.len()
                && boundary.end == boundaries[index + 1].start
                && &text[boundaries[index + 1].start..boundaries[index + 1].end] == "-"
                && boundaries[index + 1].end == boundaries[index + 2].start
                && boundary_is_word_component(text, boundaries[index + 2])
            {
                boundary.end = boundaries[index + 2].end;
                index += 2;
            }
        }
        joined.push(boundary);
        index += 1;
    }

    joined
        .into_iter()
        .filter_map(|boundary| {
            let surface = &text[boundary.start..boundary.end];
            let kind = token_kind(surface);
            if kind == TokenKind::Punctuation && !config.include_punctuation {
                return None;
            }
            Some(Token {
                surface,
                byte_start: boundary.start,
                byte_end: boundary.end,
                document_id,
                kind,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surfaces(text: &str) -> Vec<&str> {
        tokenize(text, 7, &TokenizationConfig::default())
            .into_iter()
            .map(|token| token.surface)
            .collect()
    }

    #[test]
    fn golden_tailored_boundaries_are_stable() {
        assert_eq!(
            surfaces("well-being state-of-the-art"),
            ["well-being", "state-of-the-art"]
        );
        assert_eq!(surfaces("don't don’t"), ["don't", "don’t"]);
        assert_eq!(surfaces("3.14 1,000"), ["3", ".", "14", "1", ",", "000"]);
        assert_eq!(
            surfaces("-leading trailing-"),
            ["-", "leading", "trailing", "-"]
        );
        assert_eq!(surfaces("alpha—beta"), ["alpha", "—", "beta"]);
    }

    #[test]
    fn preserves_surface_offsets_and_document_id() {
        let text = "élan well-being";
        let tokens = tokenize(text, 42, &TokenizationConfig::default());
        assert_eq!(tokens[1].surface, "well-being");
        assert_eq!(
            &text[tokens[1].byte_start..tokens[1].byte_end],
            "well-being"
        );
        assert_eq!(tokens[1].document_id, 42);
    }

    #[test]
    fn aggregation_keys_normalise_and_fold_without_changing_surface() {
        let text = "CAFE\u{301}";
        let tokens = tokenize(text, 0, &TokenizationConfig::default());
        assert_eq!(tokens[0].surface, text);
        assert_eq!(
            tokens[0].aggregation_key(&TokenizationConfig::default()),
            "café"
        );
    }

    #[test]
    fn tokenisation_is_deterministic() {
        let config = TokenizationConfig::default();
        assert_eq!(
            tokenize("One—two 3.14", 9, &config),
            tokenize("One—two 3.14", 9, &config)
        );
    }

    #[test]
    fn explicit_config_can_disable_compatibility_tailorings() {
        let config = TokenizationConfig {
            retain_ascii_hyphen_compounds: false,
            retain_apostrophe_contractions: false,
            ..TokenizationConfig::default()
        };
        let surfaces = tokenize("well-being don't", 0, &config)
            .into_iter()
            .map(|token| token.surface)
            .collect::<Vec<_>>();
        assert_eq!(surfaces, ["well", "-", "being", "don", "'", "t"]);
    }
}
