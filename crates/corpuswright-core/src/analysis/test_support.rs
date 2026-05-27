use super::annotation_build::build_annotation_layer_with_mapping;
use super::build::{AnalysisBuildProduct, build_analysis_corpus_with_mapping};
use super::*;
use crate::analysis::concordance::index::SurfaceIndexCancellationToken;
use crate::annotation::{UniversalPos, default_model_dir};
use crate::corpus::{DocumentRecord, DocumentType};
use crate::preparation::cleaning::{
    CleaningConfig, RemovalMatcher, RemovalRule, RemovalRuleSource, RemovalScope,
};
use crate::text::tokenisation::TokenizationConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
pub(super) fn record(name: &str) -> DocumentRecord {
    DocumentRecord {
        source_path: PathBuf::from(name),
        relative_path: PathBuf::from(name),
        document_type: DocumentType::Text,
        size_bytes: 0,
        extraction_route: None,
        pdf_profile: None,
        ocr_quality: None,
    }
}

pub(super) fn build_options() -> AnalysisBuildOptions {
    let cleaning_config = CleaningConfig::default();
    AnalysisBuildOptions {
        built_at: "2026-01-01T00:00:00Z".to_string(),
        build_signature: "signature".to_string(),
        processing_config_signature: cleaning_config_signature(&cleaning_config),
        overview_context: CorpusOverviewContext::default(),
        udpipe_model_dir: Some(PathBuf::from("__missing_udpipe_models_for_tests__")),
        language_overrides: std::collections::HashMap::new(),
        tokenization: TokenizationConfig::default(),
        annotation_cache_root: None,
        force_compute_hashes: false,
    }
}

pub(super) fn anywhere_literal_rule(text: &str) -> RemovalRule {
    RemovalRule {
        id: "rule-1".to_string(),
        label: format!("Anywhere: {text}"),
        source: RemovalRuleSource::PromotedRepeatedArtifact,
        matcher: RemovalMatcher::Literal {
            text: text.to_string(),
        },
        scope: RemovalScope::Anywhere,
        enabled: true,
    }
}

pub(super) fn processed_resolution(record: &DocumentRecord, text: &str) -> AnalysisInputResolution {
    resolve_analysis_input(
        record,
        &CleaningConfig::default(),
        |_, _| Ok(text.to_string()),
        |_, _| panic!("processed test input must not use fallback"),
    )
}

pub(super) fn read_annotation_cache_envelope(
    cache_root: &std::path::Path,
) -> crate::annotation::cache::AnnotationLayerCacheEnvelope {
    let cache_file = cache_root.join(format!(
        "annotation-layer.v{}.json",
        crate::annotation::cache::ANNOTATION_LAYER_CACHE_FILE_VERSION
    ));
    let bytes = std::fs::read(cache_file).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn surface_term_table(corpus: &AnalysisCorpus) -> Vec<String> {
    corpus.surface_term_tokens().map(str::to_string).collect()
}

pub(super) fn corpus_for(texts: &[(&str, &str)]) -> AnalysisCorpus {
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    build_analysis_corpus(
        &records,
        build_options(),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        |_| {},
    )
    .unwrap()
}

pub(super) fn base_frequency_query() -> FrequencyQuery {
    FrequencyQuery {
        selection: AnalysisDocumentSelection::default(),
        query: String::new(),
        match_mode: FrequencyMatchMode::Contains,
        minimum_frequency: 1,
        token_class: FrequencyTokenClass::All,
        hide_stopwords: false,
        group_by: FrequencyGroupMode::Surface,
        case_fold: false,
        language: None,
        pos_filter: None,
        sort_key: FrequencySortKey::Token,
        sort_direction: SortDirection::Asc,
        offset: 0,
        limit: 100,
    }
}

pub(super) fn frequency_tokens(page: &FrequencyPage) -> Vec<(&str, u64)> {
    page.items
        .iter()
        .map(|item| (item.token.as_str(), item.frequency))
        .collect()
}

pub(super) fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-12,
        "expected {actual} to be close to {expected}"
    );
}

