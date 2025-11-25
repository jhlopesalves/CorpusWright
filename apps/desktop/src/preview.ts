import { invoke } from "@tauri-apps/api/core";

import { dom } from "./dom";
import { state } from "./state";
import { highlightPreviewText } from "./utils";
import type { CombinedPreview } from "./generated/CombinedPreview.js";
import type { FilePreview } from "./generated/FilePreview.js";
import type { PdfPageRangePage } from "./generated/PdfPageRangePage.js";
import type { PdfPageRangeResult } from "./generated/PdfPageRangeResult.js";

const PREVIEW_CHUNK_SIZE = 50;
const FULL_OCR_CHUNK_SIZE = 2;
const FULL_OCR_MAX_CHARS_PER_PAGE = 50_000;

let previewObserver: IntersectionObserver | null = null;
let chunkSentinelObserver: IntersectionObserver | null = null;
let fullOcrRunId = 0;
let fullOcrCancelRequested = false;

interface PreviewCallbacks {
  onPreviewTabChanged: () => void;
}

export function invalidatePreviewSession(): void {
  state.previewGeneration += 1;
  state.activePreviewGeneration = state.previewGeneration;
  invalidateFullOcrSession();
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
  invalidateFullOcrSession();
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
    if (!append) {
      renderFullOcrPreviewLauncher(indices, myVersion, myPreviewGeneration);
    }

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

function invalidateFullOcrSession(): void {
  fullOcrRunId += 1;
  fullOcrCancelRequested = true;
}

function selectedFullOcrPdfIndex(indices: number[]): number | null {
  if (indices.length !== 1) return null;
  const index = indices[0];
  const record = state.allFiles[index];
  if (!record || record.document_type !== "pdf") return null;
  if (state.activeCleaningConfig.pdf_text_source !== "ForceOcr") return null;
  if (state.activeCleaningConfig.pdf_ocr_quality !== "HighQuality") return null;
  return index;
}

function renderFullOcrPreviewLauncher(
  selectedIndices: number[],
  corpusVersion: number,
  previewGeneration: number,
): void {
  const index = selectedFullOcrPdfIndex(selectedIndices);
  if (index === null) return;

  const section = document.createElement("div");
  section.className = "full-ocr-section";

  const controls = document.createElement("div");
  controls.className = "full-ocr-controls";

  const title = document.createElement("div");
  title.className = "full-ocr-title";
  title.textContent = "Full OCR preview";

  const actions = document.createElement("div");
  actions.className = "full-ocr-actions";

  const runButton = document.createElement("button");
  runButton.type = "button";
  runButton.className = "secondary-btn";
  runButton.textContent = "Run full OCR preview";

  const cancelButton = document.createElement("button");
  cancelButton.type = "button";
  cancelButton.className = "secondary-btn hidden";
  cancelButton.textContent = "Cancel";
  cancelButton.disabled = true;

  actions.append(runButton, cancelButton);
  controls.append(title, actions);

  const progress = document.createElement("div");
  progress.className = "full-ocr-progress";
  progress.setAttribute("aria-live", "polite");

  const pageList = document.createElement("div");
  pageList.className = "full-ocr-page-list";

  runButton.addEventListener("click", () => {
    runFullOcrPreview(
      index,
      corpusVersion,
      previewGeneration,
      runButton,
      cancelButton,
      progress,
      pageList,
    );
  });

  cancelButton.addEventListener("click", () => {
    fullOcrCancelRequested = true;
    cancelButton.disabled = true;
    progress.textContent = "Cancelling after the current chunk...";
  });

  section.append(controls, progress, pageList);
  dom.processedPreviewContent.appendChild(section);
}

async function runFullOcrPreview(
  index: number,
  corpusVersion: number,
  previewGeneration: number,
  runButton: HTMLButtonElement,
  cancelButton: HTMLButtonElement,
  progress: HTMLDivElement,
  pageList: HTMLDivElement,
): Promise<void> {
  const runId = ++fullOcrRunId;
  fullOcrCancelRequested = false;
  runButton.disabled = true;
  cancelButton.disabled = false;
  cancelButton.classList.remove("hidden");
  pageList.replaceChildren();

  let startPageIndex = 0;
  let totalPages: number | null = null;

  try {
    while (!fullOcrCancelRequested) {
      progress.textContent = totalPages === null
        ? `OCR pages ${startPageIndex} / ?`
        : `OCR pages ${startPageIndex} / ${totalPages}`;

      const result = await invoke<PdfPageRangeResult>("extract_pdf_page_range_command", {
        index,
        corpusVersion,
        cleaningConfig: state.activeCleaningConfig,
        startPageIndex,
        pageCount: FULL_OCR_CHUNK_SIZE,
        pdfTextSource: "ForceOcr",
        ocrQuality: "HighQuality",
        maxCharsPerPage: FULL_OCR_MAX_CHARS_PER_PAGE,
      });

      if (
        runId !== fullOcrRunId ||
        corpusVersion !== state.currentCorpusVersion ||
        previewGeneration !== state.activePreviewGeneration
      ) {
        return;
      }

      totalPages = result.page_count;
      appendFullOcrPages(pageList, result.pages);
      startPageIndex = result.end_page_index;
      progress.textContent = `OCR pages ${startPageIndex} / ${totalPages}`;

      if (result.pages.length === 0 || startPageIndex >= totalPages) {
        break;
      }
    }

    if (fullOcrCancelRequested) {
      const total = totalPages === null ? "?" : totalPages.toString();
      progress.textContent = `OCR cancelled at ${startPageIndex} / ${total} pages.`;
    } else if (totalPages !== null) {
      progress.textContent = `OCR pages ${totalPages} / ${totalPages}`;
    }
  } catch (error) {
    if (
      runId === fullOcrRunId &&
      corpusVersion === state.currentCorpusVersion &&
      previewGeneration === state.activePreviewGeneration
    ) {
      progress.textContent = `OCR error: ${error}`;
    }
  } finally {
    if (runId === fullOcrRunId) {
      runButton.disabled = false;
      cancelButton.disabled = true;
      cancelButton.classList.add("hidden");
    }
  }
}

function appendFullOcrPages(container: HTMLDivElement, pages: PdfPageRangePage[]): void {
  for (const page of pages) {
    const card = document.createElement("div");
    card.className = "full-ocr-page-card";

    const header = document.createElement("div");
    header.className = "full-ocr-page-header";

    const title = document.createElement("div");
    title.className = "full-ocr-page-title";
    title.textContent = `Page ${page.page_number}`;

    const meta = document.createElement("div");
    meta.className = "full-ocr-page-meta";
    meta.textContent = `${page.char_count.toLocaleString()} chars`;

    header.append(title, meta);
    card.appendChild(header);

    if (page.error) {
      const error = document.createElement("div");
      error.className = "full-ocr-page-error";
      error.textContent = page.error;
      card.appendChild(error);
    } else {
      const body = document.createElement("div");
      body.className = "full-ocr-page-body";
      body.dataset.originalText = page.text;
      body.innerHTML = highlightPreviewText(page.text, state.currentSearchQuery);
      card.appendChild(body);
    }

    if (page.warnings.length > 0) {
      const warnings = document.createElement("ul");
      warnings.className = "full-ocr-page-warnings";
      for (const warning of page.warnings) {
        const item = document.createElement("li");
        item.textContent = warning;
        warnings.appendChild(item);
      }
      card.appendChild(warnings);
    }

    container.appendChild(card);
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
