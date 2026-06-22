//! UDPipe-based annotation pipeline: lemma, UPOS, and morphology tagging.
//!
//! Canonical tokens from `tokenization.rs` are aligned with UDPipe output by
//! surface form. UDPipe always runs its own segmentation; surface-form
//! alignment ensures we never re-segment, and mismatches are surfaced as
//! errors.
//!
//! Binding path: `udpipe-rs` (0.2.0) — Rust bindings that compile on Windows
//! MSVC via a vendored C++ UDPipe wrapper. The crate does not expose a
//! pre-tokenised API, so we align by surface form.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Closed UPOS enum (u8-backed, not String)
// ---------------------------------------------------------------------------

/// Universal POS tag as a closed, compact enum.
///
/// Only the UPOS tags used in UD v2 are included. Unknown tags or empty values
/// from the pipeline are stored as `X`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
#[repr(u8)]
pub enum UniversalPos {
    Adj = 0,
    Adp = 1,
    Adv = 2,
    Aux = 3,
    Cconj = 4,
    Det = 5,
    Intj = 6,
    Noun = 7,
    Num = 8,
    Part = 9,
    Pron = 10,
    Propn = 11,
    Punct = 12,
    Sconj = 13,
    Sym = 14,
    Verb = 15,
    X = 16,
}

impl UniversalPos {
    /// Parse the UPOS string returned by UDPipe (or any CoNLL-U XPOS column).
    pub fn from_upos_str(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "ADJ" => Self::Adj,
            "ADP" => Self::Adp,
            "ADV" => Self::Adv,
            "AUX" => Self::Aux,
            "CCONJ" => Self::Cconj,
            "DET" => Self::Det,
            "INTJ" => Self::Intj,
            "NOUN" => Self::Noun,
            "NUM" => Self::Num,
            "PART" => Self::Part,
            "PRON" => Self::Pron,
            "PROPN" => Self::Propn,
            "PUNCT" => Self::Punct,
            "SCONJ" => Self::Sconj,
            "SYM" => Self::Sym,
            "VERB" => Self::Verb,
            _ => Self::X,
        }
    }

    /// Returns true if this UPOS matches a filter string (case-insensitive).
    pub fn matches_filter(self, filter: &str) -> bool {
        self.as_upos_str().eq_ignore_ascii_case(filter.trim())
    }

    pub fn as_upos_str(self) -> &'static str {
        match self {
            Self::Adj => "ADJ",
            Self::Adp => "ADP",
            Self::Adv => "ADV",
            Self::Aux => "AUX",
            Self::Cconj => "CCONJ",
            Self::Det => "DET",
            Self::Intj => "INTJ",
            Self::Noun => "NOUN",
            Self::Num => "NUM",
            Self::Part => "PART",
            Self::Pron => "PRON",
            Self::Propn => "PROPN",
            Self::Punct => "PUNCT",
            Self::Sconj => "SCONJ",
            Self::Sym => "SYM",
            Self::Verb => "VERB",
            Self::X => "X",
        }
    }
}

// ---------------------------------------------------------------------------
// Annotated token
// ---------------------------------------------------------------------------

/// One token annotated by UDPipe, aligned to a canonical corpuswright token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotatedToken {
    /// The surface form (must match the canonical token's surface).
    pub surface: String,
    /// Lemma form.
    pub lemma: String,
    /// Universal POS tag.
    pub upos: UniversalPos,
    /// Index into the canonical token list for this document.
    #[ts(type = "number")]
    pub token_index: u32,
    /// Additional morphological features from the FEATS column.
    #[serde(default)]
    pub features: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Compact annotation storage per surface term
// ---------------------------------------------------------------------------

/// Per-surface-type annotation data stored in the analysis index.
///
/// `lemma_id` is an index into an interning table stored on `AnalysisCorpus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceAnnotation {
    /// Index into the lemma intern table (-1 for none).
    pub lemma_id: i32,
    /// Universal POS tag (X for unannotated).
    pub upos: UniversalPos,
}

