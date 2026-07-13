import type { LengthHistogramBin } from "./LengthHistogramBin.js";

export type CorpusOverviewBalance = { largestShare: number | null, top3Share: number | null, gini: number | null, coefficientOfVariation: number | null, effectiveDocuments: number | null, medianTokens: number | null, minimumTokens: number | null, maximumTokens: number | null, lengthHistogram: Array<LengthHistogramBin>, remainingDocuments: number, reading: string, };
