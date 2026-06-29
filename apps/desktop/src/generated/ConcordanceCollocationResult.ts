import type { ConcordanceCollocationDirection } from "./ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure } from "./ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationSortKey } from "./ConcordanceCollocationSortKey.js";
import type { SortDirection } from "./SortDirection.js";
import type { ConcordanceCollocateRow } from "./ConcordanceCollocateRow.js";

export type ConcordanceCollocationResult = { query: string, caseSensitive: boolean, span: number, direction: ConcordanceCollocationDirection, minCooccurrence: number, minCollocateFrequency: number, hideStopwords: boolean, sortMeasure: ConcordanceCollocationMeasure, sortKey: ConcordanceCollocationSortKey, sortDirection: SortDirection, limit: number, r1: number, n: number, fNode: number, totalCandidates: number, rows: Array<ConcordanceCollocateRow>, };
