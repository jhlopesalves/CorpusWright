use crate::clean::PdfEmbeddedTextStrategy;
use lopdf::Document;
use regex::Regex;
use std::sync::{LazyLock, Mutex};

pub static PDFIUM_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Named options for PDF extraction, replacing anonymous boolean parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PdfExtractionOptions {
    pub strategy: PdfEmbeddedTextStrategy,
    pub use_ocr: bool,
    pub remove_repeated_headers_footers: bool,
    pub remove_page_labels: bool,
    pub remove_symbol_heavy_artifacts: bool,
    pub remove_code_like_blocks: bool,
    pub remove_formula_like_lines: bool,
}

impl PdfExtractionOptions {
    /// Build options from a `CleaningConfig`. OCR defaults to `false`;
    /// call sites that need OCR must override it explicitly.
    pub fn from_cleaning_config(config: &crate::clean::CleaningConfig) -> Self {
        Self {
            strategy: config.pdf_embedded_text_strategy,
            use_ocr: false,
            remove_repeated_headers_footers: config.remove_repeated_pdf_headers_footers,
            remove_page_labels: config.remove_pdf_page_labels,
            remove_symbol_heavy_artifacts: config.remove_pdf_symbol_heavy_artifacts,
            remove_code_like_blocks: config.remove_pdf_code_like_blocks,
            remove_formula_like_lines: config.remove_pdf_formula_like_lines,
        }
    }

    /// Default options for raw extraction: PdfiumFlat strategy, no cleanup, no OCR.
    pub fn raw_default() -> Self {
        Self {
            strategy: PdfEmbeddedTextStrategy::PdfiumFlat,
            use_ocr: false,
            remove_repeated_headers_footers: false,
            remove_page_labels: false,
            remove_symbol_heavy_artifacts: false,
            remove_code_like_blocks: false,
            remove_formula_like_lines: false,
        }
    }
}

pub(crate) fn is_graphical_marker(c: char) -> bool {
    matches!(
        c,
        '●' | '•'
            | '·'
            | '○'
            | '■'
            | '□'
            | '▲'
            | '△'
            | '◆'
            | '◇'
            | '🞎'
            | '🞏'
            | '🞐'
            | '🞑'
            | '🞒'
            | '🞓'
            | '🞔'
            | '🞕'
            | '🞖'
            | '🞗'
            | '🞘'
            | '🞙'
            | '🞚'
            | '🞛'
            | '🞜'
            | '🞝'
            | '🞞'
            | '🞟'
    )
}

pub(crate) fn is_noise_symbol(c: char) -> bool {
    is_graphical_marker(c) || matches!(c, '.' | '*' | '+' | '-' | '_' | '|' | '~' | '^' | '=' | '°')
}

pub(crate) fn is_symbol_heavy_artifact(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let non_ws_chars: Vec<char> = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if non_ws_chars.is_empty() {
        return false;
    }

    // If the line consists entirely of noise symbols (and whitespace), it is a symbol-heavy artefact regardless of length.
    if non_ws_chars.iter().all(|&c| is_noise_symbol(c)) {
        return true;
    }

    if trimmed.chars().count() < 7 {
        return false;
    }

    let non_ws_len = non_ws_chars.len();
    let symbol_punct_count = non_ws_chars
        .iter()
        .filter(|&&c| !c.is_alphanumeric())
        .count();
    let alphabetic_count = non_ws_chars.iter().filter(|&&c| c.is_alphabetic()).count();

    let symbol_ratio = symbol_punct_count as f32 / non_ws_len as f32;
    let alphabetic_ratio = alphabetic_count as f32 / non_ws_len as f32;

    if symbol_ratio < 0.70 || alphabetic_ratio > 0.20 {
        return false;
    }

    let word_tokens_count = trimmed
        .split_whitespace()
        .filter(|token| token.chars().any(|c| c.is_alphanumeric()))
        .count();

    if word_tokens_count > 1 {
        return false;
    }

    // Graphical markers count check
    let graphical_markers_count = trimmed.chars().filter(|&c| is_graphical_marker(c)).count();
    if graphical_markers_count >= 5 {
        return true;
    }

    // Dominance check
    let mut symbol_freqs = std::collections::HashMap::new();
    for &c in &non_ws_chars {
        if !c.is_alphanumeric() {
            *symbol_freqs.entry(c).or_insert(0) += 1;
        }
    }
    let max_symbol_count = symbol_freqs.values().max().cloned().unwrap_or(0);
    if max_symbol_count as f32 / non_ws_len as f32 >= 0.70 {
        return true;
    }

    // Obvious repeated run check
    let collapsed_line: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    let collapsed_chars: Vec<char> = collapsed_line.chars().collect();
    if collapsed_chars.len() >= 4 {
        for i in 0..collapsed_chars.len() - 3 {
            let c = collapsed_chars[i];
            if !c.is_alphanumeric()
                && collapsed_chars[i + 1] == c
                && collapsed_chars[i + 2] == c
                && collapsed_chars[i + 3] == c
            {
                return true;
            }
        }
    }

    false
}

