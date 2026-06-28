import type { TermDistributionItem } from "./TermDistributionItem.js";

export type TermDistributionPage = { token: string, totalDocuments: number, offset: number, limit: number, items: Array<TermDistributionItem>, maxRatePer10k: number, maxRatePerMillion: number, };
