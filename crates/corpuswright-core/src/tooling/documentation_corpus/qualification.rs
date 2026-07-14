use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

static PAGE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\s*(?:page\s+)?(?:\d{1,5}|[ivxlcdm]{1,12})\s*$").unwrap());

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalPolicy {
    pub minimum_request_interval_ms: u64,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            minimum_request_interval_ms: 2_000,
        }
    }
}

impl RetrievalPolicy {
    pub fn validate(self) -> Result<Self, String> {
        if self.minimum_request_interval_ms < 2_000 {
            return Err(
                "Project Gutenberg retrieval must wait at least 2,000 ms between ebook requests."
                    .into(),
            );
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SeedTechnicalAudit {
    pub bytes: u64,
    pub sha256: String,
    pub utf8_valid: bool,
    pub bom: bool,
    pub nul_bytes: usize,
    pub crlf_count: usize,
    pub lone_cr_count: usize,
    pub replacement_character_count: usize,
    pub page_number_line_count: usize,
    pub repeated_long_line_groups: usize,
    pub start_marker_count: usize,
    pub end_marker_count: usize,
}

pub fn technical_audit(raw: &[u8]) -> SeedTechnicalAudit {
    let utf8 = std::str::from_utf8(raw).ok();
    let text = utf8.unwrap_or_default();
    let mut line_counts = HashMap::<&str, usize>::new();
    for line in text.lines().map(str::trim).filter(|line| line.len() >= 40) {
        *line_counts.entry(line).or_default() += 1;
    }
    let folded = text.to_ascii_lowercase();
    SeedTechnicalAudit {
        bytes: raw.len() as u64,
        sha256: sha256_hex(raw),
        utf8_valid: utf8.is_some(),
        bom: raw.starts_with(&[0xef, 0xbb, 0xbf]),
        nul_bytes: raw.iter().filter(|byte| **byte == 0).count(),
        crlf_count: raw.windows(2).filter(|pair| *pair == b"\r\n").count(),
        lone_cr_count: raw
            .iter()
            .enumerate()
            .filter(|(index, byte)| **byte == b'\r' && raw.get(index + 1).copied() != Some(b'\n'))
            .count(),
        replacement_character_count: text.matches('\u{fffd}').count(),
        page_number_line_count: text
            .lines()
            .filter(|line| PAGE_NUMBER.is_match(line))
            .count(),
        repeated_long_line_groups: line_counts.values().filter(|count| **count >= 3).count(),
        start_marker_count: folded
            .matches("start of the project gutenberg ebook")
            .count()
            + folded
                .matches("start of this project gutenberg ebook")
                .count(),
        end_marker_count: folded.matches("end of the project gutenberg ebook").count()
            + folded
                .matches("end of this project gutenberg ebook")
                .count(),
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateEvidence {
    pub sha256: String,
    pub paths: Vec<String>,
}

pub fn detect_duplicate_content<'a>(
    documents: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Vec<DuplicateEvidence> {
    let mut by_hash = BTreeMap::<String, Vec<String>>::new();
    for (path, bytes) in documents {
        by_hash
            .entry(sha256_hex(bytes))
            .or_default()
            .push(path.to_string());
    }
    by_hash
        .into_iter()
        .filter_map(|(sha256, mut paths)| {
            if paths.len() < 2 {
                return None;
            }
            paths.sort();
            Some(DuplicateEvidence { sha256, paths })
        })
        .collect()
}

/// Returns stable corpus identity from ordered path/checksum pairs and analysis configuration.
///
/// Callers must supply strictly ordered paths. Length boundaries use zero bytes, and the domain
/// prefix keeps this digest separate from individual document checksums.
pub fn corpus_checksum<'a>(
    ordered_documents: impl IntoIterator<Item = (&'a str, &'a str)>,
    analysis_configuration: &[u8],
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"corpuswright-documentation-corpus-v1\0");
    for (path, checksum) in ordered_documents {
        hash.update(path.as_bytes());
        hash.update([0]);
        hash.update(checksum.as_bytes());
        hash.update([0]);
    }
    hash.update(analysis_configuration);
    format!("{:x}", hash.finalize())
}

pub fn is_publishable_status(status: &str) -> bool {
    matches!(status, "ACCEPTED" | "ACCEPTED WITH NOTE")
}

pub fn manifest_paths_are_strictly_ordered<'a>(paths: impl IntoIterator<Item = &'a str>) -> bool {
    let mut previous = None::<&str>;
    for path in paths {
        if previous.is_some_and(|value| value >= path) {
            return false;
        }
        previous = Some(path);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_cannot_be_configured_below_two_seconds() {
        assert!(RetrievalPolicy::default().validate().is_ok());
        assert!(
            RetrievalPolicy {
                minimum_request_interval_ms: 1_999,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn checksum_and_audit_use_raw_bytes() {
        let raw = b"\xef\xbb\xbfOne\r\nTwo\0";
        let audit = technical_audit(raw);
        assert_eq!(audit.sha256, sha256_hex(raw));
        assert!(audit.bom);
        assert_eq!(audit.crlf_count, 1);
        assert_eq!(audit.nul_bytes, 1);
    }

    #[test]
    fn duplicate_detection_is_ordered_and_exact() {
        let duplicates = detect_duplicate_content([
            ("z.txt", b"same".as_slice()),
            ("a.txt", b"same".as_slice()),
            ("other.txt", b"different".as_slice()),
        ]);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].paths, ["a.txt", "z.txt"]);
    }

    #[test]
    fn corpus_checksum_is_stable_but_order_sensitive() {
        let documents = [("a.txt", "aaa"), ("b.txt", "bbb")];
        let first = corpus_checksum(documents, b"config");
        assert_eq!(first, corpus_checksum(documents, b"config"));
        assert_ne!(
            first,
            corpus_checksum([("b.txt", "bbb"), ("a.txt", "aaa")], b"config")
        );
        assert_ne!(first, corpus_checksum(documents, b"other"));
    }

    #[test]
    fn only_explicitly_accepted_statuses_are_publishable() {
        assert!(is_publishable_status("ACCEPTED"));
        assert!(is_publishable_status("ACCEPTED WITH NOTE"));
        for excluded in [
            "PROVENANCE UNRESOLVED",
            "REDISTRIBUTION BLOCKED",
            "CLEANING REVIEW REQUIRED",
            "TECHNICALLY INVALID",
            "DUPLICATE",
            "REJECTED BY SELECTION POLICY",
        ] {
            assert!(!is_publishable_status(excluded), "{excluded}");
        }
    }

    #[test]
    fn manifest_paths_must_be_strictly_ordered() {
        assert!(manifest_paths_are_strictly_ordered([
            "texts/a.txt",
            "texts/b.txt",
            "texts/c.txt",
        ]));
        assert!(!manifest_paths_are_strictly_ordered([
            "texts/b.txt",
            "texts/a.txt",
        ]));
        assert!(!manifest_paths_are_strictly_ordered([
            "texts/a.txt",
            "texts/a.txt",
        ]));
    }
}
