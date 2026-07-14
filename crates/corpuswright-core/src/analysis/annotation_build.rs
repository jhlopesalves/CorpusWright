use super::build::{
    AnalysisBuildError, AnalysisBuildOptions, AnalysisCorpus, AnalysisDocument,
    build_analysis_corpus, compare_document_records, document_name,
};
use super::concordance::analysis_word_tokens;
use super::input::{AnalysisInputProvenance, AnalysisInputResolution};
use crate::annotation::cache::{AnnotationLayerCacheEnvelope, AnnotationManifest};
use crate::annotation::{
    ANNOTATION_SENTENCE_CHUNK_WORD_CAP, AnnotatedToken, AnnotationError, LemmaTable, ModelManager,
    UniversalPos, default_model_dir, sentence_chunks,
};
use crate::corpus::DocumentRecord;
use crate::text::language::detect_and_resolve;
use crate::text::tokenisation::{Token, TokenizationConfig, tokenize};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};
use ts_rs::TS;
const ANNOTATION_TIMING_ENV: &str = "CORPUSWRIGHT_ANNOTATION_TIMING";
const ANNOTATION_WAVE_CHUNKS_PER_WORKER: usize = 16;
#[cfg(test)]
pub(super) static TEST_ANNOTATION_TIMING_ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotatedTermCount {
    pub surface_term_id: u32,
    pub lemma_id: u32,
    pub upos: UniversalPos,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationLayer {
    pub documents: Vec<AnnotationLayerDocument>,
    pub lemma_table: LemmaTable,
    pub languages_used: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationLayerDocument {
    /// Index into `AnalysisCorpus.documents`.
    pub analysis_document_index: usize,
    pub source_path: String,
    pub relative_path: String,
    pub term_counts: Vec<AnnotatedTermCount>,
}

#[derive(Debug, Clone, Default)]
pub struct AnnotationLayerBuildOptions {
    pub udpipe_model_dir: Option<PathBuf>,
    /// Per-document language override keyed by the original records slice,
    /// before the corpus build sorts records into analysis order.
    pub language_overrides: HashMap<usize, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationLayerBuildError {
    Cancelled,
    TextLoadFailed {
        source_path: String,
        error: String,
    },
    CorpusDocumentMismatch {
        reason: String,
    },
    UnsupportedLanguages {
        languages: Vec<String>,
    },
    ModelPreflightFailed {
        language: String,
        error: String,
    },
    ModelTaggingFailed {
        analysis_document_index: usize,
        language: String,
        error: String,
    },
    AlignmentOrIdentityMismatch {
        analysis_document_index: usize,
        reason: String,
    },
}

impl std::fmt::Display for AnnotationLayerBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Annotation layer build cancelled."),
            Self::TextLoadFailed { source_path, error } => {
                write!(
                    formatter,
                    "Failed to load analysis text for {source_path}: {error}"
                )
            }
            Self::CorpusDocumentMismatch { reason } => {
                write!(
                    formatter,
                    "Annotation inputs do not match the analysis corpus: {reason}"
                )
            }
            Self::UnsupportedLanguages { languages } => {
                write!(
                    formatter,
                    "No UDPipe model is available for language(s): {}",
                    languages.join(", ")
                )
            }
            Self::ModelPreflightFailed { language, error } => {
                write!(
                    formatter,
                    "Failed to preflight UDPipe model for language {language}: {error}"
                )
            }
            Self::ModelTaggingFailed {
                analysis_document_index,
                language,
                error,
            } => {
                write!(
                    formatter,
                    "UDPipe tagging failed for analysis document {analysis_document_index} ({language}): {error}"
                )
            }
            Self::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason,
            } => {
                write!(
                    formatter,
                    "Annotation identity mismatch for analysis document {analysis_document_index}: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for AnnotationLayerBuildError {}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(tag = "type")]
pub enum AnnotationCacheBuildPhase {
    BuildingSurface { phase: String },
    LoadingModel,
    Annotating,
    WritingCache,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct AnnotationCacheBuildProgress {
    pub files_done: usize,
    pub files_total: usize,
    pub tokens_done: u64,
    pub tokens_total: u64,
    pub current_phase: AnnotationCacheBuildPhase,
}

fn annotation_build_progress(
    files_done: usize,
    files_total: usize,
    current_phase: AnnotationCacheBuildPhase,
) -> AnnotationCacheBuildProgress {
    annotation_build_progress_with_tokens(files_done, files_total, 0, 0, current_phase)
}

fn annotation_build_progress_with_tokens(
    files_done: usize,
    files_total: usize,
    tokens_done: u64,
    tokens_total: u64,
    current_phase: AnnotationCacheBuildPhase,
) -> AnnotationCacheBuildProgress {
    AnnotationCacheBuildProgress {
        files_done,
        files_total,
        tokens_done,
        tokens_total,
        current_phase,
    }
}

pub(crate) struct AnnotationBuildTimingCounters {
    internal_surface_rebuild_ns: AtomicU64,
    manifest_cache_ready_ns: AtomicU64,
    annotation_input_load_ns: AtomicU64,
    identity_verification_ns: AtomicU64,
    in_loop_retokenisation_ns: AtomicU64,
    model_checksum_ns: AtomicU64,
    model_load_ns: AtomicU64,
    model_load_attempts: AtomicU64,
    model_load_actual: AtomicU64,
    manager_instances_created: AtomicU64,
    annotation_segments_processed: AtomicU64,
    udpipe_tag_call_ns: AtomicU64,
    alignment_ns: AtomicU64,
    local_counting_ns: AtomicU64,
    reduce_global_interning_ns: AtomicU64,
    cache_serialise_ns: AtomicU64,
    cache_write_ns: AtomicU64,
    cache_readback_verification_ns: AtomicU64,
    cache_readback_verification_runs: AtomicU64,
}

impl AnnotationBuildTimingCounters {
    pub(crate) fn new() -> Self {
        Self {
            internal_surface_rebuild_ns: AtomicU64::new(0),
            manifest_cache_ready_ns: AtomicU64::new(0),
            annotation_input_load_ns: AtomicU64::new(0),
            identity_verification_ns: AtomicU64::new(0),
            in_loop_retokenisation_ns: AtomicU64::new(0),
            model_checksum_ns: AtomicU64::new(0),
            model_load_ns: AtomicU64::new(0),
            model_load_attempts: AtomicU64::new(0),
            model_load_actual: AtomicU64::new(0),
            manager_instances_created: AtomicU64::new(0),
            annotation_segments_processed: AtomicU64::new(0),
            udpipe_tag_call_ns: AtomicU64::new(0),
            alignment_ns: AtomicU64::new(0),
            local_counting_ns: AtomicU64::new(0),
            reduce_global_interning_ns: AtomicU64::new(0),
            cache_serialise_ns: AtomicU64::new(0),
            cache_write_ns: AtomicU64::new(0),
            cache_readback_verification_ns: AtomicU64::new(0),
            cache_readback_verification_runs: AtomicU64::new(0),
        }
    }

    fn add_duration(bucket: &AtomicU64, duration: Duration) {
        let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        bucket.fetch_add(nanos, AtomicOrdering::Relaxed);
    }

    fn load_duration(bucket: &AtomicU64) -> Duration {
        Duration::from_nanos(bucket.load(AtomicOrdering::Relaxed))
    }

    fn record_internal_surface_rebuild(&self, duration: Duration) {
        Self::add_duration(&self.internal_surface_rebuild_ns, duration);
    }

    fn record_manifest_cache_ready(&self, duration: Duration) {
        Self::add_duration(&self.manifest_cache_ready_ns, duration);
    }

    fn record_annotation_input_load(&self, duration: Duration) {
        Self::add_duration(&self.annotation_input_load_ns, duration);
    }

    fn record_identity_verification(&self, duration: Duration) {
        Self::add_duration(&self.identity_verification_ns, duration);
    }

    fn record_in_loop_retokenisation(&self, duration: Duration) {
        Self::add_duration(&self.in_loop_retokenisation_ns, duration);
    }

    fn record_model_checksum(&self, duration: Duration) {
        Self::add_duration(&self.model_checksum_ns, duration);
    }

    fn record_model_load(&self, duration: Duration) {
        Self::add_duration(&self.model_load_ns, duration);
    }

    fn record_model_load_attempt(&self) {
        self.model_load_attempts
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn record_model_load_actual(&self) {
        self.model_load_actual.fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn record_manager_instance_created(&self) {
        self.manager_instances_created
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn record_annotation_segment_processed(&self) {
        self.annotation_segments_processed
            .fetch_add(1, AtomicOrdering::Relaxed);
    }

    fn record_udpipe_tag_call(&self, duration: Duration) {
        Self::add_duration(&self.udpipe_tag_call_ns, duration);
    }

    fn record_alignment(&self, duration: Duration) {
        Self::add_duration(&self.alignment_ns, duration);
    }

    fn record_local_counting(&self, duration: Duration) {
        Self::add_duration(&self.local_counting_ns, duration);
    }

    fn record_reduce_global_interning(&self, duration: Duration) {
        Self::add_duration(&self.reduce_global_interning_ns, duration);
    }

    pub(crate) fn record_cache_serialise(&self, duration: Duration) {
        Self::add_duration(&self.cache_serialise_ns, duration);
    }

    pub(crate) fn record_cache_write(&self, duration: Duration) {
        Self::add_duration(&self.cache_write_ns, duration);
    }

    pub(crate) fn record_cache_readback_verification(&self, duration: Duration) {
        self.cache_readback_verification_runs
            .fetch_add(1, AtomicOrdering::Relaxed);
        Self::add_duration(&self.cache_readback_verification_ns, duration);
    }

    pub(crate) fn readback_verification_ran(&self) -> bool {
        self.cache_readback_verification_runs
            .load(AtomicOrdering::Relaxed)
            > 0
    }
}

fn annotation_timing_enabled() -> bool {
    #[cfg(test)]
    if TEST_ANNOTATION_TIMING_ENABLED.load(AtomicOrdering::Relaxed) {
        return true;
    }

    std::env::var(ANNOTATION_TIMING_ENV).as_deref() == Ok("1")
}

fn log_annotation_cache_decision(outcome: &str, mismatch: Option<&str>) {
    if !annotation_timing_enabled() {
        return;
    }

    if let Some(mismatch) = mismatch {
        eprintln!("annotation-cache decision: {outcome} (mismatch: {mismatch})");
    } else {
        eprintln!("annotation-cache decision: {outcome}");
    }
}

fn annotation_cache_file_path(cache_root: &Path) -> PathBuf {
    cache_root.join(format!(
        "annotation-layer.v{}.json",
        crate::annotation::cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ))
}

fn read_annotation_cache_envelope_for_diagnostics(
    cache_root: &Path,
) -> Option<AnnotationLayerCacheEnvelope> {
    let bytes = std::fs::read(annotation_cache_file_path(cache_root)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn annotation_manifest_mismatch_for_diagnostics(
    cache_file_version: u32,
    cached: &AnnotationManifest,
    expected: &AnnotationManifest,
) -> Option<&'static str> {
    if cache_file_version != crate::annotation::cache::ANNOTATION_LAYER_CACHE_FILE_VERSION {
        return Some("schema_version");
    }
    if cached.schema_version != expected.schema_version {
        return Some("schema_version");
    }
    if cached.annotation_pipeline_version != expected.annotation_pipeline_version {
        return Some("annotation_pipeline_version");
    }
    if cached.tokenization != expected.tokenization {
        return Some("tokenization_config");
    }
    if cached.corpus.documents_hash_sha256 != expected.corpus.documents_hash_sha256 {
        return Some("documents_hash");
    }
    if cached.corpus.surface_terms_hash_sha256 != expected.corpus.surface_terms_hash_sha256 {
        return Some("surface_terms_hash");
    }
    if cached.corpus.processed_texts_hash_sha256 != expected.corpus.processed_texts_hash_sha256
        || cached.corpus.decode_policy_version != expected.corpus.decode_policy_version
    {
        return Some("processed_texts_hash");
    }
    if cached.models != expected.models {
        return Some("model_identity");
    }
    if cached.corpus.documents != expected.corpus.documents {
        return Some("documents_hash");
    }
    if cached.corpus.surface_term_count != expected.corpus.surface_term_count {
        return Some("surface_terms_hash");
    }
    if cached.languages != expected.languages {
        return Some("language_identity");
    }
    if cached.corpus != expected.corpus {
        return Some("documents_hash");
    }
    if cached != expected {
        return Some("manifest_identity");
    }
    None
}

fn annotation_cache_invalid_mismatch_for_diagnostics(detail: &str) -> &'static str {
    let detail = detail.to_ascii_lowercase();
    if detail.contains("surface term") {
        "surface_terms_hash"
    } else if detail.contains("document") {
        "documents_hash"
    } else if detail.contains("schema") {
        "schema_version"
    } else if detail.contains("model") {
        "model_identity"
    } else {
        "cache_file"
    }
}

fn annotation_cache_mismatch_for_diagnostics(
    cache_root: &Path,
    expected_manifest: &AnnotationManifest,
    invalid_detail: Option<&str>,
) -> Option<&'static str> {
    read_annotation_cache_envelope_for_diagnostics(cache_root)
        .and_then(|envelope| {
            annotation_manifest_mismatch_for_diagnostics(
                envelope.cache_file_version,
                &envelope.manifest,
                expected_manifest,
            )
        })
        .or_else(|| invalid_detail.map(annotation_cache_invalid_mismatch_for_diagnostics))
}

fn log_annotation_cache_load_decision(
    cache_root: &Path,
    expected_manifest: &AnnotationManifest,
    load: &crate::annotation::cache::AnnotationLayerCacheLoad,
) {
    match load {
        crate::annotation::cache::AnnotationLayerCacheLoad::Ready(_) => {
            log_annotation_cache_decision("ready", None);
        }
        crate::annotation::cache::AnnotationLayerCacheLoad::Missing => {
            log_annotation_cache_decision("missing", None);
        }
        crate::annotation::cache::AnnotationLayerCacheLoad::Stale => {
            log_annotation_cache_decision(
                "stale",
                annotation_cache_mismatch_for_diagnostics(cache_root, expected_manifest, None),
            );
        }
        crate::annotation::cache::AnnotationLayerCacheLoad::Invalid(detail) => {
            log_annotation_cache_decision(
                "invalid",
                annotation_cache_mismatch_for_diagnostics(
                    cache_root,
                    expected_manifest,
                    Some(detail.as_str()),
                ),
            );
        }
    }
}

fn log_annotation_cache_envelope_decision(
    cache_root: &Path,
    expected_manifest: &AnnotationManifest,
    load: &crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad,
) {
    match load {
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Ready(_) => {}
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Missing => {
            log_annotation_cache_decision("missing", None);
        }
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Stale => {
            log_annotation_cache_decision(
                "stale",
                annotation_cache_mismatch_for_diagnostics(cache_root, expected_manifest, None),
            );
        }
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Invalid(detail) => {
            log_annotation_cache_decision(
                "invalid",
                annotation_cache_mismatch_for_diagnostics(
                    cache_root,
                    expected_manifest,
                    Some(detail.as_str()),
                ),
            );
        }
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn timing_percent(duration: Duration, total: Duration) -> f64 {
    if total.is_zero() {
        0.0
    } else {
        duration.as_secs_f64() / total.as_secs_f64() * 100.0
    }
}

fn log_annotation_timing_bucket(name: &str, duration: Duration, total: Duration) {
    eprintln!(
        "annotation_timing bucket=\"{}\" wall_ms={:.2} pct_total={:.2}",
        name,
        duration_ms(duration),
        timing_percent(duration, total)
    );
}

#[derive(Clone, Copy)]
struct AnnotationBuildTimingLogContext {
    documents: usize,
    tokens: u64,
    reused_existing: bool,
    cache_written: bool,
    active_surface_reused: bool,
    internal_surface_rebuild_ran: bool,
}

fn log_annotation_build_timings(
    timing: &AnnotationBuildTimingCounters,
    total: Duration,
    context: AnnotationBuildTimingLogContext,
) {
    let total_seconds = total.as_secs_f64();
    let docs_per_sec = if total_seconds > 0.0 {
        context.documents as f64 / total_seconds
    } else {
        0.0
    };
    let tokens_per_sec = if total_seconds > 0.0 {
        context.tokens as f64 / total_seconds
    } else {
        0.0
    };
    let cache_status = if context.reused_existing {
        "reused"
    } else {
        "rebuilt"
    };
    let model_load_attempts = timing.model_load_attempts.load(AtomicOrdering::Relaxed);
    let model_load_actual = timing.model_load_actual.load(AtomicOrdering::Relaxed);
    let manager_instances_created = timing
        .manager_instances_created
        .load(AtomicOrdering::Relaxed);
    let annotation_segments_processed = timing
        .annotation_segments_processed
        .load(AtomicOrdering::Relaxed);

    eprintln!(
        "annotation_timing total_wall_ms={:.2} documents={} tokens={} docs_per_sec={:.2} tokens_per_sec={:.2} rayon_workers={} cache={} cache_written={} active_surface_reused={} internal_surface_rebuild_ran={} duplicate_tokenisation_avoided=false readback_verification={} model_load_attempts={} model_load_actual={} manager_instances_created={} annotation_segments_processed={}",
        duration_ms(total),
        context.documents,
        context.tokens,
        docs_per_sec,
        tokens_per_sec,
        rayon::current_num_threads(),
        cache_status,
        context.cache_written,
        context.active_surface_reused,
        context.internal_surface_rebuild_ran,
        timing.readback_verification_ran(),
        model_load_attempts,
        model_load_actual,
        manager_instances_created,
        annotation_segments_processed
    );
    log_annotation_timing_bucket(
        "internal surface rebuild",
        AnnotationBuildTimingCounters::load_duration(&timing.internal_surface_rebuild_ns),
        total,
    );
    log_annotation_timing_bucket(
        "manifest construction / cache-ready check",
        AnnotationBuildTimingCounters::load_duration(&timing.manifest_cache_ready_ns),
        total,
    );
    log_annotation_timing_bucket(
        "annotation input load",
        AnnotationBuildTimingCounters::load_duration(&timing.annotation_input_load_ns),
        total,
    );
    log_annotation_timing_bucket(
        "identity verification",
        AnnotationBuildTimingCounters::load_duration(&timing.identity_verification_ns),
        total,
    );
    log_annotation_timing_bucket(
        "in-loop re-tokenisation",
        AnnotationBuildTimingCounters::load_duration(&timing.in_loop_retokenisation_ns),
        total,
    );
    log_annotation_timing_bucket(
        "model_checksum",
        AnnotationBuildTimingCounters::load_duration(&timing.model_checksum_ns),
        total,
    );
    log_annotation_timing_bucket(
        "model_load",
        AnnotationBuildTimingCounters::load_duration(&timing.model_load_ns),
        total,
    );
    log_annotation_timing_bucket(
        "UDPipe tag call",
        AnnotationBuildTimingCounters::load_duration(&timing.udpipe_tag_call_ns),
        total,
    );
    log_annotation_timing_bucket(
        "alignment",
        AnnotationBuildTimingCounters::load_duration(&timing.alignment_ns),
        total,
    );
    log_annotation_timing_bucket(
        "local per-document counting",
        AnnotationBuildTimingCounters::load_duration(&timing.local_counting_ns),
        total,
    );
    log_annotation_timing_bucket(
        "reduce + global lemma interning",
        AnnotationBuildTimingCounters::load_duration(&timing.reduce_global_interning_ns),
        total,
    );
    log_annotation_timing_bucket(
        "serialise",
        AnnotationBuildTimingCounters::load_duration(&timing.cache_serialise_ns),
        total,
    );
    log_annotation_timing_bucket(
        "write",
        AnnotationBuildTimingCounters::load_duration(&timing.cache_write_ns),
        total,
    );
    log_annotation_timing_bucket(
        "read-back verification",
        AnnotationBuildTimingCounters::load_duration(&timing.cache_readback_verification_ns),
        total,
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationCacheBuildReport {
    pub documents_annotated: usize,
    pub total_annotated_term_count_rows: usize,
    pub languages_used: Vec<String>,
    pub cache_root: String,
    pub reused_existing: bool,
    pub cache_written: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AnnotationCacheBuildError {
    Cancelled,
    MissingCacheRoot,
    SurfaceBuildFailed { error: String },
    UnsupportedLanguages { languages: Vec<String> },
    AnnotationFailed { error: String },
    ManifestError { error: String },
    CacheWriteError { error: String },
}

impl std::fmt::Display for AnnotationCacheBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Annotation cache build cancelled."),
            Self::MissingCacheRoot => formatter.write_str("Missing annotation cache root."),
            Self::SurfaceBuildFailed { error } => {
                write!(formatter, "Surface build failed: {error}")
            }
            Self::UnsupportedLanguages { languages } => {
                write!(
                    formatter,
                    "No UDPipe model is available for language(s): {}",
                    languages.join(", ")
                )
            }
            Self::AnnotationFailed { error } => {
                write!(formatter, "Annotation build failed: {error}")
            }
            Self::ManifestError { error } => write!(formatter, "Manifest error: {error}"),
            Self::CacheWriteError { error } => write!(formatter, "Cache write error: {error}"),
        }
    }
}

impl std::error::Error for AnnotationCacheBuildError {}

pub(super) struct AnnotationDocumentInput {
    pub(super) analysis_document_index: usize,
    pub(super) source_path: String,
    pub(super) relative_path: String,
    pub(super) text: String,
    pub(super) language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalAnnotatedTermCount {
    pub(super) surface_term_id: u32,
    pub(super) lemma: String,
    pub(super) upos: UniversalPos,
    pub(super) count: u64,
}

type LocalAnnotatedTermCountAccumulator = HashMap<(u32, String, UniversalPos), u64>;

struct WorkerAnnotationDocument {
    analysis_document_index: usize,
    source_path: String,
    relative_path: String,
    term_counts: Vec<LocalAnnotatedTermCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AnnotationCacheReuseKey {
    processed_hash_sha256: String,
    effective_language: String,
    model_sha256: String,
    tokenization_identity: String,
    decode_policy_version: u32,
}

struct AnnotationCacheReuseContext {
    tokenization_identity: String,
    decode_policy_version: u32,
    language_by_document_index: HashMap<usize, String>,
    model_sha_by_language: HashMap<String, String>,
}

struct AnnotationCacheSalvagePlan {
    reused_documents: Vec<WorkerAnnotationDocument>,
    miss_indices: Vec<usize>,
}

struct AnnotationCacheSalvageBuildContext<'a> {
    corpus: &'a AnalysisCorpus,
    records: &'a [DocumentRecord],
    options: &'a AnnotationLayerBuildOptions,
    manifest: &'a AnnotationManifest,
    surface_term_lookup: &'a HashMap<String, u32>,
    cancel: &'a AtomicBool,
    parallel_map: bool,
    runtime: AnnotationLayerBuildRuntime<'a>,
}

struct StagedAnnotationDocument<'a> {
    input: &'a AnnotationDocumentInput,
    tokens: Vec<Token<'a>>,
    chunks: Vec<Range<usize>>,
}

#[derive(Clone)]
struct AnnotationChunkJob {
    document_position: usize,
    chunk_index: usize,
    chunk: Range<usize>,
}

struct AnnotatedAnnotationChunk {
    document_position: usize,
    chunk_index: usize,
    chunk: Range<usize>,
    annotations: Vec<AnnotatedToken>,
}

#[derive(Clone, Copy)]
struct AnnotationLayerBuildRuntime<'a> {
    docs_done: &'a std::sync::atomic::AtomicUsize,
    tokens_done: &'a AtomicU64,
    document_token_counts: &'a [u64],
    timing: Option<&'a AnnotationBuildTimingCounters>,
    loading_steps_done: Option<&'a std::sync::atomic::AtomicUsize>,
    annotating_started: Option<&'a AtomicBool>,
}

impl AnnotationLayerBuildRuntime<'_> {
    fn token_count_for_document(&self, analysis_document_index: usize) -> u64 {
        self.document_token_counts
            .get(analysis_document_index)
            .copied()
            .unwrap_or(0)
    }

    fn credit_document_tokens(&self, analysis_document_index: usize) {
        self.credit_tokens(self.token_count_for_document(analysis_document_index));
    }

    fn credit_tokens(&self, count: u64) {
        if count > 0 {
            self.tokens_done.fetch_add(count, AtomicOrdering::Relaxed);
        }
    }

    fn record_loading_step(&self) {
        if let Some(loading_steps_done) = self.loading_steps_done {
            loading_steps_done.fetch_add(1, AtomicOrdering::Relaxed);
        }
    }

    fn mark_annotating_started(&self) {
        if let Some(annotating_started) = self.annotating_started {
            annotating_started.store(true, AtomicOrdering::Relaxed);
        }
    }
}

fn ensure_annotation_not_cancelled(cancel: &AtomicBool) -> Result<(), AnnotationLayerBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        Err(AnnotationLayerBuildError::Cancelled)
    } else {
        Ok(())
    }
}

fn record_annotation_loading_step(loading_steps_done: Option<&std::sync::atomic::AtomicUsize>) {
    if let Some(loading_steps_done) = loading_steps_done {
        loading_steps_done.fetch_add(1, AtomicOrdering::Relaxed);
    }
}

fn annotation_document_token_counts(corpus: &AnalysisCorpus) -> Vec<u64> {
    corpus
        .documents
        .iter()
        .map(|document| document.summary.token_count)
        .collect()
}

#[derive(Clone, Copy)]
struct AnnotationModelBuildContext<'a> {
    languages_used: &'a BTreeSet<String>,
    model_dir: &'a Path,
}

enum AnnotationSurfaceCorpusInput<'a> {
    Borrowed(&'a AnalysisCorpus),
    Owned(Box<AnalysisCorpus>),
}

impl AnnotationSurfaceCorpusInput<'_> {
    fn as_ref(&self) -> &AnalysisCorpus {
        match self {
            Self::Borrowed(corpus) => corpus,
            Self::Owned(corpus) => corpus,
        }
    }
}

/// Cache state used to gate annotation-only frequency views.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnnotationCacheState {
    Ready,
    Missing,
    Stale,
    Invalid,
}

/// Readiness detail derived from the existing annotation-cache loader.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnnotationCacheAvailability {
    pub state: AnnotationCacheState,
    pub detail: Option<String>,
}

pub fn build_annotation_layer<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    load_text: TextLoader,
    docs_done: &std::sync::atomic::AtomicUsize,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    build_annotation_layer_with_mapping(
        corpus, records, options, cancel, load_text, true, docs_done,
    )
}

