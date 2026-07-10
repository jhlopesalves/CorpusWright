use crate::analysis::concordance_key;
use crate::decode::DECODE_POLICY_VERSION;
use crate::tokenization::{TokenizationConfig, TokenizationManifest, tokenize};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub const INDEX_SCHEMA_VERSION: u32 = 3;
pub const SURFACE_INDEX_CACHE_FILE_VERSION: u32 = 1;

const DEFAULT_WINDOW_TOKENS: usize = 5;
const MAX_WINDOW_TOKENS: usize = 20;
const PARALLEL_INDEX_DOCUMENT_BATCH_SIZE: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexDocumentInput<'text> {
    pub document_id: usize,
    pub source_path: String,
    pub display_name: String,
    pub prepared_text: &'text str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    surface_symbol_id: u32,
    normalised_surface_symbol_id: u32,
    exact_surface_symbol_id: u32,
    pub prepared_char_start: usize,
    pub prepared_char_end: usize,
    pub counts_as_word: bool,
}

impl Serialize for SurfaceIndexTokenRecord {
    fn serialize<Serializer>(
        &self,
        serializer: Serializer,
    ) -> Result<Serializer::Ok, Serializer::Error>
    where
        Serializer: serde::Serializer,
    {
        (
            self.document_id,
            self.token_ordinal_in_document,
            self.surface_symbol_id,
            self.normalised_surface_symbol_id,
            self.exact_surface_symbol_id,
            self.prepared_char_start,
            self.prepared_char_end,
            self.counts_as_word,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SurfaceIndexTokenRecord {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let (
            document_id,
            token_ordinal_in_document,
            surface_symbol_id,
            normalised_surface_symbol_id,
            exact_surface_symbol_id,
            prepared_char_start,
            prepared_char_end,
            counts_as_word,
        ) = <(usize, usize, u32, u32, u32, usize, usize, bool)>::deserialize(deserializer)?;
        Ok(Self {
            document_id,
            token_ordinal_in_document,
            surface_symbol_id,
            normalised_surface_symbol_id,
            exact_surface_symbol_id,
            prepared_char_start,
            prepared_char_end,
            counts_as_word,
        })
    }
}

impl SurfaceIndexTokenRecord {
    fn concordance_symbol_id(&self, case_sensitive: bool) -> u32 {
        if case_sensitive {
            self.exact_surface_symbol_id
        } else {
            self.normalised_surface_symbol_id
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfacePosting {
    pub document_id: usize,
    pub token_ordinal_in_document: usize,
}

impl Serialize for SurfacePosting {
    fn serialize<Serializer>(
        &self,
        serializer: Serializer,
    ) -> Result<Serializer::Ok, Serializer::Error>
    where
        Serializer: serde::Serializer,
    {
        (self.document_id, self.token_ordinal_in_document).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SurfacePosting {
    fn deserialize<Deserializer>(deserializer: Deserializer) -> Result<Self, Deserializer::Error>
    where
        Deserializer: serde::Deserializer<'de>,
    {
        let (document_id, token_ordinal_in_document) = <(usize, usize)>::deserialize(deserializer)?;
        Ok(Self {
            document_id,
            token_ordinal_in_document,
        })
    }
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
    Allocation(String),
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
            Self::Allocation(detail) => {
                write!(formatter, "surface index allocation failed: {detail}")
            }
        }
    }
}

impl std::error::Error for SurfaceIndexError {}

pub trait IndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]>;
    fn exact_surface_postings(&self, exact_surface: &str) -> Option<&[SurfacePosting]>;
    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]>;
    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument>;
    fn total_indexed_tokens(&self) -> usize;
    fn total_indexed_documents(&self) -> usize;
    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store>;
    fn indexed_exact_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store>;
    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store>;
    fn surface_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
    ) -> Option<&'store str>;
    fn concordance_key_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<&'store str>;

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

    fn postings_for_concordance_key(
        &self,
        concordance_key: &str,
        case_sensitive: bool,
    ) -> Option<&[SurfacePosting]> {
        if case_sensitive {
            self.exact_surface_postings(concordance_key)
        } else {
            self.postings(concordance_key)
        }
    }

    fn indexed_terms_for_concordance_key<'store>(
        &'store self,
        case_sensitive: bool,
    ) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        if case_sensitive {
            self.indexed_exact_terms()
        } else {
            self.indexed_terms()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InMemoryIndexStore {
    documents: Vec<SurfaceIndexDocument>,
    records_by_document: Vec<SurfaceIndexDocumentRecords>,
    symbols: Vec<String>,
    #[serde(skip)]
    symbol_ids_by_hash: HashMap<u64, Vec<u32>>,
    postings_by_normalised_symbol: Vec<Vec<SurfacePosting>>,
    postings_by_exact_symbol: Vec<Vec<SurfacePosting>>,
    total_indexed_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SurfaceIndexDocumentRecords {
    document_id: usize,
    records: Vec<SurfaceIndexTokenRecord>,
}

#[derive(Debug)]
struct TokenizedIndexDocument {
    document_id: usize,
    records: Vec<TokenizedIndexRecord>,
}

#[derive(Debug, Clone, Copy)]
struct TokenizedIndexRecord {
    byte_start: usize,
    byte_end: usize,
    prepared_char_start: usize,
    prepared_char_end: usize,
}

impl InMemoryIndexStore {
    pub fn build(
        documents: &[SurfaceIndexDocumentInput<'_>],
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        let mut sorted_documents = Vec::new();
        try_reserve(
            &mut sorted_documents,
            documents.len(),
            "document input ordering",
        )?;
        sorted_documents.extend(documents.iter());
        sorted_documents.sort_by_key(|document| document.document_id);
        for pair in sorted_documents.windows(2) {
            if pair[0].document_id == pair[1].document_id {
                return Err(SurfaceIndexError::DuplicateDocumentId(pair[0].document_id));
            }
        }

        let mut store = Self::default();
        try_reserve(
            &mut store.documents,
            sorted_documents.len(),
            "indexed documents",
        )?;
        try_reserve(
            &mut store.records_by_document,
            sorted_documents.len(),
            "per-document record lists",
        )?;

        for document_batch in sorted_documents.chunks(PARALLEL_INDEX_DOCUMENT_BATCH_SIZE) {
            let mut tokenized = document_batch
                .par_iter()
                .map(|document| tokenize_index_document(document, tokenization_config))
                .try_fold(Vec::new, |mut chunk, document| {
                    let document = document?;
                    try_reserve(&mut chunk, 1, "parallel document results")?;
                    chunk.push(document);
                    Ok(chunk)
                })
                .try_reduce(Vec::new, |mut left, mut right| {
                    try_reserve(&mut left, right.len(), "merged parallel document results")?;
                    left.append(&mut right);
                    Ok(left)
                })?;
            tokenized.sort_by_key(|document| document.document_id);
            for (document, tokenized_document) in document_batch.iter().copied().zip(tokenized) {
                debug_assert_eq!(document.document_id, tokenized_document.document_id);
                store.merge_tokenized_document(document, tokenized_document)?;
            }
        }

        Ok(store)
    }

    fn merge_tokenized_document(
        &mut self,
        document: &SurfaceIndexDocumentInput<'_>,
        tokenized_document: TokenizedIndexDocument,
    ) -> Result<(), SurfaceIndexError> {
        let mut records = Vec::new();
        try_reserve(
            &mut records,
            tokenized_document.records.len(),
            "token records",
        )?;
        for raw_record in tokenized_document.records {
            let surface = &document.prepared_text[raw_record.byte_start..raw_record.byte_end];
            let surface_symbol_id = self.intern_symbol(surface)?;
            let normalised_surface = concordance_key(surface, false);
            let normalised_surface_symbol_id = self.intern_symbol(&normalised_surface)?;
            let exact_surface = concordance_key(surface, true);
            let exact_surface_symbol_id = self.intern_symbol(&exact_surface)?;
            let record = SurfaceIndexTokenRecord {
                document_id: document.document_id,
                token_ordinal_in_document: records.len(),
                surface_symbol_id,
                normalised_surface_symbol_id,
                exact_surface_symbol_id,
                prepared_char_start: raw_record.prepared_char_start,
                prepared_char_end: raw_record.prepared_char_end,
                counts_as_word: true,
            };
            self.push_posting(
                normalised_surface_symbol_id,
                exact_surface_symbol_id,
                &record,
            )?;
            records.push(record);
        }

        self.total_indexed_tokens = self
            .total_indexed_tokens
            .checked_add(records.len())
            .ok_or_else(|| SurfaceIndexError::Allocation("token count overflow".to_string()))?;
        self.documents.push(SurfaceIndexDocument {
            document_id: document.document_id,
            source_path: fallible_string_clone(&document.source_path, "source path")?,
            display_name: fallible_string_clone(&document.display_name, "display name")?,
            token_count: records.len(),
        });
        self.records_by_document.push(SurfaceIndexDocumentRecords {
            document_id: document.document_id,
            records,
        });
        Ok(())
    }

    fn intern_symbol(&mut self, symbol: &str) -> Result<u32, SurfaceIndexError> {
        let hash = symbol_hash(symbol);
        if let Some(ids) = self.symbol_ids_by_hash.get(&hash) {
            for &id in ids {
                if self
                    .symbols
                    .get(id as usize)
                    .is_some_and(|known| known == symbol)
                {
                    return Ok(id);
                }
            }
        }

        let id = u32::try_from(self.symbols.len()).map_err(|_| {
            SurfaceIndexError::Allocation("symbol table exceeds u32 capacity".to_string())
        })?;
        try_reserve(&mut self.symbols, 1, "symbol table")?;
        try_reserve(
            &mut self.postings_by_normalised_symbol,
            1,
            "normalised posting table",
        )?;
        try_reserve(&mut self.postings_by_exact_symbol, 1, "exact posting table")?;
        self.symbols
            .push(fallible_string_clone(symbol, "interned symbol")?);
        self.postings_by_normalised_symbol.push(Vec::new());
        self.postings_by_exact_symbol.push(Vec::new());

        if !self.symbol_ids_by_hash.contains_key(&hash) {
            self.symbol_ids_by_hash
                .try_reserve(1)
                .map_err(|error| allocation_error("symbol hash table", error))?;
        }
        let ids = self.symbol_ids_by_hash.entry(hash).or_default();
        try_reserve(ids, 1, "symbol collision bucket")?;
        ids.push(id);
        Ok(id)
    }

    fn push_posting(
        &mut self,
        normalised_symbol_id: u32,
        exact_symbol_id: u32,
        record: &SurfaceIndexTokenRecord,
    ) -> Result<(), SurfaceIndexError> {
        let posting = SurfacePosting {
            document_id: record.document_id,
            token_ordinal_in_document: record.token_ordinal_in_document,
        };
        let normalised = &mut self.postings_by_normalised_symbol[normalised_symbol_id as usize];
        try_reserve(normalised, 1, "normalised postings")?;
        normalised.push(posting.clone());
        let exact = &mut self.postings_by_exact_symbol[exact_symbol_id as usize];
        try_reserve(exact, 1, "exact postings")?;
        exact.push(posting);
        Ok(())
    }

    fn symbol_id(&self, symbol: &str) -> Option<u32> {
        let hash = symbol_hash(symbol);
        self.symbol_ids_by_hash.get(&hash).and_then(|ids| {
            ids.iter().copied().find(|&id| {
                self.symbols
                    .get(id as usize)
                    .is_some_and(|known| known == symbol)
            })
        })
    }

    fn symbol(&self, symbol_id: u32) -> Option<&str> {
        self.symbols.get(symbol_id as usize).map(String::as_str)
    }

    pub(crate) fn rebuild_symbol_lookup(&mut self) -> Result<(), SurfaceIndexError> {
        self.symbol_ids_by_hash.clear();
        self.symbol_ids_by_hash
            .try_reserve(self.symbols.len())
            .map_err(|error| allocation_error("symbol hash table", error))?;
        for (index, symbol) in self.symbols.iter().enumerate() {
            let id = u32::try_from(index).map_err(|_| {
                SurfaceIndexError::Allocation("symbol table exceeds u32 capacity".to_string())
            })?;
            let ids = self
                .symbol_ids_by_hash
                .entry(symbol_hash(symbol))
                .or_default();
            try_reserve(ids, 1, "symbol collision bucket")?;
            ids.push(id);
        }
        Ok(())
    }

    #[cfg(test)]
    fn total_postings(&self) -> usize {
        self.postings_by_normalised_symbol
            .iter()
            .map(Vec::len)
            .sum()
    }

    pub(crate) fn refresh_document_metadata(
        &mut self,
        inputs: &[SurfaceIndexDocumentInput<'_>],
    ) -> Result<(), SurfaceIndexError> {
        if inputs.len() != self.documents.len() {
            return Err(SurfaceIndexError::InconsistentStore {
                document_id: 0,
                token_ordinal_in_document: 0,
                node_token_length: 0,
            });
        }
        for input in inputs {
            let document = self.document_mut(input.document_id).ok_or(
                SurfaceIndexError::InconsistentStore {
                    document_id: input.document_id,
                    token_ordinal_in_document: 0,
                    node_token_length: 0,
                },
            )?;
            document.source_path = fallible_string_clone(&input.source_path, "source path")?;
            document.display_name = fallible_string_clone(&input.display_name, "display name")?;
        }
        Ok(())
    }

    fn document_mut(&mut self, document_id: usize) -> Option<&mut SurfaceIndexDocument> {
        self.documents
            .binary_search_by_key(&document_id, |document| document.document_id)
            .ok()
            .map(|index| &mut self.documents[index])
    }

    fn has_valid_internal_references(&self) -> bool {
        if self.postings_by_normalised_symbol.len() != self.symbols.len()
            || self.postings_by_exact_symbol.len() != self.symbols.len()
            || self.documents.len() != self.records_by_document.len()
            || self
                .documents
                .windows(2)
                .any(|pair| pair[0].document_id >= pair[1].document_id)
        {
            return false;
        }
        let mut token_total = 0usize;
        for (document, document_records) in self.documents.iter().zip(&self.records_by_document) {
            if document.document_id != document_records.document_id
                || document.token_count != document_records.records.len()
            {
                return false;
            }
            let Some(next_total) = token_total.checked_add(document_records.records.len()) else {
                return false;
            };
            token_total = next_total;
            for (ordinal, record) in document_records.records.iter().enumerate() {
                if record.document_id != document.document_id
                    || record.token_ordinal_in_document != ordinal
                    || record.prepared_char_start > record.prepared_char_end
                    || record.surface_symbol_id as usize >= self.symbols.len()
                    || record.normalised_surface_symbol_id as usize >= self.symbols.len()
                    || record.exact_surface_symbol_id as usize >= self.symbols.len()
                {
                    return false;
                }
            }
        }
        if token_total != self.total_indexed_tokens {
            return false;
        }
        self.postings_are_valid(&self.postings_by_normalised_symbol, false)
            && self.postings_are_valid(&self.postings_by_exact_symbol, true)
    }

    fn postings_are_valid(&self, posting_table: &[Vec<SurfacePosting>], exact: bool) -> bool {
        for (symbol_id, postings) in posting_table.iter().enumerate() {
            if postings.windows(2).any(|pair| {
                (pair[0].document_id, pair[0].token_ordinal_in_document)
                    >= (pair[1].document_id, pair[1].token_ordinal_in_document)
            }) {
                return false;
            }
            for posting in postings {
                let Some(records) = self.records_for_document(posting.document_id) else {
                    return false;
                };
                let Some(record) = records.get(posting.token_ordinal_in_document) else {
                    return false;
                };
                let record_symbol_id = if exact {
                    record.exact_surface_symbol_id
                } else {
                    record.normalised_surface_symbol_id
                };
                if record_symbol_id as usize != symbol_id {
                    return false;
                }
            }
        }
        true
    }
}

impl IndexStore for InMemoryIndexStore {
    fn postings(&self, normalised_surface: &str) -> Option<&[SurfacePosting]> {
        let symbol_id = self.symbol_id(normalised_surface)?;
        self.postings_by_normalised_symbol
            .get(symbol_id as usize)
            .filter(|postings| !postings.is_empty())
            .map(Vec::as_slice)
    }

    fn exact_surface_postings(&self, exact_surface: &str) -> Option<&[SurfacePosting]> {
        let symbol_id = self.symbol_id(exact_surface)?;
        self.postings_by_exact_symbol
            .get(symbol_id as usize)
            .filter(|postings| !postings.is_empty())
            .map(Vec::as_slice)
    }

    fn records_for_document(&self, document_id: usize) -> Option<&[SurfaceIndexTokenRecord]> {
        self.records_by_document
            .binary_search_by_key(&document_id, |entry| entry.document_id)
            .ok()
            .map(|index| self.records_by_document[index].records.as_slice())
    }

    fn document(&self, document_id: usize) -> Option<&SurfaceIndexDocument> {
        self.documents
            .binary_search_by_key(&document_id, |document| document.document_id)
            .ok()
            .map(|index| &self.documents[index])
    }

    fn total_indexed_tokens(&self) -> usize {
        self.total_indexed_tokens
    }

    fn total_indexed_documents(&self) -> usize {
        self.documents.len()
    }

    fn indexed_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        Box::new(
            self.symbols
                .iter()
                .enumerate()
                .filter_map(|(index, symbol)| {
                    (!self.postings_by_normalised_symbol[index].is_empty())
                        .then_some(symbol.as_str())
                }),
        )
    }

    fn indexed_exact_terms<'store>(&'store self) -> Box<dyn Iterator<Item = &'store str> + 'store> {
        Box::new(
            self.symbols
                .iter()
                .enumerate()
                .filter_map(|(index, symbol)| {
                    (!self.postings_by_exact_symbol[index].is_empty()).then_some(symbol.as_str())
                }),
        )
    }

    fn indexed_documents<'store>(
        &'store self,
    ) -> Box<dyn Iterator<Item = &'store SurfaceIndexDocument> + 'store> {
        Box::new(self.documents.iter())
    }

    fn surface_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
    ) -> Option<&'store str> {
        self.symbol(record.surface_symbol_id)
    }