/// Lemma intern table for compact storage.
///
/// Each unique lemma string gets a single u32 id shared across the corpus.
#[derive(Debug, Clone, Default)]
pub struct LemmaTable {
    /// lemma → id mapping
    map: HashMap<String, u32>,
    /// id → lemma reverse mapping
    reverse: Vec<String>,
}

impl LemmaTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a lemma string, returning its stable id.
    pub fn intern(&mut self, lemma: &str) -> u32 {
        if let Some(&id) = self.map.get(lemma) {
            return id;
        }
        let id = u32::try_from(self.reverse.len()).unwrap_or(u32::MAX);
        self.map.insert(lemma.to_string(), id);
        self.reverse.push(lemma.to_string());
        id
    }

    /// Look up a lemma by id.
    pub fn lookup(&self, id: u32) -> Option<&str> {
        self.reverse.get(id as usize).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.reverse.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Model management
// ---------------------------------------------------------------------------

/// Errors that can occur during annotation.
#[derive(Debug, Clone)]
pub enum AnnotationError {
    ModelNotFound { language: String },
    ModelLoadFailed { path: PathBuf, error: String },
    TaggingFailed { error: String },
    AlignmentFailed { reason: String },
}

impl std::fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotFound { language } => {
                write!(
                    f,
                    "No UDPipe model is available for language \"{language}\""
                )
            }
            Self::ModelLoadFailed { path, error } => {
                write!(
                    f,
                    "Failed to load UDPipe model at {}: {error}",
                    path.display()
                )
            }
            Self::TaggingFailed { error } => {
                write!(f, "UDPipe tagging failed: {error}")
            }
            Self::AlignmentFailed { reason } => {
                write!(f, "Token alignment failed: {reason}")
            }
        }
    }
}

impl std::error::Error for AnnotationError {}

/// Manages loaded UDPipe models keyed by language.
pub struct ModelManager {
    model_dir: PathBuf,
    loaded: HashMap<String, udpipe_rs::Model>,
}

impl ModelManager {
    pub fn new(model_dir: PathBuf) -> Self {
        Self {
            model_dir,
            loaded: HashMap::new(),
        }
    }

    /// Returns the path where the model for `language` is expected.
    pub fn model_path(&self, language: &str) -> PathBuf {
        let model_name = udpipe_rs::model_filename(&language_to_udpipe_name(language));
        self.model_dir.join(model_name)
    }

    /// Load a model for `language` (cached).
    pub fn load_model(&mut self, language: &str) -> Result<(), AnnotationError> {
        if self.loaded.contains_key(language) {
            return Ok(());
        }
        let path = self.model_path(language);
        if !path.exists() {
            return Err(AnnotationError::ModelNotFound {
                language: language.to_string(),
            });
        }
        let model =
            udpipe_rs::Model::load(&path).map_err(|error| AnnotationError::ModelLoadFailed {
                path: path.clone(),
                error: error.to_string(),
            })?;
        self.loaded.insert(language.to_string(), model);
        Ok(())
    }

