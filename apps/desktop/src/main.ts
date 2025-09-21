import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { CleaningConfig, PdfEmbeddedTextStrategy, ReplacementRule, TableExtractionStrategy } from "./generated/CleaningConfig.js";

// Models matching Rust core
interface DocumentRecord {
  source_path: string;
  relative_path: string;
  document_type: string;
  size_bytes: number;
}

interface DocumentTypeCounts {
  text: number;
  html: number;
  docx: number;
  pdf: number;
}

interface CorpusSummary {
  root: string;
  files_discovered: number;
  files_supported: number;
  files_ignored: number;
  total_size_bytes: number;
  document_type_counts: DocumentTypeCounts;
}

interface ScanReport {
  root: string;
  files: DocumentRecord[];
  files_discovered: number;
  files_supported: number;
  files_ignored: number;
  total_size_bytes: number;
  summary: CorpusSummary;
}

interface CorpusLoadResult {
  report: ScanReport;
  corpusVersion: number;
}

interface ExportReport {
  files_exported: number;
  warnings_count: number;
}

interface PreviewWarning {
  source_path?: string;
  relative_path?: string;
  kind: string;
  message: string;
}

interface FilePreview {
  source_path: string;
  relative_path: string;
  document_type: string;
  text: string;
  source_size_bytes: number;
  included_char_count: number;
  truncated: boolean;
  warnings: PreviewWarning[];
}

interface CombinedPreview {
  files: FilePreview[];
  combined_text: string;
  total_files_previewed: number;
  total_characters_included: number;
  warnings: PreviewWarning[];
}

let currentCorpusVersion = 0;
let allFiles: DocumentRecord[] = [];
let visibleFiles: { corpusIndex: number; record: DocumentRecord }[] = [];
let selectedCorpusIndices: Set<number> = new Set();
let debounceTimer: number | null = null;
let currentCorpusRoot: string | null = null;
let lastSelectedCorpusIndex: number | null = null;
let previewGeneration = 0;
let activePreviewGeneration = 0;
let wordCountGeneration = 0;

// Virtual scrolling constants
const ITEM_HEIGHT = 26; // px, must match CSS #file-list li height
const OVERSCAN = 10; // extra items rendered above/below viewport
let vsScrollTop = 0;
let vsContainerHeight = 0;

// Preview Lazy Loading
const PREVIEW_CHUNK_SIZE = 50;
let currentPreviewOffset = 0;
let isFetchingPreview = false;

function createDefaultCleaningConfig(): CleaningConfig {
  return {
    join_line_breaks: false,
    normalize_irregular_line_breaks: false,
    remove_standalone_page_numbers: false,
    remove_standalone_roman_page_numbers: false,
    remove_page_indicators: false,
    remove_page_delimiters: false,
    lowercase: false,
    normalize_line_endings: false,
    trim_lines: false,
    collapse_blank_lines: false,
    normalize_unicode: false,
    replace_diacritics: false,
    extract_html: false,
    table_extraction_strategy: "TabSeparated" as TableExtractionStrategy,
    remove_headers: false,
    remove_footers: false,
    remove_footnotes: false,
    remove_endnotes: false,
    remove_comments: false,
    remove_table_of_contents: false,
    remove_patterns: [],
    replace_patterns: [],
    pdf_embedded_text_strategy: "PdfiumFlat",
    remove_repeated_pdf_headers_footers: false,
    remove_pdf_page_labels: false,
    remove_pdf_symbol_heavy_artifacts: false,
    remove_pdf_code_like_blocks: false,
    remove_pdf_formula_like_lines: false
  };
}

let activeCleaningConfig = createDefaultCleaningConfig();

// DOM Elements
const fileList = document.getElementById("file-list") as HTMLUListElement;
const searchInput = document.getElementById("search-input") as HTMLInputElement;
const selectAllCheckbox = document.getElementById("select-all-checkbox") as HTMLInputElement;
const previewCapInput = document.getElementById("preview-cap-input") as HTMLInputElement;
const previewCapWarning = document.getElementById("preview-cap-warning") as HTMLDivElement;
const filesStatus = document.getElementById("files-status") as HTMLDivElement;
const corpusSummary = document.getElementById("corpus-summary") as HTMLDivElement;
const previewContent = document.getElementById("preview-content") as HTMLDivElement;
const processedPreviewContent = document.getElementById("processed-preview-content") as HTMLDivElement;
const previewLoadingOverlay = document.getElementById("preview-loading-overlay") as HTMLDivElement;
const statusBar = document.getElementById("status-bar") as HTMLElement;
const previewTabs = document.querySelectorAll(".right-panel .tab");
const previewTabContents = document.querySelectorAll(".right-panel .tab-content");

const settingsTabs = document.querySelectorAll("#settings-modal .tab");
const settingsTabContents = document.querySelectorAll("#settings-modal .tab-content");

const chkJoinLineBreaks = document.getElementById("chk-join-line-breaks") as HTMLInputElement;
const chkNormalizeIrregularLineBreaks = document.getElementById("chk-normalize-irregular-line-breaks") as HTMLInputElement;
const chkRemoveStandalonePageNumbers = document.getElementById("chk-remove-standalone-page-numbers") as HTMLInputElement;
const chkRemoveStandaloneRomanPageNumbers = document.getElementById("chk-remove-standalone-roman-page-numbers") as HTMLInputElement;
const chkRemovePageIndicators = document.getElementById("chk-remove-page-indicators") as HTMLInputElement;
const chkRemovePageDelimiters = document.getElementById("chk-remove-page-delimiters") as HTMLInputElement;

const chkLowercase = document.getElementById("chk-lowercase") as HTMLInputElement;
const chkNormalize = document.getElementById("chk-normalize") as HTMLInputElement;
const chkTrim = document.getElementById("chk-trim") as HTMLInputElement;
const chkCollapse = document.getElementById("chk-collapse") as HTMLInputElement;
const chkNormalizeUnicode = document.getElementById("chk-normalize-unicode") as HTMLInputElement;
const chkReplaceDiacritics = document.getElementById("chk-replace-diacritics") as HTMLInputElement;

const chkExtractHtml = document.getElementById("chk-extract-html") as HTMLInputElement;
const selTableExtraction = document.getElementById("sel-table-extraction") as HTMLSelectElement;
const selPdfEmbeddedTextStrategy = document.getElementById("sel-pdf-embedded-text-strategy") as HTMLSelectElement;

const chkRemoveHeaders = document.getElementById("chk-remove-headers") as HTMLInputElement;
const chkRemoveFooters = document.getElementById("chk-remove-footers") as HTMLInputElement;
const chkRemoveFootnotes = document.getElementById("chk-remove-footnotes") as HTMLInputElement;
const chkRemoveEndnotes = document.getElementById("chk-remove-endnotes") as HTMLInputElement;
const chkRemoveComments = document.getElementById("chk-remove-comments") as HTMLInputElement;
const chkRemoveToc = document.getElementById("chk-remove-toc") as HTMLInputElement;
const chkRemoveRepeatedPdfHeadersFooters = document.getElementById("chk-remove-repeated-pdf-headers-footers") as HTMLInputElement;
const chkRemovePdfPageLabels = document.getElementById("chk-remove-pdf-page-labels") as HTMLInputElement;
const chkRemovePdfSymbolHeavyArtifacts = document.getElementById("chk-remove-pdf-symbol-heavy-artifacts") as HTMLInputElement;
const chkRemovePdfCodeLikeBlocks = document.getElementById("chk-remove-pdf-code-like-blocks") as HTMLInputElement;
const chkRemovePdfFormulaLikeLines = document.getElementById("chk-remove-pdf-formula-like-lines") as HTMLInputElement;

// ── Data-driven checkbox binding table ──────────────────────────────────────
// Maps CleaningConfig boolean fields → checkbox DOM elements so that
// syncCheckboxesFromConfig / readCheckboxesIntoConfig replace the three
// repeated manual sync blocks in init(), modal-open and apply.
type BooleanCleaningConfigKey = {
  [K in keyof CleaningConfig & string]: CleaningConfig[K] extends boolean ? K : never;
}[keyof CleaningConfig & string];

interface CheckboxBinding {
  configKey: BooleanCleaningConfigKey;
  element: HTMLInputElement;
}

const cleaningCheckboxBindings: CheckboxBinding[] = [
  { configKey: "join_line_breaks",                           element: chkJoinLineBreaks },
  { configKey: "normalize_irregular_line_breaks",            element: chkNormalizeIrregularLineBreaks },
  { configKey: "remove_standalone_page_numbers",             element: chkRemoveStandalonePageNumbers },
  { configKey: "remove_standalone_roman_page_numbers",       element: chkRemoveStandaloneRomanPageNumbers },
  { configKey: "remove_page_indicators",                     element: chkRemovePageIndicators },
  { configKey: "remove_page_delimiters",                     element: chkRemovePageDelimiters },
  { configKey: "lowercase",                                  element: chkLowercase },
  { configKey: "normalize_line_endings",                     element: chkNormalize },
  { configKey: "trim_lines",                                 element: chkTrim },
  { configKey: "collapse_blank_lines",                       element: chkCollapse },
  { configKey: "normalize_unicode",                          element: chkNormalizeUnicode },
  { configKey: "replace_diacritics",                         element: chkReplaceDiacritics },
  { configKey: "extract_html",                               element: chkExtractHtml },
  { configKey: "remove_headers",                             element: chkRemoveHeaders },
  { configKey: "remove_footers",                             element: chkRemoveFooters },
  { configKey: "remove_footnotes",                           element: chkRemoveFootnotes },
  { configKey: "remove_endnotes",                            element: chkRemoveEndnotes },
  { configKey: "remove_comments",                            element: chkRemoveComments },
  { configKey: "remove_table_of_contents",                   element: chkRemoveToc },
  { configKey: "remove_repeated_pdf_headers_footers",        element: chkRemoveRepeatedPdfHeadersFooters },
  { configKey: "remove_pdf_page_labels",                     element: chkRemovePdfPageLabels },
  { configKey: "remove_pdf_symbol_heavy_artifacts",          element: chkRemovePdfSymbolHeavyArtifacts },
  { configKey: "remove_pdf_code_like_blocks",                element: chkRemovePdfCodeLikeBlocks },
  { configKey: "remove_pdf_formula_like_lines",              element: chkRemovePdfFormulaLikeLines },
];

function syncCheckboxesFromConfig(config: CleaningConfig): void {
  for (const { configKey, element } of cleaningCheckboxBindings) {
    element.checked = config[configKey];
  }
}

function readCheckboxesIntoConfig(config: CleaningConfig): void {
  for (const { configKey, element } of cleaningCheckboxBindings) {
    config[configKey] = element.checked;
  }
}

const menuOpenDir = document.getElementById("menu-open-dir") as HTMLDivElement;
const menuOpenFiles = document.getElementById("menu-open-files") as HTMLDivElement;
const menuSaveCorpus = document.getElementById("menu-save-corpus") as HTMLDivElement;

const menuProcessingParams = document.getElementById("menu-processing-params") as HTMLDivElement;

const settingsModal = document.getElementById("settings-modal") as HTMLDivElement;
const cancelSettingsBtn = document.getElementById("cancel-settings-btn") as HTMLButtonElement;
const applySettingsBtn = document.getElementById("apply-settings-btn") as HTMLButtonElement;

