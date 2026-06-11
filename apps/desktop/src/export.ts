import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { dom } from "./dom";
import { state } from "./state";
import { sanitizeFolderName } from "./utils";
import type { ExportReport } from "./types";

async function handleExport(): Promise<void> {
  if (state.allFiles.length === 0) {
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
    ? `${corpusName}_corpusaid_processed_${timestamp}`
    : `CorpusAid_processed_${timestamp}`;
  const separator = selected.includes("\\") ? "\\" : "/";
  const targetDir = `${selected}${selected.endsWith(separator) ? "" : separator}${exportDirName}`;

  dom.statusBar.textContent = "Saving processed corpus... 0%";
  dom.menuSaveCorpus.style.pointerEvents = "none";
  dom.menuSaveCorpus.style.opacity = "0.5";
  document.body.style.cursor = "wait";
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
    const indices = Array.from(
      { length: state.allFiles.length },
      (_, corpusIndex) => corpusIndex
    );

    const report: ExportReport = await invoke("export_corpus_command", {
      indices,
      corpusVersion: myVersion,
      outputDir: targetDir,
      cleaningConfig: state.activeCleaningConfig
    });

    if (myVersion !== state.currentCorpusVersion) return;

    dom.statusBar.textContent = `Saved processed corpus: ${report.files_exported} files written to ${targetDir}`;
  } catch (error) {
    dom.statusBar.textContent = `Save error: ${error}`;
    console.error("Save error", error);
  } finally {
    if (unlisten) unlisten();
    dom.menuSaveCorpus.style.pointerEvents = "auto";
    dom.menuSaveCorpus.style.opacity = "1";
    document.body.style.cursor = "default";
    document.body.classList.remove("is-exporting");
  }
}

export function initExport(): void {
  dom.menuSaveCorpus.addEventListener("click", handleExport);
}