pub(super) fn validate_reusable_surface_corpus(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    analysis_options: &AnalysisBuildOptions,
    language_overrides: &HashMap<usize, String>,
) -> Result<(), String> {
    if corpus.overview.build_signature != analysis_options.build_signature {
        return Err("build signature differs".to_string());
    }
    if corpus.overview.processing_config_signature != analysis_options.processing_config_signature {
        return Err("processing config signature differs".to_string());
    }
    if corpus.tokenization != analysis_options.tokenization {
        return Err("tokenisation config differs".to_string());
    }
    if corpus.overview.source_count != records.len() {
        return Err(format!(
            "source count differs: corpus has {}, request has {}",
            corpus.overview.source_count,
            records.len()
        ));
    }
    if corpus.overview.document_count != corpus.documents.len() {
        return Err("overview document count does not match corpus documents".to_string());
    }

    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records.sort_by(|left, right| compare_document_records(left.1, right.1));
    let mut record_iter = ordered_records.into_iter().peekable();

    for (analysis_document_index, document) in corpus.documents.iter().enumerate() {
        if document.summary.id != analysis_document_index {
            return Err(format!(
                "document id {} does not match analysis index {analysis_document_index}",
                document.summary.id
            ));
        }
        if document.summary.token_count == 0 {
            return Err(format!(
                "document {analysis_document_index} has uncertain zero token count"
            ));
        }
        if document.processed_hash_sha256.len() != 64
            || !document
                .processed_hash_sha256
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(format!(
                "document {analysis_document_index} has missing or invalid processed hash"
            ));
        }

        let mut matched_record = None;
        for (original_record_index, record) in record_iter.by_ref() {
            if record.source_path.to_string_lossy() == document.summary.source_path
                && record.relative_path.to_string_lossy() == document.summary.relative_path
            {
                matched_record = Some((original_record_index, record));
                break;
            }
        }
        let Some((original_record_index, _record)) = matched_record else {
            return Err(format!(
                "document {analysis_document_index} path is not present in requested records"
            ));
        };

        match language_overrides.get(&original_record_index) {
            Some(expected_language) => {
                if document.summary.language.as_deref() != Some(expected_language.as_str())
                    || !document.summary.language_is_overridden
                {
                    return Err(format!(
                        "document {analysis_document_index} language override differs"
                    ));
                }
            }
            None => {
                if document.summary.language.is_none() {
                    return Err(format!(
                        "document {analysis_document_index} has missing language identity"
                    ));
                }
                if document.summary.language_is_overridden {
                    return Err(format!(
                        "document {analysis_document_index} carries an unexpected language override"
                    ));
                }
            }
        }
    }

    Ok(())
}

