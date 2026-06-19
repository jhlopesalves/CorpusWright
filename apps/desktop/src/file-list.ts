import { dom, getFileListContainer } from "./dom";
import type { ProjectSource } from "./generated/ProjectSource.js";
import { state } from "./state";
import {
  clearProjectSourceSelection,
  forgetSelectedProjectSource,
  hasProjectSourceSelection,
  isPreviewableSource,
  isSelectableSource,
  isProjectSourceSelected,
  loadProjectSourcesForActiveProject,
  pathKey,
  pdfHandlingLabel,
  primaryReasonLabel,
  projectSourceSelectionCount,
  queryProjectSourceSelectionActionSummary,
  rememberSelectedProjectSource,
  selectAllMatchingProjectSources,
  selectedProjectSources,
  sourceHasExtractedText,
  sourceDisplayStatusLabel,
  sourceKindLabel,
  sourceReasonDescriptions,
  type ProjectSourceSelectionActionSummary,
} from "./sources";

const ITEM_HEIGHT = 26; // px, must match CSS #file-list li height
const SOURCE_ITEM_HEIGHT = 76; // px, must match CSS source row height
const OVERSCAN = 10; // extra items rendered above/below viewport

interface FileListCallbacks {
  updatePreview: () => void;
  schedulePreviewUpdate: (delay: number) => void;
  invalidatePreviewSession: () => void;
}

let callbacks: FileListCallbacks = {
  updatePreview: () => {},
  schedulePreviewUpdate: () => {},
  invalidatePreviewSession: () => {},
};

let projectSourceActionGeneration = 0;

export function configureFileListCallbacks(nextCallbacks: FileListCallbacks): void {
  callbacks = nextCallbacks;
}

export function initVirtualScroll(): void {
  const container = getFileListContainer();
  if (!container) return;

  let sentinel = container.querySelector(".virtual-scroll-sentinel") as HTMLElement;
  if (!sentinel) {
    sentinel = document.createElement("div");
    sentinel.className = "virtual-scroll-sentinel";
    container.insertBefore(sentinel, dom.fileList);
  }

  container.addEventListener("scroll", () => {
    state.vsScrollTop = container.scrollTop;
    renderVisibleItems();
  });

  const ro = new ResizeObserver((entries) => {
    for (const entry of entries) {
      state.vsContainerHeight = entry.contentRect.height;
    }
    renderVisibleItems();
  });
  ro.observe(container);
  state.vsContainerHeight = container.clientHeight;
}

export function initFileFilter(): void {
  dom.searchInput.addEventListener("input", handleSearch);
}

export function initSelectionControls(): void {
  dom.selectAllCheckbox.addEventListener("change", (e) => {
    const isChecked = (e.target as HTMLInputElement).checked;
    if (sourceExplorerMode()) {
      if (isChecked) {
        selectAllMatchingProjectSources();
      } else {
        clearProjectSourceSelection();
      }
      state.focusedProjectSourceId = null;
      state.focusedProjectSourceOffset = null;
      state.projectSourcePreviewScrollTop = 0;
      syncCorpusSelectionFromProjectSources();
    } else {
      const selectableIndices = state.visibleFiles.map(vf => vf.corpusIndex);
      if (isChecked) {
        selectableIndices.forEach(index => state.selectedCorpusIndices.add(index));
      } else {
        state.selectedCorpusIndices.clear();
      }
    }
    callbacks.invalidatePreviewSession();
    renderVisibleItems();
    updateStatusLine();

    callbacks.schedulePreviewUpdate(150);
  });

}

export function updateFileList(updatePreviewAfterRender = true): void {
  updateStatusLine();
  syncContainerMode();

  const container = getFileListContainer();
  const sentinel = container?.querySelector(".virtual-scroll-sentinel") as HTMLElement;
  if (sentinel) {
    sentinel.style.height = sourceExplorerMode()
      ? `${(state.projectSourceQuerySummary?.total_count ?? 0) * SOURCE_ITEM_HEIGHT}px`
      : `${state.visibleFiles.length * ITEM_HEIGHT}px`;
  }

  renderVisibleItems();
  if (updatePreviewAfterRender) {
    callbacks.updatePreview();
  }
}

