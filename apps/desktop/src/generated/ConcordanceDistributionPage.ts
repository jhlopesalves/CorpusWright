import type { ConcordanceDistributionItem } from "./ConcordanceDistributionItem.js";

export type ConcordanceDistributionPage = { totalDocuments: number, offset: number, limit: number, items: Array<ConcordanceDistributionItem>, maxRatePer10k: number, };
