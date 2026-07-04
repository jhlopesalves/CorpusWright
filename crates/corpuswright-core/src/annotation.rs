//! UDPipe-based annotation pipeline: lemma, UPOS, and morphology tagging.
//!
//! Canonical tokens from `tokenization.rs` are the only token identity. The
//! UDPipe binding below builds a pre-tokenised sentence from those surfaces and
//! calls the tagger directly; it never invokes UDPipe tokenisation.
//!
//! Binding path: a core-owned C ABI shim compiled against `udpipe-rs`'s
//! vendored UDPipe public header. `udpipe-rs` is still used for model filenames
//! and downloads, but not for tagging text.

use crate::tokenization::{Token, TokenKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::{Duration, Instant};
use ts_rs::TS;

pub const ANNOTATION_SENTENCE_CHUNK_WORD_CAP: usize = 512;
pub const ANNOTATION_PIPELINE_VERSION: u32 = 2;

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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

    /// Returns the ordered slice of all interned lemmas.
    pub fn entries(&self) -> &[String] {
        &self.reverse
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

const BUNDLED_MODEL_SHA256: &[(&str, &str)] = &[
    (
        "english-ud-2.1-20180111.udpipe",
        "20432a6f87b1f258927207b8fbd2dc21ebff9722b381b7a36cef34c8c9a380dc",
    ),
    (
        "french-sequoia-ud-2.1-20180111.udpipe",
        "d8f3ebb24b00a094b7fe23354730ca55900ad5de4cc07f710025aef949f454f4",
    ),
    (
        "portuguese-ud-2.1-20180111.udpipe",
        "476c5d0d3f435449a4d42629259d0925dfdcd4da71f9175b88e892a6d09d4345",
    ),
];

fn bundled_model_filename(iso: &str) -> Option<&'static str> {
    match iso {
        // These bnosac UD 2.1 models predate the repository renames from
        // UD_English/UD_Portuguese to UD_English-EWT/UD_Portuguese-Bosque.
        "en" => Some("english-ud-2.1-20180111.udpipe"),
        "fr" => Some("french-sequoia-ud-2.1-20180111.udpipe"),
        "pt" => Some("portuguese-ud-2.1-20180111.udpipe"),
        _ => None,
    }
}

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

fn compute_and_verify_model_checksum(path: &Path) -> Result<String, AnnotationError> {
    let mut file = File::open(path).map_err(|error| AnnotationError::ModelLoadFailed {
        path: path.to_path_buf(),
        error: format!("failed to open model for checksum verification: {error}"),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let bytes_read =
            file.read(&mut buffer)
                .map_err(|error| AnnotationError::ModelLoadFailed {
                    path: path.to_path_buf(),
                    error: format!("failed to read model for checksum verification: {error}"),
                })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    let actual = format!("{:02x}", hasher.finalize());

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if let Some((_, expected)) = BUNDLED_MODEL_SHA256
        .iter()
        .find(|(known_name, _)| *known_name == file_name)
        && actual != *expected
    {
        return Err(AnnotationError::ModelLoadFailed {
            path: path.to_path_buf(),
            error: format!("SHA-256 mismatch: expected {expected}, got {actual}"),
        });
    }

    Ok(actual)
}

pub fn model_identity(
    language: &str,
    model_dir: &Path,
) -> Result<(String, String, String, String), AnnotationError> {
    let udpipe_name = language_to_udpipe_name(language);
    let model_name = bundled_model_filename(language)
        .map(str::to_string)
        .unwrap_or_else(|| udpipe_rs::model_filename(&udpipe_name));

    let path = model_dir.join(&model_name);

    let sha256 = compute_and_verify_model_checksum(&path)?;

    Ok((language.to_string(), model_name, udpipe_name, sha256))
}

#[repr(C)]
struct CwUdpipeModel {
    _private: [u8; 0],
}

#[repr(C)]
struct CwUdpipeTaggedResult {
    _private: [u8; 0],
}

#[repr(C)]
struct CwUdpipeWord {
    form: *const c_char,
    lemma: *const c_char,
    upostag: *const c_char,
    xpostag: *const c_char,
    feats: *const c_char,
    misc: *const c_char,
    id: i32,
}

unsafe extern "C" {
    fn cw_udpipe_model_load(model_path: *const c_char) -> *mut CwUdpipeModel;
    fn cw_udpipe_model_free(model: *mut CwUdpipeModel);
    fn cw_udpipe_tag_pretokenized(
        model: *mut CwUdpipeModel,
        tokens: *const *const c_char,
        token_count: usize,
    ) -> *mut CwUdpipeTaggedResult;
    fn cw_udpipe_tagged_result_free(result: *mut CwUdpipeTaggedResult);
    fn cw_udpipe_result_word_count(result: *mut CwUdpipeTaggedResult) -> i32;
    fn cw_udpipe_result_range_count(result: *mut CwUdpipeTaggedResult) -> i32;
    fn cw_udpipe_result_get_word(result: *mut CwUdpipeTaggedResult, index: i32) -> CwUdpipeWord;
    fn cw_udpipe_get_error() -> *const c_char;
}

struct NativeModel {
    ptr: NonNull<CwUdpipeModel>,
}

// SAFETY: CwUdpipeModel is an owned, heap-allocated handle with no
// thread affinity. The shim stores no model ownership or model pointer
// in thread-local or global mutable state; each operation receives the
// model pointer explicitly. The only permitted thread-local state in
// the shim is per-thread error-message storage, not model state.
// NativeModel is never used concurrently by the annotation manager pool:
// each ModelManager is protected by a Mutex in annotation_build.rs, so
// load/tag access to the underlying native model is exclusive. This
// grants Send only, allowing the owned handle to be transferred between
// threads. NativeModel remains !Sync and must not be shared for
// concurrent access.
unsafe impl Send for NativeModel {}

impl NativeModel {
    fn load(path: &Path) -> Result<Self, AnnotationError> {
        let path_string = path.to_string_lossy();
        let c_path = CString::new(path_string.as_bytes()).map_err(|error| {
            AnnotationError::ModelLoadFailed {
                path: path.to_path_buf(),
                error: format!("model path contains an interior NUL byte: {error}"),
            }
        })?;
        let ptr = unsafe { cw_udpipe_model_load(c_path.as_ptr()) };
        let Some(ptr) = NonNull::new(ptr) else {
            return Err(AnnotationError::ModelLoadFailed {
                path: path.to_path_buf(),
                error: last_udpipe_error(),
            });
        };
        Ok(Self { ptr })
    }

    fn tag_pretokenized_words(
        &self,
        tokens: &[&str],
    ) -> Result<(Vec<TaggedWord>, usize), AnnotationError> {
        let c_tokens = tokens
            .iter()
            .map(|token| {
                CString::new(token.as_bytes()).map_err(|error| AnnotationError::AlignmentFailed {
                    reason: format!(
                        "canonical token surface contains an interior NUL byte: {error}"
                    ),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_ptrs = c_tokens
            .iter()
            .map(|token| token.as_ptr())
            .collect::<Vec<_>>();
        let raw_result = unsafe {
            cw_udpipe_tag_pretokenized(self.ptr.as_ptr(), token_ptrs.as_ptr(), token_ptrs.len())
        };
        let Some(raw_result) = NonNull::new(raw_result) else {
            return Err(AnnotationError::TaggingFailed {
                error: last_udpipe_error(),
            });
        };
        let result = NativeTaggedResult { ptr: raw_result };
        result.tagged_words()
    }
}

impl Drop for NativeModel {
    fn drop(&mut self) {
        unsafe { cw_udpipe_model_free(self.ptr.as_ptr()) };
    }
}

struct NativeTaggedResult {
    ptr: NonNull<CwUdpipeTaggedResult>,
}

impl NativeTaggedResult {
    fn tagged_words(&self) -> Result<(Vec<TaggedWord>, usize), AnnotationError> {
        let word_count = unsafe { cw_udpipe_result_word_count(self.ptr.as_ptr()) };
        let word_count =
            usize::try_from(word_count).map_err(|_| AnnotationError::TaggingFailed {
                error: "UDPipe returned a negative word count".to_string(),
            })?;
        let range_count = unsafe { cw_udpipe_result_range_count(self.ptr.as_ptr()) };
        let range_count =
            usize::try_from(range_count).map_err(|_| AnnotationError::TaggingFailed {
                error: "UDPipe returned a negative range-entry count".to_string(),
            })?;

        let mut words = Vec::with_capacity(word_count);
        for index in 0..word_count {
            let word = unsafe { cw_udpipe_result_get_word(self.ptr.as_ptr(), index as i32) };
            words.push(TaggedWord {
                surface: c_string(word.form, "FORM")?,
                lemma: c_string(word.lemma, "LEMMA")?,
                upostag: c_string(word.upostag, "UPOS")?,
                xpostag: c_string(word.xpostag, "XPOS")?,
                feats: c_string(word.feats, "FEATS")?,
                misc: c_string(word.misc, "MISC")?,
                id: word.id,
            });
        }
        Ok((words, range_count))
    }
}

impl Drop for NativeTaggedResult {
    fn drop(&mut self) {
        unsafe { cw_udpipe_tagged_result_free(self.ptr.as_ptr()) };
    }
}

fn c_string(ptr: *const c_char, field: &str) -> Result<String, AnnotationError> {
    if ptr.is_null() {
        return Err(AnnotationError::TaggingFailed {
            error: format!("UDPipe returned a null {field} field"),
        });
    }
    Ok(unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned())
}

fn last_udpipe_error() -> String {
    let ptr = unsafe { cw_udpipe_get_error() };
    if ptr.is_null() {
        return "unknown UDPipe error".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaggedWord {
    surface: String,
    lemma: String,
    upostag: String,
    xpostag: String,
    feats: String,
    misc: String,
    id: i32,
}

fn parse_features(feats: &str) -> HashMap<String, String> {
    let mut features = HashMap::new();
    if feats.is_empty() || feats == "_" {
        return features;
    }
    for pair in feats.split('|') {
        if let Some((key, value)) = pair.split_once('=') {
            features.insert(key.to_string(), value.to_string());
        }
    }
    features
}

fn is_sentence_terminal(surface: &str) -> bool {
    surface
        .chars()
        .any(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

fn is_closing_delimiter(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\''
            | ')'
            | ']'
            | '}'
            | '»'
            | '›'
            | '”'
            | '’'
            | '〉'
            | '》'
            | '」'
            | '』'
            | '】'
            | '〗'
            | '〕'
            | '〙'
            | '〛'
    )
}

fn is_closing_delimiter_token(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Punctuation
        && !token.surface.is_empty()
        && token.surface.chars().all(is_closing_delimiter)
}

pub fn sentence_chunks(tokens: &[Token<'_>], max_word_tokens: usize) -> Vec<Range<usize>> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let max_word_tokens = if max_word_tokens == 0 {
        ANNOTATION_SENTENCE_CHUNK_WORD_CAP
    } else {
        max_word_tokens
    };

    let mut chunks = Vec::new();
    let mut chunk_start = 0usize;
    let mut words_in_chunk = 0usize;
    let mut index = 0usize;

    while index < tokens.len() {
        let token = &tokens[index];
        if token.counts_as_word() {
            if words_in_chunk == max_word_tokens {
                chunks.push(chunk_start..index);
                chunk_start = index;
                words_in_chunk = 0;
            }
            words_in_chunk += 1;
        }

        if token.kind == TokenKind::Punctuation && is_sentence_terminal(token.surface) {
            let mut chunk_end = index + 1;
            while chunk_end < tokens.len() && is_closing_delimiter_token(&tokens[chunk_end]) {
                chunk_end += 1;
            }
            chunks.push(chunk_start..chunk_end);
            chunk_start = chunk_end;
            words_in_chunk = 0;
            index = chunk_end;
        } else {
            index += 1;
        }
    }

    if chunk_start < tokens.len() {
        chunks.push(chunk_start..tokens.len());
    }

    chunks
}

fn align_tagged_words(
    tokens: &[&str],
    words: &[TaggedWord],
    range_count: usize,
) -> Result<Vec<AnnotatedToken>, AnnotationError> {
    if range_count > 0 {
        return Err(AnnotationError::AlignmentFailed {
            reason: format!(
                "UDPipe produced {range_count} multiword-token range entries while tagging pre-tokenised input"
            ),
        });
    }
    if words.len() != tokens.len() {
        return Err(AnnotationError::AlignmentFailed {
            reason: format!(
                "UDPipe produced {} words but canonical token count is {}",
                words.len(),
                tokens.len()
            ),
        });
    }

    let mut annotated = Vec::with_capacity(tokens.len());
    for (token_index, (canonical, word)) in tokens.iter().zip(words.iter()).enumerate() {
        if word.surface != *canonical {
            return Err(AnnotationError::AlignmentFailed {
                reason: format!(
                    "Surface mismatch at token {token_index}: canonical \"{canonical}\", UDPipe \"{}\"",
                    word.surface
                ),
            });
        }
        let lemma = if word.lemma.is_empty() || word.lemma == "_" {
            word.surface.clone()
        } else {
            word.lemma.clone()
        };

        annotated.push(AnnotatedToken {
            surface: word.surface.clone(),
            lemma,
            upos: UniversalPos::from_upos_str(&word.upostag),
            token_index: token_index as u32,
            features: parse_features(&word.feats),
        });
    }

    Ok(annotated)
}

/// Manages loaded UDPipe models keyed by language.
pub struct ModelManager {
    model_dir: PathBuf,
    loaded: HashMap<String, NativeModel>,
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
        let model_name = bundled_model_filename(language)
            .map(str::to_string)
            .unwrap_or_else(|| udpipe_rs::model_filename(&language_to_udpipe_name(language)));
        self.model_dir.join(model_name)
    }

    /// Load a model for `language` (cached).
    pub fn load_model(&mut self, language: &str) -> Result<(), AnnotationError> {
        self.load_model_with_timing(language, || {}, || {}, |_| {}, |_| {})
    }

    pub fn load_model_with_timing<Attempt, Actual, ChecksumTiming, LoadTiming>(
        &mut self,
        language: &str,
        mut record_attempt: Attempt,
        mut record_actual: Actual,
        mut record_checksum_timing: ChecksumTiming,
        mut record_load_timing: LoadTiming,
    ) -> Result<(), AnnotationError>
    where
        Attempt: FnMut(),
        Actual: FnMut(),
        ChecksumTiming: FnMut(Duration),
        LoadTiming: FnMut(Duration),
    {
        record_attempt();
        if self.loaded.contains_key(language) {
            return Ok(());
        }
        let path = self.model_path(language);
        if !path.exists() {
            return Err(AnnotationError::ModelNotFound {
                language: language.to_string(),
            });
        }
        record_actual();
        let checksum_started = Instant::now();
        let checksum_result = compute_and_verify_model_checksum(&path);
        record_checksum_timing(checksum_started.elapsed());
        checksum_result?;
        let load_started = Instant::now();
        let model_result = NativeModel::load(&path);
        record_load_timing(load_started.elapsed());
        let model = model_result?;
        self.loaded.insert(language.to_string(), model);
        Ok(())
    }

    /// Tag pre-tokenised canonical surfaces and return one annotation per
    /// canonical token. Range entries, count mismatches, and surface mismatches
    /// are hard runtime errors because they prove token identity changed.
    pub fn tag_and_align(
        &self,
        language: &str,
        tokens: &[&str],
    ) -> Result<Vec<AnnotatedToken>, AnnotationError> {
        self.tag_and_align_with_timing(language, tokens, |_| {}, |_| {})
    }

    pub fn tag_and_align_with_timing<TagTiming, AlignTiming>(
        &self,
        language: &str,
        tokens: &[&str],
        mut record_tag_timing: TagTiming,
        mut record_alignment_timing: AlignTiming,
    ) -> Result<Vec<AnnotatedToken>, AnnotationError>
    where
        TagTiming: FnMut(Duration),
        AlignTiming: FnMut(Duration),
    {
        let model = self
            .loaded
            .get(language)
            .ok_or_else(|| AnnotationError::ModelNotFound {
                language: language.to_string(),
            })?;
        let tag_started = Instant::now();
        let tagged = model.tag_pretokenized_words(tokens);
        record_tag_timing(tag_started.elapsed());
        let (words, range_count) = tagged?;

        let alignment_started = Instant::now();
        let aligned = align_tagged_words(tokens, &words, range_count);
        record_alignment_timing(alignment_started.elapsed());
        aligned
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

/// Default model directory used by the analysis build.
///
/// Packaged applications can set `CORPUSWRIGHT_UDPIPE_MODEL_DIR` to point at
/// bundled resources. Development builds fall back to `models/udpipe` under the
/// current working directory.
pub fn default_model_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CORPUSWRIGHT_UDPIPE_MODEL_DIR") {
        return PathBuf::from(path);
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        PathBuf::from("models").join("udpipe"),
        PathBuf::from("apps/desktop/src-tauri/resources/udpipe"),
        manifest_dir.join("../../apps/desktop/src-tauri/resources/udpipe"),
        PathBuf::from("resources").join("udpipe"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .unwrap_or_else(|| PathBuf::from("models").join("udpipe"))
}

// ---------------------------------------------------------------------------
// Model fetch + checksum
// ---------------------------------------------------------------------------

/// Downloads a model using udpipe-rs's built-in download mechanism and verifies
/// the result exists.
pub fn download_model_if_missing(language_iso: &str, model_dir: &Path) -> Result<PathBuf, String> {
    if let Some(model_name) = bundled_model_filename(language_iso) {
        let model_path = model_dir.join(model_name);
        if model_path.exists() {
            return Ok(model_path);
        }
        return Err(format!(
            "Bundled UDPipe model for {language_iso} is missing at {}; runtime download is disabled for bundled languages",
            model_path.display()
        ));
    }

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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_token(surface: &'static str, kind: TokenKind, index: usize) -> Token<'static> {
        Token {
            surface,
            byte_start: index,
            byte_end: index + surface.len(),
            document_id: 0,
            kind,
        }
    }

    fn word(surface: &'static str, index: usize) -> Token<'static> {
        test_token(surface, TokenKind::Word, index)
    }

    fn punct(surface: &'static str, index: usize) -> Token<'static> {
        test_token(surface, TokenKind::Punctuation, index)
    }

    fn assert_chunks_cover(tokens: &[Token<'_>], chunks: &[Range<usize>]) {
        let mut expected_start = 0usize;
        for chunk in chunks {
            assert_eq!(chunk.start, expected_start);
            assert!(chunk.start < chunk.end, "chunk must not be empty");
            assert!(chunk.end <= tokens.len(), "chunk must stay inside tokens");
            expected_start = chunk.end;
        }
        assert_eq!(expected_start, tokens.len());
    }

    fn tagged(surface: &str, lemma: &str, upos: &str) -> TaggedWord {
        tagged_with_features(surface, lemma, upos, "_")
    }

    fn tagged_with_features(surface: &str, lemma: &str, upos: &str, feats: &str) -> TaggedWord {
        TaggedWord {
            surface: surface.to_string(),
            lemma: lemma.to_string(),
            upostag: upos.to_string(),
            xpostag: "_".to_string(),
            feats: feats.to_string(),
            misc: "_".to_string(),
            id: 0,
        }
    }

    #[test]
    fn sentence_chunks_split_on_terminal_punctuation_and_attach_closers() {
        let tokens = vec![
            word("Hello", 0),
            punct(".", 1),
            punct("\"", 2),
            word("Go", 3),
            punct("!", 4),
            punct(")", 5),
            word("Why", 6),
            punct("?", 7),
            word("Fine", 8),
            punct("…", 9),
            punct("]", 10),
        ];

        let chunks = sentence_chunks(&tokens, 512);

        assert_eq!(chunks, vec![0..3, 3..6, 6..8, 8..11]);
        assert_chunks_cover(&tokens, &chunks);
    }

    #[test]
    fn sentence_chunks_respect_cap_and_prefer_boundary_at_cap() {
        let tokens = vec![
            word("one", 0),
            word("two", 1),
            punct(".", 2),
            word("three", 3),
        ];

        let chunks = sentence_chunks(&tokens, 2);

        assert_eq!(chunks, vec![0..3, 3..4]);
        assert_chunks_cover(&tokens, &chunks);
    }

    #[test]
    fn sentence_chunks_hard_split_when_cap_is_forced() {
        let tokens = vec![
            word("one", 0),
            word("two", 1),
            word("three", 2),
            word("four", 3),
            word("five", 4),
        ];

        let chunks = sentence_chunks(&tokens, 2);

        assert_eq!(chunks, vec![0..2, 2..4, 4..5]);
        assert_chunks_cover(&tokens, &chunks);
    }

    #[test]
    fn sentence_chunks_zero_cap_uses_default_cap() {
        let tokens = (0..(ANNOTATION_SENTENCE_CHUNK_WORD_CAP + 1))
            .map(|index| word("w", index))
            .collect::<Vec<_>>();

        let chunks = sentence_chunks(&tokens, 0);

        assert_eq!(
            chunks,
            vec![
                0..ANNOTATION_SENTENCE_CHUNK_WORD_CAP,
                ANNOTATION_SENTENCE_CHUNK_WORD_CAP..ANNOTATION_SENTENCE_CHUNK_WORD_CAP + 1
            ]
        );
        assert_chunks_cover(&tokens, &chunks);
    }

    #[test]
    fn sentence_chunks_handle_punctuation_only_and_empty_inputs() {
        let empty: Vec<Token<'static>> = Vec::new();
        assert!(sentence_chunks(&empty, 512).is_empty());

        let tokens = vec![punct("!", 0), punct(")", 1), punct(",", 2)];
        let chunks = sentence_chunks(&tokens, 512);

        assert_eq!(chunks, vec![0..2, 2..3]);
        assert_chunks_cover(&tokens, &chunks);
    }

    #[test]
    fn udpipe_tags_pretokenised_without_resegmentation() {
        let tokens = &["co-operate", "don't", "1,000", "U.S.A.", "Mr."];
        let mut manager = ModelManager::new(default_model_dir());
        manager
            .load_model("en")
            .expect("bundled English UDPipe model should load");
        let annotated = manager
            .tag_and_align("en", tokens)
            .expect("pre-tokenised tagging should preserve canonical tokens");

        assert_eq!(annotated.len(), tokens.len());
        for (index, token) in annotated.iter().enumerate() {
            assert_eq!(token.surface, tokens[index], "surface changed at {index}");
            assert_eq!(token.token_index, index as u32);
            assert!(!token.lemma.is_empty(), "lemma missing at {index}");
        }
    }

    #[test]
    fn annotation_preserves_canonical_tokens() {
        let tokens = &["co-operate", "don't", "1,000", "U.S.A.", "Mr."];
        let words = tokens
            .iter()
            .map(|surface| tagged(surface, surface, "X"))
            .collect::<Vec<_>>();
        let annotated = align_tagged_words(tokens, &words, 0).unwrap();

        assert_eq!(
            annotated
                .iter()
                .map(|token| token.surface.as_str())
                .collect::<Vec<_>>(),
            tokens
        );
    }

    #[test]
    fn annotation_attaches_lemma_and_upos() {
        let tokens = &["running", "cats", "quickly"];
        let words = [
            tagged_with_features("running", "run", "VERB", "VerbForm=Ger"),
            tagged_with_features("cats", "cat", "NOUN", "Number=Plur"),
            tagged("quickly", "quickly", "ADV"),
        ];
        let annotated = align_tagged_words(tokens, &words, 0).unwrap();

        assert_eq!(annotated[0].lemma, "run");
        assert_eq!(annotated[0].upos, UniversalPos::Verb);
        assert_eq!(
            annotated[0].features.get("VerbForm").map(String::as_str),
            Some("Ger")
        );
        assert_eq!(annotated[1].lemma, "cat");
        assert_eq!(annotated[1].upos, UniversalPos::Noun);
        assert_eq!(
            annotated[1].features.get("Number").map(String::as_str),
            Some("Plur")
        );
        assert_eq!(annotated[2].upos, UniversalPos::Adv);
    }

    #[test]
    fn annotation_is_deterministic() {
        let tokens = &["co-operate", "don't", "1,000", "U.S.A.", "Mr."];
        let words = [
            tagged("co-operate", "co-operate", "VERB"),
            tagged("don't", "do", "AUX"),
            tagged("1,000", "1,000", "NUM"),
            tagged("U.S.A.", "U.S.A.", "PROPN"),
            tagged("Mr.", "Mr.", "PROPN"),
        ];
        let first = align_tagged_words(tokens, &words, 0).unwrap();
        let second = align_tagged_words(tokens, &words, 0).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn missing_model_emits_notice() {
        let model_dir = tempfile::tempdir().unwrap();
        let mut manager = ModelManager::new(model_dir.path().to_path_buf());
        let result = manager.load_model("fr");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AnnotationError::ModelNotFound { .. }),
            "Expected ModelNotFound, got: {err}"
        );
    }

    #[test]
    fn bundled_languages_do_not_use_runtime_download() {
        let model_dir = tempfile::tempdir().unwrap();
        let result = download_model_if_missing("en", model_dir.path());

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("runtime download is disabled for bundled languages")
        );
    }

    #[test]
    fn range_entries_are_hard_error() {
        let result = align_tagged_words(&["don't"], &[tagged("don't", "do", "AUX")], 1);
        assert!(matches!(
            result,
            Err(AnnotationError::AlignmentFailed { .. })
        ));
        assert!(result.unwrap_err().to_string().contains("range entries"));
    }

    #[test]
    fn count_mismatch_is_hard_error() {
        let result = align_tagged_words(
            &["co-operate", "don't"],
            &[tagged("co-operate", "co-operate", "VERB")],
            0,
        );
        assert!(matches!(
            result,
            Err(AnnotationError::AlignmentFailed { .. })
        ));
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("canonical token count")
        );
    }

    #[test]
    fn surface_mismatch_is_hard_error() {
        let result = align_tagged_words(
            &["co-operate"],
            &[tagged("co", "co", "X"), tagged("operate", "operate", "X")],
            0,
        );
        assert!(matches!(
            result,
            Err(AnnotationError::AlignmentFailed { .. })
        ));

        let result = align_tagged_words(&["Mr."], &[tagged("Mr", "Mr", "PROPN")], 0);
        assert!(matches!(
            result,
            Err(AnnotationError::AlignmentFailed { .. })
        ));
        assert!(result.unwrap_err().to_string().contains("Surface mismatch"));
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
