import type { ConcordanceCollocatePositionCounts } from "./ConcordanceCollocatePositionCounts.js";

export type ConcordanceCollocateRow = { surface: string, o11: number, o12: number, o21: number, o22: number, collocateFrequency: number, positionCounts: ConcordanceCollocatePositionCounts, mi: number | null, mi3: number | null, tScore: number | null, zScore: number | null, g2: number | null, logDice: number | null, logRatio: number | null, };