const customRemovalInput = document.getElementById("custom-removal-input") as HTMLInputElement;
const btnAddCustomRemoval = document.getElementById("btn-add-custom-removal") as HTMLButtonElement;
const btnClearCustomRemovals = document.getElementById("btn-clear-custom-removals") as HTMLButtonElement;
const customRemovalsList = document.getElementById("custom-removals-list") as HTMLDivElement;
const customRemovalsCount = document.getElementById("custom-removals-count") as HTMLSpanElement;

let tempRemovePatterns: string[] = [];
let tempReplacePatterns: ReplacementRule[] = [];

const ALLOWED_TABLE_STRATEGIES = ["TabSeparated", "FlattenParagraphs", "Ignore"] as const;

const ALLOWED_PDF_EMBEDDED_TEXT_STRATEGIES = [
  "PdfiumFlat",
  "PdfiumVisualSingleColumn",
  "PdfiumVisualColumnsExperimental",
] as const;

function isPdfEmbeddedTextStrategy(value: unknown): value is PdfEmbeddedTextStrategy {
  return (
    typeof value === "string" &&
    (ALLOWED_PDF_EMBEDDED_TEXT_STRATEGIES as readonly string[]).includes(value)
  );
}

const btnLoadConfig = document.getElementById("btn-load-config") as HTMLButtonElement;
const btnSaveConfig = document.getElementById("btn-save-config") as HTMLButtonElement;
const modalConfigStatus = document.getElementById("modal-config-status") as HTMLSpanElement;

function setModalConfigStatus(message: string): void {
  modalConfigStatus.textContent = message;
}

const themeToggle = document.getElementById("theme-toggle") as HTMLButtonElement;

const previewSearchInput = document.getElementById("preview-search-input") as HTMLInputElement;
const previewMatchCount = document.getElementById("preview-match-count") as HTMLSpanElement;
const searchPrev = document.getElementById("search-prev") as HTMLButtonElement;
const searchNext = document.getElementById("search-next") as HTMLButtonElement;

let currentMatchIndex = -1;
let currentSearchQuery = "";
let lastSearchedQuery = "";
let isSearching = false;
let searchGeneration = 0;
let pendingSearchAfterCurrent = false;
let pendingSearchNavigation: -1 | 0 | 1 = 0;
let searchDebounceTimer: number | undefined;

interface SearchHit {
  corpus_index: number;
  relative_path: string;
  source_path?: string;
  context_before: string;
  match_text: string;
  context_after: string;
  file_match_index: number;
}

interface SearchResult {
  total_matches: number;
  matching_file_indices: number[];
  returned_hits: number;
  truncated: boolean;
  hits: SearchHit[];
}

let lastSearchResult: SearchResult | null = null;

function renderCustomRemovals() {
  customRemovalsList.innerHTML = "";
  customRemovalsCount.textContent = `${tempRemovePatterns.length} item${tempRemovePatterns.length === 1 ? "" : "s"}`;

  tempRemovePatterns.forEach((pattern, index) => {
    const pill = document.createElement("div");
    pill.className = "sequence-pill";

    const textSpan = document.createElement("span");
    textSpan.textContent = pattern; // This prevents HTML from rendering!

    const delBtn = document.createElement("button");
    delBtn.className = "sequence-pill-delete";
    delBtn.type = "button";
    delBtn.innerHTML = "&times;";
    delBtn.onclick = () => {
      tempRemovePatterns.splice(index, 1);
      renderCustomRemovals();
    };

    pill.appendChild(textSpan);
    pill.appendChild(delBtn);
    customRemovalsList.appendChild(pill);
  });
}


// ── Config load/save helpers ─────────────────────────────────────────────

function normaliseCleaningConfig(raw: unknown): CleaningConfig {
  if (raw === null || raw === undefined || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("Invalid config: expected a JSON object.");
  }

  const config = createDefaultCleaningConfig();
  const obj = raw as Record<string, unknown>;

  // Boolean fields from checkbox bindings
  const configAny = config as unknown as Record<string, unknown>;
  for (const { configKey } of cleaningCheckboxBindings) {
    const val = obj[configKey];
    if (typeof val === "boolean") {
      configAny[configKey] = val;
    }
  }

  // table_extraction_strategy
  if (typeof obj.table_extraction_strategy === "string" &&
      (ALLOWED_TABLE_STRATEGIES as ReadonlyArray<string>).includes(obj.table_extraction_strategy)) {
    config.table_extraction_strategy = obj.table_extraction_strategy as TableExtractionStrategy;
  }

  // pdf_embedded_text_strategy
  if (isPdfEmbeddedTextStrategy(obj.pdf_embedded_text_strategy)) {
    config.pdf_embedded_text_strategy = obj.pdf_embedded_text_strategy;
  }

  // remove_patterns
  if (Array.isArray(obj.remove_patterns) && obj.remove_patterns.every((p: unknown) => typeof p === "string")) {
    config.remove_patterns = [...obj.remove_patterns];
  }

  // replace_patterns
  if (Array.isArray(obj.replace_patterns) &&
      obj.replace_patterns.every(
        (r: unknown) =>
          typeof r === "object" &&
          r !== null &&
          typeof (r as Record<string, unknown>).pattern === "string" &&
          typeof (r as Record<string, unknown>).replacement === "string"
      )) {
    config.replace_patterns = [...obj.replace_patterns];
  }

  return config;
}

function buildConfigFromModalControls(): CleaningConfig {
  const config = createDefaultCleaningConfig();
  readCheckboxesIntoConfig(config);
  config.table_extraction_strategy = selTableExtraction.value as TableExtractionStrategy;
  config.pdf_embedded_text_strategy = selPdfEmbeddedTextStrategy.value as PdfEmbeddedTextStrategy;
  config.remove_patterns = [...tempRemovePatterns];
  config.replace_patterns = [...tempReplacePatterns];
  return config;
}

function syncModalControlsFromConfig(config: CleaningConfig): void {
  syncCheckboxesFromConfig(config);
  selTableExtraction.value = config.table_extraction_strategy;
  selPdfEmbeddedTextStrategy.value = config.pdf_embedded_text_strategy;
  tempRemovePatterns = [...config.remove_patterns];
  tempReplacePatterns = [...config.replace_patterns];
  renderCustomRemovals();
}

// ── Load / Save Config handlers ──────────────────────────────────────────

async function handleLoadConfig(): Promise<void> {
  setModalConfigStatus("");
  const selected = await open({
    multiple: false,
    filters: [{ name: "JSON Config", extensions: ["json"] }]
  });
  if (selected === null) return;

  let fileContent: string;
  try {
    fileContent = await invoke<string>("read_config_file_command", { path: selected });
  } catch (err) {
    setModalConfigStatus(`Error reading file: ${err}`);
    return;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(fileContent);
  } catch {
    setModalConfigStatus("Error: Invalid JSON file.");
    return;
  }

  // Accept { cleaning_config: ... } wrapper if present
  if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed) &&
      "cleaning_config" in (parsed as Record<string, unknown>)) {
    parsed = (parsed as Record<string, unknown>).cleaning_config;
  }

  let normalised: CleaningConfig;
  try {
    normalised = normaliseCleaningConfig(parsed);
  } catch (err) {
    setModalConfigStatus(`Error: ${err}`);
    return;
  }

  syncModalControlsFromConfig(normalised);
  setModalConfigStatus("Loaded config. Click Apply to use it.");
}

async function handleSaveConfig(): Promise<void> {
  setModalConfigStatus("");
  const config = buildConfigFromModalControls();
  const json = JSON.stringify(config, null, 2);

  const selected = await save({
    defaultPath: "corpusaid-processing-config.json",
    filters: [{ name: "JSON Config", extensions: ["json"] }]
  });
  if (selected === null) return;

  try {
    await invoke("save_config_file_command", { path: selected, content: json });
    setModalConfigStatus("Saved processing config.");
  } catch (err) {
    setModalConfigStatus(`Error saving config: ${err}`);
  }
}

