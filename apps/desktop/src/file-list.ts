import { dom, getFileListContainer } from "./dom";
import { state } from "./state";

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
    if (isChecked) {
      state.visibleFiles.forEach(vf => state.selectedCorpusIndices.add(vf.corpusIndex));
    } else {
      state.selectedCorpusIndices.clear();
    }
    callbacks.invalidatePreviewSession();
    renderVisibleItems();
    dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;

    callbacks.schedulePreviewUpdate(150);
  });

  dom.previewCapInput.addEventListener("input", (e) => {
    const valStr = (e.target as HTMLInputElement).value;
    if (!valStr) return; // ignore empty
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
        const limit = Math.min(val, state.visibleFiles.length);
        for (let i = 0; i < limit; i++) {
          state.selectedCorpusIndices.add(state.visibleFiles[i].corpusIndex);
        }
        callbacks.invalidatePreviewSession();
        renderVisibleItems();
        dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;
        callbacks.updatePreview();
      }, 500);
    }
  });
}

export function updateFileList(): void {
  dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;

  const container = getFileListContainer();
  const sentinel = container?.querySelector(".virtual-scroll-sentinel") as HTMLElement;
  if (sentinel) {
    sentinel.style.height = `${state.visibleFiles.length * ITEM_HEIGHT}px`;
  }

  renderVisibleItems();
  callbacks.updatePreview();
}

export function renderVisibleItems(): void {
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

function handleSearch(): void {
  const query = dom.searchInput.value.toLowerCase();
  state.visibleFiles = state.allFiles
    .map((record, corpusIndex) => ({ corpusIndex, record }))
    .filter(({ record }) => !query || record.relative_path.toLowerCase().includes(query));
  state.selectedCorpusIndices.clear();
  callbacks.invalidatePreviewSession();

  const container = getFileListContainer();
  if (container) {
    container.scrollTop = 0;
    state.vsScrollTop = 0;
  }

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
      if (state.selectedCorpusIndices.has(corpusIndex)) {
        state.selectedCorpusIndices.delete(corpusIndex);
      } else {
        state.selectedCorpusIndices.add(corpusIndex);
      }
    }
  } else {
    if (event.ctrlKey || event.metaKey) {
      if (state.selectedCorpusIndices.has(corpusIndex)) {
        state.selectedCorpusIndices.delete(corpusIndex);
      } else {
        state.selectedCorpusIndices.add(corpusIndex);
      }
    } else if (event.shiftKey && state.lastSelectedCorpusIndex !== null) {
      const lastVis = state.visibleFiles.findIndex(vf => vf.corpusIndex === state.lastSelectedCorpusIndex);
      state.selectedCorpusIndices.clear();
      if (lastVis !== -1) {
        const startVis = Math.min(lastVis, visibleIndex);
        const endVis = Math.max(lastVis, visibleIndex);
        for (let vi = startVis; vi <= endVis; vi++) {
          state.selectedCorpusIndices.add(state.visibleFiles[vi].corpusIndex);
        }
      }
    } else {
      state.selectedCorpusIndices.clear();
      state.selectedCorpusIndices.add(corpusIndex);
    }
  }

  state.lastSelectedCorpusIndex = corpusIndex;

  callbacks.invalidatePreviewSession();
  renderVisibleItems();

  dom.filesStatus.textContent = `${state.allFiles.length} loaded | ${state.selectedCorpusIndices.size} selected`;

  callbacks.schedulePreviewUpdate(150);
}
