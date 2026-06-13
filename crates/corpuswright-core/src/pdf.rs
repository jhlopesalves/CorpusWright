use crate::clean::{PdfEmbeddedTextStrategy, PdfOcrQuality, PdfTextSource};
use lopdf::Document;
use pdfium_render::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};
use ts_rs::TS;

pub static PDFIUM_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const PDF_OCR_PREVIEW_CHAR_CAP: usize = 5_000;
const PDF_OCR_PREVIEW_PAGE_CAP: usize = 1;

/// Named options for PDF extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PdfExtractionOptions {
    pub strategy: PdfEmbeddedTextStrategy,
    pub text_source: PdfTextSource,
    pub ocr_quality: PdfOcrQuality,
    pub remove_repeated_headers_footers: bool,
    pub remove_page_labels: bool,
    pub remove_symbol_heavy_artifacts: bool,
    pub remove_code_like_blocks: bool,
    pub remove_formula_like_lines: bool,
}

impl PdfExtractionOptions {
    /// Build options from a `CleaningConfig`.
    pub fn from_cleaning_config(config: &crate::clean::CleaningConfig) -> Self {
        Self {
            strategy: config.pdf_embedded_text_strategy,
            text_source: config.pdf_text_source,
            ocr_quality: config.pdf_ocr_quality,
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
            text_source: PdfTextSource::EmbeddedText,
            ocr_quality: PdfOcrQuality::Balanced,
            remove_repeated_headers_footers: false,
            remove_page_labels: false,
            remove_symbol_heavy_artifacts: false,
            remove_code_like_blocks: false,
            remove_formula_like_lines: false,
        }
    }
}

fn capped_ocr_chars_for_preview(max_chars: Option<usize>) -> Option<usize> {
    max_chars.map(|limit| limit.min(PDF_OCR_PREVIEW_CHAR_CAP))
}

fn warn_about_ocr_preview_cap(warnings: &mut Vec<String>, max_chars: Option<usize>) {
    if let Some(cap) = capped_ocr_chars_for_preview(max_chars) {
        let page_unit = if PDF_OCR_PREVIEW_PAGE_CAP == 1 {
            "page"
        } else {
            "pages"
        };
        warnings.push(format!(
            "OCR preview is capped to at most {} characters and {} {}. Search/export may process more pages and take much longer.",
            cap, PDF_OCR_PREVIEW_PAGE_CAP, page_unit
        ));
    }
}