// Initialize
function init() {
  // Sync checkboxes and strategy dropdown to initial activeCleaningConfig state (all false/unchecked)
  syncCheckboxesFromConfig(activeCleaningConfig);
  selTableExtraction.value = activeCleaningConfig.table_extraction_strategy;
  selPdfEmbeddedTextStrategy.value = activeCleaningConfig.pdf_embedded_text_strategy;

  menuOpenDir.addEventListener("click", handleOpenDir);
  menuOpenFiles.addEventListener("click", handleOpenFiles);
  menuSaveCorpus.addEventListener("click", handleExport);

  // Initialise virtual scrolling for file list
  initVirtualScroll();

  themeToggle.addEventListener("click", () => {
    const current = document.documentElement.dataset.theme;
    document.documentElement.dataset.theme = current === "light" ? "dark" : "light";
  });

  btnAddCustomRemoval.addEventListener("click", () => {
    const val = customRemovalInput.value;
    if (val) {
      tempRemovePatterns.push(val);
      customRemovalInput.value = "";
      renderCustomRemovals();
    }
  });

  customRemovalInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      btnAddCustomRemoval.click();
    }
  });

  btnClearCustomRemovals.addEventListener("click", () => {
    tempRemovePatterns = [];
    renderCustomRemovals();
  });

  previewSearchInput.addEventListener("input", () => {
    currentSearchQuery = previewSearchInput.value;
    searchGeneration += 1;          // immediately invalidate any in-flight search
    pendingSearchNavigation = 0;    // cancel queued navigation
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = window.setTimeout(() => {
      executeGlobalSearch();
    }, 250);
  });

  previewSearchInput.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    if (!currentSearchQuery) return;

    const dir = e.shiftKey ? -1 : 1;

    // Flush debounce
    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = undefined;
    }

    if (isSearching) {
      // Search in flight — queue for after it completes
      // Invalidate current search if query changed
      if (currentSearchQuery !== lastSearchedQuery) {
        searchGeneration += 1;
      }
      pendingSearchAfterCurrent = true;
      pendingSearchNavigation = dir;
      return;
    }

    // Not currently searching
    if (currentSearchQuery !== lastSearchedQuery) {
      // Fresh query — run search, land on first match (no double-jump)
      searchGeneration += 1;
      pendingSearchNavigation = 0;
      executeGlobalSearch();
      return;
    }

    // Already searched — navigate directly through backend hits
    if (lastSearchResult && lastSearchResult.hits.length > 0) {
      navigateSearch(dir);
    }
  });

  searchPrev.addEventListener("click", () => navigateSearch(-1));
  searchNext.addEventListener("click", () => navigateSearch(1));

  searchInput.addEventListener("input", handleSearch);

  previewTabs.forEach(tab => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-target");
      previewTabs.forEach(t => {
        t.classList.remove("active");
        t.setAttribute("aria-selected", "false");
      });
      previewTabContents.forEach(c => c.classList.remove("active"));

      tab.classList.add("active");
      tab.setAttribute("aria-selected", "true");
      document.getElementById(target!)?.classList.add("active");

      // Reset search state for new tab content
      searchGeneration += 1;
      currentMatchIndex = -1;
      lastSearchResult = null;
      pendingSearchNavigation = 0;
      pendingSearchAfterCurrent = false;
      if (searchDebounceTimer) { clearTimeout(searchDebounceTimer); searchDebounceTimer = undefined; }

      executeGlobalSearch();
    });
  });

  settingsTabs.forEach(tab => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-target");
      settingsTabs.forEach(t => {
        t.classList.remove("active");
        t.setAttribute("aria-selected", "false");
      });
      settingsTabContents.forEach(c => c.classList.remove("active"));

      tab.classList.add("active");
      tab.setAttribute("aria-selected", "true");
      document.getElementById(target!)?.classList.add("active");
    });
  });

  // Config load/save buttons
  btnLoadConfig.addEventListener("click", handleLoadConfig);
  btnSaveConfig.addEventListener("click", handleSaveConfig);

  menuProcessingParams.addEventListener("click", () => {
    // Clear any previous modal status
    setModalConfigStatus("");
    // Populate draft state from active config
    syncCheckboxesFromConfig(activeCleaningConfig);
    selTableExtraction.value = activeCleaningConfig.table_extraction_strategy;
    selPdfEmbeddedTextStrategy.value = activeCleaningConfig.pdf_embedded_text_strategy;
    tempRemovePatterns = [...activeCleaningConfig.remove_patterns];
    tempReplacePatterns = [...activeCleaningConfig.replace_patterns];
    renderCustomRemovals();

    // Reset modal to General tab by default when opened
    settingsTabs.forEach(t => {
      t.classList.remove("active");
      t.setAttribute("aria-selected", "false");
    });
    settingsTabContents.forEach(c => c.classList.remove("active"));
    const generalTab = document.querySelector('#settings-modal .tab[data-target="tab-general"]');
    if (generalTab) {
      generalTab.classList.add("active");
      generalTab.setAttribute("aria-selected", "true");
    }
    document.getElementById("tab-general")?.classList.add("active");

    settingsModal.classList.remove("hidden");
  });

  function closeSettingsModal(): void {
    setModalConfigStatus("");
    settingsModal.classList.add("hidden");
  }

  cancelSettingsBtn.addEventListener("click", closeSettingsModal);

  const btnCloseSettingsModalTop = document.getElementById("btn-close-settings-modal-top") as HTMLButtonElement;
  if (btnCloseSettingsModalTop) {
    btnCloseSettingsModalTop.addEventListener("click", closeSettingsModal);
  }

  applySettingsBtn.addEventListener("click", () => {
    // Save draft state to active config
    readCheckboxesIntoConfig(activeCleaningConfig);
    activeCleaningConfig.table_extraction_strategy = selTableExtraction.value as TableExtractionStrategy;
    activeCleaningConfig.pdf_embedded_text_strategy = selPdfEmbeddedTextStrategy.value as PdfEmbeddedTextStrategy;
    activeCleaningConfig.remove_patterns = [...tempRemovePatterns];
    activeCleaningConfig.replace_patterns = [...tempReplacePatterns];

    setModalConfigStatus("");
    settingsModal.classList.add("hidden");

    // Recompute word count with the updated cleaning config
    updateWordCount();

    // Refresh preview if files are selected
    if (selectedCorpusIndices.size > 0) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(updatePreview, 150);
    }
  });
  selectAllCheckbox.addEventListener("change", (e) => {
    const isChecked = (e.target as HTMLInputElement).checked;
    if (isChecked) {
      visibleFiles.forEach(vf => selectedCorpusIndices.add(vf.corpusIndex));
    } else {
      selectedCorpusIndices.clear();
    }
    previewGeneration += 1;
    activePreviewGeneration = previewGeneration;
    renderVisibleItems();
    filesStatus.textContent = `${allFiles.length} loaded | ${selectedCorpusIndices.size} selected`;

    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(updatePreview, 150);
  });

  previewCapInput.addEventListener("input", (e) => {
    const valStr = (e.target as HTMLInputElement).value;
    if (!valStr) return; // ignore empty
    const val = parseInt(valStr, 10);

    if (val > 500) {
      previewCapWarning.style.display = "block";
    } else {
      previewCapWarning.style.display = "none";
    }

    if (!isNaN(val) && val > 0) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => {
        selectedCorpusIndices.clear();
        const limit = Math.min(val, visibleFiles.length);
        for (let i = 0; i < limit; i++) {
          selectedCorpusIndices.add(visibleFiles[i].corpusIndex);
        }
        previewGeneration += 1;
        activePreviewGeneration = previewGeneration;
        renderVisibleItems();
        filesStatus.textContent = `${allFiles.length} loaded | ${selectedCorpusIndices.size} selected`;
        updatePreview();
      }, 500);
    }
  });



  // Accessibility: Escape to close modals and menus
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (!settingsModal.classList.contains("hidden")) {
        settingsModal.classList.add("hidden");
      }

      document.querySelectorAll('.dropdown').forEach(d => d.classList.remove('is-open'));
    }
  });

  // Menu Dropdown Click Handling
  const dropdowns = document.querySelectorAll('.dropdown');

  dropdowns.forEach(dropdown => {
    const menuLabel = dropdown.querySelector('.menu-item');
    menuLabel?.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = dropdown.classList.contains('is-open');
      dropdowns.forEach(d => d.classList.remove('is-open'));
      if (!isOpen) {
        dropdown.classList.add('is-open');
      }
    });
  });

  document.addEventListener('click', () => {
    dropdowns.forEach(d => d.classList.remove('is-open'));
  });

  const dropdownItems = document.querySelectorAll('.dropdown-item');
  dropdownItems.forEach(item => {
    item.addEventListener('click', () => {
      dropdowns.forEach(d => d.classList.remove('is-open'));
    });
  });

  // Sidebar Resizing Logic
  const splitter = document.getElementById("sidebar-splitter");
  const leftPanel = document.querySelector(".left-panel") as HTMLElement;
  let isResizing = false;

  if (splitter && leftPanel) {
    splitter.addEventListener("mousedown", (e) => {
      isResizing = true;
      splitter.classList.add("active");
      document.body.style.cursor = "col-resize";
      e.preventDefault();
    });

    document.addEventListener("mousemove", (e) => {
      if (!isResizing) return;
      e.preventDefault();

      let newWidth = e.clientX;
      const maxW = Math.min(640, window.innerWidth * 0.55);

      if (newWidth < 240) newWidth = 240;
      if (newWidth > maxW) newWidth = maxW;

      leftPanel.style.width = `${newWidth}px`;
    });

    document.addEventListener("mouseup", () => {
      if (isResizing) {
        isResizing = false;
        splitter.classList.remove("active");
        document.body.style.cursor = "default";
      }
    });
  }

  initRepeatedArtifactFinder();
}

function clearStateForLoad() {
  currentCorpusVersion = 0;
  allFiles = [];
  visibleFiles = [];
  selectedCorpusIndices.clear();
  previewContent.textContent = "Select files from the left panel to preview their contents.";
  processedPreviewContent.textContent = "Select files from the left panel to preview processed text.";
  updateFileList();
}

async function handleOpenDir() {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    statusBar.textContent = "Open directory cancelled.";
    return;
  }

  try {
    statusBar.textContent = "Scanning...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("scan_directory_command", { path: selected });

    currentCorpusVersion = result.corpusVersion;
    currentCorpusRoot = result.report.root;
    allFiles = result.report.files;
    visibleFiles = allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();

    statusBar.textContent = `Loaded ${allFiles.length} files.`;
    updateWordCount();
  } catch (error) {
    statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
}

