import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { ConcordanceContextFilterDirection } from "./ConcordanceContextFilterDirection.js";
import type { ConcordanceSortKey } from "./ConcordanceSortKey.js";

export type ConcordanceQuery = { selection: AnalysisDocumentSelection, query: string, contextFilter: string | null, contextFilterDirection: ConcordanceContextFilterDirection, caseSensitive: boolean, sortKey: ConcordanceSortKey, windowTokens: number, offset: number, limit: number, includeRows: boolean, distributionOffset: number, distributionLimit: number, includeDistribution: boolean, };
