import type { RemovalRule } from "./generated/CleaningConfig.js";
import type { RepeatedArtifactCandidate } from "./generated/RepeatedArtifactCandidate.js";
import type { RemovalScope } from "./generated/RemovalScope.js";

export type RepeatedArtifactDecisionMatcherKind = "literal" | "normalized_line";

export function repeatedArtifactRuleId(
  candidate: RepeatedArtifactCandidate,
  scope: RemovalScope
): string {
  if (candidate.kind === "normalized_line") {
    return `repeated-artifact:normalized-line:${scope}:${candidate.candidate_id}`;
  }
  return `repeated-artifact:${scope}:${candidate.candidate_id}`;
}

export function repeatedArtifactCandidateScope(candidate: RepeatedArtifactCandidate): RemovalScope {
  if (candidate.kind === "inline_artifact") {
    return "anywhere";
  }

  if (
    candidate.position_summary_is_page_based &&
    candidate.position_summary.middle_count === 0
  ) {
    const top = candidate.position_summary.top_count;
    const bot = candidate.position_summary.bottom_count;
    if (top > 0 && bot === 0) {
      return "page_top";
    }
    if (top === 0 && bot > 0) {
      return "page_bottom";
    }
    if (top > 0 && bot > 0) {
      return "page_top_or_bottom";
    }
  }

  return "whole_line";
}

export function repeatedArtifactMatcherKindForCandidate(
  candidate: RepeatedArtifactCandidate
): RepeatedArtifactDecisionMatcherKind {
  return candidate.kind === "normalized_line" ? "normalized_line" : "literal";
}

export function repeatedArtifactCandidateRuleText(candidate: RepeatedArtifactCandidate): string {
  return candidate.kind === "normalized_line"
    ? (candidate.normalized_key || "").trim()
    : candidate.display_text;
}

export function canCreateRepeatedArtifactRule(candidate: RepeatedArtifactCandidate): boolean {
  return repeatedArtifactCandidateRuleText(candidate).trim().length > 0;
}

export function createRepeatedArtifactRemovalRule(
  candidate: RepeatedArtifactCandidate,
  label: string,
  scope = repeatedArtifactCandidateScope(candidate)
): RemovalRule | null {
  const ruleText = repeatedArtifactCandidateRuleText(candidate).trim();
  if (!ruleText) return null;

  const matcher = candidate.kind === "normalized_line"
    ? { kind: "normalized_line" as const, normalized_key: ruleText }
    : { kind: "literal" as const, text: candidate.display_text };

  return {
    id: repeatedArtifactRuleId(candidate, scope),
    label,
    source: "promoted_repeated_artifact",
    matcher,
    scope,
    enabled: true,
  };
}

export function removeRepeatedArtifactRulesForCandidate(
  rules: RemovalRule[],
  candidateId: string
): RemovalRule[] {
  const suffix = `:${candidateId}`;
  return rules.filter(rule => !rule.id.endsWith(suffix));
}
