import type { RemovalRule } from "./generated/RemovalRule.js";

export function removalRuleDuplicateKey(rule: RemovalRule): string {
  if (rule.matcher.kind === "literal") {
    return `${rule.scope}\0${rule.matcher.text}`;
  }
  return `${rule.scope}\0normalized_line\0${rule.matcher.normalized_key}`;
}

export function cloneRemovalRules(rules: readonly RemovalRule[]): RemovalRule[] {
  return rules.map(rule => ({
    ...rule,
    matcher: { ...rule.matcher },
  }));
}
