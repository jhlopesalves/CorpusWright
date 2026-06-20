import type { RepeatedArtifactCandidate } from "./RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanDiagnostics } from "./RepeatedArtifactScanDiagnostics.js";
import type { RepeatedArtifactPromotion } from "./RepeatedArtifactPromotion.js";
import type { AutoPromotedRepeatedArtifactRule } from "./AutoPromotedRepeatedArtifactRule.js";

export type RepeatedArtifactScanReport = { candidates: Array<RepeatedArtifactCandidate>, promotions: Array<RepeatedArtifactPromotion>, auto_promoted_rules: Array<AutoPromotedRepeatedArtifactRule>, diagnostics: RepeatedArtifactScanDiagnostics, };