    /// Tag raw text, align returned words to canonical tokens by surface form,
    /// and return per-canonical-token AnnotatedToken entries.
    ///
    /// UDPipe always runs its own tokenization. We reconstruct the original
    /// text from canonical surface tokens (space-separated) and feed it to
    /// UDPipe. Then we walk the UDPipe words in order and match each canonical
    /// token by surface form, consuming one UDPipe word per canonical token.
    /// Mismatches are surfaced as AlignmentFailed.
    pub fn tag_and_align(
        &self,
        language: &str,
        tokens: &[&str],
    ) -> Result<Vec<AnnotatedToken>, AnnotationError> {
        let model = self
            .loaded
            .get(language)
            .ok_or_else(|| AnnotationError::ModelNotFound {
                language: language.to_string(),
            })?;

        // Reconstruct text: space-separated canonical tokens.
        let text = tokens.join(" ");

        let words = model
            .parse(&text)
            .map_err(|error| AnnotationError::TaggingFailed {
                error: error.to_string(),
            })?;

        // Align by surface form — walk both lists in order.
        let mut annotated = Vec::with_capacity(tokens.len());
        let mut word_idx = 0usize;

        for (tok_idx, canonical) in tokens.iter().enumerate() {
            // Skip any multiword-token entries (range entries) in UDPipe output.
            // Words derived from MWT have `id` values like 1-2; we skip those.
            while word_idx < words.len() && words[word_idx].form.is_empty() {
                word_idx += 1;
            }
            if word_idx >= words.len() {
                return Err(AnnotationError::AlignmentFailed {
                    reason: format!(
                        "UDPipe produced fewer words ({}) than canonical tokens ({})",
                        words.len(),
                        tokens.len()
                    ),
                });
            }
            let ud_word = &words[word_idx];
            let surface = ud_word.form.clone();

            if surface != *canonical {
                return Err(AnnotationError::AlignmentFailed {
                    reason: format!(
                        "Surface mismatch at token {tok_idx}: expected \"{canonical}\", UDPipe produced \"{surface}\""
                    ),
                });
            }

            let lemma = if ud_word.lemma.is_empty() || ud_word.lemma == "_" {
                surface.clone()
            } else {
                ud_word.lemma.clone()
            };

            let upos = UniversalPos::from_upos_str(&ud_word.upostag);

            let mut features = HashMap::new();
            if !ud_word.feats.is_empty() && ud_word.feats != "_" {
                for pair in ud_word.feats.split('|') {
                    if let Some((k, v)) = pair.split_once('=') {
                        features.insert(k.to_string(), v.to_string());
                    }
                }
            }

            annotated.push(AnnotatedToken {
                surface,
                lemma,
                upos,
                token_index: tok_idx as u32,
                features,
            });

            word_idx += 1;
        }

        if annotated.len() != tokens.len() {
            return Err(AnnotationError::AlignmentFailed {
                reason: format!(
                    "Aligned {} tokens but expected {}",
                    annotated.len(),
                    tokens.len()
                ),
            });
        }

        Ok(annotated)
    }
}

// ---------------------------------------------------------------------------
// Language name mapping
// ---------------------------------------------------------------------------

/// Maps an ISO 639-1 code to the udpipe-rs model name.
fn language_to_udpipe_name(iso: &str) -> String {
    match iso {
        "en" => "english-ewt".to_string(),
        "pt" => "portuguese-bosque".to_string(),
        "fr" => "french-sequoia".to_string(),
        other => other.to_string(),
    }
}

/// Returns the udpipe-rs model name for a given ISO 639-1 code.
pub fn udpipe_model_name(iso: &str) -> String {
    language_to_udpipe_name(iso)
}

// ---------------------------------------------------------------------------
// Model fetch + checksum
// ---------------------------------------------------------------------------

/// Downloads a model using udpipe-rs's built-in download mechanism and verifies
/// the result exists.
pub fn download_model_if_missing(language_iso: &str, model_dir: &Path) -> Result<PathBuf, String> {
    let udpipe_name = language_to_udpipe_name(language_iso);
    let model_path = model_dir.join(udpipe_rs::model_filename(&udpipe_name));

    if model_path.exists() {
        return Ok(model_path);
    }

    std::fs::create_dir_all(model_dir)
        .map_err(|e| format!("Failed to create model directory: {e}"))?;

    udpipe_rs::download_model(&udpipe_name, model_dir)
        .map_err(|e| format!("Failed to download model for {language_iso}: {e}"))?;

    if !model_path.exists() {
        return Err(format!(
            "Model download claimed success but file missing: {}",
            model_path.display()
        ));
    }

    Ok(model_path)
}

