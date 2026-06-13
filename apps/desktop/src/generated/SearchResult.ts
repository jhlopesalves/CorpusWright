import type { SearchHit } from "./SearchHit.js";

export type SearchResult = {
/**
 * Total number of matches across all searched files (may exceed returned_hits).
 */
total_matches: number,
/**
 * Indices (into the records slice) of files that contain at least one match.
 */
matching_file_indices: Array<number>,
/**
 * Number of SearchHit structs actually returned (capped by max_hits).
 */
returned_hits: number,
/**
 * True if total_matches > returned_hits.
 */
truncated: boolean,
/**
 * Bounded list of navigable hits.
 */
hits: Array<SearchHit>, };
