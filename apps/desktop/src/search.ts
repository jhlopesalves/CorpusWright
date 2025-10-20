import { invoke } from "@tauri-apps/api/core";

import {
  dom,
  getActivePreviewCardsContainer,
  getActivePreviewTab,
} from "./dom";
import { state } from "./state";
import type { SearchResult } from "./generated/SearchResult.js";

interface SearchCallbacks {
  schedulePreviewUpdate: (delay: number) => void;
}

let callbacks: SearchCallbacks = {
  schedulePreviewUpdate: () => {},
};

export function initSearch(nextCallbacks: SearchCallbacks): void {
  callbacks = nextCallbacks;

  dom.previewSearchInput.addEventListener("input", () => {
    state.currentSearchQuery = dom.previewSearchInput.value;
    state.searchGeneration += 1;
    state.pendingSearchNavigation = 0;
    if (state.searchDebounceTimer) clearTimeout(state.searchDebounceTimer);
    state.searchDebounceTimer = window.setTimeout(() => {
      executeGlobalSearch();
    }, 250);
  });

  dom.previewSearchInput.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    if (!state.currentSearchQuery) return;

    const dir = e.shiftKey ? -1 : 1;

    if (state.searchDebounceTimer) {
      clearTimeout(state.searchDebounceTimer);
      state.searchDebounceTimer = undefined;
    }

    if (state.isSearching) {
      if (state.currentSearchQuery !== state.lastSearchedQuery) {
        state.searchGeneration += 1;
      }
      state.pendingSearchAfterCurrent = true;
      state.pendingSearchNavigation = dir;
      return;
    }

    if (state.currentSearchQuery !== state.lastSearchedQuery) {
      state.searchGeneration += 1;
      state.pendingSearchNavigation = 0;
      executeGlobalSearch();
      return;
    }

    if (state.lastSearchResult && state.lastSearchResult.hits.length > 0) {
      navigateSearch(dir);
    }
  });

  dom.searchPrev.addEventListener("click", () => navigateSearch(-1));
  dom.searchNext.addEventListener("click", () => navigateSearch(1));
}

export function handlePreviewTabChanged(): void {
  state.searchGeneration += 1;
  state.currentMatchIndex = -1;
  state.lastSearchResult = null;
  state.pendingSearchNavigation = 0;
  state.pendingSearchAfterCurrent = false;
  if (state.searchDebounceTimer) {
    clearTimeout(state.searchDebounceTimer);
    state.searchDebounceTimer = undefined;
  }

  executeGlobalSearch();
}

async function executeGlobalSearch(): Promise<void> {
  if (state.isSearching) return;

  if (!state.currentSearchQuery) {
    state.lastSearchedQuery = "";
    state.lastSearchResult = null;
    state.currentMatchIndex = -1;
    state.pendingSearchNavigation = 0;
    state.pendingSearchAfterCurrent = false;
    if (state.selectedCorpusIndices.size > 0) {
      callbacks.schedulePreviewUpdate(50);
    } else {
      const activeTabContent = getActivePreviewCardsContainer();
      if (activeTabContent) {
        activeTabContent.innerHTML = "Select files from the left panel to preview their contents.";
      }
    }
    updateSearchUI();
    return;
  }

  const query = state.currentSearchQuery.trim();
  const mySearchGen = ++state.searchGeneration;
  state.isSearching = true;
  updateSearchUI();

  let resultAccepted = false;
  let myVersion = state.currentCorpusVersion;

  try {
    myVersion = state.currentCorpusVersion;
    const isProcessed = getActivePreviewTab()?.getAttribute("data-target") === "processed-text";
    const indices = Array.from(state.selectedCorpusIndices);

    const result = await invoke<SearchResult>("search_corpus_command", {
      indices,
      corpusVersion: myVersion,
      query: query,
      isProcessed: isProcessed,
      cleaningConfig: state.activeCleaningConfig,
      maxHits: 1000
    });

    if (mySearchGen === state.searchGeneration && myVersion === state.currentCorpusVersion) {
      state.lastSearchResult = result;
      state.lastSearchedQuery = query;
      resultAccepted = true;
    }
  } catch (err) {
    if (mySearchGen === state.searchGeneration) {
      console.error("Search failed", err);
    }
  } finally {
    state.isSearching = false;
  }

  if (state.pendingSearchAfterCurrent) {
    state.pendingSearchAfterCurrent = false;
    if (state.currentSearchQuery !== state.lastSearchedQuery) {
      executeGlobalSearch();
      return;
    }
    if (state.pendingSearchNavigation !== 0 && state.lastSearchResult && state.lastSearchResult.hits.length > 0) {
      const navDir = state.pendingSearchNavigation;
      state.pendingSearchNavigation = 0;
      navigateSearch(navDir);
    }
    return;
  }

  if (!resultAccepted) return;

  if (state.pendingSearchNavigation !== 0 && state.lastSearchResult && state.lastSearchResult.hits.length > 0) {
    const navDir = state.pendingSearchNavigation;
    state.pendingSearchNavigation = 0;
    navigateSearch(navDir);
  }

  if (state.lastSearchResult && state.lastSearchResult.hits.length > 0) {
    state.currentMatchIndex = 0;
    renderSearchHit();
  }
  updateSearchUI();
}

