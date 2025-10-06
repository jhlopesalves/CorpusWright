export type SearchHit = { 
/**
 * Index into the corpus records array.
 */
corpus_index: number, 
/**
 * Relative path of the containing file.
 */
relative_path: string, 
/**
 * Full source path, if available.
 */
source_path: string | null, 
/**
 * Up to ~CONTEXT_CHARS chars of text before the match.
 */
context_before: string, 
/**
 * The exact matched substring.
 */
match_text: string, 
/**
 * Up to ~CONTEXT_CHARS chars of text after the match.
 */
context_after: string, 
/**
 * 0-based index of this match within its file.
 */
file_match_index: number, };
