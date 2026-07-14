import { invoke } from "@tauri-apps/api/core";

import { dom } from "../../../app/dom";
import { renderVisibleItems } from "../corpus/file-list";
import { state } from "../../../app/state";
import { effectiveCleaningConfig } from "../cleaning/effective-config";
import {
  hasProjectSourceSelection,
  isProjectSourceSelected,
  pdfHandlingLabel,
  primaryReasonLabel,
  reasonLabel,
  projectSourceSelectionDescriptor,
  sourceForPath,
  sourceHasExtractedText,
  sourceDisplayStatusLabel,
  sourceKindLabel,
  sourceReasonDescriptions,
  sourceStatusLabel,
} from "../sources";
import { highlightPreviewText } from "./text";
import type { CombinedPreview } from "../../../generated/CombinedPreview.js";
import type { FilePreview } from "../../../generated/FilePreview.js";
import type { PdfPageRangePage } from "../../../generated/PdfPageRangePage.js";
import type { PdfPageRangeResult } from "../../../generated/PdfPageRangeResult.js";
import type { ProjectSource } from "../../../generated/ProjectSource.js";
import type { ProjectSourcePreviewItem } from "../../../generated/ProjectSourcePreviewItem.js";
import type { ProjectSourcePreviewViewport } from "../../../generated/ProjectSourcePreviewViewport.js";

const PREVIEW_CHUNK_SIZE = 50;
const FULL_OCR_CHUNK_SIZE = 2;
const FULL_OCR_MAX_CHARS_PER_PAGE = 50_000;
const PROJECT_SOURCE_PREVIEW_DEFAULT_ITEM_HEIGHT = 280;
const PROJECT_SOURCE_PREVIEW_OVERSCAN = 3;
const PROJECT_SOURCE_PREVIEW_TEXT_CAP = 2_000;
const PROJECT_SOURCE_PREVIEW_MAX_VIEWPORT_SIZE = 50;

let previewObserver: IntersectionObserver | null = null;
let chunkSentinelObserver: IntersectionObserver | null = null;
let fullOcrRunId = 0;
let fullOcrCancelRequested = false;
let projectSourcePreviewActive = false;
let projectSourcePreviewRequest = 0;
let projectSourcePreviewTimer: number | null = null;
let projectSourcePreviewEstimatedItemHeight = PROJECT_SOURCE_PREVIEW_DEFAULT_ITEM_HEIGHT;
let projectSourcePreviewLoading = false;
let projectSourcePreviewIdentity: string | null = null;
let projectSourcePreviewWindow: {
  start: number;
  end: number;
  total: number;
} | null = null;

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
  for (const content of [dom.previewContent, dom.processedPreviewContent]) {
    content.parentElement?.addEventListener("scroll", handleProjectSourcePreviewScroll);
  }
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

  const hasSourceSelection = state.activeProject !== null && hasProjectSourceSelection();
  if (hasSourceSelection) {
    await renderSelectedProjectSourcePreviewViewport(
      myVersion,
      myPreviewGeneration,
      state.focusedProjectSourceId !== null
        && isProjectSourceSelected(state.focusedProjectSourceId)
        && state.focusedProjectSourceOffset !== null
        ? state.focusedProjectSourceOffset * projectSourcePreviewEstimatedItemHeight
        : state.projectSourcePreviewScrollTop,
    );
    return;
  }

  leaveProjectSourcePreviewMode();

  if (state.selectedCorpusIndices.size === 0) {
    if (myVersion !== state.currentCorpusVersion) return;
    dom.previewContent.innerHTML = "Select files from the left panel to preview their contents.";
    dom.processedPreviewContent.innerHTML = "Select files from the left panel to preview processed text.";
    const loadedCount = state.activeProject ? state.projectCorpusFileCount : state.allFiles.length;
    dom.statusBar.textContent = `Loaded ${loadedCount.toLocaleString()} files.`;
    return;
  }

  const selectedIndices = Array.from(state.selectedCorpusIndices);
  const sourcesNeedingExtraction: { name: string; status: string; statusLabel: string }[] = [];

  for (const idx of selectedIndices) {
    const record = state.allFiles[idx];
    if (!record) continue;
    const source = sourceForPath(record.source_path);
    if (!source) continue;
    if (source.kind === "pdf" && !sourceHasExtractedText(source)) {
      sourcesNeedingExtraction.push({
        status: source.status,
        statusLabel: sourceStatusLabel(source.status),
        name: source.file_name,
      });
    }
  }

  if (sourcesNeedingExtraction.length > 0) {
    renderTextExtractionRequiredCards(dom.previewContent, sourcesNeedingExtraction, "original");
    renderTextExtractionRequiredCards(dom.processedPreviewContent, sourcesNeedingExtraction, "processed");
    dom.previewLoadingOverlay.classList.add("hidden");
    dom.statusBar.textContent = "Text has not been extracted yet.";
    return;
  }

  state.currentPreviewOffset = 0;

  dom.previewLoadingText.textContent = "Loading preview...";
  dom.previewLoadingOverlay.classList.remove("hidden");
  dom.statusBar.textContent = "Previewing...";

  await fetchAndRenderPreviewChunk(selectedIndices, state.currentPreviewOffset, false, myVersion, myPreviewGeneration);
}

