import type { CorpusOverviewFindingKind } from "./CorpusOverviewFindingKind.js";
import type { CorpusOverviewFindingSeverity } from "./CorpusOverviewFindingSeverity.js";

export type CorpusOverviewFinding = { key: CorpusOverviewFindingKind, severity: CorpusOverviewFindingSeverity, severityLabel: string, text: string, };
