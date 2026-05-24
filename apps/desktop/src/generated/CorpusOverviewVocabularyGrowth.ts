import type { VocabularyGrowthPoint } from "./VocabularyGrowthPoint.js";

export type CorpusOverviewVocabularyGrowth = { heapsK: number | null, heapsBeta: number | null, rSquared: number | null, points: Array<VocabularyGrowthPoint>, reading: string, };
