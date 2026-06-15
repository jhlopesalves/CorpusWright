import { dom, getFileListContainer } from "./dom";
import type { ProjectSource } from "./generated/ProjectSource.js";
import { state } from "./state";
import {
  SOURCE_GROUP_ORDER,
  isLoadableSource,
  pathKey,
  pdfHandlingLabel,
  primaryReasonLabel,
  reasonLabel,
  sourceKindLabel,
  sourceStatusLabel,
} from "./sources";

const ITEM_HEIGHT = 26; // px, must match CSS #file-list li height
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
    const selectableIndices = sourceExplorerMode()
      ? visibleSourceCorpusIndices()
      : state.visibleFiles.map(vf => vf.corpusIndex);

    if (isChecked) {
      selectableIndices.forEach(index => state.selectedCorpusIndices.add(index));
    } else {
      state.selectedCorpusIndices.clear();
    }
    callbacks.invalidatePreviewSession();
    renderVisibleItems();
    updateStatusLine();

    callbacks.schedulePreviewUpdate(150);
  });

  dom.previewCapInput.addEventListener("input", (e) => {
    const valStr = (e.target as HTMLInputElement).value;
    if (!valStr) return;
    const val = parseInt(valStr, 10);

    if (val > 500) {
      dom.previewCapWarning.style.display = "block";
    } else {
      dom.previewCapWarning.style.display = "none";
    }

    if (!isNaN(val) && val > 0) {
      if (state.debounceTimer) clearTimeout(state.debounceTimer);
      state.debounceTimer = window.setTimeout(() => {
        state.selectedCorpusIndices.clear();
        const selectableIndices = sourceExplorerMode()
          ? visibleSourceCorpusIndices()
          : state.visibleFiles.map(vf => vf.corpusIndex);
        const limit = Math.min(val, selectableIndices.length);
        for (let i = 0; i < limit; i++) {
          state.selectedCorpusIndices.add(selectableIndices[i]);
        }
        callbacks.invalidatePreviewSession();
        renderVisibleItems();
        updateStatusLine();
        callbacks.updatePreview();
      }, 500);
    }
  });
}