pub(super) fn assert_close_4dp(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("score should be present");
    assert!(
        (actual - expected).abs() < 0.00005,
        "expected {actual:.4} to be close to {expected:.4}"
    );
}

pub(super) fn concordance_document(index: usize, name: &str, text: &str) -> ConcordanceDocument {
    ConcordanceDocument {
        document_index: index,
        source_path: name.to_string(),
        display_name: name.to_string(),
        text: text.to_string(),
    }
}

pub(super) fn concordance_query(query: &str) -> ConcordanceQuery {
    ConcordanceQuery {
        selection: AnalysisDocumentSelection::default(),
        query: query.to_string(),
        context_filter: None,
        context_filter_direction: ConcordanceContextFilterDirection::Both,
        case_sensitive: false,
        sort_keys: vec![ConcordanceSortKey::FilePosition],
        window_tokens: 5,
        offset: 0,
        limit: 100,
        include_rows: true,
        distribution_offset: 0,
        distribution_limit: 10,
        include_distribution: true,
    }
}

pub(super) fn concordance_for(
    documents: &[ConcordanceDocument],
    query: ConcordanceQuery,
) -> Result<ConcordanceResult, ConcordanceError> {
    search_concordance_documents(documents, &query, &TokenizationConfig::default())
}

pub(super) fn routed_concordance_for(
    documents: &[ConcordanceDocument],
    query: &ConcordanceQuery,
    context: &mut ConcordanceSearchContext,
    backend: ConcordanceBackend,
) -> Result<RoutedConcordanceResult, ConcordanceError> {
    search_concordance_documents_routed(
        documents,
        query,
        &TokenizationConfig::default(),
        context,
        backend,
    )
}

pub(super) fn parity_concordance_query(query: &str, case_sensitive: bool) -> ConcordanceQuery {
    ConcordanceQuery {
        case_sensitive,
        limit: 500,
        distribution_limit: 500,
        ..concordance_query(query)
    }
}

pub(super) fn assert_routed_index_parity(
    documents: &[ConcordanceDocument],
    query_text: &str,
    case_sensitive: bool,
) {
    let query = parity_concordance_query(query_text, case_sensitive);
    let mut scan_context = ConcordanceSearchContext::default();
    let scan = routed_concordance_for(
        documents,
        &query,
        &mut scan_context,
        ConcordanceBackend::Scan,
    )
    .unwrap();
    assert_eq!(scan.backend_used, ConcordanceBackendUsed::Scan);

    let mut index_context = ConcordanceSearchContext::default();
    let index = routed_concordance_for(
        documents,
        &query,
        &mut index_context,
        ConcordanceBackend::Index,
    )
    .unwrap();
    assert_eq!(index.backend_used, ConcordanceBackendUsed::Index);
    assert_eq!(
        index.result, scan.result,
        "{query_text} case_sensitive={case_sensitive}"
    );
}

pub(super) fn assert_routed_index_parity_both_modes(
    documents: &[ConcordanceDocument],
    query_text: &str,
) {
    assert_routed_index_parity(documents, query_text, false);
    assert_routed_index_parity(documents, query_text, true);
}

pub(super) fn collocation_query(query: &str) -> ConcordanceCollocationQuery {
    ConcordanceCollocationQuery {
        selection: AnalysisDocumentSelection::default(),
        query: query.to_string(),
        language: None,
        case_sensitive: false,
        span: 2,
        direction: ConcordanceCollocationDirection::Both,
        min_cooccurrence: 1,
        min_collocate_frequency: 1,
        hide_stopwords: false,
        sort_measure: ConcordanceCollocationMeasure::LogDice,
        sort_key: ConcordanceCollocationSortKey::Association,
        sort_direction: SortDirection::Desc,
        limit: 100,
    }
}

