import type { CompareExpressionSummary } from "./CompareExpressionSummary.js";
import type { CompareDocumentRow } from "./CompareDocumentRow.js";

export type CompareExpressionsResult = { expressionsQueried: Array<string>, caseSensitive: boolean, minDocumentTokens: number, selectedDocumentsTotal: number, selectedTokensTotal: number, expressions: Array<CompareExpressionSummary>, totalDocuments: number, offset: number, limit: number, documents: Array<CompareDocumentRow>, };
