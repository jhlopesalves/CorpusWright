use crate::text_normalization::normalize_line_for_repeated_artifact as normalize_line;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::LazyLock;
use ts_rs::TS;

static COMMON_SECTION_HEADING_KEYS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    normalized_key_set(&[
        "abstract",
        "introduction",
        "method",
        "methods",
        "methodology",
        "materials and methods",
        "results",
        "discussion",
        "conclusion",
        "conclusions",
        "references",
        "bibliography",
        "appendix",
        "appendices",
        "acknowledgements",
        "acknowledgments",
        "resumo",
        "introdução",
        "introducao",
        "método",
        "metodo",
        "métodos",
        "metodos",
        "metodologia",
        "materiais e métodos",
        "materiais e metodos",
        "resultados",
        "discussão",
        "discussao",
        "conclusão",
        "conclusao",
        "conclusões",
        "conclusoes",
        "referências",
        "referencias",
        "bibliografia",
        "apêndice",
        "apendice",
        "apêndices",
        "apendices",
        "anexo",
        "anexos",
        "agradecimentos",
        "résumé",
        "resume",
        "méthode",
        "methode",
        "méthodes",
        "methodes",
        "méthodologie",
        "methodologie",
        "matériels et méthodes",
        "materiels et methodes",
        "résultats",
        "resultats",
        "références",
        "bibliographie",
        "remerciements",
        "annexe",
        "annexes",
    ])
});

static PAGE_LABEL_KEYS: LazyLock<BTreeSet<String>> = LazyLock::new(|| {
    normalized_key_set(&[
        "page 1",
        "page 1 of 2",
        "p 1",
        "p. 1",
        "pag 1",
        "pag. 1",
        "pág 1",
        "pág. 1",
        "pagina 1",
        "página 1",
        "pagina 1 de 2",
        "página 1 de 2",
    ])
});

static RE_DECIMAL_DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Nd}$").unwrap());

/// Known inline markup/conversion patterns.
pub(crate) const KNOWN_INLINE_PATTERNS: &[&str] = &[
    "<br/>", "<br>", "<br />", "<BR/>", "<BR>", "<BR />", "&nbsp;", "&amp;", "&lt;", "&gt;",
    "&quot;", "&apos;",
];

/// Advisory character and token profile for a local text line or candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct CandidateTextProfile {
    pub char_count: usize,
    pub token_count: usize,
    pub alphabetic_ratio: f64,
    pub digit_ratio: f64,
    pub symbol_ratio: f64,
    pub whitespace_ratio: f64,
    pub uppercase_ratio: f64,
    pub average_token_length: f64,
    pub max_repeated_char_run: usize,
    pub suspicious_token_ratio: f64,
    pub has_sentence_punctuation: bool,
    pub looks_like_common_section_heading: bool,
    pub looks_like_page_label: bool,
    pub looks_like_table_or_numeric_row: bool,
    pub looks_like_formula_or_code: bool,
    pub looks_like_markup_or_extraction_noise: bool,
}

impl Default for CandidateTextProfile {
    fn default() -> Self {
        Self {
            char_count: 0,
            token_count: 0,
            alphabetic_ratio: 0.0,
            digit_ratio: 0.0,
            symbol_ratio: 0.0,
            whitespace_ratio: 0.0,
            uppercase_ratio: 0.0,
            average_token_length: 0.0,
            max_repeated_char_run: 0,
            suspicious_token_ratio: 0.0,
            has_sentence_punctuation: false,
            looks_like_common_section_heading: false,
            looks_like_page_label: false,
            looks_like_table_or_numeric_row: false,
            looks_like_formula_or_code: false,
            looks_like_markup_or_extraction_noise: false,
        }
    }
}

impl Eq for CandidateTextProfile {}

/// Advisory text/noise label for candidate review.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CandidateTextSignalLabel {
    LikelyNaturalText,
    LikelySectionHeading,
    LikelyPageLabel,
    LikelyTableOrNumericRow,
    LikelyFormulaOrCode,
    LikelyMarkupOrExtractionNoise,
    #[default]
    Ambiguous,
}

