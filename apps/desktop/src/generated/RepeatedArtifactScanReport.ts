import type { RepeatedArtifactCandidate } from "./RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanDiagnostics } from "./RepeatedArtifactScanDiagnostics.js";

export type RepeatedArtifactScanReport = { candidates: Array<RepeatedArtifactCandidate>, diagnostics: RepeatedArtifactScanDiagnostics, };
