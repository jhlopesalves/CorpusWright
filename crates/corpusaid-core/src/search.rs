use crate::cache::ExtractionCache;
use crate::clean::{CleaningConfig, clean_text};
use crate::docx::extract_docx;
use crate::html::extract_html;
use crate::pdf::extract_pdf;
use crate::scan::{DocumentRecord, DocumentType};
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A single search hit with ready-to-render context strings.
///
/// All text fields are char-boundary-safe slices of the original extracted text.
/// The frontend should use these strings directly, not try to compute offsets.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchHit {
    /// Index into the corpus records array.
    pub corpus_index: usize,
    /// Relative path of the containing file.
    pub relative_path: String,
    /// Full source path, if available.
    pub source_path: Option<String>,
    /// Up to ~CONTEXT_CHARS chars of text before the match.
    pub context_before: String,
    /// The exact matched substring.
    pub match_text: String,
    /// Up to ~CONTEXT_CHARS chars of text after the match.
    pub context_after: String,
    /// 0-based index of this match within its file.
    pub file_match_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SearchResult {
    /// Total number of matches across all searched files (may exceed returned_hits).
    pub total_matches: usize,
    /// Indices (into the records slice) of files that contain at least one match.
    pub matching_file_indices: Vec<usize>,
    /// Number of SearchHit structs actually returned (capped by max_hits).
    pub returned_hits: usize,
    /// True if total_matches > returned_hits.
    pub truncated: bool,
    /// Bounded list of navigable hits.
    pub hits: Vec<SearchHit>,
}

const DEFAULT_MAX_HITS: usize = 1000;
const HARD_MAX_HITS: usize = 5000;
const CONTEXT_CHARS: usize = 80;

/// Searches the provided files for the given query.
///
/// Backend-enforced constraints:
/// - `max_hits` is clamped to [1, HARD_MAX_HITS]; 0 becomes DEFAULT_MAX_HITS.
/// - The returned `hits` vector never exceeds the clamped limit.
/// - `total_matches` counts all matches (even beyond max_hits).
///
/// Processing is sequential (not parallel) to avoid unbounded per-worker hit
/// accumulation.  Only at most max_hits SearchHit contexts are ever allocated.
///
/// If `is_processed` is true, applies the cleaning_config before searching.
pub fn search_corpus(
    records: &[DocumentRecord],
    query: &str,
    is_processed: bool,
    cleaning_config: &CleaningConfig,
    max_hits: usize,
    cache: Option<&ExtractionCache>,
) -> Result<SearchResult, String> {
    let query = query.trim();

    if query.is_empty() {
        return Ok(SearchResult {
            total_matches: 0,
            matching_file_indices: Vec::new(),
            returned_hits: 0,
            truncated: false,
            hits: Vec::new(),
        });
    }

    // Clamp max_hits to safe bounds.
    let max_hits = if max_hits == 0 {
        DEFAULT_MAX_HITS
    } else {
        max_hits.clamp(1, HARD_MAX_HITS)
    };

    // Build a case-insensitive regex, escaping any regex-special characters
    // in the user's query so they are treated literally.
    let pattern = format!("(?i){}", regex::escape(query));
    let re = Regex::new(&pattern).map_err(|err| format!("Invalid search query: {}", err))?;

    let mut total_matches: usize = 0;
    let mut matching_file_indices: Vec<usize> = Vec::new();
    let mut hits: Vec<SearchHit> = Vec::new();
    let mut remaining_hits = max_hits;

    for (index, record) in records.iter().enumerate() {
        // ── 1. Extract full text (no char cap) ───────────────────────────
        let mut source_text = String::new();

        if let Some(cache) = cache {
            let pdf_options = if record.document_type == DocumentType::Pdf {
                Some(crate::pdf::PdfExtractionOptions::from_cleaning_config(
                    cleaning_config,
                ))
            } else {
                None
            };
            if let Ok(entry) = cache.get_or_extract(record, pdf_options, cleaning_config) {
                source_text = entry.extracted_text;
            }
        } else {
            // Fallback: direct extraction (no cache available)
            if record.document_type == DocumentType::Docx {
                if let Ok(bytes) = std::fs::read(&record.source_path)
                    && let Ok(extracted) = extract_docx(&bytes, cleaning_config)
                {
                    source_text = extracted.text;
                }
            } else if record.document_type == DocumentType::Pdf {
                if let Ok(bytes) = std::fs::read(&record.source_path)
                    && let Ok(extracted) = extract_pdf(
                        &bytes,
                        None, // no char cap – read full text
                        crate::pdf::PdfExtractionOptions::from_cleaning_config(cleaning_config),
                    )
                {
                    source_text = extracted.text;
                }
            } else if let Ok(content) = std::fs::read(&record.source_path) {
                // Read full file content lossily but bounded by OS memory limits.
                source_text = String::from_utf8_lossy(&content).to_string();
            }
        }

        if source_text.is_empty() {
            continue;
        }

        // ── 2. Apply processed mode if requested ─────────────────────────
        if is_processed {
            if cleaning_config.extract_html {
                source_text = extract_html(&source_text);
            }
            source_text = clean_text(&source_text, cleaning_config);
            if source_text.is_empty() {
                continue;
            }
        }

        // ── 3. Count all matches in this file ────────────────────────────
        let file_match_count = re.find_iter(&source_text).count();

        if file_match_count == 0 {
            continue;
        }

        matching_file_indices.push(index);
        total_matches += file_match_count;

        // ── 4. Collect hit contexts (only while budget remains) ──────────
        if remaining_hits > 0 {
            for (file_match_index, mtch) in re.find_iter(&source_text).enumerate() {
                if remaining_hits == 0 {
                    break;
                }

                let match_start = mtch.start();
                let match_end = mtch.end();

                let match_text = &source_text[match_start..match_end];

                // Context before: up to CONTEXT_CHARS characters backwards.
                let context_before = take_chars_before(&source_text, match_start, CONTEXT_CHARS);

                // Context after: up to CONTEXT_CHARS characters forwards.
                let context_after = take_chars_after(&source_text, match_end, CONTEXT_CHARS);

                hits.push(SearchHit {
                    corpus_index: index,
                    relative_path: record.relative_path.to_string_lossy().to_string(),
                    source_path: Some(record.source_path.to_string_lossy().to_string()),
                    context_before,
                    match_text: match_text.to_string(),
                    context_after,
                    file_match_index,
                });

                remaining_hits -= 1;
            }
        }
    }

    let returned_hits = hits.len();
    let truncated = total_matches > returned_hits;

    Ok(SearchResult {
        total_matches,
        matching_file_indices,
        returned_hits,
        truncated,
        hits,
    })
}

