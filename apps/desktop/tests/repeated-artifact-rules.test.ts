import {
  createRepeatedArtifactRemovalRule,
  repeatedArtifactCandidateScope,
  repeatedArtifactMatcherKindForCandidate,
  removeRepeatedArtifactRulesForCandidate,
} from "../src/repeated-artifact-rules.js";
import type { RepeatedArtifactCandidate } from "../src/generated/RepeatedArtifactCandidate.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) {
    throw new Error(message);
  }
}

function candidate(overrides: Partial<RepeatedArtifactCandidate>): RepeatedArtifactCandidate {
  return {
    candidate_id: "exact_0000000000000000",
    kind: "exact_line",
    display_text: "Header",
    normalized_key: "header",
    occurrence_count: 2,
    file_count: 2,
    example_count: 0,
    position_summary: {
      top_count: 0,
      middle_count: 2,
      bottom_count: 0,
      unknown_count: 0,
    },
    position_summary_is_page_based: false,
    risk_label: "ambiguous",
    content_class: "text_dominant",
    text_profile: {
      char_count: 6,
      token_count: 1,
      alphabetic_ratio: 1,
      digit_ratio: 0,
      symbol_ratio: 0,
      whitespace_ratio: 0,
      uppercase_ratio: 0,
      average_token_length: 6,
      max_repeated_char_run: 1,
      suspicious_token_ratio: 0,
      has_sentence_punctuation: false,
      looks_like_common_section_heading: false,
      looks_like_page_label: false,
      looks_like_table_or_numeric_row: false,
      looks_like_formula_or_code: false,
      looks_like_markup_or_extraction_noise: false,
    },
    text_signal_label: "ambiguous",
    text_signal_reasons: [],
    raw_variant_count: 1,
    raw_variant_count_is_capped: false,
    raw_variants: [],
    examples: [],
    ...overrides,
  };
}

const inlineCandidate = candidate({
  candidate_id: "inline_1111111111111111",
  kind: "inline_artifact",
  display_text: "<br />",
  normalized_key: "",
});
const inlineRule = createRepeatedArtifactRemovalRule(
  inlineCandidate,
  "Anywhere: <br />"
);
assert(inlineRule !== null, "inline candidate should produce a rule");
assert(inlineRule.scope === "anywhere", "inline candidate should use anywhere scope");
assert(inlineRule.matcher.kind === "literal", "inline candidate should use a literal matcher");
assert(inlineRule.matcher.text === "<br />", "inline literal text should be preserved");
assert(
  repeatedArtifactMatcherKindForCandidate(inlineCandidate) === "literal",
  "inline decision should persist as a literal matcher"
);

const normalizedCandidate = candidate({
  candidate_id: "normalized_2222222222222222",
  kind: "normalized_line",
  display_text: "Page 12",
  normalized_key: "page #",
});
const normalizedRule = createRepeatedArtifactRemovalRule(
  normalizedCandidate,
  "Whole-line family: page #"
);
assert(normalizedRule !== null, "normalised candidate should produce a rule");
assert(
  repeatedArtifactCandidateScope(normalizedCandidate) === "whole_line",
  "normalised non-page candidate should remain whole-line scoped"
);
assert(
  normalizedRule.matcher.kind === "normalized_line" &&
    normalizedRule.matcher.normalized_key === "page #",
  "normalised candidate should preserve its normalised key"
);

const emptyInlineRule = createRepeatedArtifactRemovalRule(
  candidate({
    candidate_id: "inline_empty",
    kind: "inline_artifact",
    display_text: "",
    normalized_key: "",
  }),
  "Anywhere:"
);
assert(emptyInlineRule === null, "empty inline candidates cannot produce rules");

const remainingRules = removeRepeatedArtifactRulesForCandidate(
  [
    inlineRule,
    {
      ...inlineRule,
      id: "repeated-artifact:anywhere:inline_3333333333333333",
    },
  ],
  "inline_1111111111111111"
);
assert(remainingRules.length === 1, "unmark should remove only the selected candidate rule");
assert(
  remainingRules[0].id.endsWith(":inline_3333333333333333"),
  "unmark should keep other repeated artefact rules"
);
