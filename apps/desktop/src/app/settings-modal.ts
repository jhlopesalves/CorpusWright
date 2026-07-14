import { invoke, open, save } from "./system-ipc";

import {
  BOOLEAN_CLEANING_CONFIG_KEYS,
  normalisePdfTextSource,
  normaliseCleaningConfigWithMigration,
  type BooleanCleaningConfigKey,
} from "../features/prepare/cleaning/config";
import { dom } from "./dom";
import { bindModalVisibility } from "./modal";
import { syncDraftCustomRemovalsFromConfig } from "../features/prepare/cleaning/custom-removals";
import {
  syncDecisionsWithActiveRules,
  syncRepeatedArtifactSettingsSection,
} from "../features/prepare/repeated-artifacts/controller";
import { state } from "./state";
import { hasProjectSourceSelection } from "../features/prepare/sources";
import { formatRemovalRuleMigrationSummary } from "../features/prepare/cleaning/removal-rule-migration.ts";
import type {
  CleaningConfig,
  PdfEmbeddedTextStrategy,
  PdfOcrQuality,
  PdfTextSource,
  TableExtractionStrategy,
} from "../generated/CleaningConfig.js";

interface CheckboxBinding {
  configKey: BooleanCleaningConfigKey;
  element: HTMLInputElement;
}

const cleaningCheckboxBindings: CheckboxBinding[] = [
  { configKey: "normalize_whitespace_line_endings",          element: dom.chkNormalizeWhitespaceLineEndings },
  { configKey: "expand_ligatures",                           element: dom.chkExpandLigatures },
  { configKey: "remove_invisible_controls",                  element: dom.chkRemoveInvisibleControls },
  { configKey: "repair_wrapped_pdf_line_breaks",             element: dom.chkRepairWrappedPdfLineBreaks },
  { configKey: "remove_isolated_page_markers",               element: dom.chkRemoveIsolatedPageMarkers },
  { configKey: "remove_url_only_lines",                      element: dom.chkRemoveUrlOnlyLines },
  { configKey: "remove_email_only_lines",                    element: dom.chkRemoveEmailOnlyLines },
  { configKey: "remove_doi_only_lines",                      element: dom.chkRemoveDoiOnlyLines },
  { configKey: "remove_figure_table_captions",               element: dom.chkRemoveFigureTableCaptions },
  { configKey: "lowercase",                                  element: dom.chkLowercase },
  { configKey: "normalize_unicode",                          element: dom.chkNormalizeUnicode },
  { configKey: "replace_diacritics",                         element: dom.chkReplaceDiacritics },
  { configKey: "extract_html",                               element: dom.chkExtractHtml },
  { configKey: "remove_obvious_extraction_noise",            element: dom.chkRemoveObviousExtractionNoise },
  { configKey: "remove_headers",                             element: dom.chkRemoveHeaders },
  { configKey: "remove_footers",                             element: dom.chkRemoveFooters },
  { configKey: "remove_footnotes",                           element: dom.chkRemoveFootnotes },
  { configKey: "remove_endnotes",                            element: dom.chkRemoveEndnotes },
  { configKey: "remove_comments",                            element: dom.chkRemoveComments },
  { configKey: "remove_table_of_contents",                   element: dom.chkRemoveToc },
  { configKey: "remove_pdf_code_like_blocks",                element: dom.chkRemovePdfCodeLikeBlocks },
  { configKey: "remove_pdf_formula_like_lines",              element: dom.chkRemovePdfFormulaLikeLines },
];

interface SettingsModalCallbacks {
  updateWordCount: () => void;
  schedulePreviewUpdate: (delay: number) => void;
}

type SettingsTabId = "tab-general" | "tab-line-page" | "tab-extraction" | "tab-custom-removals";

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
  config.pdf_text_source = normalisePdfTextSource(dom.selPdfTextSource.value as PdfTextSource);
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
  dom.selPdfTextSource.value = normalisePdfTextSource(config.pdf_text_source);
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
  let migrationStatus = "";
  try {
    const result = normaliseCleaningConfigWithMigration(parsed);
    normalised = result.config;
    migrationStatus = formatRemovalRuleMigrationSummary(result.migrationSummary);
  } catch (err) {
    setModalConfigStatus(`Error: ${err}`);
    return;
  }

  syncModalControlsFromConfig(normalised);
  setModalConfigStatus(
    `Loaded config. Click Apply to use it.${migrationStatus ? ` ${migrationStatus}` : ""}`
  );
}

async function handleSaveConfig(): Promise<void> {
  setModalConfigStatus("");
  const config = buildConfigFromModalControls();
  const json = JSON.stringify(config, null, 2);

  const selected = await save({
    defaultPath: "corpuswright-cleaning-config.json",
    filters: [{ name: "JSON Config", extensions: ["json"] }]
  });
  if (selected === null) return;

  try {
    await invoke("save_config_file_command", { path: selected, content: json });
    setModalConfigStatus("Saved cleaning config.");
  } catch (err) {
    setModalConfigStatus(`Error saving config: ${err}`);
  }
}

function closeSettingsModal(): void {
  setModalConfigStatus("");
  dom.settingsModal.classList.add("hidden");
}

function activateSettingsTab(targetId: SettingsTabId): void {
  dom.settingsTabs.forEach(t => {
    const active = t.getAttribute("data-target") === targetId;
    t.classList.toggle("active", active);
    t.setAttribute("aria-selected", active ? "true" : "false");
  });
  dom.settingsTabContents.forEach(c => c.classList.remove("active"));
  document.getElementById(targetId)?.classList.add("active");
}

export function openSettingsModal(targetId: SettingsTabId = "tab-general"): void {
  setModalConfigStatus("");
  syncModalControlsFromConfig(state.activeCleaningConfig);
  syncRepeatedArtifactSettingsSection();
  activateSettingsTab(targetId);
  dom.settingsModal.classList.remove("hidden");
}

export function syncInitialSettingsControls(): void {
  syncCheckboxesFromConfig(state.activeCleaningConfig);
  dom.selTableExtraction.value = state.activeCleaningConfig.table_extraction_strategy;
  dom.selPdfTextSource.value = normalisePdfTextSource(state.activeCleaningConfig.pdf_text_source);
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

  bindModalVisibility(
    {
      modal: dom.settingsModal,
      open: dom.prepareCleaningSettings,
      close: [dom.cancelSettingsBtn, dom.btnCloseSettingsModalTop],
    },
    () => openSettingsModal("tab-general"),
    closeSettingsModal,
  );

  dom.applySettingsBtn.addEventListener("click", () => {
    const nextConfig = buildConfigFromModalControls();
    Object.assign(state.activeCleaningConfig, nextConfig);
    syncDecisionsWithActiveRules();

    setModalConfigStatus("");
    dom.settingsModal.classList.add("hidden");

    callbacks.updateWordCount();

    if (state.selectedCorpusIndices.size > 0 || hasProjectSourceSelection()) {
      callbacks.schedulePreviewUpdate(150);
    }
  });
}