async function renderSelectedProjectSourcePreviewViewport(
  corpusVersion: number,
  previewGeneration: number,
  scrollTop: number,
): Promise<void> {
  projectSourcePreviewActive = true;
  const request = ++projectSourcePreviewRequest;
  projectSourcePreviewLoading = true;
  const selection = projectSourceSelectionDescriptor();
  const previewTextCap = !selection.all_matching && selection.ids.length === 1
    ? null
    : PROJECT_SOURCE_PREVIEW_TEXT_CAP;
  const requestIdentity = JSON.stringify({
    rootPath: state.activeProject?.rootPath ?? null,
    selection,
    corpusVersion,
    cleaningConfig: effectiveCleaningConfig(),
  });
  const showBlockingOverlay = projectSourcePreviewIdentity !== requestIdentity
    || projectSourcePreviewWindow === null;
  const originalScroll = dom.previewContent.parentElement;
  const processedScroll = dom.processedPreviewContent.parentElement;
  const viewportHeight = Math.max(
    originalScroll?.clientHeight ?? 0,
    processedScroll?.clientHeight ?? 0,
    projectSourcePreviewEstimatedItemHeight,
  );
  const { viewportStart, viewportSize } = projectSourcePreviewViewport(scrollTop, viewportHeight);
  state.projectSourcePreviewScrollTop = scrollTop;
  if (originalScroll && Math.abs(originalScroll.scrollTop - scrollTop) > 1) {
    originalScroll.scrollTop = scrollTop;
  }
  if (processedScroll && Math.abs(processedScroll.scrollTop - scrollTop) > 1) {
    processedScroll.scrollTop = scrollTop;
  }
  if (showBlockingOverlay) {
    dom.previewLoadingText.textContent = "Preparing preview...";
    dom.previewLoadingOverlay.classList.remove("hidden");
  }
  dom.statusBar.textContent = "Previewing selected sources...";
  let requestSucceeded = false;

  try {
    const requestArgs = {
      rootPath: state.activeProject?.rootPath,
      selection,
      viewportStart,
      viewportSize,
      previewTextCap,
      corpusVersion,
      cleaningConfig: effectiveCleaningConfig(),
    };
    const [original, processed] = await Promise.all([
      invoke<ProjectSourcePreviewViewport>("query_selected_source_preview_viewport_command", {
        ...requestArgs,
        processed: false,
      }),
      invoke<ProjectSourcePreviewViewport>("query_selected_source_preview_viewport_command", {
        ...requestArgs,
        processed: true,
      }),
    ]);
    if (
      request !== projectSourcePreviewRequest
      || corpusVersion !== state.currentCorpusVersion
      || previewGeneration !== state.activePreviewGeneration
    ) return;

    renderProjectSourcePreviewItems(dom.previewContent, original, "original");
    renderProjectSourcePreviewItems(dom.processedPreviewContent, processed, "processed");
    projectSourcePreviewWindow = {
      start: original.viewport_start,
      end: original.viewport_start + original.items.length,
      total: original.total_selected,
    };
    projectSourcePreviewIdentity = requestIdentity;
    requestSucceeded = true;
    dom.statusBar.textContent = `Preview ready for ${original.total_selected.toLocaleString()} selected source${original.total_selected === 1 ? "" : "s"}.`;
  } catch (error) {
    if (
      request !== projectSourcePreviewRequest
      || corpusVersion !== state.currentCorpusVersion
      || previewGeneration !== state.activePreviewGeneration
    ) return;
    if (showBlockingOverlay) {
      dom.previewContent.textContent = `Error loading source preview: ${error}`;
      dom.processedPreviewContent.textContent = `Error loading processed preview: ${error}`;
    }
    dom.statusBar.textContent = "Error loading source preview.";
  } finally {
    if (request === projectSourcePreviewRequest) {
      dom.previewLoadingOverlay.classList.add("hidden");
      projectSourcePreviewLoading = false;
      const activeScroll = dom.previewContent.parentElement?.classList.contains("active")
        ? dom.previewContent.parentElement
        : dom.processedPreviewContent.parentElement;
      if (
        requestSucceeded
        && projectSourcePreviewActive
        && activeScroll
        && projectSourcePreviewNeedsLoad(
          state.projectSourcePreviewScrollTop,
          activeScroll.clientHeight,
        )
      ) {
        scheduleProjectSourcePreviewViewportLoad();
      }
    }
  }
}

