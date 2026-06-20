import {
  formatRemovalRuleMigrationSummary,
  migrateRemovalRules,
  migrateRepeatedArtifactDecisionRule,
} from "../src/removal-rule-migration.ts";
import { normaliseCleaningConfigWithMigration } from "../src/config.ts";
import type { RemovalRule } from "../src/generated/CleaningConfig.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function literalRule(id: string, text: string): RemovalRule {
  return {
    id,
    label: id,
    source: "manual",
    matcher: { kind: "literal", text },
    scope: "anywhere",
    enabled: true,
  };
}

const rules: RemovalRule[] = [
  literalRule("valid-inline", "<br />"),
  literalRule("empty-inline", ""),
  literalRule("whitespace-inline", "   "),
  literalRule("multiline-inline", "one\ntwo"),
  {
    id: "normalised-inline",
    label: "normalised-inline",
    source: "promoted_repeated_artifact",
    matcher: { kind: "normalized_line", normalized_key: "page #" },
    scope: "anywhere",
    enabled: true,
  },
];

const first = migrateRemovalRules(rules);
assert(first.rules[0].enabled, "valid literal Anywhere rules must remain enabled");
assert(!first.rules[1].enabled, "empty inline literals must be disabled");
assert(!first.rules[2].enabled, "whitespace-only inline literals must be disabled");
assert(!first.rules[3].enabled, "multiline inline literals must be disabled");
assert(!first.rules[4].enabled, "normalised-line Anywhere rules must be disabled");
assert(first.summary.inlineLiteralRuleCount === 1, "the valid inline interpretation must be reported");
assert(first.summary.disabledInvalidRuleCount === 4, "all invalid combinations must be reported");
assert(first.summary.notices.length === 5, "each migrated rule must have a reason");

const second = migrateRemovalRules(first.rules);
assert(
  JSON.stringify(second.rules) === JSON.stringify(first.rules),
  "removal-rule migration must be idempotent"
);
assert(
  JSON.stringify(second.summary) === JSON.stringify(first.summary),
  "idempotent migration must produce the same summary"
);

const message = formatRemovalRuleMigrationSummary(first.summary);
for (const rule of rules) {
  assert(message.includes(rule.id), `migration feedback must identify ${rule.id}`);
}

const imported = normaliseCleaningConfigWithMigration({ removal_rules: rules });
assert(
  JSON.stringify(imported.config.removal_rules) === JSON.stringify(first.rules),
  "cleaning-config import must apply the shared removal-rule migration"
);
assert(
  JSON.stringify(imported.migrationSummary) === JSON.stringify(first.summary),
  "cleaning-config import must expose the shared migration summary"
);

const inlineDecision = migrateRepeatedArtifactDecisionRule("inline_123", {
  candidate_signature: "inline_123",
  candidate_text: " <br /> ",
  scope: "whole_line",
  matcher_kind: "literal",
  rule_id: "repeated-artifact:whole_line:inline_123",
});
assert(inlineDecision.rule !== null, "persisted inline decisions must produce a rule");
assert(inlineDecision.rule.scope === "anywhere", "persisted inline decisions must migrate to Anywhere");
assert(inlineDecision.rule.enabled, "valid persisted inline decisions must remain enabled");
assert(
  inlineDecision.rule.matcher.kind === "literal" && inlineDecision.rule.matcher.text === " <br /> ",
  "persisted literal decision text must remain verbatim"
);

const normalisedDecision = migrateRepeatedArtifactDecisionRule("normalised_123", {
  candidate_signature: "normalised_123",
  candidate_text: " page # ",
  scope: "anywhere",
  matcher_kind: "normalized_line",
  rule_id: "repeated-artifact:normalized-line:anywhere:normalised_123",
});
assert(normalisedDecision.rule !== null, "invalid persisted decisions must remain present");
assert(!normalisedDecision.rule.enabled, "normalised-line Anywhere decisions must be disabled");
assert(
  normalisedDecision.rule.matcher.kind === "normalized_line" &&
    normalisedDecision.rule.matcher.normalized_key === "page #",
  "normalised decision keys must remain trimmed"
);
assert(
  normalisedDecision.summary.notices[0]?.reason === "normalized_line_anywhere_disabled",
  "project-decision migration must expose the invalid-combination reason"
);
