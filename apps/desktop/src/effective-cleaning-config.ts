import type { CleaningConfig } from "./generated/CleaningConfig.js";
import { state } from "./state";
import { mergeEffectiveRemovalRules } from "./removal-rule-utils";

export function effectiveCleaningConfig(): CleaningConfig {
  const base = state.activeCleaningConfig;
  return {
    ...base,
    remove_patterns: [...base.remove_patterns],
    removal_rules: mergeEffectiveRemovalRules(
      base.removal_rules,
      state.autoPromotedRemovalRules,
      base.auto_apply_strong_header_footer
    ),
    replace_patterns: base.replace_patterns.map(rule => ({ ...rule })),
  };
}

export function clearRuntimeAutoPromotedRules(): void {
  state.lastScanAutoPromotedRules = [];
  state.autoPromotedRemovalRules = [];
}