function renderProjectSourcePreviewItems(
  container: HTMLDivElement,
  viewport: ProjectSourcePreviewViewport,
  mode: "original" | "processed",
): void {
  container.classList.add("project-source-preview-viewport");
  container.style.removeProperty("height");
  container.replaceChildren();

  const topSpacer = createProjectSourcePreviewSpacer(viewport.viewport_start);
  const layer = document.createElement("div");
  layer.className = "project-source-preview-layer";
  for (const [index, item] of viewport.items.entries()) {
    layer.appendChild(createProjectSourcePreviewViewportCard(
      item,
      viewport.viewport_start + index,
      viewport.total_selected,
      mode,
    ));
  }
  const remainingItems = Math.max(
    0,
    viewport.total_selected - viewport.viewport_start - viewport.items.length,
  );
  const bottomSpacer = createProjectSourcePreviewSpacer(remainingItems);
  container.append(topSpacer, layer, bottomSpacer);

  if (viewport.viewport_start === 0 && container.parentElement?.classList.contains("active")) {
    updateProjectSourcePreviewItemHeight(layer, viewport.items.length);
  }
}

function createProjectSourcePreviewSpacer(itemCount: number): HTMLDivElement {
  const spacer = document.createElement("div");
  spacer.className = "project-source-preview-spacer";
  spacer.dataset.itemCount = itemCount.toString();
  spacer.style.height = `${itemCount * projectSourcePreviewEstimatedItemHeight}px`;
  return spacer;
}

function updateProjectSourcePreviewItemHeight(layer: HTMLDivElement, itemCount: number): void {
  if (itemCount === 0) return;
  const measuredItemHeight = Math.ceil(layer.getBoundingClientRect().height / itemCount);
  if (measuredItemHeight <= 0) return;

  projectSourcePreviewEstimatedItemHeight = Math.max(
    PROJECT_SOURCE_PREVIEW_DEFAULT_ITEM_HEIGHT,
    measuredItemHeight,
  );
  for (const container of [dom.previewContent, dom.processedPreviewContent]) {
    for (const spacer of container.querySelectorAll<HTMLElement>(".project-source-preview-spacer")) {
      const spacerItemCount = Number.parseInt(spacer.dataset.itemCount ?? "0", 10);
      spacer.style.height = `${spacerItemCount * projectSourcePreviewEstimatedItemHeight}px`;
    }
  }
}