pub(super) fn collocations_for(
    documents: &[ConcordanceDocument],
    query: ConcordanceCollocationQuery,
) -> Result<ConcordanceCollocationResult, ConcordanceError> {
    query_concordance_collocations_documents(
        documents,
        &query,
        &TokenizationConfig::default(),
        &SurfaceIndexCancellationToken::default(),
    )
}

pub(super) fn collocation_aggregate_for(
    documents: &[ConcordanceDocument],
    query: &ConcordanceCollocationQuery,
) -> Result<ConcordanceCollocationAggregate, ConcordanceError> {
    aggregate_concordance_collocations_documents(
        documents,
        query,
        &TokenizationConfig::default(),
        SurfaceIndexCancellationToken::default(),
    )
}

pub(super) fn collocate<'a>(
    result: &'a ConcordanceCollocationResult,
    surface: &str,
) -> &'a ConcordanceCollocateRow {
    result
        .rows
        .iter()
        .find(|row| row.surface == surface)
        .unwrap_or_else(|| panic!("missing collocate {surface}"))
}

pub(super) fn concordance_expansion_for(
    document: &ConcordanceDocument,
    query: ConcordanceExpansionQuery,
) -> Result<ConcordanceExpansionResult, ConcordanceError> {
    expand_concordance_context(document, &query, &TokenizationConfig::default())
}

#[derive(Debug, PartialEq)]
pub(super) struct AnalysisCorpusSnapshot {
    pub(super) overview: CorpusOverview,
    pub(super) documents: Vec<AnalysisDocumentSnapshot>,
    pub(super) surface_terms: Vec<(String, Vec<(usize, u64)>)>,
    pub(super) prepared_terms: Vec<AnalysisPreparedTermSnapshot>,
    pub(super) frequency_by_token: FrequencyPage,
    pub(super) frequency_by_frequency: FrequencyPage,
    pub(super) alpha_distribution: TermDistributionPage,
}

#[derive(Debug, PartialEq)]
pub(super) struct AnalysisDocumentSnapshot {
    pub(super) summary: AnalysisDocumentSummary,
    pub(super) surface_term_counts: Vec<(u32, u64)>,
    pub(super) annotated_term_counts: Vec<(u32, u32, UniversalPos, u64)>,
    pub(super) annotation_available: bool,
    pub(super) mattr: Option<f64>,
    pub(super) mtld: Option<f64>,
}

#[derive(Debug, PartialEq)]
pub(super) struct AnalysisPreparedTermSnapshot {
    pub(super) token: String,
    pub(super) frequency: u64,
    pub(super) document_frequency: usize,
    pub(super) document_counts: Vec<(usize, u64)>,
    pub(super) dispersion: f64,
}

pub(super) fn analysis_product_for(
    texts: &[(&str, &str)],
    options: AnalysisBuildOptions,
    parallel_map: bool,
) -> AnalysisBuildProduct {
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    build_analysis_corpus_with_mapping(
        &records,
        options,
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        |_| {},
        parallel_map,
    )
    .unwrap()
}

pub(super) fn annotation_options(
    language_overrides: HashMap<usize, String>,
) -> AnnotationLayerBuildOptions {
    AnnotationLayerBuildOptions {
        udpipe_model_dir: Some(default_model_dir()),
        language_overrides,
    }
}

pub(super) fn annotation_layer_for(
    texts: &[(&str, &str)],
    language_overrides: HashMap<usize, String>,
    parallel_map: bool,
) -> (AnalysisCorpus, AnnotationLayer) {
    let mut analysis_options = build_options();
    analysis_options.language_overrides = language_overrides.clone();
    let corpus = analysis_product_for(texts, analysis_options, true).corpus;
    let records = texts
        .iter()
        .map(|(name, _)| record(name))
        .collect::<Vec<_>>();
    let text_by_name = texts.iter().copied().collect::<HashMap<_, _>>();
    let layer = build_annotation_layer_with_mapping(
        &corpus,
        &records,
        annotation_options(language_overrides),
        &AtomicBool::new(false),
        |record| {
            Ok(processed_resolution(
                record,
                text_by_name
                    .get(record.source_path.to_str().unwrap())
                    .copied()
                    .unwrap_or_default(),
            ))
        },
        parallel_map,
        &std::sync::atomic::AtomicUsize::new(0),
    )
    .unwrap();
    (corpus, layer)
}

