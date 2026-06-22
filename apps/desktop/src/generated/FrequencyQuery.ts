import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { FrequencyMatchMode } from "./FrequencyMatchMode.js";
import type { FrequencyTokenClass } from "./FrequencyTokenClass.js";
import type { FrequencyGroupMode } from "./FrequencyGroupMode.js";
import type { FrequencySortKey } from "./FrequencySortKey.js";
import type { SortDirection } from "./SortDirection.js";

export type FrequencyQuery = { selection: AnalysisDocumentSelection, query: string, matchMode: FrequencyMatchMode, minimumFrequency: number, tokenClass: FrequencyTokenClass, hideStopwords: boolean, groupBy: FrequencyGroupMode, caseFold: boolean, language: string | null, sortKey: FrequencySortKey, sortDirection: SortDirection, offset: number, limit: number, };