function createProjectSourcePreviewViewportCard(
  item: ProjectSourcePreviewItem,
  sourceIndex: number,
  totalSelected: number,
  mode: "original" | "processed",
): HTMLElement {
  if (!item.preview) {
    const card = createProjectSourceDetailCard(item.source, mode);
    if (item.preview_error) {
      const body = card.querySelector<HTMLElement>(".file-card-body");
      if (body) {
        body.textContent = isPendingProjectSourcePreview(item.preview_error)
          ? "Preparing preview..."
          : `Preview unavailable: ${item.preview_error}`;
      }
    }
    card.classList.add("project-source-preview-card");
    return card;
  }

  const card = document.createElement("div");
  card.className = "file-card project-source-preview-card";
  const header = document.createElement("div");
  header.className = "file-card-header";
  const titleArea = document.createElement("div");
  titleArea.className = "file-card-title-area";
  const number = document.createElement("div");
  number.className = "file-card-num";
  number.textContent = `Source ${sourceIndex + 1} of ${totalSelected.toLocaleString()}`;
  const title = document.createElement("div");
  title.className = "file-card-title";
  title.textContent = item.source.file_name;
  const path = document.createElement("div");
  path.className = "file-card-path";
  path.textContent = item.source.path;
  titleArea.append(number, title, path);
  header.appendChild(titleArea);

  const badges = document.createElement("div");
  badges.className = "file-card-badges";
  for (const label of projectSourceDetailBadges(item.source)) {
    const badge = document.createElement("span");
    badge.className = "badge";
    badge.textContent = label;
    badges.appendChild(badge);
  }
  if (item.preview.truncated) {
    const truncationBadge = document.createElement("span");
    truncationBadge.className = "badge";
    truncationBadge.textContent = "Preview capped";
    badges.appendChild(truncationBadge);
  }
  header.appendChild(badges);

  const body = document.createElement("div");
  body.className = "file-card-body";
  body.innerHTML = highlightPreviewText(item.preview.text, state.currentSearchQuery);
  card.append(header, body);
  if (item.preview.truncated) {
    card.appendChild(createPreviewCapNotice(mode, false));
  }
  return card;
}

function isPendingProjectSourcePreview(error: string): boolean {
  return error.toLowerCase().includes("selected project source is not loaded");
}

function handleProjectSourcePreviewScroll(event: Event): void {
  if (!projectSourcePreviewActive || !hasProjectSourceSelection()) return;
  const target = event.currentTarget as HTMLElement;
  if (!target.classList.contains("active")) return;
  state.projectSourcePreviewScrollTop = target.scrollTop;
  state.focusedProjectSourceId = null;
  state.focusedProjectSourceOffset = null;
  if (projectSourcePreviewTimer !== null) {
    window.clearTimeout(projectSourcePreviewTimer);
    projectSourcePreviewTimer = null;
  }
  if (
    projectSourcePreviewLoading
    || !projectSourcePreviewNeedsLoad(target.scrollTop, target.clientHeight)
  ) return;

  scheduleProjectSourcePreviewViewportLoad();
}

function scheduleProjectSourcePreviewViewportLoad(): void {
  if (projectSourcePreviewTimer !== null) window.clearTimeout(projectSourcePreviewTimer);
  projectSourcePreviewTimer = window.setTimeout(() => {
    projectSourcePreviewTimer = null;
    void renderSelectedProjectSourcePreviewViewport(
      state.currentCorpusVersion,
      state.activePreviewGeneration,
      state.projectSourcePreviewScrollTop,
    );
  }, 50);
}

function projectSourcePreviewViewport(
  scrollTop: number,
  viewportHeight: number,
): { viewportStart: number; viewportSize: number } {
  const viewportStart = Math.max(
    0,
    Math.floor(scrollTop / projectSourcePreviewEstimatedItemHeight) - PROJECT_SOURCE_PREVIEW_OVERSCAN,
  );
  const viewportSize = Math.min(
    PROJECT_SOURCE_PREVIEW_MAX_VIEWPORT_SIZE,
    Math.ceil(viewportHeight / projectSourcePreviewEstimatedItemHeight)
      + 2 * PROJECT_SOURCE_PREVIEW_OVERSCAN,
  );
  return { viewportStart, viewportSize };
}