export function updateFileList(updatePreviewAfterRender = true): void {
  updateStatusLine();
  syncContainerMode();

  const container = getFileListContainer();
  const sentinel = container?.querySelector(".virtual-scroll-sentinel") as HTMLElement;
  if (sentinel) {
    sentinel.style.height = sourceExplorerMode()
      ? "0px"
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
  dom.fileList.style.transform = "";
  dom.fileList.innerHTML = "";

  const fragment = document.createDocumentFragment();
  const sources = filteredProjectSources();
  const corpusIndexByPath = buildCorpusIndexByPath();

  if (state.projectSources.length === 0) {
    fragment.appendChild(createSourceEmptyState());
    dom.fileList.appendChild(fragment);
    return;
  }

  if (sources.length === 0) {
    const empty = document.createElement("li");
    empty.className = "source-empty-state source-empty-filtered";
    empty.textContent = "No sources match this search.";
    fragment.appendChild(empty);
    dom.fileList.appendChild(fragment);
    return;
  }

  for (const status of SOURCE_GROUP_ORDER) {
    const groupSources = sources.filter((source) => source.status === status);
    if (groupSources.length === 0) {
      continue;
    }

    const header = document.createElement("li");
    header.className = "source-group-header";
    header.textContent = `${sourceStatusLabel(status)} (${groupSources.length})`;
    fragment.appendChild(header);

    for (const source of groupSources) {
      const corpusIndex = corpusIndexByPath.get(pathKey(source.path)) ?? null;
      fragment.appendChild(createSourceRow(source, corpusIndex, sources));
    }
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
  corpusIndex: number | null,
  visibleSources: ProjectSource[],
): HTMLLIElement {
  const li = document.createElement("li");
  li.className = `source-row source-status-${source.status.replace(/_/g, "-")}`;
  li.title = source.path;

  if (corpusIndex !== null && state.selectedCorpusIndices.has(corpusIndex)) {
    li.classList.add("selected");
  }

  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.className = "file-checkbox";
  checkbox.disabled = corpusIndex === null || !isLoadableSource(source);
  checkbox.checked = corpusIndex !== null && state.selectedCorpusIndices.has(corpusIndex);

  const textWrap = document.createElement("span");
  textWrap.className = "source-row-text";

  const name = document.createElement("span");
  name.className = "file-name source-file-name";
  name.textContent = source.file_name;

  const meta = document.createElement("span");
  meta.className = "source-row-meta";

  const kindBadge = document.createElement("span");
  kindBadge.className = "source-badge source-kind-badge";
  kindBadge.textContent = sourceKindLabel(source);
  meta.appendChild(kindBadge);

  const reason = primaryReasonLabel(source);
  if (reason) {
    const reasonBadge = document.createElement("span");
    reasonBadge.className = "source-badge source-reason-badge";
    reasonBadge.textContent = reason;
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
    handleSourceSelect(event, source, visibleSources, true);
  });

  li.addEventListener("click", (event) => {
    handleSourceSelect(event, source, visibleSources, false);
  });

  return li;
}

function handleSearch(): void {
  const query = dom.searchInput.value.toLowerCase();
  if (sourceExplorerMode()) {
    state.sourceSearchQuery = query;
    state.selectedCorpusIndices.clear();
    callbacks.invalidatePreviewSession();
    resetScroll();
    updateFileList();
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
  isCheckboxClick: boolean,
): void {
  const corpusIndexByPath = buildCorpusIndexByPath();
  const corpusIndex = corpusIndexByPath.get(pathKey(source.path));
  if (corpusIndex === undefined || !isLoadableSource(source)) {
    return;
  }

  const visibleIndices = visibleSources
    .map((visibleSource) => corpusIndexByPath.get(pathKey(visibleSource.path)))
    .filter((index): index is number => index !== undefined);

  if (isCheckboxClick) {
    if (event.shiftKey && state.lastSelectedCorpusIndex !== null) {
      addVisibleRange(visibleIndices, corpusIndex);
    } else {
      toggleCorpusIndex(corpusIndex);
    }
  } else {
    applyRowSelection(event, corpusIndex, visibleIndices);
  }

  state.lastSelectedCorpusIndex = corpusIndex;
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

function toggleCorpusIndex(corpusIndex: number): void {
  if (state.selectedCorpusIndices.has(corpusIndex)) {
    state.selectedCorpusIndices.delete(corpusIndex);
  } else {
    state.selectedCorpusIndices.add(corpusIndex);
  }
}

function filteredProjectSources(): ProjectSource[] {
  const query = state.sourceSearchQuery.trim().toLowerCase();
  return state.projectSources.filter((source) => {
    if (!query) return true;
    const haystack = [
      source.file_name,
      source.path,
      source.kind,
      source.extraction_route,
      source.pdf_profile ?? "",
      sourceStatusLabel(source.status),
      ...source.reason_codes,
      ...source.reason_codes.map(reasonLabel),
    ].join(" ").toLowerCase();
    return haystack.includes(query);
  });
}

function visibleSourceCorpusIndices(): number[] {
  const corpusIndexByPath = buildCorpusIndexByPath();
  return filteredProjectSources()
    .filter(isLoadableSource)
    .map((source) => corpusIndexByPath.get(pathKey(source.path)))
    .filter((index): index is number => index !== undefined);
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

function updateStatusLine(): void {
  if (sourceExplorerMode()) {
    const sourceCount = state.projectSources.length;
    const noun = sourceCount === 1 ? "source" : "sources";
    dom.filesStatus.textContent = `${sourceCount} ${noun} | ${state.selectedCorpusIndices.size} selected`;
    dom.selectAllCheckbox.checked =
      visibleSourceCorpusIndices().length > 0 &&
      visibleSourceCorpusIndices().every(index => state.selectedCorpusIndices.has(index));

    if (state.selectedCorpusIndices.size > 0) {
      const selectedRecords = Array.from(state.selectedCorpusIndices)
        .map(index => state.allFiles[index])
        .filter(record => record !== undefined);
      const allSelectedArePdfs =
        selectedRecords.length === state.selectedCorpusIndices.size &&
        selectedRecords.every(record => record.document_type === "pdf");
      dom.sourcePreparePdfsBtn.classList.toggle("hidden", !allSelectedArePdfs);
      dom.sourcePreparePdfsBtn.disabled = !allSelectedArePdfs;
    } else {
      dom.sourcePreparePdfsBtn.classList.add("hidden");
      dom.sourcePreparePdfsBtn.disabled = true;
    }

    return;
  }

  dom.sourcePreparePdfsBtn.classList.add("hidden");
  dom.sourcePreparePdfsBtn.disabled = true;
  dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;
  dom.selectAllCheckbox.checked =
    state.visibleFiles.length > 0 &&
    state.visibleFiles.every(vf => state.selectedCorpusIndices.has(vf.corpusIndex));
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
