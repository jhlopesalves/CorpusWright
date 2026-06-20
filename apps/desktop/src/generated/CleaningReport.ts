import type { RemovalSummary } from "./RemovalSummary.js";
import type { SkippedRuleNotice } from "./SkippedRuleNotice.js";
import type { CleaningCount } from "./CleaningCount.js";
import type { ReasonCount } from "./ReasonCount.js";

export type CleaningReport = { summaries: Array<RemovalSummary>, skipped_rules: Array<SkippedRuleNotice>, totals_by_operation: Array<CleaningCount>, totals_by_reason: Array<ReasonCount>, };