function projectSourcePreviewNeedsLoad(scrollTop: number, viewportHeight: number): boolean {
  if (!projectSourcePreviewWindow) return true;
  const { start, end, total } = projectSourcePreviewWindow;
  if (start === 0 && end >= total) return false;

  const { viewportStart, viewportSize } = projectSourcePreviewViewport(scrollTop, viewportHeight);
  const requestedStart = Math.min(viewportStart, total);
  const requestedEnd = Math.min(total, requestedStart + viewportSize);
  return requestedStart < start || requestedEnd > end;
}

function leaveProjectSourcePreviewMode(): void {
  projectSourcePreviewActive = false;
  projectSourcePreviewLoading = false;
  if (projectSourcePreviewTimer !== null) {
    window.clearTimeout(projectSourcePreviewTimer);
    projectSourcePreviewTimer = null;
  }
  projectSourcePreviewIdentity = null;
  projectSourcePreviewWindow = null;
  projectSourcePreviewRequest += 1;
  for (const container of [dom.previewContent, dom.processedPreviewContent]) {
    container.classList.remove("project-source-preview-viewport");
    container.style.removeProperty("height");
  }
}

function createProjectSourceDetailCard(source: ProjectSource, mode: "original" | "processed"): HTMLElement {
  const card = document.createElement("div");
  card.className = "file-card";

  const header = document.createElement("div");
  header.className = "file-card-header";

  const titleArea = document.createElement("div");
  titleArea.className = "file-card-title-area";

  const title = document.createElement("div");
  title.className = "file-card-title";
  title.textContent = source.file_name;

  const path = document.createElement("div");
  path.className = "file-card-path";
  path.textContent = source.path;

  titleArea.append(title, path);
  header.appendChild(titleArea);

  const badges = document.createElement("div");
  badges.className = "file-card-badges";
  for (const label of projectSourceDetailBadges(source)) {
    const badge = document.createElement("span");
    badge.className = "badge";
    badge.textContent = label;
    badges.appendChild(badge);
  }
  if (badges.children.length > 0) {
    header.appendChild(badges);
  }

  const body = document.createElement("div");
  body.className = "file-card-body";
  for (const line of projectSourceDetailLines(source, mode)) {
    const paragraph = document.createElement("p");
    paragraph.textContent = line;
    body.appendChild(paragraph);
  }

  card.append(header, body);
  return card;
}

function projectSourceDetailBadges(source: ProjectSource): string[] {
  const labels = [
    sourceKindLabel(source),
    sourceDisplayStatusLabel(source),
  ];
  const handling = pdfHandlingLabel(source);
  if (handling) {
    labels.push(handling);
  }
  // Only add reason badge for concrete failures/errors, not generic warnings
  if (source.status === "failed") {
    const reason = primaryReasonLabel(source);
    if (reason) {
      labels.push(reason);
    }
  } else {
    const reasonCodes = source.reason_codes;
    const concreteCodes = reasonCodes.filter(code =>
      code === "extraction_cancelled" ||
      code === "text_extraction_failed" ||
      code === "render_size_clamped" ||
      code === "many_repeated_artifacts" ||
      code === "ocr_generated" ||
      code === "ocr_review_recommended" ||
      code === "layout_heavy_profile" ||
      code === "layout_warning" ||
      code === "low_text_density"
    );
    if (concreteCodes.length > 0) {
      labels.push(reasonLabel(concreteCodes[0]));
    }
  }
  return labels;
}

