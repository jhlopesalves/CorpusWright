import { dom } from "./dom";
import { sourceHasExtractedText } from "./sources";
import { state, type WorkbenchId } from "./state";

const FUTURE_ANALYSIS_TOOLS = [
  "Corpus Overview",
  "Frequency List",
  "KWIC / Concordance",
  "N-grams",
  "Exports",
];

function pluralise(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
}

function processedSourceCount(): number {
  return state.projectSources.filter((source) =>
    source.status === "processed" || source.status === "processed_with_warnings"
  ).length;
}

function extractedTextSourceCount(): number {
  return state.projectSources.filter(sourceHasExtractedText).length;
}

function appendSummaryRow(labelText: string, valueText: string): void {
  const label = document.createElement("dt");
  label.textContent = labelText;

  const value = document.createElement("dd");
  value.textContent = valueText;

  dom.analyseProjectSummary.append(label, value);
}

export function renderAnalyseWorkbench(): void {
  const sourceCount = state.projectSources.length;
  const fileCount = state.allFiles.length;
  const processedCount = processedSourceCount();
  const extractedCount = extractedTextSourceCount();

  dom.analyseStatusDetail.textContent = processedCount > 0
    ? "Processed text available - analysis index not built."
    : "Analysis corpus not built yet.";

  dom.analyseProjectSummary.replaceChildren();
  appendSummaryRow("Project", state.activeProject?.name ?? "No project open");
  appendSummaryRow("Sources", pluralise(sourceCount, "source"));
  appendSummaryRow("Files loaded", pluralise(fileCount, "file"));
  appendSummaryRow("Text available", `${extractedCount.toLocaleString()} of ${sourceCount.toLocaleString()} sources`);
  appendSummaryRow("Processed sources", pluralise(processedCount, "source"));
  if (state.cleanedTokenCountText) {
    appendSummaryRow("Cleaned token count", state.cleanedTokenCountText);
  }
}

export function setActiveWorkbench(workbench: WorkbenchId): void {
  state.activeWorkbench = workbench;

  const analyseIsActive = workbench === "analyse";
  dom.prepareWorkbench.classList.toggle("hidden", analyseIsActive);
  dom.analyseWorkbench.classList.toggle("hidden", !analyseIsActive);
  dom.prepareWorkbench.setAttribute("aria-hidden", analyseIsActive ? "true" : "false");
  dom.analyseWorkbench.setAttribute("aria-hidden", analyseIsActive ? "false" : "true");

  dom.workbenchPrepare.classList.toggle("active", !analyseIsActive);
  dom.workbenchAnalyse.classList.toggle("active", analyseIsActive);
  dom.workbenchPrepare.setAttribute("aria-selected", analyseIsActive ? "false" : "true");
  dom.workbenchAnalyse.setAttribute("aria-selected", analyseIsActive ? "true" : "false");
  dom.workbenchPrepare.tabIndex = analyseIsActive ? -1 : 0;
  dom.workbenchAnalyse.tabIndex = analyseIsActive ? 0 : -1;

  if (analyseIsActive) {
    renderAnalyseWorkbench();
  }
}

export function resetWorkbenchForProject(): void {
  dom.workbenchSwitcher.classList.remove("hidden");
  setActiveWorkbench("prepare");
}

export function hideWorkbenchSwitcher(): void {
  dom.workbenchSwitcher.classList.add("hidden");
  setActiveWorkbench("prepare");
}

export function refreshAnalyseWorkbench(): void {
  if (state.activeWorkbench === "analyse") {
    renderAnalyseWorkbench();
  }
}

export function initWorkbenchSwitcher(): void {
  function focusWorkbench(workbench: WorkbenchId): void {
    setActiveWorkbench(workbench);
    const button = workbench === "prepare" ? dom.workbenchPrepare : dom.workbenchAnalyse;
    button.focus();
  }

  function handleWorkbenchKeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowLeft" || event.key === "Home") {
      event.preventDefault();
      focusWorkbench("prepare");
    } else if (event.key === "ArrowRight" || event.key === "End") {
      event.preventDefault();
      focusWorkbench("analyse");
    }
  }

  dom.workbenchPrepare.addEventListener("click", () => {
    setActiveWorkbench("prepare");
  });
  dom.workbenchAnalyse.addEventListener("click", () => {
    setActiveWorkbench("analyse");
  });
  dom.workbenchPrepare.addEventListener("keydown", handleWorkbenchKeydown);
  dom.workbenchAnalyse.addEventListener("keydown", handleWorkbenchKeydown);

  const toolsList = document.getElementById("analyse-tools-list");
  if (!toolsList) {
    return;
  }

  toolsList.replaceChildren();
  for (const tool of FUTURE_ANALYSIS_TOOLS) {
    const item = document.createElement("li");
    item.className = "analyse-tool-item";
    item.setAttribute("aria-disabled", "true");

    const name = document.createElement("span");
    name.textContent = tool;

    const status = document.createElement("span");
    status.className = "analyse-tool-status";
    status.textContent = "Coming next";

    item.append(name, status);
    toolsList.appendChild(item);
  }
}
