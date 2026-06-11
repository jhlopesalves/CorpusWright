import { invoke } from "@tauri-apps/api/core";

import { dom } from "./dom";
import { state } from "./state";
import { highlightPreviewText } from "./utils";
import type { CombinedPreview } from "./generated/CombinedPreview.js";
import type { FilePreview } from "./generated/FilePreview.js";

const PREVIEW_CHUNK_SIZE = 50;

let previewObserver: IntersectionObserver | null = null;
let chunkSentinelObserver: IntersectionObserver | null = null;

interface PreviewCallbacks {
  onPreviewTabChanged: () => void;
}

export function invalidatePreviewSession(): void {
  state.previewGeneration += 1;
  state.activePreviewGeneration = state.previewGeneration;
}

export function schedulePreviewUpdate(delay: number): void {
  if (state.debounceTimer) clearTimeout(state.debounceTimer);
  state.debounceTimer = window.setTimeout(updatePreview, delay);
}

export function initPreviewTabs(callbacks: PreviewCallbacks): void {
  dom.previewTabs.forEach(tab => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-target");
      dom.previewTabs.forEach(t => {
        t.classList.remove("active");
        t.setAttribute("aria-selected", "false");
      });
      dom.previewTabContents.forEach(c => c.classList.remove("active"));

      tab.classList.add("active");
      tab.setAttribute("aria-selected", "true");
      document.getElementById(target!)?.classList.add("active");

      callbacks.onPreviewTabChanged();
    });
  });
}

export async function updatePreview(): Promise<void> {
  const myVersion = state.currentCorpusVersion;
  const myPreviewGeneration = ++state.previewGeneration;
  state.activePreviewGeneration = myPreviewGeneration;

  if (state.selectedCorpusIndices.size === 0) {
    if (myVersion !== state.currentCorpusVersion) return;
    dom.previewContent.innerHTML = "Select files from the left panel to preview their contents.";
    dom.processedPreviewContent.innerHTML = "Select files from the left panel to preview processed text.";
    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    return;
  }

  const selectedIndices = Array.from(state.selectedCorpusIndices);
  state.currentPreviewOffset = 0;

  dom.previewLoadingOverlay.style.display = "flex";
  dom.statusBar.textContent = "Previewing processed text...";

  await fetchAndRenderPreviewChunk(selectedIndices, state.currentPreviewOffset, false, myVersion, myPreviewGeneration);
}

async function fetchAndRenderPreviewChunk(
  indices: number[],
  offset: number,
  append: boolean,
  myVersion: number,
  myPreviewGeneration: number
): Promise<void> {
  if (state.isFetchingPreview) return;
  state.isFetchingPreview = true;

  const chunkIndices = indices.slice(offset, offset + PREVIEW_CHUNK_SIZE);
  if (chunkIndices.length === 0) {
    state.isFetchingPreview = false;
    return;
  }

  const maxChars = indices.length === 1 ? 10000000 : 5000;

  try {
    const [original, processed] = await Promise.all([
      invoke<CombinedPreview>("preview_files_command", {
        indices: chunkIndices,
        corpusVersion: myVersion,
        maxCharsPerFile: maxChars,
        includePaths: true,
        maxFiles: chunkIndices.length
      }),
      invoke<CombinedPreview>("preview_processed_files_command", {
        indices: chunkIndices,
        corpusVersion: myVersion,
        maxCharsPerFile: maxChars,
        includePaths: true,
        maxFiles: chunkIndices.length,
        cleaningConfig: state.activeCleaningConfig
      })
    ]);

    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;

    renderPreviewCards(dom.previewContent, original.files, append, offset, indices.length);
    renderPreviewCards(dom.processedPreviewContent, processed.files, append, offset, indices.length);

    highlightRenderedCards();

    dom.statusBar.textContent = `Processed preview ready for ${indices.length} selected files. (Loaded ${offset + chunkIndices.length})`;
    state.currentPreviewOffset += chunkIndices.length;
  } catch (error) {
    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;
    if (!append) {
      dom.previewContent.textContent = `Error loading preview: ${error}`;
      dom.processedPreviewContent.textContent = `Error loading preview: ${error}`;
    }
    dom.statusBar.textContent = "Error loading preview.";
  } finally {
    dom.previewLoadingOverlay.style.display = "none";
    state.isFetchingPreview = false;
  }
}