function projectSourceDetailLines(source: ProjectSource, mode: "original" | "processed"): string[] {
  const lines: string[] = [];
  const reasonLabels = source.reason_codes.map(reasonLabel);
  const reasonDescriptions = sourceReasonDescriptions(source);
  const reasonText = reasonLabels.length > 0
    ? `Last error: ${reasonLabels.join(", ")}.`
    : "No detailed error was recorded.";

  if (mode === "processed") {
    if (source.status === "failed") {
      const cancelled = source.reason_codes.includes("extraction_cancelled");
      lines.push(cancelled
        ? "Processed text is not available because extraction was cancelled."
        : "Processed text is not available because text extraction failed.");
      lines.push("Resolve the source extraction state before generating processed text.");
      lines.push(reasonText);
      return lines;
    }

    if (source.kind === "pdf" && !sourceHasExtractedText(source)) {
      lines.push("Processed preview is unavailable until PDF text extraction completes.");
      lines.push("Run extraction with the selected PDF handling before reviewing processed text.");
      return lines;
    }

    lines.push("Processed preview is unavailable because source text cannot be read.");
    if (reasonDescriptions.length > 0 && sourceShouldShowReviewNotice(source)) {
      lines.push("Review recommended:");
      for (const description of reasonDescriptions) {
        lines.push(`- ${description}`);
      }
    }
    return lines;
  }

  if (source.status === "failed") {
    const cancelled = source.reason_codes.includes("extraction_cancelled");
    if (cancelled) {
      lines.push("Extraction cancelled.");
    } else if (source.kind === "pdf") {
      lines.push("Text extraction failed.");
    } else {
      lines.push("Source text is unavailable.");
    }
    lines.push(source.kind === "pdf"
      ? "You can retry extraction, change PDF handling, or remove this source from the project."
      : "You can remove this source from the project inventory.");
    lines.push(reasonText);
    return lines;
  }

  if (reasonDescriptions.length > 0 && sourceShouldShowReviewNotice(source)) {
    lines.push(source.status === "processed_with_warnings"
      ? "Processed text has warnings."
      : "Text extracted with warnings.");
    lines.push("Review recommended:");
    for (const description of reasonDescriptions) {
      lines.push(`- ${description}`);
    }
    return lines;
  }

  if (source.kind === "pdf" && !sourceHasExtractedText(source)) {
    lines.push("Text has not been extracted yet.");
    lines.push("Use Extract/OCR selected PDFs to materialise source text before preview or search.");
    return lines;
  }

  lines.push("Source text is unavailable for preview.");
  lines.push(reasonText);
  return lines;
}

