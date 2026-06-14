import type { RemovalRuleSource } from "./RemovalRuleSource.js";
import type { RemovalMatcher } from "./RemovalMatcher.js";
import type { RemovalScope } from "./RemovalScope.js";

export type RemovalRule = { id: string, label: string, source: RemovalRuleSource, matcher: RemovalMatcher, scope: RemovalScope, enabled: boolean, };
