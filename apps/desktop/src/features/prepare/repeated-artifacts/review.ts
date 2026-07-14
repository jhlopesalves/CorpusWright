import type { RepeatedArtifactCandidate } from "../../../generated/RepeatedArtifactCandidate.js";

export type ArtifactCategory =
  | "page_labels"
  | "headings"
  | "headers_footers"
  | "boilerplate"
  | "symbol_noise"
  | "other";

export type ReviewDecision = "accepted" | "dismissed";
export type DecisionState = "unreviewed" | "marked" | "dismissed";
export type DecisionFilter = "all" | DecisionState;

export const ARTIFACT_CATEGORIES: readonly ArtifactCategory[] = [
  "page_labels",
  "headings",
  "headers_footers",
  "boilerplate",
  "symbol_noise",
  "other",
];

export const ARTIFACT_CATEGORY_LABELS: Readonly<Record<ArtifactCategory, string>> = {
  page_labels: "Page numbers & labels",
  headings: "Headings",
  headers_footers: "Headers / Footers",
  boilerplate: "Boilerplate",
  symbol_noise: "Symbol / Noise",
  other: "Other",
};

export const DECISION_FILTER_LABELS: Readonly<Record<DecisionFilter, string>> = {
  all: "All",
  unreviewed: "Unreviewed",
  marked: "Marked",
  dismissed: "Dismissed",
};

const RISK_RANK: Readonly<Record<string, number>> = {
  strong_header_footer_candidate: 5,
  symbol_or_noise_candidate: 4,
  possible_boilerplate: 3,
  common_section_heading_review_carefully: 2,
  ambiguous: 1,
};

export function categoryForCandidate(candidate: RepeatedArtifactCandidate): ArtifactCategory {
  if (candidate.text_signal_label === "likely_page_label") return "page_labels";
  if (
    candidate.risk_label === "common_section_heading_review_carefully" ||
    candidate.text_signal_label === "likely_section_heading"
  ) {
    return "headings";
  }
  if (candidate.risk_label === "strong_header_footer_candidate") return "headers_footers";
  if (candidate.risk_label === "possible_boilerplate") return "boilerplate";
  if (candidate.risk_label === "symbol_or_noise_candidate") return "symbol_noise";
  return "other";
}

export function decisionStateForCandidate(decision: ReviewDecision | undefined): DecisionState {
  if (decision === "accepted") return "marked";
  if (decision === "dismissed") return "dismissed";
  return "unreviewed";
}

export function filterCandidatesForReview(
  candidates: readonly RepeatedArtifactCandidate[],
  category: ArtifactCategory,
  decisionFilter: DecisionFilter,
  decisions: Readonly<Record<string, { decision: ReviewDecision } | undefined>>
): RepeatedArtifactCandidate[] {
  return candidates.filter(candidate => {
    if (categoryForCandidate(candidate) !== category) return false;
    if (decisionFilter === "all") return true;
    return decisionStateForCandidate(decisions[candidate.candidate_id]?.decision) === decisionFilter;
  });
}

function pagePositionEvidence(candidate: RepeatedArtifactCandidate): number {
  if (!candidate.position_summary_is_page_based) return 0;
  const position = candidate.position_summary;
  return position.top_count + position.middle_count + position.bottom_count;
}

export function riskRank(candidate: RepeatedArtifactCandidate): number {
  return RISK_RANK[candidate.risk_label] ?? 0;
}

export function compareCandidatesByEvidence(
  left: RepeatedArtifactCandidate,
  right: RepeatedArtifactCandidate
): number {
  return (
    riskRank(right) - riskRank(left) ||
    right.occurrence_count - left.occurrence_count ||
    right.file_count - left.file_count ||
    pagePositionEvidence(right) - pagePositionEvidence(left) ||
    left.candidate_id.localeCompare(right.candidate_id)
  );
}