pub(crate) fn is_punctuation(c: char) -> bool {
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

pub(crate) fn normalize_candidate_line(s: &str) -> String {
    let trimmed = s.trim();
    // Strip surrounding punctuation
    let chars: Vec<char> = trimmed.chars().collect();

    let mut i = 0;
    while i < chars.len() && is_punctuation(chars[i]) {
        i += 1;
    }
    let start = i;

    let mut j = chars.len();
    while j > start && is_punctuation(chars[j - 1]) {
        j -= 1;
    }
    let end = j;

    let substring: String = chars[start..end].iter().collect();
    let trimmed_sub = substring.trim();

    // Lowercase
    let lower = trimmed_sub.to_lowercase();

    // Collapse internal whitespace and replace digit runs
    static RE_DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let with_replaced_digits = RE_DIGITS.replace_all(&lower, "#");
    let collapsed_spaces = RE_SPACES.replace_all(&with_replaced_digits, " ");

    collapsed_spaces.trim().to_string()
}

pub(crate) fn is_page_label(line: &str) -> bool {
    let s = line.trim();
    if s.is_empty() {
        return false;
    }

    static RE_ARABIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+$").unwrap());
    static RE_ROMAN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^M*(?:C[MD]|D?C{0,3})(?:X[CL]|L?X{0,3})(?:I[XV]|V?I{0,3})$").unwrap()
    });
    static RE_PAGE_WORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^page\s+(\S+)$").unwrap());
    static RE_P_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^p\.\s*(\d+)$").unwrap());
    static RE_DASHES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[-—–]\s*(\S+)\s*[-—–]$").unwrap());
    static RE_OF_SLASH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(?:page\s+)?(\d+)\s*(?:/|of)\s*(\d+)$").unwrap());

    if RE_ARABIC.is_match(s) {
        return true;
    }

    if RE_ROMAN.is_match(s) && !s.is_empty() {
        return true;
    }

    if let Some(caps) = RE_PAGE_WORD.captures(s) {
        let val = caps.get(1).unwrap().as_str();
        if RE_ARABIC.is_match(val) || (RE_ROMAN.is_match(val) && !val.is_empty()) {
            return true;
        }
    }

    if RE_P_DOT.is_match(s) {
        return true;
    }

    if let Some(caps) = RE_DASHES.captures(s) {
        let val = caps.get(1).unwrap().as_str();
        if RE_ARABIC.is_match(val) || (RE_ROMAN.is_match(val) && !val.is_empty()) {
            return true;
        }
    }

    if RE_OF_SLASH.is_match(s) {
        return true;
    }

    false
}

#[derive(Clone, Debug)]
pub struct CharInfo {
    pub c: String,
    pub bottom: f32,
    pub left: f32,
    pub top: f32,
    pub right: f32,
}

pub struct ExtractedPdf {
    pub text: String,
    pub warnings: Vec<String>,
    pub page_count: usize,
}

#[derive(Debug)]
pub enum PdfExtractionError {
    InvalidFormat(String),
}