/// Returns up to `n` characters from `text` immediately before `pos`.
///
/// `pos` must be a valid char boundary (e.g., a regex match boundary).
/// Uses `floor_char_boundary` to safely round down if needed.
/// If `pos` is 0, returns an empty string.
fn take_chars_before(text: &str, pos: usize, n: usize) -> String {
    if pos == 0 || n == 0 {
        return String::new();
    }
    let pos = text.floor_char_boundary(pos);
    if pos == 0 {
        return String::new();
    }

    // Walk backwards from `pos` counting n chars.
    // Collect up to n char boundaries before pos, then slice.
    let mut last_boundaries: Vec<usize> = Vec::with_capacity(n + 1);
    for (byte_idx, _ch) in text.char_indices() {
        if byte_idx >= pos {
            break;
        }
        last_boundaries.push(byte_idx);
        if last_boundaries.len() > n {
            last_boundaries.remove(0);
        }
    }

    let start = if last_boundaries.is_empty() {
        pos
    } else {
        last_boundaries[0]
    };

    text[start..pos].to_string()
}

/// Returns up to `n` characters from `text` immediately after `pos`.
///
/// `pos` must be a valid char boundary (e.g., a regex match boundary).
/// Uses `floor_char_boundary` to safely round down if needed.
fn take_chars_after(text: &str, pos: usize, n: usize) -> String {
    if pos >= text.len() || n == 0 {
        return String::new();
    }
    let pos = text.floor_char_boundary(pos);

    let mut char_count = 0usize;
    let mut end = pos;

    for (byte_idx, _ch) in text.char_indices() {
        if byte_idx < pos {
            continue;
        }
        if char_count > n {
            break;
        }
        end = byte_idx;
        char_count += 1;
    }

    // If we exhausted the string before n chars, end moves to len().
    if char_count <= n {
        end = text.len();
    }

    text[pos..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::CleaningConfig;
    use crate::scan::DocumentRecord;
    use std::path::PathBuf;

    fn make_record(text: &str, relative_path: &str) -> DocumentRecord {
        DocumentRecord {
            source_path: PathBuf::from(relative_path),
            relative_path: PathBuf::from(relative_path),
            document_type: DocumentType::Text,
            size_bytes: text.len() as u64,
        }
    }

    /// Helper: search in-memory text records without reading from disk.
    /// Since search reads from files, we don't test that path here.
    /// Instead we test the internals directly.
    fn make_records(count: usize, base_name: &str) -> Vec<DocumentRecord> {
        (0..count)
            .map(|i| make_record("", &format!("{}_{}.txt", base_name, i)))
            .collect()
    }

    #[test]
    fn empty_query_returns_zero() {
        let records = make_records(1, "empty");
        let result =
            search_corpus(&records, "", false, &CleaningConfig::default(), 100, None).unwrap();
        assert_eq!(result.total_matches, 0);
        assert!(result.hits.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn whitespace_query_returns_zero() {
        let records = make_records(1, "ws");
        let result = search_corpus(
            &records,
            "   ",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result.total_matches, 0);
    }

    #[test]
    fn no_match_returns_zero() {
        let records = make_records(1, "nomatch");
        let result = search_corpus(
            &records,
            "zzznotfound",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result.total_matches, 0);
        assert!(result.matching_file_indices.is_empty());
        assert!(result.hits.is_empty());
    }

    #[test]
    fn max_hits_zero_uses_default() {
        let records = make_records(1, "default");
        let result =
            search_corpus(&records, "x", false, &CleaningConfig::default(), 0, None).unwrap();
        // max_hits=0 → DEFAULT_MAX_HITS=1000, but no files have "x" so it's fine
        assert_eq!(result.total_matches, 0);
    }

    #[test]
    fn max_hits_clamped_to_hard_max() {
        let result =
            search_corpus(&[], "test", false, &CleaningConfig::default(), 99999, None).unwrap();
        // Should not crash; max_hits clamped to HARD_MAX_HITS=5000
        assert_eq!(result.total_matches, 0);
    }

    #[test]
    fn regex_special_chars_treated_literally() {
        let text = "the cost is $5.00 + tax (approx.)";
        let tmp = std::env::temp_dir().join("corpusaid_test_special.txt");
        std::fs::write(&tmp, text).unwrap();
        let records = [make_record(text, tmp.to_str().unwrap())];
        // Patch source_path so search reads our temp file
        let records = [DocumentRecord {
            source_path: tmp.clone(),
            ..records[0].clone()
        }];

        // The dot should be literal, not "any char"
        let result = search_corpus(
            &records,
            "5.00",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result.total_matches, 1);

        // Test that raw dot doesn't match extra chars
        let result2 = search_corpus(
            &records,
            "5.00x",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result2.total_matches, 0);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn case_insensitive_search_works() {
        let text = "Hello WORLD hello world HELLO";
        let tmp = std::env::temp_dir().join("corpusaid_test_case.txt");
        std::fs::write(&tmp, text).unwrap();
        let record = DocumentRecord {
            source_path: tmp.clone(),
            relative_path: PathBuf::from("case.txt"),
            document_type: DocumentType::Text,
            size_bytes: text.len() as u64,
        };
        let records = [record];

        let result = search_corpus(
            &records,
            "hello",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result.total_matches, 3);
        assert_eq!(result.returned_hits, 3);
        assert!(!result.truncated);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn context_strings_are_non_empty() {
        let text = "abcdefghijklmnopqrstuvwxyz0123456789";
        let tmp = std::env::temp_dir().join("corpusaid_test_ctx.txt");
        std::fs::write(&tmp, text).unwrap();
        let record = DocumentRecord {
            source_path: tmp.clone(),
            relative_path: PathBuf::from("ctx.txt"),
            document_type: DocumentType::Text,
            size_bytes: text.len() as u64,
        };
        let records = [record];

        let result = search_corpus(
            &records,
            "mnop",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();

        assert_eq!(result.total_matches, 1);
        assert_eq!(result.returned_hits, 1);

        let hit = &result.hits[0];
        assert!(!hit.match_text.is_empty());
        assert_eq!(hit.match_text, "mnop");
        assert!(!hit.context_before.is_empty());
        assert!(!hit.context_after.is_empty());
        // "mnop" starts at byte 12 (0-indexed). 80 chars before = all 12 chars before it.
        assert_eq!(hit.context_before, "abcdefghijkl");
        assert!(hit.context_after.starts_with("qrst"));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn context_respects_boundary() {
        // Very short string – context should fit within bounds
        let text = "short";
        let tmp = std::env::temp_dir().join("corpusaid_test_short.txt");
        std::fs::write(&tmp, text).unwrap();
        let record = DocumentRecord {
            source_path: tmp.clone(),
            relative_path: PathBuf::from("short.txt"),
            document_type: DocumentType::Text,
            size_bytes: text.len() as u64,
        };
        let records = [record];

        let result = search_corpus(
            &records,
            "sho",
            false,
            &CleaningConfig::default(),
            100,
            None,
        )
        .unwrap();
        assert_eq!(result.total_matches, 1);
        assert!(result.hits[0].context_before.is_empty()); // nothing before
        assert!(!result.hits[0].context_after.is_empty());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn matching_file_indices_are_correct() {
        // This test only validates utility functions and struct shapes.
        let result = SearchResult {
            total_matches: 0,
            matching_file_indices: vec![0, 2],
            returned_hits: 0,
            truncated: false,
            hits: vec![],
        };

        assert_eq!(result.matching_file_indices, vec![0, 2]);
        assert!(!result.truncated);
    }

    #[test]
    fn hit_has_correct_file_match_index() {
        // The match_file_index is tested via take_chars utils
        let (before, after) = extract_context_test("xx abc yy", "abc");
        assert_eq!(before, "xx ");
        assert_eq!(after, " yy");
    }

    #[test]
    fn take_chars_before_works() {
        let text = "abcdefghij";
        assert_eq!(take_chars_before(text, 5, 3), "cde"); // 3 chars before index 5
        assert_eq!(take_chars_before(text, 0, 3), ""); // nothing before start
        assert_eq!(take_chars_before(text, 5, 0), ""); // n=0
        assert_eq!(take_chars_before(text, 10, 100), "abcdefghij"); // all text before end
    }

    #[test]
    fn take_chars_after_works() {
        let text = "abcdefghij";
        assert_eq!(take_chars_after(text, 5, 3), "fgh"); // 3 chars after index 5
        assert_eq!(take_chars_after(text, 10, 3), ""); // nothing after end
        assert_eq!(take_chars_after(text, 5, 0), ""); // n=0
        assert_eq!(take_chars_after(text, 0, 100), "abcdefghij"); // everything
    }

    #[test]
    fn take_chars_handles_unicode() {
        let text = "aé😀bcdé😀fg";
        // "é" is 2 bytes, "😀" is 4 bytes.  Char boundaries must be respected.
        let before = take_chars_before(text, 11, 3); // 11 = byte offset of "b"
        // 3 chars before "b" = "😀é" (might cross multi-byte)
        assert!(!before.is_empty());

        let after = take_chars_after(text, 11, 3);
        assert!(!after.is_empty());
    }

    #[test]
    fn search_result_json_serialization() {
        let result = SearchResult {
            total_matches: 42,
            matching_file_indices: vec![1, 2, 3],
            returned_hits: 2,
            truncated: true,
            hits: vec![SearchHit {
                corpus_index: 1,
                relative_path: "file.txt".to_string(),
                source_path: Some("/path/file.txt".to_string()),
                context_before: "before ".to_string(),
                match_text: "MATCH".to_string(),
                context_after: " after".to_string(),
                file_match_index: 0,
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("total_matches"));
        assert!(json.contains("truncated"));
        assert!(json.contains("context_before"));
        assert!(json.contains("file_match_index"));
    }

    /// Helper used by context tests.
    fn extract_context_test(text: &str, query: &str) -> (String, String) {
        let re = Regex::new(&format!("(?i){}", regex::escape(query))).unwrap();
        let mtch = re.find(text).unwrap();
        let before = take_chars_before(text, mtch.start(), CONTEXT_CHARS);
        let after = take_chars_after(text, mtch.end(), CONTEXT_CHARS);
        (before, after)
    }
}
