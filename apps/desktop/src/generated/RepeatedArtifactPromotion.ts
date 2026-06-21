import type { RepeatedArtifactMatcherKind } from "./RepeatedArtifactMatcherKind.js";
import type { RemovalScope } from "./RemovalScope.js";
import type { RemovalRule } from "./RemovalRule.js";

export type RepeatedArtifactPromotion = { candidate_id: string, rule_text: string, matcher_kind: RepeatedArtifactMatcherKind, scope: RemovalScope, rule_id: string, can_promote: boolean, rule: RemovalRule | null, };