/// Scanner-specific evidence used to turn a local text profile into an advisory label.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextSignalContext {
    pub content_is_text_dominant: bool,
    pub has_common_section_heading_risk: bool,
    pub has_symbol_or_noise_risk: bool,
    pub has_strong_header_footer_risk: bool,
    pub page_edge_ratio: f64,
}

/// Profiles a single line or repeated-artefact candidate using deterministic local signals.
pub fn profile_text_line(display_text: &str, normalized_key: &str) -> CandidateTextProfile {
    let trimmed = display_text.trim();
    let chars = trimmed.chars().collect::<Vec<_>>();
    let char_count = chars.len();
    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let token_count = tokens.len();

    if char_count == 0 {
        return CandidateTextProfile::default();
    }

    let alphabetic_count = chars.iter().filter(|c| c.is_alphabetic()).count();
    let digit_count = chars.iter().filter(|c| is_decimal_digit(**c)).count();
    let whitespace_count = chars.iter().filter(|c| c.is_whitespace()).count();
    let symbol_count = chars
        .iter()
        .filter(|c| !c.is_alphabetic() && !is_decimal_digit(**c) && !c.is_whitespace())
        .count();
    let uppercase_count = chars.iter().filter(|c| c.is_uppercase()).count();
    let token_char_count = tokens
        .iter()
        .map(|token| token.chars().count())
        .sum::<usize>();
    let suspicious_token_count = tokens
        .iter()
        .filter(|token| is_suspicious_token(token))
        .count();

    let char_total = char_count as f64;
    let normalized = if normalized_key.trim().is_empty() {
        normalize_line(trimmed)
    } else {
        normalized_key.trim().to_string()
    };

    let mut profile = CandidateTextProfile {
        char_count,
        token_count,
        alphabetic_ratio: alphabetic_count as f64 / char_total,
        digit_ratio: digit_count as f64 / char_total,
        symbol_ratio: symbol_count as f64 / char_total,
        whitespace_ratio: whitespace_count as f64 / char_total,
        uppercase_ratio: if alphabetic_count > 0 {
            uppercase_count as f64 / alphabetic_count as f64
        } else {
            0.0
        },
        average_token_length: if token_count > 0 {
            token_char_count as f64 / token_count as f64
        } else {
            0.0
        },
        max_repeated_char_run: max_repeated_non_whitespace_char_run(trimmed),
        suspicious_token_ratio: if token_count > 0 {
            suspicious_token_count as f64 / token_count as f64
        } else {
            0.0
        },
        has_sentence_punctuation: chars
            .iter()
            .any(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？')),
        looks_like_common_section_heading: is_common_section_heading(&normalized),
        looks_like_page_label: is_page_label_like(&normalized),
        looks_like_table_or_numeric_row: false,
        looks_like_formula_or_code: false,
        looks_like_markup_or_extraction_noise: false,
    };

    profile.looks_like_table_or_numeric_row = looks_like_table_or_numeric_row(trimmed, &profile);
    profile.looks_like_formula_or_code = looks_like_formula_or_code(trimmed, &profile);
    profile.looks_like_markup_or_extraction_noise =
        looks_like_markup_or_extraction_noise(trimmed, &profile);

    profile
}

/// Classifies a profile into the advisory text/noise label shown during candidate review.
pub fn classify_text_signal(
    profile: &CandidateTextProfile,
    context: TextSignalContext,
) -> CandidateTextSignalLabel {
    if profile.looks_like_common_section_heading || context.has_common_section_heading_risk {
        return CandidateTextSignalLabel::LikelySectionHeading;
    }

    if profile.looks_like_page_label {
        return CandidateTextSignalLabel::LikelyPageLabel;
    }

    if profile.looks_like_markup_or_extraction_noise
        || (context.has_symbol_or_noise_risk
            && profile.symbol_ratio >= 0.50
            && profile.alphabetic_ratio < 0.30)
    {
        return CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise;
    }

    if profile.looks_like_table_or_numeric_row {
        return CandidateTextSignalLabel::LikelyTableOrNumericRow;
    }

    if profile.looks_like_formula_or_code {
        return CandidateTextSignalLabel::LikelyFormulaOrCode;
    }

    if context.content_is_text_dominant
        && profile.alphabetic_ratio >= 0.65
        && profile.symbol_ratio <= 0.15
        && profile.token_count >= 4
        && profile.average_token_length >= 3.0
        && context.page_edge_ratio < 0.75
        && !context.has_strong_header_footer_risk
    {
        return CandidateTextSignalLabel::LikelyNaturalText;
    }

    CandidateTextSignalLabel::Ambiguous
}

/// Stable reason codes explaining the advisory text signal.
pub fn text_signal_reasons(
    display_text: &str,
    profile: &CandidateTextProfile,
    label: CandidateTextSignalLabel,
    page_edge_ratio: f64,
) -> Vec<String> {
    let mut reasons = Vec::new();

    match label {
        CandidateTextSignalLabel::LikelySectionHeading => {
            push_reason(&mut reasons, "common_section_heading");
        }
        CandidateTextSignalLabel::LikelyPageLabel => {
            push_reason(&mut reasons, "page_label_pattern");
        }
        CandidateTextSignalLabel::LikelyTableOrNumericRow => {
            push_reason(&mut reasons, "table_or_numeric_row");
        }
        CandidateTextSignalLabel::LikelyFormulaOrCode => {
            push_reason(&mut reasons, "formula_or_code_symbols");
        }
        CandidateTextSignalLabel::LikelyMarkupOrExtractionNoise => {
            if is_markup_like_token(display_text) {
                push_reason(&mut reasons, "markup_entity_or_tag");
            }
            if is_cid_like_marker(display_text) {
                push_reason(&mut reasons, "cid_marker");
            }
            if display_text.contains('\u{fffd}') {
                push_reason(&mut reasons, "replacement_character_junk");
            }
            if profile.symbol_ratio >= 0.50 {
                push_reason(&mut reasons, "high_symbol_ratio");
            }
        }
        CandidateTextSignalLabel::LikelyNaturalText => {
            push_reason(&mut reasons, "mostly_alphabetic");
            push_reason(&mut reasons, "multi_token_text");
        }
        CandidateTextSignalLabel::Ambiguous => {}
    }

    if profile.digit_ratio >= 0.35 {
        push_reason(&mut reasons, "high_digit_ratio");
    }
    if profile.max_repeated_char_run >= 4 {
        push_reason(&mut reasons, "long_repeated_character_run");
    }
    if page_edge_ratio >= 0.75 {
        push_reason(&mut reasons, "page_edge_repetition");
    }

    reasons
}

/// Returns true only for standalone lines with overwhelming extraction/OCR/markup noise signals.
pub fn is_obvious_extraction_noise(profile: &CandidateTextProfile, reasons: &[String]) -> bool {
    if profile.char_count == 0 {
        return false;
    }

    let has_cid_marker = has_reason(reasons, "cid_marker");
    let has_markup_entity_or_tag = has_reason(reasons, "markup_entity_or_tag");
    if has_cid_marker
        || (has_markup_entity_or_tag
            && profile.char_count <= 12
            && profile.token_count <= 3
            && profile.symbol_ratio >= 0.25)
    {
        return true;
    }

    if profile.looks_like_common_section_heading
        || profile.looks_like_page_label
        || profile.looks_like_table_or_numeric_row
        || profile.looks_like_formula_or_code
    {
        return false;
    }

    if profile.alphabetic_ratio >= 0.50 {
        return false;
    }

    if profile.token_count >= 2
        && profile.alphabetic_ratio >= 0.35
        && profile.average_token_length >= 2.0
        && profile.suspicious_token_ratio < 0.50
    {
        return false;
    }

    if profile.has_sentence_punctuation && profile.alphabetic_ratio >= 0.25 {
        return false;
    }

    let near_zero_alphabetic = profile.alphabetic_ratio <= 0.10;
    let has_replacement_junk = has_reason(reasons, "replacement_character_junk")
        && near_zero_alphabetic
        && profile.symbol_ratio >= 0.50;
    let repeated_symbol_junk = has_reason(reasons, "long_repeated_character_run")
        && near_zero_alphabetic
        && profile.max_repeated_char_run >= 4
        && profile.symbol_ratio >= 0.50;
    let very_high_symbol_ratio =
        profile.char_count >= 4 && near_zero_alphabetic && profile.symbol_ratio >= 0.85;
    let suspicious_symbol_tokens = profile.char_count >= 4
        && profile.token_count <= 3
        && near_zero_alphabetic
        && profile.suspicious_token_ratio >= 0.75
        && profile.symbol_ratio >= 0.50;

    has_replacement_junk
        || repeated_symbol_junk
        || very_high_symbol_ratio
        || suspicious_symbol_tokens
}

pub(crate) fn is_decimal_digit(c: char) -> bool {
    if c.is_ascii_digit() {
        return true;
    }

    let mut buf = [0; 4];
    RE_DECIMAL_DIGIT.is_match(c.encode_utf8(&mut buf))
}

fn normalized_key_set(entries: &[&str]) -> BTreeSet<String> {
    entries
        .iter()
        .map(|entry| normalize_line(entry))
        .filter(|key| !key.is_empty())
        .collect()
}

pub(crate) fn is_common_section_heading(normalized_key: &str) -> bool {
    COMMON_SECTION_HEADING_KEYS.contains(normalized_key.trim())
}

pub(crate) fn is_page_label_like(normalized_key: &str) -> bool {
    PAGE_LABEL_KEYS.contains(normalized_key.trim())
}

fn push_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|existing| existing == reason) {
        reasons.push(reason.to_string());
    }
}