function renderTextExtractionRequiredCards(
  container: HTMLDivElement,
  sources: { name: string; status: string; statusLabel: string }[],
  mode: "original" | "processed",
): void {
  container.replaceChildren();
  for (const source of sources) {
    const card = document.createElement("div");
    card.className = "file-card";

    const header = document.createElement("div");
    header.className = "file-card-header";

    const titleArea = document.createElement("div");
    titleArea.className = "file-card-title-area";

    const title = document.createElement("div");
    title.className = "file-card-title";
    title.textContent = source.name;

    const status = document.createElement("div");
    status.className = "file-card-path";
    status.textContent = source.statusLabel;

    titleArea.append(title, status);
    header.appendChild(titleArea);

    const body = document.createElement("div");
    body.className = "file-card-body";
    body.textContent = mode === "processed"
      ? source.status === "failed"
        ? "Processed text is not available because source review or text extraction failed. Review the source status before processing."
        : "Processed preview is unavailable until PDF text extraction completes."
      : source.status === "failed"
      ? "Source text is unavailable because source review or text extraction failed. Review the source status before preview or search."
      : "Text has not been extracted yet. Use Extract/OCR selected PDFs to materialise source text before preview or search.";

    card.append(header, body);
    container.appendChild(card);
  }
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

  const maxChars = indices.length === 1 ? null : 5_000;

  try {
    const [original, processed] = await Promise.all([
      invoke<CombinedPreview>("preview_files_command", {
        indices: chunkIndices,
        corpusVersion: myVersion,
        maxCharsPerFile: maxChars,
        includePaths: true,
        maxFiles: chunkIndices.length,
        cleaningConfig: effectiveCleaningConfig()
      }),
      invoke<CombinedPreview>("preview_processed_files_command", {
        indices: chunkIndices,
        corpusVersion: myVersion,
        maxCharsPerFile: maxChars,
        includePaths: true,
        maxFiles: chunkIndices.length,
        cleaningConfig: effectiveCleaningConfig()
      })
    ]);

    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;

    renderPreviewCards(dom.previewContent, original.files, append, offset, indices.length, "original");
    renderPreviewCards(dom.processedPreviewContent, processed.files, append, offset, indices.length, "processed");
    renderVisibleItems();
    if (!append) {
      renderFullOcrPreviewLauncher(indices, myVersion, myPreviewGeneration);
    }

    highlightRenderedCards();

    dom.statusBar.textContent = `Preview ready for ${indices.length} selected files. (Loaded ${offset + chunkIndices.length})`;
    state.currentPreviewOffset += chunkIndices.length;
  } catch (error) {
    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;
    if (!append) {
      dom.previewContent.textContent = `Error loading preview: ${error}`;
      dom.processedPreviewContent.textContent = `Error loading preview: ${error}`;
    }
    dom.statusBar.textContent = "Error loading preview.";
  } finally {
    dom.previewLoadingOverlay.classList.add("hidden");
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
  const source = sourceForPath(record.source_path);
  const sourceForcesHighQualityOcr =
    source?.extraction_route === "force_ocr" && source.ocr_quality === "HighQuality";
  const fallbackForcesHighQualityOcr =
    state.activeCleaningConfig.pdf_text_source === "ForceOcr" &&
    state.activeCleaningConfig.pdf_ocr_quality === "HighQuality";
  if (!sourceForcesHighQualityOcr && !fallbackForcesHighQualityOcr) return null;
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
        cleaningConfig: effectiveCleaningConfig(),
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
  totalFiles: number,
  mode: "original" | "processed",
  loadFullAttempted = false,
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

    const source = sourceForPath(file.source_path);
    if (source) {
      const kindBadge = document.createElement("span");
      kindBadge.className = "badge";
      kindBadge.textContent = sourceKindLabel(source);
      badges.appendChild(kindBadge);

      const statusBadge = document.createElement("span");
      statusBadge.className = "badge";
      statusBadge.textContent = sourceDisplayStatusLabel(source);
      badges.appendChild(statusBadge);

  // Only add reason badge for concrete failures/errors, not generic warnings
  if (source.status === "failed") {
    const reason = primaryReasonLabel(source);
    if (reason) {
      const reasonBadge = document.createElement("span");
      reasonBadge.className = "badge";
      reasonBadge.textContent = reason;
      badges.appendChild(reasonBadge);
    }
  } else {
    const concreteCodes = source.reason_codes.filter(code =>
      code === "extraction_cancelled" ||
      code === "text_extraction_failed" ||
      code === "render_size_clamped" ||
      code === "many_repeated_artifacts" ||
      code === "ocr_generated" ||
      code === "ocr_review_recommended" ||
      code === "layout_heavy_profile" ||
      code === "layout_warning" ||
      code === "low_text_density"
    );
    if (concreteCodes.length > 0) {
      const reasonBadge = document.createElement("span");
      reasonBadge.className = "badge";
      reasonBadge.textContent = reasonLabel(concreteCodes[0]);
      badges.appendChild(reasonBadge);
    }
  }
    }

    if (file.truncated) {
      const truncBadge = document.createElement("span");
      truncBadge.className = "badge";
      truncBadge.textContent = "Preview capped";
      badges.appendChild(truncBadge);
    }

    header.appendChild(titleArea);
    if (badges.children.length > 0) {
      header.appendChild(badges);
    }

    const body = document.createElement("div");
    body.className = "file-card-body";
    body.classList.add("file-card-body--pending");
    body.dataset.originalText = file.text;

    previewObserver!.observe(body);

    card.appendChild(header);
    if (source) {
      appendSourcePreviewNotices(card, source, mode);
    }
    card.appendChild(body);
    if (file.truncated) {
      card.appendChild(createPreviewCapNotice(mode, totalFiles === 1 && !loadFullAttempted));
    }
    container.appendChild(card);
  });

  if (offset + files.length < totalFiles) {
    const sentinel = document.createElement("div");
    sentinel.className = "chunk-sentinel";
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

function appendSourcePreviewNotices(
  card: HTMLElement,
  source: ProjectSource,
  mode: "original" | "processed",
): void {
  if (mode === "processed") {
    card.appendChild(createSourcePreviewNotice(
      "source-preview-notice-info",
      "Processed preview generated with current Cleaning Settings.",
      ["This is an on-demand preview; source files are not modified."],
    ));
  }

  const descriptions = sourceReasonDescriptions(source);
  if (descriptions.length === 0 || !sourceShouldShowReviewNotice(source)) {
    return;
  }

  card.appendChild(createSourcePreviewNotice(
    "source-preview-notice-warning",
    source.status === "processed_with_warnings"
      ? "Processed text has warnings"
      : "Text extracted with warnings",
    descriptions,
    "Review recommended:",
  ));
}

function createSourcePreviewNotice(
  className: string,
  titleText: string,
  lines: string[],
  listLabel?: string,
): HTMLElement {
  const notice = document.createElement("div");
  notice.className = `source-preview-notice ${className}`;

  const title = document.createElement("strong");
  title.textContent = titleText;
  notice.appendChild(title);

  if (listLabel) {
    const label = document.createElement("span");
    label.textContent = listLabel;
    notice.appendChild(label);

    const list = document.createElement("ul");
    for (const line of lines) {
      const item = document.createElement("li");
      item.textContent = line;
      list.appendChild(item);
    }
    notice.appendChild(list);
    return notice;
  }

  for (const line of lines) {
    const paragraph = document.createElement("span");
    paragraph.textContent = line;
    notice.appendChild(paragraph);
  }

  return notice;
}

function sourceShouldShowReviewNotice(source: ProjectSource): boolean {
  // Only show a review notice when there is a concrete, actionable reason.
  // Do NOT show a generic review banner just because the status says
  // "text_extracted_with_warnings" or "processed_with_warnings".
  return source.reason_codes.some((code) =>
    code === "ocr_generated" ||
    code === "ocr_review_recommended" ||
    code === "layout_heavy_profile" ||
    code === "layout_warning" ||
    code === "low_text_density" ||
    code === "render_size_clamped" ||
    code === "many_repeated_artifacts"
  );
}

function createPreviewCapNotice(mode: "original" | "processed", canLoadFull: boolean): HTMLElement {
  const notice = document.createElement("div");
  notice.className = "preview-cap-note";

  const message = document.createElement("span");
  message.textContent = "Preview capped for performance.";
  notice.appendChild(message);

  if (canLoadFull) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "secondary-btn";
    button.textContent = "Load full text";
    button.addEventListener("click", () => {
      loadFullSingleSourcePreview(mode, button);
    });
    notice.appendChild(button);
  }

  return notice;
}