fn run_pdf_ocr(
    bytes: &[u8],
    max_chars: Option<usize>,
    ocr_quality: PdfOcrQuality,
    warnings: &mut Vec<String>,
    success_warning: &str,
) -> Option<String> {
    warn_about_ocr_preview_cap(warnings, max_chars);
    match crate::pdf_ocr::extract_text_via_ocr(
        bytes,
        capped_ocr_chars_for_preview(max_chars),
        max_chars.map(|_| PDF_OCR_PREVIEW_PAGE_CAP),
        ocr_quality,
    ) {
        Ok(ocr) => {
            warnings.extend(ocr.warnings);
            warnings.push(success_warning.to_string());
            Some(ocr.text)
        }
        Err(e) => {
            warnings.push(format!("Experimental OCR failed: {}", e));
            None
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

    let graphical_markers_count = trimmed.chars().filter(|&c| is_graphical_marker(c)).count();
    if graphical_markers_count >= 5 {
        return true;
    }

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

    let lower = trimmed_sub.to_lowercase();

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PdfPageExtractionMethod {
    Embedded,
    Ocr,
    ForceOcr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PdfPageRangePage {
    pub page_index: usize,
    pub page_number: usize,
    pub text: String,
    pub char_count: usize,
    pub method: PdfPageExtractionMethod,
    pub warnings: Vec<String>,
    pub render_clamped: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct PdfPageRangeResult {
    pub page_count: usize,
    pub start_page_index: usize,
    pub end_page_index: usize,
    pub pages: Vec<PdfPageRangePage>,
    pub warnings: Vec<String>,
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

fn finalize_lopdf_fallback_text(
    mut text: String,
    page_count: usize,
    max_chars: Option<usize>,
    mut warnings: Vec<String>,
) -> ExtractedPdf {
    if let Some(limit) = max_chars {
        text = text.chars().take(limit).collect();
    }

    warnings.push("PDF backend: lopdf fallback (degraded).".to_string());

    let (quality, _) = crate::pdf_quality::evaluate(&text);
    match quality {
        crate::pdf_quality::ExtractionQuality::Good => {
            warnings.push(
                "PDFium is unavailable; used degraded lopdf embedded-text extraction.".to_string(),
            );
        }
        crate::pdf_quality::ExtractionQuality::Suspicious => {
            warnings.push(
                "PDFium is unavailable; degraded lopdf extraction produced suspicious text quality."
                    .to_string(),
            );
        }
        crate::pdf_quality::ExtractionQuality::Poor => {
            warnings.push(
                "PDFium is unavailable and degraded PDF extraction produced low-quality text. OCR/PDFium extraction is required for this PDF."
                    .to_string(),
            );
            text.clear();
        }
        crate::pdf_quality::ExtractionQuality::Empty => {
            warnings.push(
                "PDFium is unavailable and degraded PDF extraction produced no text.".to_string(),
            );
        }
    }

    ExtractedPdf {
        text,
        warnings,
        page_count,
    }
}

fn extract_with_lopdf_fallback(
    doc: &Document,
    page_numbers: &[u32],
    max_chars: Option<usize>,
    warnings: Vec<String>,
    pdfium_error: &str,
) -> Result<ExtractedPdf, PdfExtractionError> {
    if page_numbers.is_empty() {
        return Err(PdfExtractionError::InvalidFormat(
            "PDF has no pages.".to_string(),
        ));
    }

    match doc.extract_text(page_numbers) {
        Ok(text) => Ok(finalize_lopdf_fallback_text(
            text,
            page_numbers.len(),
            max_chars,
            warnings,
        )),
        Err(lopdf_err) => Err(PdfExtractionError::InvalidFormat(format!(
            "Failed to initialize PDFium ({}) and lopdf fallback also failed: {}",
            pdfium_error, lopdf_err
        ))),
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

                // A gap greater than half a character width usually represents a space.
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

fn page_range_method(text_source: PdfTextSource) -> PdfPageExtractionMethod {
    match text_source {
        PdfTextSource::EmbeddedText => PdfPageExtractionMethod::Embedded,
        PdfTextSource::Ocr => PdfPageExtractionMethod::Ocr,
        PdfTextSource::ForceOcr => PdfPageExtractionMethod::ForceOcr,
    }
}

fn apply_page_char_cap(page: &mut PdfPageRangePage, max_chars_per_page: Option<usize>) {
    let original_char_count = page.text.chars().count();
    if let Some(limit) = max_chars_per_page
        && original_char_count > limit
    {
        page.text = page.text.chars().take(limit).collect();
        page.warnings.push(format!(
            "Page {} text was truncated to {} characters.",
            page.page_number, limit
        ));
    }
    page.char_count = page.text.chars().count();
}

fn validate_page_range(
    start_page_index: usize,
    requested_page_count: usize,
    total_page_count: usize,
) -> Result<std::ops::Range<usize>, PdfExtractionError> {
    if requested_page_count == 0 {
        return Ok(start_page_index..start_page_index);
    }
    if start_page_index > total_page_count {
        return Err(PdfExtractionError::InvalidFormat(format!(
            "Page range starts at index {start_page_index}, but the PDF has {total_page_count} pages."
        )));
    }
    let end_page_index = start_page_index
        .saturating_add(requested_page_count)
        .min(total_page_count);
    Ok(start_page_index..end_page_index)
}

fn extract_embedded_page_text(
    page: &PdfPage<'_>,
    strategy: PdfEmbeddedTextStrategy,
) -> Result<(String, Vec<String>), String> {
    let mut warnings = Vec::new();
    let text_page = page.text().map_err(|error| error.to_string())?;
    let flat_text = text_page.all();

    match strategy {
        PdfEmbeddedTextStrategy::PdfiumFlat => Ok((flat_text, warnings)),
        PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn
        | PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental => {
            if strategy == PdfEmbeddedTextStrategy::PdfiumVisualColumnsExperimental {
                warnings.push("Experimental two-column PDF extraction is currently unsupported/stubbed; falling back to visual single-column extraction.".to_string());
            }

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

            Ok((reconstruct_visual_single_column(infos), warnings))
        }
    }
}

fn extract_pdf_embedded_page_range(
    bytes: &[u8],
    start_page_index: usize,
    requested_page_count: usize,
    options: PdfExtractionOptions,
) -> Result<(usize, Vec<PdfPageRangePage>), PdfExtractionError> {
    let pdfium = crate::pdf_ocr::init_pdfium().map_err(|error| {
        PdfExtractionError::InvalidFormat(format!("Failed to initialize PDFium: {error}"))
    })?;

    let document = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|error| {
                PdfExtractionError::InvalidFormat(format!(
                    "Failed to load PDF with PDFium: {error:?}"
                ))
            })?
    };

    let page_count = {
        let _lock = PDFIUM_LOCK.lock().unwrap();
        document.pages().len() as usize
    };

    let range = validate_page_range(start_page_index, requested_page_count, page_count)?;
    let mut pages = Vec::with_capacity(range.len());

    for page_index in range {
        let page_number = page_index + 1;
        let result = {
            let _lock = PDFIUM_LOCK.lock().unwrap();
            match document.pages().get(page_index as i32) {
                Ok(page) => extract_embedded_page_text(&page, options.strategy),
                Err(error) => Err(error.to_string()),
            }
        };

        match result {
            Ok((text, warnings)) => pages.push(PdfPageRangePage {
                page_index,
                page_number,
                char_count: text.chars().count(),
                text,
                method: PdfPageExtractionMethod::Embedded,
                warnings,
                render_clamped: false,
                error: None,
            }),
            Err(error) => pages.push(PdfPageRangePage {
                page_index,
                page_number,
                text: String::new(),
                char_count: 0,
                method: PdfPageExtractionMethod::Embedded,
                warnings: Vec::new(),
                render_clamped: false,
                error: Some(error),
            }),
        }
    }

    Ok((page_count, pages))
}

/// Extracts a page range from a single PDF and returns page-level results.
///
/// The OCR modes process only the requested pages and keep failures local to
/// the affected page. `PdfTextSource::Ocr` runs OCR directly for this
/// inspection path; mixed embedded-text/OCR fallback remains part of the
/// full-document extraction path.
pub fn extract_pdf_page_range(
    bytes: &[u8],
    start_page_index: usize,
    requested_page_count: usize,
    options: PdfExtractionOptions,
    max_chars_per_page: Option<usize>,
) -> Result<PdfPageRangeResult, PdfExtractionError> {
    let doc = Document::load_mem(bytes).map_err(|error| {
        PdfExtractionError::InvalidFormat(format!("Failed to load PDF: {error}"))
    })?;
    let mut warnings = Vec::new();
    if doc.is_encrypted() {
        warnings.push(
            "PDF is encrypted or password protected. Text extraction may fail or produce garbage."
                .to_string(),
        );
    }

    let method = page_range_method(options.text_source);
    let (page_count, mut pages) = match options.text_source {
        PdfTextSource::EmbeddedText => {
            extract_pdf_embedded_page_range(bytes, start_page_index, requested_page_count, options)?
        }
        PdfTextSource::Ocr | PdfTextSource::ForceOcr => {
            if options.text_source == PdfTextSource::Ocr {
                warnings.push(
                    "Page-range OCR preview runs OCR directly; mixed embedded-text fallback is not used in this path."
                        .to_string(),
                );
            }
            let (page_count, ocr_pages) = crate::pdf_ocr::extract_page_range_via_ocr(
                bytes,
                start_page_index,
                requested_page_count,
                options.ocr_quality,
            )
            .map_err(|error| PdfExtractionError::InvalidFormat(error.to_string()))?;

            if start_page_index > page_count {
                return Err(PdfExtractionError::InvalidFormat(format!(
                    "Page range starts at index {start_page_index}, but the PDF has {page_count} pages."
                )));
            }

            let pages = ocr_pages
                .into_iter()
                .map(|page| PdfPageRangePage {
                    page_index: page.page_index,
                    page_number: page.page_index + 1,
                    char_count: page.text.chars().count(),
                    text: page.text,
                    method: method.clone(),
                    warnings: page.warnings,
                    render_clamped: page.render_clamped,
                    error: page.error,
                })
                .collect::<Vec<_>>();
            (page_count, pages)
        }
    };

    for page in &mut pages {
        apply_page_char_cap(page, max_chars_per_page);
    }

    let end_page_index = pages
        .last()
        .map(|page| page.page_index + 1)
        .unwrap_or(start_page_index);

    Ok(PdfPageRangeResult {
        page_count,
        start_page_index,
        end_page_index,
        pages,
        warnings,
    })
}

pub fn extract_pdf(
    bytes: &[u8],
    max_chars: Option<usize>,
    options: PdfExtractionOptions,
) -> Result<ExtractedPdf, PdfExtractionError> {
    let PdfExtractionOptions {
        strategy,
        text_source,
        ocr_quality,
        remove_repeated_headers_footers,
        remove_page_labels,
        remove_symbol_heavy_artifacts,
        remove_code_like_blocks,
        remove_formula_like_lines,
    } = options;

    // lopdf is used first for format and encryption checks.
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

    // Fall back to lopdf when the native PDFium library is unavailable.
    let pdfium = match crate::pdf_ocr::init_pdfium() {
        Ok(p) => p,
        Err(e) => {
            let pdfium_error = e.to_string();
            warnings.push(format!(
                "PDFium native library not available; OCR fallback is disabled and PDF extraction may be limited. ({})",
                pdfium_error
            ));
            if text_source == PdfTextSource::ForceOcr {
                warnings.push(
                    "Force OCR was selected, but PDFium is unavailable; embedded-text fallback was not used for this OCR mode."
                        .to_string(),
                );
                return Ok(ExtractedPdf {
                    text: String::new(),
                    warnings,
                    page_count: doc.get_pages().len(),
                });
            }
            let page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();
            return extract_with_lopdf_fallback(
                &doc,
                &page_numbers,
                max_chars,
                warnings,
                &pdfium_error,
            );
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
    warnings.push("PDF backend: PDFium.".to_string());

    if text_source == PdfTextSource::ForceOcr {
        // OCR opens its own PDFium document; release the embedded-text handle first.
        drop(document);
        let text = run_pdf_ocr(
            bytes,
            max_chars,
            ocr_quality,
            &mut warnings,
            "Used experimental Force OCR to extract PDF text from rendered pages.",
        )
        .unwrap_or_default();
        if text.trim().is_empty() {
            warnings.push("Force OCR completed but produced no text.".to_string());
        }

        return Ok(ExtractedPdf {
            text,
            warnings,
            page_count,
        });
    }

    let mut all_text = String::new();
    let mut has_any_text = false;

    let mut total_flat_chars = 0;
    let mut total_visual_chars = 0;

    // The two-column strategy is still routed through single-column extraction.
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

        // PDFium document access is serialised across the native library boundary.
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

    if remove_repeated_headers_footers && total_pages >= 3 {
        let n = 3;
        let mut candidate_counts = std::collections::HashMap::new();

        for (_, page) in &page_lines_list {
            let len = page.len();
            let top_limit = n.min(len);
            let bottom_start = top_limit.max(len.saturating_sub(n));

            // Page-level uniqueness keeps repeated lines from inflating page counts.
            let mut page_candidates = std::collections::HashSet::new();

            for line in &page[0..top_limit] {
                let norm = normalize_candidate_line(line);
                if !norm.is_empty() {
                    page_candidates.insert(norm);
                }
            }

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

        // Header/footer removal requires at least 3 pages and at least half of all pages.
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
                if remove_repeated_headers_footers && total_pages >= 3 {
                    let norm = normalize_candidate_line(&line);
                    if !norm.is_empty() && repeated_patterns.contains(&norm) {
                        remove = true;
                        header_footer_removed_on_this_page = true;
                        header_footer_removed_patterns.insert(norm);
                    }
                }

                if !remove && remove_page_labels && is_page_label(&line) {
                    remove = true;
                    page_labels_removed_on_this_page = true;
                }
            }

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

    // Compare layout-aware output against flat extraction as a loss signal.
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

    if text_source == PdfTextSource::Ocr && (!has_any_text || is_poor_quality) {
        // OCR opens its own PDFium document; release the embedded-text handle first.
        drop(document);
        if let Some(ocr_text) = run_pdf_ocr(
            bytes,
            max_chars,
            ocr_quality,
            &mut warnings,
            "Used experimental OCR rescue to extract text from scanned pages.",
        ) {
            if ocr_text.trim().is_empty() {
                warnings.push("OCR rescue completed but produced no text.".to_string());
            }
            all_text = ocr_text;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::{CleaningConfig, PdfEmbeddedTextStrategy, PdfOcrQuality, PdfTextSource};
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, StringFormat, dictionary};

    #[test]
    fn test_pdf_extraction_options_from_cleaning_config() {
        let config = CleaningConfig {
            pdf_text_source: PdfTextSource::Ocr,
            pdf_ocr_quality: PdfOcrQuality::HighQuality,
            pdf_embedded_text_strategy: PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn,
            remove_repeated_pdf_headers_footers: true,
            remove_pdf_page_labels: true,
            remove_pdf_symbol_heavy_artifacts: false,
            remove_pdf_code_like_blocks: true,
            remove_pdf_formula_like_lines: false,
            ..CleaningConfig::default()
        };
        let options = PdfExtractionOptions::from_cleaning_config(&config);
        assert_eq!(
            options.strategy,
            PdfEmbeddedTextStrategy::PdfiumVisualSingleColumn
        );
        assert_eq!(options.text_source, PdfTextSource::Ocr);
        assert_eq!(options.ocr_quality, PdfOcrQuality::HighQuality);
        assert!(options.remove_repeated_headers_footers);
        assert!(options.remove_page_labels);
        assert!(!options.remove_symbol_heavy_artifacts);
        assert!(options.remove_code_like_blocks);
        assert!(!options.remove_formula_like_lines);
    }

    #[test]
    fn test_pdf_extraction_options_default_to_embedded_text() {
        let options = PdfExtractionOptions::from_cleaning_config(&CleaningConfig::default());
        assert_eq!(options.text_source, PdfTextSource::EmbeddedText);
        assert_eq!(options.ocr_quality, PdfOcrQuality::Balanced);
    }

    #[test]
    fn test_pdf_extraction_options_raw_default() {
        let options = PdfExtractionOptions::raw_default();
        assert_eq!(options.strategy, PdfEmbeddedTextStrategy::PdfiumFlat);
        assert_eq!(options.text_source, PdfTextSource::EmbeddedText);
        assert_eq!(options.ocr_quality, PdfOcrQuality::Balanced);
        assert!(!options.remove_repeated_headers_footers);
        assert!(!options.remove_page_labels);
        assert!(!options.remove_symbol_heavy_artifacts);
        assert!(!options.remove_code_like_blocks);
        assert!(!options.remove_formula_like_lines);
    }

    #[test]
    fn ocr_preview_cap_warning_mentions_page_cap() {
        let mut warnings = Vec::new();

        warn_about_ocr_preview_cap(&mut warnings, Some(10_000));

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("OCR preview is capped"));
        assert!(warnings[0].contains("5000 characters"));
        assert!(warnings[0].contains("1 page"));
        assert!(warnings[0].contains("Search/export may process more pages"));
    }

    #[test]
    fn test_reconstruct_visual_single_column_preserves_line_order() {
        let chars = vec![
            CharInfo {
                c: "T".to_string(),
                bottom: 500.0,
                left: 10.0,
                top: 510.0,
                right: 15.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 500.0,
                left: 15.0,
                top: 510.0,
                right: 20.0,
            },
            CharInfo {
                c: "p".to_string(),
                bottom: 500.0,
                left: 20.0,
                top: 510.0,
                right: 25.0,
            },
            CharInfo {
                c: " ".to_string(),
                bottom: 500.0,
                left: 25.0,
                top: 510.0,
                right: 26.0,
            },
            CharInfo {
                c: "l".to_string(),
                bottom: 500.0,
                left: 26.0,
                top: 510.0,
                right: 30.0,
            },
            CharInfo {
                c: "i".to_string(),
                bottom: 500.0,
                left: 30.0,
                top: 510.0,
                right: 35.0,
            },
            CharInfo {
                c: "n".to_string(),
                bottom: 500.0,
                left: 35.0,
                top: 510.0,
                right: 40.0,
            },
            CharInfo {
                c: "e".to_string(),
                bottom: 500.0,
                left: 40.0,
                top: 510.0,
                right: 45.0,
            },
            CharInfo {
                c: "B".to_string(),
                bottom: 480.0,
                left: 10.0,
                top: 490.0,
                right: 15.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 480.0,
                left: 15.0,
                top: 490.0,
                right: 20.0,
            },
            CharInfo {
                c: "t".to_string(),
                bottom: 480.0,
                left: 20.0,
                top: 490.0,
                right: 25.0,
            },
            CharInfo {
                c: "t".to_string(),
                bottom: 480.0,
                left: 25.0,
                top: 490.0,
                right: 30.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 480.0,
                left: 30.0,
                top: 490.0,
                right: 35.0,
            },
            CharInfo {
                c: "m".to_string(),
                bottom: 480.0,
                left: 35.0,
                top: 490.0,
                right: 40.0,
            },
        ];

        let result = reconstruct_visual_single_column(chars);
        assert_eq!(result, "Top line\nBottom");
    }

    #[test]
    fn test_reconstruct_visual_single_column_missing_space_reconstruction() {
        let chars = vec![
            // "Word1" and "Word2" with a gap of 10.0 (char width is 5.0)
            CharInfo {
                c: "W".to_string(),
                bottom: 100.0,
                left: 10.0,
                top: 110.0,
                right: 15.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 100.0,
                left: 15.0,
                top: 110.0,
                right: 20.0,
            },
            CharInfo {
                c: "r".to_string(),
                bottom: 100.0,
                left: 20.0,
                top: 110.0,
                right: 25.0,
            },
            CharInfo {
                c: "d".to_string(),
                bottom: 100.0,
                left: 25.0,
                top: 110.0,
                right: 30.0,
            },
            CharInfo {
                c: "1".to_string(),
                bottom: 100.0,
                left: 30.0,
                top: 110.0,
                right: 35.0,
            },
            CharInfo {
                c: "W".to_string(),
                bottom: 100.0,
                left: 45.0,
                top: 110.0,
                right: 50.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 100.0,
                left: 50.0,
                top: 110.0,
                right: 55.0,
            },
            CharInfo {
                c: "r".to_string(),
                bottom: 100.0,
                left: 55.0,
                top: 110.0,
                right: 60.0,
            },
            CharInfo {
                c: "d".to_string(),
                bottom: 100.0,
                left: 60.0,
                top: 110.0,
                right: 65.0,
            },
            CharInfo {
                c: "2".to_string(),
                bottom: 100.0,
                left: 65.0,
                top: 110.0,
                right: 70.0,
            },
        ];
        let result = reconstruct_visual_single_column(chars);
        assert_eq!(result, "Word1 Word2");
    }

    #[test]
    fn test_reconstruct_visual_single_column_right_aligned_toc() {
        // TOC rows often have left text, dot leaders, and right-aligned page numbers.
        // Single-column extraction must keep them on the same line.
        let chars = vec![
            CharInfo {
                c: "1".to_string(),
                bottom: 100.0,
                left: 10.0,
                top: 110.0,
                right: 15.0,
            },
            CharInfo {
                c: " ".to_string(),
                bottom: 100.0,
                left: 15.0,
                top: 110.0,
                right: 16.0,
            },
            CharInfo {
                c: "I".to_string(),
                bottom: 100.0,
                left: 16.0,
                top: 110.0,
                right: 20.0,
            },
            CharInfo {
                c: "n".to_string(),
                bottom: 100.0,
                left: 20.0,
                top: 110.0,
                right: 25.0,
            },
            CharInfo {
                c: "t".to_string(),
                bottom: 100.0,
                left: 25.0,
                top: 110.0,
                right: 30.0,
            },
            CharInfo {
                c: "r".to_string(),
                bottom: 100.0,
                left: 30.0,
                top: 110.0,
                right: 35.0,
            },
            CharInfo {
                c: "o".to_string(),
                bottom: 100.0,
                left: 35.0,
                top: 110.0,
                right: 40.0,
            },
            // The wide gap stands in for dot leaders before a right-aligned page number.
            CharInfo {
                c: "1".to_string(),
                bottom: 100.0,
                left: 300.0,
                top: 110.0,
                right: 305.0,
            },
        ];
        let result = reconstruct_visual_single_column(chars);
        assert_eq!(result, "1 Intro 1");
    }

    #[test]
    fn test_two_column_strategy_is_not_default() {
        assert_eq!(
            PdfEmbeddedTextStrategy::default(),
            PdfEmbeddedTextStrategy::PdfiumFlat
        );
    }

    /// Skip this test if PDFium is not available on the current platform.
    macro_rules! require_pdfium {
        () => {
            if !crate::pdf_ocr::pdfium_available() {
                eprintln!("skipping PDFium-dependent test: PDFium library is not available");
                return;
            }
        };
    }

    #[test]
    fn test_extract_empty_pdf() {
        require_pdfium!();
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary!(
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
        ));

        let pages = dictionary!(
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary!(
            "Type" => "Catalog",
            "Pages" => pages_id
        ));
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let result = extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert_eq!(result.text, "");
        assert_eq!(result.page_count, 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("no extractable text"))
        );
    }

    #[test]
    fn test_extract_invalid_pdf() {
        let invalid = b"This is not a PDF file.";
        let result = extract_pdf(invalid, None, PdfExtractionOptions::raw_default());
        assert!(matches!(result, Err(PdfExtractionError::InvalidFormat(_))));
    }

    #[test]
    fn lopdf_fallback_accepts_readable_text_with_warning() {
        let text = "This is readable embedded text from a small synthetic PDF. It has enough ordinary words to be treated as usable fallback output.".to_string();

        let extracted = finalize_lopdf_fallback_text(text.clone(), 1, None, Vec::new());

        assert_eq!(extracted.text, text);
        assert!(
            extracted
                .warnings
                .iter()
                .any(|warning| warning.contains("PDF backend: lopdf fallback"))
        );
        assert!(
            extracted
                .warnings
                .iter()
                .any(|warning| warning.contains("used degraded lopdf"))
        );
    }

    #[test]
    fn lopdf_fallback_suppresses_low_quality_garbage() {
        let text = "!\" #$%\n&\"'\"\n\"! () \" *\n(+ ,- .( /01( #".to_string();

        let extracted = finalize_lopdf_fallback_text(text, 1, None, Vec::new());

        assert!(extracted.text.is_empty());
        assert!(
            extracted
                .warnings
                .iter()
                .any(|warning| warning.contains("low-quality text"))
        );
    }

    #[test]
    fn test_extract_valid_pdf() {
        require_pdfium!();
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary!(
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica"
        ));

        let resources_id = doc.add_object(dictionary!(
            "Font" => dictionary!(
                "F1" => font_id
            )
        ));

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![100.into(), 100.into()]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        b"Hello World".to_vec(),
                        StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        };

        let content_id = doc.add_object(Stream::new(dictionary!(), content.encode().unwrap()));

        let page_id = doc.add_object(dictionary!(
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
        ));

        let pages = dictionary!(
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary!(
            "Type" => "Catalog",
            "Pages" => pages_id
        ));

        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let result = extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert_eq!(result.text.trim(), "Hello World");
        assert_eq!(result.page_count, 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("PDF reading order is not guaranteed"))
        );
    }

    #[test]
    fn test_is_page_label() {
        assert!(is_page_label("1"));
        assert!(is_page_label("12"));
        assert!(is_page_label("104"));
        assert!(is_page_label("Page 12"));
        assert!(is_page_label("page 12"));
        assert!(is_page_label("p. 12"));
        assert!(is_page_label("- 12 -"));
        assert!(is_page_label("— 12 —"));
        assert!(is_page_label("12 / 40"));
        assert!(is_page_label("Page 12 of 40"));
        assert!(is_page_label("i"));
        assert!(is_page_label("iv"));
        assert!(is_page_label("xii"));
        assert!(is_page_label("Page iv"));

        assert!(!is_page_label(""));
        assert!(!is_page_label("12 apples"));
        assert!(!is_page_label("Chapter 12"));
        assert!(!is_page_label("dim"));
        assert!(!is_page_label("mid"));
        assert!(!is_page_label("This is page 12 of the book"));
    }

    #[test]
    fn test_normalize_candidate_line() {
        assert_eq!(normalize_candidate_line("Page 1"), "page #");
        assert_eq!(normalize_candidate_line("- 12 -"), "#");
        assert_eq!(normalize_candidate_line("Chapter 2"), "chapter #");
        assert_eq!(normalize_candidate_line("  Introduction  "), "introduction");
    }

    fn create_multipage_pdf(pages_content: &[Vec<&str>]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary!(
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica"
        ));

        let resources_id = doc.add_object(dictionary!(
            "Font" => dictionary!(
                "F1" => font_id
            )
        ));

        let mut kids = Vec::new();
        for page_lines in pages_content {
            let mut ops = vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
            ];

            let mut first = true;
            for line in page_lines {
                if first {
                    ops.push(Operation::new("Td", vec![100.into(), 750.into()]));
                    first = false;
                } else {
                    ops.push(Operation::new("Td", vec![0.into(), (-30.0).into()]));
                }
                ops.push(Operation::new(
                    "Tj",
                    vec![Object::String(
                        line.as_bytes().to_vec(),
                        StringFormat::Literal,
                    )],
                ));
            }
            ops.push(Operation::new("ET", vec![]));

            let content = Content { operations: ops };
            let content_id = doc.add_object(Stream::new(dictionary!(), content.encode().unwrap()));

            let page_id = doc.add_object(dictionary!(
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()]
            ));
            kids.push(page_id.into());
        }

        let pages = dictionary!(
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages_content.len() as i32
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog_id = doc.add_object(dictionary!(
            "Type" => "Catalog",
            "Pages" => pages_id
        ));
        doc.trailer.set("Root", catalog_id);

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn test_pdf_page_range_extracts_embedded_pages() {
        require_pdfium!();
        let bytes = create_multipage_pdf(&[
            vec!["Page one body"],
            vec!["Page two body"],
            vec!["Page three body"],
        ]);

        let result =
            extract_pdf_page_range(&bytes, 1, 2, PdfExtractionOptions::raw_default(), None)
                .unwrap();

        assert_eq!(result.page_count, 3);
        assert_eq!(result.start_page_index, 1);
        assert_eq!(result.end_page_index, 3);
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].page_index, 1);
        assert_eq!(result.pages[0].page_number, 2);
        assert_eq!(result.pages[0].method, PdfPageExtractionMethod::Embedded);
        assert!(result.pages[0].text.contains("Page two body"));
        assert!(result.pages[0].error.is_none());
        assert!(result.pages[1].text.contains("Page three body"));
    }

    #[test]
    fn test_pdf_page_range_caps_page_text() {
        require_pdfium!();
        let bytes = create_multipage_pdf(&[vec!["abcdef"]]);

        let result =
            extract_pdf_page_range(&bytes, 0, 1, PdfExtractionOptions::raw_default(), Some(3))
                .unwrap();

        assert_eq!(result.pages[0].text.chars().count(), 3);
        assert_eq!(result.pages[0].char_count, 3);
        assert!(
            result.pages[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("truncated"))
        );
    }

    #[test]
    fn test_pdf_cleanup_repeated_headers_footers() {
        require_pdfium!();
        let pages = vec![
            vec![
                "Repeated Header",
                "Top 1-2",
                "Top 1-3",
                "Body Line 1",
                "Bottom 1-1",
                "Bottom 1-2",
                "Repeated Footer",
            ],
            vec![
                "Repeated Header",
                "Top 2-2",
                "Top 2-3",
                "Body Line 2",
                "Bottom 2-1",
                "Bottom 2-2",
                "Repeated Footer",
            ],
            vec![
                "Repeated Header",
                "Top 3-2",
                "Top 3-3",
                "Body Line 3",
                "Bottom 3-1",
                "Bottom 3-2",
                "Repeated Footer",
            ],
            vec![
                "Repeated Header",
                "Top 4-2",
                "Top 4-3",
                "Body Line 4",
                "Bottom 4-1",
                "Bottom 4-2",
                "Repeated Footer",
            ],
        ];

        let bytes = create_multipage_pdf(&pages);

        let result_disabled =
            extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert!(result_disabled.text.contains("Repeated Header"));
        assert!(result_disabled.text.contains("Repeated Footer"));
        assert!(
            !result_disabled
                .warnings
                .iter()
                .any(|w| w.contains("Removed repeated PDF header/footer"))
        );

        let result_enabled = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_repeated_headers_footers: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(!result_enabled.text.contains("Repeated Header"));
        assert!(!result_enabled.text.contains("Repeated Footer"));
        assert!(result_enabled.text.contains("Body Line 1"));
        assert!(
            result_enabled
                .warnings
                .iter()
                .any(|w| w.contains("Removed repeated PDF header/footer"))
        );
    }

    #[test]
    fn test_pdf_cleanup_body_line_preserved() {
        require_pdfium!();
        // Body-zone repeats are not header/footer candidates.
        let pages = vec![
            vec![
                "Top 1",
                "Top 2",
                "Top 3",
                "Repeated Body",
                "Bottom 1",
                "Bottom 2",
                "Bottom 3",
            ],
            vec![
                "Top 1",
                "Top 2",
                "Top 3",
                "Repeated Body",
                "Bottom 1",
                "Bottom 2",
                "Bottom 3",
            ],
            vec![
                "Top 1",
                "Top 2",
                "Top 3",
                "Repeated Body",
                "Bottom 1",
                "Bottom 2",
                "Bottom 3",
            ],
            vec![
                "Top 1",
                "Top 2",
                "Top 3",
                "Repeated Body",
                "Bottom 1",
                "Bottom 2",
                "Bottom 3",
            ],
        ];

        let bytes = create_multipage_pdf(&pages);

        let result = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_repeated_headers_footers: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(result.text.contains("Repeated Body"));
    }

    #[test]
    fn test_pdf_cleanup_page_labels() {
        require_pdfium!();
        let pages = vec![
            vec!["Header", "Some text in the page body.", "1"],
            vec!["Header", "Some text in the page body.", "Page 2"],
            vec!["Header", "Some text in the page body.", "- 3 -"],
            vec!["Header", "Some text in the page body.", "page iv"],
        ];

        let bytes = create_multipage_pdf(&pages);

        let result_disabled =
            extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert!(result_disabled.text.contains("1"));
        assert!(result_disabled.text.contains("Page 2"));
        assert!(result_disabled.text.contains("- 3 -"));
        assert!(result_disabled.text.contains("page iv"));

        let result_enabled = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_page_labels: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(!result_enabled.text.contains("\n1\n") && !result_enabled.text.ends_with("\n1"));
        assert!(!result_enabled.text.contains("Page 2"));
        assert!(!result_enabled.text.contains("- 3 -"));
        assert!(!result_enabled.text.contains("page iv"));
        assert!(result_enabled.text.contains("Some text in the page body."));
    }

    #[test]
    fn test_pdf_cleanup_short_pdf_conservative() {
        require_pdfium!();
        let pages = vec![
            vec![
                "Repeated Header",
                "Body Line 1",
                "Footer Line 1",
                "Repeated Footer",
            ],
            vec![
                "Repeated Header",
                "Body Line 2",
                "Footer Line 2",
                "Repeated Footer",
            ],
        ];

        let bytes = create_multipage_pdf(&pages);

        // Fewer than 3 pages is too little evidence for repeated header/footer removal.
        let result = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_repeated_headers_footers: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(result.text.contains("Repeated Header"));
        assert!(result.text.contains("Repeated Footer"));
    }

    #[test]
    fn test_is_symbol_heavy_artifact() {
        assert!(is_symbol_heavy_artifact("● ● ● ● ● ● ● ● ● ● ● ● ●"));
        assert!(is_symbol_heavy_artifact("● ● ● ●"));
        assert!(is_symbol_heavy_artifact("---------"));
        assert!(is_symbol_heavy_artifact("********"));
        assert!(is_symbol_heavy_artifact("●●●●●●●●●●●●●●"));

        assert!(!is_symbol_heavy_artifact("y = a + bx"));
        assert!(!is_symbol_heavy_artifact("non-parametric / parametric"));
        assert!(!is_symbol_heavy_artifact("C:\\Users\\example\\file.txt"));
        assert!(!is_symbol_heavy_artifact("x + y / z"));

        assert!(!is_symbol_heavy_artifact("Intercept, a"));
        assert!(!is_symbol_heavy_artifact("35 40 45 50 55 60"));
        assert!(!is_symbol_heavy_artifact("• This is a real bullet item."));
        assert!(!is_symbol_heavy_artifact(
            "Figure 9.6 100 posterior simulations"
        ));
        assert!(is_symbol_heavy_artifact("● ● ●")); // short pure graphical markers
        assert!(is_symbol_heavy_artifact("●"));
        assert!(is_symbol_heavy_artifact("● ●"));
        assert!(is_symbol_heavy_artifact("● + ●")); // composed entirely of noise symbols
        assert!(!is_symbol_heavy_artifact("● / ●")); // short, but contains non-noise symbol '/'
        assert!(!is_symbol_heavy_artifact("{ }")); // braces are preserved
        assert!(!is_symbol_heavy_artifact("[ ]")); // brackets are preserved
    }

    #[test]
    fn test_pdf_cleanup_symbol_heavy_artifacts() {
        require_pdfium!();
        let pages = vec![
            vec![
                "* * * * * * * * *",
                "* * * * * * * * *",
                "Body Line 1",
                "More prose text.",
            ],
            vec![
                "Another Body Line",
                "* * * * * * * * *",
                "* * * * * * * * *",
            ],
        ];

        let bytes = create_multipage_pdf(&pages);

        let result_disabled =
            extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert!(result_disabled.text.contains("* * * * * * * * *"));
        assert!(
            result_disabled
                .warnings
                .iter()
                .all(|w| !w.contains("symbol-heavy PDF graphical artefact"))
        );

        let result_enabled = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_symbol_heavy_artifacts: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(!result_enabled.text.contains("* * * * * * * * *"));
        assert!(result_enabled.text.contains("Body Line 1"));
        assert!(result_enabled.text.contains("Another Body Line"));

        let warning = result_enabled
            .warnings
            .iter()
            .find(|w| w.contains("symbol-heavy PDF"))
            .unwrap();
        assert!(warning.contains("Removed 4 symbol-heavy PDF graphical artefact lines across 2 pages. Example pages: 1, 2."));
    }

    #[test]
    fn test_pdf_cleanup_code_block_removal() {
        require_pdfium!();
        let pages = vec![
            vec![
                "The null hypothesis H0 is rejected.",
                "The function returns a value.",
                "This function returns a vector.",
                "The model y = a + bx is simple.",
                "non-parametric / parametric",
                "p-values are often misunderstood.",
                "library(dplyr)",
                "data <- read.csv(\"data.csv\")",
                "data |> filter(group == \"A\")",
                "Some final prose words in a sentence.",
            ],
            vec![
                "Ordinary header",
                "y = a + bx",
                "model <- lm(y ~ x, data = df)",
                "summary(model)",
                "plot(x, y)",
                "This function returns a value.",
            ],
        ];
        let bytes = create_multipage_pdf(&pages);

        let result_disabled =
            extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert!(result_disabled.text.contains("library(dplyr)"));
        assert!(result_disabled.text.contains("data <- read.csv"));
        assert!(result_disabled.text.contains("model <- lm"));
        assert!(result_disabled.text.contains("summary(model)"));
        assert!(
            !result_disabled
                .warnings
                .iter()
                .any(|w| w.contains("code-like"))
        );

        let result_enabled = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_code_like_blocks: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(
            result_enabled
                .text
                .contains("The null hypothesis H0 is rejected.")
        );
        assert!(
            result_enabled
                .text
                .contains("The function returns a value.")
        );
        assert!(
            result_enabled
                .text
                .contains("This function returns a vector.")
        );
        assert!(
            result_enabled
                .text
                .contains("The model y = a + bx is simple.")
        );
        assert!(result_enabled.text.contains("non-parametric / parametric"));
        assert!(
            result_enabled
                .text
                .contains("p-values are often misunderstood.")
        );
        assert!(
            result_enabled
                .text
                .contains("Some final prose words in a sentence.")
        );
        assert!(result_enabled.text.contains("y = a + bx")); // formula preserved when formula option disabled!

        assert!(!result_enabled.text.contains("library(dplyr)"));
        assert!(!result_enabled.text.contains("data <- read.csv"));
        assert!(!result_enabled.text.contains("model <- lm"));
        assert!(!result_enabled.text.contains("summary(model)"));
        assert!(!result_enabled.text.contains("plot(x, y)"));

        let warning = result_enabled
            .warnings
            .iter()
            .find(|w| w.contains("code-like"))
            .unwrap();
        assert!(
            warning.contains("Removed 2 PDF code-like blocks containing 6 lines across 2 pages.")
        );
    }

    #[test]
    fn test_pdf_cleanup_formula_removal() {
        require_pdfium!();
        let pages = vec![vec![
            "The null hypothesis H0 is rejected.",
            "The model y = a + bx is simple.",
            "p-values are often misunderstood.",
            "non-parametric / parametric",
            "y = a + bx",
            "H0: β1 = 0",
            "E[X] = μ",
            "logit(p) = β0 + β1x",
            "model <- lm(y ~ x, data = df)",
            "summary(model)",
        ]];
        let bytes = create_multipage_pdf(&pages);

        let result_disabled =
            extract_pdf(&bytes, None, PdfExtractionOptions::raw_default()).unwrap();
        assert!(result_disabled.text.contains("y = a + bx"));
        assert!(result_disabled.text.contains("E[X] = ˛…"));
        assert!(result_disabled.text.contains("logit(p) = ˛†0 + ˛†1x"));
        assert!(
            !result_disabled
                .warnings
                .iter()
                .any(|w| w.contains("formula-like"))
        );

        let result_enabled = extract_pdf(
            &bytes,
            None,
            PdfExtractionOptions {
                remove_formula_like_lines: true,
                ..PdfExtractionOptions::raw_default()
            },
        )
        .unwrap();
        assert!(
            result_enabled
                .text
                .contains("The null hypothesis H0 is rejected.")
        );
        assert!(
            result_enabled
                .text
                .contains("The model y = a + bx is simple.")
        );
        assert!(
            result_enabled
                .text
                .contains("p-values are often misunderstood.")
        );
        assert!(result_enabled.text.contains("non-parametric / parametric"));
        assert!(
            result_enabled
                .text
                .contains("model <- lm(y ~ x, data = df)")
        ); // code block preserved when code option disabled!

        assert!(
            !result_enabled
                .text
                .lines()
                .any(|l| l.trim() == "y = a + bx")
        );
        assert!(
            !result_enabled
                .text
                .lines()
                .any(|l| l.trim() == "H0: ˛†1 = 0")
        );
        assert!(!result_enabled.text.lines().any(|l| l.trim() == "E[X] = ˛…"));
        assert!(
            !result_enabled
                .text
                .lines()
                .any(|l| l.trim() == "logit(p) = ˛†0 + ˛†1x")
        );

        let warning = result_enabled
            .warnings
            .iter()
            .find(|w| w.contains("formula-like"))
            .unwrap();
        assert!(warning.contains("Removed 4 PDF formula-like lines across 1 pages."));
    }
}
