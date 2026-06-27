import type { ConcordanceRow } from "./ConcordanceRow.js";

export type ConcordanceResult = { query: string, caseSensitive: boolean, windowTokens: number, totalHits: number, documentsWithHits: number, offset: number, limit: number, rows: Array<ConcordanceRow>, };