async function loadFullSingleSourcePreview(
  mode: "original" | "processed",
  button: HTMLButtonElement,
): Promise<void> {
  const indices = Array.from(state.selectedCorpusIndices);
  if (indices.length !== 1) return;

  const myVersion = state.currentCorpusVersion;
  const myPreviewGeneration = state.activePreviewGeneration;
  const container = mode === "processed" ? dom.processedPreviewContent : dom.previewContent;
  button.disabled = true;
  button.textContent = "Loading...";

  try {
    const command = mode === "processed" ? "preview_processed_files_command" : "preview_files_command";
    const result = await invoke<CombinedPreview>(command, {
      indices,
      corpusVersion: myVersion,
      maxCharsPerFile: null,
      includePaths: true,
      maxFiles: 1,
      cleaningConfig: effectiveCleaningConfig(),
    });

    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;

    renderPreviewCards(container, result.files, false, 0, 1, mode, true);
    highlightRenderedCards();
    dom.statusBar.textContent = result.files[0]?.truncated
      ? "Loaded a larger capped preview."
      : "Loaded full text preview.";
  } catch (error) {
    if (myVersion !== state.currentCorpusVersion || myPreviewGeneration !== state.activePreviewGeneration) return;
    button.disabled = false;
    button.textContent = "Load full text";
    dom.statusBar.textContent = `Error loading full preview: ${error}`;
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
