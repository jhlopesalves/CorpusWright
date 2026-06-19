import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { dom } from "./dom";
import { updateFileList } from "./file-list";
import {
  hasProjectSourceSelection,
  loadProjectSourcesForActiveProject,
  projectSourceSelectionDescriptor,
  projectSourceSelectionCount,
} from "./sources";
import { state } from "./state";

interface ProcessSourcesCallbacks {
  updateWordCount: () => void;
  schedulePreviewUpdate: (delay: number) => void;
}

interface ProcessSourcesProgress {
  current: number;
  total: number;
  currentFile: string;
}

interface ProcessSourcesReport {
  totalCount: number;
  processedCount: number;
  failedCount: number;
  skippedCount: number;
  outputDir: string;
}

let callbacks: ProcessSourcesCallbacks = {
  updateWordCount: () => {},
  schedulePreviewUpdate: () => {},
};

function processScopeLabel(): string {
  if (!hasProjectSourceSelection()) {
    return "all sources";
  }
  const count = projectSourceSelectionCount();
  return `${count.toLocaleString()} selected source${count === 1 ? "" : "s"}`;
}

function processCompletionMessage(report: ProcessSourcesReport): string {
  const parts = [
    `${report.processedCount.toLocaleString()} processed`,
  ];
  if (report.skippedCount > 0) {
    parts.push(`${report.skippedCount.toLocaleString()} need extraction`);
  }
  if (report.failedCount > 0) {
    parts.push(`${report.failedCount.toLocaleString()} failed`);
  }
  return `Processed ${report.totalCount.toLocaleString()} source${report.totalCount === 1 ? "" : "s"}: ${parts.join(", ")}.`;
}

export async function processProjectSources(): Promise<void> {
  if (!state.activeProject) {
    dom.statusBar.textContent = "Open a CorpusWright project before processing sources.";
    return;
  }

  const selection = hasProjectSourceSelection()
    ? projectSourceSelectionDescriptor()
    : null;

  dom.prepareProcessSources.disabled = true;
  dom.statusBar.textContent = `Processing ${processScopeLabel()}...`;
  document.body.style.cursor = "wait";

  let unlisten: UnlistenFn | null = null;
  try {
    unlisten = await listen<ProcessSourcesProgress>("process-sources-progress", ({ payload }) => {
      dom.statusBar.textContent =
        `Processing ${payload.current.toLocaleString()} / ${payload.total.toLocaleString()}: ${payload.currentFile}`;
    });

    const report = await invoke<ProcessSourcesReport>("process_project_sources_command", {
      rootPath: state.activeProject.rootPath,
      selection,
      corpusVersion: state.currentCorpusVersion,
      cleaningConfig: state.activeCleaningConfig,
    });

    await loadProjectSourcesForActiveProject(state.projectSourcePageOffset);
    updateFileList(false);
    callbacks.updateWordCount();
    callbacks.schedulePreviewUpdate(0);
    dom.statusBar.textContent = processCompletionMessage(report);
  } catch (error) {
    dom.statusBar.textContent = `Processing error: ${error}`;
    console.error(error);
  } finally {
    unlisten?.();
    dom.prepareProcessSources.disabled = state.activeProject === null
      || (state.projectSourceSummary?.loadable_count ?? state.projectCorpusFileCount) === 0;
    document.body.style.cursor = "default";
  }
}

export function initProcessSources(nextCallbacks: ProcessSourcesCallbacks): void {
  callbacks = nextCallbacks;
  dom.prepareProcessSources.addEventListener("click", () => {
    void processProjectSources();
  });
}