export function renderVisibleItems(): void {
  if (sourceExplorerMode()) {
    renderSourceExplorerItems();
    return;
  }

  const totalItems = state.visibleFiles.length;

  const startIdx = Math.max(0, Math.floor(state.vsScrollTop / ITEM_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(state.vsContainerHeight / ITEM_HEIGHT) + 2 * OVERSCAN;
  const endIdx = Math.min(totalItems, startIdx + visibleCount);

  dom.fileList.style.transform = `translateY(${startIdx * ITEM_HEIGHT}px)`;
  dom.fileList.innerHTML = "";

  const fragment = document.createDocumentFragment();
  for (let i = startIdx; i < endIdx; i++) {
    const { corpusIndex, record } = state.visibleFiles[i];
    const li = document.createElement("li");
    const parts = record.relative_path.split(/[/\\]/);

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.className = "file-checkbox";
    checkbox.checked = state.selectedCorpusIndices.has(corpusIndex);

    const span = document.createElement("span");
    span.className = "file-name";
    span.textContent = parts[parts.length - 1];

    li.appendChild(checkbox);
    li.appendChild(span);
    li.title = record.relative_path;

    if (state.selectedCorpusIndices.has(corpusIndex)) {
      li.classList.add("selected");
    }

    checkbox.addEventListener("click", (e) => {
      e.stopPropagation();
      handleFileSelect(e, i, true);
    });

    li.addEventListener("click", (e) => {
      handleFileSelect(e, i, false);
    });

    fragment.appendChild(li);
  }

  dom.fileList.appendChild(fragment);
}

function renderSourceExplorerItems(): void {
  const totalItems = state.projectSourceQuerySummary?.total_count ?? 0;
  const startIdx = Math.max(0, Math.floor(state.vsScrollTop / SOURCE_ITEM_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(state.vsContainerHeight / SOURCE_ITEM_HEIGHT) + 2 * OVERSCAN;
  const pageEnd = state.projectSourcePageOffset + state.projectSources.length;
  const desiredPageOffset = startIdx < state.projectSourcePageOffset
    || startIdx + visibleCount > pageEnd
    ? Math.max(0, startIdx - OVERSCAN)
    : state.projectSourcePageOffset;
  if (
    desiredPageOffset !== state.projectSourcePageOffset
    && !state.projectSourceQueryPending
    && desiredPageOffset < totalItems
  ) {
    void loadProjectSourcesForActiveProject(desiredPageOffset).then(() => {
      updateStatusLine();
      renderVisibleItems();
    });
  }

  const pageStart = state.projectSourcePageOffset;
  const localStart = Math.max(0, startIdx - pageStart);
  const localEnd = Math.min(
    state.projectSources.length,
    localStart + visibleCount,
  );
  dom.fileList.style.transform = `translateY(${(pageStart + localStart) * SOURCE_ITEM_HEIGHT}px)`;
  dom.fileList.innerHTML = "";

  const fragment = document.createDocumentFragment();

  if (totalItems === 0 && !state.sourceSearchQuery) {
    fragment.appendChild(createSourceEmptyState());
    dom.fileList.appendChild(fragment);
    return;
  }

  if (totalItems === 0) {
    const empty = document.createElement("li");
    empty.className = "source-empty-state source-empty-filtered";
    empty.textContent = "No sources match this search.";
    fragment.appendChild(empty);
    dom.fileList.appendChild(fragment);
    return;
  }

  const visibleSources = state.projectSources.slice(localStart, localEnd);
  for (const [index, source] of visibleSources.entries()) {
    fragment.appendChild(createSourceRow(
      source,
      state.projectSources,
      pageStart + localStart + index,
    ));
  }

  dom.fileList.appendChild(fragment);
}

function createSourceEmptyState(): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "source-empty-state";

  const title = document.createElement("strong");
  title.textContent = "This corpus project has no source files yet.";

  const body = document.createElement("span");
  body.textContent = "Add PDFs, DOCX, TXT, HTML, or plain text files to begin preparing your corpus.";

  const actions = document.createElement("div");
  actions.className = "source-empty-actions";

  const addFiles = document.createElement("button");
  addFiles.type = "button";
  addFiles.className = "secondary-btn";
  addFiles.textContent = "Add Files";
  addFiles.addEventListener("click", () => dom.sourceAddFilesBtn.click());

  const addFolder = document.createElement("button");
  addFolder.type = "button";
  addFolder.className = "secondary-btn";
  addFolder.textContent = "Add Folder";
  addFolder.addEventListener("click", () => dom.sourceAddFolderBtn.click());

  actions.append(addFiles, addFolder);
  item.append(title, body, actions);
  return item;
}

function createSourceRow(
  source: ProjectSource,
  visibleSources: ProjectSource[],
  sourceOffset: number,
): HTMLLIElement {
  const li = document.createElement("li");
  li.className = `source-row source-status-${source.status.replace(/_/g, "-")}`;
  li.title = source.path;

  if (isProjectSourceSelected(source.id)) {
    li.classList.add("selected");
  }
  if (state.focusedProjectSourceId === source.id) {
    li.classList.add("focused");
  }

  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.className = "file-checkbox";
  checkbox.disabled = !isSelectableSource(source);
  checkbox.checked = isProjectSourceSelected(source.id);

  const textWrap = document.createElement("span");
  textWrap.className = "source-row-text";

  const name = document.createElement("span");
  name.className = "file-name source-file-name";
  name.textContent = source.file_name;
  name.title = source.path;

  const meta = document.createElement("span");
  meta.className = "source-row-meta";

  const kindBadge = document.createElement("span");
  kindBadge.className = "source-badge source-kind-badge";
  kindBadge.textContent = sourceKindLabel(source);
  meta.appendChild(kindBadge);

  const statusBadge = document.createElement("span");
  statusBadge.className = "source-badge source-status-badge";
  statusBadge.textContent = sourceDisplayStatusLabel(source);
  meta.appendChild(statusBadge);

  const reason = primaryReasonLabel(source);
  if (reason) {
    const reasonBadge = document.createElement("span");
    reasonBadge.className = "source-badge source-reason-badge";
    reasonBadge.textContent = reason;
    const descriptions = sourceReasonDescriptions(source);
    if (descriptions.length > 0) {
      reasonBadge.title = descriptions.join("\n");
    }
    meta.appendChild(reasonBadge);
  }

  const handling = pdfHandlingLabel(source);
  if (handling) {
    const handlingBadge = document.createElement("span");
    handlingBadge.className = "source-badge source-route-badge";
    handlingBadge.textContent = handling;
    meta.appendChild(handlingBadge);
  }

  textWrap.append(name, meta);
  li.append(checkbox, textWrap);

  checkbox.addEventListener("click", (event) => {
    event.stopPropagation();
    handleSourceSelect(event, source, visibleSources, sourceOffset, true);
  });

  li.addEventListener("click", (event) => {
    handleSourceSelect(event, source, visibleSources, sourceOffset, false);
  });

  return li;
}

function handleSearch(): void {
  const query = dom.searchInput.value.toLowerCase();
  if (sourceExplorerMode()) {
    state.sourceSearchQuery = query;
    state.selectedCorpusIndices.clear();
    clearProjectSourceSelection();
    state.lastSelectedProjectSourceId = null;
    state.focusedProjectSourceId = null;
    state.focusedProjectSourceOffset = null;
    state.projectSourcePreviewScrollTop = 0;
    callbacks.invalidatePreviewSession();
    resetScroll();
    updateFileList(false);
    if (state.debounceTimer) clearTimeout(state.debounceTimer);
    state.debounceTimer = window.setTimeout(() => {
      void loadProjectSourcesForActiveProject(0).then(() => {
        updateFileList();
      });
    }, 150);
    return;
  }

  state.visibleFiles = state.allFiles
    .map((record, corpusIndex) => ({ corpusIndex, record }))
    .filter(({ record }) => !query || record.relative_path.toLowerCase().includes(query));
  state.selectedCorpusIndices.clear();
  callbacks.invalidatePreviewSession();
  resetScroll();

  updateFileList();
}

function handleFileSelect(event: MouseEvent, visibleIndex: number, isCheckboxClick: boolean = false): void {
  const corpusIndex = state.visibleFiles[visibleIndex].corpusIndex;

  if (isCheckboxClick) {
    if (event.shiftKey && state.lastSelectedCorpusIndex !== null) {
      const lastVis = state.visibleFiles.findIndex(vf => vf.corpusIndex === state.lastSelectedCorpusIndex);
      if (lastVis !== -1) {
        const startVis = Math.min(lastVis, visibleIndex);
        const endVis = Math.max(lastVis, visibleIndex);
        for (let vi = startVis; vi <= endVis; vi++) {
          state.selectedCorpusIndices.add(state.visibleFiles[vi].corpusIndex);
        }
      }
    } else {
      toggleCorpusIndex(corpusIndex);
    }
  } else {
    applyRowSelection(event, corpusIndex, state.visibleFiles.map(vf => vf.corpusIndex));
  }

  state.lastSelectedCorpusIndex = corpusIndex;
  callbacks.invalidatePreviewSession();
  renderVisibleItems();
  updateStatusLine();
  callbacks.schedulePreviewUpdate(150);
}

function handleSourceSelect(
  event: MouseEvent,
  source: ProjectSource,
  visibleSources: ProjectSource[],
  sourceOffset: number,
  isCheckboxClick: boolean,
): void {
  if (!isSelectableSource(source)) {
    return;
  }

  if (isCheckboxClick) {
    const visibleIds = visibleSources
      .filter(isSelectableSource)
      .map((visibleSource) => visibleSource.id);
    if (
      event.shiftKey
      && state.lastSelectedProjectSourceId !== null
      && !state.allMatchingProjectSourcesSelected
    ) {
      addVisibleSourceRange(visibleIds, source.id);
    } else {
      toggleProjectSourceId(source.id);
    }
    state.lastSelectedProjectSourceId = source.id;
    syncCorpusSelectionFromProjectSources();
  } else {
    state.focusedProjectSourceId = source.id;
    state.focusedProjectSourceOffset = sourceOffset;
  }

  callbacks.invalidatePreviewSession();
  renderVisibleItems();
  updateStatusLine();
  callbacks.schedulePreviewUpdate(150);
}

function applyRowSelection(event: MouseEvent, corpusIndex: number, visibleIndices: number[]): void {
  if (event.ctrlKey || event.metaKey) {
    toggleCorpusIndex(corpusIndex);
  } else if (event.shiftKey && state.lastSelectedCorpusIndex !== null) {
    state.selectedCorpusIndices.clear();
    addVisibleRange(visibleIndices, corpusIndex);
  } else {
    state.selectedCorpusIndices.clear();
    state.selectedCorpusIndices.add(corpusIndex);
  }
}

function addVisibleRange(visibleIndices: number[], corpusIndex: number): void {
  const lastVisible = visibleIndices.indexOf(state.lastSelectedCorpusIndex ?? -1);
  const currentVisible = visibleIndices.indexOf(corpusIndex);
  if (lastVisible === -1 || currentVisible === -1) {
    state.selectedCorpusIndices.add(corpusIndex);
    return;
  }

  const start = Math.min(lastVisible, currentVisible);
  const end = Math.max(lastVisible, currentVisible);
  for (let index = start; index <= end; index++) {
    state.selectedCorpusIndices.add(visibleIndices[index]);
  }
}

function addVisibleSourceRange(visibleIds: string[], sourceId: string): void {
  const lastVisible = visibleIds.indexOf(state.lastSelectedProjectSourceId ?? "");
  const currentVisible = visibleIds.indexOf(sourceId);
  if (lastVisible === -1 || currentVisible === -1) {
    const source = state.projectSources.find((item) => item.id === sourceId);
    if (source) rememberSelectedProjectSource(source);
    return;
  }

  const start = Math.min(lastVisible, currentVisible);
  const end = Math.max(lastVisible, currentVisible);
  for (let index = start; index <= end; index++) {
    const source = state.projectSources.find((item) => item.id === visibleIds[index]);
    if (source) rememberSelectedProjectSource(source);
  }
}

function toggleProjectSourceId(sourceId: string): void {
  if (state.allMatchingProjectSourcesSelected) {
    if (state.deselectedProjectSourceIds.has(sourceId)) {
      state.deselectedProjectSourceIds.delete(sourceId);
    } else {
      state.deselectedProjectSourceIds.add(sourceId);
    }
    return;
  }
  if (state.selectedProjectSourceIds.has(sourceId)) {
    forgetSelectedProjectSource(sourceId);
  } else {
    const source = state.projectSources.find((item) => item.id === sourceId);
    if (source) rememberSelectedProjectSource(source);
  }
}

function toggleCorpusIndex(corpusIndex: number): void {
  if (state.selectedCorpusIndices.has(corpusIndex)) {
    state.selectedCorpusIndices.delete(corpusIndex);
  } else {
    state.selectedCorpusIndices.add(corpusIndex);
  }
}

function syncCorpusSelectionFromProjectSources(corpusIndexByPath = buildCorpusIndexByPath()): void {
  state.selectedCorpusIndices.clear();
  if (state.allMatchingProjectSourcesSelected) return;
  for (const source of selectedProjectSources()) {
    if (!isPreviewableSource(source)) {
      continue;
    }
    const corpusIndex = corpusIndexByPath.get(pathKey(source.path));
    if (corpusIndex !== undefined) {
      state.selectedCorpusIndices.add(corpusIndex);
    }
  }
}

function buildCorpusIndexByPath(): Map<string, number> {
  const result = new Map<string, number>();
  state.allFiles.forEach((record, corpusIndex) => {
    result.set(pathKey(record.source_path), corpusIndex);
  });
  return result;
}

function sourceExplorerMode(): boolean {
  return state.activeProject !== null;
}

function sourceHasUsablePdfRoute(source: ProjectSource): boolean {
  return source.kind === "pdf"
    && source.extraction_route !== "unsupported"
    && !source.reason_codes.includes("missing_file");
}

function explicitSelectionActionSummary(selectedSources: ProjectSource[]): ProjectSourceSelectionActionSummary {
  let pdfCount = 0;
  let usablePdfCount = 0;
  let extractablePdfCount = 0;
  let reextractablePdfCount = 0;
  let failedPdfCount = 0;

  for (const source of selectedSources) {
    if (source.kind !== "pdf") continue;
    pdfCount += 1;
    if (!sourceHasUsablePdfRoute(source)) continue;
    usablePdfCount += 1;
    if (sourceHasExtractedText(source)) {
      reextractablePdfCount += 1;
    } else if (source.status === "failed") {
      failedPdfCount += 1;
    } else {
      extractablePdfCount += 1;
    }
  }

  return {
    totalSelected: selectedSources.length,
    allSelectedArePdfs: selectedSources.length > 0 && selectedSources.length === pdfCount,
    usablePdfCount,
    extractablePdfCount,
    reextractablePdfCount,
    failedPdfCount,
  };
}

function selectedPdfLabel(count: number): string {
  return count === 1 ? "selected PDF" : `${count.toLocaleString()} selected PDFs`;
}

function hideSourceActionItems(): void {
  dom.sourceActionExtract.classList.add("hidden");
  dom.sourceActionReExtract.classList.add("hidden");
  dom.sourceActionRetry.classList.add("hidden");
  dom.sourceActionChangeHandling.classList.add("hidden");
  dom.sourcePreparePdfsBtn.classList.add("hidden");
  dom.sourcePreparePdfsBtn.disabled = true;
}

function applySourceActionSummary(
  summary: ProjectSourceSelectionActionSummary | null,
  allowChangeHandling: boolean,
  keepLegacyPdfButton: boolean,
): void {
  hideSourceActionItems();
  if (!summary?.allSelectedArePdfs) {
    return;
  }

  const allNeedExtraction =
    summary.usablePdfCount === summary.totalSelected
    && summary.extractablePdfCount === summary.totalSelected;
  const hasRetryableFailure = summary.failedPdfCount > 0;
  const hasReextractablePdf = summary.reextractablePdfCount > 0;

  dom.sourceActionExtract.classList.toggle("hidden", !allNeedExtraction);
  dom.sourceActionReExtract.classList.toggle("hidden", !hasReextractablePdf);
  dom.sourceActionRetry.classList.toggle("hidden", !hasRetryableFailure);
  dom.sourceActionChangeHandling.classList.toggle("hidden", !allowChangeHandling);
  dom.sourceActionReExtract.textContent = `Re-extract ${selectedPdfLabel(summary.totalSelected)}`;

  if (keepLegacyPdfButton) {
    if (hasRetryableFailure) {
      dom.sourcePreparePdfsBtn.textContent = "Retry extraction...";
    } else if (hasReextractablePdf) {
      dom.sourcePreparePdfsBtn.textContent = "Re-extract/OCR selected PDFs...";
    } else {
      dom.sourcePreparePdfsBtn.textContent = "Extract/OCR selected PDFs...";
    }
    dom.sourcePreparePdfsBtn.classList.remove("hidden");
    dom.sourcePreparePdfsBtn.disabled = false;
  }
}

function updateStatusLine(): void {
  if (sourceExplorerMode()) {
    const sourceCount = state.projectSourceQuerySummary?.total_count ?? 0;
    const selectedCount = projectSourceSelectionCount();
    const noun = sourceCount === 1 ? "source" : "sources";
    dom.filesStatus.textContent = `${sourceCount.toLocaleString()} ${noun} | ${selectedCount.toLocaleString()} selected`;
    dom.selectAllCheckbox.checked = state.allMatchingProjectSourcesSelected
      && state.deselectedProjectSourceIds.size === 0
      && sourceCount > 0;
    dom.selectAllCheckbox.indeterminate = selectedCount > 0 && !dom.selectAllCheckbox.checked;

    if (hasProjectSourceSelection()) {
      dom.sourceSelectedTrigger.disabled = false;
      if (state.allMatchingProjectSourcesSelected) {
        const generation = ++projectSourceActionGeneration;
        hideSourceActionItems();
        void queryProjectSourceSelectionActionSummary()
          .then((summary) => {
            if (generation !== projectSourceActionGeneration) return;
            applySourceActionSummary(summary, false, false);
          })
          .catch((error) => {
            if (generation !== projectSourceActionGeneration) return;
            hideSourceActionItems();
            console.error(error);
          });
      } else {
        projectSourceActionGeneration += 1;
        applySourceActionSummary(explicitSelectionActionSummary(selectedProjectSources()), true, true);
      }
      dom.sourceRemoveBtn.classList.remove("hidden");
      dom.sourceRemoveBtn.disabled = false;
    } else {
      projectSourceActionGeneration += 1;
      dom.sourceSelectedTrigger.disabled = true;
      hideSourceActionItems();
      dom.sourceRemoveBtn.classList.add("hidden");
      dom.sourceRemoveBtn.disabled = true;
    }

    return;
  }

  dom.sourceSelectedTrigger.disabled = true;
  projectSourceActionGeneration += 1;
  hideSourceActionItems();
  dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;
  dom.selectAllCheckbox.checked =
    state.visibleFiles.length > 0 &&
    state.visibleFiles.every(vf => state.selectedCorpusIndices.has(vf.corpusIndex));
  dom.selectAllCheckbox.indeterminate =
    state.selectedCorpusIndices.size > 0 && !dom.selectAllCheckbox.checked;
}

function syncContainerMode(): void {
  const container = getFileListContainer();
  if (!container) return;
  container.classList.toggle("source-explorer-mode", sourceExplorerMode());
}

function resetScroll(): void {
  const container = getFileListContainer();
  if (container) {
    container.scrollTop = 0;
    state.vsScrollTop = 0;
  }
}
