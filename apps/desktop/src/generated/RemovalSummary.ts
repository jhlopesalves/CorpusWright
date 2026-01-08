import type { CleaningOperation } from "./CleaningOperation.js";
import type { RemovalRuleSource } from "./RemovalRuleSource.js";
import type { RemovedTextExample } from "./RemovedTextExample.js";

export type RemovalSummary = { operation: CleaningOperation, rule_id: string | null, rule_source: RemovalRuleSource | null, reason_codes: Array<string>, source_id: string | null, source_label: string | null, occurrence_count: number, examples: Array<RemovedTextExample>, };
