import type { CumulativeCoveragePoint } from "./CumulativeCoveragePoint.js";
import type { CoverageTargetCrossing } from "./CoverageTargetCrossing.js";

export type CorpusOverviewFrequencyConcentration = { top10Coverage: number | null, top100Coverage: number | null, top1000Coverage: number | null, top10000Coverage: number | null, cumulativeCoverage: Array<CumulativeCoveragePoint>, exactTargetCrossings: Array<CoverageTargetCrossing>, shannonEntropyBits: number | null, evennessJ: number | null, effectiveVocabularyEntropy: number | null, reading: string, };
