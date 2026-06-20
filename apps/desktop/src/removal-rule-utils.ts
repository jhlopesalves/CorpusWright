import type { RemovalRule } from "./generated/RemovalRule.js";

export function removalRuleDuplicateKey(rule: RemovalRule): string {
  if (rule.matcher.kind === "literal") {
    return `${rule.scope}\0${rule.matcher.text}`;
  }
  return `${rule.scope}\0normalized_line\0${rule.matcher.normalized_key}`;
}

function cloneRule(rule: RemovalRule): RemovalRule {
  return { ...rule, matcher: { ...rule.matcher } };
}

export function mergeEffectiveRemovalRules(
  baseRules: RemovalRule[],
  autoRules: RemovalRule[],
  enabled: boolean
): RemovalRule[] {
  const merged = baseRules.map(cloneRule);
  if (!enabled) return merged;

  const ruleIds = new Set(merged.map(rule => rule.id));
  const ruleKeys = new Set(merged.map(removalRuleDuplicateKey));
  for (const rule of [...autoRules].sort((left, right) => (
    left.id < right.id ? -1 : left.id > right.id ? 1 : 0
  ))) {
    const key = removalRuleDuplicateKey(rule);
    if (ruleIds.has(rule.id) || ruleKeys.has(key)) continue;
    merged.push(cloneRule(rule));
    ruleIds.add(rule.id);
    ruleKeys.add(key);
  }
  return merged;
}
