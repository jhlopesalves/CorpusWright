import type { RankFrequencyPoint } from "./RankFrequencyPoint.js";

export type CorpusOverviewRankFrequency = { zipfExponent: number | null, zipfCoefficient: number | null, rSquared: number | null, fittedRankStart: number | null, fittedRankEnd: number | null, points: Array<RankFrequencyPoint>, reading: string, };
