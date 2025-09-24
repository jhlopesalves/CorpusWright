import { dom } from "./dom";
import { state } from "./state";
import type { CleaningConfig } from "./generated/CleaningConfig.js";

export function renderCustomRemovals(): void {
  dom.customRemovalsList.innerHTML = "";
  dom.customRemovalsCount.textContent = `${state.tempRemovePatterns.length} item${state.tempRemovePatterns.length === 1 ? "" : "s"}`;

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
}

export function syncDraftCustomRemovalsFromConfig(config: CleaningConfig): void {
  state.tempRemovePatterns = [...config.remove_patterns];
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
    renderCustomRemovals();
  });
}
