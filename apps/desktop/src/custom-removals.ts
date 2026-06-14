import { dom } from "./dom";
import { state } from "./state";
import type { CleaningConfig, RemovalRule } from "./generated/CleaningConfig.js";

export function formatRemovalRuleText(rule: RemovalRule): string {
  if (rule.matcher.kind === "literal") {
    const scopeLabel = (() => {
      switch (rule.scope) {
        case "whole_line": return "Whole line";
        case "page_top": return "Page top";
        case "page_bottom": return "Page bottom";
        case "page_top_or_bottom": return "Page top/bottom";
        default: return "Anywhere";
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