function renderSearchHit(): void {
  if (!state.lastSearchResult || state.currentMatchIndex < 0 || state.currentMatchIndex >= state.lastSearchResult.hits.length) return;

  const hit = state.lastSearchResult.hits[state.currentMatchIndex];
  const isProcessed = getActivePreviewTab()?.getAttribute("data-target") === "processed-text";
  const container = isProcessed ? dom.processedPreviewContent : dom.previewContent;
  if (!container) return;

  container.innerHTML = "";

  const card = document.createElement("div");
  card.className = "file-card";

  const header = document.createElement("div");
  header.className = "file-card-header";

  const titleArea = document.createElement("div");
  titleArea.className = "file-card-title-area";

  const fileNum = document.createElement("div");
  fileNum.className = "file-card-num";
  const { returned_hits, total_matches, truncated } = state.lastSearchResult;
  let countStr = `Hit ${state.currentMatchIndex + 1} of ${returned_hits}`;
  if (truncated) {
    countStr += ` (${total_matches.toLocaleString()} total matches in selected files)`;
  } else if (total_matches > returned_hits) {
    countStr += ` of ${total_matches.toLocaleString()} in selected files`;
  }
  fileNum.textContent = countStr;

  const filename = document.createElement("div");
  filename.className = "file-card-title";
  filename.textContent = hit.relative_path;

  titleArea.appendChild(fileNum);
  titleArea.appendChild(filename);
  header.appendChild(titleArea);
  card.appendChild(header);

  const body = document.createElement("div");
  body.className = "file-card-body";
  body.style.minHeight = "auto";

  const before = document.createTextNode(hit.context_before);
  const mark = document.createElement("mark");
  mark.className = "search-match current-match";
  mark.textContent = hit.match_text;
  const after = document.createTextNode(hit.context_after);

  body.appendChild(before);
  body.appendChild(mark);
  body.appendChild(after);
  card.appendChild(body);

  container.appendChild(card);
}

async function navigateSearch(dir: number): Promise<void> {
  if (!state.lastSearchResult || state.lastSearchResult.hits.length === 0) return;

  const activeTabContent = getActivePreviewCardsContainer();
  if (activeTabContent) {
    const prevMarks = activeTabContent.querySelectorAll("mark.current-match");
    prevMarks.forEach(m => m.classList.remove("current-match"));
  }

  state.currentMatchIndex += dir;

  if (state.currentMatchIndex < 0) state.currentMatchIndex = state.lastSearchResult.hits.length - 1;
  if (state.currentMatchIndex >= state.lastSearchResult.hits.length) state.currentMatchIndex = 0;

  renderSearchHit();
  updateSearchUI();
}

function updateSearchUI(): void {
  if (!state.currentSearchQuery) {
    dom.previewMatchCount.textContent = "";
    dom.searchPrev.disabled = true;
    dom.searchNext.disabled = true;
    return;
  }

  if (state.isSearching) {
    dom.previewMatchCount.textContent = "Searching...";
    dom.searchPrev.disabled = true;
    dom.searchNext.disabled = true;
    return;
  }

  if (!state.lastSearchResult) {
    dom.previewMatchCount.textContent = "";
    dom.searchPrev.disabled = true;
    dom.searchNext.disabled = true;
    return;
  }

  const { hits, returned_hits, total_matches, truncated } = state.lastSearchResult;

  if (hits.length === 0) {
    dom.previewMatchCount.textContent = "0 matches in selected files";
    dom.searchPrev.disabled = true;
    dom.searchNext.disabled = true;
    return;
  }

  let text = `${state.currentMatchIndex + 1}/${returned_hits}`;
  if (truncated) {
    text += ` shown, ${total_matches.toLocaleString()} total in selected files`;
  } else if (total_matches > returned_hits) {
    text += ` of ${total_matches.toLocaleString()} in selected files`;
  } else {
    text += " in selected files";
  }
  dom.previewMatchCount.textContent = text;
  dom.searchPrev.disabled = false;
  dom.searchNext.disabled = false;
}
