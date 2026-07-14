import { dom } from "../../../app/dom";
import { state } from "../../../app/state";
import type { CleaningConfig, RemovalRule } from "../../../generated/CleaningConfig.js";

export function formatRemovalRuleText(rule: RemovalRule): string {
  if (rule.matcher.kind === "literal") {
    const scopeLabel = (() => {
      switch (rule.scope) {
        case "whole_line": return "Whole line";
        case "page_top": return "Page top";
        case "page_bottom": return "Page bottom";
        case "page_top_or_bottom": return "Page top/bottom";
        default: return "Inline literal";
      }
    })();
    return `${scopeLabel}: ${rule.matcher.text}`;
  }
  if (rule.matcher.kind === "normalized_line") {
    const scopeLabel = (() => {
      switch (rule.scope) {
        case "whole_line": return "Whole-line";
        case "page_top": return "Page top";
        case "page_bottom": return "Page bottom";
        case "page_top_or_bottom": return "Page top/bottom";
        default: return "Anywhere";
      }
    })();
    return `${scopeLabel} family: ${rule.matcher.normalized_key}`;
  }
  return rule.label;
}

function removalRuleText(rule: RemovalRule): string {
  return formatRemovalRuleText(rule);
}

export function countRepeatedArtifactRemovalRules(rules: RemovalRule[]): { total: number; enabled: number; disabled: number } {
  const repeatedArtifactRules = rules.filter(rule => rule.source === "promoted_repeated_artifact");
  const enabled = repeatedArtifactRules.filter(rule => rule.enabled).length;
  return {
    total: repeatedArtifactRules.length,
    enabled,
    disabled: repeatedArtifactRules.length - enabled,
  };
}

export function syncRepeatedArtifactRulesSummary(): void {
  const rules = dom.settingsModal.classList.contains("hidden")
    ? state.activeCleaningConfig.removal_rules
    : state.tempRemovalRules;
  const counts = countRepeatedArtifactRemovalRules(rules);

  if (counts.total === 0) {
    dom.repeatedArtifactRulesSummary.textContent = "No repeated artefact rules have been accepted yet.";
    return;
  }

  dom.repeatedArtifactRulesSummary.textContent =
    `Accepted repeated artefact rules: ${counts.total.toLocaleString()}. ` +
    `Enabled: ${counts.enabled.toLocaleString()}. ` +
    `Disabled: ${counts.disabled.toLocaleString()}.`;
}

export function renderCustomRemovals(): void {
  dom.customRemovalsList.innerHTML = "";
  const itemCount = state.tempRemovePatterns.length + state.tempRemovalRules.length;
  dom.customRemovalsCount.textContent = `${itemCount} item${itemCount === 1 ? "" : "s"}`;

  state.tempRemovePatterns.forEach((pattern, index) => {
    const pill = document.createElement("div");
    pill.className = "sequence-pill";

    const textSpan = document.createElement("span");
    textSpan.textContent = pattern; // This prevents HTML from rendering!

    const delBtn = document.createElement("button");
    delBtn.className = "sequence-pill-delete";
    delBtn.type = "button";
    delBtn.innerHTML = "&times;";
    delBtn.onclick = () => {
      state.tempRemovePatterns.splice(index, 1);
      renderCustomRemovals();
    };

    pill.appendChild(textSpan);
    pill.appendChild(delBtn);
    dom.customRemovalsList.appendChild(pill);
  });

  state.tempRemovalRules.forEach((rule, index) => {
    const pill = document.createElement("div");
    pill.className = "sequence-pill structured-removal-pill";
    pill.title = `${rule.label} · ${rule.source}`;

    const textSpan = document.createElement("span");
    textSpan.textContent = removalRuleText(rule);

    const delBtn = document.createElement("button");
    delBtn.className = "sequence-pill-delete";
    delBtn.type = "button";
    delBtn.innerHTML = "&times;";
    delBtn.onclick = () => {
      state.tempRemovalRules.splice(index, 1);
      renderCustomRemovals();
    };

    pill.appendChild(textSpan);
    pill.appendChild(delBtn);
    dom.customRemovalsList.appendChild(pill);
  });

  syncRepeatedArtifactRulesSummary();
}

export function syncDraftCustomRemovalsFromConfig(config: CleaningConfig): void {
  state.tempRemovePatterns = [...config.remove_patterns];
  state.tempRemovalRules = config.removal_rules.map((rule) => ({
    ...rule,
    matcher: { ...rule.matcher },
  }));
  state.tempReplacePatterns = [...config.replace_patterns];
  renderCustomRemovals();
}

export function initCustomRemovals(): void {
  dom.btnAddCustomRemoval.addEventListener("click", () => {
    const val = dom.customRemovalInput.value;
    if (val) {
      state.tempRemovePatterns.push(val);
      dom.customRemovalInput.value = "";
      renderCustomRemovals();
    }
  });

  dom.customRemovalInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      dom.btnAddCustomRemoval.click();
    }
  });

  dom.btnClearCustomRemovals.addEventListener("click", () => {
    state.tempRemovePatterns = [];
    state.tempRemovalRules = [];
    renderCustomRemovals();
  });
}
