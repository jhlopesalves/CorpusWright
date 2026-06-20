import type { AutoPromotedRepeatedArtifactRule } from "./generated/AutoPromotedRepeatedArtifactRule.js";
import type { RemovalRule } from "./generated/RemovalRule.js";
import { removalRuleDuplicateKey } from "./removal-rule-utils.ts";

export function reconcileAutoPromotedRules(
  scannedRules: AutoPromotedRepeatedArtifactRule[],
  manualRules: RemovalRule[],
  dismissedCandidateIds: ReadonlySet<string>,
  enabled: boolean
): RemovalRule[] {
  if (!enabled) return [];

  const manualIds = new Set(manualRules.map(rule => rule.id));
  const manualKeys = new Set(manualRules.map(removalRuleDuplicateKey));
  const reconciled: RemovalRule[] = [];
  const autoIds = new Set<string>();
  const autoKeys = new Set<string>();

  for (const promotion of [...scannedRules].sort((left, right) => (
    left.rule.id < right.rule.id ? -1 : left.rule.id > right.rule.id ? 1 : 0
  ))) {
    if (dismissedCandidateIds.has(promotion.candidate_id)) continue;
    const key = removalRuleDuplicateKey(promotion.rule);
    if (
      manualIds.has(promotion.rule.id) ||
      manualKeys.has(key) ||
      autoIds.has(promotion.rule.id) ||
      autoKeys.has(key)
    ) {
      continue;
    }
    reconciled.push({ ...promotion.rule, matcher: { ...promotion.rule.matcher } });
    autoIds.add(promotion.rule.id);
    autoKeys.add(key);
  }

  return reconciled;
}
