import { readFileSync } from "node:fs";

import { reconcileAutoPromotedRules } from "../src/auto-promoted-rules.ts";
import { mergeEffectiveRemovalRules } from "../src/removal-rule-utils.ts";
import type { AutoPromotedRepeatedArtifactRule } from "../src/generated/AutoPromotedRepeatedArtifactRule.js";
import type { RemovalRule } from "../src/generated/RemovalRule.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function rule(
  candidateId: string,
  text: string,
  source: RemovalRule["source"] = "auto_promoted_repeated_artifact"
): RemovalRule {
  return {
    id: `repeated-artifact:page_top:${candidateId}`,
    label: `Page top: ${text}`,
    source,
    matcher: { kind: "literal", text },
    scope: "page_top",
    enabled: true,
  };
}

function promotion(candidateId: string, text: string): AutoPromotedRepeatedArtifactRule {
  return { candidate_id: candidateId, rule: rule(candidateId, text) };
}

const fixturePath = new URL(
  "../../../crates/corpuswright-core/tests/fixtures/repeated_artifact_promotion.json",
  import.meta.url
);
const characterisationFixture = JSON.parse(readFileSync(fixturePath, "utf8")) as Array<{
  candidate_id: string;
  expected_rule: RemovalRule | null;
}>;
const characterisedRules = characterisationFixture
  .filter((entry): entry is { candidate_id: string; expected_rule: RemovalRule } => entry.expected_rule !== null)
  .map(entry => ({
    candidate_id: entry.candidate_id,
    rule: {
      ...entry.expected_rule,
      source: "auto_promoted_repeated_artifact" as const,
      matcher: { ...entry.expected_rule.matcher },
    },
  }));
assert(
  reconcileAutoPromotedRules(characterisedRules, [], new Set(), true).length === characterisedRules.length,
  "the shared Rust/TypeScript characterisation fixture must remain valid runtime input"
);

const scanned = [
  promotion("exact_b", "Header B"),
  promotion("exact_a", "Header A"),
];

assert(
  reconcileAutoPromotedRules(scanned, [], new Set(), false).length === 0,
  "the disabled toggle must exclude every runtime rule"
);

const enabled = reconcileAutoPromotedRules(scanned, [], new Set(), true);
assert(enabled.length === 2, "the enabled toggle must include scanned rules");
assert(enabled[0].id.endsWith(":exact_a"), "runtime rules must be sorted deterministically");

const dismissed = reconcileAutoPromotedRules(scanned, [], new Set(["exact_a"]), true);
assert(dismissed.length === 1, "a dismissal must exclude only its candidate");
assert(dismissed[0].id.endsWith(":exact_b"), "other automatic rules must remain active");

const restored = reconcileAutoPromotedRules(scanned, [], new Set(), true);
assert(restored.length === 2, "restoring a dismissal must restore the latest scanned rule");

const manual = rule("exact_a", "Header A", "promoted_repeated_artifact");
const manualWins = reconcileAutoPromotedRules(scanned, [manual], new Set(), true);
assert(manualWins.length === 1, "a manual rule must replace its automatic duplicate");
assert(manualWins[0].id.endsWith(":exact_b"), "unrelated automatic rules must remain");

const laterScan = reconcileAutoPromotedRules(
  [promotion("exact_c", "Header C")],
  [],
  new Set(),
  true
);
assert(laterScan.length === 1, "a later scan must replace rather than accumulate rules");
assert(laterScan[0].id.endsWith(":exact_c"), "the latest scan must own the runtime set");

const baseRules = [manual];
const effective = mergeEffectiveRemovalRules(baseRules, enabled, true);
assert(baseRules.length === 1, "building effective config must not mutate saved base rules");
assert(effective.length === 2, "effective rules must deduplicate the manual/automatic overlap");
assert(
  mergeEffectiveRemovalRules(baseRules, enabled, false).length === 1,
  "disabling auto-apply must leave manual rules untouched"
);
