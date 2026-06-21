import type { RepeatedArtifactCandidate } from "./RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanDiagnostics } from "./RepeatedArtifactScanDiagnostics.js";
import type { RepeatedArtifactPromotion } from "./RepeatedArtifactPromotion.js";

export type RepeatedArtifactScanReport = { candidates: Array<RepeatedArtifactCandidate>, promotions: Array<RepeatedArtifactPromotion>, diagnostics: RepeatedArtifactScanDiagnostics, };
