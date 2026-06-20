import type { RemovalRuleSource } from "./RemovalRuleSource.js";

export type SkippedRuleNotice = { rule_id: string | null, rule_source: RemovalRuleSource | null, source_id: string | null, source_label: string | null, notice: string, occurrence_count: number, };
