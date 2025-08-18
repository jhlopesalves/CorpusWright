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

pub fn reconstruct_visual_single_column(mut char_infos: Vec<CharInfo>) -> String {
    if char_infos.is_empty() {
        return String::new();
    }

    // Group into lines by Y midpoint. Sort by midpoint descending first.
    char_infos.sort_by(|a, b| {
        let mid_a = (a.top + a.bottom) / 2.0;
        let mid_b = (b.top + b.bottom) / 2.0;
        mid_b
            .partial_cmp(&mid_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<Vec<CharInfo>> = Vec::new();
    for char_info in char_infos {
        let mid = (char_info.top + char_info.bottom) / 2.0;
        let mut placed = false;
        for line in &mut lines {
            if let Some(first) = line.first() {
                let line_mid = (first.top + first.bottom) / 2.0;
                let line_h = first.top - first.bottom;
                let tolerance = (line_h * 0.4).max(4.0);
                if (mid - line_mid).abs() <= tolerance {
                    line.push(char_info.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            lines.push(vec![char_info]);
        }
    }

    // Sort lines by Y average coordinate descending (top to bottom)
    lines.sort_by(|line_a, line_b| {
        let avg_y_a =
            line_a.iter().map(|c| (c.top + c.bottom) / 2.0).sum::<f32>() / line_a.len() as f32;
        let avg_y_b =
            line_b.iter().map(|c| (c.top + c.bottom) / 2.0).sum::<f32>() / line_b.len() as f32;
        avg_y_b
            .partial_cmp(&avg_y_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut page_text = String::new();
    for mut line in lines {
        // Sort characters left to right
        line.sort_by(|a, b| {
            a.left
                .partial_cmp(&b.left)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut line_str = String::new();
        for (i, char_info) in line.iter().enumerate() {
            if i > 0 {
                let prev = &line[i - 1];
                let gap = char_info.left - prev.right;
                let char_w = char_info.right - char_info.left;

                // Programmatic spacing reconstruction
                if gap > char_w * 0.5 && prev.c != " " && char_info.c != " " {
                    line_str.push(' ');
                }
            }
            line_str.push_str(&char_info.c);
        }
        let trimmed = line_str.trim();
        if !trimmed.is_empty() {
            if !page_text.is_empty() {
                page_text.push('\n');
            }
            page_text.push_str(trimmed);
        }
    }
    page_text
}

pub fn extract_pdf(
    bytes: &[u8],
    max_chars: Option<usize>,
    options: PdfExtractionOptions,
) -> Result<ExtractedPdf, PdfExtractionError> {
    let PdfExtractionOptions {
        strategy,
        use_ocr,
        remove_repeated_headers_footers,
        remove_page_labels,
        remove_symbol_heavy_artifacts,
        remove_code_like_blocks,
        remove_formula_like_lines,
    } = options;

    // 1. Initial formats & encrypted check using lopdf
    let doc = match Document::load_mem(bytes) {
        Ok(doc) => doc,
        Err(e) => {
            return Err(PdfExtractionError::InvalidFormat(format!(
                "Failed to load PDF: {}",
                e
            )));
        }
    };

    let mut warnings = Vec::new();

    if doc.is_encrypted() {
        warnings.push(
            "PDF is encrypted or password protected. Text extraction may fail or produce garbage."
                .to_string(),
        );
    }

    // Load in PDFium
    let pdfium = match crate::pdf_ocr::init_pdfium() {
        Ok(p) => p,
        Err(e) => {
            return Err(PdfExtractionError::InvalidFormat(format!(
                "Failed to initialize PDFium: {}",
                e
            )));
        }
    };

    let document = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        match pdfium.load_pdf_from_byte_slice(bytes, None) {
            Ok(d) => d,
            Err(e) => {
                return Err(PdfExtractionError::InvalidFormat(format!(
                    "Failed to load PDF with PDFium: {:?}",
                    e
                )));
            }
        }
    };

    let page_count = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    warnings.push("PDF reading order is not guaranteed. Formatting may be lost.".to_string());

    let mut all_text = String::new();
    let mut has_any_text = false;

    let mut total_flat_chars = 0;
    let mut total_visual_chars = 0;

    // Warn on two-column strategy (since it is stubbed as unsupported)
    if strategy == PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental {
        warnings.push("Experimental two-column PDF extraction is currently unsupported/stubbed; falling back to visual single-column extraction.".to_string());
    }

    let mut page_lines_list: Vec<(usize, Vec<String>)> = Vec::new();
    let mut current_chars_count = 0;

    for page_index in 0..page_count {
        if let Some(limit) = max_chars
            && current_chars_count >= limit
        {
            break;
        }

        // Lock strictly to read page and extract characters once
        let (flat_text_page, char_infos) = {
            let _lock = PDFIUM_LOCK.lock().unwrap();
            let page = match document.pages().get(page_index as i32) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let text_page = match page.text() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let flat_text = text_page.all();

            let mut infos = Vec::new();
            let mut last_valid_coords = None;
            for c in text_page.chars().iter() {
                let txt = match c.unicode_string() {
                    Some(s) => s,
                    None => continue,
                };
                if txt == "\n" || txt == "\r" || txt.as_bytes().first().is_some_and(|&b| b < 32) {
                    continue;
                }
                let bounds = c.loose_bounds();
                let (bottom, left, top, right) = match bounds {
                    Ok(b) => {
                        let bottom = b.bottom().value;
                        let left = b.left().value;
                        let top = b.top().value;
                        let right = b.right().value;
                        if (right - left).abs() > 0.001 || (top - bottom).abs() > 0.001 {
                            last_valid_coords = Some((bottom, left, top, right));
                            (bottom, left, top, right)
                        } else if txt == " " {
                            if let Some((b, _, t, r)) = last_valid_coords {
                                (b, r, t, r + 0.1)
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                    Err(_) => {
                        if txt == " " {
                            if let Some((b, _, t, r)) = last_valid_coords {
                                (b, r, t, r + 0.1)
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }
                };
                infos.push(CharInfo {
                    c: txt,
                    bottom,
                    left,
                    top,
                    right,
                });
            }
            (flat_text, infos)
        }; // Lock is released here!

        let visual_text_page = reconstruct_visual_single_column(char_infos);

        let flat_len = flat_text_page
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        let visual_len = visual_text_page
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        total_flat_chars += flat_len;
        total_visual_chars += visual_len;

        let page_text = match strategy {
            PdfEmbeddedTextStrategy::PdfiumFlat => flat_text_page,
            PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn
            | PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => visual_text_page,
        };

        let lines: Vec<String> = page_text.lines().map(|s| s.to_string()).collect();
        let page_chars: usize = lines.iter().map(|l| l.chars().count() + 1).sum();
        current_chars_count += page_chars + 1;
        page_lines_list.push((page_index + 1, lines));
    }

    let mut repeated_patterns = std::collections::HashSet::new();
    let total_pages = page_lines_list.len();

    // Part B: Repeated header/footer removal
    if remove_repeated_headers_footers && total_pages >= 3 {
        let n = 3;
        let mut candidate_counts = std::collections::HashMap::new();

        for (_, page) in &page_lines_list {
            let len = page.len();
            let top_limit = n.min(len);
            let bottom_start = top_limit.max(len.saturating_sub(n));

            // Collect unique normalized candidates on this page to count page occurrences
            let mut page_candidates = std::collections::HashSet::new();

            // Top candidates
            for line in &page[0..top_limit] {
                let norm = normalize_candidate_line(line);
                if !norm.is_empty() {
                    page_candidates.insert(norm);
                }
            }

            // Bottom candidates
            for line in &page[bottom_start..len] {
                let norm = normalize_candidate_line(line);
                if !norm.is_empty() {
                    page_candidates.insert(norm);
                }
            }

            for cand in page_candidates {
                *candidate_counts.entry(cand).or_insert(0) += 1;
            }
        }

        // Find repeated patterns that meet the threshold: at least 3 pages AND at least 50% of pages
        let threshold = 3.max(total_pages.div_ceil(2));
        for (cand, count) in candidate_counts {
            if count >= threshold {
                repeated_patterns.insert(cand);
            }
        }
    }

    let mut cleaned_pages = Vec::with_capacity(total_pages);
    let mut header_footer_removed_pages_count = 0;
    let mut page_labels_removed_pages_count = 0;
    let mut header_footer_removed_patterns = std::collections::HashSet::new();

    let mut symbol_heavy_removed_pages: Vec<(usize, usize, usize)> = Vec::new(); // (page_num, lines_count, blocks_count)
    let n = 3;

    let mut code_blocks_removed_total = 0;
    let mut code_lines_removed_total = 0;
    let mut code_pages_with_removals = 0;
    let mut formula_lines_removed_total = 0;
    let mut formula_pages_with_removals = 0;

    for (page_num, page) in page_lines_list {
        let len = page.len();
        let top_limit = n.min(len);
        let bottom_start = top_limit.max(len.saturating_sub(n));

        let mut cleaned_page_lines = Vec::with_capacity(len);
        let mut header_footer_removed_on_this_page = false;
        let mut page_labels_removed_on_this_page = false;

        let mut symbol_heavy_removed_lines_on_this_page = 0;
        let mut symbol_heavy_removed_blocks_on_this_page = 0;
        let mut current_block_active = false;

        for (idx, line) in page.into_iter().enumerate() {
            let is_in_candidate_zone = idx < top_limit || idx >= bottom_start;
            let mut remove = false;

            if is_in_candidate_zone {
                // Check repeated header/footer
                if remove_repeated_headers_footers && total_pages >= 3 {
                    let norm = normalize_candidate_line(&line);
                    if !norm.is_empty() && repeated_patterns.contains(&norm) {
                        remove = true;
                        header_footer_removed_on_this_page = true;
                        header_footer_removed_patterns.insert(norm);
                    }
                }

                // Check page labels/number
                if !remove && remove_page_labels && is_page_label(&line) {
                    remove = true;
                    page_labels_removed_on_this_page = true;
                }
            }

            // Check symbol-heavy graphical artifacts
            if !remove && remove_symbol_heavy_artifacts {
                if is_symbol_heavy_artifact(&line) {
                    remove = true;
                    symbol_heavy_removed_lines_on_this_page += 1;
                    if !current_block_active {
                        symbol_heavy_removed_blocks_on_this_page += 1;
                        current_block_active = true;
                    }
                } else {
                    current_block_active = false;
                }
            } else {
                current_block_active = false;
            }

            if !remove {
                cleaned_page_lines.push(line);
            }
        }

        if header_footer_removed_on_this_page {
            header_footer_removed_pages_count += 1;
        }
        if page_labels_removed_on_this_page {
            page_labels_removed_pages_count += 1;
        }
        if symbol_heavy_removed_lines_on_this_page > 0 {
            symbol_heavy_removed_pages.push((
                page_num,
                symbol_heavy_removed_lines_on_this_page,
                symbol_heavy_removed_blocks_on_this_page,
            ));
        }

        let mut final_page_lines = cleaned_page_lines;
        let mut code_removed_on_this_page = 0;
        let mut code_blocks_removed_on_this_page = 0;
        let mut formula_removed_on_this_page = 0;

        if remove_code_like_blocks || remove_formula_like_lines {
            let len = final_page_lines.len();
            let mut classification = Vec::with_capacity(len);
            for line in &final_page_lines {
                classification.push(classify_pdf_line(line));
            }

            let mut to_remove = vec![false; len];

            if remove_code_like_blocks {
                let mut start_idx = None;
                for (i, class) in classification.iter().enumerate() {
                    if *class == PdfLineKind::CodeLike {
                        if start_idx.is_none() {
                            start_idx = Some(i);
                        }
                    } else {
                        if let Some(start) = start_idx {
                            let run_len = i - start;
                            if run_len >= 2 {
                                for item in to_remove.iter_mut().take(i).skip(start) {
                                    *item = true;
                                }
                            } else {
                                if is_extremely_code_like_single_line(&final_page_lines[start]) {
                                    to_remove[start] = true;
                                }
                            }
                            start_idx = None;
                        }
                    }
                }
                if let Some(start) = start_idx {
                    let run_len = len - start;
                    if run_len >= 2 {
                        for item in to_remove.iter_mut().take(len).skip(start) {
                            *item = true;
                        }
                    } else {
                        if is_extremely_code_like_single_line(&final_page_lines[start]) {
                            to_remove[start] = true;
                        }
                    }
                }
            }

            if remove_formula_like_lines {
                for i in 0..len {
                    if classification[i] == PdfLineKind::FormulaLike {
                        to_remove[i] = true;
                    }
                }
            }

            let mut filtered = Vec::with_capacity(len);
            let mut in_code_block = false;
            for i in 0..len {
                if to_remove[i] {
                    if classification[i] == PdfLineKind::CodeLike {
                        code_removed_on_this_page += 1;
                        if !in_code_block {
                            code_blocks_removed_on_this_page += 1;
                            in_code_block = true;
                        }
                    } else {
                        in_code_block = false;
                    }
                    if classification[i] == PdfLineKind::FormulaLike {
                        formula_removed_on_this_page += 1;
                    }
                } else {
                    in_code_block = false;
                    filtered.push(final_page_lines[i].clone());
                }
            }
            final_page_lines = filtered;
        }

        if code_removed_on_this_page > 0 {
            code_lines_removed_total += code_removed_on_this_page;
            code_blocks_removed_total += code_blocks_removed_on_this_page;
            code_pages_with_removals += 1;
        }
        if formula_removed_on_this_page > 0 {
            formula_lines_removed_total += formula_removed_on_this_page;
            formula_pages_with_removals += 1;
        }

        cleaned_pages.push(final_page_lines);
    }

    // Reconstruct all_text
    for page_lines in cleaned_pages {
        let page_text = page_lines.join("\n");
        let trimmed = page_text.trim();
        if !trimmed.is_empty() {
            has_any_text = true;
            if !all_text.is_empty() {
                all_text.push('\n');
                all_text.push('\n');
            }
            all_text.push_str(trimmed);
        }
    }

    if header_footer_removed_pages_count > 0 {
        let mut patterns_vec: Vec<String> = header_footer_removed_patterns.into_iter().collect();
        patterns_vec.sort();
        let patterns_str = patterns_vec
            .iter()
            .map(|p| format!("\"{}\"", p))
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(format!(
            "Removed repeated PDF header/footer lines from {} pages (patterns: {}).",
            header_footer_removed_pages_count, patterns_str
        ));
    }
    if page_labels_removed_pages_count > 0 {
        warnings.push(format!(
            "Removed PDF page labels/page-number lines from {} pages.",
            page_labels_removed_pages_count
        ));
    }
    if !symbol_heavy_removed_pages.is_empty() {
        let total_removed_lines: usize = symbol_heavy_removed_pages.iter().map(|(_, l, _)| l).sum();
        let total_pages_with_removals = symbol_heavy_removed_pages.len();

        if total_pages_with_removals == 1 {
            let (page_num, lines_count, blocks_count) = symbol_heavy_removed_pages[0];
            warnings.push(format!(
                "Removed {} symbol-heavy PDF graphical artefact lines (folded into {} block{}) from page {}.",
                lines_count, blocks_count, if blocks_count == 1 { "" } else { "s" }, page_num
            ));
        } else {
            let example_pages: Vec<String> = symbol_heavy_removed_pages
                .iter()
                .take(3)
                .map(|(p, _, _)| p.to_string())
                .collect();
            let example_str = example_pages.join(", ");
            warnings.push(format!(
                "Removed {} symbol-heavy PDF graphical artefact lines across {} pages. Example pages: {}.",
                total_removed_lines, total_pages_with_removals, example_str
            ));
        }
    }
    if code_lines_removed_total > 0 {
        warnings.push(format!(
            "Removed {} PDF code-like blocks containing {} lines across {} pages.",
            code_blocks_removed_total, code_lines_removed_total, code_pages_with_removals
        ));
    }
    if formula_lines_removed_total > 0 {
        warnings.push(format!(
            "Removed {} PDF formula-like lines across {} pages.",
            formula_lines_removed_total, formula_pages_with_removals
        ));
    }

    // Diagnostics / warning for visual vs flat drop
    if total_flat_chars > 0 {
        let ratio = (total_visual_chars as f64) / (total_flat_chars as f64);
        if ratio < 0.7 {
            warnings.push("Layout-aware PDF extraction returned substantially less text than flat extraction; some nested or positioned text may have been skipped.".to_string());
        }
    }

    let (quality, _) = crate::pdf_quality::evaluate(&all_text);
    let is_poor_quality = quality == crate::pdf_quality::ExtractionQuality::Poor
        || quality == crate::pdf_quality::ExtractionQuality::Suspicious;

    if !has_any_text {
        warnings.push(
            "PDF produced no extractable text. It may be a scanned image-only document."
                .to_string(),
        );
    } else if is_poor_quality {
        warnings.push("PDF embedded-text extraction appears low quality. The output contains unusually many symbols or non-word fragments.".to_string());
    }

    if use_ocr && (!has_any_text || is_poor_quality) {
        // Cap OCR fallback length for large previews to prevent UI freezing.
        // Export passes None, so it remains uncapped.
        let ocr_max_chars = max_chars.map(|limit| if limit > 2000 { 1000 } else { limit });

        match crate::pdf_ocr::extract_text_via_ocr(bytes, ocr_max_chars) {
            Ok(ocr_text) => {
                all_text = ocr_text;
                warnings
                    .push("Used experimental OCR to extract text from scanned pages.".to_string());
            }
            Err(e) => {
                warnings.push(format!("Experimental OCR failed: {}", e));
            }
        }
    }

    Ok(ExtractedPdf {
        text: all_text,
        warnings,
        page_count,
    })
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