function renderPreviewCards(
  container: HTMLDivElement,
  files: FilePreview[],
  append: boolean,
  offset: number,
  totalFiles: number
): void {
  if (!append) {
    container.innerHTML = "";
  }

  const existingSentinel = container.querySelector(".chunk-sentinel");
  if (existingSentinel) existingSentinel.remove();

  if (!previewObserver) {
    previewObserver = new IntersectionObserver((entries, observer) => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          const body = entry.target as HTMLElement;
          const text = body.dataset.originalText || "";
          body.innerHTML = highlightPreviewText(text, state.currentSearchQuery);
          observer.unobserve(body);

          body.style.minHeight = "auto";
        }
      });
    }, { rootMargin: "200px 0px" });
  }

  files.forEach((file, index) => {
    const card = document.createElement("div");
    card.className = "file-card";

    const header = document.createElement("div");
    header.className = "file-card-header";

    const titleArea = document.createElement("div");
    titleArea.className = "file-card-title-area";

    const fileNum = document.createElement("div");
    fileNum.className = "file-card-num";
    fileNum.textContent = `File ${offset + index + 1} of ${totalFiles}`;

    const filename = document.createElement("div");
    filename.className = "file-card-title";
    const parts = file.relative_path.split(/[\/]/);
    filename.textContent = parts[parts.length - 1];

    const path = document.createElement("div");
    path.className = "file-card-path";
    path.textContent = file.source_path;

    titleArea.appendChild(fileNum);
    titleArea.appendChild(filename);
    titleArea.appendChild(path);

    const badges = document.createElement("div");
    badges.className = "file-card-badges";

    if (file.truncated) {
      const truncBadge = document.createElement("span");
      truncBadge.className = "badge";
      truncBadge.textContent = "Preview capped";
      badges.appendChild(truncBadge);
      header.appendChild(badges);
    }

    header.appendChild(titleArea);

    const body = document.createElement("div");
    body.className = "file-card-body";
    body.style.minHeight = "100px";
    body.dataset.originalText = file.text;

    previewObserver!.observe(body);

    card.appendChild(header);
    card.appendChild(body);
    container.appendChild(card);
  });

  if (offset + files.length < totalFiles) {
    const sentinel = document.createElement("div");
    sentinel.className = "chunk-sentinel";
    sentinel.style.height = "10px";
    container.appendChild(sentinel);

    if (!chunkSentinelObserver) {
      chunkSentinelObserver = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
          if (entry.isIntersecting) {
            const myVersion = state.currentCorpusVersion;
            const myPreviewGeneration = state.activePreviewGeneration;
            const selectedIndices = Array.from(state.selectedCorpusIndices);
            fetchAndRenderPreviewChunk(selectedIndices, state.currentPreviewOffset, true, myVersion, myPreviewGeneration);
          }
        });
      }, { rootMargin: "400px 0px" });
    }
    chunkSentinelObserver.observe(sentinel);
  }
}

function highlightRenderedCards(): void {
  const activeTabContent = document.querySelector(".right-panel .tab-content.active .preview-cards-container");
  if (!activeTabContent) return;
  const bodies = activeTabContent.querySelectorAll(".file-card-body");
  if (!state.currentSearchQuery) {
    bodies.forEach((body) => {
      const htmlBody = body as HTMLElement;
      const text = htmlBody.dataset.originalText || "";
      htmlBody.innerHTML = highlightPreviewText(text, "");
    });
  }
}
