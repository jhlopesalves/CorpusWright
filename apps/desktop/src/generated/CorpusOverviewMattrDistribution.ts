import type { MattrWitness } from "./MattrWitness.js";

export type CorpusOverviewMattrDistribution = { binWidth: number, counts: Array<number>, usedDocuments: number, excludedDocuments: number, lowWitnesses: Array<MattrWitness>, highWitnesses: Array<MattrWitness>, reading: string, };