fn has_reason(reasons: &[String], reason: &str) -> bool {
    reasons.iter().any(|existing| existing == reason)
}

fn max_repeated_non_whitespace_char_run(text: &str) -> usize {
    let mut previous = None;
    let mut current_run = 0;
    let mut max_run = 0;

    for character in text.chars().filter(|c| !c.is_whitespace()) {
        if Some(character) == previous {
            current_run += 1;
        } else {
            previous = Some(character);
            current_run = 1;
        }
        max_run = max_run.max(current_run);
    }

    max_run
}

fn is_suspicious_token(token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return false;
    }

    if is_markup_like_token(trimmed) || is_cid_like_marker(trimmed) || trimmed.contains('\u{fffd}')
    {
        return true;
    }

    let char_count = trimmed.chars().count();
    let alphabetic_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    let digit_count = trimmed.chars().filter(|c| is_decimal_digit(*c)).count();
    let symbol_count = trimmed
        .chars()
        .filter(|c| !c.is_alphabetic() && !is_decimal_digit(*c) && !c.is_whitespace())
        .count();

    (char_count >= 3 && symbol_count * 2 >= char_count)
        || (char_count >= 4 && max_repeated_non_whitespace_char_run(trimmed) >= 4)
        || (alphabetic_count == 0 && digit_count + symbol_count > 0)
}

