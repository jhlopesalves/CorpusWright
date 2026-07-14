import type { CleaningConfig } from "../../../generated/CleaningConfig.js";
import { state } from "../../../app/state";
import { cloneRemovalRules } from "./removal-rule-utils.js";

export function effectiveCleaningConfig(): CleaningConfig {
  const base = state.activeCleaningConfig;
  return {
    ...base,
    remove_patterns: [...base.remove_patterns],
    removal_rules: cloneRemovalRules(base.removal_rules),
    replace_patterns: base.replace_patterns.map(rule => ({ ...rule })),
  };
}
