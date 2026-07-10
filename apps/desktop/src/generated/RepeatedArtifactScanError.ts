import type { RepeatedArtifactMemoryStage } from "./RepeatedArtifactMemoryStage.js";

export type RepeatedArtifactScanError = { "type": "cancelled" } | { "type": "corpusTooLarge", stage: RepeatedArtifactMemoryStage, ceiling_bytes: number, estimated_bytes: number, suggestions: Array<string>, } | { "type": "scanFailed", message: string, };
