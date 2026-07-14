import { readFileSync } from "node:fs";

import type { RemovalRule } from "../src/generated/RemovalRule.js";
import { cloneRemovalRules } from "../src/features/prepare/cleaning/removal-rule-utils.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const fixturePath = new URL(
  "../../../crates/corpuswright-core/tests/fixtures/repeated_artifact_promotion.json",
  import.meta.url
);
const characterisationFixture = JSON.parse(readFileSync(fixturePath, "utf8")) as Array<{
  expected_rule: RemovalRule | null;
}>;
const promotableRules = characterisationFixture
  .map(entry => entry.expected_rule)
  .filter((rule): rule is RemovalRule => rule !== null);

assert(promotableRules.length > 0, "the shared Rust promotion fixture must contain promotable rules");
assert(
  promotableRules.every(rule => rule.source === "promoted_repeated_artifact"),
  "manual promotion fixtures must retain promoted-repeated-artefact provenance"
);

const manualPromotedRule = promotableRules.find(rule => rule.scope === "whole_line");
assert(manualPromotedRule !== undefined, "the fixture must cover a whole-line promoted rule");

const persistedRules = [{
  ...manualPromotedRule,
  matcher: { ...manualPromotedRule.matcher },
}];
const effectiveRules = cloneRemovalRules(persistedRules);
assert(
  effectiveRules.length === 1 &&
    JSON.stringify(effectiveRules[0]) === JSON.stringify(manualPromotedRule),
  "effective preview/processing/export config must retain a manually promoted rule byte-for-byte"
);
assert(
  effectiveRules !== persistedRules,
  "building effective config must not alias the persisted rule array"
);
assert(
  effectiveRules[0].matcher !== persistedRules[0].matcher,
  "building effective config must not alias the persisted matcher"
);