impl std::fmt::Display for PdfExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdfExtractionError::InvalidFormat(msg) => write!(f, "Invalid PDF: {}", msg),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfLineKind {
    Prose,
    CodeLike,
    FormulaLike,
    SymbolHeavy,
    Ambiguous,
}

fn is_greek_or_math(c: char) -> bool {
    matches!(c, '\u{0370}'..='\u{03FF}' | '\u{2200}'..='\u{22FF}' | '\u{02DB}')
}

fn has_function_call(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    for i in 0..chars.len() - 1 {
        if chars[i].is_alphabetic() && chars[i + 1] == '(' {
            return true;
        }
    }
    false
}

fn is_extremely_code_like_single_line(line: &str) -> bool {
    let trimmed = line.trim();
    let len = trimmed.chars().count();
    if len < 15 {
        return false;
    }

    let has_operators = trimmed.contains("<-")
        || trimmed.contains("|>")
        || trimmed.contains("%>%")
        || trimmed.contains("::");
    let has_indexing = trimmed.contains("[[") || trimmed.contains("]]") || trimmed.contains("$");
    let has_func = has_function_call(trimmed);

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut prose_tokens = 0;
    for token in &tokens {
        let clean = token.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
        if clean.len() >= 2
            && clean
                .chars()
                .all(|c| c.is_alphabetic() || c == '-' || c == '\'')
        {
            prose_tokens += 1;
        }
    }

    let prose_ratio = if !tokens.is_empty() {
        prose_tokens as f64 / tokens.len() as f64
    } else {
        0.0
    };

    let mut indicators = 0;
    if has_operators {
        indicators += 2;
    }
    if has_indexing {
        indicators += 2;
    }
    if has_func {
        indicators += 1;
    }
    if trimmed.ends_with(';') {
        indicators += 1;
    }

    if prose_ratio < 0.2 && indicators >= 2 {
        return true;
    }
    if indicators >= 3 {
        return true;
    }

    false
}