pub fn build_and_write_annotation_cache<TextLoader, Progress>(
    records: &[DocumentRecord],
    analysis_options: AnalysisBuildOptions,
    annotation_options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    load_text: TextLoader,
    report_progress: Progress,
) -> Result<AnnotationCacheBuildReport, AnnotationCacheBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String> + Send,
    Progress: FnMut(AnnotationCacheBuildProgress),
{
    build_and_write_annotation_cache_with_reusable_surface_corpus(
        records,
        analysis_options,
        annotation_options,
        cancel,
        load_text,
        report_progress,
        None,
    )
}

pub fn build_and_write_annotation_cache_with_reusable_surface_corpus<TextLoader, Progress>(
    records: &[DocumentRecord],
    mut analysis_options: AnalysisBuildOptions,
    annotation_options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    mut report_progress: Progress,
    reusable_surface_corpus: Option<&AnalysisCorpus>,
) -> Result<AnnotationCacheBuildReport, AnnotationCacheBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String> + Send,
    Progress: FnMut(AnnotationCacheBuildProgress),
{
    let total_started = Instant::now();
    let timing = annotation_timing_enabled().then(AnnotationBuildTimingCounters::new);
    let timing_ref = timing.as_ref();

    let cache_root = analysis_options
        .annotation_cache_root
        .take()
        .ok_or(AnnotationCacheBuildError::MissingCacheRoot)?;
    analysis_options.force_compute_hashes = true;

    let cache_root_str = cache_root.to_string_lossy().into_owned();

    let surface_files_total = records.len();
    let reusable_surface_corpus = reusable_surface_corpus.and_then(|candidate| {
        match validate_reusable_surface_corpus(
            candidate,
            records,
            &analysis_options,
            &annotation_options.language_overrides,
        ) {
            Ok(()) => Some(candidate),
            Err(reason) => {
                if timing_ref.is_some() {
                    eprintln!(
                        "annotation_timing active_surface_reuse=rejected reason=\"{reason}\""
                    );
                }
                None
            }
        }
    });

    let surface_corpus = if let Some(corpus) = reusable_surface_corpus {
        AnnotationSurfaceCorpusInput::Borrowed(corpus)
    } else {
        report_progress(annotation_build_progress(
            0,
            surface_files_total,
            AnnotationCacheBuildPhase::BuildingSurface {
                phase: "Building surface analysis".to_string(),
            },
        ));

        let surface_started = Instant::now();
        let corpus_result = build_analysis_corpus(
            records,
            analysis_options,
            cancel,
            &mut load_text,
            |progress| {
                report_progress(annotation_build_progress(
                    progress.files_done,
                    progress.files_total,
                    AnnotationCacheBuildPhase::BuildingSurface {
                        phase: format!("{:?}", progress.current_phase),
                    },
                ));
            },
        );
        if let Some(timing) = timing_ref {
            timing.record_internal_surface_rebuild(surface_started.elapsed());
        }
        match corpus_result {
            Ok(corpus) => AnnotationSurfaceCorpusInput::Owned(Box::new(corpus)),
            Err(error) => {
                let cancelled = matches!(&error, AnalysisBuildError::Cancelled);
                report_progress(annotation_build_progress(
                    0,
                    surface_files_total,
                    if cancelled {
                        AnnotationCacheBuildPhase::Cancelled
                    } else {
                        AnnotationCacheBuildPhase::Failed
                    },
                ));
                return Err(if cancelled {
                    AnnotationCacheBuildError::Cancelled
                } else {
                    AnnotationCacheBuildError::SurfaceBuildFailed {
                        error: error.to_string(),
                    }
                });
            }
        }
    };
    let active_surface_reused = matches!(surface_corpus, AnnotationSurfaceCorpusInput::Borrowed(_));
    let internal_surface_rebuild_ran = !active_surface_reused;
    let corpus = surface_corpus.as_ref();

    if active_surface_reused && let Some(timing) = timing_ref {
        timing.record_internal_surface_rebuild(Duration::ZERO);
    }

    let annotation_files_total = corpus.documents.len();
    let annotation_token_counts = annotation_document_token_counts(corpus);
    let annotation_tokens_total = annotation_token_counts.iter().sum::<u64>();
    let tokens_done = AtomicU64::new(0);

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        report_progress(annotation_build_progress_with_tokens(
            0,
            annotation_files_total,
            0,
            annotation_tokens_total,
            AnnotationCacheBuildPhase::Cancelled,
        ));
        return Err(AnnotationCacheBuildError::Cancelled);
    }

    let manifest_cache_started = Instant::now();
    let manifest = match build_expected_manifest(
        corpus,
        records,
        annotation_options.udpipe_model_dir.as_deref(),
        &annotation_options.language_overrides,
    ) {
        Ok(manifest) => manifest,
        Err(error) => {
            if let Some(timing) = timing_ref {
                timing.record_manifest_cache_ready(manifest_cache_started.elapsed());
            }
            report_progress(annotation_build_progress_with_tokens(
                0,
                annotation_files_total,
                0,
                annotation_tokens_total,
                AnnotationCacheBuildPhase::Failed,
            ));
            return Err(AnnotationCacheBuildError::ManifestError {
                error: format!("{:?}", error),
            });
        }
    };

    let cache_load = crate::annotation::cache::load_annotation_layer_cache(&cache_root, &manifest);
    log_annotation_cache_load_decision(&cache_root, &manifest, &cache_load);
    if let Some(timing) = timing_ref {
        timing.record_manifest_cache_ready(manifest_cache_started.elapsed());
    }
    if let crate::annotation::cache::AnnotationLayerCacheLoad::Ready(layer) = cache_load {
        let documents_annotated = layer.documents.len();
        let total_annotated_term_count_rows = layer
            .documents
            .iter()
            .map(|doc| doc.term_counts.len())
            .sum();

        report_progress(annotation_build_progress_with_tokens(
            documents_annotated,
            annotation_files_total,
            annotation_tokens_total,
            annotation_tokens_total,
            AnnotationCacheBuildPhase::Completed,
        ));

        if let Some(timing) = timing_ref {
            log_annotation_build_timings(
                timing,
                total_started.elapsed(),
                AnnotationBuildTimingLogContext {
                    documents: documents_annotated,
                    tokens: corpus.overview().total_tokens,
                    reused_existing: true,
                    cache_written: false,
                    active_surface_reused,
                    internal_surface_rebuild_ran,
                },
            );
        }

        return Ok(AnnotationCacheBuildReport {
            documents_annotated,
            total_annotated_term_count_rows,
            languages_used: layer.languages_used,
            cache_root: cache_root_str,
            reused_existing: true,
            cache_written: false,
        });
    }

    let envelope_load = crate::annotation::cache::read_annotation_layer_cache_envelope(&cache_root);
    log_annotation_cache_envelope_decision(&cache_root, &manifest, &envelope_load);
    let salvage_plan = match envelope_load {
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Ready(envelope) => {
            let surface_term_lookup = match build_surface_term_lookup(corpus) {
                Ok(lookup) => lookup,
                Err(error) => {
                    report_progress(annotation_build_progress_with_tokens(
                        0,
                        annotation_files_total,
                        0,
                        annotation_tokens_total,
                        AnnotationCacheBuildPhase::Failed,
                    ));
                    return Err(AnnotationCacheBuildError::AnnotationFailed {
                        error: error.to_string(),
                    });
                }
            };
            match plan_annotation_cache_salvage(
                corpus,
                &manifest,
                &envelope,
                &surface_term_lookup,
                cancel,
            ) {
                Ok(plan) => Some((plan, surface_term_lookup)),
                Err(error) => {
                    let cancelled = matches!(&error, AnnotationLayerBuildError::Cancelled);
                    report_progress(annotation_build_progress_with_tokens(
                        0,
                        annotation_files_total,
                        0,
                        annotation_tokens_total,
                        if cancelled {
                            AnnotationCacheBuildPhase::Cancelled
                        } else {
                            AnnotationCacheBuildPhase::Failed
                        },
                    ));
                    return Err(match error {
                        AnnotationLayerBuildError::Cancelled => {
                            AnnotationCacheBuildError::Cancelled
                        }
                        AnnotationLayerBuildError::UnsupportedLanguages { languages } => {
                            AnnotationCacheBuildError::UnsupportedLanguages { languages }
                        }
                        other => AnnotationCacheBuildError::AnnotationFailed {
                            error: other.to_string(),
                        },
                    });
                }
            }
        }
        crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Stale
        | crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Missing
        | crate::annotation::cache::AnnotationLayerCacheEnvelopeLoad::Invalid(_) => None,
    };

    report_progress(annotation_build_progress_with_tokens(
        0,
        annotation_files_total,
        0,
        annotation_tokens_total,
        AnnotationCacheBuildPhase::LoadingModel,
    ));

    let docs_done = std::sync::atomic::AtomicUsize::new(0);
    let loading_steps_done = std::sync::atomic::AtomicUsize::new(0);
    let annotating_started = AtomicBool::new(false);
    if let Some((plan, _)) = &salvage_plan {
        docs_done.store(
            plan.reused_documents.len(),
            std::sync::atomic::Ordering::Relaxed,
        );
        let reused_tokens = plan
            .reused_documents
            .iter()
            .map(|document| {
                annotation_token_counts
                    .get(document.analysis_document_index)
                    .copied()
                    .unwrap_or(0)
            })
            .sum::<u64>();
        tokens_done.store(reused_tokens, AtomicOrdering::Relaxed);
        if !plan.reused_documents.is_empty() {
            annotating_started.store(true, AtomicOrdering::Relaxed);
        }
    }
    report_progress(annotation_build_progress_with_tokens(
        docs_done
            .load(std::sync::atomic::Ordering::Relaxed)
            .min(annotation_files_total),
        annotation_files_total,
        tokens_done
            .load(AtomicOrdering::Relaxed)
            .min(annotation_tokens_total),
        annotation_tokens_total,
        if annotating_started.load(AtomicOrdering::Relaxed) {
            AnnotationCacheBuildPhase::Annotating
        } else {
            AnnotationCacheBuildPhase::LoadingModel
        },
    ));

    let layer = if let Some((salvage_plan, surface_term_lookup)) = salvage_plan {
        std::thread::scope(|s| {
            let handle = s.spawn(|| {
                build_annotation_layer_from_salvage_plan_timed(
                    AnnotationCacheSalvageBuildContext {
                        corpus,
                        records,
                        options: &annotation_options,
                        manifest: &manifest,
                        surface_term_lookup: &surface_term_lookup,
                        cancel,
                        parallel_map: true,
                        runtime: AnnotationLayerBuildRuntime {
                            docs_done: &docs_done,
                            tokens_done: &tokens_done,
                            document_token_counts: &annotation_token_counts,
                            timing: timing_ref,
                            loading_steps_done: Some(&loading_steps_done),
                            annotating_started: Some(&annotating_started),
                        },
                    },
                    salvage_plan,
                    &mut load_text,
                )
            });

            while !handle.is_finished() {
                let done = docs_done.load(std::sync::atomic::Ordering::Relaxed);
                let current_phase = if annotating_started.load(AtomicOrdering::Relaxed) {
                    AnnotationCacheBuildPhase::Annotating
                } else {
                    AnnotationCacheBuildPhase::LoadingModel
                };
                report_progress(annotation_build_progress_with_tokens(
                    done.min(annotation_files_total),
                    annotation_files_total,
                    tokens_done
                        .load(AtomicOrdering::Relaxed)
                        .min(annotation_tokens_total),
                    annotation_tokens_total,
                    current_phase,
                ));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            handle.join().unwrap()
        })
    } else {
        std::thread::scope(|s| {
            let handle = s.spawn(|| {
                build_annotation_layer_with_mapping_timed(
                    corpus,
                    records,
                    annotation_options,
                    cancel,
                    &mut load_text,
                    true,
                    AnnotationLayerBuildRuntime {
                        docs_done: &docs_done,
                        tokens_done: &tokens_done,
                        document_token_counts: &annotation_token_counts,
                        timing: timing_ref,
                        loading_steps_done: Some(&loading_steps_done),
                        annotating_started: Some(&annotating_started),
                    },
                )
            });

            while !handle.is_finished() {
                let done = docs_done.load(std::sync::atomic::Ordering::Relaxed);
                let current_phase = if annotating_started.load(AtomicOrdering::Relaxed) {
                    AnnotationCacheBuildPhase::Annotating
                } else {
                    AnnotationCacheBuildPhase::LoadingModel
                };
                report_progress(annotation_build_progress_with_tokens(
                    done.min(annotation_files_total),
                    annotation_files_total,
                    tokens_done
                        .load(AtomicOrdering::Relaxed)
                        .min(annotation_tokens_total),
                    annotation_tokens_total,
                    current_phase,
                ));
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            handle.join().unwrap()
        })
    };

    let layer = match layer {
        Ok(layer) => layer,
        Err(error) => {
            let cancelled = matches!(&error, AnnotationLayerBuildError::Cancelled);
            report_progress(annotation_build_progress_with_tokens(
                docs_done
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .min(annotation_files_total),
                annotation_files_total,
                tokens_done
                    .load(AtomicOrdering::Relaxed)
                    .min(annotation_tokens_total),
                annotation_tokens_total,
                if cancelled {
                    AnnotationCacheBuildPhase::Cancelled
                } else {
                    AnnotationCacheBuildPhase::Failed
                },
            ));
            return Err(match error {
                AnnotationLayerBuildError::Cancelled => AnnotationCacheBuildError::Cancelled,
                AnnotationLayerBuildError::UnsupportedLanguages { languages } => {
                    AnnotationCacheBuildError::UnsupportedLanguages { languages }
                }
                other => AnnotationCacheBuildError::AnnotationFailed {
                    error: other.to_string(),
                },
            });
        }
    };

    tokens_done.store(annotation_tokens_total, AtomicOrdering::Relaxed);

    report_progress(annotation_build_progress_with_tokens(
        annotation_files_total,
        annotation_files_total,
        annotation_tokens_total,
        annotation_tokens_total,
        AnnotationCacheBuildPhase::Annotating,
    ));

    report_progress(annotation_build_progress_with_tokens(
        annotation_files_total,
        annotation_files_total,
        annotation_tokens_total,
        annotation_tokens_total,
        AnnotationCacheBuildPhase::WritingCache,
    ));

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        report_progress(annotation_build_progress_with_tokens(
            annotation_files_total,
            annotation_files_total,
            annotation_tokens_total,
            annotation_tokens_total,
            AnnotationCacheBuildPhase::Cancelled,
        ));
        return Err(AnnotationCacheBuildError::Cancelled);
    }

    let surface_terms = corpus
        .surface_term_tokens()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let write_result = std::thread::scope(|s| {
        let handle = s.spawn(|| {
            crate::annotation::cache::write_annotation_layer_cache_with_timing(
                &cache_root,
                &manifest,
                &surface_terms,
                &layer,
                timing_ref,
            )
        });

        while !handle.is_finished() {
            report_progress(annotation_build_progress_with_tokens(
                annotation_files_total,
                annotation_files_total,
                annotation_tokens_total,
                annotation_tokens_total,
                AnnotationCacheBuildPhase::WritingCache,
            ));
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        handle.join().unwrap()
    });
    if let Err(error) = write_result {
        report_progress(annotation_build_progress_with_tokens(
            annotation_files_total,
            annotation_files_total,
            annotation_tokens_total,
            annotation_tokens_total,
            AnnotationCacheBuildPhase::Failed,
        ));
        return Err(AnnotationCacheBuildError::CacheWriteError {
            error: format!("{:?}", error),
        });
    }

    let documents_annotated = layer.documents.len();
    let total_annotated_term_count_rows = layer
        .documents
        .iter()
        .map(|doc| doc.term_counts.len())
        .sum();

    report_progress(annotation_build_progress_with_tokens(
        documents_annotated,
        annotation_files_total,
        annotation_tokens_total,
        annotation_tokens_total,
        AnnotationCacheBuildPhase::Completed,
    ));

    if let Some(timing) = timing_ref {
        log_annotation_build_timings(
            timing,
            total_started.elapsed(),
            AnnotationBuildTimingLogContext {
                documents: documents_annotated,
                tokens: corpus.overview().total_tokens,
                reused_existing: false,
                cache_written: true,
                active_surface_reused,
                internal_surface_rebuild_ran,
            },
        );
    }

    Ok(AnnotationCacheBuildReport {
        documents_annotated,
        total_annotated_term_count_rows,
        languages_used: layer.languages_used,
        cache_root: cache_root_str,
        reused_existing: false,
        cache_written: true,
    })
}

pub(super) fn build_expected_manifest(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    model_dir: Option<&Path>,
    language_overrides: &HashMap<usize, String>,
) -> Result<
    crate::annotation::cache::AnnotationManifest,
    crate::annotation::cache::AnnotationLayerCacheError,
> {
    let tokenization = crate::text::tokenisation::TokenizationManifest {
        id: crate::text::tokenisation::TOKENIZER_ID.to_string(),
        version: crate::text::tokenisation::TOKENIZER_VERSION.to_string(),
        unicode_segmentation_version: crate::text::tokenisation::UNICODE_SEGMENTATION_VERSION
            .to_string(),
        config: corpus.tokenization,
    };

    let document_languages: Vec<String> = corpus
        .documents
        .iter()
        .map(|doc| doc.summary.language.clone().unwrap_or_default())
        .collect();

    let languages_set: BTreeSet<String> = document_languages.iter().cloned().collect();
    let default_dir;
    let resolved_model_dir = match model_dir {
        Some(dir) => dir,
        None => {
            default_dir = default_model_dir();
            &default_dir
        }
    };

    let mut models = Vec::new();
    for lang in &languages_set {
        let (language, model_filename, udpipe_model_name, sha256) =
            crate::annotation::model_identity(lang, resolved_model_dir).map_err(|e| {
                crate::annotation::cache::AnnotationLayerCacheError::ValidationError(e.to_string())
            })?;
        models.push(crate::annotation::cache::AnnotationModelIdentity {
            language,
            model_filename,
            udpipe_model_name,
            sha256,
        });
    }

    let document_hashes: Vec<String> = corpus
        .documents
        .iter()
        .map(|doc| doc.processed_hash_sha256.clone())
        .collect();

    crate::annotation::cache::build_annotation_manifest(
        corpus,
        records,
        tokenization,
        language_overrides,
        models,
        &document_hashes,
        &document_languages,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationAttachError {
    DocumentCountMismatch {
        expected: usize,
        got: usize,
    },
    DocumentIndexMismatch {
        index: usize,
        got: usize,
    },
    DocumentPathMismatch {
        index: usize,
        expected_source: String,
        got_source: String,
        expected_relative: String,
        got_relative: String,
    },
    SurfaceTermIdOutOfRange {
        index: usize,
        term_id: u32,
        limit: usize,
    },
    LemmaIdOutOfRange {
        index: usize,
        lemma_id: u32,
        limit: usize,
    },
    LanguagesUsedNotSortedOrDeduplicated,
}

pub(super) fn attach_annotation_layer(
    corpus: &mut AnalysisCorpus,
    layer: AnnotationLayer,
) -> Result<(), AnnotationAttachError> {
    if layer.documents.len() != corpus.documents.len() {
        return Err(AnnotationAttachError::DocumentCountMismatch {
            expected: corpus.documents.len(),
            got: layer.documents.len(),
        });
    }

    let mut langs = layer.languages_used.clone();
    let original_langs = langs.clone();
    langs.sort_unstable();
    langs.dedup();
    if langs != original_langs {
        return Err(AnnotationAttachError::LanguagesUsedNotSortedOrDeduplicated);
    }

    let max_surface_term_len = corpus.surface_terms.len();
    let max_lemma_len = layer.lemma_table.len();

    for (idx, layer_doc) in layer.documents.iter().enumerate() {
        if layer_doc.analysis_document_index != idx {
            return Err(AnnotationAttachError::DocumentIndexMismatch {
                index: idx,
                got: layer_doc.analysis_document_index,
            });
        }

        let corpus_doc = &corpus.documents[idx];
        if corpus_doc.summary.source_path != layer_doc.source_path
            || corpus_doc.summary.relative_path != layer_doc.relative_path
        {
            return Err(AnnotationAttachError::DocumentPathMismatch {
                index: idx,
                expected_source: corpus_doc.summary.source_path.clone(),
                got_source: layer_doc.source_path.clone(),
                expected_relative: corpus_doc.summary.relative_path.clone(),
                got_relative: layer_doc.relative_path.clone(),
            });
        }

        for count in &layer_doc.term_counts {
            if count.surface_term_id as usize >= max_surface_term_len {
                return Err(AnnotationAttachError::SurfaceTermIdOutOfRange {
                    index: idx,
                    term_id: count.surface_term_id,
                    limit: max_surface_term_len,
                });
            }
            if count.lemma_id as usize >= max_lemma_len {
                return Err(AnnotationAttachError::LemmaIdOutOfRange {
                    index: idx,
                    lemma_id: count.lemma_id,
                    limit: max_lemma_len,
                });
            }
        }
    }

    // Now mutate corpus (all-or-nothing check passed!)
    corpus.lemma_table = layer.lemma_table;
    for (idx, layer_doc) in layer.documents.into_iter().enumerate() {
        let corpus_doc = &mut corpus.documents[idx];
        corpus_doc.annotated_term_counts = layer_doc.term_counts;
        corpus_doc.annotation_available = true;
    }
    corpus.overview.annotation_available = true;

    Ok(())
}

pub(super) fn build_annotation_layer_with_mapping<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    load_text: TextLoader,
    parallel_map: bool,
    docs_done: &std::sync::atomic::AtomicUsize,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let tokens_done = AtomicU64::new(0);
    let document_token_counts = annotation_document_token_counts(corpus);
    build_annotation_layer_with_mapping_timed(
        corpus,
        records,
        options,
        cancel,
        load_text,
        parallel_map,
        AnnotationLayerBuildRuntime {
            docs_done,
            tokens_done: &tokens_done,
            document_token_counts: &document_token_counts,
            timing: None,
            loading_steps_done: None,
            annotating_started: None,
        },
    )
}

fn build_annotation_layer_with_mapping_timed<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    options: AnnotationLayerBuildOptions,
    cancel: &AtomicBool,
    mut load_text: TextLoader,
    parallel_map: bool,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let model_dir = options.udpipe_model_dir.unwrap_or_else(default_model_dir);
    let inputs = load_annotation_document_inputs(
        corpus,
        records,
        &options.language_overrides,
        cancel,
        &mut load_text,
        runtime.timing,
        runtime.loading_steps_done,
    )?;
    let surface_term_lookup = build_surface_term_lookup(corpus)?;
    let languages_used = inputs
        .iter()
        .map(|input| input.language.clone())
        .collect::<BTreeSet<_>>();

    let worker_documents = annotate_annotation_documents(
        &inputs,
        &surface_term_lookup,
        AnnotationModelBuildContext {
            languages_used: &languages_used,
            model_dir: &model_dir,
        },
        cancel,
        parallel_map,
        &corpus.tokenization,
        runtime,
    )?;
    let reduce_started = Instant::now();
    let layer = reduce_annotation_documents(worker_documents, languages_used.into_iter().collect());
    if let Some(timing) = runtime.timing {
        timing.record_reduce_global_interning(reduce_started.elapsed());
    }
    layer
}

fn build_annotation_layer_from_salvage_plan_timed<TextLoader>(
    context: AnnotationCacheSalvageBuildContext<'_>,
    salvage_plan: AnnotationCacheSalvagePlan,
    mut load_text: TextLoader,
) -> Result<AnnotationLayer, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let model_dir = context
        .options
        .udpipe_model_dir
        .clone()
        .unwrap_or_else(default_model_dir);
    let mut worker_documents = salvage_plan.reused_documents;
    let miss_inputs = load_annotation_document_inputs_for_indices(
        context.corpus,
        context.records,
        context.manifest,
        &salvage_plan.miss_indices,
        context.cancel,
        &mut load_text,
        context.runtime,
    )?;
    let miss_languages_used = miss_inputs
        .iter()
        .map(|input| input.language.clone())
        .collect::<BTreeSet<_>>();

    if !miss_inputs.is_empty() {
        let miss_documents = annotate_annotation_documents(
            &miss_inputs,
            context.surface_term_lookup,
            AnnotationModelBuildContext {
                languages_used: &miss_languages_used,
                model_dir: &model_dir,
            },
            context.cancel,
            context.parallel_map,
            &context.corpus.tokenization,
            context.runtime,
        )?;
        worker_documents.extend(miss_documents);
    }

    let languages_used = context
        .manifest
        .languages
        .resolved_documents
        .iter()
        .map(|document| document.effective_language.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let reduce_started = Instant::now();
    let layer = reduce_annotation_documents(worker_documents, languages_used);
    if let Some(timing) = context.runtime.timing {
        timing.record_reduce_global_interning(reduce_started.elapsed());
    }
    layer
}

pub(super) fn load_annotation_document_inputs<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    language_overrides: &HashMap<usize, String>,
    cancel: &AtomicBool,
    load_text: &mut TextLoader,
    timing: Option<&AnnotationBuildTimingCounters>,
    loading_steps_done: Option<&std::sync::atomic::AtomicUsize>,
) -> Result<Vec<AnnotationDocumentInput>, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let mut ordered_records = records.iter().enumerate().collect::<Vec<_>>();
    ordered_records.sort_by(|left, right| compare_document_records(left.1, right.1));

    let mut inputs = Vec::with_capacity(corpus.documents.len());
    for (original_record_index, record) in ordered_records {
        ensure_annotation_not_cancelled(cancel)?;

        let source_path = record.source_path.to_string_lossy().into_owned();
        let relative_path = record.relative_path.to_string_lossy().into_owned();
        let load_started = Instant::now();
        let resolution_result = load_text(record);
        if let Some(timing) = timing {
            timing.record_annotation_input_load(load_started.elapsed());
        }
        let resolution =
            resolution_result.map_err(|error| AnnotationLayerBuildError::TextLoadFailed {
                source_path: source_path.clone(),
                error,
            })?;
        let text = match resolution.provenance {
            AnalysisInputProvenance::Processed | AnalysisInputProvenance::SourceTextFallback => {
                resolution.text
            }
            AnalysisInputProvenance::EmptyOrFailed => None,
        };
        let Some(text) = text.filter(|value| !value.trim().is_empty()) else {
            continue;
        };

        let analysis_document_index = inputs.len();
        let Some(document) = corpus.documents.get(analysis_document_index) else {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "canonical text supplied more usable documents than the analysis corpus; first extra source is {relative_path}"
                ),
            });
        };
        let verify_started = Instant::now();
        verify_annotation_document_identity(
            analysis_document_index,
            document,
            &source_path,
            &relative_path,
            &text,
            &corpus.tokenization,
        )?;
        if let Some(timing) = timing {
            timing.record_identity_verification(verify_started.elapsed());
        }

        ensure_annotation_not_cancelled(cancel)?;
        let language =
            if let Some(override_language) = language_overrides.get(&original_record_index) {
                detect_and_resolve(&text, Some(override_language))
                    .resolution
                    .effective_language
            } else if let Some(language) = document.summary.language.clone() {
                language
            } else {
                detect_and_resolve(&text, None)
                    .resolution
                    .effective_language
            };
        record_annotation_loading_step(loading_steps_done);
        ensure_annotation_not_cancelled(cancel)?;

        inputs.push(AnnotationDocumentInput {
            analysis_document_index,
            source_path,
            relative_path,
            text,
            language,
        });
    }

    if inputs.len() != corpus.documents.len() {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "canonical text supplied {} usable documents, but the analysis corpus contains {} documents",
                inputs.len(),
                corpus.documents.len()
            ),
        });
    }

    Ok(inputs)
}