async function handleOpenFiles() {
  const selected = await open({
    multiple: true,
    filters: [{ name: "Supported Documents", extensions: ["txt", "html", "htm", "docx", "pdf"] }]
  });
  if (selected === null || !Array.isArray(selected)) {
    statusBar.textContent = "Open files cancelled.";
    return;
  }

  try {
    statusBar.textContent = "Loading files...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("load_files_command", { paths: selected });

    currentCorpusVersion = result.corpusVersion;
    currentCorpusRoot = result.report.root;
    allFiles = result.report.files;
    visibleFiles = allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();

    statusBar.textContent = `Loaded ${allFiles.length} files.`;
    updateWordCount();
  } catch (error) {
    statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
}

function renderSummary(summary: CorpusSummary) {
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

  corpusSummary.innerHTML = `
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

async function updateWordCount() {
  const myWordCountGeneration = ++wordCountGeneration;
  const myVersion = currentCorpusVersion;
  const totalWordsEl = document.getElementById('summary-total-words');
  const avgWordsEl = document.getElementById('summary-avg-words');
  const wordLabelEl = document.getElementById('summary-word-label');
  if (!totalWordsEl || !avgWordsEl) return;

  if (allFiles.length === 0) {
    totalWordsEl.textContent = '0';
    avgWordsEl.textContent = '0';
    return;
  }

  // Process word count in batches for progressive feedback
  const BATCH_SIZE = 500;
  let totalWords = 0;
  const totalFiles = allFiles.length;

  totalWordsEl.textContent = 'Counting...';
  avgWordsEl.textContent = 'Counting...';
  if (wordLabelEl) wordLabelEl.textContent = 'Known Cleaned Tokens';

  try {
    for (let offset = 0; offset < totalFiles; offset += BATCH_SIZE) {
      const batchSize = Math.min(BATCH_SIZE, totalFiles - offset);
      const batchIndices = Array.from({ length: batchSize }, (_, batchIndex) => offset + batchIndex);
      const batchWords = await invoke<number>("compute_word_count_command", {
        indices: batchIndices,
        corpusVersion: myVersion,
        cleaningConfig: activeCleaningConfig
      });
      totalWords += batchWords;

      // Progressive update
      const processed = Math.min(offset + BATCH_SIZE, totalFiles);
      const avgWords = totalWords / processed;
      if (myVersion !== currentCorpusVersion || myWordCountGeneration !== wordCountGeneration) return;
      totalWordsEl.textContent = `${totalWords.toLocaleString()} (${processed}/${totalFiles} files)`;
      avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
    }

    if (myVersion !== currentCorpusVersion || myWordCountGeneration !== wordCountGeneration) return;
    // Final display
    if (wordLabelEl) {
      wordLabelEl.textContent = 'Cleaned Token Count';
      wordLabelEl.title = 'Counts whitespace-separated tokens after cleaning. PDF OCR is not used for this count.';
    }
    const avgWords = totalWords / totalFiles;
    totalWordsEl.textContent = totalWords.toLocaleString();
    avgWordsEl.textContent = avgWords.toLocaleString(undefined, { maximumFractionDigits: 2 });
  } catch (err) {
    if (myVersion !== currentCorpusVersion || myWordCountGeneration !== wordCountGeneration) return;
    totalWordsEl.textContent = 'Error';
    avgWordsEl.textContent = 'Error';
    console.error("Failed to compute word count", err);
  }
}

function handleSearch() {
  const query = searchInput.value.toLowerCase();
  visibleFiles = allFiles
    .map((record, corpusIndex) => ({ corpusIndex, record }))
    .filter(({ record }) => !query || record.relative_path.toLowerCase().includes(query));
  selectedCorpusIndices.clear();
  previewGeneration += 1;
  activePreviewGeneration = previewGeneration;

  const container = document.querySelector('.file-list-container') as HTMLElement;
  if (container) {
    container.scrollTop = 0;
    vsScrollTop = 0;
  }

  updateFileList();
}

function initVirtualScroll() {
  const container = document.querySelector('.file-list-container') as HTMLElement;
  if (!container) return;

  // Create sentinel div that sets the total scrollable height
  let sentinel = container.querySelector('.virtual-scroll-sentinel') as HTMLElement;
  if (!sentinel) {
    sentinel = document.createElement('div');
    sentinel.className = 'virtual-scroll-sentinel';
    container.insertBefore(sentinel, fileList);
  }

  container.addEventListener('scroll', () => {
    vsScrollTop = container.scrollTop;
    renderVisibleItems();
  });

  // Track container resize
  const ro = new ResizeObserver((entries) => {
    for (const entry of entries) {
      vsContainerHeight = entry.contentRect.height;
    }
    renderVisibleItems();
  });
  ro.observe(container);
  vsContainerHeight = container.clientHeight;
}

function updateFileList() {
  filesStatus.textContent = `${allFiles.length} loaded | ${selectedCorpusIndices.size} selected`;

  // Update sentinel height to reflect total list size
  const container = document.querySelector('.file-list-container') as HTMLElement;
  const sentinel = container?.querySelector('.virtual-scroll-sentinel') as HTMLElement;
  if (sentinel) {
    sentinel.style.height = `${visibleFiles.length * ITEM_HEIGHT}px`;
  }

  renderVisibleItems();
  updatePreview();
}

function renderVisibleItems() {
  const totalItems = visibleFiles.length;

  // Calculate which items are visible
  const startIdx = Math.max(0, Math.floor(vsScrollTop / ITEM_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(vsContainerHeight / ITEM_HEIGHT) + 2 * OVERSCAN;
  const endIdx = Math.min(totalItems, startIdx + visibleCount);

  // Position the UL using transform (avoids layout thrashing)
  fileList.style.transform = `translateY(${startIdx * ITEM_HEIGHT}px)`;
  fileList.innerHTML = '';

  const fragment = document.createDocumentFragment();
  for (let i = startIdx; i < endIdx; i++) {
    const { corpusIndex, record } = visibleFiles[i];
    const li = document.createElement('li');
    const parts = record.relative_path.split(/[/\\]/);

    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.className = 'file-checkbox';
    checkbox.checked = selectedCorpusIndices.has(corpusIndex);

    const span = document.createElement('span');
    span.className = 'file-name';
    span.textContent = parts[parts.length - 1];

    li.appendChild(checkbox);
    li.appendChild(span);
    li.title = record.relative_path;

    if (selectedCorpusIndices.has(corpusIndex)) {
      li.classList.add('selected');
    }

    checkbox.addEventListener('click', (e) => {
      e.stopPropagation();
      handleFileSelect(e, i, true);
    });

    li.addEventListener('click', (e) => {
      handleFileSelect(e, i, false);
    });

    fragment.appendChild(li);
  }

  fileList.appendChild(fragment);
}

function handleFileSelect(event: MouseEvent, visibleIndex: number, isCheckboxClick: boolean = false) {
  const corpusIndex = visibleFiles[visibleIndex].corpusIndex;

  if (isCheckboxClick) {
    if (event.shiftKey && lastSelectedCorpusIndex !== null) {
      const lastVis = visibleFiles.findIndex(vf => vf.corpusIndex === lastSelectedCorpusIndex);
      if (lastVis !== -1) {
        const startVis = Math.min(lastVis, visibleIndex);
        const endVis = Math.max(lastVis, visibleIndex);
        for (let vi = startVis; vi <= endVis; vi++) {
          selectedCorpusIndices.add(visibleFiles[vi].corpusIndex);
        }
      }
    } else {
      if (selectedCorpusIndices.has(corpusIndex)) {
        selectedCorpusIndices.delete(corpusIndex);
      } else {
        selectedCorpusIndices.add(corpusIndex);
      }
    }
  } else {
    if (event.ctrlKey || event.metaKey) {
      if (selectedCorpusIndices.has(corpusIndex)) {
        selectedCorpusIndices.delete(corpusIndex);
      } else {
        selectedCorpusIndices.add(corpusIndex);
      }
    } else if (event.shiftKey && lastSelectedCorpusIndex !== null) {
      const lastVis = visibleFiles.findIndex(vf => vf.corpusIndex === lastSelectedCorpusIndex);
      selectedCorpusIndices.clear();
      if (lastVis !== -1) {
        const startVis = Math.min(lastVis, visibleIndex);
        const endVis = Math.max(lastVis, visibleIndex);
        for (let vi = startVis; vi <= endVis; vi++) {
          selectedCorpusIndices.add(visibleFiles[vi].corpusIndex);
        }
      }
    } else {
      selectedCorpusIndices.clear();
      selectedCorpusIndices.add(corpusIndex);
    }
  }

  // Update shift-click anchor — after a normal click or shift-click, anchor is the clicked item
  lastSelectedCorpusIndex = corpusIndex;

  // Invalidate current preview session
  previewGeneration += 1;
  activePreviewGeneration = previewGeneration;

  // Re-render only visible items (virtual scroll handles this efficiently)
  renderVisibleItems();

  filesStatus.textContent = `${allFiles.length} loaded | ${selectedCorpusIndices.size} selected`;

  // Debounce preview
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = window.setTimeout(updatePreview, 150);
}


async function updatePreview() {
  const myVersion = currentCorpusVersion;
  const myPreviewGeneration = ++previewGeneration;
  activePreviewGeneration = myPreviewGeneration;

  if (selectedCorpusIndices.size === 0) {
    if (myVersion !== currentCorpusVersion) return;
    previewContent.innerHTML = "Select files from the left panel to preview their contents.";
    processedPreviewContent.innerHTML = "Select files from the left panel to preview processed text.";
    statusBar.textContent = `Loaded ${allFiles.length} files.`;
    return;
  }

  const selectedIndices = Array.from(selectedCorpusIndices);
  currentPreviewOffset = 0;

  // Show loading overlay
  previewLoadingOverlay.style.display = 'flex';
  statusBar.textContent = `Previewing processed text...`;

  await fetchAndRenderPreviewChunk(selectedIndices, currentPreviewOffset, false, myVersion, myPreviewGeneration);
}

async function fetchAndRenderPreviewChunk(indices: number[], offset: number, append: boolean, myVersion: number, myPreviewGeneration: number) {
  if (isFetchingPreview) return;
  isFetchingPreview = true;

  const chunkIndices = indices.slice(offset, offset + PREVIEW_CHUNK_SIZE);
  if (chunkIndices.length === 0) {
    isFetchingPreview = false;
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
        cleaningConfig: activeCleaningConfig
      })
    ]);

    if (myVersion !== currentCorpusVersion || myPreviewGeneration !== activePreviewGeneration) return;

    renderPreviewCards(previewContent, original.files, append, offset, indices.length);
    renderPreviewCards(processedPreviewContent, processed.files, append, offset, indices.length);

    highlightRenderedCards(); // Highlighting

    statusBar.textContent = `Processed preview ready for ${indices.length} selected files. (Loaded ${offset + chunkIndices.length})`;
    currentPreviewOffset += chunkIndices.length;
  } catch (error) {
    if (myVersion !== currentCorpusVersion || myPreviewGeneration !== activePreviewGeneration) return;
    if (!append) {
      previewContent.textContent = `Error loading preview: ${error}`;
      processedPreviewContent.textContent = `Error loading preview: ${error}`;
    }
    statusBar.textContent = `Error loading preview.`;
  } finally {
    previewLoadingOverlay.style.display = 'none';
    isFetchingPreview = false;
  }
}

function escapeHtml(unsafe: string) {
  const map: Record<string, string> = {};
  map["&"] = String.fromCharCode(38, 97, 109, 112, 59);
  map["<"] = String.fromCharCode(38, 108, 116, 59);
  map[">"] = String.fromCharCode(38, 103, 116, 59);
  map['"'] = String.fromCharCode(38, 113, 117, 111, 116, 59);
  map["'"] = String.fromCharCode(38, 35, 48, 51, 57, 59);
  return unsafe.replace(/[&<>"']/g, (ch) => map[ch] || ch);
}

/**
 * Renders preview text as escaped plain text with optional search highlighting.
 * Tabs are rendered inline (expanded via CSS tab-size) — no table or column inference.
 */
function highlightPreviewText(text: string, query: string): string {
  const lines = text.split('\n');
  let outHtml = "";

  const processStr = (s: string) => {
    if (!query) return escapeHtml(s);
    const lowerS = s.toLowerCase();
    const lowerQ = query.toLowerCase();
    let idx = 0;
    let newHtml = "";
    while (true) {
      const found = lowerS.indexOf(lowerQ, idx);
      if (found === -1) {
        newHtml += escapeHtml(s.substring(idx));
        break;
      }
      newHtml += escapeHtml(s.substring(idx, found));
      newHtml += `<mark class="search-match">${escapeHtml(s.substring(found, found + lowerQ.length))}</mark>`;
      idx = found + lowerQ.length;
    }
    return newHtml;
  };

  for (let i = 0; i < lines.length; i++) {
    outHtml += `${processStr(lines[i])}${i < lines.length - 1 ? '\n' : ''}`;
  }

  return outHtml;
}

let previewObserver: IntersectionObserver | null = null;
let chunkSentinelObserver: IntersectionObserver | null = null;

function renderPreviewCards(container: HTMLDivElement, files: FilePreview[], append: boolean, offset: number, totalFiles: number) {
  if (!append) {
    container.innerHTML = "";
  }

  // Remove existing sentinel if any
  const existingSentinel = container.querySelector('.chunk-sentinel');
  if (existingSentinel) existingSentinel.remove();

  if (!previewObserver) {
    previewObserver = new IntersectionObserver((entries, observer) => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          const body = entry.target as HTMLElement;
          const text = body.dataset.originalText || "";
          body.innerHTML = highlightPreviewText(text, currentSearchQuery);
          observer.unobserve(body);

          // Remove min-height after content is loaded
          body.style.minHeight = 'auto';
        }
      });
    }, { rootMargin: '200px 0px' });
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
    // Set a min-height so that cards aren't all 0 height initially
    body.style.minHeight = '100px';
    (body as any).dataset.originalText = file.text;

    // Instead of rendering immediately, just append and observe
    previewObserver!.observe(body);

    card.appendChild(header);
    card.appendChild(body);
    container.appendChild(card);
  });

  if (offset + files.length < totalFiles) {
    const sentinel = document.createElement('div');
    sentinel.className = 'chunk-sentinel';
    sentinel.style.height = '10px';
    container.appendChild(sentinel);

    if (!chunkSentinelObserver) {
      chunkSentinelObserver = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
          if (entry.isIntersecting) {
            const myVersion = currentCorpusVersion;
            const myPreviewGeneration = activePreviewGeneration;
            const selectedIndices = Array.from(selectedCorpusIndices);
            fetchAndRenderPreviewChunk(selectedIndices, currentPreviewOffset, true, myVersion, myPreviewGeneration);
          }
        });
      }, { rootMargin: '400px 0px' });
    }
    chunkSentinelObserver.observe(sentinel);
  }
}

async function executeGlobalSearch() {
  if (isSearching) return;

  if (!currentSearchQuery) {
    lastSearchedQuery = "";
    lastSearchResult = null;
    currentMatchIndex = -1;
    pendingSearchNavigation = 0;
    pendingSearchAfterCurrent = false;
    // Restore normal preview
    if (selectedCorpusIndices.size > 0) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(updatePreview, 50);
    } else {
      const activeTabContent = document.querySelector(".right-panel .tab-content.active .preview-cards-container");
      if (activeTabContent) {
        const container = activeTabContent as HTMLDivElement;
        container.innerHTML = "Select files from the left panel to preview their contents.";
      }
    }
    updateSearchUI();
    return;
  }

  const query = currentSearchQuery.trim();
  const mySearchGen = ++searchGeneration;
  isSearching = true;
  updateSearchUI(); // shows "Searching..."

  let resultAccepted = false;
  let myVersion = currentCorpusVersion;

  try {
    myVersion = currentCorpusVersion;
    const isProcessed = document.querySelector(".right-panel .tab.active")?.getAttribute("data-target") === "processed-text";
    const indices = Array.from(selectedCorpusIndices);

    const result = await invoke<SearchResult>("search_corpus_command", {
      indices,
      corpusVersion: myVersion,
      query: query,
      isProcessed: isProcessed,
      cleaningConfig: activeCleaningConfig,
      maxHits: 1000
    });

    // Accept result only if search generation and corpus version still match
    if (mySearchGen === searchGeneration && myVersion === currentCorpusVersion) {
      lastSearchResult = result;
      lastSearchedQuery = query;
      resultAccepted = true;
    }
  } catch (err) {
    if (mySearchGen === searchGeneration) {
      console.error("Search failed", err);
    }
  } finally {
    isSearching = false;
  }

  // Handle Enter-pressed-while-we-were-searching
  if (pendingSearchAfterCurrent) {
    pendingSearchAfterCurrent = false;
    if (currentSearchQuery !== lastSearchedQuery) {
      // User typed a newer query while we were searching — run it now
      executeGlobalSearch();
      return;
    }
    // Same query — apply queued navigation
    if (pendingSearchNavigation !== 0 && lastSearchResult && lastSearchResult.hits.length > 0) {
      const navDir = pendingSearchNavigation;
      pendingSearchNavigation = 0;
      navigateSearch(navDir);
    }
    return;
  }

  if (!resultAccepted) return;

  // Apply any pending navigation (from Enter while search was running)
  if (pendingSearchNavigation !== 0 && lastSearchResult && lastSearchResult.hits.length > 0) {
    const navDir = pendingSearchNavigation;
    pendingSearchNavigation = 0;
    navigateSearch(navDir);
  }

  // Show the first result
  if (lastSearchResult && lastSearchResult.hits.length > 0) {
    currentMatchIndex = 0;
    renderSearchHit();
  }
  updateSearchUI();
}

function renderSearchHit() {
  if (!lastSearchResult || currentMatchIndex < 0 || currentMatchIndex >= lastSearchResult.hits.length) return;

  const hit = lastSearchResult.hits[currentMatchIndex];
  const isProcessed = document.querySelector(".right-panel .tab.active")?.getAttribute("data-target") === "processed-text";
  const containerId = isProcessed ? "processed-preview-content" : "preview-content";
  const container = document.getElementById(containerId) as HTMLDivElement;
  if (!container) return;

  // Clear container and render focused hit
  container.innerHTML = "";

  const card = document.createElement("div");
  card.className = "file-card";

  const header = document.createElement("div");
  header.className = "file-card-header";

  const titleArea = document.createElement("div");
  titleArea.className = "file-card-title-area";

  const fileNum = document.createElement("div");
  fileNum.className = "file-card-num";
  const { returned_hits, total_matches, truncated } = lastSearchResult;
  let countStr = `Hit ${currentMatchIndex + 1} of ${returned_hits}`;
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
  body.style.minHeight = 'auto';

  // Build context snippet using DOM nodes (safe, no HTML injection)
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

function highlightRenderedCards() {
  // No-op: search highlighting is now handled by renderSearchHit().
  // This function is kept to avoid breaking existing callers but does nothing
  // related to search hit navigation. It only restores original preview text
  // when no search query is active.
  const activeTabContent = document.querySelector(".right-panel .tab-content.active .preview-cards-container");
  if (!activeTabContent) return;
  const bodies = activeTabContent.querySelectorAll(".file-card-body");
  if (!currentSearchQuery) {
    bodies.forEach((body: any) => {
      const text = body.dataset.originalText || "";
      body.innerHTML = highlightPreviewText(text, "");
    });
  }
}

async function navigateSearch(dir: number) {
  // Navigate backend-returned hits, not DOM marks
  if (!lastSearchResult || lastSearchResult.hits.length === 0) return;

  // Clear previous visual highlight
  const activeTabContent = document.querySelector(".right-panel .tab-content.active .preview-cards-container");
  if (activeTabContent) {
    const prevMarks = activeTabContent.querySelectorAll("mark.current-match");
    prevMarks.forEach(m => m.classList.remove("current-match"));
  }

  currentMatchIndex += dir;

  // Wrap around
  if (currentMatchIndex < 0) currentMatchIndex = lastSearchResult.hits.length - 1;
  if (currentMatchIndex >= lastSearchResult.hits.length) currentMatchIndex = 0;

  renderSearchHit();
  updateSearchUI();
}

function updateSearchUI() {
  if (!currentSearchQuery) {
    previewMatchCount.textContent = "";
    searchPrev.disabled = true;
    searchNext.disabled = true;
    return;
  }

  if (isSearching) {
    previewMatchCount.textContent = "Searching...";
    searchPrev.disabled = true;
    searchNext.disabled = true;
    return;
  }

  if (!lastSearchResult) {
    previewMatchCount.textContent = "";
    searchPrev.disabled = true;
    searchNext.disabled = true;
    return;
  }

  const { hits, returned_hits, total_matches, truncated } = lastSearchResult;

  if (hits.length === 0) {
    previewMatchCount.textContent = "0 matches in selected files";
    searchPrev.disabled = true;
    searchNext.disabled = true;
    return;
  }

  let text = `${currentMatchIndex + 1}/${returned_hits}`;
  if (truncated) {
    text += ` shown, ${total_matches.toLocaleString()} total in selected files`;
  } else if (total_matches > returned_hits) {
    text += ` of ${total_matches.toLocaleString()} in selected files`;
  } else {
    text += ` in selected files`;
  }
  previewMatchCount.textContent = text;
  searchPrev.disabled = false;
  searchNext.disabled = false;
}

/**
 * Sanitise a folder name for safe filesystem use.
 * Replaces Windows-invalid filename characters and control characters with '_',
 * trims trailing dots/whitespace.
 */
function sanitizeFolderName(name: string): string {
  return name.replace(/[<>:"\/\\|?*\x00-\x1f]/g, '_')
    .replace(/\.+$/, '')
    .trim() || "CorpusAid";
}

async function handleExport() {
  if (allFiles.length === 0) {
    statusBar.textContent = "No files loaded. Open a corpus before saving.";
    return;
  }



  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: currentCorpusRoot ? currentCorpusRoot : undefined,
  });
  if (selected === null || Array.isArray(selected)) {
    statusBar.textContent = "Save cancelled.";
    return;
  }

const now = new Date();
  const pad = (n: number) => n.toString().padStart(2, '0');
  const timestamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
  // Derive corpus name from currentCorpusRoot basename
  let corpusName = "";
  if (currentCorpusRoot) {
    const parts = currentCorpusRoot.replace(/\\/g, '/').split('/');
    const basename = parts[parts.length - 1] || "";
    corpusName = sanitizeFolderName(basename);
  }
  const exportDirName = corpusName
    ? `${corpusName}_corpusaid_processed_${timestamp}`
    : `CorpusAid_processed_${timestamp}`;
  const separator = selected.includes("\\") ? "\\" : "/";
  const targetDir = `${selected}${selected.endsWith(separator) ? "" : separator}${exportDirName}`;

  statusBar.textContent = "Saving processed corpus... 0%";
  menuSaveCorpus.style.pointerEvents = "none";
  menuSaveCorpus.style.opacity = "0.5";
  document.body.style.cursor = "wait";
  document.body.classList.add("is-exporting");

  // Yield to browser to paint busy state
  await new Promise(requestAnimationFrame);
  await new Promise(resolve => setTimeout(resolve, 50));

  // Listen for progress events from backend
  let unlisten: UnlistenFn | null = null;
  try {
    unlisten = await listen<{ current: number; total: number; current_file: string }>("export-progress", (event) => {
      const { current, total, current_file } = event.payload;
      const pct = Math.round((current / total) * 100);
      statusBar.textContent = `Saving... ${pct}% (${current}/${total}) — ${current_file}`;
    });

    const myVersion = currentCorpusVersion;
    const indices = Array.from(
      { length: allFiles.length },
      (_, corpusIndex) => corpusIndex
    );

    const report: ExportReport = await invoke("export_corpus_command", {
      indices,
      corpusVersion: myVersion,
      outputDir: targetDir,
      cleaningConfig: activeCleaningConfig
    });

    if (myVersion !== currentCorpusVersion) return;

    statusBar.textContent = `Saved processed corpus: ${report.files_exported} files written to ${targetDir}`;
  } catch (error) {
    statusBar.textContent = `Save error: ${error}`;
    console.error("Save error", error);
  } finally {
    if (unlisten) unlisten();
    menuSaveCorpus.style.pointerEvents = "auto";
    menuSaveCorpus.style.opacity = "1";
    document.body.style.cursor = "default";
    document.body.classList.remove("is-exporting");
  }
}

interface RepeatedArtifactScanConfig {
  analyse_processed_text: boolean;
  include_exact_lines: boolean;
  include_normalized_lines: boolean;
  include_inline_artifacts: boolean;
  include_two_line_blocks: boolean;
  include_three_line_blocks: boolean;
  include_text_dominant: boolean;
  include_mixed_text_numbers: boolean;
  include_numeric_dominant: boolean;
  include_symbol_noise: boolean;
  min_occurrences: number;
  min_files: number;
  max_candidates: number;
  max_examples_per_candidate: number;
  min_line_chars: number;
  max_line_chars: number;
}

interface PositionSummary {
  top_count: number;
  middle_count: number;
  bottom_count: number;
  unknown_count: number;
}

interface RepeatedArtifactExample {
  file_name: string;
  file_path: string;
  line_number: number | null;
  page_number: number | null;
  context_before: string | null;
  matched_text: string;
  context_after: string | null;
}

interface RepeatedArtifactCandidate {
  candidate_id: string;
  kind: string;
  display_text: string;
  normalized_key: string;
  occurrence_count: number;
  file_count: number;
  example_count: number;
  position_summary: PositionSummary;
  risk_label: string;
  content_class: string;
  raw_variant_count: number;
  raw_variant_count_is_capped: boolean;
  raw_variants: string[];
  examples: RepeatedArtifactExample[];
}

interface RepeatedArtifactScanDiagnostics {
  files_requested: number;
  files_scanned: number;
  files_failed_extraction: number;
  files_empty_after_extraction: number;
  total_raw_lines: number;
  total_candidate_keys_before_filtering: number;
  candidates_after_min_occurrences: number;
  candidates_after_min_files: number;
  final_candidates: number;
  analysed_processed_text: boolean;
  custom_removals_active: number;
  max_examples_per_candidate: number;
}

interface RepeatedArtifactScanReport {
  candidates: RepeatedArtifactCandidate[];
  diagnostics: RepeatedArtifactScanDiagnostics;
}

let lastScanCandidates: RepeatedArtifactCandidate[] = [];
let selectedCandidateIds: Set<string> = new Set();
/// Tracks scan state for zero-result explanation
let scanWasProcessed = false;
let removalCountAtScanStart = 0;

function initRepeatedArtifactFinder() {
  const menuRepeatedArtifactFinder = document.getElementById("menu-repeated-artifact-finder") as HTMLDivElement;
  const repeatedArtifactModal = document.getElementById("repeated-artifact-modal") as HTMLDivElement;
  const btnCloseArtifactModal = document.getElementById("btn-close-artifact-modal") as HTMLButtonElement;
  const btnRunArtifactScan = document.getElementById("btn-run-artifact-scan") as HTMLButtonElement;
  const btnCancelScan = document.getElementById("btn-cancel-artifact-scan") as HTMLButtonElement;
  const tblArtifactCandidates = document.getElementById("tbl-artifact-candidates") as HTMLTableSectionElement;
  const lblArtifactResultsCount = document.getElementById("lbl-artifact-results-count") as HTMLSpanElement;
  const artifactDetailsContent = document.getElementById("artifact-details-content") as HTMLDivElement;
  const lblScanTime = document.getElementById("lbl-artifact-scan-time") as HTMLSpanElement;

  const btnAddSelectedRemovals = document.getElementById("btn-add-selected-removals") as HTMLButtonElement;
  const lblArtifactAddStatus = document.getElementById("lbl-artifact-add-status") as HTMLSpanElement;
  const artifactProcessedWarning = document.getElementById("artifact-processed-warning") as HTMLDivElement;
  const artifactDiagnostics = document.getElementById("artifact-scan-diagnostics") as HTMLDivElement;

  const chkArtifactExact = document.getElementById("chk-artifact-exact") as HTMLInputElement;
  const chkArtifactNorm = document.getElementById("chk-artifact-norm") as HTMLInputElement;
  const chkArtifactInline = document.getElementById("chk-artifact-inline") as HTMLInputElement;
  const chkArtifact2Line = document.getElementById("chk-artifact-2line") as HTMLInputElement;
  const chkArtifact3Line = document.getElementById("chk-artifact-3line") as HTMLInputElement;

  const chkArtifactText = document.getElementById("chk-artifact-text") as HTMLInputElement;
  const chkArtifactMixed = document.getElementById("chk-artifact-mixed") as HTMLInputElement;
  const chkArtifactNumeric = document.getElementById("chk-artifact-numeric") as HTMLInputElement;
  const chkArtifactSymbol = document.getElementById("chk-artifact-symbol") as HTMLInputElement;

  const numArtifactMinOcc = document.getElementById("num-artifact-min-occ") as HTMLInputElement;
  const numArtifactMinFiles = document.getElementById("num-artifact-min-files") as HTMLInputElement;
  const numArtifactMaxCand = document.getElementById("num-artifact-max-cand") as HTMLInputElement;
  const numArtifactMaxExamples = document.getElementById("num-artifact-max-examples") as HTMLInputElement;

  const radioModes = document.querySelectorAll('input[name="artifact-text-mode"]') as NodeListOf<HTMLInputElement>;

  if (!menuRepeatedArtifactFinder || !repeatedArtifactModal) return;

  // Track scan state for cancellation and stale-result prevention
  let scanGeneration = 0;
  let scanTimerInterval: ReturnType<typeof setInterval> | null = null;
  let currentAbortController: AbortController | null = null;

  function updateProcessedWarning() {
    const isProcessed = (document.querySelector('input[name="artifact-text-mode"]:checked') as HTMLInputElement)?.value === "processed";
    if (isProcessed && activeCleaningConfig.remove_patterns.length > 0) {
      const removals = activeCleaningConfig.remove_patterns;
      const count = removals.length;
      let previewText = `Processed scans apply ${count} active Custom Removal sequence(s).`;
      if (count > 0) {
        const examples = removals.slice(0, 3);
        previewText += ` Active removals include: ${examples.join(", ")}${count > 3 ? ", ..." : ""}.`;
      }
      artifactProcessedWarning.textContent = previewText;
      artifactProcessedWarning.style.display = "block";
    } else {
      artifactProcessedWarning.style.display = "none";
    }
  }

  function setStatus(stage: string, elapsed?: number) {
    const timeStr = elapsed !== undefined ? ` (${elapsed.toFixed(1)}s)` : "";
    tblArtifactCandidates.innerHTML = `
      <tr>
        <td colspan="8" style="padding: 20px; text-align: center;">
          <div class="spinner" style="margin: 0 auto 10px auto; width: 24px; height: 24px;"></div>
          ${stage}${timeStr}
        </td>
      </tr>
    `;
    artifactDetailsContent.innerHTML = `<div style="color: var(--text-muted); text-align: center; margin-top: 40px;">${stage}...</div>`;
  }

  function resetScanControls() {
    btnRunArtifactScan.style.display = "inline-block";
    btnRunArtifactScan.disabled = false;
    btnRunArtifactScan.textContent = "Run Scan";
    btnCancelScan.style.display = "none";
    if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }
  }

  function updateAddSelectedButtonState() {
    btnAddSelectedRemovals.disabled = selectedCandidateIds.size === 0;
  }

  function renderDiagnostics(diags: RepeatedArtifactScanDiagnostics) {
    let parts: string[] = [];
    parts.push(`Files: ${diags.files_scanned}/${diags.files_requested} scanned`);
    parts.push(`Lines: ${diags.total_raw_lines.toLocaleString()}`);
    parts.push(`Candidate keys: ${diags.total_candidate_keys_before_filtering}`);
    parts.push(`After occurrence filter: ${diags.candidates_after_min_occurrences}`);
    parts.push(`After file filter: ${diags.candidates_after_min_files}`);
    parts.push(`Final: ${diags.final_candidates}`);

    const failureParts: string[] = [];
    if (diags.files_failed_extraction > 0) {
      failureParts.push(`Extraction failures: ${diags.files_failed_extraction}`);
    }
    if (diags.files_empty_after_extraction > 0) {
      failureParts.push(`Empty extractions: ${diags.files_empty_after_extraction}`);
    }

    let text = parts.join(" · ");
    if (failureParts.length > 0) {
      text += " · " + failureParts.join(" · ");
    }

    if (diags.analysed_processed_text && diags.custom_removals_active > 0) {
      text += ` · Processed scan applied ${diags.custom_removals_active} active Custom Removal sequence(s).`;
    }

    artifactDiagnostics.textContent = text;
    artifactDiagnostics.classList.remove("hidden");
  }

  menuRepeatedArtifactFinder.addEventListener("click", () => {
    repeatedArtifactModal.classList.remove("hidden");
    updateProcessedWarning();
  });

  function closeArtifactModal(): void {
    // Cancel any running scan
    scanGeneration++;
    if (currentAbortController) { currentAbortController.abort(); currentAbortController = null; }
    invoke("cancel_repeated_artifacts_command").catch(() => {});
    resetScanControls();
    lblScanTime.style.display = "none";
    selectedCandidateIds.clear();
    artifactDiagnostics.classList.add("hidden");
    repeatedArtifactModal.classList.add("hidden");
  }

  btnCloseArtifactModal.addEventListener("click", closeArtifactModal);

  const btnCloseArtifactModalTop = document.getElementById("btn-close-artifact-modal-top") as HTMLButtonElement;
  if (btnCloseArtifactModalTop) {
    btnCloseArtifactModalTop.addEventListener("click", closeArtifactModal);
  }

  btnAddSelectedRemovals.addEventListener("click", () => {
    const count = selectedCandidateIds.size;
    if (count === 0) return;

    const existingSet = new Set(activeCleaningConfig.remove_patterns);
    let addedCount = 0;
    let skippedDuplicates = 0;
    let groupedCandidatesExpanded = 0;

    for (const id of selectedCandidateIds) {
      const cand = lastScanCandidates.find(c => c.candidate_id === id);
      if (!cand) continue;

      const isNormalized = cand.kind === "normalized_line";

      if (isNormalized) {
        // Grouped candidate: add each raw variant string individually
        if (!cand.raw_variants || cand.raw_variants.length === 0) continue;
        groupedCandidatesExpanded++;
        for (const variant of cand.raw_variants) {
          if (!existingSet.has(variant)) {
            activeCleaningConfig.remove_patterns.push(variant);
            existingSet.add(variant);
            addedCount++;
          } else {
            skippedDuplicates++;
          }
        }
      } else {
        // Literal candidate: add display_text as before
        const text = cand.display_text;
        if (!existingSet.has(text)) {
          activeCleaningConfig.remove_patterns.push(text);
          existingSet.add(text);
          addedCount++;
        } else {
          skippedDuplicates++;
        }
      }
    }

    if (addedCount > 0) {
      tempRemovePatterns = [...activeCleaningConfig.remove_patterns];
      renderCustomRemovals();
    }

    // Build informative status message
    let statusParts: string[] = [];
    statusParts.push(`Added ${addedCount} sequence${addedCount === 1 ? "" : "s"} to Custom Removals`);
    if (groupedCandidatesExpanded > 0) {
      statusParts.push(`(${groupedCandidatesExpanded} grouped candidate${groupedCandidatesExpanded === 1 ? "" : "s"} expanded)`);
    }
    if (skippedDuplicates > 0) {
      statusParts.push(`${skippedDuplicates} duplicate${skippedDuplicates === 1 ? "" : "s"} skipped`);
    }
    lblArtifactAddStatus.textContent = statusParts.join(". ") + ".";
    setTimeout(() => { lblArtifactAddStatus.textContent = ""; }, 5000);

    // Update processed warning
    updateProcessedWarning();

    // Refresh preview if files are selected
    if (selectedCorpusIndices.size > 0) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(updatePreview, 150);
    }
  });

  btnCancelScan.addEventListener("click", () => {
    // Cancel via Tauri managed state
    invoke("cancel_repeated_artifacts_command").catch((e) => console.error(e));
    btnCancelScan.disabled = true;
    btnCancelScan.textContent = "Cancelling...";
  });

  btnRunArtifactScan.addEventListener("click", async () => {
    if (allFiles.length === 0) {
      alert("No files loaded in the corpus. Please open a directory or load files first.");
      return;
    }

    const textMode = (document.querySelector('input[name="artifact-text-mode"]:checked') as HTMLInputElement)?.value || "original";
    const analyseProcessed = textMode === "processed";

    // Capture scan state at start
    scanWasProcessed = analyseProcessed;
    removalCountAtScanStart = activeCleaningConfig.remove_patterns.length;

    const config: RepeatedArtifactScanConfig = {
      analyse_processed_text: analyseProcessed,
      include_exact_lines: chkArtifactExact.checked,
      include_normalized_lines: chkArtifactNorm.checked,
      include_inline_artifacts: chkArtifactInline.checked,
      include_two_line_blocks: chkArtifact2Line.checked,
      include_three_line_blocks: chkArtifact3Line.checked,
      include_text_dominant: chkArtifactText.checked,
      include_mixed_text_numbers: chkArtifactMixed.checked,
      include_numeric_dominant: chkArtifactNumeric.checked,
      include_symbol_noise: chkArtifactSymbol.checked,
      min_occurrences: parseInt(numArtifactMinOcc.value, 10) || 5,
      min_files: parseInt(numArtifactMinFiles.value, 10) || 1,
      max_candidates: parseInt(numArtifactMaxCand.value, 10) || 100,
      max_examples_per_candidate: Math.min(Math.max(parseInt(numArtifactMaxExamples.value, 10) || 25, 1), 100),
      min_line_chars: 4,
      max_line_chars: 300,
    };

    // Reset cancellation flag, show Cancel button
    btnRunArtifactScan.style.display = "none";
    btnCancelScan.style.display = "inline-block";
    btnCancelScan.disabled = false;
    btnCancelScan.textContent = "Cancel";
    lblScanTime.style.display = "inline";
    lblScanTime.textContent = "0.0s";

    // Track this scan generation
    const myGen = ++scanGeneration;
    const startTime = performance.now();

    // Status stages
    setStatus("Preparing files...", 0);

    if (scanTimerInterval) clearInterval(scanTimerInterval);
    scanTimerInterval = setInterval(() => {
      const elapsed = (performance.now() - startTime) / 1000;
      lblScanTime.textContent = `${elapsed.toFixed(1)}s`;
      // Update status based on elapsed time
      if (elapsed < 2) {
        setStatus("Preparing files...", elapsed);
      } else if (elapsed < 3) {
        setStatus("Extracting text...", elapsed);
      } else if (elapsed < 8) {
        setStatus("Scanning lines...", elapsed);
      } else if (elapsed < 30) {
        setStatus("Merging candidates...", elapsed);
      } else {
        setStatus("Ranking candidates...", elapsed);
      }
    }, 500);

    const myVersion = currentCorpusVersion;
    const indices = Array.from(
      { length: allFiles.length },
      (_, corpusIndex) => corpusIndex
    );

    try {
      const report = await invoke<RepeatedArtifactScanReport>("scan_repeated_artifacts_command", {
        indices,
        corpusVersion: myVersion,
        config: config,
        cleaningConfig: activeCleaningConfig,
      });

      if (myVersion !== currentCorpusVersion) return;

      // Discard stale results or check for cancellation
      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }

      const totalElapsed = (performance.now() - startTime) / 1000;
      lblScanTime.textContent = `Done in ${totalElapsed.toFixed(1)}s`;

      lastScanCandidates = report.candidates;

      // Render diagnostics
      renderDiagnostics(report.diagnostics);

      // Clear stale detail panel
      artifactDetailsContent.innerHTML = `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">Select a candidate to inspect examples.</div>`;

      renderCandidates(report.candidates, report.diagnostics);
    } catch (err) {
      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }

      const errStr = String(err);
      // Check for cancellation
      if (errStr.includes("cancelled") || errStr.includes("Cancelled") || errStr.includes("cancel")) {
        lblScanTime.textContent = "Cancelled";
        tblArtifactCandidates.innerHTML = `
          <tr>
            <td colspan="8" style="padding: 20px; text-align: center; color: var(--text-muted);">Scan was cancelled.</td>
          </tr>
        `;
      } else {
        lblScanTime.textContent = "Error";
        console.error(err);
        tblArtifactCandidates.innerHTML = "";
        const errorRow = document.createElement("tr");
        const errorCell = document.createElement("td");
        errorCell.colSpan = 8;
        errorCell.style.cssText = "padding: 20px; text-align: center; color: #ff5e5e;";
        errorCell.textContent = `Error during scan: ${errStr}`;
        errorRow.appendChild(errorCell);
        tblArtifactCandidates.appendChild(errorRow);
      }

    } finally {
      if (myGen !== scanGeneration) return;
      resetScanControls();
    }
  });

  // Listen for radio changes to update the warning
  radioModes.forEach(radio => {
    radio.addEventListener("change", updateProcessedWarning);
  });

  function renderCandidates(candidates: RepeatedArtifactCandidate[], diagnostics: RepeatedArtifactScanDiagnostics | null) {
    selectedCandidateIds.clear();
    updateAddSelectedButtonState();

    tblArtifactCandidates.innerHTML = "";
    lblArtifactResultsCount.textContent = `${candidates.length} found`;

    if (candidates.length === 0) {
      let msg: string;

      if (diagnostics && diagnostics.total_raw_lines === 0) {
        msg = "No candidates found because no text lines were extracted. Check whether the documents are scanned/image-only PDFs or whether extraction failed.";
        if (diagnostics.files_failed_extraction > 0) {
          msg += ` Extraction failures were reported for ${diagnostics.files_failed_extraction} file(s).`;
        }
      } else if (diagnostics && diagnostics.total_candidate_keys_before_filtering > 0 && diagnostics.candidates_after_min_occurrences === 0) {
        msg = "Candidate keys were found, but none met Min Occurrences. Try lowering Min Occurrences.";
      } else if (diagnostics && diagnostics.candidates_after_min_occurrences > 0 && diagnostics.candidates_after_min_files === 0) {
        msg = "Candidates were found, but none appeared in enough distinct files. Try Min Files = 1 for repeated artefacts inside a single PDF/book.";
      } else {
        msg = "No candidates found meeting the thresholds.";
      }

      if (scanWasProcessed && removalCountAtScanStart > 0) {
        msg += " Processed scans apply current Custom Removals, so already-removed artefacts will not appear. Scan Original extracted text to rediscover raw artefacts.";
      }

      tblArtifactCandidates.innerHTML = `
        <tr>
          <td colspan="8" style="padding: 20px; text-align: center; color: var(--text-muted);">${msg}</td>
        </tr>
      `;
      return;
    }

    candidates.forEach((cand) => {
      const tr = document.createElement("tr");
      tr.dataset.id = cand.candidate_id;

      // Checkbox column
      const tdCheck = document.createElement("td");
      tdCheck.style.padding = "6px 8px";
      tdCheck.style.textAlign = "center";
      const chk = document.createElement("input");
      chk.type = "checkbox";
      const isNormalized = cand.kind === "normalized_line";
      if (isNormalized && (!cand.raw_variants || cand.raw_variants.length === 0)) {
        chk.disabled = true;
        chk.title = "This grouped pattern has no actionable raw variants to add.";
      } else if (isNormalized) {
        const cappedNote = cand.raw_variant_count_is_capped ? ` (${cand.raw_variant_count}+ known, may be incomplete)` : "";
        chk.title = `Selecting adds all ${cand.raw_variants.length} exact raw variant${cand.raw_variants.length === 1 ? "" : "s"} to Custom Removals${cappedNote}.`;
      }
      chk.addEventListener("click", (e) => {
        e.stopPropagation();
        if (chk.checked) {
          selectedCandidateIds.add(cand.candidate_id);
        } else {
          selectedCandidateIds.delete(cand.candidate_id);
        }
        updateAddSelectedButtonState();
      });
      tdCheck.appendChild(chk);

      const tdText = document.createElement("td");
      tdText.className = "candidate-text-cell";
      tdText.textContent = cand.normalized_key && cand.kind === "normalized_line"
        ? cand.normalized_key
        : cand.display_text;
      tdText.title = cand.display_text;

      const tdKind = document.createElement("td");
      tdKind.textContent = formatKind(cand.kind);

      const tdContent = document.createElement("td");
      tdContent.textContent = formatContentClass(cand.content_class);
      tdContent.style.fontSize = "0.8rem";

      const tdOcc = document.createElement("td");
      tdOcc.textContent = cand.occurrence_count.toString();
      tdOcc.style.textAlign = "right";

      const tdFiles = document.createElement("td");
      tdFiles.textContent = cand.file_count.toString();
      tdFiles.style.textAlign = "right";

      const tdPos = document.createElement("td");
      tdPos.textContent = formatPosition(cand.position_summary);
      tdPos.style.fontSize = "0.8rem";

      const tdRisk = document.createElement("td");
      const spanBadge = document.createElement("span");
      spanBadge.className = `risk-badge ${cand.risk_label}`;
      spanBadge.textContent = formatRiskLabel(cand.risk_label);
      tdRisk.appendChild(spanBadge);

      tr.appendChild(tdCheck);
      tr.appendChild(tdText);
      tr.appendChild(tdKind);
      tr.appendChild(tdContent);
      tr.appendChild(tdOcc);
      tr.appendChild(tdFiles);
      tr.appendChild(tdPos);
      tr.appendChild(tdRisk);

      tr.onclick = () => {
        tblArtifactCandidates.querySelectorAll("tr").forEach((r) => r.classList.remove("selected"));
        tr.classList.add("selected");
        artifactDetailsContent.innerHTML = "";
        showCandidateDetails(cand);
      };

      tblArtifactCandidates.appendChild(tr);
    });

    // Show a note if dedup removed normalized variants
    const noteContainer = tblArtifactCandidates.closest(".repeated-artifact-table-container");
    if (noteContainer) {
      const existingNote = noteContainer.querySelector(".artifact-dedup-note");
      if (existingNote) existingNote.remove();
    }
  }

  function formatKind(kind: string): string {
    switch (kind) {
      case "exact_line": return "Exact";
      case "normalized_line": return "Pattern";
      case "two_line_block": return "2-Block";
      case "three_line_block": return "3-Block";
      case "inline_artifact": return "Inline";
      default: return kind;
    }
  }

  function formatContentClass(cls: string): string {
    switch (cls) {
      case "text_dominant": return "Text";
      case "mixed_text_numbers": return "Mixed";
      case "numeric_dominant": return "Numeric";
      case "symbol_noise_dominant": return "Symbol";
      default: return cls;
    }
  }

  function formatPosition(ps: PositionSummary): string {
    const total = ps.top_count + ps.middle_count + ps.bottom_count;
    if (total === 0) return "Unknown";
    const topPct = Math.round((ps.top_count / total) * 100);
    const botPct = Math.round((ps.bottom_count / total) * 100);
    return `${topPct}% / ${botPct}%`;
  }

  function formatRiskLabel(label: string): string {
    switch (label) {
      case "strong_header_footer_candidate": return "Header/Footer";
      case "possible_boilerplate": return "Boilerplate";
      case "common_section_heading_review_carefully": return "Heading";
      case "symbol_or_noise_candidate": return "Noise";
      case "ambiguous": return "Review";
      default: return label;
    }
  }

  function showCandidateDetails(cand: RepeatedArtifactCandidate) {
    artifactDetailsContent.innerHTML = "";

    const isLiteral = cand.kind === "exact_line" ||
      cand.kind === "inline_artifact" ||
      cand.kind === "two_line_block" ||
      cand.kind === "three_line_block";

    const isNormalized = cand.kind === "normalized_line";

    // --- Metadata summary box (always shown) ---
    const metaDiv = document.createElement("div");
    metaDiv.style.display = "flex";
    metaDiv.style.flexWrap = "wrap";
    metaDiv.style.gap = "12px";
    metaDiv.style.fontSize = "0.85rem";
    metaDiv.style.padding = "8px";
    metaDiv.style.background = "var(--bg-color)";
    metaDiv.style.borderRadius = "4px";
    metaDiv.style.border = "1px solid var(--border-color)";

    let rawVariantsText: string;
    if (cand.raw_variant_count_is_capped) {
      rawVariantsText = `${cand.raw_variant_count}+`;
    } else {
      rawVariantsText = String(cand.raw_variant_count);
    }

    metaDiv.innerHTML = `
      <span><strong>Kind:</strong> ${formatKind(cand.kind)}</span>
      <span><strong>Content:</strong> ${formatContentClass(cand.content_class)}</span>
      <span><strong>Occurrences:</strong> ${cand.occurrence_count}</span>
      <span><strong>Files:</strong> ${cand.file_count}</span>
      <span><strong>Raw variants:</strong> ${rawVariantsText}</span>
      <span><strong>Risk:</strong> ${formatRiskLabel(cand.risk_label)}</span>
    `;
    artifactDetailsContent.appendChild(metaDiv);

    // --- Numeric dominance caution ---
    if (cand.content_class === "numeric_dominant") {
      const numericCaution = document.createElement("div");
      numericCaution.style.fontSize = "0.8rem";
      numericCaution.style.color = "#e8a000";
      numericCaution.style.background = "rgba(232, 160, 0, 0.08)";
      numericCaution.style.padding = "6px 8px";
      numericCaution.style.borderRadius = "4px";
      numericCaution.style.borderLeft = "3px solid #e8a000";
      numericCaution.textContent = "Numeric-dominant candidate — review carefully. These may group unrelated tables, formulas, axis ticks, or statistical output.";
      artifactDetailsContent.appendChild(numericCaution);
    }

    // --- Literal candidate display block (only for literal kinds) ---
    if (isLiteral) {
      const dispBlock = document.createElement("div");
      dispBlock.className = "candidate-display-block";
      const dispHeader = document.createElement("div");
      dispHeader.className = "candidate-display-header";
      dispHeader.textContent = "Candidate";
      const dispText = document.createElement("div");
      dispText.className = "candidate-display-text";
      dispText.textContent = cand.display_text;
      dispBlock.appendChild(dispHeader);
      dispBlock.appendChild(dispText);
      artifactDetailsContent.appendChild(dispBlock);

      // Copy Candidate button for literal candidates
      const copyBtn = document.createElement("button");
      copyBtn.className = "btn-copy-candidate";
      copyBtn.textContent = "Copy Candidate";
      copyBtn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(cand.display_text);
          copyBtn.textContent = "Copied";
          setTimeout(() => { copyBtn.textContent = "Copy Candidate"; }, 2000);
        } catch (_) {
          copyBtn.textContent = "Copy failed";
        }
      });
      artifactDetailsContent.appendChild(copyBtn);
    }

    // --- Normalised candidate grouping key display block ---
    if (isNormalized) {
      const dispBlock = document.createElement("div");
      dispBlock.className = "candidate-display-block";
      const dispHeader = document.createElement("div");
      dispHeader.className = "candidate-display-header";
      dispHeader.textContent = "Normalised grouping key";
      const dispText = document.createElement("div");
      dispText.className = "candidate-display-text";
      dispText.textContent = cand.normalized_key || cand.display_text;
      dispBlock.appendChild(dispHeader);
      dispBlock.appendChild(dispText);
      artifactDetailsContent.appendChild(dispBlock);

      // Note: no copy button for normalized candidates
      const normNote = document.createElement("div");
      normNote.className = "detail-note detail-note-info";
      normNote.textContent = "This is a grouping pattern, not a literal removal string. Use the sample occurrences to inspect exact variants.";
      artifactDetailsContent.appendChild(normNote);

      // --- Show actionable raw variants for grouped candidates ---
      if (cand.raw_variants && cand.raw_variants.length > 0) {
        const variantsActionDiv = document.createElement("div");
        variantsActionDiv.style.fontSize = "0.85rem";
        variantsActionDiv.style.padding = "8px";
        variantsActionDiv.style.background = "rgba(80, 200, 120, 0.06)";
        variantsActionDiv.style.borderRadius = "4px";
        variantsActionDiv.style.borderLeft = "3px solid #50c878";

        let variantsLabel = `Selecting this candidate adds all ${cand.raw_variants.length} exact raw variant${cand.raw_variants.length === 1 ? "" : "s"} to Custom Removals`;
        if (cand.raw_variant_count_is_capped) {
          variantsLabel += ` (tracking capped at ${cand.raw_variants.length} — the actual count is ${cand.raw_variant_count}+; some rare variants may not be included)`;
        }
        variantsLabel += ".";
        const variantsLabelP = document.createElement("div");
        variantsLabelP.textContent = variantsLabel;
        variantsLabelP.style.marginBottom = "6px";
        variantsActionDiv.appendChild(variantsLabelP);

        // Toggle to show/hide the variant list
        const toggleBtn = document.createElement("button");
        toggleBtn.textContent = "Show raw variants";
        toggleBtn.style.cssText = "font-size: 0.8rem; padding: 3px 10px; border-radius: 3px; cursor: pointer; background: var(--bg-secondary); color: var(--text-color); border: 1px solid var(--border-color);";
        variantsActionDiv.appendChild(toggleBtn);

        const variantsList = document.createElement("div");
        variantsList.style.display = "none";
        variantsList.style.marginTop = "6px";
        variantsList.style.maxHeight = "200px";
        variantsList.style.overflowY = "auto";
        variantsList.style.fontFamily = "monospace";
        variantsList.style.fontSize = "0.8rem";
        variantsList.style.lineHeight = "1.5";

        for (const variant of cand.raw_variants) {
          const varLine = document.createElement("div");
          varLine.style.padding = "2px 4px";
          varLine.style.borderBottom = "1px solid var(--border-color)";
          varLine.style.whiteSpace = "pre-wrap";
          varLine.style.wordBreak = "break-all";
          varLine.textContent = variant;
          variantsList.appendChild(varLine);
        }
        variantsActionDiv.appendChild(variantsList);

        toggleBtn.addEventListener("click", () => {
          if (variantsList.style.display === "none") {
            variantsList.style.display = "block";
            toggleBtn.textContent = "Hide raw variants";
          } else {
            variantsList.style.display = "none";
            toggleBtn.textContent = "Show raw variants";
          }
        });

        artifactDetailsContent.appendChild(variantsActionDiv);
      } else {
        const noVariantsNote = document.createElement("div");
        noVariantsNote.className = "detail-note detail-note-warning";
        noVariantsNote.textContent = "No raw variants were tracked for this candidate. It cannot be added to Custom Removals.";
        artifactDetailsContent.appendChild(noVariantsNote);
      }
    }

    // --- Position summary ---
    if (cand.position_summary) {
      const posSummaryDiv = document.createElement("div");
      posSummaryDiv.style.fontSize = "0.85rem";
      posSummaryDiv.style.display = "flex";
      posSummaryDiv.style.flexDirection = "column";
      posSummaryDiv.style.gap = "4px";
      posSummaryDiv.style.background = "var(--bg-color)";
      posSummaryDiv.style.padding = "8px";
      posSummaryDiv.style.borderRadius = "4px";
      posSummaryDiv.style.border = "1px solid var(--border-color)";

      const total = cand.position_summary.top_count + cand.position_summary.middle_count + cand.position_summary.bottom_count;
      const topPct = total > 0 ? Math.round((cand.position_summary.top_count / total) * 100) : 0;
      const midPct = total > 0 ? Math.round((cand.position_summary.middle_count / total) * 100) : 0;
      const botPct = total > 0 ? Math.round((cand.position_summary.bottom_count / total) * 100) : 0;

      posSummaryDiv.innerHTML = `
        <strong>Estimated Layout Positions (Approximate):</strong>
        <div style="display: flex; justify-content: space-between; margin-top: 4px;">
          <span>Top: ${cand.position_summary.top_count} (${topPct}%)</span>
          <span>Body: ${cand.position_summary.middle_count} (${midPct}%)</span>
          <span>Bottom: ${cand.position_summary.bottom_count} (${botPct}%)</span>
        </div>
      `;
      artifactDetailsContent.appendChild(posSummaryDiv);
    }

    // --- Risk advisory ---
    const riskAdvisoryDiv = document.createElement("div");
    riskAdvisoryDiv.style.fontSize = "0.8rem";
    riskAdvisoryDiv.style.color = "var(--text-muted)";
    riskAdvisoryDiv.style.lineHeight = "1.4";
    riskAdvisoryDiv.style.padding = "6px 8px";
    riskAdvisoryDiv.style.background = "rgba(255, 255, 255, 0.02)";
    riskAdvisoryDiv.style.borderLeft = "2px solid var(--border-color)";

    let advisoryText = "";
    switch (cand.risk_label) {
      case "strong_header_footer_candidate":
        advisoryText = "Notice (Advisory Only): This sequence is heavily concentrated at page headers or footers. It is a very strong candidate for cleanup.";
        break;
      case "possible_boilerplate":
        advisoryText = "Notice (Advisory Only): This sequence occurs across multiple documents in similar forms, suggesting boilerplate (e.g. copyright notices or publisher headers).";
        break;
      case "common_section_heading_review_carefully":
        advisoryText = "Notice (Advisory Only): This matches common academic sections (e.g. Introduction). Review carefully as removing this may alter document structure.";
        break;
      case "symbol_or_noise_candidate":
        advisoryText = "Notice (Advisory Only): This contains a high proportion of non-alphanumeric symbols. It is highly likely to be extraction noise or page dividers.";
        break;
      default:
        advisoryText = "Notice (Advisory Only): Classification is ambiguous. Review occurrences below to check if it represents noise or content.";
    }
    riskAdvisoryDiv.textContent = advisoryText;
    artifactDetailsContent.appendChild(riskAdvisoryDiv);

    // --- Sample occurrences ---
    if (cand.examples && cand.examples.length > 0) {
      const examplesTitle = document.createElement("strong");
      examplesTitle.style.fontSize = "0.9rem";
      examplesTitle.style.marginTop = "8px";
      if (cand.occurrence_count > cand.examples.length) {
        examplesTitle.textContent = `Sample Occurrences (showing ${cand.examples.length} of ${cand.occurrence_count})`;
      } else {
        examplesTitle.textContent = `Sample Occurrences (showing all ${cand.examples.length})`;
      }

      const examplesListDiv = document.createElement("div");
      examplesListDiv.style.display = "flex";
      examplesListDiv.style.flexDirection = "column";
      examplesListDiv.style.gap = "8px";

      cand.examples.forEach((ex, idx) => {
        const card = document.createElement("div");
        card.className = "artifact-example-card";

        const cardHeader = document.createElement("div");
        cardHeader.className = "artifact-example-header";

        const docSpan = document.createElement("span");
        docSpan.textContent = `Instance ${idx + 1}: ${ex.file_name}`;
        docSpan.style.fontWeight = "600";
        docSpan.style.overflow = "hidden";
        docSpan.style.textOverflow = "ellipsis";
        docSpan.title = ex.file_path;

        const posSpan = document.createElement("span");
        const pageStr = ex.page_number !== null && ex.page_number !== undefined ? `Page ~${ex.page_number} (Approx)` : "";
        const lineStr = ex.line_number !== null && ex.line_number !== undefined ? `Line ${ex.line_number}` : "";
        posSpan.textContent = [pageStr, lineStr].filter(Boolean).join(", ");

        cardHeader.appendChild(docSpan);
        cardHeader.appendChild(posSpan);

        const cardBody = document.createElement("div");
        cardBody.className = "artifact-example-body";

        if (ex.context_before) {
          const lineBefore = document.createElement("div");
          lineBefore.className = "artifact-context-line";
          lineBefore.textContent = ex.context_before;
          cardBody.appendChild(lineBefore);
        }

        const lineMatched = document.createElement("div");
        lineMatched.className = "artifact-matched-line";
        lineMatched.textContent = ex.matched_text;
        cardBody.appendChild(lineMatched);

        if (ex.context_after) {
          const lineAfter = document.createElement("div");
          lineAfter.className = "artifact-context-line";
          lineAfter.textContent = ex.context_after;
          cardBody.appendChild(lineAfter);
        }

        card.appendChild(cardHeader);
        card.appendChild(cardBody);
        examplesListDiv.appendChild(card);
      });

      artifactDetailsContent.appendChild(examplesTitle);
      artifactDetailsContent.appendChild(examplesListDiv);
    }
  }

  // v1 is diagnostic only — no automatic deletion.
}

document.addEventListener('DOMContentLoaded', init);
