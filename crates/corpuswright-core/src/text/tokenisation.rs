//! Shared Unicode tokenisation for Prepare and Analyse.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Stable identifier recorded with exported corpora.
pub const TOKENIZER_ID: &str = "corpuswright-uax29";
/// Increment when a boundary rule or aggregation key changes.
pub const TOKENIZER_VERSION: &str = "1";
/// Exact unicode-segmentation version resolved by Cargo for this build.
pub const UNICODE_SEGMENTATION_VERSION: &str = env!("CORPUSWRIGHT_UNICODE_SEGMENTATION_VERSION");

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
            unicode_segmentation_version: UNICODE_SEGMENTATION_VERSION.to_string(),
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

fn visit_tailored_boundaries<E>(
    text: &str,
    start: usize,
    end: usize,
    config: &TokenizationConfig,
    visitor: &mut impl FnMut(Boundary) -> Result<(), E>,
) -> Result<(), E> {
    let slice = &text[start..end];
    let mut piece_start = start;
    for (relative, ch) in slice.char_indices() {
        let split_numeric =
            config.split_numeric_separators && is_numeric_separator(slice, relative, ch);
        let split_apostrophe = !config.retain_apostrophe_contractions && matches!(ch, '\'' | '’');
        if split_numeric || split_apostrophe {
            let separator_start = start + relative;
            if piece_start < separator_start {
                visitor(Boundary {
                    start: piece_start,
                    end: separator_start,
                })?;
            }
            visitor(Boundary {
                start: separator_start,
                end: separator_start + ch.len_utf8(),
            })?;
            piece_start = separator_start + ch.len_utf8();
        }
    }
    if piece_start < end {
        visitor(Boundary {
            start: piece_start,
            end,
        })?;
    }
    Ok(())
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

fn emit_token_boundary<'text, E>(
    text: &'text str,
    boundary: Boundary,
    document_id: u32,
    config: &TokenizationConfig,
    visitor: &mut impl FnMut(Token<'text>) -> Result<(), E>,
) -> Result<(), E> {
    let surface = &text[boundary.start..boundary.end];
    let kind = token_kind(surface);
    if kind == TokenKind::Punctuation && !config.include_punctuation {
        return Ok(());
    }
    visitor(Token {
        surface,
        byte_start: boundary.start,
        byte_end: boundary.end,
        document_id,
        kind,
    })
}

/// Visits original-text tokens in canonical order without constructing a
/// document-sized token or boundary vector.
///
/// The only cross-boundary look-ahead retained is the current word component
/// plus a possible ASCII hyphen, which is sufficient for arbitrarily long
/// tailored hyphen chains.
pub fn visit_tokens<'text, E>(
    text: &'text str,
    document_id: u32,
    config: &TokenizationConfig,
    mut visitor: impl FnMut(Token<'text>) -> Result<(), E>,
) -> Result<(), E> {
    let mut current = None::<Boundary>;
    let mut pending_hyphen = None::<Boundary>;

    {
        let mut accept_boundary = |next: Boundary| -> Result<(), E> {
            let Some(mut active) = current else {
                current = Some(next);
                return Ok(());
            };

            if let Some(hyphen) = pending_hyphen.take() {
                if hyphen.end == next.start && boundary_is_word_component(text, next) {
                    active.end = next.end;
                    current = Some(active);
                    return Ok(());
                }
                emit_token_boundary(text, active, document_id, config, &mut visitor)?;
                emit_token_boundary(text, hyphen, document_id, config, &mut visitor)?;
                current = Some(next);
                return Ok(());
            }

            if config.retain_ascii_hyphen_compounds
                && boundary_is_word_component(text, active)
                && active.end == next.start
                && &text[next.start..next.end] == "-"
            {
                current = Some(active);
                pending_hyphen = Some(next);
                return Ok(());
            }

            emit_token_boundary(text, active, document_id, config, &mut visitor)?;
            current = Some(next);
            Ok(())
        };

        for (start, segment) in text.split_word_bound_indices() {
            if segment.chars().all(char::is_whitespace) {
                continue;
            }
            let end = start + segment.len();
            visit_tailored_boundaries(text, start, end, config, &mut accept_boundary)?;
        }
    }

    if let Some(active) = current {
        emit_token_boundary(text, active, document_id, config, &mut visitor)?;
    }
    if let Some(hyphen) = pending_hyphen {
        emit_token_boundary(text, hyphen, document_id, config, &mut visitor)?;
    }
    Ok(())
}

/// Tokenises original text with UAX #29 boundaries and documented compatibility tailorings.
pub fn tokenize<'text>(
    text: &'text str,
    document_id: u32,
    config: &TokenizationConfig,
) -> Vec<Token<'text>> {
    let mut tokens = Vec::new();
    visit_tokens(text, document_id, config, |token| {
        tokens.push(token);
        Ok::<(), std::convert::Infallible>(())
    })
    .expect("infallible token collector");
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_visited<'text>(
        text: &'text str,
        document_id: u32,
        config: &TokenizationConfig,
    ) -> Vec<Token<'text>> {
        let mut visited = Vec::new();
        visit_tokens(text, document_id, config, |token| {
            visited.push(token);
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap();
        visited
    }

    fn assert_visitor_parity(text: &str, config: TokenizationConfig) {
        assert_eq!(
            tokenize(text, 37, &config),
            collect_visited(text, 37, &config)
        );
    }

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

    #[test]
    fn streaming_visitor_matches_collector_across_tailorings_and_unicode() {
        let fixtures = [
            "",
            "plain words and 123 numbers",
            "CAFE\u{301} café e\u{301}lan",
            "don't don’t rock'n'roll l’esprit",
            "3.14 1,000 १२,३४५",
            "well-being state-of-the-art a-b-c-d-e",
            "alpha—beta; (gamma)! … punctuation",
            "\u{2003}\n\t",
        ];
        let default = TokenizationConfig::default();
        let configs = [
            default,
            TokenizationConfig {
                retain_ascii_hyphen_compounds: false,
                ..default
            },
            TokenizationConfig {
                retain_apostrophe_contractions: false,
                ..default
            },
            TokenizationConfig {
                split_numeric_separators: false,
                ..default
            },
            TokenizationConfig {
                include_punctuation: false,
                ..default
            },
            TokenizationConfig {
                fold_aggregation_keys: false,
                ..default
            },
        ];

        for config in configs {
            for fixture in fixtures {
                assert_visitor_parity(fixture, config);
            }
        }
    }

    #[test]
    fn streaming_visitor_handles_long_hyphen_chains_and_large_document() {
        let long_chain = std::iter::repeat_n("part", 20_000)
            .collect::<Vec<_>>()
            .join("-");
        assert_visitor_parity(&long_chain, TokenizationConfig::default());
        assert_eq!(surfaces(&long_chain), [long_chain.as_str()]);

        let large_document = "alpha 3.14 state-of-the-art don’t café\n".repeat(100_000);
        assert_visitor_parity(&large_document, TokenizationConfig::default());
    }

    #[test]
    fn segmentation_version_matches_lockfile() {
        let lockfile = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"));
        let mut in_unicode_segmentation = false;
        let mut resolved = None;
        for line in lockfile.lines() {
            let trimmed = line.trim();
            if trimmed == "[[package]]" {
                in_unicode_segmentation = false;
            } else if trimmed == "name = \"unicode-segmentation\"" {
                in_unicode_segmentation = true;
            } else if in_unicode_segmentation
                && let Some(version) = trimmed
                    .strip_prefix("version = \"")
                    .and_then(|value| value.strip_suffix('"'))
            {
                resolved = Some(version);
                break;
            }
        }
        assert_eq!(UNICODE_SEGMENTATION_VERSION, resolved.unwrap());
        assert_eq!(
            TokenizationManifest::default().unicode_segmentation_version,
            resolved.unwrap()
        );
    }
}
