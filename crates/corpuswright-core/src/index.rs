use crate::decode::DECODE_POLICY_VERSION;
use crate::tokenization::{TokenizationConfig, TokenizationManifest, tokenize};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INDEX_SCHEMA_VERSION: u32 = 1;

const DEFAULT_WINDOW_TOKENS: usize = 5;
const MAX_WINDOW_TOKENS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexDocumentInput<'text> {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub prepared_text: &'text str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexDocument {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexTokenRecord {
    pub document_id: usize,
    pub token_ordinal_in_document: usize,
    pub surface: String,
    pub normalised_surface: String,
    pub prepared_char_start: usize,
    pub prepared_char_end: usize,
    pub counts_as_word: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfacePosting {
    pub document_id: usize,
    pub token_ordinal_in_document: usize,
    pub prepared_char_start: usize,
    pub prepared_char_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceIndexError {
    EmptyQuery,
    DuplicateDocumentId(usize),
    DocumentIdTooLarge(usize),
    InvalidPreparedOffset {
        document_id: usize,
        byte_start: usize,
        byte_end: usize,
    },
    InconsistentStore {
        document_id: usize,
        token_ordinal_in_document: usize,
        node_token_length: usize,
    },
}

impl std::fmt::Display for SurfaceIndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyQuery => formatter.write_str("surface index query contains no word tokens"),
            Self::DuplicateDocumentId(document_id) => {
                write!(
                    formatter,
                    "duplicate surface index document id: {document_id}"
                )
            }
            Self::DocumentIdTooLarge(document_id) => write!(
                formatter,
                "surface index document id {document_id} cannot be passed to the canonical tokenizer"
            ),
            Self::InvalidPreparedOffset {
                document_id,
                byte_start,
                byte_end,
            } => write!(
                formatter,
                "canonical token offsets are not prepared-text character boundaries for document {document_id}: {byte_start}..{byte_end}"
            ),
            Self::InconsistentStore {
                document_id,
                token_ordinal_in_document,
                node_token_length,
            } => write!(
                formatter,
                "surface index store is missing token span document={document_id} ordinal={token_ordinal_in_document} length={node_token_length}"
            ),
        }
    }
}

impl std::error::Error for SurfaceIndexError {}

pub trait IndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]>;
    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]>;
    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument>;
    fn total_indexed_tokens(&self) -> usize;
    fn total_indexed_documents(&self) -> usize;
    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store>;
    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store>;

    fn document_frequency(&self, normalised_surface: &str) -> usize {
        let Some(postings) = self.postings(normalised_surface) else {
            return 0;
        };
        let mut last_document_id = None;
        let mut count = 0usize;
        for posting in postings {
            if last_document_id != Some(posting.document_id) {
                count += 1;
                last_document_id = Some(posting.document_id);
            }
        }
        count
    }
}

/// In-memory storage only. Persistent, mmap-backed, or incremental stores are deferred.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryIndexStore {
    documents: BTreeMap<usize, SurfaceIndexDocument>,
    records_by_document: BTreeMap<usize, Vec<SurfaceIndexTokenRecord>>,
    postings_by_normalised_surface: BTreeMap<String, Vec<SurfacePosting>>,
    total_indexed_tokens: usize,
}

