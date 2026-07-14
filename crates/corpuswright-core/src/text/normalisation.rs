use regex::Regex;
use std::sync::LazyLock;

static RE_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Normalises a line for repeated-artefact family matching.
pub(crate) fn normalize_line_for_repeated_artifact(s: &str) -> String {
    let trimmed = s.trim();
    let chars: Vec<char> = trimmed.chars().collect();

    let mut i = 0;
    while i < chars.len() && is_repeated_artifact_boundary_punctuation(chars[i]) {
        i += 1;
    }
    let start = i;

    let mut j = chars.len();
    while j > start && is_repeated_artifact_boundary_punctuation(chars[j - 1]) {
        j -= 1;
    }
    let end = j;

    if start >= end {
        return String::new();
    }

    let substring: String = chars[start..end].iter().collect();
    let trimmed_sub = substring.trim();
    let lower = trimmed_sub.to_lowercase();

    let with_replaced_digits = RE_DIGITS.replace_all(&lower, "#");
    let collapsed_spaces = RE_SPACES.replace_all(&with_replaced_digits, " ");

    collapsed_spaces.trim().to_string()
}

fn is_repeated_artifact_boundary_punctuation(c: char) -> bool {
    matches!(
        c,
        '-' | '—'
            | '–'
            | '['
            | ']'
            | '('
            | ')'
            | '{'
            | '}'
            | '.'
            | ','
            | '*'
            | '/'
            | '\\'
            | '_'
            | '|'
            | '•'
            | '°'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_artifact_line_normalisation_matches_scanner_examples() {
        assert_eq!(
            normalize_line_for_repeated_artifact("--- Page 12 ---"),
            "page #"
        );
        assert_eq!(
            normalize_line_for_repeated_artifact("Chapter 1: Intro"),
            "chapter #: intro"
        );
        assert_eq!(
            normalize_line_for_repeated_artifact("   Some    spaces   "),
            "some spaces"
        );
        assert_eq!(
            normalize_line_for_repeated_artifact("  --- test 123 --- "),
            "test #"
        );
        assert_eq!(
            normalize_line_for_repeated_artifact("Another Line"),
            "another line"
        );
        assert_eq!(
            normalize_line_for_repeated_artifact("Another [Line]"),
            "another [line"
        );
    }
}