fn load_annotation_document_inputs_for_indices<TextLoader>(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    manifest: &AnnotationManifest,
    analysis_document_indices: &[usize],
    cancel: &AtomicBool,
    load_text: &mut TextLoader,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<Vec<AnnotationDocumentInput>, AnnotationLayerBuildError>
where
    TextLoader: FnMut(&DocumentRecord) -> Result<AnalysisInputResolution, String>,
{
    let language_by_document_index = manifest
        .languages
        .resolved_documents
        .iter()
        .map(|document| {
            (
                document.analysis_document_index,
                document.effective_language.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let document_identity_by_index = manifest
        .corpus
        .documents
        .iter()
        .map(|document| (document.analysis_document_index, document))
        .collect::<HashMap<_, _>>();

    let mut inputs = Vec::with_capacity(analysis_document_indices.len());
    for &analysis_document_index in analysis_document_indices {
        ensure_annotation_not_cancelled(cancel)?;

        let document_identity = document_identity_by_index.get(&analysis_document_index).ok_or_else(
            || AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "manifest has no document identity for analysis document {analysis_document_index}"
                ),
            },
        )?;
        let record = records
            .get(document_identity.original_record_index)
            .ok_or_else(|| AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "manifest document {analysis_document_index} references missing original record {}",
                    document_identity.original_record_index
                ),
            })?;

        let source_path = record.source_path.to_string_lossy().into_owned();
        let relative_path = record.relative_path.to_string_lossy().into_owned();
        let load_started = Instant::now();
        let resolution_result = load_text(record);
        if let Some(timing) = runtime.timing {
            timing.record_annotation_input_load(load_started.elapsed());
        }
        let resolution =
            resolution_result.map_err(|error| AnnotationLayerBuildError::TextLoadFailed {
                source_path: source_path.clone(),
                error,
            })?;
        let text = match resolution.provenance {
            AnalysisInputProvenance::Processed | AnalysisInputProvenance::SourceTextFallback => {
                resolution.text
            }
            AnalysisInputProvenance::EmptyOrFailed => None,
        };
        let Some(text) = text.filter(|value| !value.trim().is_empty()) else {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!("canonical text omitted miss document {analysis_document_index}"),
            });
        };

        let Some(document) = corpus.documents.get(analysis_document_index) else {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!("analysis corpus has no document {analysis_document_index}"),
            });
        };
        let verify_started = Instant::now();
        verify_annotation_document_identity(
            analysis_document_index,
            document,
            &source_path,
            &relative_path,
            &text,
            &corpus.tokenization,
        )?;
        if let Some(timing) = runtime.timing {
            timing.record_identity_verification(verify_started.elapsed());
        }

        ensure_annotation_not_cancelled(cancel)?;
        let language = language_by_document_index
            .get(&analysis_document_index)
            .ok_or_else(|| AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "manifest has no resolved language for analysis document {analysis_document_index}"
                ),
            })?
            .clone();
        runtime.record_loading_step();

        inputs.push(AnnotationDocumentInput {
            analysis_document_index,
            source_path,
            relative_path,
            text,
            language,
        });
    }

    Ok(inputs)
}