impl InMemoryIndexStore {
    pub fn build(
        documents: &[SurfaceIndexDocumentInput<'_>],
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        let mut store = Self::default();

        for document in documents {
            if store.documents.contains_key(&document.document_id) {
                return Err(SurfaceIndexError::DuplicateDocumentId(document.document_id));
            }

            let tokenizer_document_id = u32::try_from(document.document_id)
                .map_err(|_| SurfaceIndexError::DocumentIdTooLarge(document.document_id))?;
            let mut records = Vec::new();
            for token in tokenize(
                document.prepared_text,
                tokenizer_document_id,
                tokenization_config,
            )
            .into_iter()
            .filter(|token| token.counts_as_word())
            {
                let prepared_char_start = byte_to_char_offset(
                    document.prepared_text,
                    document.document_id,
                    token.byte_start,
                    token.byte_end,
                    token.byte_start,
                )?;
                let prepared_char_end = byte_to_char_offset(
                    document.prepared_text,
                    document.document_id,
                    token.byte_start,
                    token.byte_end,
                    token.byte_end,
                )?;
                records.push(SurfaceIndexTokenRecord {
                    document_id: document.document_id,
                    token_ordinal_in_document: records.len(),
                    surface: token.surface.to_string(),
                    normalised_surface: token.aggregation_key(tokenization_config),
                    prepared_char_start,
                    prepared_char_end,
                    counts_as_word: token.counts_as_word(),
                });
            }

            for record in &records {
                store
                    .postings_by_normalised_surface
                    .entry(record.normalised_surface.clone())
                    .or_default()
                    .push(SurfacePosting {
                        document_id: record.document_id,
                        token_ordinal_in_document: record.token_ordinal_in_document,
                        prepared_char_start: record.prepared_char_start,
                        prepared_char_end: record.prepared_char_end,
                    });
            }

            store.total_indexed_tokens += records.len();
            store.documents.insert(
                document.document_id,
                SurfaceIndexDocument {
                    document_id: document.document_id,
                    source_path: document.source_path.clone(),
                    display_name: document.display_name.clone(),
                    token_count: records.len(),
                },
            );
            store
                .records_by_document
                .insert(document.document_id, records);
        }

        for postings in store.postings_by_normalised_surface.values_mut() {
            postings.sort_by(|left, right| {
                left.document_id.cmp(&right.document_id).then_with(|| {
                    left.token_ordinal_in_document
                        .cmp(&right.token_ordinal_in_document)
                })
            });
        }

        Ok(store)
    }
}

impl IndexStore for InMemoryIndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]> {
        self.postings_by_normalised_surface
            .get(normalised_surface)
            .map(Vec::as_slice)
    }

    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]> {
        self.records_by_document
            .get(&document_id)
            .map(Vec::as_slice)
    }

    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument> {
        self.documents.get(&document_id)
    }

    fn total_indexed_tokens(&self) -> usize {
        self.total_indexed_tokens
    }

    fn total_indexed_documents(&self) -> usize {
        self.documents.len()
    }

    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        Box::new(
            self.postings_by_normalised_surface
                .keys()
                .map(String::as_str),
        )
    }

    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store> {
        Box::new(self.documents.values())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexTerm {
    pub surface: String,
    pub normalised_surface: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceIndexQuery {
    Term(SurfaceIndexTerm),
    Phrase(Vec<SurfaceIndexTerm>),
}

impl SurfaceIndexQuery {
    pub fn from_surface_text(
        query: &str,
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        let terms = query_terms(query, tokenization_config)?;
        if terms.len() == 1 {
            Ok(Self::Term(terms.into_iter().next().expect("term exists")))
        } else {
            Ok(Self::Phrase(terms))
        }
    }

    pub fn terms(&self) -> &[SurfaceIndexTerm] {
        match self {
            Self::Term(term) => std::slice::from_ref(term),
            Self::Phrase(terms) => terms.as_slice(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexHit {
    pub document_id: usize,
    pub token_ordinal_in_document: usize,
    pub node_token_length: usize,
    pub prepared_char_start: usize,
    pub prepared_char_end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceIndexSearchResult {
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub selected_documents_total: usize,
    pub selected_tokens_total: usize,
    pub top_document_share: f64,
    pub document_hit_counts: BTreeMap<usize, u64>,
    pub hits: Vec<SurfaceIndexHit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexKwicRow {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub token_ordinal_in_document: usize,
    pub node_token_length: usize,
    pub prepared_char_start: usize,
    pub prepared_char_end: usize,
    pub left_context: Vec<String>,
    pub node: String,
    pub right_context: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceIndexKwicResult {
    pub total_hits: usize,
    pub documents_with_hits: usize,
    pub selected_documents_total: usize,
    pub selected_tokens_total: usize,
    pub top_document_share: f64,
    pub document_hit_counts: BTreeMap<usize, u64>,
    pub rows: Vec<SurfaceIndexKwicRow>,
}

pub fn search_surface_index(
    store: &impl IndexStore,
    query: &SurfaceIndexQuery,
) -> Result<SurfaceIndexSearchResult, SurfaceIndexError> {
    let terms = query.terms();
    if terms.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }

    let mut hits = if terms.len() == 1 {
        term_hits(store, &terms[0].normalised_surface)
    } else {
        phrase_hits(store, terms)
    };
    hits.sort_by(|left, right| {
        left.document_id.cmp(&right.document_id).then_with(|| {
            left.token_ordinal_in_document
                .cmp(&right.token_ordinal_in_document)
        })
    });

    let mut document_hit_counts = BTreeMap::<usize, u64>::new();
    for hit in &hits {
        *document_hit_counts.entry(hit.document_id).or_default() += 1;
    }
    let top_document_share = if hits.is_empty() {
        0.0
    } else {
        document_hit_counts.values().copied().max().unwrap_or(0) as f64 / hits.len() as f64
    };

    Ok(SurfaceIndexSearchResult {
        total_hits: hits.len(),
        documents_with_hits: document_hit_counts.len(),
        selected_documents_total: store.total_indexed_documents(),
        selected_tokens_total: store.total_indexed_tokens(),
        top_document_share,
        document_hit_counts,
        hits,
    })
}

pub fn search_surface_index_kwic(
    store: &impl IndexStore,
    query: &SurfaceIndexQuery,
    window_tokens: usize,
) -> Result<SurfaceIndexKwicResult, SurfaceIndexError> {
    let window_tokens = surface_index_window_tokens(window_tokens);
    let search = search_surface_index(store, query)?;
    let mut rows = Vec::with_capacity(search.hits.len());
    for hit in &search.hits {
        rows.push(kwic_row(store, hit, window_tokens)?);
    }

    Ok(SurfaceIndexKwicResult {
        total_hits: search.total_hits,
        documents_with_hits: search.documents_with_hits,
        selected_documents_total: search.selected_documents_total,
        selected_tokens_total: search.selected_tokens_total,
        top_document_share: search.top_document_share,
        document_hit_counts: search.document_hit_counts,
        rows,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SurfaceIndexManifest {
    pub schema_version: u32,
    pub processed_texts_hash_sha256: String,
    pub decode_policy_version: u32,
    pub tokenization: TokenizationManifest,
}

impl SurfaceIndexManifest {
    pub fn new(
        processed_texts_hash_sha256: String,
        tokenization: TokenizationManifest,
    ) -> SurfaceIndexManifest {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            processed_texts_hash_sha256,
            decode_policy_version: DECODE_POLICY_VERSION,
            tokenization,
        }
    }

    pub fn is_valid_for(&self, expected: &SurfaceIndexManifest) -> bool {
        self == expected
    }
}

fn byte_to_char_offset(
    text: &str,
    document_id: usize,
    byte_start: usize,
    byte_end: usize,
    byte_offset: usize,
) -> Result<usize, SurfaceIndexError> {
    text.get(..byte_offset)
        .map(str::chars)
        .map(Iterator::count)
        .ok_or(SurfaceIndexError::InvalidPreparedOffset {
            document_id,
            byte_start,
            byte_end,
        })
}

fn query_terms(
    query: &str,
    tokenization_config: &TokenizationConfig,
) -> Result<Vec<SurfaceIndexTerm>, SurfaceIndexError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }
    let terms = tokenize(trimmed, 0, tokenization_config)
        .into_iter()
        .filter(|token| token.counts_as_word())
        .map(|token| SurfaceIndexTerm {
            surface: token.surface.to_string(),
            normalised_surface: token.aggregation_key(tokenization_config),
        })
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }
    Ok(terms)
}

fn term_hits(store: &impl IndexStore, normalised_surface: &str) -> Vec<SurfaceIndexHit> {
    store
        .postings(normalised_surface)
        .unwrap_or_default()
        .iter()
        .map(|posting| SurfaceIndexHit {
            document_id: posting.document_id,
            token_ordinal_in_document: posting.token_ordinal_in_document,
            node_token_length: 1,
            prepared_char_start: posting.prepared_char_start,
            prepared_char_end: posting.prepared_char_end,
        })
        .collect()
}

fn phrase_hits(store: &impl IndexStore, terms: &[SurfaceIndexTerm]) -> Vec<SurfaceIndexHit> {
    let Some(first_postings) = store.postings(&terms[0].normalised_surface) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for posting in first_postings {
        let Some(records) = store.records_for_document(posting.document_id) else {
            continue;
        };
        let Some(last_index) = posting
            .token_ordinal_in_document
            .checked_add(terms.len().saturating_sub(1))
        else {
            continue;
        };
        let Some(last_record) = records.get(last_index) else {
            continue;
        };
        let mut matches = true;
        for (offset, term) in terms.iter().enumerate() {
            let Some(token_ordinal) = posting.token_ordinal_in_document.checked_add(offset) else {
                matches = false;
                break;
            };
            let Some(record) = records.get(token_ordinal) else {
                matches = false;
                break;
            };
            if record.normalised_surface != term.normalised_surface {
                matches = false;
                break;
            }
        }
        if matches {
            hits.push(SurfaceIndexHit {
                document_id: posting.document_id,
                token_ordinal_in_document: posting.token_ordinal_in_document,
                node_token_length: terms.len(),
                prepared_char_start: posting.prepared_char_start,
                prepared_char_end: last_record.prepared_char_end,
            });
        }
    }
    hits
}

fn kwic_row(
    store: &impl IndexStore,
    hit: &SurfaceIndexHit,
    window_tokens: usize,
) -> Result<SurfaceIndexKwicRow, SurfaceIndexError> {
    let records = store.records_for_document(hit.document_id).ok_or(
        SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        },
    )?;
    let document = store
        .document(hit.document_id)
        .ok_or(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        })?;
    let node_end = hit
        .token_ordinal_in_document
        .checked_add(hit.node_token_length)
        .ok_or(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        })?;
    if hit.node_token_length == 0 || node_end > records.len() {
        return Err(SurfaceIndexError::InconsistentStore {
            document_id: hit.document_id,
            token_ordinal_in_document: hit.token_ordinal_in_document,
            node_token_length: hit.node_token_length,
        });
    }

    let left_start = hit.token_ordinal_in_document.saturating_sub(window_tokens);
    let right_end = (node_end + window_tokens).min(records.len());
    let left_context = surfaces(&records[left_start..hit.token_ordinal_in_document]);
    let node_tokens = surfaces(&records[hit.token_ordinal_in_document..node_end]);
    let right_context = surfaces(&records[node_end..right_end]);

    Ok(SurfaceIndexKwicRow {
        document_id: hit.document_id,
        source_path: document.source_path.clone(),
        display_name: document.display_name.clone(),
        token_ordinal_in_document: hit.token_ordinal_in_document,
        node_token_length: hit.node_token_length,
        prepared_char_start: hit.prepared_char_start,
        prepared_char_end: hit.prepared_char_end,
        left_context,
        node: node_tokens.join(" "),
        right_context,
    })
}

fn surfaces(records: &[SurfaceIndexTokenRecord]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.surface.clone())
        .collect()
}

fn surface_index_window_tokens(window_tokens: usize) -> usize {
    if window_tokens == 0 {
        DEFAULT_WINDOW_TOKENS
    } else {
        window_tokens.clamp(1, MAX_WINDOW_TOKENS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{
        AnalysisDocumentSelection, ConcordanceDocument, ConcordanceQuery, ConcordanceSortKey,
        search_concordance_documents,
    };
    use crate::decode::{DecodeStrategy, SourceEncoding, decode_source_bytes};

    fn document_input<'text>(
        document_id: usize,
        name: &str,
        prepared_text: &'text str,
    ) -> SurfaceIndexDocumentInput<'text> {
        SurfaceIndexDocumentInput {
            document_id,
            source_path: name.to_string(),
            display_name: name.to_string(),
            prepared_text,
        }
    }

    fn concordance_document(index: usize, name: &str, text: &str) -> ConcordanceDocument {
        ConcordanceDocument {
            document_index: index,
            source_path: name.to_string(),
            display_name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn build_index<'text>(documents: &[SurfaceIndexDocumentInput<'text>]) -> InMemoryIndexStore {
        InMemoryIndexStore::build(documents, &TokenizationConfig::default()).unwrap()
    }

    fn surface_at_char_offsets(text: &str, start: usize, end: usize) -> String {
        text.chars().skip(start).take(end - start).collect()
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < 0.000_000_1, "{left} != {right}");
    }

    fn scan_query(query: &str) -> ConcordanceQuery {
        ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            case_sensitive: false,
            sort_key: ConcordanceSortKey::FilePosition,
            window_tokens: 5,
            offset: 0,
            limit: 500,
            distribution_offset: 0,
            distribution_limit: 500,
        }
    }

    fn assert_scan_index_parity(documents: &[ConcordanceDocument], query_text: &str) {
        let tokenization_config = TokenizationConfig::default();
        let scan =
            search_concordance_documents(documents, &scan_query(query_text), &tokenization_config)
                .unwrap();
        let inputs = documents
            .iter()
            .map(|document| SurfaceIndexDocumentInput {
                document_id: document.document_index,
                source_path: document.source_path.clone(),
                display_name: document.display_name.clone(),
                prepared_text: document.text.as_str(),
            })
            .collect::<Vec<_>>();
        let store = InMemoryIndexStore::build(&inputs, &tokenization_config).unwrap();
        let query = SurfaceIndexQuery::from_surface_text(query_text, &tokenization_config).unwrap();
        let index = search_surface_index_kwic(&store, &query, scan.window_tokens).unwrap();

        assert_eq!(index.total_hits, scan.total_hits, "{query_text}");
        assert_eq!(
            index.documents_with_hits, scan.documents_with_hits,
            "{query_text}"
        );
        assert_eq!(
            index.selected_documents_total, scan.selected_documents_total,
            "{query_text}"
        );
        assert_eq!(
            index.selected_tokens_total as u64, scan.selected_tokens_total,
            "{query_text}"
        );
        assert_close(index.top_document_share, scan.top_document_share);
        let scan_hit_counts = scan
            .distribution
            .items
            .iter()
            .filter(|item| item.count > 0)
            .map(|item| (item.document_id, item.count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(index.document_hit_counts, scan_hit_counts, "{query_text}");
        assert_eq!(index.rows.len(), scan.rows.len(), "{query_text}");
        for (index_row, scan_row) in index.rows.iter().zip(&scan.rows) {
            assert_eq!(
                index_row.document_id, scan_row.document_index,
                "{query_text}"
            );
            assert_eq!(
                index_row.token_ordinal_in_document, scan_row.token_index,
                "{query_text}"
            );
            assert_eq!(
                index_row.node_token_length, scan_row.node_token_length,
                "{query_text}"
            );
            assert_eq!(
                index_row.left_context, scan_row.left_context,
                "{query_text}"
            );
            assert_eq!(index_row.node, scan_row.node, "{query_text}");
            assert_eq!(
                index_row.right_context, scan_row.right_context,
                "{query_text}"
            );
        }
    }

    #[test]
    fn records_preserve_canonical_surfaces_and_prepared_character_offsets() {
        let text = "ação coração não João maçã 42";
        let documents = [document_input(7, "pt.txt", text)];

        let store = build_index(&documents);
        let records = store.records_for_document(7).unwrap();

        assert_eq!(
            records
                .iter()
                .map(|record| record.surface.as_str())
                .collect::<Vec<_>>(),
            ["ação", "coração", "não", "João", "maçã", "42"]
        );
        assert_eq!(records[0].normalised_surface, "ação");
        assert_eq!(records[3].normalised_surface, "joão");
        assert_eq!(records[3].token_ordinal_in_document, 3);
        assert_eq!(records[0].prepared_char_start, 0);
        assert_eq!(records[0].prepared_char_end, 4);
        assert_eq!(
            surface_at_char_offsets(
                text,
                records[1].prepared_char_start,
                records[1].prepared_char_end
            ),
            "coração"
        );
        assert!(records.iter().all(|record| record.counts_as_word));
    }

    #[test]
    fn postings_are_sorted_by_document_and_token_ordinal() {
        let documents = [
            document_input(2, "b.txt", "alpha beta alpha"),
            document_input(0, "a.txt", "beta alpha"),
        ];

        let store = build_index(&documents);
        let postings = store.postings("alpha").unwrap();

        assert_eq!(
            postings
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 1), (2, 0), (2, 2)]
        );
        assert_eq!(store.document_frequency("alpha"), 2);
    }

    #[test]
    fn query_ast_resolves_single_terms_and_phrases_without_crossing_documents() {
        let documents = [
            document_input(0, "a.txt", "a good movie is here"),
            document_input(1, "b.txt", "good"),
            document_input(2, "c.txt", "movie"),
        ];
        let store = build_index(&documents);
        let config = TokenizationConfig::default();

        let term = SurfaceIndexQuery::from_surface_text("movie", &config).unwrap();
        let term_hits = search_surface_index(&store, &term).unwrap();
        assert_eq!(
            term_hits
                .hits
                .iter()
                .map(|hit| (hit.document_id, hit.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 2), (2, 0)]
        );

        let phrase = SurfaceIndexQuery::from_surface_text("good movie", &config).unwrap();
        let phrase_hits = search_surface_index(&store, &phrase).unwrap();
        assert_eq!(
            phrase_hits
                .hits
                .iter()
                .map(|hit| (hit.document_id, hit.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 1)]
        );
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_english_queries() {
        let documents = [
            concordance_document(
                0,
                "a.txt",
                "I love it. This is a good movie. Another movie is good.",
            ),
            concordance_document(
                1,
                "b.txt",
                "Good movie lovers say I love it when the movie is good.",
            ),
        ];

        for query in ["movie", "good movie", "I love it"] {
            assert_scan_index_parity(&documents, query);
        }
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_portuguese_queries() {
        let documents = [
            concordance_document(
                0,
                "pt-a.txt",
                "João não perdeu a ação. O coração de João ama ação.",
            ),
            concordance_document(1, "pt-b.txt", "A ação toca o coração, e João diz não."),
        ];

        for query in ["ação", "coração", "não", "João"] {
            assert_scan_index_parity(&documents, query);
        }
    }

    #[test]
    fn windows_1252_recovered_portuguese_offsets_and_parity_hold() {
        let bytes = [
            b'c', b'a', b'f', 0xe9, b' ', b'a', 0xe7, 0xe3, b'o', b' ', b'J', b'o', 0xe3, b'o',
            b' ', b'c', b'o', b'r', b'a', 0xe7, 0xe3, b'o', b' ', b'n', 0xe3, b'o', b' ', b'm',
            b'a', 0xe7, 0xe3,
        ];
        let decoded = decode_source_bytes(&bytes);
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert_eq!(decoded.text, "café ação João coração não maçã");

        let documents = [document_input(0, "pt-win1252.txt", &decoded.text)];
        let store = build_index(&documents);
        let records = store.records_for_document(0).unwrap();

        for expected_surface in ["ação", "João", "coração", "não", "maçã"] {
            let record = records
                .iter()
                .find(|record| record.surface == expected_surface)
                .unwrap();
            assert_eq!(
                surface_at_char_offsets(
                    &decoded.text,
                    record.prepared_char_start,
                    record.prepared_char_end
                ),
                expected_surface
            );
        }
        let acao = records
            .iter()
            .find(|record| record.surface == "ação")
            .unwrap();
        assert_eq!((acao.prepared_char_start, acao.prepared_char_end), (5, 9));

        let concordance_documents = [concordance_document(
            0,
            "pt-win1252.txt",
            decoded.text.as_str(),
        )];
        for query in ["ação", "coração", "não", "João"] {
            assert_scan_index_parity(&concordance_documents, query);
        }
    }

    #[test]
    fn manifest_invalidates_when_identity_inputs_change() {
        let expected = SurfaceIndexManifest::new(
            "processed-hash".to_string(),
            TokenizationManifest::default(),
        );

        let mut changed_schema = expected.clone();
        changed_schema.schema_version += 1;
        assert!(!changed_schema.is_valid_for(&expected));

        let mut changed_decode = expected.clone();
        changed_decode.decode_policy_version += 1;
        assert!(!changed_decode.is_valid_for(&expected));

        let mut changed_tokenization = expected.clone();
        changed_tokenization.tokenization.version = "2".to_string();
        assert!(!changed_tokenization.is_valid_for(&expected));

        let mut changed_processed_text = expected.clone();
        changed_processed_text.processed_texts_hash_sha256 = "other-hash".to_string();
        assert!(!changed_processed_text.is_valid_for(&expected));
    }

    #[test]
    fn index_types_remain_rust_only_without_ts_export() {
        let source = include_str!("index.rs");
        let ts_export_attr = concat!("#[", "ts(export)]");
        let ts_derive = concat!("derive(", "TS");

        assert!(!source.contains(ts_export_attr));
        assert!(!source.contains(ts_derive));
    }
}
