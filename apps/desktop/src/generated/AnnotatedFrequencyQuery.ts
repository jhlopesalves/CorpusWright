import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { AnnotatedFrequencyMode } from "./AnnotatedFrequencyMode.js";
import type { FrequencyMatchMode } from "./FrequencyMatchMode.js";
import type { FrequencySortKey } from "./FrequencySortKey.js";
import type { SortDirection } from "./SortDirection.js";

export type AnnotatedFrequencyQuery = { selection: AnalysisDocumentSelection, mode: AnnotatedFrequencyMode, query: string, language: string | null, matchMode: FrequencyMatchMode, minimumFrequency: number, hideStopwords: boolean, caseFold: boolean, posFilter: string | null, sortKey: FrequencySortKey, sortDirection: SortDirection, offset: number, limit: number, };
