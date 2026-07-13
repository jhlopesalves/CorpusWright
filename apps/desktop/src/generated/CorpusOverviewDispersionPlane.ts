import type { DispersionWitness } from "./DispersionWitness.js";

export type CorpusOverviewDispersionPlane = { xBins: number, yBins: number, freqLog10Max: number, counts: Array<number>, maxCellCount: number, witnesses: Array<DispersionWitness>, loudButLocalCount: number, loudButLocalMinFrequency: number, highDispersionThreshold: number, reading: string, };
