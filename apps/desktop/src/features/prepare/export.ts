/**
 * Coordinates processed-corpus export and progress presentation.
 *
 * The backend owns file production and manifest provenance; this controller owns destination
 * selection, the active cleaning snapshot, and user-visible completion state.
 */
import { invoke, listen, open, type UnlistenFn } from "./ipc";

import { dom } from "../../app/dom";
import { state } from "../../app/state";
import { effectiveCleaningConfig } from "./cleaning/effective-config";
import { sanitizeFolderName } from "../project/folder-name";
import type { ExportReport } from "../../generated/ExportReport.js";

export async function exportProcessedCorpus(): Promise<void> {
  const fileCount = state.activeProject ? state.projectCorpusFileCount : state.allFiles.length;
  if (fileCount === 0) {
    dom.statusBar.textContent = "No files loaded. Open a corpus before saving.";
    return;
  }

  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: state.currentCorpusRoot ? state.currentCorpusRoot : undefined,
  });
  if (selected === null || Array.isArray(selected)) {
    dom.statusBar.textContent = "Save cancelled.";
    return;
  }

  const now = new Date();
  const pad = (n: number) => n.toString().padStart(2, "0");
  const timestamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  let corpusName = "";
  if (state.currentCorpusRoot) {
    const parts = state.currentCorpusRoot.replace(/\\/g, "/").split("/");
    const basename = parts[parts.length - 1] || "";
    corpusName = sanitizeFolderName(basename);
  }
  const exportDirName = corpusName
    ? `${corpusName}_corpuswright_processed_${timestamp}`
    : `CorpusWright_processed_${timestamp}`;
  const separator = selected.includes("\\") ? "\\" : "/";
  const targetDir = `${selected}${selected.endsWith(separator) ? "" : separator}${exportDirName}`;

  dom.statusBar.textContent = "Saving processed corpus... 0%";
  document.body.classList.add("is-exporting");

  await new Promise(requestAnimationFrame);
  await new Promise(resolve => setTimeout(resolve, 50));

  let unlisten: UnlistenFn | null = null;
  try {
    unlisten = await listen<{ current: number; total: number; current_file: string }>("export-progress", (event) => {
      const { current, total, current_file } = event.payload;
      const pct = Math.round((current / total) * 100);
      dom.statusBar.textContent = `Saving... ${pct}% (${current}/${total}) — ${current_file}`;
    });

    const myVersion = state.currentCorpusVersion;
    const indices = state.activeProject
      ? null
      : Array.from({ length: state.allFiles.length }, (_, corpusIndex) => corpusIndex);

    const report: ExportReport = await invoke("export_corpus_command", {
      indices,
      corpusVersion: myVersion,
      outputDir: targetDir,
      cleaningConfig: effectiveCleaningConfig()
    });

    if (myVersion !== state.currentCorpusVersion) return;

    dom.statusBar.textContent = `Saved processed corpus: ${report.files_exported} files written to ${targetDir}`;
  } catch (error) {
    dom.statusBar.textContent = `Save error: ${error}`;
    console.error("Save error", error);
  } finally {
    if (unlisten) unlisten();
    document.body.classList.remove("is-exporting");
  }
}

export function initExport(): void {
  dom.menuSaveCorpus.addEventListener("click", exportProcessedCorpus);
}