fn looks_like_table_or_numeric_row(text: &str, profile: &CandidateTextProfile) -> bool {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return false;
    }

    let numeric_like_count = tokens
        .iter()
        .filter(|token| is_numeric_like_token(token))
        .count();
    let digit_token_count = tokens
        .iter()
        .filter(|token| token.chars().any(is_decimal_digit))
        .count();
    let statistical_pattern = has_statistical_pattern(text);
    let column_separator_count = count_column_like_separators(text);
    let table_header = tokens.len() >= 3
        && tokens
            .iter()
            .all(|token| is_table_header_token(&strip_token_for_matching(token)));

    table_header
        || (statistical_pattern && digit_token_count > 0 && tokens.len() >= 4)
        || (numeric_like_count >= 3 && numeric_like_count * 2 >= tokens.len())
        || (profile.digit_ratio >= 0.45 && profile.alphabetic_ratio < 0.35 && tokens.len() >= 3)
        || (column_separator_count >= 2 && numeric_like_count >= 2)
}

fn is_numeric_like_token(token: &str) -> bool {
    let trimmed =
        token.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'));
    let has_digit = trimmed.chars().any(is_decimal_digit);
    has_digit
        && trimmed.chars().all(|c| {
            is_decimal_digit(c) || matches!(c, '.' | ',' | '%' | '+' | '-' | '/' | ':' | '٫' | '٬')
        })
}

