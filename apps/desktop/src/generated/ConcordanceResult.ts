import type { ConcordanceSortKey } from "./ConcordanceSortKey.js";
import type { ConcordanceRow } from "./ConcordanceRow.js";
import type { ConcordanceDistributionPage } from "./ConcordanceDistributionPage.js";

export type ConcordanceResult = { query: string, contextFilter: string | null, caseSensitive: boolean, sortKey: ConcordanceSortKey, windowTokens: number, totalHits: number, documentsWithHits: number, selectedDocumentsTotal: number, selectedTokensTotal: number, dp: number, dispersionLabel: string, topDocumentShare: number, offset: number, limit: number, rows: Array<ConcordanceRow> | null, distribution: ConcordanceDistributionPage | null, };