fn verify_annotation_document_identity(
    analysis_document_index: usize,
    document: &AnalysisDocument,
    source_path: &str,
    relative_path: &str,
    text: &str,
    tokenization_config: &TokenizationConfig,
) -> Result<(), AnnotationLayerBuildError> {
    if document.summary.source_path != source_path
        || document.summary.relative_path != relative_path
    {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "document {analysis_document_index} is {}, but canonical text supplied {}",
                document_name(&document.summary),
                if relative_path.is_empty() {
                    source_path
                } else {
                    relative_path
                }
            ),
        });
    }

    let document_id = u32::try_from(analysis_document_index).map_err(|_| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: "analysis corpus contains more than u32::MAX documents".to_string(),
        }
    })?;
    let tokens = analysis_word_tokens(text, document_id, tokenization_config);
    if tokens.len() as u64 != document.summary.token_count {
        return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!(
                "document {analysis_document_index} token count changed from {} to {}",
                document.summary.token_count,
                tokens.len()
            ),
        });
    }

    Ok(())
}

pub(super) fn build_surface_term_lookup(
    corpus: &AnalysisCorpus,
) -> Result<HashMap<String, u32>, AnnotationLayerBuildError> {
    let mut lookup = HashMap::with_capacity(corpus.surface_terms.len());
    for (index, term) in corpus.surface_terms.iter().enumerate() {
        let term_id = u32::try_from(index).map_err(|_| {
            AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: "analysis surface vocabulary contains more than u32::MAX types".to_string(),
            }
        })?;
        if lookup.insert(term.token.clone(), term_id).is_some() {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "analysis surface vocabulary contains duplicate term {:?}",
                    term.token
                ),
            });
        }
    }
    Ok(lookup)
}

fn annotation_cache_reuse_context(
    manifest: &AnnotationManifest,
) -> Result<AnnotationCacheReuseContext, AnnotationLayerBuildError> {
    let tokenization_identity = serde_json::to_string(&manifest.tokenization).map_err(|error| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: format!("failed to serialise tokenisation identity: {error}"),
        }
    })?;
    let language_by_document_index = manifest
        .languages
        .resolved_documents
        .iter()
        .map(|document| {
            (
                document.analysis_document_index,
                document.effective_language.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let model_sha_by_language = manifest
        .models
        .iter()
        .map(|model| (model.language.clone(), model.sha256.clone()))
        .collect::<HashMap<_, _>>();

    Ok(AnnotationCacheReuseContext {
        tokenization_identity,
        decode_policy_version: manifest.corpus.decode_policy_version,
        language_by_document_index,
        model_sha_by_language,
    })
}

impl AnnotationCacheReuseContext {
    fn key_for_document(
        &self,
        document: &crate::annotation::cache::AnnotationDocumentIdentity,
    ) -> Option<AnnotationCacheReuseKey> {
        let effective_language = self
            .language_by_document_index
            .get(&document.analysis_document_index)?;
        let model_sha256 = self.model_sha_by_language.get(effective_language)?;
        Some(AnnotationCacheReuseKey {
            processed_hash_sha256: document.processed_hash_sha256.clone(),
            effective_language: effective_language.clone(),
            model_sha256: model_sha256.clone(),
            tokenization_identity: self.tokenization_identity.clone(),
            decode_policy_version: self.decode_policy_version,
        })
    }
}

fn plan_annotation_cache_salvage(
    corpus: &AnalysisCorpus,
    manifest: &AnnotationManifest,
    old_envelope: &AnnotationLayerCacheEnvelope,
    surface_term_lookup: &HashMap<String, u32>,
    cancel: &AtomicBool,
) -> Result<AnnotationCacheSalvagePlan, AnnotationLayerBuildError> {
    let old_context = annotation_cache_reuse_context(&old_envelope.manifest)?;
    let new_context = annotation_cache_reuse_context(manifest)?;

    let cached_documents_by_index = old_envelope
        .layer
        .documents
        .iter()
        .map(|document| (document.analysis_document_index, document))
        .collect::<HashMap<_, _>>();
    let mut reusable_documents = HashMap::new();
    for document in &old_envelope.manifest.corpus.documents {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnnotationLayerBuildError::Cancelled);
        }
        let Some(key) = old_context.key_for_document(document) else {
            continue;
        };
        let Some(cached_document) =
            cached_documents_by_index.get(&document.analysis_document_index)
        else {
            continue;
        };
        reusable_documents.entry(key).or_insert(*cached_document);
    }

    let mut reused_documents = Vec::new();
    let mut miss_indices = Vec::new();
    for document in &manifest.corpus.documents {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnnotationLayerBuildError::Cancelled);
        }

        let reused_document = new_context
            .key_for_document(document)
            .and_then(|key| reusable_documents.get(&key))
            .and_then(|cached_document| {
                remap_cached_annotation_document(
                    corpus,
                    document.analysis_document_index,
                    cached_document,
                    old_envelope,
                    surface_term_lookup,
                    cancel,
                )
            });

        if let Some(document) = reused_document {
            reused_documents.push(document);
        } else {
            miss_indices.push(document.analysis_document_index);
        }
    }

    Ok(AnnotationCacheSalvagePlan {
        reused_documents,
        miss_indices,
    })
}

