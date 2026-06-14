import type { RepeatedArtifactKind } from "./RepeatedArtifactKind.js";
import type { PositionSummary } from "./PositionSummary.js";
import type { ArtifactRiskLabel } from "./ArtifactRiskLabel.js";
import type { CandidateContentClass } from "./CandidateContentClass.js";
import type { CandidateTextProfile } from "./CandidateTextProfile.js";
import type { CandidateTextSignalLabel } from "./CandidateTextSignalLabel.js";
import type { RepeatedArtifactExample } from "./RepeatedArtifactExample.js";

export type RepeatedArtifactCandidate = { candidate_id: string, kind: RepeatedArtifactKind, display_text: string, normalized_key: string, occurrence_count: number, file_count: number, example_count: number, position_summary: PositionSummary, position_summary_is_page_based: boolean, risk_label: ArtifactRiskLabel,
/**
 * Content classification (text, mixed, numeric, symbol).
 */
content_class: CandidateContentClass,
/**
 * Deterministic advisory text/noise profile used during candidate review.
 */
text_profile: CandidateTextProfile,
/**
 * Compact advisory label derived from the text profile and existing evidence.
 */
text_signal_label: CandidateTextSignalLabel,
/**
 * Stable reason codes explaining the advisory text signal.
 */
text_signal_reasons: Array<string>,
/**
 * How many distinct raw text variants appear under this candidate's grouping key.
 * For normalised candidates this shows how many distinct lines were grouped.
 */
raw_variant_count: number,
/**
 * True if the raw_variant_count is capped at RAW_VARIANT_TRACK_CAP and may be higher.
 */
raw_variant_count_is_capped: boolean,
/**
 * The actual distinct raw text variants tracked for this candidate.
 * For exact-line candidates this contains the single literal string.
 * For normalised candidates this contains all distinct raw lines that
 * normalise to the same grouping key (up to RAW_VARIANT_TRACK_CAP).
 */
raw_variants: Array<string>, examples: Array<RepeatedArtifactExample>, };
