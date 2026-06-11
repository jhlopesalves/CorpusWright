export type RepeatedArtifactScanConfig = { 
/**
 * Use processed (cleaned) text instead of original extracted text.
 * Processed scans may be slower because they apply current extraction and cleanup settings.
 */
analyse_processed_text: boolean, include_exact_lines: boolean, include_normalized_lines: boolean, 
/**
 * Detect repeated inline markup/conversion tokens (e.g. `<br/>`, `&nbsp;`).
 * Bounded known-pattern scanning; enabled by default.
 */
include_inline_artifacts: boolean, 
/**
 * 2-line block detection is more expensive; disabled by default. Opt-in only.
 */
include_two_line_blocks: boolean, 
/**
 * 3-line block detection is more expensive; disabled by default. Opt-in only.
 */
include_three_line_blocks: boolean, 
/**
 * Include candidates whose content is predominantly text.
 */
include_text_dominant: boolean, 
/**
 * Include candidates with a mix of text and numbers (e.g. "Page 12", "Chapter 5").
 */
include_mixed_text_numbers: boolean, 
/**
 * Include numeric-dominant candidates (risky — may group unrelated statistical output).
 */
include_numeric_dominant: boolean, 
/**
 * Include symbol/noise-dominant candidates (extraction junk markers).
 */
include_symbol_noise: boolean, min_occurrences: number, min_files: number, max_candidates: number, max_examples_per_candidate: number, min_line_chars: number, max_line_chars: number, };