pub(super) fn snapshot_analysis_product(product: &AnalysisBuildProduct) -> AnalysisCorpusSnapshot {
    let corpus = &product.corpus;
    let mut frequency_by_frequency_query = base_frequency_query();
    frequency_by_frequency_query.sort_key = FrequencySortKey::Frequency;
    frequency_by_frequency_query.sort_direction = SortDirection::Desc;
    frequency_by_frequency_query.limit = 10;
    AnalysisCorpusSnapshot {
        overview: corpus.overview().clone(),
        documents: corpus
            .documents
            .iter()
            .map(|document| AnalysisDocumentSnapshot {
                summary: document.summary.clone(),
                surface_term_counts: document.surface_term_counts.clone(),
                annotated_term_counts: document
                    .annotated_term_counts
                    .iter()
                    .map(|count| {
                        (
                            count.surface_term_id,
                            count.lemma_id,
                            count.upos,
                            count.count,
                        )
                    })
                    .collect(),
                annotation_available: document.annotation_available,
                mattr: document.mattr,
                mtld: document.mtld,
            })
            .collect(),
        surface_terms: corpus
            .surface_terms
            .iter()
            .map(|term| (term.token.clone(), term.document_counts.clone()))
            .collect(),
        prepared_terms: product
            .terms
            .iter()
            .map(|term| AnalysisPreparedTermSnapshot {
                token: term.token.clone(),
                frequency: term.frequency,
                document_frequency: term.document_frequency,
                document_counts: term.document_counts.clone(),
                dispersion: term.dispersion,
            })
            .collect(),
        frequency_by_token: query_frequency_list(corpus, &base_frequency_query()),
        frequency_by_frequency: query_frequency_list(corpus, &frequency_by_frequency_query),
        alpha_distribution: query_term_distribution(
            corpus,
            "alpha",
            &TermDistributionQuery {
                selection: AnalysisDocumentSelection::default(),
                group_by: FrequencyGroupMode::Surface,
                case_fold: true,
                language: None,
                offset: 0,
                limit: 10,
            },
        ),
    }
}

pub(super) fn add_document_annotations(
    corpus: &mut AnalysisCorpus,
    document_id: usize,
    entries: &[(&str, &str, UniversalPos, u64)],
) {
    let counts = entries
        .iter()
        .map(|(surface, lemma, upos, count)| {
            let surface_term_id = corpus
                .surface_terms
                .iter()
                .position(|term| term.token == *surface)
                .expect("test surface term should exist") as u32;
            let lemma_id = corpus.lemma_table.intern(lemma);
            AnnotatedTermCount {
                surface_term_id,
                lemma_id,
                upos: *upos,
                count: *count,
            }
        })
        .collect::<Vec<_>>();
    corpus.documents[document_id].annotation_available = true;
    corpus.documents[document_id].annotated_term_counts = counts;
}

pub(super) fn ready_annotation_cache() -> AnnotationCacheAvailability {
    AnnotationCacheAvailability {
        state: AnnotationCacheState::Ready,
        detail: None,
    }
}

pub(super) fn annotated_frequency_query(mode: AnnotatedFrequencyMode) -> AnnotatedFrequencyQuery {
    AnnotatedFrequencyQuery {
        selection: AnalysisDocumentSelection::default(),
        mode,
        query: String::new(),
        language: None,
        match_mode: FrequencyMatchMode::Contains,
        minimum_frequency: 1,
        hide_stopwords: false,
        case_fold: false,
        pos_filter: None,
        sort_key: FrequencySortKey::Frequency,
        sort_direction: SortDirection::Desc,
        offset: 0,
        limit: 100,
    }
}
