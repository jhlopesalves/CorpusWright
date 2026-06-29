import type { ConcordanceSortKey } from "./ConcordanceSortKey.js";
import type { ConcordanceRow } from "./ConcordanceRow.js";
import type { ConcordanceDistributionPage } from "./ConcordanceDistributionPage.js";

export type ConcordanceResult = { query: string, caseSensitive: boolean, sortKey: ConcordanceSortKey, windowTokens: number, totalHits: number, documentsWithHits: number, selectedDocumentsTotal: number, selectedTokensTotal: number, dp: number, dispersionLabel: string, topDocumentShare: number, offset: number, limit: number, rows: Array<ConcordanceRow>, distribution: ConcordanceDistributionPage, };