fn remap_cached_annotation_document(
    corpus: &AnalysisCorpus,
    new_analysis_document_index: usize,
    cached_document: &AnnotationLayerDocument,
    old_envelope: &AnnotationLayerCacheEnvelope,
    surface_term_lookup: &HashMap<String, u32>,
    cancel: &AtomicBool,
) -> Option<WorkerAnnotationDocument> {
    let summary = corpus.document_summary(new_analysis_document_index)?;
    let mut term_counts = Vec::with_capacity(cached_document.term_counts.len());
    for count in &cached_document.term_counts {
        if cancel.load(AtomicOrdering::Relaxed) {
            return None;
        }
        let surface = old_envelope
            .layer
            .surface_terms
            .get(count.surface_term_id as usize)?;
        let &surface_term_id = surface_term_lookup.get(surface)?;
        let lemma = old_envelope
            .layer
            .lemma_table
            .entries
            .get(count.lemma_id as usize)?
            .clone();
        term_counts.push(LocalAnnotatedTermCount {
            surface_term_id,
            lemma,
            upos: count.upos,
            count: count.count,
        });
    }
    term_counts.sort_by(|left, right| {
        left.surface_term_id
            .cmp(&right.surface_term_id)
            .then_with(|| left.lemma.cmp(&right.lemma))
            .then_with(|| left.upos.as_upos_str().cmp(right.upos.as_upos_str()))
    });

    Some(WorkerAnnotationDocument {
        analysis_document_index: new_analysis_document_index,
        source_path: summary.source_path.clone(),
        relative_path: summary.relative_path.clone(),
        term_counts,
    })
}

fn preflight_annotation_languages(
    languages: &BTreeSet<String>,
    manager: &mut ModelManager,
    cancel: &AtomicBool,
    loading_steps_done: Option<&std::sync::atomic::AtomicUsize>,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<(), AnnotationLayerBuildError> {
    let mut unsupported = Vec::new();
    for language in languages {
        ensure_annotation_not_cancelled(cancel)?;
        let load_result = load_annotation_model(manager, language, timing);
        ensure_annotation_not_cancelled(cancel)?;
        record_annotation_loading_step(loading_steps_done);
        match load_result {
            Ok(()) => {}
            Err(AnnotationError::ModelNotFound { language }) => unsupported.push(language),
            Err(error) => {
                return Err(AnnotationLayerBuildError::ModelPreflightFailed {
                    language: language.clone(),
                    error: error.to_string(),
                });
            }
        }
    }

    if !unsupported.is_empty() {
        return Err(AnnotationLayerBuildError::UnsupportedLanguages {
            languages: unsupported,
        });
    }

    Ok(())
}

fn annotate_annotation_documents(
    inputs: &[AnnotationDocumentInput],
    surface_term_lookup: &HashMap<String, u32>,
    model_context: AnnotationModelBuildContext<'_>,
    cancel: &AtomicBool,
    parallel_map: bool,
    tokenization_config: &TokenizationConfig,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    if parallel_map {
        annotate_annotation_documents_in_staged_waves(
            inputs,
            surface_term_lookup,
            model_context,
            cancel,
            tokenization_config,
            runtime,
        )
    } else {
        let mut manager = new_annotation_model_manager(model_context.model_dir, runtime.timing);
        preflight_annotation_languages(
            model_context.languages_used,
            &mut manager,
            cancel,
            runtime.loading_steps_done,
            runtime.timing,
        )?;
        runtime.record_loading_step();
        runtime.mark_annotating_started();
        inputs
            .iter()
            .map(|input| {
                let result = annotate_annotation_document(
                    input,
                    surface_term_lookup,
                    &mut manager,
                    cancel,
                    tokenization_config,
                    runtime.timing,
                );
                if result.is_ok() {
                    runtime.credit_document_tokens(input.analysis_document_index);
                    runtime
                        .docs_done
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                result
            })
            .collect()
    }
}

fn annotate_annotation_documents_in_staged_waves(
    inputs: &[AnnotationDocumentInput],
    surface_term_lookup: &HashMap<String, u32>,
    model_context: AnnotationModelBuildContext<'_>,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    annotate_annotation_documents_in_staged_waves_with_budget(
        inputs,
        surface_term_lookup,
        model_context,
        cancel,
        tokenization_config,
        runtime,
        annotation_wave_token_budget(),
    )
}

fn annotate_annotation_documents_in_staged_waves_with_budget(
    inputs: &[AnnotationDocumentInput],
    surface_term_lookup: &HashMap<String, u32>,
    model_context: AnnotationModelBuildContext<'_>,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    runtime: AnnotationLayerBuildRuntime<'_>,
    token_budget: usize,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    let manager_count = rayon::current_num_threads().max(1);
    let managers = (0..manager_count)
        .map(|_| {
            Mutex::new(new_annotation_model_manager(
                model_context.model_dir,
                runtime.timing,
            ))
        })
        .collect::<Vec<_>>();
    {
        let mut manager =
            managers[0]
                .lock()
                .map_err(|_| AnnotationLayerBuildError::ModelPreflightFailed {
                    language: "*".to_string(),
                    error: "annotation model manager lock poisoned".to_string(),
                })?;
        preflight_annotation_languages(
            model_context.languages_used,
            &mut manager,
            cancel,
            runtime.loading_steps_done,
            runtime.timing,
        )?;
    }
    runtime.record_loading_step();
    runtime.mark_annotating_started();

    let mut worker_documents = Vec::with_capacity(inputs.len());
    let mut next_input = 0usize;
    let mut pending = None;
    let token_budget = token_budget.max(1);

    let mut current_wave = if next_input < inputs.len() || pending.is_some() {
        stage_next_annotation_wave(
            inputs,
            &mut next_input,
            &mut pending,
            token_budget,
            cancel,
            tokenization_config,
            runtime.timing,
        )?
    } else {
        Vec::new()
    };

    while !current_wave.is_empty() {
        let (wave_documents_result, next_wave_result) = rayon::join(
            || {
                annotate_staged_annotation_wave_documents_with_budget(
                    &current_wave,
                    surface_term_lookup,
                    &managers,
                    cancel,
                    runtime,
                    token_budget,
                )
            },
            || {
                if next_input < inputs.len() || pending.is_some() {
                    stage_next_annotation_wave(
                        inputs,
                        &mut next_input,
                        &mut pending,
                        token_budget,
                        cancel,
                        tokenization_config,
                        runtime.timing,
                    )
                } else {
                    Ok(Vec::new())
                }
            },
        );
        let wave_documents = wave_documents_result?;
        let next_wave = next_wave_result?;
        worker_documents.extend(wave_documents);
        current_wave = next_wave;
    }

    Ok(worker_documents)
}

fn new_annotation_model_manager(
    model_dir: &Path,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> ModelManager {
    if let Some(timing) = timing {
        timing.record_manager_instance_created();
    }
    ModelManager::new(model_dir.to_path_buf())
}

fn load_annotation_model(
    manager: &mut ModelManager,
    language: &str,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<(), AnnotationError> {
    if let Some(timing) = timing {
        manager.load_model_with_timing(
            language,
            || timing.record_model_load_attempt(),
            || timing.record_model_load_actual(),
            |duration| timing.record_model_checksum(duration),
            |duration| timing.record_model_load(duration),
        )
    } else {
        manager.load_model(language)
    }
}

fn annotation_wave_token_budget() -> usize {
    rayon::current_num_threads()
        .max(1)
        .saturating_mul(ANNOTATION_SENTENCE_CHUNK_WORD_CAP)
        .saturating_mul(ANNOTATION_WAVE_CHUNKS_PER_WORKER)
        .max(ANNOTATION_SENTENCE_CHUNK_WORD_CAP)
}

fn stage_next_annotation_wave<'a>(
    inputs: &'a [AnnotationDocumentInput],
    next_input: &mut usize,
    pending: &mut Option<StagedAnnotationDocument<'a>>,
    token_budget: usize,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<Vec<StagedAnnotationDocument<'a>>, AnnotationLayerBuildError> {
    let mut wave = Vec::new();
    let mut staged_tokens = 0usize;

    if let Some(staged) = pending.take() {
        staged_tokens = staged_annotation_document_token_weight(&staged);
        wave.push(staged);
    }

    if staged_tokens >= token_budget {
        return Ok(wave);
    }

    while *next_input < inputs.len() {
        let staged =
            stage_annotation_document(&inputs[*next_input], cancel, tokenization_config, timing)?;
        *next_input += 1;
        let token_weight = staged_annotation_document_token_weight(&staged);
        if !wave.is_empty() && staged_tokens.saturating_add(token_weight) > token_budget {
            *pending = Some(staged);
            break;
        }

        staged_tokens = staged_tokens.saturating_add(token_weight);
        wave.push(staged);
        if staged_tokens >= token_budget {
            break;
        }
    }

    Ok(wave)
}

fn staged_annotation_document_token_weight(document: &StagedAnnotationDocument<'_>) -> usize {
    document.tokens.len().max(1)
}

fn stage_annotation_document<'a>(
    input: &'a AnnotationDocumentInput,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<StagedAnnotationDocument<'a>, AnnotationLayerBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        return Err(AnnotationLayerBuildError::Cancelled);
    }

    let document_id = u32::try_from(input.analysis_document_index).map_err(|_| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: "analysis corpus contains more than u32::MAX documents".to_string(),
        }
    })?;
    let retokenisation_started = Instant::now();
    let mut annotation_tokenization_config = *tokenization_config;
    annotation_tokenization_config.include_punctuation = true;
    let tokens = tokenize(&input.text, document_id, &annotation_tokenization_config);
    if let Some(timing) = timing {
        timing.record_in_loop_retokenisation(retokenisation_started.elapsed());
    }
    let chunks = sentence_chunks(&tokens, ANNOTATION_SENTENCE_CHUNK_WORD_CAP);

    Ok(StagedAnnotationDocument {
        input,
        tokens,
        chunks,
    })
}

