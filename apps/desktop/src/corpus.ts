import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { dom } from "./dom";
import { updateFileList } from "./file-list";
import { state } from "./state";
import type { CorpusLoadResult, CorpusSummary } from "./types";

interface CorpusCallbacks {
  updateWordCount: () => void;
}

let callbacks: CorpusCallbacks = {
  updateWordCount: () => {},
};

function clearStateForLoad(): void {
  state.currentCorpusVersion = 0;
  state.allFiles = [];
  state.visibleFiles = [];
  state.selectedCorpusIndices.clear();
  dom.previewContent.textContent = "Select files from the left panel to preview their contents.";
  dom.processedPreviewContent.textContent = "Select files from the left panel to preview processed text.";
  updateFileList();
}

async function handleOpenDir(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    dom.statusBar.textContent = "Open directory cancelled.";
    return;
  }

  try {
    dom.statusBar.textContent = "Scanning...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("scan_directory_command", { path: selected });

    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
}

async function handleOpenFiles(): Promise<void> {
  const selected = await open({
    multiple: true,
    filters: [{ name: "Supported Documents", extensions: ["txt", "html", "htm", "docx", "pdf"] }]
  });
  if (selected === null || !Array.isArray(selected)) {
    dom.statusBar.textContent = "Open files cancelled.";
    return;
  }

  try {
    dom.statusBar.textContent = "Loading files...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("load_files_command", { paths: selected });

    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
}

function renderSummary(summary: CorpusSummary): void {
  const totalFiles = summary.files_supported;
  const sizeMB = summary.total_size_bytes / (1024 * 1024);
  const avgSizeBytes = totalFiles > 0 ? summary.total_size_bytes / totalFiles : 0;

  let avgSizeStr = "0 MB";
  if (avgSizeBytes > 0) {
    if (avgSizeBytes < 1024 * 1024) {
      avgSizeStr = (avgSizeBytes / 1024).toFixed(2) + " KB";
    } else {
      avgSizeStr = (avgSizeBytes / (1024 * 1024)).toFixed(2) + " MB";
    }
  }

  dom.corpusSummary.innerHTML = `
    <div class="summary-header">Corpus Summary</div>
    <div class="summary-grid">
      <div class="summary-metric">
        <div class="summary-value">${totalFiles}</div>
        <div class="summary-label">Total Files</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value">${sizeMB.toFixed(2)} MB</div>
        <div class="summary-label">Total Size</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value">${avgSizeStr}</div>
        <div class="summary-label">Average File</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value" id="summary-total-words">Calculating...</div>
        <div class="summary-label" id="summary-word-label">Cleaned Token Count</div>
      </div>
      <div class="summary-metric full-width">
        <div class="summary-value" id="summary-avg-words">Calculating...</div>
        <div class="summary-label">Avg Words / File</div>
      </div>
    </div>
    <div class="summary-diagnostics">
      Types: TXT ${summary.document_type_counts.text}, HTML ${summary.document_type_counts.html}, DOCX ${summary.document_type_counts.docx}, PDF ${summary.document_type_counts.pdf}
    </div>
  `;
}

export function initCorpusHandlers(nextCallbacks: CorpusCallbacks): void {
  callbacks = nextCallbacks;
  dom.menuOpenDir.addEventListener("click", handleOpenDir);
  dom.menuOpenFiles.addEventListener("click", handleOpenFiles);
}
