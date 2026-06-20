import type { RemovalRuleSource } from "./RemovalRuleSource.js";

export type SkippedRuleNotice = { rule_source: RemovalRuleSource | null, source_id: string | null, source_label: string | null, notice: string, occurrence_count: number, };