fn has_statistical_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("χ²") || lower.contains("chi2") {
        return true;
    }

    let tokens = lower.split_whitespace().collect::<Vec<_>>();
    for (idx, token) in tokens.iter().enumerate() {
        let trimmed = token.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '[' | ']'));
        if matches!(
            trimmed,
            "p<" | "p=" | "f=" | "t=" | "z=" | "r=" | "p≤" | "p>="
        ) || trimmed.starts_with("p<")
            || trimmed.starts_with("p=")
            || trimmed.starts_with("f=")
            || trimmed.starts_with("t=")
            || trimmed.starts_with("z=")
            || trimmed.starts_with("r=")
        {
            return true;
        }

        let variable = strip_token_for_matching(trimmed);
        if matches!(variable.as_str(), "p" | "f" | "t" | "z" | "r")
            && let Some(next) = tokens.get(idx + 1)
        {
            let op = next.trim();
            if op.starts_with('=') || op.starts_with('<') || op.starts_with('≤') {
                return true;
            }
        }
    }

    false
}

fn count_column_like_separators(text: &str) -> usize {
    let mut count = 0;
    let mut space_run = 0;

    for character in text.chars() {
        if character == '\t' {
            count += 1;
            space_run = 0;
        } else if character == ' ' {
            space_run += 1;
            if space_run == 2 {
                count += 1;
            }
        } else {
            space_run = 0;
        }
    }

    count
}

fn strip_token_for_matching(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn is_table_header_token(token: &str) -> bool {
    matches!(
        token,
        "mean"
            | "median"
            | "mode"
            | "sd"
            | "se"
            | "n"
            | "min"
            | "max"
            | "df"
            | "p"
            | "f"
            | "t"
            | "z"
            | "ci"
            | "m"
    )
}

fn looks_like_formula_or_code(text: &str, profile: &CandidateTextProfile) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    let operator_count = trimmed.chars().filter(|c| is_formula_operator(*c)).count();
    let bracket_count = trimmed.chars().filter(|c| is_code_bracket(*c)).count();
    let contains_assignment = trimmed.contains('=');
    let contains_semicolon = trimmed.contains(';');
    let contains_underscore = trimmed.contains('_');
    let contains_math_symbol = trimmed.chars().any(is_math_symbol);
    let lower = trimmed.to_lowercase();
    let code_keyword = lower.starts_with("if ")
        || lower.starts_with("if(")
        || lower.starts_with("for ")
        || lower.starts_with("for(")
        || lower.starts_with("while ")
        || lower.starts_with("while(")
        || lower.contains(" return ");

    if bracket_count >= 2 && (operator_count > 0 || contains_semicolon || code_keyword) {
        return true;
    }

    if contains_semicolon && (contains_assignment || bracket_count > 0 || contains_underscore) {
        return true;
    }

    if contains_assignment
        && (operator_count >= 2
            || profile.digit_ratio > 0.0
            || contains_underscore
            || contains_math_symbol)
    {
        return true;
    }

    contains_math_symbol && (operator_count > 0 || profile.digit_ratio > 0.0)
}

fn is_formula_operator(c: char) -> bool {
    matches!(
        c,
        '+' | '-'
            | '*'
            | '/'
            | '='
            | '<'
            | '>'
            | '^'
            | '≤'
            | '≥'
            | '≠'
            | '±'
            | '×'
            | '÷'
            | '≈'
            | '∑'
            | '√'
            | '∫'
            | '→'
            | '←'
            | '⇒'
    )
}

