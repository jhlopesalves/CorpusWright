import type { ConcordancePhase } from "./ConcordancePhase.js";

export type ConcordanceProgress = { filesDone: number, filesTotal: number, currentPhase: ConcordancePhase, };