fn classify_pdf_line(line: &str) -> PdfLineKind {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return PdfLineKind::Ambiguous;
    }

    let is_numeric_ticks = trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c.is_whitespace());
    if is_numeric_ticks {
        return PdfLineKind::Prose;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return PdfLineKind::Ambiguous;
    }

    let len = trimmed.chars().count();

    let mut prose_score = 0i32;
    let mut prose_tokens = 0;
    let mut single_char_vars = 0;
    let mut math_symbols_count = 0;

    for token in &tokens {
        let clean = token.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
        if clean.is_empty() {
            continue;
        }
        if clean == "a" || clean == "A" || clean == "i" || clean == "I" {
            prose_tokens += 1;
            continue;
        }
        if clean.len() == 1 {
            let c = clean.chars().next().unwrap();
            if c.is_alphabetic() {
                single_char_vars += 1;
            }
        }
        if clean.len() >= 2
            && clean
                .chars()
                .all(|c| c.is_alphabetic() || c == '-' || c == '\'')
        {
            prose_tokens += 1;
        }
    }

    let prose_ratio = if !tokens.is_empty() {
        prose_tokens as f64 / tokens.len() as f64
    } else {
        0.0
    };

    prose_score += prose_tokens * 3;

    if prose_ratio > 0.7 {
        prose_score += 6;
    }
    if prose_ratio > 0.9 {
        prose_score += 6;
    }

    let lower = trimmed.to_lowercase();
    if lower.starts_with("figure")
        || lower.starts_with("table")
        || lower.starts_with("section")
        || lower.starts_with("chap")
    {
        prose_score += 6;
    }
    if (trimmed.starts_with('•') || trimmed.starts_with('-') || trimmed.starts_with('*'))
        && prose_tokens > 0
    {
        prose_score += 5;
    }

    let mut code_score = 0i32;
    if trimmed.contains("<-") {
        code_score += 5;
    }
    if trimmed.contains(":=") {
        code_score += 5;
    }
    if trimmed.contains("=>") {
        code_score += 3;
    }
    if trimmed.contains("|>") {
        code_score += 5;
    }
    if trimmed.contains("%>%") {
        code_score += 5;
    }
    if trimmed.contains("::") {
        code_score += 4;
    }
    if trimmed.contains("$") {
        code_score += 3;
    }

    if has_function_call(trimmed) {
        code_score += 3;
        if prose_tokens == 0 {
            code_score += 3;
        }
    }

    let bracket_count = trimmed
        .chars()
        .filter(|&c| matches!(c, '{' | '}' | '[' | ']'))
        .count();
    if bracket_count > 0 {
        code_score += 2;
        if trimmed.contains("[[") || trimmed.contains("]]") {
            code_score += 3;
        }
    }

    let mut keyword_count = 0;
    for token in &tokens {
        let clean = token.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
        if matches!(
            clean,
            "for"
                | "while"
                | "if"
                | "else"
                | "function"
                | "return"
                | "def"
                | "class"
                | "lambda"
                | "let"
                | "mut"
                | "library"
                | "import"
                | "from"
        ) {
            keyword_count += 1;
        } else {
            for kw in &[
                "for", "while", "if", "else", "function", "return", "def", "class", "lambda",
                "let", "mut", "library", "import", "from",
            ] {
                if token.starts_with(&format!("{}(", kw)) {
                    keyword_count += 1;
                    break;
                }
            }
        }
    }
    code_score += keyword_count * 4;

    if trimmed.ends_with(';') {
        code_score += 3;
    }

    if prose_ratio < 0.2 && tokens.len() >= 2 {
        code_score += 4;
    }
    if prose_ratio > 0.7 {
        code_score -= 6;
    }

    let mut formula_score = 0i32;
    let has_greek_math = trimmed.chars().any(is_greek_or_math);
    if has_greek_math {
        formula_score += 4;
    }

    math_symbols_count += trimmed
        .chars()
        .filter(|&c| {
            matches!(
                c,
                '=' | '+' | '-' | '*' | '/' | '<' | '>' | '~' | '±' | '×' | '÷'
            )
        })
        .count();
    if math_symbols_count > 0 {
        formula_score += 3;
    }

    if trimmed.contains("H0")
        || trimmed.contains("H1")
        || trimmed.contains("E[")
        || trimmed.contains("Var(")
        || trimmed.contains("Pr(")
    {
        formula_score += 5;
    }

    for token in &tokens {
        if token.starts_with("p(") || token.starts_with("P(") || token.starts_with("Pr(") {
            formula_score += 3;
        }
    }

    if single_char_vars > 0 {
        formula_score += 2;
    }

    if prose_ratio < 0.2 && tokens.len() >= 2 {
        formula_score += 4;
    } else if prose_ratio < 0.4 && tokens.len() >= 2 {
        formula_score += 2;
    }

    if prose_ratio > 0.7 {
        formula_score -= 6;
    }

    let mut symbol_score = 0i32;
    let symbol_count = trimmed
        .chars()
        .filter(|&c| !c.is_alphanumeric() && !c.is_whitespace())
        .count();
    let symbol_ratio = symbol_count as f64 / len as f64;
    symbol_score += symbol_count as i32 * 2;
    if symbol_ratio > 0.7 {
        symbol_score += 6;
    }

    if prose_score >= 8 && prose_ratio > 0.5 {
        return PdfLineKind::Prose;
    }

    if code_score >= 5 {
        prose_score -= 5;
    }
    if formula_score >= 5 {
        prose_score -= 5;
    }

    if code_score >= 5 && code_score > formula_score && code_score > prose_score {
        PdfLineKind::CodeLike
    } else if formula_score >= 5 && formula_score > code_score && formula_score > prose_score {
        if prose_ratio < 0.3 || (has_greek_math && prose_ratio < 0.5) || math_symbols_count >= 2 {
            PdfLineKind::FormulaLike
        } else {
            PdfLineKind::Ambiguous
        }
    } else if symbol_score >= 8 && symbol_ratio > 0.8 {
        PdfLineKind::SymbolHeavy
    } else {
        PdfLineKind::Ambiguous
    }
}

