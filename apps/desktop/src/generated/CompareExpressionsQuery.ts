import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";

export type CompareExpressionsQuery = { selection: AnalysisDocumentSelection, expressions: Array<string>, caseSensitive: boolean, minDocumentTokens: number, offset: number, limit: number, };