    fn concordance_key_for_record<'store>(
        &'store self,
        record: &SurfaceIndexTokenRecord,
        case_sensitive: bool,
    ) -> Option<&'store str> {
        self.symbol(record.concordance_symbol_id(case_sensitive))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexTerm {
    pub surface: String,
    pub normalised_surface: String,
    pub exact_surface: String,
}

impl SurfaceIndexTerm {
    fn concordance_key(&self, case_sensitive: bool) -> &str {
        if case_sensitive {
            &self.exact_surface
        } else {
            &self.normalised_surface
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceIndexQuery {
    Term {
        term: SurfaceIndexTerm,
        case_sensitive: bool,
    },
    Phrase {
        terms: Vec<SurfaceIndexTerm>,
        case_sensitive: bool,
    },
}

impl SurfaceIndexQuery {
    pub fn from_surface_text(
        query: &str,
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        Self::from_surface_text_with_case(query, false, tokenization_config)
    }

    pub fn from_surface_text_with_case(
        query: &str,
        case_sensitive: bool,
        tokenization_config: &TokenizationConfig,
    ) -> Result<Self, SurfaceIndexError> {
        let terms = query_terms(query, tokenization_config)?;
        if terms.len() == 1 {
            Ok(Self::Term {
                term: terms.into_iter().next().expect("term exists"),
                case_sensitive,
            })
        } else {
            Ok(Self::Phrase {
                terms,
                case_sensitive,
            })
        }
    }

    pub fn terms(&self) -> &[SurfaceIndexTerm] {
        match self {
            Self::Term { term, .. } => std::slice::from_ref(term),
            Self::Phrase { terms, .. } => terms.as_slice(),
        }
    }

    pub fn case_sensitive(&self) -> bool {
        match self {
            Self::Term { case_sensitive, .. } | Self::Phrase { case_sensitive, .. } => {
                *case_sensitive
            }
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

    let case_sensitive = query.case_sensitive();
    let mut hits = if terms.len() == 1 {
        term_hits(store, &terms[0], case_sensitive)
    } else {
        phrase_hits(store, terms, case_sensitive)
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
    let mut rows = Vec::new();
    try_reserve(&mut rows, search.hits.len(), "materialized KWIC rows")?;
    for hit in &search.hits {
        rows.push(materialize_surface_index_kwic_row(
            store,
            hit,
            window_tokens,
        )?);
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

#[derive(Debug)]
pub enum SurfaceIndexCacheLoad {
    Ready(InMemoryIndexStore),
    Missing,
    Stale,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceIndexCacheError(String);

impl std::fmt::Display for SurfaceIndexCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SurfaceIndexCacheError {}

#[derive(Serialize)]
struct SurfaceIndexCacheEnvelopeRef<'store> {
    cache_file_version: u32,
    manifest: &'store SurfaceIndexManifest,
    store: &'store InMemoryIndexStore,
}

#[derive(Deserialize)]
struct SurfaceIndexCacheEnvelope {
    cache_file_version: u32,
    manifest: SurfaceIndexManifest,
    store: InMemoryIndexStore,
}

pub fn surface_index_cache_path(cache_root: &Path, manifest: &SurfaceIndexManifest) -> PathBuf {
    let mut key_bytes = Vec::new();
    key_bytes.extend_from_slice(b"corpuswright-surface-index-cache-key-v1\0");
    key_bytes.extend_from_slice(&SURFACE_INDEX_CACHE_FILE_VERSION.to_le_bytes());
    if let Ok(manifest_bytes) = serde_json::to_vec(manifest) {
        key_bytes.extend_from_slice(&manifest_bytes);
    }
    let key = crate::hash::sha256_hex(&key_bytes);
    cache_root.join(format!(
        "surface-index.v{}.{}.json",
        SURFACE_INDEX_CACHE_FILE_VERSION, key
    ))
}

pub fn write_surface_index_cache(
    cache_root: &Path,
    manifest: &SurfaceIndexManifest,
    store: &InMemoryIndexStore,
) -> Result<(), SurfaceIndexCacheError> {
    std::fs::create_dir_all(cache_root)
        .map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    let final_path = surface_index_cache_path(cache_root, manifest);
    let temp_path = final_path.with_extension("tmp");
    let file =
        File::create(&temp_path).map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    let mut writer = BufWriter::new(file);
    let envelope = SurfaceIndexCacheEnvelopeRef {
        cache_file_version: SURFACE_INDEX_CACHE_FILE_VERSION,
        manifest,
        store,
    };
    serde_json::to_writer(&mut writer, &envelope)
        .map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    writer
        .flush()
        .map_err(|error| SurfaceIndexCacheError(error.to_string()))?;
    drop(writer);
    if let Err(first_error) = std::fs::rename(&temp_path, &final_path) {
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&temp_path, &final_path).map_err(|second_error| {
            SurfaceIndexCacheError(format!(
                "cache rename failed: {first_error}; retry failed: {second_error}"
            ))
        })?;
    }
    Ok(())
}

pub fn load_surface_index_cache(
    cache_root: &Path,
    expected_manifest: &SurfaceIndexManifest,
) -> SurfaceIndexCacheLoad {
    let path = surface_index_cache_path(cache_root, expected_manifest);
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return SurfaceIndexCacheLoad::Missing;
        }
        Err(_) => return SurfaceIndexCacheLoad::Invalid,
    };
    let mut envelope: SurfaceIndexCacheEnvelope =
        match serde_json::from_reader(std::io::BufReader::new(file)) {
            Ok(envelope) => envelope,
            Err(_) => return SurfaceIndexCacheLoad::Invalid,
        };
    if envelope.cache_file_version != SURFACE_INDEX_CACHE_FILE_VERSION
        || envelope.manifest.schema_version != INDEX_SCHEMA_VERSION
        || !envelope.manifest.is_valid_for(expected_manifest)
    {
        return SurfaceIndexCacheLoad::Stale;
    }
    if envelope.store.rebuild_symbol_lookup().is_err()
        || !envelope.store.has_valid_internal_references()
    {
        return SurfaceIndexCacheLoad::Invalid;
    }
    SurfaceIndexCacheLoad::Ready(envelope.store)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparedOffsetKind {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedOffsetRequest {
    token_index: usize,
    byte_start: usize,
    byte_end: usize,
    byte_offset: usize,
    kind: PreparedOffsetKind,
}

fn tokenize_index_document(
    document: &SurfaceIndexDocumentInput<'_>,
    tokenization_config: &TokenizationConfig,
) -> Result<TokenizedIndexDocument, SurfaceIndexError> {
    let tokenizer_document_id = u32::try_from(document.document_id)
        .map_err(|_| SurfaceIndexError::DocumentIdTooLarge(document.document_id))?;
    let mut word_tokens = tokenize(
        document.prepared_text,
        tokenizer_document_id,
        tokenization_config,
    );
    word_tokens.retain(|token| token.counts_as_word());

    let mut token_byte_offsets = Vec::new();
    try_reserve(
        &mut token_byte_offsets,
        word_tokens.len(),
        "token byte offsets",
    )?;
    token_byte_offsets.extend(
        word_tokens
            .iter()
            .map(|token| (token.byte_start, token.byte_end)),
    );
    let prepared_char_offsets = prepared_char_offsets_for_token_bytes(
        document.prepared_text,
        document.document_id,
        &token_byte_offsets,
    )?;
    let mut records = Vec::new();
    try_reserve(&mut records, word_tokens.len(), "parallel token records")?;
    records.extend(word_tokens.into_iter().zip(prepared_char_offsets).map(
        |(token, (prepared_char_start, prepared_char_end))| TokenizedIndexRecord {
            byte_start: token.byte_start,
            byte_end: token.byte_end,
            prepared_char_start,
            prepared_char_end,
        },
    ));
    Ok(TokenizedIndexDocument {
        document_id: document.document_id,
        records,
    })
}

fn symbol_hash(symbol: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    symbol.hash(&mut hasher);
    hasher.finish()
}

fn try_reserve<T>(
    values: &mut Vec<T>,
    additional: usize,
    purpose: &str,
) -> Result<(), SurfaceIndexError> {
    values
        .try_reserve(additional)
        .map_err(|error| allocation_error(purpose, error))
}

fn allocation_error(purpose: &str, error: impl std::fmt::Display) -> SurfaceIndexError {
    SurfaceIndexError::Allocation(format!("{purpose}: {error}"))
}

fn fallible_string_clone(value: &str, purpose: &str) -> Result<String, SurfaceIndexError> {
    let mut cloned = String::new();
    cloned
        .try_reserve(value.len())
        .map_err(|error| allocation_error(purpose, error))?;
    cloned.push_str(value);
    Ok(cloned)
}

fn prepared_char_offsets_for_token_bytes(
    text: &str,
    document_id: usize,
    token_byte_offsets: &[(usize, usize)],
) -> Result<Vec<(usize, usize)>, SurfaceIndexError> {
    // Tokenizer offsets are prepared-text byte offsets. The index stores
    // prepared-text character offsets for KWIC spans, and deliberately does not
    // compute raw/source offsets.
    //
    // Resolve every requested byte offset during one UTF-8 boundary walk. Any
    // tokenizer offset that is out of range or inside a multi-byte character
    // remains unresolved and is reported loudly.
    let request_count = token_byte_offsets.len().checked_mul(2).ok_or_else(|| {
        SurfaceIndexError::Allocation("prepared offset count overflow".to_string())
    })?;
    let mut requests = Vec::new();
    try_reserve(&mut requests, request_count, "prepared offset requests")?;
    for (token_index, (byte_start, byte_end)) in token_byte_offsets.iter().copied().enumerate() {
        requests.push(PreparedOffsetRequest {
            token_index,
            byte_start,
            byte_end,
            byte_offset: byte_start,
            kind: PreparedOffsetKind::Start,
        });
        requests.push(PreparedOffsetRequest {
            token_index,
            byte_start,
            byte_end,
            byte_offset: byte_end,
            kind: PreparedOffsetKind::End,
        });
    }
    requests.sort_by(|left, right| {
        left.byte_offset
            .cmp(&right.byte_offset)
            .then_with(|| left.token_index.cmp(&right.token_index))
            .then_with(|| {
                matches!(left.kind, PreparedOffsetKind::End)
                    .cmp(&matches!(right.kind, PreparedOffsetKind::End))
            })
    });

    let mut resolved = Vec::new();
    try_reserve(
        &mut resolved,
        token_byte_offsets.len(),
        "resolved prepared offsets",
    )?;
    resolved.resize(token_byte_offsets.len(), (0usize, 0usize));
    let mut next_request = 0usize;

    resolve_prepared_offset_requests(&requests, &mut next_request, 0, 0, &mut resolved);
    let mut byte_offset = 0usize;
    let mut char_offset = 0usize;
    for ch in text.chars() {
        byte_offset += ch.len_utf8();
        char_offset += 1;
        resolve_prepared_offset_requests(
            &requests,
            &mut next_request,
            byte_offset,
            char_offset,
            &mut resolved,
        );
    }

    if let Some(request) = requests.get(next_request) {
        return Err(SurfaceIndexError::InvalidPreparedOffset {
            document_id,
            byte_start: request.byte_start,
            byte_end: request.byte_end,
        });
    }

    Ok(resolved)
}

fn resolve_prepared_offset_requests(
    requests: &[PreparedOffsetRequest],
    next_request: &mut usize,
    byte_offset: usize,
    char_offset: usize,
    resolved: &mut [(usize, usize)],
) {
    while requests
        .get(*next_request)
        .is_some_and(|request| request.byte_offset == byte_offset)
    {
        let request = requests[*next_request];
        match request.kind {
            PreparedOffsetKind::Start => resolved[request.token_index].0 = char_offset,
            PreparedOffsetKind::End => resolved[request.token_index].1 = char_offset,
        }
        *next_request += 1;
    }
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
            normalised_surface: concordance_key(token.surface, false),
            exact_surface: concordance_key(token.surface, true),
        })
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Err(SurfaceIndexError::EmptyQuery);
    }
    Ok(terms)
}

fn term_hits(
    store: &impl IndexStore,
    term: &SurfaceIndexTerm,
    case_sensitive: bool,
) -> Vec<SurfaceIndexHit> {
    store
        .postings_for_concordance_key(term.concordance_key(case_sensitive), case_sensitive)
        .unwrap_or_default()
        .iter()
        .filter_map(|posting| {
            let record = store
                .records_for_document(posting.document_id)?
                .get(posting.token_ordinal_in_document)?;
            Some(SurfaceIndexHit {
                document_id: posting.document_id,
                token_ordinal_in_document: posting.token_ordinal_in_document,
                node_token_length: 1,
                prepared_char_start: record.prepared_char_start,
                prepared_char_end: record.prepared_char_end,
            })
        })
        .collect()
}

fn phrase_hits(
    store: &impl IndexStore,
    terms: &[SurfaceIndexTerm],
    case_sensitive: bool,
) -> Vec<SurfaceIndexHit> {
    let Some(first_postings) = store
        .postings_for_concordance_key(terms[0].concordance_key(case_sensitive), case_sensitive)
    else {
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
        let Some(first_record) = records.get(posting.token_ordinal_in_document) else {
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
            if store.concordance_key_for_record(record, case_sensitive)
                != Some(term.concordance_key(case_sensitive))
            {
                matches = false;
                break;
            }
        }
        if matches {
            hits.push(SurfaceIndexHit {
                document_id: posting.document_id,
                token_ordinal_in_document: posting.token_ordinal_in_document,
                node_token_length: terms.len(),
                prepared_char_start: first_record.prepared_char_start,
                prepared_char_end: last_record.prepared_char_end,
            });
        }
    }
    hits
}

pub fn materialize_surface_index_kwic_row(
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
    let left_context = surfaces(store, &records[left_start..hit.token_ordinal_in_document])?;
    let node_tokens = surfaces(store, &records[hit.token_ordinal_in_document..node_end])?;
    let right_context = surfaces(store, &records[node_end..right_end])?;

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

fn surfaces(
    store: &impl IndexStore,
    records: &[SurfaceIndexTokenRecord],
) -> Result<Vec<String>, SurfaceIndexError> {
    let mut surfaces = Vec::new();
    try_reserve(&mut surfaces, records.len(), "materialized KWIC surfaces")?;
    for record in records {
        let surface =
            store
                .surface_for_record(record)
                .ok_or(SurfaceIndexError::InconsistentStore {
                    document_id: record.document_id,
                    token_ordinal_in_document: record.token_ordinal_in_document,
                    node_token_length: 1,
                })?;
        surfaces.push(surface.to_string());
    }
    Ok(surfaces)
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
        AnalysisDocumentSelection, ConcordanceBackend, ConcordanceContextFilterDirection,
        ConcordanceDocument, ConcordanceQuery, ConcordanceSearchContext, ConcordanceSortKey,
        search_concordance_documents, search_concordance_documents_routed,
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

    fn scan_query(query: &str, case_sensitive: bool) -> ConcordanceQuery {
        ConcordanceQuery {
            selection: AnalysisDocumentSelection::default(),
            query: query.to_string(),
            context_filter: None,
            context_filter_direction: ConcordanceContextFilterDirection::Both,
            case_sensitive,
            sort_key: ConcordanceSortKey::FilePosition,
            window_tokens: 5,
            offset: 0,
            limit: 500,
            distribution_offset: 0,
            distribution_limit: 500,
        }
    }

    fn assert_scan_index_parity(
        documents: &[ConcordanceDocument],
        query_text: &str,
        case_sensitive: bool,
    ) {
        let tokenization_config = TokenizationConfig::default();
        let scan = search_concordance_documents(
            documents,
            &scan_query(query_text, case_sensitive),
            &tokenization_config,
        )
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
        let query = SurfaceIndexQuery::from_surface_text_with_case(
            query_text,
            case_sensitive,
            &tokenization_config,
        )
        .unwrap();
        let index = search_surface_index_kwic(&store, &query, scan.window_tokens).unwrap();
        let message = format!("{query_text} case_sensitive={case_sensitive}");

        assert_eq!(index.total_hits, scan.total_hits, "{message}");
        assert_eq!(
            index.documents_with_hits, scan.documents_with_hits,
            "{message}"
        );
        assert_eq!(
            index.selected_documents_total, scan.selected_documents_total,
            "{message}"
        );
        assert_eq!(
            index.selected_tokens_total as u64, scan.selected_tokens_total,
            "{message}"
        );
        assert_close(index.top_document_share, scan.top_document_share);
        let scan_hit_counts = scan
            .distribution
            .items
            .iter()
            .filter(|item| item.count > 0)
            .map(|item| (item.document_id, item.count))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(index.document_hit_counts, scan_hit_counts, "{message}");
        assert_eq!(index.rows.len(), scan.rows.len(), "{message}");
        for (index_row, scan_row) in index.rows.iter().zip(&scan.rows) {
            assert_eq!(index_row.document_id, scan_row.document_index, "{message}");
            assert_eq!(
                index_row.token_ordinal_in_document, scan_row.token_index,
                "{message}"
            );
            assert_eq!(
                index_row.node_token_length, scan_row.node_token_length,
                "{message}"
            );
            assert_eq!(index_row.left_context, scan_row.left_context, "{message}");
            assert_eq!(index_row.node, scan_row.node, "{message}");
            assert_eq!(index_row.right_context, scan_row.right_context, "{message}");
        }
    }

    fn assert_scan_index_parity_both_modes(documents: &[ConcordanceDocument], query_text: &str) {
        assert_scan_index_parity(documents, query_text, false);
        assert_scan_index_parity(documents, query_text, true);
    }

    fn hit_positions(result: &SurfaceIndexSearchResult) -> Vec<(usize, usize)> {
        result
            .hits
            .iter()
            .map(|hit| (hit.document_id, hit.token_ordinal_in_document))
            .collect()
    }

    fn search_positions(
        store: &InMemoryIndexStore,
        query_text: &str,
        case_sensitive: bool,
    ) -> Vec<(usize, usize)> {
        let config = TokenizationConfig::default();
        let query =
            SurfaceIndexQuery::from_surface_text_with_case(query_text, case_sensitive, &config)
                .unwrap();
        hit_positions(&search_surface_index(store, &query).unwrap())
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
                .map(|record| store.surface_for_record(record).unwrap())
                .collect::<Vec<_>>(),
            ["ação", "coração", "não", "João", "maçã", "42"]
        );
        assert_eq!(
            store.concordance_key_for_record(&records[0], false),
            Some("ação")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[3], false),
            Some("joão")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[3], true),
            Some("João")
        );
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
    fn default_aggregation_key_matches_concordance_case_insensitive_key() {
        let config = TokenizationConfig::default();
        let tokens = tokenize("Movie ação João", 0, &config)
            .into_iter()
            .filter(|token| token.counts_as_word())
            .collect::<Vec<_>>();

        for token in tokens {
            assert_eq!(
                token.aggregation_key(&config),
                concordance_key(token.surface, false)
            );
        }
    }

    #[test]
    fn exact_surface_postings_distinguish_case_from_normalised_postings() {
        let documents = [
            document_input(2, "b.txt", "MOVIE movie"),
            document_input(0, "a.txt", "Movie movie MOVIE"),
        ];

        let store = build_index(&documents);
        let normalised_postings = store.postings("movie").unwrap();

        assert_eq!(
            normalised_postings
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 0), (0, 1), (0, 2), (2, 0), (2, 1)]
        );
        assert_eq!(
            store
                .exact_surface_postings("movie")
                .unwrap()
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 1), (2, 1)]
        );
        assert_eq!(
            store
                .exact_surface_postings("Movie")
                .unwrap()
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 0)]
        );
        assert_eq!(
            store
                .exact_surface_postings("MOVIE")
                .unwrap()
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 2), (2, 0)]
        );
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

        let exact_postings = store.exact_surface_postings("alpha").unwrap();
        assert_eq!(
            exact_postings
                .iter()
                .map(|posting| (posting.document_id, posting.token_ordinal_in_document))
                .collect::<Vec<_>>(),
            [(0, 1), (2, 0), (2, 2)]
        );
    }

    #[test]
    fn query_ast_resolves_case_modes_without_crossing_documents() {
        let documents = [
            document_input(
                0,
                "a.txt",
                "Movie, movie. MOVIE. good movie. Good movie! I love it. i love it.",
            ),
            document_input(1, "b.txt", "good"),
            document_input(2, "c.txt", "movie"),
        ];
        let store = build_index(&documents);

        assert_eq!(
            search_positions(&store, "movie", false),
            [(0, 0), (0, 1), (0, 2), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(
            search_positions(&store, "Movie", false),
            [(0, 0), (0, 1), (0, 2), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(
            search_positions(&store, "movie", true),
            [(0, 1), (0, 4), (0, 6), (2, 0)]
        );
        assert_eq!(search_positions(&store, "Movie", true), [(0, 0)]);
        assert_eq!(search_positions(&store, "MOVIE", true), [(0, 2)]);

        assert_eq!(
            search_positions(&store, "good movie", false),
            [(0, 3), (0, 5)]
        );
        assert_eq!(
            search_positions(&store, "Good movie", false),
            [(0, 3), (0, 5)]
        );
        assert_eq!(search_positions(&store, "good movie", true), [(0, 3)]);
        assert_eq!(search_positions(&store, "Good movie", true), [(0, 5)]);

        let boundary_documents = [
            document_input(1, "boundary-a.txt", "good"),
            document_input(2, "boundary-b.txt", "movie"),
        ];
        let boundary_store = build_index(&boundary_documents);
        assert!(search_positions(&boundary_store, "good movie", false).is_empty());
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_english_queries() {
        let documents = [
            concordance_document(
                0,
                "a.txt",
                "Movie, movie. MOVIE. good movie. Good movie! I love it. i love it.",
            ),
            concordance_document(
                1,
                "b.txt",
                "A movie is not the same as a Good movie? I love it.",
            ),
        ];

        for query in [
            "movie",
            "Movie",
            "MOVIE",
            "good movie",
            "Good movie",
            "I love it",
            "i love it",
        ] {
            assert_scan_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn scan_vs_index_parity_holds_for_required_portuguese_queries() {
        let documents = [
            concordance_document(
                0,
                "pt-a.txt",
                "ação Ação coração Coração não Não João joão maçã Maçã.",
            ),
            concordance_document(1, "pt-b.txt", "A ação toca o coração, e João diz não."),
        ];

        for query in [
            "ação",
            "Ação",
            "coração",
            "Coração",
            "não",
            "Não",
            "João",
            "joão",
        ] {
            assert_scan_index_parity_both_modes(&documents, query);
        }
    }

    #[test]
    fn windows_1252_recovered_portuguese_offsets_and_parity_hold() {
        let bytes = b"caf\xe9 a\xe7\xe3o A\xe7\xe3o Jo\xe3o jo\xe3o cora\xe7\xe3o Cora\xe7\xe3o n\xe3o N\xe3o ma\xe7\xe3 Ma\xe7\xe3";
        let decoded = decode_source_bytes(bytes);
        assert_eq!(decoded.detected_encoding, SourceEncoding::Windows1252);
        assert_eq!(decoded.decode_strategy, DecodeStrategy::Windows1252Fallback);
        assert!(decoded.had_utf8_error);
        assert_eq!(
            decoded.text,
            "café ação Ação João joão coração Coração não Não maçã Maçã"
        );
        assert!(!decoded.text.contains('\u{fffd}'));

        let documents = [document_input(0, "pt-win1252.txt", &decoded.text)];
        let store = build_index(&documents);
        let records = store.records_for_document(0).unwrap();

        for expected_surface in [
            "ação",
            "Ação",
            "João",
            "joão",
            "coração",
            "Coração",
            "não",
            "Não",
            "maçã",
            "Maçã",
        ] {
            let record = records
                .iter()
                .find(|record| store.surface_for_record(record) == Some(expected_surface))
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
            .find(|record| store.surface_for_record(record) == Some("ação"))
            .unwrap();
        assert_eq!((acao.prepared_char_start, acao.prepared_char_end), (5, 9));

        assert!(
            records
                .iter()
                .any(|record| store.surface_for_record(record) == Some("João"))
        );
        assert!(
            records
                .iter()
                .any(|record| store.surface_for_record(record) == Some("joão"))
        );

        let concordance_documents = [concordance_document(
            0,
            "pt-win1252.txt",
            decoded.text.as_str(),
        )];
        for query in [
            "ação",
            "Ação",
            "coração",
            "Coração",
            "não",
            "Não",
            "João",
            "joão",
        ] {
            assert_scan_index_parity_both_modes(&concordance_documents, query);
        }
    }

    #[test]
    fn linear_offset_conversion_matches_reference_character_offsets() {
        let config = TokenizationConfig::default();
        let text = "ação Coração não João maçã ".repeat(40);
        let documents = [document_input(3, "long-pt.txt", &text)];
        let store = build_index(&documents);
        let records = store.records_for_document(3).unwrap();
        let word_tokens = tokenize(&text, 3, &config)
            .into_iter()
            .filter(|token| token.counts_as_word())
            .collect::<Vec<_>>();

        assert_eq!(records.len(), word_tokens.len());
        for (record, token) in records.iter().zip(&word_tokens) {
            let reference_start = text[..token.byte_start].chars().count();
            let reference_end = text[..token.byte_end].chars().count();
            assert_eq!(record.prepared_char_start, reference_start);
            assert_eq!(record.prepared_char_end, reference_end);
        }
    }

    #[test]
    fn invalid_prepared_offset_inside_utf8_codepoint_still_errors() {
        let error = prepared_char_offsets_for_token_bytes("ação", 7, &[(2, 3)]).unwrap_err();

        assert_eq!(
            error,
            SurfaceIndexError::InvalidPreparedOffset {
                document_id: 7,
                byte_start: 2,
                byte_end: 3,
            }
        );
    }

    #[test]
    fn manifest_invalidates_when_identity_inputs_change() {
        let expected = SurfaceIndexManifest::new(
            "processed-hash".to_string(),
            TokenizationManifest::default(),
        );
        assert_eq!(expected.schema_version, 3);

        let mut changed_schema = expected.clone();
        changed_schema.schema_version = 2;
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

    #[test]
    fn parallel_index_build_is_deterministic_across_thread_counts_and_input_order() {
        let texts = (0..256)
            .map(|index| {
                format!(
                    "shared Movie movie MOVIE alpha{} beta{} shared ação João ",
                    index % 17,
                    index % 11
                )
                .repeat(12)
            })
            .collect::<Vec<_>>();
        let one_thread_inputs = (0..texts.len())
            .map(|document_id| SurfaceIndexDocumentInput {
                document_id,
                source_path: format!("{document_id}.txt"),
                display_name: format!("{document_id}.txt"),
                prepared_text: texts[document_id].as_str(),
            })
            .collect::<Vec<_>>();
        let four_thread_inputs = (0..texts.len())
            .rev()
            .map(|document_id| SurfaceIndexDocumentInput {
                document_id,
                source_path: format!("{document_id}.txt"),
                display_name: format!("{document_id}.txt"),
                prepared_text: texts[document_id].as_str(),
            })
            .collect::<Vec<_>>();
        let one_thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let four_thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        assert_eq!(one_thread_pool.current_num_threads(), 1);
        assert_eq!(four_thread_pool.current_num_threads(), 4);

        let one_thread = one_thread_pool.install(|| build_index(&one_thread_inputs));
        let four_thread = four_thread_pool.install(|| build_index(&four_thread_inputs));
        assert_eq!(one_thread, four_thread);
        assert_eq!(
            search_positions(&one_thread, "movie", false),
            search_positions(&four_thread, "movie", false)
        );
        assert_eq!(
            search_positions(&one_thread, "Movie", true),
            search_positions(&four_thread, "Movie", true)
        );
    }

    #[test]
    fn interned_surfaces_resolve_across_case_handling_paths() {
        let documents = [document_input(
            0,
            "case.txt",
            "Movie movie MOVIE Movie movie MOVIE",
        )];
        let store = build_index(&documents);
        let records = store.records_for_document(0).unwrap();

        assert_eq!(store.symbols.len(), 3);
        assert_eq!(store.surface_for_record(&records[0]), Some("Movie"));
        assert_eq!(store.surface_for_record(&records[1]), Some("movie"));
        assert_eq!(store.surface_for_record(&records[2]), Some("MOVIE"));
        assert_eq!(
            store.concordance_key_for_record(&records[0], false),
            Some("movie")
        );
        assert_eq!(
            store.concordance_key_for_record(&records[0], true),
            Some("Movie")
        );
        assert_eq!(search_positions(&store, "movie", false).len(), 6);
        assert_eq!(search_positions(&store, "Movie", true).len(), 2);
    }

    #[test]
    fn surface_index_disk_cache_round_trip_and_miss_states() {
        let temp_dir = tempfile::tempdir().unwrap();
        let documents = [
            document_input(1, "b.txt", "Movie alpha movie"),
            document_input(0, "a.txt", "ação Movie João"),
        ];
        let store = build_index(&documents);
        let manifest =
            SurfaceIndexManifest::new("content-hash".to_string(), TokenizationManifest::default());
        write_surface_index_cache(temp_dir.path(), &manifest, &store).unwrap();
        let loaded = match load_surface_index_cache(temp_dir.path(), &manifest) {
            SurfaceIndexCacheLoad::Ready(store) => store,
            state => panic!("expected ready cache, got {state:?}"),
        };
        assert_eq!(loaded, store);

        let mut changed_config = manifest.clone();
        changed_config.tokenization.config.include_punctuation = false;
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &changed_config),
            SurfaceIndexCacheLoad::Missing
        ));

        let cache_path = surface_index_cache_path(temp_dir.path(), &manifest);
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&cache_path).unwrap()).unwrap();
        envelope["cache_file_version"] =
            serde_json::Value::from(SURFACE_INDEX_CACHE_FILE_VERSION + 1);
        std::fs::write(&cache_path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &manifest),
            SurfaceIndexCacheLoad::Stale
        ));

        std::fs::write(&cache_path, b"{\"cache_file_version\":").unwrap();
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &manifest),
            SurfaceIndexCacheLoad::Invalid
        ));
        std::fs::write(&cache_path, b"not an index").unwrap();
        assert!(matches!(
            load_surface_index_cache(temp_dir.path(), &manifest),
            SurfaceIndexCacheLoad::Invalid
        ));
    }

    #[test]
    fn concordance_index_performance_instrumentation() {
        let texts = (0..2_000)
            .map(|index| {
                format!(
                    "alpha beta target blue gamma Movie ação document{} ",
                    index % 97
                )
                .repeat(5)
            })
            .collect::<Vec<_>>();
        let documents = texts
            .iter()
            .enumerate()
            .map(|(document_index, text)| ConcordanceDocument {
                document_index,
                source_path: format!("{document_index}.txt"),
                display_name: format!("{document_index}.txt"),
                text: text.clone(),
            })
            .collect::<Vec<_>>();
        let inputs = documents
            .iter()
            .map(|document| SurfaceIndexDocumentInput {
                document_id: document.document_index,
                source_path: document.source_path.clone(),
                display_name: document.display_name.clone(),
                prepared_text: document.text.as_str(),
            })
            .collect::<Vec<_>>();
        let config = TokenizationConfig::default();
        let build_started = std::time::Instant::now();
        let store = InMemoryIndexStore::build(&inputs, &config).unwrap();
        let build_ms = build_started.elapsed().as_secs_f64() * 1_000.0;
        let postings_count = store.total_postings();

        let mut query = scan_query("target", false);
        query.context_filter = Some("blue".to_string());
        query.context_filter_direction = ConcordanceContextFilterDirection::Right;
        let scan_started = std::time::Instant::now();
        let scan = search_concordance_documents(&documents, &query, &config).unwrap();
        let scan_ms = scan_started.elapsed().as_secs_f64() * 1_000.0;

        let cache_dir = tempfile::tempdir().unwrap();
        let mut context = ConcordanceSearchContext::with_cache_root(cache_dir.path().to_path_buf());
        let first_started = std::time::Instant::now();
        let first = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let first_ms = first_started.elapsed().as_secs_f64() * 1_000.0;
        let warm_started = std::time::Instant::now();
        let warm = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let warm_ms = warm_started.elapsed().as_secs_f64() * 1_000.0;
        let mut restarted_context =
            ConcordanceSearchContext::with_cache_root(cache_dir.path().to_path_buf());
        let disk_started = std::time::Instant::now();
        let disk = search_concordance_documents_routed(
            &documents,
            &query,
            &config,
            &mut restarted_context,
            ConcordanceBackend::Index,
        )
        .unwrap();
        let disk_ms = disk_started.elapsed().as_secs_f64() * 1_000.0;

        assert_eq!(first.result, scan);
        assert_eq!(warm.result, scan);
        assert_eq!(disk.result, scan);
        assert!(first.index_rebuilt);
        assert!(!warm.index_rebuilt);
        assert!(!disk.index_rebuilt);
        let evidence = serde_json::json!({
            "documents": documents.len(),
            "tokens": store.total_indexed_tokens(),
            "postings": postings_count,
            "build_ms": build_ms,
            "scan_first_query_ms": scan_ms,
            "first_query_ms": first_ms,
            "warm_query_ms": warm_ms,
            "cold_start_with_disk_cache_ms": disk_ms,
        });
        std::fs::write(
            "/tmp/corpuswright-concordance-index-timing.json",
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        eprintln!("concordance index timing: {evidence}");
    }
}
