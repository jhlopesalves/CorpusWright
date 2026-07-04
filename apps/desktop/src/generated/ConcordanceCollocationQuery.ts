import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { ConcordanceCollocationDirection } from "./ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure } from "./ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationSortKey } from "./ConcordanceCollocationSortKey.js";
import type { SortDirection } from "./SortDirection.js";

export type ConcordanceCollocationQuery = { selection: AnalysisDocumentSelection, query: string, language: string | null, caseSensitive: boolean, span: number, direction: ConcordanceCollocationDirection, minCooccurrence: number, minCollocateFrequency: number, hideStopwords: boolean, sortMeasure: ConcordanceCollocationMeasure, sortKey: ConcordanceCollocationSortKey, sortDirection: SortDirection, limit: number, };