// ---------------------------------------------------------------------------
// Spike tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: get a model directory for tests (cached under temp/).
    fn test_model_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("corpuswright-test-models");
        std::fs::create_dir_all(&dir).ok();
        dir
    }

    /// Spike: downloads the English-EWT model (if needed), tags text, and
    /// asserts that canonical tokens align 1:1 with UDPipe output for simple
    /// English text where both tokenizers agree.
    #[test]
    fn udpipe_tags_pretokenised_without_resegmentation() {
        let model_dir = test_model_dir();

        // Download model.
        let _model_path = match download_model_if_missing("en", &model_dir) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("Skipping spike test (model download failed): {e}");
                return;
            }
        };

        let mut manager = ModelManager::new(model_dir);
        manager
            .load_model("en")
            .expect("Failed to load English model");

        // Canonical tokens for "The cat runs." — both UAX #29 and UDPipe should agree.
        let tokens = &["The", "cat", "runs"];
        let annotated = manager
            .tag_and_align("en", tokens)
            .expect("Tag + align failed");

        // 1:1 token identity.
        assert_eq!(
            annotated.len(),
            tokens.len(),
            "Token count mismatch — re-segmentation detected"
        );
        for (i, tok) in annotated.iter().enumerate() {
            assert_eq!(tok.surface, tokens[i], "Surface mismatch at index {i}");
        }

        // Lemma and UPOS present.
        for tok in &annotated {
            assert!(!tok.lemma.is_empty(), "Lemma must not be empty");
            assert!(!tok.lemma.starts_with('_'), "Lemma must not be '_'");
            assert!(
                matches!(
                    tok.upos,
                    UniversalPos::Det | UniversalPos::Noun | UniversalPos::Verb
                ),
                "Unexpected UPOS {:?} for \"{}\"",
                tok.upos,
                tok.surface
            );
        }

        eprintln!(
            "Spike PASSED: {} tokens, lemmas={:?}, upos={:?}",
            annotated.len(),
            annotated.iter().map(|t| &t.lemma).collect::<Vec<_>>(),
            annotated
                .iter()
                .map(|t| t.upos.as_upos_str())
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn annotation_preserves_canonical_tokens() {
        udpipe_tags_pretokenised_without_resegmentation();
    }

    #[test]
    fn annotation_attaches_lemma_and_upos() {
        let model_dir = test_model_dir();
        if download_model_if_missing("en", &model_dir).is_err() {
            eprintln!("Skipping test (model download failed)");
            return;
        }

        let mut manager = ModelManager::new(model_dir);
        manager.load_model("en").expect("Failed to load model");

        let annotated = manager
            .tag_and_align("en", &["running", "cats", "quickly"])
            .expect("Tag + align failed");

        for tok in &annotated {
            assert!(!tok.lemma.is_empty());
            assert_ne!(tok.upos, UniversalPos::X);
        }
    }

    #[test]
    fn annotation_is_deterministic() {
        let model_dir = test_model_dir();
        if download_model_if_missing("en", &model_dir).is_err() {
            eprintln!("Skipping test (model download failed)");
            return;
        }

        let mut manager = ModelManager::new(model_dir);
        manager.load_model("en").expect("Failed to load model");

        let tokens = &["The", "quick", "brown", "fox", "jumps"];
        let first = manager
            .tag_and_align("en", tokens)
            .expect("First tagging failed");
        let second = manager
            .tag_and_align("en", tokens)
            .expect("Second tagging failed");

        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.surface, b.surface);
            assert_eq!(a.lemma, b.lemma);
            assert_eq!(a.upos, b.upos);
        }
    }

    #[test]
    fn missing_model_emits_notice() {
        let mut manager = ModelManager::new(test_model_dir());
        let result = manager.tag_and_align("fr", &["bonjour"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AnnotationError::ModelNotFound { .. }),
            "Expected ModelNotFound, got: {err}"
        );
    }

    #[test]
    fn universal_pos_parsing() {
        assert_eq!(UniversalPos::from_upos_str("NOUN"), UniversalPos::Noun);
        assert_eq!(UniversalPos::from_upos_str("verb"), UniversalPos::Verb);
        assert_eq!(UniversalPos::from_upos_str("adj"), UniversalPos::Adj);
        assert_eq!(UniversalPos::from_upos_str("unknown-tag"), UniversalPos::X);
        assert_eq!(UniversalPos::from_upos_str(""), UniversalPos::X);
        assert_eq!(UniversalPos::from_upos_str("_"), UniversalPos::X);
    }

    #[test]
    fn lemma_table_intern_and_lookup() {
        let mut table = LemmaTable::new();
        let id1 = table.intern("be");
        let id2 = table.intern("run");
        let id3 = table.intern("be"); // should return same id
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(table.lookup(id1), Some("be"));
        assert_eq!(table.lookup(id2), Some("run"));
        assert_eq!(table.lookup(999), None);
    }
}