fn annotation_chunk_jobs(wave: &[StagedAnnotationDocument<'_>]) -> Vec<AnnotationChunkJob> {
    wave.iter()
        .enumerate()
        .flat_map(|(document_position, document)| {
            document
                .chunks
                .iter()
                .cloned()
                .enumerate()
                .map(move |(chunk_index, chunk)| AnnotationChunkJob {
                    document_position,
                    chunk_index,
                    chunk,
                })
        })
        .collect()
}

// Preserve the original default-budget entry point; the wave loop uses the
// budgeted sibling so tests can force segmentation without changing thread count.
#[allow(dead_code)]
fn annotate_staged_annotation_wave_documents(
    wave: &[StagedAnnotationDocument<'_>],
    surface_term_lookup: &HashMap<String, u32>,
    managers: &[Mutex<ModelManager>],
    cancel: &AtomicBool,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    annotate_staged_annotation_wave_documents_with_budget(
        wave,
        surface_term_lookup,
        managers,
        cancel,
        runtime,
        annotation_wave_token_budget(),
    )
}

fn annotate_staged_annotation_wave_documents_with_budget(
    wave: &[StagedAnnotationDocument<'_>],
    surface_term_lookup: &HashMap<String, u32>,
    managers: &[Mutex<ModelManager>],
    cancel: &AtomicBool,
    runtime: AnnotationLayerBuildRuntime<'_>,
    segment_budget: usize,
) -> Result<Vec<WorkerAnnotationDocument>, AnnotationLayerBuildError> {
    let segment_budget = segment_budget.max(1);
    if wave.len() == 1 && staged_annotation_document_token_weight(&wave[0]) > segment_budget {
        return annotate_segmented_staged_annotation_document(
            &wave[0],
            surface_term_lookup,
            managers,
            cancel,
            runtime,
            segment_budget,
        )
        .map(|document| vec![document]);
    }

    let jobs = annotation_chunk_jobs(wave);
    let annotated_chunks = annotate_annotation_chunk_jobs(wave, &jobs, managers, cancel, runtime)?;

    let mut chunks_by_document = (0..wave.len()).map(|_| Vec::new()).collect::<Vec<_>>();
    for annotated_chunk in annotated_chunks {
        chunks_by_document[annotated_chunk.document_position].push(annotated_chunk);
    }

    let finished_documents = chunks_by_document
        .into_par_iter()
        .enumerate()
        .map(|(document_position, mut document_chunks)| {
            let staged_document = &wave[document_position];
            document_chunks.sort_by_key(|chunk| chunk.chunk_index);
            let annotated_chunks = document_chunks
                .into_iter()
                .map(|chunk| (chunk.chunk, chunk.annotations))
                .collect();
            let document = finish_staged_annotation_document(
                staged_document,
                surface_term_lookup,
                annotated_chunks,
                runtime.timing,
            )?;
            runtime.credit_document_tokens(staged_document.input.analysis_document_index);
            runtime.docs_done.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(document)
        })
        .collect::<Vec<Result<_, _>>>();

    finished_documents.into_iter().collect()
}

fn annotate_annotation_chunk_jobs(
    wave: &[StagedAnnotationDocument<'_>],
    jobs: &[AnnotationChunkJob],
    managers: &[Mutex<ModelManager>],
    cancel: &AtomicBool,
    runtime: AnnotationLayerBuildRuntime<'_>,
) -> Result<Vec<AnnotatedAnnotationChunk>, AnnotationLayerBuildError> {
    let annotated_chunks = jobs
        .par_iter()
        .map(|job| {
            let staged_document = &wave[job.document_position];
            let manager_slot = rayon::current_thread_index().unwrap_or(0) % managers.len();
            let mut manager = managers[manager_slot].lock().map_err(|_| {
                AnnotationLayerBuildError::ModelTaggingFailed {
                    analysis_document_index: staged_document.input.analysis_document_index,
                    language: staged_document.input.language.clone(),
                    error: "annotation model manager lock poisoned".to_string(),
                }
            })?;
            tag_staged_annotation_chunk(staged_document, job, &mut manager, cancel, runtime.timing)
                .map(|annotations| AnnotatedAnnotationChunk {
                    document_position: job.document_position,
                    chunk_index: job.chunk_index,
                    chunk: job.chunk.clone(),
                    annotations,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(annotated_chunks)
}

fn annotate_segmented_staged_annotation_document(
    staged_document: &StagedAnnotationDocument<'_>,
    surface_term_lookup: &HashMap<String, u32>,
    managers: &[Mutex<ModelManager>],
    cancel: &AtomicBool,
    runtime: AnnotationLayerBuildRuntime<'_>,
    segment_budget: usize,
) -> Result<WorkerAnnotationDocument, AnnotationLayerBuildError> {
    let segment_budget = segment_budget.max(1);
    let segments = annotation_chunk_segments(&staged_document.chunks, segment_budget);
    let mut count_accumulator = LocalAnnotatedTermCountAccumulator::new();
    let mut next_word_token_index = 0usize;
    let document_token_count =
        runtime.token_count_for_document(staged_document.input.analysis_document_index);
    let total_segment_tokens = staged_document.tokens.len();
    let mut processed_segment_tokens = 0usize;
    let mut credited_document_tokens = 0u64;

    for segment in segments {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Err(AnnotationLayerBuildError::Cancelled);
        }

        let segment_chunks = &staged_document.chunks[segment.clone()];
        let token_range = annotation_segment_token_range(segment_chunks);
        let segment_token_count = token_range.end.saturating_sub(token_range.start);
        let jobs = annotation_segment_chunk_jobs(segment_chunks, segment.start);
        let mut annotated_chunks = annotate_annotation_chunk_jobs(
            std::slice::from_ref(staged_document),
            &jobs,
            managers,
            cancel,
            runtime,
        )?;
        annotated_chunks.sort_by_key(|chunk| chunk.chunk_index);
        let annotated_chunks = annotated_chunks
            .into_iter()
            .map(|chunk| (chunk.chunk, chunk.annotations))
            .collect();

        let projected_len = {
            let projected = project_chunk_annotations_to_word_tokens_for_token_range(
                staged_document.input,
                &staged_document.tokens,
                token_range,
                annotated_chunks,
                next_word_token_index,
            )?;
            let projected_len = projected.len();
            let counting_started = Instant::now();
            accumulate_annotated_tokens_to_local_counts(
                staged_document.input.analysis_document_index,
                &projected,
                surface_term_lookup,
                &mut count_accumulator,
            )?;
            if let Some(timing) = runtime.timing {
                timing.record_local_counting(counting_started.elapsed());
            }
            projected_len
        };

        next_word_token_index = next_word_token_index
            .checked_add(projected_len)
            .ok_or_else(|| {
                annotation_projection_invariant_error(
                    staged_document.input,
                    next_word_token_index,
                    "annotated word-token index overflowed usize".to_string(),
                )
            })?;
        if let Some(timing) = runtime.timing {
            timing.record_annotation_segment_processed();
        }
        processed_segment_tokens = processed_segment_tokens.saturating_add(segment_token_count);
        let segment_credit = proportional_segment_token_credit(
            document_token_count,
            total_segment_tokens,
            processed_segment_tokens,
            credited_document_tokens,
        );
        runtime.credit_tokens(segment_credit);
        credited_document_tokens = credited_document_tokens.saturating_add(segment_credit);
    }

    let term_counts = finish_accumulated_local_counts(count_accumulator);
    runtime.credit_tokens(document_token_count.saturating_sub(credited_document_tokens));
    runtime.docs_done.fetch_add(1, AtomicOrdering::Relaxed);
    Ok(worker_annotation_document_from_local_counts(
        staged_document.input,
        term_counts,
    ))
}

fn annotation_chunk_segments(chunks: &[Range<usize>], segment_budget: usize) -> Vec<Range<usize>> {
    let segment_budget = segment_budget.max(1);
    let mut segments = Vec::new();
    let mut segment_start = 0usize;
    let mut segment_weight = 0usize;

    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let chunk_weight = annotation_chunk_token_weight(chunk);
        if segment_start < chunk_index
            && segment_weight.saturating_add(chunk_weight) > segment_budget
        {
            segments.push(segment_start..chunk_index);
            segment_start = chunk_index;
            segment_weight = 0;
        }

        segment_weight = segment_weight.saturating_add(chunk_weight);
        if chunk_weight > segment_budget {
            segments.push(segment_start..chunk_index + 1);
            segment_start = chunk_index + 1;
            segment_weight = 0;
        }
    }

    if segment_start < chunks.len() {
        segments.push(segment_start..chunks.len());
    }

    segments
}

fn annotation_chunk_token_weight(chunk: &Range<usize>) -> usize {
    chunk.end.saturating_sub(chunk.start).max(1)
}

fn annotation_segment_chunk_jobs(
    segment_chunks: &[Range<usize>],
    first_chunk_index: usize,
) -> Vec<AnnotationChunkJob> {
    segment_chunks
        .iter()
        .cloned()
        .enumerate()
        .map(|(offset, chunk)| AnnotationChunkJob {
            document_position: 0,
            chunk_index: first_chunk_index + offset,
            chunk,
        })
        .collect()
}

fn annotation_segment_token_range(segment_chunks: &[Range<usize>]) -> Range<usize> {
    let start = segment_chunks.first().map_or(0, |chunk| chunk.start);
    let end = segment_chunks.last().map_or(start, |chunk| chunk.end);
    start..end
}

fn proportional_segment_token_credit(
    document_token_count: u64,
    total_segment_tokens: usize,
    processed_segment_tokens: usize,
    credited_document_tokens: u64,
) -> u64 {
    if document_token_count == 0 || total_segment_tokens == 0 {
        return 0;
    }

    let capped_processed = processed_segment_tokens.min(total_segment_tokens) as u128;
    let target_credit = (u128::from(document_token_count) * capped_processed
        / total_segment_tokens as u128)
        .min(u128::from(document_token_count)) as u64;
    target_credit.saturating_sub(credited_document_tokens)
}

fn tag_staged_annotation_chunk(
    staged_document: &StagedAnnotationDocument<'_>,
    job: &AnnotationChunkJob,
    manager: &mut ModelManager,
    cancel: &AtomicBool,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<Vec<AnnotatedToken>, AnnotationLayerBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        return Err(AnnotationLayerBuildError::Cancelled);
    }

    let input = staged_document.input;
    load_annotation_model(manager, &input.language, timing)
        .map_err(|error| annotation_document_error(input, error))?;

    let token_surfaces = staged_document.tokens[job.chunk.clone()]
        .iter()
        .map(|token| token.surface)
        .collect::<Vec<_>>();
    if let Some(timing) = timing {
        manager.tag_and_align_with_timing(
            &input.language,
            &token_surfaces,
            |duration| timing.record_udpipe_tag_call(duration),
            |duration| timing.record_alignment(duration),
        )
    } else {
        manager.tag_and_align(&input.language, &token_surfaces)
    }
    .map_err(|error| annotation_document_error(input, error))
}

fn finish_staged_annotation_document(
    staged_document: &StagedAnnotationDocument<'_>,
    surface_term_lookup: &HashMap<String, u32>,
    annotated_chunks: Vec<(Range<usize>, Vec<AnnotatedToken>)>,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<WorkerAnnotationDocument, AnnotationLayerBuildError> {
    let input = staged_document.input;
    let annotated_tokens =
        project_chunk_annotations_to_word_tokens(input, &staged_document.tokens, annotated_chunks)?;
    let counting_started = Instant::now();
    let term_counts = resolve_annotated_tokens_to_local_counts(
        input.analysis_document_index,
        &annotated_tokens,
        surface_term_lookup,
    )?;
    if let Some(timing) = timing {
        timing.record_local_counting(counting_started.elapsed());
    }

    Ok(worker_annotation_document_from_local_counts(
        input,
        term_counts,
    ))
}

fn worker_annotation_document_from_local_counts(
    input: &AnnotationDocumentInput,
    term_counts: Vec<LocalAnnotatedTermCount>,
) -> WorkerAnnotationDocument {
    WorkerAnnotationDocument {
        analysis_document_index: input.analysis_document_index,
        source_path: input.source_path.clone(),
        relative_path: input.relative_path.clone(),
        term_counts,
    }
}

fn annotate_annotation_document(
    input: &AnnotationDocumentInput,
    surface_term_lookup: &HashMap<String, u32>,
    manager: &mut ModelManager,
    cancel: &AtomicBool,
    tokenization_config: &TokenizationConfig,
    timing: Option<&AnnotationBuildTimingCounters>,
) -> Result<WorkerAnnotationDocument, AnnotationLayerBuildError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        return Err(AnnotationLayerBuildError::Cancelled);
    }

    load_annotation_model(manager, &input.language, timing)
        .map_err(|error| annotation_document_error(input, error))?;

    let document_id = u32::try_from(input.analysis_document_index).map_err(|_| {
        AnnotationLayerBuildError::CorpusDocumentMismatch {
            reason: "analysis corpus contains more than u32::MAX documents".to_string(),
        }
    })?;
    let retokenisation_started = Instant::now();
    let mut annotation_tokenization_config = *tokenization_config;
    annotation_tokenization_config.include_punctuation = true;
    let tokens = tokenize(&input.text, document_id, &annotation_tokenization_config);
    if let Some(timing) = timing {
        timing.record_in_loop_retokenisation(retokenisation_started.elapsed());
    }

    let mut annotated_chunks = Vec::new();
    for chunk in sentence_chunks(&tokens, ANNOTATION_SENTENCE_CHUNK_WORD_CAP) {
        let token_surfaces = tokens[chunk.clone()]
            .iter()
            .map(|token| token.surface)
            .collect::<Vec<_>>();
        let chunk_annotations = if let Some(timing) = timing {
            manager.tag_and_align_with_timing(
                &input.language,
                &token_surfaces,
                |duration| timing.record_udpipe_tag_call(duration),
                |duration| timing.record_alignment(duration),
            )
        } else {
            manager.tag_and_align(&input.language, &token_surfaces)
        }
        .map_err(|error| annotation_document_error(input, error))?;
        annotated_chunks.push((chunk, chunk_annotations));
    }
    let annotated_tokens =
        project_chunk_annotations_to_word_tokens(input, &tokens, annotated_chunks)?;
    let counting_started = Instant::now();
    let term_counts = resolve_annotated_tokens_to_local_counts(
        input.analysis_document_index,
        &annotated_tokens,
        surface_term_lookup,
    )?;
    if let Some(timing) = timing {
        timing.record_local_counting(counting_started.elapsed());
    }

    Ok(WorkerAnnotationDocument {
        analysis_document_index: input.analysis_document_index,
        source_path: input.source_path.clone(),
        relative_path: input.relative_path.clone(),
        term_counts,
    })
}

fn project_chunk_annotations_to_word_tokens(
    input: &AnnotationDocumentInput,
    tokens: &[Token<'_>],
    annotated_chunks: Vec<(Range<usize>, Vec<AnnotatedToken>)>,
) -> Result<Vec<AnnotatedToken>, AnnotationLayerBuildError> {
    let expected_word_surfaces = tokens
        .iter()
        .filter(|token| token.counts_as_word())
        .map(|token| token.surface)
        .collect::<Vec<_>>();
    let mut projected = Vec::with_capacity(expected_word_surfaces.len());
    project_chunk_annotations_to_word_tokens_into(
        input,
        tokens,
        annotated_chunks,
        0,
        &mut projected,
    )?;

    validate_projected_word_annotations(input, &projected, &expected_word_surfaces, 0)?;
    Ok(projected)
}

fn project_chunk_annotations_to_word_tokens_for_token_range(
    input: &AnnotationDocumentInput,
    tokens: &[Token<'_>],
    token_range: Range<usize>,
    annotated_chunks: Vec<(Range<usize>, Vec<AnnotatedToken>)>,
    base_word_token_index: usize,
) -> Result<Vec<AnnotatedToken>, AnnotationLayerBuildError> {
    let expected_word_surfaces = tokens[token_range]
        .iter()
        .filter(|token| token.counts_as_word())
        .map(|token| token.surface)
        .collect::<Vec<_>>();
    let mut projected = Vec::with_capacity(expected_word_surfaces.len());
    project_chunk_annotations_to_word_tokens_into(
        input,
        tokens,
        annotated_chunks,
        base_word_token_index,
        &mut projected,
    )?;

    validate_projected_word_annotations(
        input,
        &projected,
        &expected_word_surfaces,
        base_word_token_index,
    )?;
    Ok(projected)
}

fn project_chunk_annotations_to_word_tokens_into(
    input: &AnnotationDocumentInput,
    tokens: &[Token<'_>],
    annotated_chunks: Vec<(Range<usize>, Vec<AnnotatedToken>)>,
    base_word_token_index: usize,
    projected: &mut Vec<AnnotatedToken>,
) -> Result<(), AnnotationLayerBuildError> {
    for (chunk, annotations) in annotated_chunks {
        let chunk_tokens = &tokens[chunk.clone()];
        if annotations.len() != chunk_tokens.len() {
            let first_mismatch = chunk.start + annotations.len().min(chunk_tokens.len());
            return Err(annotation_projection_invariant_error(
                input,
                first_mismatch,
                format!(
                    "UDPipe produced {} annotations for {} input tokens in chunk {}..{}",
                    annotations.len(),
                    chunk_tokens.len(),
                    chunk.start,
                    chunk.end
                ),
            ));
        }

        for (offset, (token, annotation)) in chunk_tokens.iter().zip(annotations).enumerate() {
            if annotation.surface != token.surface {
                return Err(annotation_projection_invariant_error(
                    input,
                    chunk.start + offset,
                    format!(
                        "annotated surface {:?} does not match token surface {:?}",
                        annotation.surface, token.surface
                    ),
                ));
            }

            if token.counts_as_word() {
                let mut word_annotation = annotation;
                let word_token_index = base_word_token_index
                    .checked_add(projected.len())
                    .ok_or_else(|| {
                        annotation_projection_invariant_error(
                            input,
                            projected.len(),
                            "annotated word-token index overflowed usize".to_string(),
                        )
                    })?;
                word_annotation.token_index = u32::try_from(word_token_index).map_err(|_| {
                    annotation_projection_invariant_error(
                        input,
                        word_token_index,
                        "annotated word-token index overflowed u32".to_string(),
                    )
                })?;
                projected.push(word_annotation);
            }
        }
    }

    Ok(())
}

fn validate_projected_word_annotations(
    input: &AnnotationDocumentInput,
    projected: &[AnnotatedToken],
    expected_word_surfaces: &[&str],
    base_word_token_index: usize,
) -> Result<(), AnnotationLayerBuildError> {
    if projected.len() != expected_word_surfaces.len() {
        let mismatch_index =
            base_word_token_index.saturating_add(projected.len().min(expected_word_surfaces.len()));
        return Err(annotation_projection_invariant_error(
            input,
            mismatch_index,
            format!(
                "projected {} word annotations but expected {} word tokens",
                projected.len(),
                expected_word_surfaces.len()
            ),
        ));
    }

    for (index, (annotation, expected_surface)) in projected
        .iter()
        .zip(expected_word_surfaces.iter())
        .enumerate()
    {
        if annotation.surface != *expected_surface {
            let mismatch_index = base_word_token_index.saturating_add(index);
            return Err(annotation_projection_invariant_error(
                input,
                mismatch_index,
                format!(
                    "projected surface {:?} but expected word surface {:?}",
                    annotation.surface, expected_surface
                ),
            ));
        }
    }

    Ok(())
}

fn annotation_projection_invariant_error(
    input: &AnnotationDocumentInput,
    first_mismatching_index: usize,
    reason: String,
) -> AnnotationLayerBuildError {
    AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
        analysis_document_index: input.analysis_document_index,
        reason: format!(
            "annotation projection invariant failed for document {:?} at index {}: {}",
            input.relative_path, first_mismatching_index, reason
        ),
    }
}

fn annotation_document_error(
    input: &AnnotationDocumentInput,
    error: AnnotationError,
) -> AnnotationLayerBuildError {
    match error {
        AnnotationError::AlignmentFailed { reason } => {
            AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index: input.analysis_document_index,
                reason,
            }
        }
        AnnotationError::ModelNotFound { language } => {
            AnnotationLayerBuildError::UnsupportedLanguages {
                languages: vec![language],
            }
        }
        error => AnnotationLayerBuildError::ModelTaggingFailed {
            analysis_document_index: input.analysis_document_index,
            language: input.language.clone(),
            error: error.to_string(),
        },
    }
}

pub(super) fn resolve_annotated_tokens_to_local_counts(
    analysis_document_index: usize,
    annotated_tokens: &[AnnotatedToken],
    surface_term_lookup: &HashMap<String, u32>,
) -> Result<Vec<LocalAnnotatedTermCount>, AnnotationLayerBuildError> {
    let mut counts = HashMap::<(u32, &str, UniversalPos), u64>::new();
    for annotated_token in annotated_tokens {
        let Some(&surface_term_id) = surface_term_lookup.get(&annotated_token.surface) else {
            return Err(AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: format!(
                    "annotated surface {:?} at token {} does not exist in the analysis surface vocabulary",
                    annotated_token.surface, annotated_token.token_index
                ),
            });
        };
        let key = (
            surface_term_id,
            annotated_token.lemma.as_str(),
            annotated_token.upos,
        );
        let count = counts.entry(key).or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: "annotated token count overflowed u64".to_string(),
            }
        })?;
    }

    let mut terms = counts
        .into_iter()
        .map(
            |((surface_term_id, lemma, upos), count)| LocalAnnotatedTermCount {
                surface_term_id,
                lemma: lemma.to_string(),
                upos,
                count,
            },
        )
        .collect::<Vec<_>>();
    sort_local_annotated_term_counts(&mut terms);
    Ok(terms)
}

fn accumulate_annotated_tokens_to_local_counts(
    analysis_document_index: usize,
    annotated_tokens: &[AnnotatedToken],
    surface_term_lookup: &HashMap<String, u32>,
    accumulator: &mut LocalAnnotatedTermCountAccumulator,
) -> Result<(), AnnotationLayerBuildError> {
    for annotated_token in annotated_tokens {
        let Some(&surface_term_id) = surface_term_lookup.get(&annotated_token.surface) else {
            return Err(AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: format!(
                    "annotated surface {:?} at token {} does not exist in the analysis surface vocabulary",
                    annotated_token.surface, annotated_token.token_index
                ),
            });
        };
        let key = (
            surface_term_id,
            annotated_token.lemma.clone(),
            annotated_token.upos,
        );
        let count = accumulator.entry(key).or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            AnnotationLayerBuildError::AlignmentOrIdentityMismatch {
                analysis_document_index,
                reason: "annotated token count overflowed u64".to_string(),
            }
        })?;
    }

    Ok(())
}

