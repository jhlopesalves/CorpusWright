use serde::{Deserialize, Serialize};
use std::io::Read;

/// One text row from Project Gutenberg's official weekly CSV catalogue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GutenbergCatalogRecord {
    pub ebook_id: u64,
    pub issued: String,
    pub title: String,
    pub language: String,
    pub authors: String,
    pub subjects: String,
    pub locc: String,
    pub bookshelves: String,
}

#[derive(Debug, Deserialize)]
struct RawCatalogRecord {
    #[serde(rename = "Text#")]
    ebook_id: u64,
    #[serde(rename = "Type")]
    record_type: String,
    #[serde(rename = "Issued")]
    issued: String,
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Language")]
    language: String,
    #[serde(rename = "Authors")]
    authors: String,
    #[serde(rename = "Subjects")]
    subjects: String,
    #[serde(rename = "LoCC")]
    locc: String,
    #[serde(rename = "Bookshelves")]
    bookshelves: String,
}

/// Parses only text records while preserving catalogue order.
pub fn parse_gutenberg_catalog(
    reader: impl Read,
) -> Result<Vec<GutenbergCatalogRecord>, csv::Error> {
    let mut parsed = Vec::new();
    for row in csv::Reader::from_reader(reader).deserialize::<RawCatalogRecord>() {
        let row = row?;
        if row.record_type != "Text" {
            continue;
        }
        parsed.push(GutenbergCatalogRecord {
            ebook_id: row.ebook_id,
            issued: row.issued,
            title: row.title,
            language: row.language,
            authors: row.authors,
            subjects: row.subjects,
            locc: row.locc,
            bookshelves: row.bookshelves,
        });
    }
    Ok(parsed)
}

/// Editorial inputs that cannot be inferred safely from catalogue metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScoringInput {
    pub estimated_tokens: u64,
    pub profile_compatibility: u8,
    pub analytical_contribution: u8,
    pub genre_balance: u8,
    pub author_is_new: bool,
    pub work_is_duplicate: bool,
    pub plain_text_available: bool,
}

/// Stable score components; the total is their signed sum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScore {
    pub profile_compatibility: i16,
    pub author_diversification: i16,
    pub useful_length: i16,
    pub analytical_contribution: i16,
    pub genre_balance: i16,
    pub provenance_completeness: i16,
    pub duplicate_penalty: i16,
    pub total: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CandidateEvaluation {
    pub ebook_id: u64,
    pub score: CandidateScore,
}

pub fn score_candidate(
    record: &GutenbergCatalogRecord,
    input: &CandidateScoringInput,
) -> CandidateEvaluation {
    let useful_length = match input.estimated_tokens {
        35_000..=100_000 => 20,
        25_000..=140_000 => 14,
        15_000..=180_000 => 7,
        _ => 0,
    };
    let profile_compatibility = i16::from(input.profile_compatibility.min(30));
    let author_diversification = if input.author_is_new { 20 } else { 0 };
    let analytical_contribution = i16::from(input.analytical_contribution.min(15));
    let genre_balance = i16::from(input.genre_balance.min(10));
    let metadata_complete = !record.title.trim().is_empty()
        && !record.authors.trim().is_empty()
        && record.language.split(';').any(|value| value.trim() == "en");
    let provenance_completeness = if metadata_complete && input.plain_text_available {
        5
    } else {
        0
    };
    let duplicate_penalty = if input.work_is_duplicate { -100 } else { 0 };
    let total = profile_compatibility
        + author_diversification
        + useful_length
        + analytical_contribution
        + genre_balance
        + provenance_completeness
        + duplicate_penalty;
    CandidateEvaluation {
        ebook_id: record.ebook_id,
        score: CandidateScore {
            profile_compatibility,
            author_diversification,
            useful_length,
            analytical_contribution,
            genre_balance,
            provenance_completeness,
            duplicate_penalty,
            total,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_parser_handles_quoted_multiline_fields_and_skips_sound() {
        let source = concat!(
            "Text#,Type,Issued,Title,Language,Authors,Subjects,LoCC,Bookshelves\n",
            "42,Text,2025-01-01,\"A title\nwith a subtitle\",en,\"Doe, Jane\",Fiction,PR,Novels\n",
            "43,Sound,2025-01-02,Audio,en,\"Doe, Jane\",Fiction,PR,Audio\n",
        );
        let records = parse_gutenberg_catalog(source.as_bytes()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].ebook_id, 42);
        assert_eq!(records[0].title, "A title\nwith a subtitle");
    }

    #[test]
    fn candidate_scoring_is_deterministic_and_penalises_duplicates() {
        let record = GutenbergCatalogRecord {
            ebook_id: 42,
            issued: "2025-01-01".into(),
            title: "A title".into(),
            language: "en".into(),
            authors: "Doe, Jane".into(),
            subjects: "Fiction".into(),
            locc: "PR".into(),
            bookshelves: "Novels".into(),
        };
        let input = CandidateScoringInput {
            estimated_tokens: 50_000,
            profile_compatibility: 30,
            analytical_contribution: 15,
            genre_balance: 10,
            author_is_new: true,
            work_is_duplicate: false,
            plain_text_available: true,
        };
        let first = score_candidate(&record, &input);
        assert_eq!(first, score_candidate(&record, &input));
        assert_eq!(first.score.total, 100);
        let duplicate = score_candidate(
            &record,
            &CandidateScoringInput {
                work_is_duplicate: true,
                ..input
            },
        );
        assert_eq!(duplicate.score.total, 0);
    }
}
