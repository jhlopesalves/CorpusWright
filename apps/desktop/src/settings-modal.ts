import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

import {
  BOOLEAN_CLEANING_CONFIG_KEYS,
  normaliseCleaningConfig,
  type BooleanCleaningConfigKey,
} from "./config";
import { dom } from "./dom";
import { renderCustomRemovals, syncDraftCustomRemovalsFromConfig } from "./custom-removals";
import { state } from "./state";
import type {
  CleaningConfig,
  PdfEmbeddedTextStrategy,
  PdfOcrQuality,
  PdfTextSource,
  TableExtractionStrategy,
} from "./generated/CleaningConfig.js";

interface CheckboxBinding {
  configKey: BooleanCleaningConfigKey;
  element: HTMLInputElement;
}

const cleaningCheckboxBindings: CheckboxBinding[] = [
  { configKey: "join_line_breaks",                           element: dom.chkJoinLineBreaks },
  { configKey: "normalize_irregular_line_breaks",            element: dom.chkNormalizeIrregularLineBreaks },
  { configKey: "remove_standalone_page_numbers",             element: dom.chkRemoveStandalonePageNumbers },
  { configKey: "remove_standalone_roman_page_numbers",       element: dom.chkRemoveStandaloneRomanPageNumbers },
  { configKey: "remove_page_indicators",                     element: dom.chkRemovePageIndicators },
  { configKey: "remove_page_delimiters",                     element: dom.chkRemovePageDelimiters },
  { configKey: "lowercase",                                  element: dom.chkLowercase },
  { configKey: "normalize_line_endings",                     element: dom.chkNormalize },
  { configKey: "trim_lines",                                 element: dom.chkTrim },
  { configKey: "collapse_blank_lines",                       element: dom.chkCollapse },
  { configKey: "normalize_unicode",                          element: dom.chkNormalizeUnicode },
  { configKey: "replace_diacritics",                         element: dom.chkReplaceDiacritics },
  { configKey: "extract_html",                               element: dom.chkExtractHtml },
  { configKey: "remove_headers",                             element: dom.chkRemoveHeaders },
  { configKey: "remove_footers",                             element: dom.chkRemoveFooters },
  { configKey: "remove_footnotes",                           element: dom.chkRemoveFootnotes },
  { configKey: "remove_endnotes",                            element: dom.chkRemoveEndnotes },
  { configKey: "remove_comments",                            element: dom.chkRemoveComments },
  { configKey: "remove_table_of_contents",                   element: dom.chkRemoveToc },
  { configKey: "remove_repeated_pdf_headers_footers",        element: dom.chkRemoveRepeatedPdfHeadersFooters },
  { configKey: "remove_pdf_page_labels",                     element: dom.chkRemovePdfPageLabels },
  { configKey: "remove_pdf_symbol_heavy_artifacts",          element: dom.chkRemovePdfSymbolHeavyArtifacts },
  { configKey: "remove_pdf_code_like_blocks",                element: dom.chkRemovePdfCodeLikeBlocks },
  { configKey: "remove_pdf_formula_like_lines",              element: dom.chkRemovePdfFormulaLikeLines },
];

interface SettingsModalCallbacks {
  updateWordCount: () => void;
  schedulePreviewUpdate: (delay: number) => void;
}

export function syncCheckboxesFromConfig(config: CleaningConfig): void {
  for (const { configKey, element } of cleaningCheckboxBindings) {
    element.checked = config[configKey];
  }
}

function readCheckboxesIntoConfig(config: CleaningConfig): void {
  for (const { configKey, element } of cleaningCheckboxBindings) {
    config[configKey] = element.checked;
  }
}

function setModalConfigStatus(message: string): void {
  dom.modalConfigStatus.textContent = message;
}

function buildConfigFromModalControls(): CleaningConfig {
  const config = { ...state.activeCleaningConfig };
  for (const key of BOOLEAN_CLEANING_CONFIG_KEYS) {
    config[key] = false;
  }
  readCheckboxesIntoConfig(config);
  config.table_extraction_strategy = dom.selTableExtraction.value as TableExtractionStrategy;
  config.pdf_text_source = dom.selPdfTextSource.value as PdfTextSource;
  config.pdf_ocr_quality = dom.selPdfOcrQuality.value as PdfOcrQuality;
  config.pdf_embedded_text_strategy = dom.selPdfEmbeddedTextStrategy.value as PdfEmbeddedTextStrategy;
  config.remove_patterns = [...state.tempRemovePatterns];
  config.removal_rules = state.tempRemovalRules.map((rule) => ({
    ...rule,
    matcher: { ...rule.matcher },
  }));
  config.replace_patterns = [...state.tempReplacePatterns];
  return config;
}

