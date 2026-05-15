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
