import type { ArtifactRiskLabel } from "../src/generated/ArtifactRiskLabel.js";
import type { CandidateTextSignalLabel } from "../src/generated/CandidateTextSignalLabel.js";
import type { RepeatedArtifactCandidate } from "../src/generated/RepeatedArtifactCandidate.js";
import {
  categoryForCandidate,
  compareCandidatesByEvidence,
  decisionStateForCandidate,
  filterCandidatesForReview,
} from "../src/repeated-artifact-review.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

interface CandidateOverrides {
  risk?: ArtifactRiskLabel;
  signal?: CandidateTextSignalLabel;
  occurrences?: number;
  files?: number;
  pageBased?: boolean;
  top?: number;
  middle?: number;
  bottom?: number;
}

function candidate(id: string, overrides: CandidateOverrides = {}): RepeatedArtifactCandidate {
  return {
    candidate_id: id,
    kind: "exact_line",
    display_text: id,
    normalized_key: id,
    occurrence_count: overrides.occurrences ?? 3,
    file_count: overrides.files ?? 1,
    example_count: 0,
    position_summary: {
      top_count: overrides.top ?? 0,
      middle_count: overrides.middle ?? 0,
      bottom_count: overrides.bottom ?? 0,
      unknown_count: 0,
    },
    position_summary_is_page_based: overrides.pageBased ?? false,
    risk_label: overrides.risk ?? "ambiguous",
    content_class: "text_dominant",
    text_profile: {
      char_count: id.length,
      token_count: 1,
      alphabetic_ratio: 1,
      digit_ratio: 0,
      symbol_ratio: 0,
      whitespace_ratio: 0,
      uppercase_ratio: 0,
      average_token_length: id.length,
      max_repeated_char_run: 1,
      suspicious_token_ratio: 0,
      has_sentence_punctuation: false,
      looks_like_common_section_heading: false,
      looks_like_page_label: false,
      looks_like_table_or_numeric_row: false,
      looks_like_formula_or_code: false,
      looks_like_markup_or_extraction_noise: false,
    },
    text_signal_label: overrides.signal ?? "ambiguous",
    text_signal_reasons: [],
    raw_variant_count: 1,
    raw_variant_count_is_capped: false,
    raw_variants: [id],
    examples: [],
  };
}

const pageLabel = candidate("page", {
  risk: "strong_header_footer_candidate",
  signal: "likely_page_label",
});
assert(
  categoryForCandidate(pageLabel) === "page_labels",
  "page-label signal must win when a candidate matches multiple axes"
);
assert(
  categoryForCandidate(candidate("heading-risk", { risk: "common_section_heading_review_carefully" })) === "headings",
  "common-section-heading risk must map to Headings"
);
assert(
  categoryForCandidate(candidate("heading-signal", {
    risk: "possible_boilerplate",
    signal: "likely_section_heading",
  })) === "headings",
  "heading signal must precede boilerplate risk"
);
assert(
  categoryForCandidate(candidate("header", { risk: "strong_header_footer_candidate" })) === "headers_footers",
  "strong header/footer risk must map to Headers / Footers"
);
assert(
  categoryForCandidate(candidate("boilerplate", { risk: "possible_boilerplate" })) === "boilerplate",
  "possible-boilerplate risk must map to Boilerplate"
);
assert(
  categoryForCandidate(candidate("noise", { risk: "symbol_or_noise_candidate" })) === "symbol_noise",
  "symbol/noise risk must map to Symbol / Noise"
);
assert(
  categoryForCandidate(candidate("other")) === "other",
  "unmatched candidates must remain visible in Other"
);

const headerCandidates = [
  candidate("unreviewed", { risk: "strong_header_footer_candidate" }),
  candidate("marked", { risk: "strong_header_footer_candidate" }),
  candidate("dismissed", { risk: "strong_header_footer_candidate" }),
];
const decisions = {
  marked: { decision: "accepted" as const },
  dismissed: { decision: "dismissed" as const },
};
assert(
  filterCandidatesForReview(headerCandidates, "headers_footers", "all", decisions).length === 3,
  "decision changes must not move candidates between type tabs"
);
assert(
  filterCandidatesForReview(headerCandidates, "headers_footers", "unreviewed", decisions)[0]?.candidate_id === "unreviewed",
  "Unreviewed must be independent of the type tab"
);
assert(
  filterCandidatesForReview(headerCandidates, "headers_footers", "marked", decisions)[0]?.candidate_id === "marked",
  "Marked must be independent of the type tab"
);
assert(
  filterCandidatesForReview(headerCandidates, "headers_footers", "dismissed", decisions)[0]?.candidate_id === "dismissed",
  "Dismissed must be independent of the type tab"
);
assert(
  decisionStateForCandidate(undefined) === "unreviewed",
  "a fresh scan candidate must be unreviewed rather than pre-marked"
);

const ranked = [
  candidate("id-b", { risk: "possible_boilerplate", occurrences: 5, files: 2, pageBased: true, top: 5 }),
  candidate("lower-risk", { risk: "ambiguous", occurrences: 100, files: 10 }),
  candidate("more-occurrences", { risk: "possible_boilerplate", occurrences: 6, files: 1 }),
  candidate("more-files", { risk: "possible_boilerplate", occurrences: 5, files: 3 }),
  candidate("id-a", { risk: "possible_boilerplate", occurrences: 5, files: 2, pageBased: true, top: 5 }),
  candidate("a-no-page-evidence", { risk: "possible_boilerplate", occurrences: 5, files: 2 }),
];
const expectedOrder = [
  "more-occurrences",
  "more-files",
  "id-a",
  "id-b",
  "a-no-page-evidence",
  "lower-risk",
];
for (let run = 0; run < 5; run++) {
  const actual = [...ranked].sort(compareCandidatesByEvidence).map(item => item.candidate_id);
  assert(JSON.stringify(actual) === JSON.stringify(expectedOrder), "default evidence ranking must be deterministic");
}