fn is_code_bracket(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
}

fn is_math_symbol(c: char) -> bool {
    matches!(c, 'χ' | 'Χ' | '²' | '³' | 'μ' | 'σ' | 'Σ' | 'π' | 'Π' | '∞')
}

fn looks_like_markup_or_extraction_noise(text: &str, profile: &CandidateTextProfile) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    is_markup_like_token(trimmed)
        || is_cid_like_marker(trimmed)
        || trimmed.contains('\u{fffd}')
        || (profile.max_repeated_char_run >= 4 && profile.alphabetic_ratio < 0.25)
        || (profile.symbol_ratio >= 0.65 && profile.alphabetic_ratio < 0.20)
        || (profile.symbol_ratio >= 0.80 && profile.token_count <= 3)
}

fn is_markup_like_token(text: &str) -> bool {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();

    if KNOWN_INLINE_PATTERNS
        .iter()
        .any(|pattern| trimmed.eq_ignore_ascii_case(pattern))
    {
        return true;
    }

    let html_tag = lower.starts_with('<')
        && lower.ends_with('>')
        && lower.chars().count() <= 80
        && lower.chars().any(|c| c.is_alphabetic());
    let html_entity = lower.starts_with('&')
        && lower.ends_with(';')
        && lower.chars().count() <= 32
        && lower
            .chars()
            .skip(1)
            .take_while(|c| *c != ';')
            .all(|c| c.is_alphanumeric() || c == '#');

    html_tag || html_entity
}

fn is_cid_like_marker(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let Some(rest) = lower.strip_prefix("cid:") else {
        return false;
    };

    !rest.is_empty() && rest.chars().all(is_decimal_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obvious(text: &str) -> bool {
        let profile = profile_text_line(text, "");
        let label = classify_text_signal(&profile, TextSignalContext::default());
        let reasons = text_signal_reasons(text, &profile, label, 0.0);
        is_obvious_extraction_noise(&profile, &reasons)
    }

    #[test]
    fn obvious_extraction_noise_predicate_accepts_strong_junk() {
        for text in [
            "------",
            "________",
            "••••••••",
            "����",
            "cid:14",
            "<br/>",
            "&nbsp;",
        ] {
            assert!(obvious(text), "expected obvious noise for {text:?}");
        }
    }

    #[test]
    fn obvious_extraction_noise_predicate_protects_headings() {
        for text in ["Introduction", "Métodos", "Références"] {
            assert!(!obvious(text), "expected heading protection for {text:?}");
        }
    }

    #[test]
    fn obvious_extraction_noise_predicate_protects_page_labels() {
        for text in ["Page 12", "Página 3 de 10"] {
            assert!(
                !obvious(text),
                "expected page-label protection for {text:?}"
            );
        }
    }

    #[test]
    fn obvious_extraction_noise_predicate_protects_formula_and_code() {
        for text in ["χ² = 5.32", "y = mx + b", "if (x > 0) { return x; }"] {
            assert!(
                !obvious(text),
                "expected formula/code protection for {text:?}"
            );
        }
    }

    #[test]
    fn obvious_extraction_noise_predicate_protects_table_and_statistical_rows() {
        for text in ["Mean SD N", "p < .05", "12.4 15.8 99.2"] {
            assert!(
                !obvious(text),
                "expected table/statistical protection for {text:?}"
            );
        }
    }

    #[test]
    fn obvious_extraction_noise_predicate_protects_ordinary_prose() {
        assert!(!obvious("The participants answered the questionnaire."));
    }

    #[test]
    fn obvious_extraction_noise_predicate_keeps_ambiguous_weak_cases() {
        for text in ["Header", "...", "12", "A * B", "<p>Introduction</p>"] {
            assert!(
                !obvious(text),
                "expected ambiguous weak case to stay: {text:?}"
            );
        }
    }
}