function syncModalControlsFromConfig(config: CleaningConfig): void {
  syncCheckboxesFromConfig(config);
  dom.selTableExtraction.value = config.table_extraction_strategy;
  dom.selPdfTextSource.value = config.pdf_text_source;
  dom.selPdfOcrQuality.value = config.pdf_ocr_quality;
  dom.selPdfEmbeddedTextStrategy.value = config.pdf_embedded_text_strategy;
  syncDraftCustomRemovalsFromConfig(config);
}

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
    defaultPath: "corpuswright-processing-config.json",
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

function closeSettingsModal(): void {
  setModalConfigStatus("");
  dom.settingsModal.classList.add("hidden");
}

export function syncInitialSettingsControls(): void {
  syncCheckboxesFromConfig(state.activeCleaningConfig);
  dom.selTableExtraction.value = state.activeCleaningConfig.table_extraction_strategy;
  dom.selPdfTextSource.value = state.activeCleaningConfig.pdf_text_source;
  dom.selPdfOcrQuality.value = state.activeCleaningConfig.pdf_ocr_quality;
  dom.selPdfEmbeddedTextStrategy.value = state.activeCleaningConfig.pdf_embedded_text_strategy;
}

export function initSettingsModal(callbacks: SettingsModalCallbacks): void {
  dom.settingsTabs.forEach(tab => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-target");
      dom.settingsTabs.forEach(t => {
        t.classList.remove("active");
        t.setAttribute("aria-selected", "false");
      });
      dom.settingsTabContents.forEach(c => c.classList.remove("active"));

      tab.classList.add("active");
      tab.setAttribute("aria-selected", "true");
      document.getElementById(target!)?.classList.add("active");
    });
  });

  dom.btnLoadConfig.addEventListener("click", handleLoadConfig);
  dom.btnSaveConfig.addEventListener("click", handleSaveConfig);

  dom.menuProcessingParams.addEventListener("click", () => {
    setModalConfigStatus("");
    syncCheckboxesFromConfig(state.activeCleaningConfig);
    dom.selTableExtraction.value = state.activeCleaningConfig.table_extraction_strategy;
    dom.selPdfTextSource.value = state.activeCleaningConfig.pdf_text_source;
    dom.selPdfOcrQuality.value = state.activeCleaningConfig.pdf_ocr_quality;
    dom.selPdfEmbeddedTextStrategy.value = state.activeCleaningConfig.pdf_embedded_text_strategy;
    state.tempRemovePatterns = [...state.activeCleaningConfig.remove_patterns];
    state.tempRemovalRules = state.activeCleaningConfig.removal_rules.map((rule) => ({
      ...rule,
      matcher: { ...rule.matcher },
    }));
    state.tempReplacePatterns = [...state.activeCleaningConfig.replace_patterns];
    renderCustomRemovals();

    dom.settingsTabs.forEach(t => {
      t.classList.remove("active");
      t.setAttribute("aria-selected", "false");
    });
    dom.settingsTabContents.forEach(c => c.classList.remove("active"));
    const generalTab = document.querySelector('#settings-modal .tab[data-target="tab-general"]');
    if (generalTab) {
      generalTab.classList.add("active");
      generalTab.setAttribute("aria-selected", "true");
    }
    document.getElementById("tab-general")?.classList.add("active");

    dom.settingsModal.classList.remove("hidden");
  });

  dom.cancelSettingsBtn.addEventListener("click", closeSettingsModal);
  dom.btnCloseSettingsModalTop.addEventListener("click", closeSettingsModal);

  dom.applySettingsBtn.addEventListener("click", () => {
    const nextConfig = buildConfigFromModalControls();
    Object.assign(state.activeCleaningConfig, nextConfig);

    setModalConfigStatus("");
    dom.settingsModal.classList.add("hidden");

    callbacks.updateWordCount();

    if (state.selectedCorpusIndices.size > 0) {
      callbacks.schedulePreviewUpdate(150);
    }
  });
}
