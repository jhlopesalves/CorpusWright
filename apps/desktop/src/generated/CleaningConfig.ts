import type { ReplacementRule } from "./ReplacementRule.js";
import type { RemovalRule } from "./RemovalRule.js";
import type { TableExtractionStrategy } from "./TableExtractionStrategy.js";
import type { PdfEmbeddedTextStrategy } from "./PdfEmbeddedTextStrategy.js";
import type { PdfTextSource } from "./PdfTextSource.js";
import type { PdfOcrQuality } from "./PdfOcrQuality.js";

export type { ReplacementRule, RemovalRule, TableExtractionStrategy, PdfEmbeddedTextStrategy, PdfTextSource, PdfOcrQuality };

export type CleaningConfig = { join_line_breaks: boolean, normalize_irregular_line_breaks: boolean, remove_standalone_page_numbers: boolean, remove_standalone_roman_page_numbers: boolean, remove_page_indicators: boolean, remove_page_delimiters: boolean, lowercase: boolean, trim_lines: boolean, collapse_blank_lines: boolean, normalize_line_endings: boolean, normalize_unicode: boolean, replace_diacritics: boolean, extract_html: boolean,
/**
 * Remove full lines that are overwhelmingly obvious extraction/OCR/markup noise.
 */
remove_obvious_extraction_noise: boolean, table_extraction_strategy: TableExtractionStrategy, remove_headers: boolean, remove_footers: boolean, remove_footnotes: boolean, remove_endnotes: boolean, remove_comments: boolean, remove_table_of_contents: boolean, remove_patterns: Array<string>, removal_rules: Array<RemovalRule>, replace_patterns: Array<ReplacementRule>,
/**
 * PDF text source used before normal text cleaning is applied.
 */
pdf_text_source: PdfTextSource,
/**
 * OCR render quality used when PDF OCR runs.
 */
pdf_ocr_quality: PdfOcrQuality,
/**
 * PDF extraction strategy.
 * NOTE: This is an extraction-layer option specifying how raw PDF text
 * is reconstructed from the character stream, NOT a text-cleaning/sanitization transformation.
 */
pdf_embedded_text_strategy: PdfEmbeddedTextStrategy,
/**
 * PDF-specific post-extraction cleanup option to remove repeated headers and footers across pages.
 */
remove_repeated_pdf_headers_footers: boolean,
/**
 * PDF-specific post-extraction cleanup option to remove page label/page number lines from top/bottom zones.
 */
remove_pdf_page_labels: boolean,
/**
 * PDF-specific post-extraction cleanup option to remove symbol-heavy graphical/plotting noise lines.
 */
remove_pdf_symbol_heavy_artifacts: boolean,
/**
 * PDF-specific post-extraction cleanup option to remove code-like blocks.
 */
remove_pdf_code_like_blocks: boolean,
/**
 * PDF-specific post-extraction cleanup option to remove formula/math-heavy lines.
 */
remove_pdf_formula_like_lines: boolean, };