fn finish_accumulated_local_counts(
    accumulator: LocalAnnotatedTermCountAccumulator,
) -> Vec<LocalAnnotatedTermCount> {
    let mut terms = accumulator
        .into_iter()
        .map(
            |((surface_term_id, lemma, upos), count)| LocalAnnotatedTermCount {
                surface_term_id,
                lemma,
                upos,
                count,
            },
        )
        .collect::<Vec<_>>();
    sort_local_annotated_term_counts(&mut terms);
    terms
}

fn sort_local_annotated_term_counts(terms: &mut [LocalAnnotatedTermCount]) {
    terms.sort_by(|left, right| {
        left.surface_term_id
            .cmp(&right.surface_term_id)
            .then_with(|| left.lemma.cmp(&right.lemma))
            .then_with(|| left.upos.as_upos_str().cmp(right.upos.as_upos_str()))
    });
}

fn reduce_annotation_documents(
    mut worker_documents: Vec<WorkerAnnotationDocument>,
    languages_used: Vec<String>,
) -> Result<AnnotationLayer, AnnotationLayerBuildError> {
    worker_documents.sort_by_key(|document| document.analysis_document_index);
    let mut lemma_table = LemmaTable::new();
    let mut documents = Vec::with_capacity(worker_documents.len());

    for (expected_index, document) in worker_documents.into_iter().enumerate() {
        if document.analysis_document_index != expected_index {
            return Err(AnnotationLayerBuildError::CorpusDocumentMismatch {
                reason: format!(
                    "annotation worker returned document index {}, expected {expected_index}",
                    document.analysis_document_index
                ),
            });
        }
        let term_counts = document
            .term_counts
            .into_iter()
            .map(|local_count| AnnotatedTermCount {
                surface_term_id: local_count.surface_term_id,
                lemma_id: lemma_table.intern(&local_count.lemma),
                upos: local_count.upos,
                count: local_count.count,
            })
            .collect::<Vec<_>>();
        documents.push(AnnotationLayerDocument {
            analysis_document_index: document.analysis_document_index,
            source_path: document.source_path,
            relative_path: document.relative_path,
            term_counts,
        });
    }

    Ok(AnnotationLayer {
        documents,
        lemma_table,
        languages_used,
    })
}

pub fn annotation_cache_availability_for_corpus(
    corpus: &AnalysisCorpus,
    records: &[DocumentRecord],
    cache_root: Option<&Path>,
    model_dir: Option<&Path>,
    language_overrides: &HashMap<usize, String>,
) -> AnnotationCacheAvailability {
    let Some(cache_root) = cache_root else {
        log_annotation_cache_decision("missing", None);
        return AnnotationCacheAvailability {
            state: AnnotationCacheState::Missing,
            detail: None,
        };
    };
    let expected_manifest =
        match build_expected_manifest(corpus, records, model_dir, language_overrides) {
            Ok(manifest) => manifest,
            Err(error) => {
                log_annotation_cache_decision("invalid", Some("manifest_identity"));
                return AnnotationCacheAvailability {
                    state: AnnotationCacheState::Invalid,
                    detail: Some(format!("{error:?}")),
                };
            }
        };

    let load =
        crate::annotation::cache::load_annotation_layer_cache(cache_root, &expected_manifest);
    log_annotation_cache_load_decision(cache_root, &expected_manifest, &load);

    match load {
        crate::annotation::cache::AnnotationLayerCacheLoad::Ready(_) => {
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Ready,
                detail: None,
            }
        }
        crate::annotation::cache::AnnotationLayerCacheLoad::Missing => {
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Missing,
                detail: None,
            }
        }
        crate::annotation::cache::AnnotationLayerCacheLoad::Stale => AnnotationCacheAvailability {
            state: AnnotationCacheState::Stale,
            detail: None,
        },
        crate::annotation::cache::AnnotationLayerCacheLoad::Invalid(detail) => {
            AnnotationCacheAvailability {
                state: AnnotationCacheState::Invalid,
                detail: Some(detail),
            }
        }
    }
}

#[cfg(test)]
#[path = "annotation_build_tests.rs"]
mod tests;
