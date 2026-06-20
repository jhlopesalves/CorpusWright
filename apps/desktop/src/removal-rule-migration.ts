import type { RemovalRule } from "./generated/CleaningConfig.js";
import type { RemovalScope } from "./generated/RemovalScope.js";

export type RemovalRuleMigrationReason =
  | "anywhere_literal_interpreted_inline"
  | "normalized_line_anywhere_disabled"
  | "empty_inline_literal_disabled"
  | "multiline_inline_literal_disabled";

export interface RemovalRuleMigrationNotice {
  ruleId: string;
  label: string;
  reason: RemovalRuleMigrationReason;
  message: string;
}

export interface RemovalRuleMigrationSummary {
  inlineLiteralRuleCount: number;
  disabledInvalidRuleCount: number;
  notices: RemovalRuleMigrationNotice[];
}

export interface MigratedRemovalRules {
  rules: RemovalRule[];
  summary: RemovalRuleMigrationSummary;
}

export interface PersistedRepeatedArtifactDecision {
  candidate_signature: string;
  candidate_text: string;
  scope: RemovalScope;
  matcher_kind: "literal" | "normalized_line";
  rule_id: string;
}

export function emptyRemovalRuleMigrationSummary(): RemovalRuleMigrationSummary {
  return {
    inlineLiteralRuleCount: 0,
    disabledInvalidRuleCount: 0,
    notices: [],
  };
}

export function isValidAnywhereLiteralText(text: string): boolean {
  return text.trim().length > 0 && !text.includes("\r") && !text.includes("\n");
}

export function effectivePersistedDecisionScope(
  candidateId: string,
  entry: PersistedRepeatedArtifactDecision
): RemovalScope {
  const inlineLiteral = entry.matcher_kind === "literal" && (
    candidateId.startsWith("inline_") ||
    entry.candidate_signature.startsWith("inline_") ||
    entry.rule_id.includes(":inline_")
  );
  return inlineLiteral ? "anywhere" : entry.scope;
}

export function persistedDecisionRuleId(
  candidateId: string,
  entry: PersistedRepeatedArtifactDecision,
  scope: RemovalScope
): string {
  if (entry.rule_id && entry.scope === scope) return entry.rule_id;
  if (entry.matcher_kind === "normalized_line") {
    return `repeated-artifact:normalized-line:${scope}:${candidateId}`;
  }
  return `repeated-artifact:${scope}:${candidateId}`;
}

export function migrateRepeatedArtifactDecisionRule(
  candidateId: string,
  entry: PersistedRepeatedArtifactDecision
): { rule: RemovalRule | null; summary: RemovalRuleMigrationSummary } {
  const scope = effectivePersistedDecisionScope(candidateId, entry);
  const candidateText = entry.matcher_kind === "normalized_line"
    ? entry.candidate_text.trim()
    : entry.candidate_text;
  if (!candidateText && scope !== "anywhere") {
    return { rule: null, summary: emptyRemovalRuleMigrationSummary() };
  }
  const matcher = entry.matcher_kind === "normalized_line"
    ? { kind: "normalized_line" as const, normalized_key: candidateText }
    : { kind: "literal" as const, text: candidateText };
  const migrated = migrateRemovalRule({
    id: persistedDecisionRuleId(candidateId, entry, scope),
    label: candidateText,
    source: "promoted_repeated_artifact",
    matcher,
    scope,
    enabled: true,
  });
  return { rule: migrated.rule, summary: migrated.summary };
}

function migrationNotice(rule: RemovalRule): RemovalRuleMigrationNotice | null {
  if (rule.scope !== "anywhere") return null;

  if (rule.matcher.kind === "normalized_line") {
    return {
      ruleId: rule.id,
      label: rule.label,
      reason: "normalized_line_anywhere_disabled",
      message: "Normalised-line matchers cannot use Inline literal scope; the rule was disabled.",
    };
  }
  if (rule.matcher.text.trim().length === 0) {
    return {
      ruleId: rule.id,
      label: rule.label,
      reason: "empty_inline_literal_disabled",
      message: "Inline literal text cannot be empty; the rule was disabled.",
    };
  }
  if (!isValidAnywhereLiteralText(rule.matcher.text)) {
    return {
      ruleId: rule.id,
      label: rule.label,
      reason: "multiline_inline_literal_disabled",
      message: "Inline literal text cannot contain line breaks; the rule was disabled.",
    };
  }
  return {
    ruleId: rule.id,
    label: rule.label,
    reason: "anywhere_literal_interpreted_inline",
    message: "Anywhere is interpreted as Inline literal; the rule remains otherwise unchanged.",
  };
}

export function migrateRemovalRule(rule: RemovalRule): {
  rule: RemovalRule;
  summary: RemovalRuleMigrationSummary;
} {
  const migrated: RemovalRule = {
    ...rule,
    matcher: { ...rule.matcher },
  };
  const notice = migrationNotice(migrated);
  const summary = emptyRemovalRuleMigrationSummary();
  if (!notice) return { rule: migrated, summary };

  summary.notices.push(notice);
  if (notice.reason === "anywhere_literal_interpreted_inline") {
    summary.inlineLiteralRuleCount = 1;
  } else {
    migrated.enabled = false;
    summary.disabledInvalidRuleCount = 1;
  }
  return { rule: migrated, summary };
}

export function mergeRemovalRuleMigrationSummaries(
  summaries: Iterable<RemovalRuleMigrationSummary>
): RemovalRuleMigrationSummary {
  const merged = emptyRemovalRuleMigrationSummary();
  for (const summary of summaries) {
    merged.inlineLiteralRuleCount += summary.inlineLiteralRuleCount;
    merged.disabledInvalidRuleCount += summary.disabledInvalidRuleCount;
    merged.notices.push(...summary.notices);
  }
  return merged;
}

export function migrateRemovalRules(rules: RemovalRule[]): MigratedRemovalRules {
  const migrated = rules.map(migrateRemovalRule);
  return {
    rules: migrated.map(result => result.rule),
    summary: mergeRemovalRuleMigrationSummaries(migrated.map(result => result.summary)),
  };
}

export function formatRemovalRuleMigrationSummary(
  summary: RemovalRuleMigrationSummary
): string {
  if (summary.notices.length === 0) return "";

  const counts = [
    summary.inlineLiteralRuleCount > 0
      ? `${summary.inlineLiteralRuleCount} Anywhere literal rule(s) interpreted as Inline literal`
      : "",
    summary.disabledInvalidRuleCount > 0
      ? `${summary.disabledInvalidRuleCount} invalid rule(s) disabled`
      : "",
  ].filter(Boolean).join("; ");
  const reasons = summary.notices
    .map(notice => `${notice.ruleId || notice.label}: ${notice.message}`)
    .join("; ");
  return `Migration: ${counts}. ${reasons}`;
}
