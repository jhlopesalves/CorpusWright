import { listen } from "@tauri-apps/api/event";
import katex from "katex";
import "katex/dist/katex.min.css";

import {
  buildAnalysisCorpus,
  cancelAnalysisBuild,
  computeCorpusOverview,
  defaultAnalysisDocumentSelection,
  getAnalysisBuildStatus,
  queryAnalysisDocuments,
  queryFrequencyList,
  queryTermDistribution,
  type AnalysisBuildProgress,
  type AnalysisDocumentPage,
  type AnalysisDocumentSelection,
  type AnalysisInputSource,
  type CorpusOverview as AnalysisCorpus,
  type CorpusOverviewFinding,
  type CorpusOverviewFindingSeverity,
  type FrequencyMatchMode,
  type FrequencyPage,
  type FrequencyGroupMode,
  type FrequencySortKey,
  type FrequencyTermItem,
  type FrequencyTokenClass as FrequencyTokenClassFilter,
  type SortDirection,
  type TermDistributionPage,
} from "./analysis";
import { BOOLEAN_CLEANING_CONFIG_KEYS } from "./config";
import { createChevron } from "./chevron";
import { dom } from "./dom";
import { effectiveCleaningConfig } from "./effective-cleaning-config";
import {
  overviewGlossary,
  type OverviewGlossaryEntry,
  type OverviewGlossaryId,
} from "./overview-glossary";
import type { ProjectLanguage } from "./generated/ProjectLanguage.js";
import { projectLanguageLabel } from "./project-creation-dialog";
import { sourceHasExtractedText } from "./sources";
import { state, type WorkbenchId } from "./state";

const FUTURE_ANALYSIS_TOOLS = [
  "KWIC / Concordance",
  "N-grams",
  "Exports",
];

const FREQUENCY_PAGE_SIZE = 200;
const FREQUENCY_DOCUMENT_PAGE_SIZE = 200;
const TERM_DISTRIBUTION_PAGE_SIZE = 100;
const FREQUENCY_TABLE_COLUMN_COUNT = 5;
const ANNOTATION_UNAVAILABLE_MESSAGE =
  "Lemma grouping and POS filters need the annotation layer, which isn't available yet.";
type AnalysisToolId = "overview" | "frequency";

interface OverviewMetricRow {
  label: string;
  value: string;
  share?: string;
  title?: string;
  glossaryId?: OverviewGlossaryId;
  labelSuffix?: string;
  description?: string;
}

interface CompositionSegment {
  label: string;
  count: number;
  share: number | null;
  className: string;
  glossaryId?: OverviewGlossaryId;
  labelSuffix?: string;
}

const ANALYSIS_TOOLS: Array<{
  id: AnalysisToolId;
  name: string;
  panelId: string;
}> = [
  { id: "overview", name: "Corpus Overview", panelId: "analyse-overview-pane" },
  { id: "frequency", name: "Frequency List", panelId: "frequency-list-pane" },
];

let activeAnalysisTool: AnalysisToolId = "overview";
let frequencySortKey: FrequencySortKey = "frequency";
let frequencySortDirection: SortDirection = "desc";
let frequencyDocumentScope = defaultAnalysisDocumentSelection();
let frequencyScopeDraft = defaultAnalysisDocumentSelection();
let frequencyPage: FrequencyPage | null = null;
let frequencyPageOffset = 0;
let frequencyQueryGeneration = 0;
let frequencyQueryPending = false;
let frequencyQueryFailure: string | null = null;
let frequencyQueryDebounceTimer: number | null = null;
let frequencyDocumentsPage: AnalysisDocumentPage | null = null;
let frequencyDocumentsOffset = 0;
let frequencyDocumentsGeneration = 0;
let frequencyDocumentsPending = false;
let overviewGlossaryControlId = 0;
let frequencyDocumentsDebounceTimer: number | null = null;
let frequencyDocumentsFailure: string | null = null;
let expandedFrequencyTokens = new Set<string>();
let termDistributionPages = new Map<string, TermDistributionPage>();
let termDistributionGenerations = new Map<string, number>();
let analysisBuildInProgress = false;
let analysisBuildProgress: AnalysisBuildProgress | null = null;
let analysisBuildGeneration = 0;
let analysisProgressUnlisten: (() => void) | null = null;
let analysisNotice: { kind: "info" | "warning"; message: string } | null = null;

function pluralise(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
}

function activeAnalysisToolName(): string {
  return ANALYSIS_TOOLS.find((tool) => tool.id === activeAnalysisTool)?.name ?? "Corpus Overview";
}

function activeProjectLanguageCode(): ProjectLanguage | null {
  return state.activeProject?.language ?? null;
}

function activeProjectLanguageLabel(): string {
  const language = activeProjectLanguageCode();
  return language ? projectLanguageLabel(language) : "Not available";
}

function processedSourceCount(): number {
  if (state.activeProject) {
    return state.projectSourceSummary?.processed_count ?? 0;
  }
  return state.projectSources.filter((source) =>
    source.status === "processed" || source.status === "processed_with_warnings"
  ).length;
}

function extractedTextSourceCount(): number {
  if (state.activeProject) {
    return state.projectSourceSummary?.extracted_text_count ?? 0;
  }
  return state.projectSources.filter(sourceHasExtractedText).length;
}

function warningSourceCount(): number {
  if (state.activeProject) {
    return state.projectSourceSummary?.warning_count ?? 0;
  }
  return state.projectSources.filter((source) =>
    source.status === "text_extracted_with_warnings" ||
    source.status === "processed_with_warnings" ||
    source.reason_codes.length > 0
  ).length;
}

function analysisBuildSignature(): string {
  return JSON.stringify({
    projectRoot: state.activeProject?.rootPath ?? null,
    corpusVersion: state.currentCorpusVersion,
    sourceManifestVersion: state.sourceManifestVersion,
    cleaningConfig: effectiveCleaningConfig(),
  });
}

function analysisIsStale(corpus: AnalysisCorpus | null = state.analysisCorpus): boolean {
  if (!corpus) {
    return false;
  }
  return corpus.buildSignature !== analysisBuildSignature();
}

function analysisStatus(): "not-built" | "ready" | "stale" {
  if (!state.analysisCorpus) {
    return "not-built";
  }
  return analysisIsStale() ? "stale" : "ready";
}

function formatNumber(value: number, maximumFractionDigits = 0): string {
  return value.toLocaleString(undefined, { maximumFractionDigits });
}

function formatFixedNumber(value: number, fractionDigits: number): string {
  return value.toLocaleString(undefined, {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  });
}

function formatPercent(value: number, fractionDigits = 1): string {
  return `${formatFixedNumber(value * 100, fractionDigits)}%`;
}

function formatDocumentMetricSummary(
  median: number,
  minimum: number | null,
  maximum: number | null,
  documentsUsed: number,
  documentsExcluded: number,
  fractionDigits: number,
): string {
  const range = minimum === null || maximum === null
    ? ""
    : `; range ${formatFixedNumber(minimum, fractionDigits)}–${formatFixedNumber(maximum, fractionDigits)}`;
  const used = `${formatNumber(documentsUsed)} document${documentsUsed === 1 ? "" : "s"} used`;
  const excluded = documentsExcluded > 0
    ? `; ${formatNumber(documentsExcluded)} excluded`
    : "";
  return `median ${formatFixedNumber(median, fractionDigits)}${range}; ${used}${excluded}`;
}

function formatCountWithShare(count: number, share: number | null): string {
  return share === null
    ? formatNumber(count)
    : `${formatNumber(count)} (${formatPercent(share)})`;
}

function inputTextSourceLabel(inputTextSource: AnalysisInputSource): string {
  switch (inputTextSource) {
    case "processed": return "processed text";
    case "processedWithSourceTextFallback": return "processed text with source-text fallback";
    case "sourceTextFallback": return "source-text fallback";
  }
}

function appendSummaryRow(labelText: string, valueText: string): void {
  const label = document.createElement("dt");
  label.textContent = labelText;

  const value = document.createElement("dd");
  value.textContent = valueText;

  dom.analyseProjectSummary.append(label, value);
}

function prepareSourceCount(): number | null {
  if (state.activeProject && state.projectSourceSummary === null) {
    return null;
  }
  return state.projectSourceSummary?.total_count ?? state.projectSources.length;
}

function prepareStageStatus(): string {
  const sourceCount = prepareSourceCount();
  const extractedCount = extractedTextSourceCount();
  const warnings = warningSourceCount();

  if (sourceCount === null) {
    return state.projectSourceQueryPending ? "Sources loading..." : "Loading source summary...";
  }
  if (sourceCount === 0) {
    return "No sources";
  }
  if (warnings > 0) {
    return `${pluralise(sourceCount, "source")} | warnings`;
  }
  if (extractedCount > 0) {
    return `${pluralise(extractedCount, "source")} text available`;
  }
  return pluralise(sourceCount, "source");
}

function activeCleaningItemCount(): number {
  const config = effectiveCleaningConfig();
  const enabledBooleanRules = BOOLEAN_CLEANING_CONFIG_KEYS
    .filter((key) => config[key])
    .length;
  const enabledStructuredRemovals = config.removal_rules
    .filter((rule) => rule.enabled)
    .length;
  return enabledBooleanRules
    + config.remove_patterns.length
    + enabledStructuredRemovals
    + config.replace_patterns.length;
}

function refreshPrepareCockpit(): void {
  const sourceCount = prepareSourceCount();
  const extractedCount = extractedTextSourceCount();
  const cleaningCount = activeCleaningItemCount();

  dom.prepareCockpitSourceCount.textContent = sourceCount === null
    ? (state.projectSourceQueryPending ? "Sources loading..." : "Loading source summary...")
    : pluralise(sourceCount, "source");
  dom.prepareCockpitCleaningSummary.textContent = cleaningCount === 0
    ? "Cleaning defaults"
    : pluralise(cleaningCount, "active cleaning item");
  dom.prepareProcessingStatus.textContent = sourceCount === null
    ? "Preview status pending"
    : sourceCount === 0
      ? "No sources"
      : `${formatNumber(extractedCount)} text available | Processed Text previews current settings`;
}

function analyseStageStatus(): string {
  switch (analysisStatus()) {
    case "ready": return activeAnalysisToolName();
    case "stale": return "Stale";
    case "not-built": return "Not built";
  }
}

function refreshStageNavigator(): void {
  const analyseIsActive = state.activeWorkbench === "analyse";
  const status = analysisStatus();
  const sourceCount = prepareSourceCount();

  dom.workbenchPrepare.classList.toggle("active", !analyseIsActive);
  dom.workbenchAnalyse.classList.toggle("active", analyseIsActive);
  dom.workbenchExport.classList.remove("active");

  dom.workbenchPrepare.setAttribute("aria-selected", analyseIsActive ? "false" : "true");
  dom.workbenchAnalyse.setAttribute("aria-selected", analyseIsActive ? "true" : "false");
  dom.workbenchExport.setAttribute("aria-selected", "false");

  dom.workbenchPrepare.dataset.stageState = sourceCount === null ? "loading" : sourceCount > 0 ? "ready" : "blocked";
  dom.workbenchAnalyse.dataset.stageState = status === "ready" ? "ready" : status === "stale" ? "stale" : "blocked";
  dom.workbenchExport.dataset.stageState = "future";

  dom.stagePrepareStatus.textContent = prepareStageStatus();
  dom.stageAnalyseStatus.textContent = analyseStageStatus();
  dom.stageExportStatus.textContent = "Coming later";
  refreshPrepareCockpit();
}

function renderAnalyseStatus(): void {
  const status = analysisStatus();
  const corpus = state.analysisCorpus;
  const processedCount = processedSourceCount();

  dom.analyseStatusChip.dataset.status = status;
  dom.analyseBuildButton.textContent = corpus ? "Rebuild Analysis Corpus" : "Build Analysis Corpus";

  if (analysisBuildInProgress) {
    dom.analyseStatusChip.textContent = "Building analysis corpus";
    dom.analyseStatusDetail.textContent = analysisBuildProgressText(analysisBuildProgress);
    dom.analyseBuildButton.textContent = "Cancel build";
    dom.analyseBuildButton.disabled = false;
    dom.analyseBuildNote.textContent = analysisBuildProgressText(analysisBuildProgress);
    return;
  }

  if (status === "ready" && corpus) {
    dom.analyseStatusChip.textContent =
      `Built ${new Date(corpus.builtAt).toLocaleString()} from ${inputTextSourceLabel(corpus.inputTextSource)}`;
    dom.analyseStatusDetail.textContent =
      `${pluralise(corpus.documentCount, "document")} - ${formatNumber(corpus.totalTokens)} tokens`;
    dom.analyseBuildNote.textContent = "Rebuilds the analysis from the current project text.";
    return;
  }

  if (status === "stale") {
    dom.analyseStatusChip.textContent = "Analysis corpus stale";
    dom.analyseStatusDetail.textContent =
      "Prepare-side inputs changed after the analysis corpus was built. Rebuild to refresh the counts.";
    dom.analyseBuildNote.textContent = "Rebuild keeps the analysis data in memory for this session.";
    return;
  }

  dom.analyseStatusChip.textContent = "Analysis corpus not built";
  dom.analyseStatusDetail.textContent = processedCount > 0
    ? "Processed text is available. Build the analysis corpus to inspect descriptive counts."
    : "Analysis corpus not built yet.";
  dom.analyseBuildNote.textContent = "Builds the analysis from the current project text.";
}

function analysisPhaseLabel(progress: AnalysisBuildProgress): string {
  switch (progress.currentPhase) {
    case "loadingSources": return "loading sources";
    case "readingFiles": return "reading files";
    case "tokenising": return "tokenising";
    case "computingDispersion": return "computing dispersion";
    case "finalisingSummaries": return "finalising summaries";
  }
}

function analysisBuildProgressText(progress: AnalysisBuildProgress | null): string {
  if (!progress) {
    return "Building analysis corpus...";
  }
  return `${formatNumber(progress.filesDone)} / ${formatNumber(progress.filesTotal)} sources · ${formatNumber(progress.tokensSeen)} tokens · ${analysisPhaseLabel(progress)}`;
}

function renderAnalysisNotice(): void {
  dom.analyseInputBanner.classList.add("hidden");
  dom.analyseInputBanner.classList.remove("warning");
  dom.analyseInputBanner.textContent = "";

  const corpus = state.analysisCorpus;
  if (!corpus) {
    if (analysisNotice) {
      dom.analyseInputBanner.textContent = analysisNotice.message;
      dom.analyseInputBanner.classList.toggle("warning", analysisNotice.kind === "warning");
      dom.analyseInputBanner.classList.remove("hidden");
    }
    return;
  }

  if (analysisIsStale(corpus)) {
    dom.analyseInputBanner.textContent =
      "The current analysis corpus is stale because Prepare-side data or cleaning settings changed.";
    dom.analyseInputBanner.classList.add("warning");
    dom.analyseInputBanner.classList.remove("hidden");
    return;
  }

  if (analysisNotice) {
    dom.analyseInputBanner.textContent = analysisNotice.message;
    dom.analyseInputBanner.classList.toggle("warning", analysisNotice.kind === "warning");
    dom.analyseInputBanner.classList.remove("hidden");
    return;
  }

  if (corpus.population.usingSourceTextFallback > 0) {
    dom.analyseInputBanner.textContent =
      `${pluralise(corpus.population.usingSourceTextFallback, "source")} used native/extracted text because processed text could not be produced.`;
    dom.analyseInputBanner.classList.add("warning");
    dom.analyseInputBanner.classList.remove("hidden");
  }
}

function renderProjectSummary(): void {
  const sourceCount = prepareSourceCount() ?? 0;
  const fileCount = state.activeProject ? state.projectCorpusFileCount : state.allFiles.length;
  const processedCount = processedSourceCount();
  const extractedCount = extractedTextSourceCount();

  dom.analyseProjectSummary.replaceChildren();
  appendSummaryRow("Project", state.activeProject?.name ?? "No project open");
  appendSummaryRow("Project language", activeProjectLanguageLabel());
  appendSummaryRow("Sources", pluralise(sourceCount, "source"));
  appendSummaryRow("Files loaded", pluralise(fileCount, "file"));
  appendSummaryRow("Text available", `${extractedCount.toLocaleString()} of ${sourceCount.toLocaleString()} sources`);
  appendSummaryRow("Processed sources", pluralise(processedCount, "source"));
  if (state.cleanedTokenCountText) {
    appendSummaryRow(state.activeProject ? "Source token count" : "Cleaned token count", state.cleanedTokenCountText);
  }
}

function renderAnalysisTools(): void {
  dom.analyseToolsList.replaceChildren();

  for (const tool of ANALYSIS_TOOLS) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "analyse-tool-item";
    button.dataset.analysisTool = tool.id;
    button.setAttribute("aria-controls", tool.panelId);
    button.setAttribute("aria-current", activeAnalysisTool === tool.id ? "page" : "false");
    button.textContent = tool.name;

    item.appendChild(button);
    dom.analyseToolsList.appendChild(item);
  }

  for (const name of FUTURE_ANALYSIS_TOOLS) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "analyse-tool-item is-future";
    button.disabled = true;
    button.setAttribute("aria-disabled", "true");

    const label = document.createElement("span");
    label.textContent = name;

    const status = document.createElement("span");
    status.className = "analyse-tool-status";
    status.textContent = "Later";

    button.append(label, status);
    item.appendChild(button);
    dom.analyseToolsList.appendChild(item);
  }
}

function renderAnalysisToolPanels(): void {
  dom.analyseOverviewPane.classList.toggle("hidden", activeAnalysisTool !== "overview");
  dom.frequencySection.classList.toggle("hidden", activeAnalysisTool !== "frequency");
}

function documentName(document: { relativePath: string; sourcePath: string }): string {
  return document.relativePath || document.sourcePath;
}

function createOverviewPanel(titleText: string): HTMLElement {
  const panel = document.createElement("section");
  panel.className = "analysis-panel";

  const title = document.createElement("h3");
  title.textContent = titleText;
  panel.appendChild(title);

  return panel;
}

function appendGlossarySection(panel: HTMLElement, titleText: string, body: HTMLElement): void {
  const section = document.createElement("section");
  section.className = "analysis-glossary-section";

  const title = document.createElement("h4");
  title.textContent = titleText;
  section.append(title, body);
  panel.appendChild(section);
}

function createGlossaryParagraph(text: string): HTMLParagraphElement {
  const paragraph = document.createElement("p");
  paragraph.textContent = text;
  return paragraph;
}

function createGlossarySymbol(source: string): HTMLSpanElement {
  const symbol = document.createElement("span");
  symbol.className = "analysis-glossary-symbol-math";
  try {
    katex.render(source, symbol, {
      displayMode: false,
      output: "htmlAndMathml",
      throwOnError: false,
    });
  } catch {
    symbol.textContent = source;
  }
  return symbol;
}

function createGlossarySymbolsList(symbols: OverviewGlossaryEntry["symbols"]): HTMLDListElement {
  const list = document.createElement("dl");
  list.className = "analysis-glossary-symbols";

  for (const symbol of symbols ?? []) {
    const row = document.createElement("div");
    row.className = "analysis-glossary-symbol-row";

    const term = document.createElement("dt");
    term.appendChild(createGlossarySymbol(symbol.sym));

    const definition = document.createElement("dd");
    definition.textContent = symbol.def;

    row.append(term, definition);
    list.appendChild(row);
  }

  return list;
}

function createGlossaryLimitationsList(limitations: string[]): HTMLUListElement {
  const list = document.createElement("ul");
  list.className = "analysis-glossary-limitations";

  for (const limitation of limitations) {
    const item = document.createElement("li");
    item.textContent = limitation;
    list.appendChild(item);
  }

  return list;
}

function createGlossaryExample(example: string): HTMLDetailsElement {
  const disclosure = document.createElement("details");
  disclosure.className = "analysis-glossary-example";

  const summary = document.createElement("summary");
  summary.textContent = "Worked example";

  disclosure.append(summary, createGlossaryParagraph(example));
  return disclosure;
}

function createGlossaryDisclosure(
  glossaryId: OverviewGlossaryId,
  onToggle?: (expanded: boolean) => void,
): { button: HTMLButtonElement; panel: HTMLDivElement } {
  const entry: OverviewGlossaryEntry = overviewGlossary[glossaryId];
  const panelId = `overview-glossary-${entry.id}-${++overviewGlossaryControlId}`;

  const button = document.createElement("button");
  button.type = "button";
  button.className = "analysis-glossary-term";
  button.setAttribute("aria-expanded", "false");
  button.setAttribute("aria-controls", panelId);

  const label = document.createElement("span");
  label.textContent = entry.label;
  const caret = document.createElement("span");
  caret.className = "analysis-glossary-caret";
  caret.setAttribute("aria-hidden", "true");
  caret.textContent = "›";
  button.append(label, caret);

  const panel = document.createElement("div");
  panel.id = panelId;
  panel.className = "analysis-glossary-detail";
  panel.hidden = true;

  const explanation = document.createElement("p");
  explanation.textContent = entry.explanation;
  panel.appendChild(explanation);

  if (entry.detail) {
    panel.appendChild(createGlossaryParagraph(entry.detail));
  }

  const formula = document.createElement("div");
  formula.className = "analysis-glossary-formula";
  katex.render(entry.latex, formula, {
    displayMode: true,
    output: "htmlAndMathml",
    throwOnError: false,
  });
  panel.appendChild(formula);

  if (entry.symbols?.length) {
    appendGlossarySection(panel, "Where", createGlossarySymbolsList(entry.symbols));
  }

  if (entry.interpretation) {
    appendGlossarySection(panel, "How to read it", createGlossaryParagraph(entry.interpretation));
  }

  if (entry.limitations?.length) {
    appendGlossarySection(panel, "Limitations", createGlossaryLimitationsList(entry.limitations));
  }

  if (entry.example) {
    panel.appendChild(createGlossaryExample(entry.example));
  }

  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(expanded));
    panel.hidden = !expanded;
    onToggle?.(expanded);
  });

  return { button, panel };
}

function createMetricTable(rows: OverviewMetricRow[], className = ""): HTMLTableElement {
  const table = document.createElement("table");
  table.className = ["analysis-metric-table", className].filter(Boolean).join(" ");

  const body = document.createElement("tbody");
  for (const row of rows) {
    const item = document.createElement("tr");
    if (row.title) {
      item.title = row.title;
    }

    const label = document.createElement("th");
    label.scope = "row";

    let glossaryPanel: HTMLDivElement | null = null;
    let glossaryRow: HTMLTableRowElement | null = null;
    if (row.glossaryId) {
      glossaryRow = document.createElement("tr");
      glossaryRow.className = "analysis-glossary-row";
      glossaryRow.hidden = true;
      const disclosure = createGlossaryDisclosure(row.glossaryId, (expanded) => {
        if (glossaryRow) glossaryRow.hidden = !expanded;
      });
      glossaryPanel = disclosure.panel;
      label.appendChild(disclosure.button);
    } else {
      const labelText = document.createElement("span");
      labelText.textContent = row.label;
      label.appendChild(labelText);
    }
    if (row.labelSuffix) {
      const suffix = document.createElement("span");
      suffix.className = "analysis-glossary-label-suffix";
      suffix.textContent = ` ${row.labelSuffix}`;
      label.appendChild(suffix);
    }
    if (row.description) {
      const description = document.createElement("span");
      description.className = "analysis-metric-description";
      description.textContent = row.description;
      label.appendChild(description);
    }

    const value = document.createElement("td");
    value.className = "numeric analysis-metric-value";
    value.textContent = row.value;

    const share = document.createElement("td");
    share.className = "numeric analysis-metric-share";
    if (row.share) {
      share.textContent = row.share;
    } else {
      share.setAttribute("aria-hidden", "true");
    }

    item.append(label, value, share);
    body.appendChild(item);
    if (glossaryPanel && glossaryRow) {
      const detailCell = document.createElement("td");
      detailCell.colSpan = 3;
      detailCell.appendChild(glossaryPanel);
      glossaryRow.appendChild(detailCell);
      body.appendChild(glossaryRow);
    }
  }

  table.appendChild(body);
  return table;
}

function createCompositionBar(
  title: string,
  total: number,
  segments: CompositionSegment[],
  glossaryId?: OverviewGlossaryId,
): HTMLElement {
  const composition = document.createElement("div");
  composition.className = "analysis-composition";

  const titleEl = document.createElement("h4");
  if (glossaryId) {
    const disclosure = createGlossaryDisclosure(glossaryId);
    titleEl.appendChild(disclosure.button);
    composition.append(titleEl, disclosure.panel);
  } else {
    titleEl.textContent = title;
    composition.appendChild(titleEl);
  }

  if (total <= 0) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = "No token types available.";
    composition.appendChild(empty);
    return composition;
  }

  const bar = document.createElement("div");
  bar.className = "analysis-composition-bar";
  bar.setAttribute("role", "img");
  bar.setAttribute(
    "aria-label",
    segments.map((segment) =>
      `${segment.label}: ${formatCountWithShare(segment.count, segment.share)}`
    ).join("; "),
  );

  for (const segment of segments) {
    if (segment.count <= 0) {
      continue;
    }
    const fill = document.createElement("span");
    fill.className = `analysis-composition-segment ${segment.className}`;
    fill.style.setProperty("--segment-width", `${(segment.share ?? 0) * 100}%`);
    bar.appendChild(fill);
  }

  const legend = document.createElement("dl");
  legend.className = "analysis-composition-legend";
  for (const segment of segments) {
    const item = document.createElement("div");
    item.className = `analysis-composition-legend-item ${segment.className}`;

    const label = document.createElement("dt");
    let glossaryDetail: HTMLDivElement | null = null;
    let glossaryDescription: HTMLElement | null = null;
    if (segment.glossaryId) {
      glossaryDescription = document.createElement("dd");
      glossaryDescription.className = "analysis-composition-glossary";
      glossaryDescription.hidden = true;
      const disclosure = createGlossaryDisclosure(segment.glossaryId, (expanded) => {
        if (glossaryDescription) glossaryDescription.hidden = !expanded;
      });
      glossaryDetail = disclosure.panel;
      label.appendChild(disclosure.button);
      if (segment.labelSuffix) {
        const suffix = document.createElement("span");
        suffix.textContent = ` ${segment.labelSuffix}`;
        label.appendChild(suffix);
      }
    } else {
      label.textContent = segment.label;
    }

    const value = document.createElement("dd");
    value.textContent = formatCountWithShare(segment.count, segment.share);

    item.append(label, value);
    if (glossaryDetail && glossaryDescription) {
      glossaryDescription.appendChild(glossaryDetail);
      item.appendChild(glossaryDescription);
    }
    legend.appendChild(item);
  }

  composition.append(bar, legend);
  return composition;
}

function findingLabel(finding: CorpusOverviewFinding): string {
  switch (finding.key) {
    case "balance": return "Balance";
    case "extractionQuality": return "Extraction quality";
    case "lexicalSpectrum": return "Lexical spectrum";
    case "dispersion": return "Dispersion";
  }
}

function severityClass(severity: CorpusOverviewFindingSeverity): string {
  switch (severity) {
    case "noIssue": return "no-issue";
    case "worthALook": return "worth-a-look";
    case "needsFixing": return "needs-fixing";
  }
}

function appendPanelReading(panel: HTMLElement, text: string): void {
  const reading = document.createElement("p");
  reading.className = "analysis-panel-note";
  reading.textContent = text;
  panel.appendChild(reading);
}

function countAndShareRow(
  label: string,
  count: number,
  share: number | null,
): OverviewMetricRow {
  return {
    label,
    value: formatNumber(count),
    share: share === null ? undefined : formatPercent(share),
  };
}

function renderCorpusReading(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Reading of this corpus");
  panel.classList.add("analysis-reading-panel");

  const verdict = document.createElement("p");
  verdict.className = "analysis-reading-verdict";
  verdict.textContent = corpus.reading.verdict;

  const list = document.createElement("ul");
  list.className = "analysis-reading-findings";
  for (const finding of corpus.reading.findings) {
    const item = document.createElement("li");
    item.className = `analysis-reading-finding is-${severityClass(finding.severity)}`;

    const dot = document.createElement("span");
    dot.className = "analysis-reading-status-dot";
    dot.setAttribute("aria-hidden", "true");

    const body = document.createElement("div");
    body.className = "analysis-reading-finding-body";

    const heading = document.createElement("span");
    heading.className = "analysis-reading-finding-heading";

    const label = document.createElement("strong");
    label.textContent = findingLabel(finding);

    const status = document.createElement("span");
    status.className = "analysis-reading-status";
    status.textContent = `(${finding.severityLabel})`;

    const text = document.createElement("span");
    text.className = "analysis-reading-finding-text";
    text.textContent = finding.text;

    heading.append(label, document.createTextNode(" "), status, document.createTextNode("."));
    body.append(heading, text);
    item.append(dot, body);
    list.appendChild(item);
  }

  const legend = document.createElement("ul");
  legend.className = "analysis-severity-legend";
  for (const [severity, label] of [
    ["noIssue", "no immediate issue"],
    ["worthALook", "worth a look"],
    ["needsFixing", "needs fixing"],
  ] as const) {
    const item = document.createElement("li");
    item.className = `is-${severityClass(severity)}`;

    const dot = document.createElement("span");
    dot.className = "analysis-reading-status-dot";
    dot.setAttribute("aria-hidden", "true");

    const text = document.createElement("span");
    text.textContent = label;
    item.append(dot, text);
    legend.appendChild(item);
  }

  panel.append(verdict, list, legend);
  return panel;
}

function renderAnalysisPopulation(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Analysis population");
  const population = corpus.population;
  const rows: OverviewMetricRow[] = [
    { label: "Tokens", value: formatNumber(corpus.totals.tokens) },
    { label: "Unique types", value: formatNumber(corpus.totals.types) },
    { label: "Documents", value: formatNumber(corpus.totals.documents) },
  ];

  if (population.sourcesInProject !== null) {
    rows.push({ label: "Sources in project", value: formatNumber(population.sourcesInProject) });
  }
  rows.push({ label: "Included in analysis", value: formatNumber(population.includedInAnalysis) });
  rows.push({
    label: "Processed text used",
    value: formatNumber(population.processedUsed),
  });
  rows.push(
    {
      label: "Using source-text fallback",
      value: formatNumber(population.usingSourceTextFallback),
    },
    { label: "Empty or failed extraction", value: formatNumber(population.emptyOrFailed) },
  );
  if (population.sourcesWithWarnings !== null) {
    rows.push({
      label: "Sources with warnings",
      value: formatNumber(population.sourcesWithWarnings),
    });
  }
  rows.push({
    label: "Input to analysis",
    value: population.inputSource,
  });

  panel.appendChild(createMetricTable(rows));
  appendPanelReading(panel, population.reading);
  return panel;
}

function renderDocumentLengthDistribution(corpus: AnalysisCorpus): HTMLElement {
  const distribution = document.createElement("div");
  distribution.className = "document-length-distribution";

  const title = document.createElement("h4");
  title.textContent = corpus.documentsTruncated
    ? "Largest document contributors"
    : "Document length distribution";
  distribution.appendChild(title);

  if (corpus.documents.length === 0) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = "No documents in the analysis corpus.";
    distribution.appendChild(empty);
    return distribution;
  }

  for (const documentRecord of corpus.documents) {
    const row = document.createElement("div");
    row.className = "document-length-row";
    row.title = documentRecord.name;

    const name = document.createElement("span");
    name.className = "document-length-name";
    name.textContent = documentRecord.name;

    const track = document.createElement("span");
    track.className = "document-length-track";

    const fill = document.createElement("span");
    fill.className = "document-length-fill";
    const barWidth = (documentRecord.relativeLength ?? 0) * 100;
    fill.style.setProperty("--bar-width", `${barWidth}%`);
    track.appendChild(fill);

    const tokenCount = document.createElement("span");
    tokenCount.className = "numeric document-length-count";
    tokenCount.textContent = formatNumber(documentRecord.tokens);

    const share = document.createElement("span");
    share.className = "numeric document-length-share";
    share.textContent = documentRecord.share === null ? "" : formatPercent(documentRecord.share);

    row.append(name, track, tokenCount, share);
    distribution.appendChild(row);
  }

  if (corpus.documentsTruncated) {
    const summary = document.createElement("p");
    summary.className = "analysis-panel-note";
    const median = corpus.balance.medianTokens === null
      ? ""
      : ` Median length: ${formatNumber(corpus.balance.medianTokens)} tokens.`;
    const range = corpus.balance.minimumTokens === null || corpus.balance.maximumTokens === null
      ? ""
      : ` Range: ${formatNumber(corpus.balance.minimumTokens)}–${formatNumber(corpus.balance.maximumTokens)} tokens.`;
    summary.textContent =
      `Showing ${formatNumber(corpus.documents.length)} largest contributors; ${formatNumber(corpus.balance.remainingDocuments)} documents are summarised rather than rendered.${median}${range}`;
    distribution.appendChild(summary);
  }

  return distribution;
}

function renderDocumentBalance(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Document balance");
  const balance = corpus.balance;
  panel.appendChild(renderDocumentLengthDistribution(corpus));

  const rows: OverviewMetricRow[] = [];
  if (balance.largestShare !== null) {
    rows.push({ label: "Largest document share", value: formatPercent(balance.largestShare) });
  }
  if (balance.top3Share !== null) {
    rows.push({ label: "Top three share", value: formatPercent(balance.top3Share) });
  }
  if (balance.gini !== null) {
    rows.push({
      label: "Gini coefficient",
      value: formatFixedNumber(balance.gini, 3),
      glossaryId: "giniCoefficient",
    });
  }
  if (balance.coefficientOfVariation !== null) {
    rows.push({
      label: "Coefficient of variation",
      value: formatFixedNumber(balance.coefficientOfVariation, 3),
      glossaryId: "coefficientOfVariation",
    });
  }
  if (balance.effectiveDocuments !== null) {
    rows.push({
      label: "Effective documents",
      value: `${formatFixedNumber(balance.effectiveDocuments, 1)} of ${formatNumber(corpus.totals.documents)}`,
      glossaryId: "effectiveDocuments",
    });
  }
  if (rows.length > 0) {
    panel.appendChild(createMetricTable(rows));
  }
  appendPanelReading(panel, balance.reading);
  return panel;
}

function renderExtractionQuality(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Extraction quality");
  const quality = corpus.extractionQuality;
  const rows: OverviewMetricRow[] = [
    countAndShareRow("Word-like types", quality.wordLikeCount, quality.wordLikeShare),
    countAndShareRow("Numeric-only types", quality.numericOnlyCount, quality.numericOnlyShare),
  ];
  if (quality.singleCharacterCount !== null) {
    rows.push(countAndShareRow(
      "Single-character types",
      quality.singleCharacterCount,
      quality.singleCharacterShare,
    ));
  }

  panel.appendChild(createMetricTable(rows));
  appendPanelReading(panel, quality.reading);
  return panel;
}

function renderLexicalSpectrum(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Lexical spectrum");
  const spectrum = corpus.lexicalSpectrum;
  panel.appendChild(createMetricTable([
    { label: "Unique types", value: formatNumber(spectrum.types) },
  ]));
  panel.appendChild(createCompositionBar("Frequency-class spectrum", spectrum.types, [
    {
      label: "Hapax",
      labelSuffix: "(1)",
      count: spectrum.hapaxCount,
      share: spectrum.hapaxShare,
      className: "is-hapax",
      glossaryId: "hapax",
    },
    {
      label: "Dis legomena",
      labelSuffix: "(2)",
      count: spectrum.disLegomenaCount,
      share: spectrum.disLegomenaShare,
      className: "is-dis",
      glossaryId: "disLegomena",
    },
    { label: "3–9 occurrences", count: spectrum.band3To9Count, share: spectrum.band3To9Share, className: "is-mid-frequency" },
    { label: "10+ occurrences", count: spectrum.band10plusCount, share: spectrum.band10plusShare, className: "is-frequent" },
  ]));

  const lexicalDiversity = document.createElement("div");
  lexicalDiversity.className = "analysis-metric-group";
  const lexicalDiversityTitle = document.createElement("h4");
  lexicalDiversityTitle.textContent = "Lexical diversity";
  let mattrValue: string;
  if (spectrum.mattrMedian !== null) {
    mattrValue = formatDocumentMetricSummary(
      spectrum.mattrMedian,
      spectrum.mattrMin,
      spectrum.mattrMax,
      spectrum.mattrDocsUsed,
      spectrum.mattrDocsExcluded,
      3,
    );
  } else if (corpus.totals.documents === 0) {
    mattrValue = "not computable — no documents";
  } else {
    mattrValue = `not computable — documents shorter than ${formatNumber(spectrum.mattrWindow)}-token window`;
  }

  let mtldValue: string;
  if (spectrum.mtldMedian !== null) {
    mtldValue = formatDocumentMetricSummary(
      spectrum.mtldMedian,
      spectrum.mtldMin,
      spectrum.mtldMax,
      spectrum.mtldDocsUsed,
      spectrum.mtldDocsExcluded,
      2,
    );
  } else if (corpus.totals.documents === 0) {
    mtldValue = "not computable — no documents";
  } else if (
    corpus.balance.maximumTokens !== null
    && corpus.balance.maximumTokens < spectrum.mtldMinDocumentTokens
  ) {
    mtldValue = `not computable — documents shorter than ${formatNumber(spectrum.mtldMinDocumentTokens)}-token minimum`;
  } else {
    mtldValue = "not computable — no eligible document produced a finite value";
  }

  const diversityRows: OverviewMetricRow[] = [
    {
      label: "MATTR",
      labelSuffix: `(window ${formatNumber(spectrum.mattrWindow)})`,
      value: mattrValue,
      glossaryId: "mattr",
    },
    {
      label: "MTLD",
      value: mtldValue,
      glossaryId: "mtld",
    },
  ];
  lexicalDiversity.append(lexicalDiversityTitle, createMetricTable(diversityRows));
  const mattrNote = document.createElement("p");
  mattrNote.className = "analysis-panel-note";
  mattrNote.textContent = "MATTR reduces raw TTR’s length sensitivity, but it still depends on window size, tokenisation, and casing.";
  const mtldNote = document.createElement("p");
  mtldNote.className = "analysis-panel-note";
  mtldNote.textContent = `MTLD is less sensitive to text length than raw TTR, but it depends on the ${formatFixedNumber(spectrum.mtldThreshold, 3)} threshold and can be unstable for very short documents.`;
  lexicalDiversity.append(mattrNote, mtldNote);
  panel.appendChild(lexicalDiversity);
  appendPanelReading(panel, spectrum.reading);
  return panel;
}

function renderFrequencyConcentration(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Frequency concentration");
  const concentration = corpus.frequencyConcentration;
  const rows: OverviewMetricRow[] = [];
  if (concentration.top100Coverage !== null) {
    rows.push({ label: "Top 100 types cover", value: formatPercent(concentration.top100Coverage) });
  }
  if (concentration.top1000Coverage !== null) {
    rows.push({ label: "Top 1,000 types cover", value: formatPercent(concentration.top1000Coverage) });
  }
  if (concentration.top10000Coverage !== null) {
    rows.push({ label: "Top 10,000 types cover", value: formatPercent(concentration.top10000Coverage) });
  }
  if (concentration.evennessJ !== null) {
    rows.push({
      label: "Normalised entropy / evenness",
      value: formatFixedNumber(concentration.evennessJ, 3),
      glossaryId: "normalisedEntropy",
    });
  }
  if (concentration.shannonEntropyBits !== null) {
    rows.push({
      label: "Shannon entropy",
      labelSuffix: "(bits)",
      value: formatFixedNumber(concentration.shannonEntropyBits, 3),
    });
  }
  if (concentration.perplexity !== null) {
    rows.push({
      label: "Perplexity",
      value: formatNumber(concentration.perplexity),
    });
  }
  if (concentration.effectiveVocabularyEntropy !== null) {
    rows.push({
      label: "Effective vocabulary (entropy-based)",
      value: `≈ ${formatNumber(concentration.effectiveVocabularyEntropy)} types`,
      glossaryId: "effectiveVocabulary",
    });
  }
  if (rows.length > 0) {
    panel.appendChild(createMetricTable(rows));
  }
  appendPanelReading(panel, concentration.reading);
  return panel;
}

function renderRankFrequency(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Rank–frequency fit");
  const rank = corpus.rankFrequency;
  const rows: OverviewMetricRow[] = [
    { label: "Distribution points", value: formatNumber(rank.points.length) },
  ];
  if (rank.zipfExponent !== null) {
    rows.push({ label: "Zipf exponent", value: formatFixedNumber(rank.zipfExponent, 3) });
  }
  if (rank.zipfCoefficient !== null) {
    rows.push({ label: "Zipf coefficient", value: formatFixedNumber(rank.zipfCoefficient, 2) });
  }
  if (rank.rSquared !== null) {
    rows.push({ label: "Fit R²", value: formatFixedNumber(rank.rSquared, 3) });
  }
  if (rank.fittedRankStart !== null && rank.fittedRankEnd !== null) {
    rows.push({
      label: "Fitted ranks",
      value: `${formatNumber(rank.fittedRankStart)}–${formatNumber(rank.fittedRankEnd)}`,
    });
  }
  panel.appendChild(createMetricTable(rows));
  appendPanelReading(panel, rank.reading);
  return panel;
}

function renderVocabularyGrowth(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Vocabulary growth");
  const growth = corpus.vocabularyGrowth;
  const finalPoint = growth.points[growth.points.length - 1];
  const rows: OverviewMetricRow[] = [
    { label: "Growth checkpoints", value: formatNumber(growth.points.length) },
  ];
  if (finalPoint) {
    rows.push({
      label: "Final checkpoint",
      value: `${formatNumber(finalPoint.types)} types / ${formatNumber(finalPoint.tokens)} tokens`,
    });
  }
  if (growth.heapsK !== null) {
    rows.push({ label: "Heaps K", value: formatFixedNumber(growth.heapsK, 3) });
  }
  if (growth.heapsBeta !== null) {
    rows.push({ label: "Heaps β", value: formatFixedNumber(growth.heapsBeta, 3) });
  }
  if (growth.rSquared !== null) {
    rows.push({ label: "Fit R²", value: formatFixedNumber(growth.rSquared, 3) });
  }
  panel.appendChild(createMetricTable(rows));
  appendPanelReading(panel, growth.reading);
  return panel;
}

function renderDispersionProfile(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Dispersion profile");
  const dispersion = corpus.dispersion;
  const rows: OverviewMetricRow[] = [
    { label: "Frequent types (10+ occurrences)", value: formatNumber(dispersion.frequentTypes) },
  ];
  if (corpus.totals.documents >= 2) {
    rows.push({
      label: "Gries DP",
      labelSuffix: "(at least 0.5, frequent types)",
      value: `${formatNumber(dispersion.highDpFrequentTypes)} of ${formatNumber(dispersion.frequentTypes)}`,
      glossaryId: "griesDp",
      description: "How many frequent types are noticeably clumped - their DP is 0.5 or higher.",
    });
    if (dispersion.medianDp !== null) {
      rows.push({
        label: "Gries DP",
        labelSuffix: "(median, frequent types)",
        value: formatFixedNumber(dispersion.medianDp, 2),
        description: "The middle DP value across frequent types - how uneven the typical frequent type is.",
      });
    }
  }
  panel.appendChild(createMetricTable(rows));

  if (corpus.totals.documents >= 2 && corpus.totals.types > 0) {
    const segments: CompositionSegment[] = [
      {
        label: "Types in 1 document only",
        count: dispersion.dfIn1Count,
        share: dispersion.dfIn1Share,
        className: "is-hapax",
      },
    ];
    if (corpus.totals.documents > 2) {
      segments.push({
        label: `Types in 2–${formatNumber(corpus.totals.documents - 1)} documents`,
        count: dispersion.dfIn2ToNMinus1Count,
        share: dispersion.dfIn2ToNMinus1Share,
        className: "is-mid-frequency",
      });
    }
    segments.push({
      label: `Types in all ${formatNumber(corpus.totals.documents)} documents`,
      count: dispersion.dfInAllCount,
      share: dispersion.dfInAllShare,
      className: "is-frequent",
    });
    panel.appendChild(createCompositionBar(
      "Document-frequency spectrum",
      corpus.totals.types,
      segments,
      "documentFrequencySpectrum",
    ));

    const frequencyRows: OverviewMetricRow[] = [
      countAndShareRow(
        "Types in 1 document only",
        dispersion.dfIn1Count,
        dispersion.dfIn1Share,
      ),
    ];
    if (corpus.totals.documents > 2) {
      frequencyRows.push(countAndShareRow(
        `Types in 2–${formatNumber(corpus.totals.documents - 1)} documents`,
        dispersion.dfIn2ToNMinus1Count,
        dispersion.dfIn2ToNMinus1Share,
      ));
    }
    frequencyRows.push(countAndShareRow(
      `Types in all ${formatNumber(corpus.totals.documents)} documents`,
      dispersion.dfInAllCount,
      dispersion.dfInAllShare,
    ));
    if (dispersion.medianDocumentsPerType !== null) {
      frequencyRows.push({
        label: "Median documents per type",
        value: formatNumber(dispersion.medianDocumentsPerType, 1),
      });
    }
    panel.appendChild(createMetricTable(frequencyRows));
  }

  appendPanelReading(panel, dispersion.reading);
  return panel;
}

function renderBuildDetails(corpus: AnalysisCorpus): HTMLElement {
  const details = document.createElement("details");
  details.className = "analysis-panel analysis-build-details";

  const summary = document.createElement("summary");
  summary.textContent = "Build details";
  details.appendChild(summary);

  const build = corpus.build;
  const rows: OverviewMetricRow[] = [];
  if (build.tokenisation !== null) {
    rows.push({ label: "Tokenisation", value: build.tokenisation });
  }
  if (build.caseHandling !== null) {
    rows.push({ label: "Case handling", value: build.caseHandling });
  }
  if (build.lemmatisation !== null) {
    rows.push({ label: "Lemmatisation", value: build.lemmatisation });
  }
  if (build.numberPunctuationHandling !== null) {
    rows.push({ label: "Number/punctuation handling", value: build.numberPunctuationHandling });
  }
  rows.push({ label: "Input to analysis", value: build.inputSource });
  if (build.configVersion !== null) {
    rows.push({ label: "Config version", value: build.configVersion });
  }
  if (build.snapshotDate !== null) {
    const snapshotDate = new Date(build.snapshotDate);
    rows.push({
      label: "Snapshot date",
      value: Number.isNaN(snapshotDate.getTime())
        ? build.snapshotDate
        : snapshotDate.toLocaleString(),
    });
  }
  if (rows.length > 0) {
    details.appendChild(createMetricTable(rows, "analysis-build-table"));
  }

  if (!build.metadataAttached) {
    appendPanelReading(
      details,
      "No metadata attached to sources. Corpus composition is assessed from file-level diagnostics only — genre, register, author and date cannot be reported, and no representativeness or domain claim is made.",
    );
  }
  return details;
}

function renderCorpusOverview(): void {
  dom.analyseOverview.replaceChildren();
  const corpus = state.analysisCorpus;
  if (!corpus) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = analysisNotice?.message ??
      "Build an analysis corpus to see the first-stage corpus diagnostics.";
    dom.analyseOverview.appendChild(empty);
    return;
  }

  dom.analyseOverview.append(
    renderCorpusReading(corpus),
    renderAnalysisPopulation(corpus),
    renderDocumentBalance(corpus),
    renderExtractionQuality(corpus),
    renderLexicalSpectrum(corpus),
    renderFrequencyConcentration(corpus),
    renderRankFrequency(corpus),
    renderVocabularyGrowth(corpus),
    renderDispersionProfile(corpus),
    renderBuildDetails(corpus),
  );
}


function cloneFrequencySelection(selection: AnalysisDocumentSelection): AnalysisDocumentSelection {
  return {
    allDocuments: selection.allDocuments,
    ids: [...selection.ids],
    excludedIds: [...selection.excludedIds],
  };
}

function frequencySelectionCount(selection: AnalysisDocumentSelection, totalDocuments: number): number {
  return selection.allDocuments
    ? Math.max(0, totalDocuments - selection.excludedIds.length)
    : selection.ids.length;
}

function frequencySelectionIncludes(selection: AnalysisDocumentSelection, documentId: number): boolean {
  return selection.allDocuments
    ? !selection.excludedIds.includes(documentId)
    : selection.ids.includes(documentId);
}

function syncFrequencyStopwordsToggle(): void {
  const isHidden = dom.frequencyHideStopwordsInput.checked;
  dom.frequencyHideStopwordsInput.setAttribute("aria-checked", isHidden ? "true" : "false");
  dom.frequencyStopwordState.textContent = isHidden ? "Hidden" : "Hide";
}

function setFrequencySelectionDocument(
  selection: AnalysisDocumentSelection,
  documentId: number,
  selected: boolean,
): void {
  if (selection.allDocuments) {
    const excluded = new Set(selection.excludedIds);
    if (selected) {
      excluded.delete(documentId);
    } else {
      excluded.add(documentId);
    }
    selection.excludedIds = [...excluded];
    return;
  }

  const ids = new Set(selection.ids);
  if (selected) {
    ids.add(documentId);
  } else {
    ids.delete(documentId);
  }
  selection.ids = [...ids];
}

function resetFrequencyCorpusState(): void {
  if (frequencyQueryDebounceTimer !== null) {
    window.clearTimeout(frequencyQueryDebounceTimer);
    frequencyQueryDebounceTimer = null;
  }
  if (frequencyDocumentsDebounceTimer !== null) {
    window.clearTimeout(frequencyDocumentsDebounceTimer);
    frequencyDocumentsDebounceTimer = null;
  }
  frequencyDocumentScope = defaultAnalysisDocumentSelection();
  frequencyScopeDraft = defaultAnalysisDocumentSelection();
  frequencyPage = null;
  frequencyPageOffset = 0;
  frequencyQueryGeneration += 1;
  frequencyQueryPending = false;
  frequencyQueryFailure = null;
  frequencyDocumentsPage = null;
  frequencyDocumentsOffset = 0;
  frequencyDocumentsGeneration += 1;
  frequencyDocumentsPending = false;
  frequencyDocumentsFailure = null;
  expandedFrequencyTokens = new Set<string>();
  termDistributionPages.clear();
  termDistributionGenerations.clear();
}

function renderFrequencyScopeControl(corpus: AnalysisCorpus | null): void {
  if (!corpus || corpus.documentCount === 0) {
    dom.frequencyFilesButton.disabled = true;
    dom.frequencyFilesButton.textContent = "Files: 0 of 0";
    dom.frequencyFilesButton.setAttribute("aria-label", "No files available for frequency scoping");
    return;
  }

  const selectedCount = frequencySelectionCount(frequencyDocumentScope, corpus.documentCount);
  dom.frequencyFilesButton.disabled = false;
  dom.frequencyFilesButton.textContent = `Files: ${formatNumber(selectedCount)} of ${formatNumber(corpus.documentCount)}`;
  dom.frequencyFilesButton.setAttribute(
    "aria-label",
    `Frequency list file scope: ${formatNumber(selectedCount)} of ${formatNumber(corpus.documentCount)} files selected`,
  );
}

function renderFrequencySearchError(message: string | null): void {
  dom.frequencySearchError.textContent = message ?? "";
  dom.frequencySearchError.classList.toggle("hidden", message === null);
}

function renderFrequencyQueryNotice(page: FrequencyPage | null): void {
  const messages = page?.notices.map((notice) => notice.message) ?? [];
  if (state.analysisCorpus && !state.analysisCorpus.annotationAvailable) {
    messages.push(ANNOTATION_UNAVAILABLE_MESSAGE);
  }
  const message = messages.join(" ");
  dom.frequencyQueryNotice.textContent = message;
  dom.frequencyQueryNotice.classList.toggle("hidden", message.length === 0);
}

function syncFrequencyAnnotationControls(corpus: AnalysisCorpus | null = state.analysisCorpus): void {
  const annotationAvailable = corpus?.annotationAvailable === true;
  const lemmaOption = dom.frequencyGroupBySelect.querySelector<HTMLOptionElement>('option[value="lemma"]');
  if (lemmaOption) {
    lemmaOption.disabled = !annotationAvailable;
    lemmaOption.textContent = annotationAvailable ? "Lemma" : "Lemma (annotation unavailable)";
  }
  if (!annotationAvailable && dom.frequencyGroupBySelect.value === "lemma") {
    dom.frequencyGroupBySelect.value = "surface";
  }
  dom.frequencyGroupBySelect.title = annotationAvailable ? "" : ANNOTATION_UNAVAILABLE_MESSAGE;

  const anyPosOption = dom.frequencyPosFilterSelect.querySelector<HTMLOptionElement>('option[value=""]');
  if (anyPosOption) {
    anyPosOption.textContent = annotationAvailable ? "Any POS" : "Annotation unavailable";
  }
  if (!annotationAvailable) {
    dom.frequencyPosFilterSelect.value = "";
  }
  dom.frequencyPosFilterSelect.disabled = !annotationAvailable;
  dom.frequencyPosFilterSelect.title = annotationAvailable ? "" : ANNOTATION_UNAVAILABLE_MESSAGE;
}

function currentFrequencyTokenClassFilter(): FrequencyTokenClassFilter {
  return dom.frequencyTokenClassSelect.value as FrequencyTokenClassFilter;
}

function currentFrequencyGroupMode(): FrequencyGroupMode {
  return dom.frequencyGroupBySelect.value as FrequencyGroupMode;
}

function currentFrequencyCaseFold(): boolean {
  return dom.frequencyCaseFoldInput.checked || currentFrequencyGroupMode() === "stem";
}

function currentFrequencyLanguage(): string | null {
  return currentFrequencyGroupMode() === "stem" ? activeProjectLanguageCode() : null;
}

function currentFrequencyPosFilter(): string | null {
  const value = dom.frequencyPosFilterSelect.value.trim();
  return value.length === 0 ? null : value;
}

function syncFrequencyCaseFoldToggle(): void {
  const stemsAreSelected = currentFrequencyGroupMode() === "stem";
  const isFolded = currentFrequencyCaseFold();
  dom.frequencyCaseFoldInput.disabled = stemsAreSelected;
  dom.frequencyCaseFoldInput.checked = isFolded;
  dom.frequencyCaseFoldInput.setAttribute("aria-checked", isFolded ? "true" : "false");
  dom.frequencyCaseFoldState.textContent = isFolded ? "Folded" : "As typed";
  dom.frequencyLanguageDisplay.textContent = activeProjectLanguageLabel();
  dom.frequencyLanguageDisplay.classList.toggle("is-disabled", !stemsAreSelected);
}

function tokenClassSummary(filter: FrequencyTokenClassFilter): string {
  switch (filter) {
    case "alphabetic":
      return "alphabetic words only";
    case "includeSingles":
      return "single-character tokens included";
    case "includeNumerals":
      return "numerals included";
    case "all":
      return "all token classes";
  }
}

function frequencyGroupingSummary(): string {
  switch (currentFrequencyGroupMode()) {
    case "surface":
      return currentFrequencyCaseFold() ? "case-folded surface forms" : "surface forms";
    case "lemma":
      return currentFrequencyCaseFold() ? "case-folded lemmas" : "lemmas";
    case "stem":
      return `${activeProjectLanguageLabel()} stems`;
  }
}

function frequencyDetailId(token: string): string {
  return `frequency-detail-${encodeURIComponent(token)}`;
}

function updateFrequencySortHeaders(): void {
  const buttons = dom.frequencyTableHead.querySelectorAll<HTMLButtonElement>(".frequency-sort");
  buttons.forEach((button) => {
    const sort = button.dataset.sort as FrequencySortKey;
    button.removeAttribute("aria-sort");
    if (sort === frequencySortKey) {
      button.setAttribute("aria-sort", frequencySortDirection === "asc" ? "ascending" : "descending");
    }
  });
}

function frequencySummaryText(page: FrequencyPage, tokenClassFilter: FrequencyTokenClassFilter): string {
  const documentScope = page.selectedDocumentCount === page.totalDocumentCount
    ? `${formatNumber(page.totalDocumentCount)} files`
    : `${formatNumber(page.selectedDocumentCount)} of ${formatNumber(page.totalDocumentCount)} files`;
  const parts = [`${formatNumber(page.totalTerms)} of ${formatNumber(page.scopedTotalTypes)} types shown`];
  if (dom.frequencyHideStopwordsInput.checked) {
    parts.push("English stopwords hidden");
  }
  parts.push(
    frequencyGroupingSummary(),
    tokenClassSummary(tokenClassFilter),
    documentScope,
    `${formatNumber(page.scopedTotalTokens)} tokens in scope`,
  );
  return parts.join(" · ");
}

function dispersionLevel(dispersion: number): "low" | "medium" | "high" {
  if (dispersion < 0.34) {
    return "low";
  }
  if (dispersion < 0.67) {
    return "medium";
  }
  return "high";
}

function createDispersionCell(term: FrequencyTermItem): HTMLTableCellElement {
  const cell = document.createElement("td");
  cell.className = "numeric frequency-dp-cell";

  const value = document.createElement("span");
  value.className = "frequency-dp-value";
  value.textContent = formatFixedNumber(term.dispersion, 2);

  const track = document.createElement("span");
  track.className = "frequency-dp-track";
  const fill = document.createElement("span");
  fill.className = "frequency-dp-fill";
  fill.dataset.level = dispersionLevel(term.dispersion);
  fill.style.setProperty("--bar-width", `${Math.max(0, Math.min(100, term.dispersion * 100))}%`);
  track.appendChild(fill);

  cell.append(value, track);
  return cell;
}

async function loadTermDistribution(token: string, offset: number): Promise<void> {
  const generation = (termDistributionGenerations.get(token) ?? 0) + 1;
  termDistributionGenerations.set(token, generation);
  termDistributionPages.delete(token);
  renderFrequencyList();
  try {
    const page = await queryTermDistribution(
      token,
      frequencyDocumentScope,
      currentFrequencyGroupMode(),
      currentFrequencyCaseFold(),
      currentFrequencyLanguage(),
      offset,
      TERM_DISTRIBUTION_PAGE_SIZE,
      effectiveCleaningConfig(),
    );
    if (termDistributionGenerations.get(token) !== generation || !expandedFrequencyTokens.has(token)) {
      return;
    }
    termDistributionPages.set(token, page);
    renderFrequencyList();
  } catch (error) {
    if (termDistributionGenerations.get(token) !== generation) {
      return;
    }
    analysisNotice = { kind: "warning", message: `Term distribution could not be loaded: ${error}` };
    expandedFrequencyTokens.delete(token);
    renderAnalysisNotice();
    renderFrequencyList();
  }
}

function createFrequencyDistributionPanel(term: FrequencyTermItem): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "frequency-distribution-panel";

  const title = document.createElement("h3");
  title.textContent = "Per-file distribution (selected files)";
  panel.appendChild(title);

  const page = termDistributionPages.get(term.token);
  if (!page) {
    const loading = document.createElement("p");
    loading.className = "analysis-panel-note";
    loading.textContent = "Loading distribution...";
    panel.appendChild(loading);
    return panel;
  }

  const header = document.createElement("div");
  header.className = "frequency-distribution-row frequency-distribution-header";
  header.append(
    Object.assign(document.createElement("span"), { textContent: "Document/File" }),
    Object.assign(document.createElement("span"), { textContent: "Count" }),
    Object.assign(document.createElement("span"), { textContent: "Per 10k" }),
    Object.assign(document.createElement("span"), { textContent: "Bar" }),
  );
  panel.appendChild(header);

  for (const item of page.items) {
    const line = document.createElement("div");
    line.className = "frequency-distribution-row";

    const label = document.createElement("span");
    label.className = "frequency-distribution-file";
    label.title = documentName(item);
    label.textContent = documentName(item);

    const count = document.createElement("span");
    count.className = "numeric";
    count.textContent = formatNumber(item.count);

    const rate = document.createElement("span");
    rate.className = "numeric";
    rate.textContent = formatFixedNumber(item.ratePerMillion, 2);

    const track = document.createElement("span");
    track.className = "frequency-distribution-track";
    const fill = document.createElement("span");
    fill.className = "frequency-distribution-fill";
    const barWidth = page.maxRatePerMillion > 0 && item.ratePerMillion > 0
      ? Math.max(3, (item.ratePerMillion / page.maxRatePerMillion) * 100)
      : 0;
    fill.style.setProperty("--bar-width", `${barWidth}%`);
    track.appendChild(fill);

    line.append(label, count, rate, track);
    panel.appendChild(line);
  }

  const pagination = document.createElement("div");
  pagination.className = "analysis-pagination";
  const previous = document.createElement("button");
  previous.type = "button";
  previous.className = "secondary-btn";
  previous.textContent = "Previous";
  previous.disabled = page.offset === 0;
  previous.addEventListener("click", () => {
    void loadTermDistribution(term.token, Math.max(0, page.offset - page.limit));
  });
  const status = document.createElement("span");
  const first = page.totalDocuments === 0 ? 0 : page.offset + 1;
  const last = Math.min(page.totalDocuments, page.offset + page.items.length);
  status.textContent = `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(page.totalDocuments)}`;
  const next = document.createElement("button");
  next.type = "button";
  next.className = "secondary-btn";
  next.textContent = "Next";
  next.disabled = page.offset + page.items.length >= page.totalDocuments;
  next.addEventListener("click", () => {
    void loadTermDistribution(term.token, page.offset + page.limit);
  });
  pagination.append(previous, status, next);
  panel.appendChild(pagination);
  return panel;
}

async function refreshFrequencyPage(resetOffset = true): Promise<void> {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    return;
  }
  syncFrequencyAnnotationControls(corpus);
  if (resetOffset) {
    frequencyPageOffset = 0;
  }
  const generation = ++frequencyQueryGeneration;
  frequencyQueryPending = true;
  frequencyQueryFailure = null;
  frequencyPage = null;
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  renderFrequencyList();

  try {
    const page = await queryFrequencyList(
      {
        selection: frequencyDocumentScope,
        query: dom.frequencySearchInput.value.trim(),
        matchMode: dom.frequencyMatchModeSelect.value as FrequencyMatchMode,
        minimumFrequency: Math.max(1, Number.parseInt(dom.frequencyMinInput.value, 10) || 1),
        tokenClass: currentFrequencyTokenClassFilter(),
        hideStopwords: dom.frequencyHideStopwordsInput.checked,
        groupBy: currentFrequencyGroupMode(),
        caseFold: currentFrequencyCaseFold(),
        language: currentFrequencyLanguage(),
        posFilter: currentFrequencyPosFilter(),
        sortKey: frequencySortKey,
        sortDirection: frequencySortDirection,
        offset: frequencyPageOffset,
        limit: FREQUENCY_PAGE_SIZE,
      },
      effectiveCleaningConfig(),
    );
    if (frequencyQueryGeneration !== generation) {
      return;
    }
    frequencyPage = page;
    frequencyPageOffset = page.offset;
  } catch (error) {
    if (frequencyQueryGeneration !== generation) {
      return;
    }
    frequencyQueryFailure = `${error}`;
  } finally {
    if (frequencyQueryGeneration === generation) {
      frequencyQueryPending = false;
      renderFrequencyList();
    }
  }
}

function scheduleFrequencyRefresh(): void {
  if (frequencyQueryDebounceTimer !== null) {
    window.clearTimeout(frequencyQueryDebounceTimer);
  }
  frequencyQueryDebounceTimer = window.setTimeout(() => {
    frequencyQueryDebounceTimer = null;
    void refreshFrequencyPage();
  }, 150);
}

function renderFrequencyList(): void {
  updateFrequencySortHeaders();
  dom.frequencyTableBody.replaceChildren();
  dom.frequencyLimitNote.textContent = "";
  renderFrequencySearchError(frequencyQueryFailure);

  const corpus = state.analysisCorpus;
  syncFrequencyAnnotationControls(corpus);
  renderFrequencyQueryNotice(frequencyPage);
  renderFrequencyScopeControl(corpus);
  if (!corpus) {
    dom.frequencyResultCount.textContent = "";
    dom.frequencySummary.textContent = "Build an analysis corpus to populate the frequency list.";
    dom.frequencyPageStatus.textContent = "";
    dom.frequencyPagePreviousButton.disabled = true;
    dom.frequencyPageNextButton.disabled = true;
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
    cell.className = "frequency-empty";
    cell.textContent = "Build an analysis corpus to populate the frequency list.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  if (!frequencyPage) {
    dom.frequencySummary.textContent = frequencyQueryFailure
      ? "Frequency query failed."
      : "Querying the analysis...";
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
    cell.className = "frequency-empty";
    cell.textContent = frequencyQueryFailure ?? "Loading frequency results...";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    dom.frequencyPageStatus.textContent = "";
    dom.frequencyPagePreviousButton.disabled = true;
    dom.frequencyPageNextButton.disabled = true;
    if (activeAnalysisTool === "frequency" && !frequencyQueryPending && !frequencyQueryFailure) {
      void refreshFrequencyPage(false);
    }
    return;
  }

  const page = frequencyPage;
  renderFrequencySearchError(page.queryError);
  renderFrequencyQueryNotice(page);
  dom.frequencySummary.textContent = frequencySummaryText(page, currentFrequencyTokenClassFilter());
  dom.frequencyResultCount.textContent = `Built from ${inputTextSourceLabel(corpus.inputTextSource)}`;
  const first = page.totalTerms === 0 ? 0 : page.offset + 1;
  const last = Math.min(page.totalTerms, page.offset + page.items.length);
  dom.frequencyPageStatus.textContent = `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(page.totalTerms)}`;
  dom.frequencyPagePreviousButton.disabled = page.offset === 0;
  dom.frequencyPageNextButton.disabled = page.offset + page.items.length >= page.totalTerms;

  if (page.items.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
    cell.className = "frequency-empty";
    cell.textContent = "No terms match the current filters.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  for (const term of page.items) {
    const isExpanded = expandedFrequencyTokens.has(term.token);
    const detailId = frequencyDetailId(term.token);
    const row = document.createElement("tr");
    row.className = "frequency-term-row";
    row.dataset.token = term.token;
    row.tabIndex = 0;
    row.setAttribute("aria-expanded", isExpanded ? "true" : "false");
    row.setAttribute("aria-controls", detailId);
    row.setAttribute("aria-label", `${isExpanded ? "Collapse" : "Expand"} per-file distribution for ${term.token}`);

    const token = document.createElement("td");
    token.className = "frequency-token-cell";
    token.title = term.token;
    const tokenContent = document.createElement("span");
    tokenContent.className = "frequency-token-content";
    const caret = createChevron("frequency-token-caret");
    const tokenText = document.createElement("span");
    tokenText.className = "frequency-token-text";
    tokenText.textContent = term.token;
    tokenContent.append(caret, tokenText);
    token.appendChild(tokenContent);

    const frequency = document.createElement("td");
    frequency.className = "numeric";
    frequency.textContent = formatNumber(term.frequency);
    const documentFrequency = document.createElement("td");
    documentFrequency.className = "numeric";
    documentFrequency.textContent = `${formatNumber(term.documentFrequency)} / ${formatNumber(page.selectedDocumentCount)}`;
    const rate = document.createElement("td");
    rate.className = "numeric";
    rate.textContent = formatFixedNumber(term.ratePerMillion, 2);
    row.append(token, frequency, documentFrequency, rate, createDispersionCell(term));
    dom.frequencyTableBody.appendChild(row);

    if (isExpanded) {
      const detailRow = document.createElement("tr");
      detailRow.className = "frequency-detail-row";
      const detailCell = document.createElement("td");
      detailCell.id = detailId;
      detailCell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
      detailCell.appendChild(createFrequencyDistributionPanel(term));
      detailRow.appendChild(detailCell);
      dom.frequencyTableBody.appendChild(detailRow);
    }
  }
}
export function renderAnalyseWorkbench(): void {
  renderAnalyseStatus();
  renderAnalysisNotice();
  dom.analyseProjectLanguage.textContent = `Project language: ${activeProjectLanguageLabel()}`;
  dom.frequencyLanguageDisplay.textContent = activeProjectLanguageLabel();
  renderProjectSummary();
  renderAnalysisTools();
  renderAnalysisToolPanels();
  renderCorpusOverview();
  renderFrequencyList();
}

export function setActiveWorkbench(workbench: WorkbenchId): void {
  state.activeWorkbench = workbench;

  const analyseIsActive = workbench === "analyse";
  dom.prepareWorkbench.classList.toggle("hidden", analyseIsActive);
  dom.analyseWorkbench.classList.toggle("hidden", !analyseIsActive);
  dom.prepareWorkbench.setAttribute("aria-hidden", analyseIsActive ? "true" : "false");
  dom.analyseWorkbench.setAttribute("aria-hidden", analyseIsActive ? "false" : "true");

  refreshStageNavigator();

  if (analyseIsActive) {
    renderAnalyseWorkbench();
  }
}

function setActiveAnalysisTool(tool: AnalysisToolId): void {
  activeAnalysisTool = tool;
  renderAnalysisTools();
  renderAnalysisToolPanels();
  if (tool === "frequency") {
    renderFrequencyList();
  }
  refreshStageNavigator();
}

export function resetWorkbenchForProject(): void {
  analysisBuildGeneration += 1;
  if (analysisBuildInProgress) {
    void cancelAnalysisBuild();
  }
  analysisBuildInProgress = false;
  analysisBuildProgress = null;
  analysisNotice = null;
  activeAnalysisTool = "overview";
  resetFrequencyCorpusState();
  dom.workbenchSwitcher.classList.remove("hidden");
  setActiveWorkbench("prepare");
}

export function hideWorkbenchSwitcher(): void {
  analysisBuildGeneration += 1;
  if (analysisBuildInProgress) {
    void cancelAnalysisBuild();
  }
  analysisBuildInProgress = false;
  analysisBuildProgress = null;
  analysisNotice = null;
  activeAnalysisTool = "overview";
  resetFrequencyCorpusState();
  dom.workbenchSwitcher.classList.add("hidden");
  setActiveWorkbench("prepare");
}

export function refreshAnalyseWorkbench(): void {
  refreshStageNavigator();
  if (state.activeWorkbench === "analyse") {
    renderAnalyseWorkbench();
  }
}

async function handleBuildAnalysisCorpus(): Promise<void> {
  if (analysisBuildInProgress) {
    dom.analyseBuildButton.disabled = true;
    dom.analyseBuildNote.textContent = "Cancelling analysis build...";
    dom.statusBar.textContent = "Cancelling analysis build...";
    try {
      await cancelAnalysisBuild();
    } finally {
      dom.analyseBuildButton.disabled = false;
    }
    return;
  }

  const indices = state.activeProject ? null : state.allFiles.map((_, index) => index);
  const buildSignature = analysisBuildSignature();

  analysisNotice = null;

  if ((indices?.length ?? state.projectCorpusFileCount) === 0) {
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    analysisNotice = {
      kind: "warning",
      message: "No text available for analysis. Go to Prepare, extract/process sources, then return to Analyse.",
    };
    dom.statusBar.textContent = "No text available for analysis.";
    renderAnalyseWorkbench();
    refreshStageNavigator();
    return;
  }

  analysisBuildInProgress = true;
  analysisBuildProgress = null;
  const buildGeneration = ++analysisBuildGeneration;
  state.analysisCorpus = null;
  resetFrequencyCorpusState();
  renderAnalyseWorkbench();
  dom.statusBar.textContent = "Building analysis corpus...";

  try {
    const builtOverview = await buildAnalysisCorpus(
      indices,
      state.currentCorpusVersion,
      buildSignature,
      effectiveCleaningConfig(),
    );
    if (analysisBuildGeneration !== buildGeneration) {
      return;
    }
    const overview = await computeCorpusOverview(effectiveCleaningConfig()).catch(() => builtOverview);
    state.analysisCorpus = overview;
    resetFrequencyCorpusState();
    activeAnalysisTool = "overview";
    const sourceMessage = overview.population.usingSourceTextFallback > 0
      ? `Built from processed text with ${pluralise(overview.population.usingSourceTextFallback, "source")} using source-text fallback.`
      : "Built from processed text.";
    const skipped = overview.skippedDocuments > 0
      ? ` ${pluralise(overview.skippedDocuments, "document")} without usable text skipped.`
      : "";
    const skippedPdfs = overview.skippedPdfDocuments > 0
      ? ` ${pluralise(overview.skippedPdfDocuments, "PDF")} had no materialised text.`
      : "";
    analysisNotice = skipped || skippedPdfs
      ? { kind: "warning", message: `${sourceMessage}${skipped}${skippedPdfs}`.trim() }
      : null;
    dom.statusBar.textContent =
      `Analysis corpus built from ${inputTextSourceLabel(overview.inputTextSource)}. ${formatNumber(overview.documentCount)} documents · ${formatNumber(overview.totalTokens)} tokens · ${formatNumber(overview.totalTypes)} types.`;
  } catch (error) {
    if (analysisBuildGeneration !== buildGeneration) {
      return;
    }
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    const message = `${error}`;
    analysisNotice = {
      kind: "warning",
      message: message.toLowerCase().includes("cancelled")
        ? "Analysis corpus build cancelled."
        : `Analysis build failed: ${message}`,
    };
    dom.statusBar.textContent = analysisNotice.message;
  } finally {
    if (analysisBuildGeneration !== buildGeneration) {
      return;
    }
    analysisBuildInProgress = false;
    analysisBuildProgress = null;
    dom.analyseBuildButton.disabled = false;
    renderAnalyseWorkbench();
    refreshStageNavigator();
  }
}

async function initialiseAnalysisBuildState(): Promise<void> {
  if (analysisProgressUnlisten) {
    analysisProgressUnlisten();
  }
  analysisProgressUnlisten = await listen<AnalysisBuildProgress>("analysis-build-progress", (event) => {
    if (!analysisBuildInProgress) {
      return;
    }
    analysisBuildProgress = event.payload;
    const progressText = analysisBuildProgressText(event.payload);
    dom.statusBar.textContent = `Building analysis corpus... ${progressText}`;
    if (state.activeWorkbench === "analyse") {
      renderAnalyseStatus();
    }
  });

  try {
    const status = await getAnalysisBuildStatus(effectiveCleaningConfig());
    analysisBuildInProgress = status.state === "building";
    analysisBuildProgress = status.progress;
    if (status.state === "ready" && status.overview && !state.analysisCorpus) {
      state.analysisCorpus = await computeCorpusOverview(effectiveCleaningConfig()).catch(() => status.overview);
    }
  } catch {
    // The Analyse screen remains usable even if startup status recovery is unavailable.
  }
}

function focusWorkbench(workbench: WorkbenchId): void {
  setActiveWorkbench(workbench);
  const button = workbench === "prepare" ? dom.workbenchPrepare : dom.workbenchAnalyse;
  button.focus();
}

function handleWorkbenchKeydown(event: KeyboardEvent): void {
  if (event.key === "ArrowLeft" || event.key === "Home") {
    event.preventDefault();
    focusWorkbench("prepare");
  } else if (event.key === "ArrowRight" || event.key === "End") {
    event.preventDefault();
    focusWorkbench("analyse");
  }
}

function handleAnalysisToolClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-analysis-tool]");
  if (!button) {
    return;
  }

  const nextTool = button.dataset.analysisTool as AnalysisToolId | undefined;
  if (!nextTool || nextTool === activeAnalysisTool) {
    return;
  }

  setActiveAnalysisTool(nextTool);
}

function handleFrequencySortClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(".frequency-sort");
  if (!button) {
    return;
  }
  const nextKey = button.dataset.sort as FrequencySortKey | undefined;
  if (!nextKey) {
    return;
  }
  if (nextKey === frequencySortKey) {
    frequencySortDirection = frequencySortDirection === "asc" ? "desc" : "asc";
  } else {
    frequencySortKey = nextKey;
    frequencySortDirection = nextKey === "token" ? "asc" : "desc";
  }
  void refreshFrequencyPage();
}

function toggleFrequencyTokenExpansion(token: string): void {
  if (expandedFrequencyTokens.has(token)) {
    expandedFrequencyTokens.delete(token);
    termDistributionGenerations.set(token, (termDistributionGenerations.get(token) ?? 0) + 1);
    termDistributionPages.delete(token);
    renderFrequencyList();
    return;
  }
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  expandedFrequencyTokens.add(token);
  void loadTermDistribution(token, 0);
}

function showPreviousFrequencyPage(): void {
  if (!frequencyPage || frequencyPage.offset === 0) {
    return;
  }
  frequencyPageOffset = Math.max(0, frequencyPage.offset - frequencyPage.limit);
  void refreshFrequencyPage(false);
}

function showNextFrequencyPage(): void {
  if (!frequencyPage || frequencyPage.offset + frequencyPage.items.length >= frequencyPage.totalTerms) {
    return;
  }
  frequencyPageOffset = frequencyPage.offset + frequencyPage.limit;
  void refreshFrequencyPage(false);
}

function handleFrequencyTableClick(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLElement) || target.closest(".frequency-detail-row")) {
    return;
  }

  const row = target.closest<HTMLTableRowElement>(".frequency-term-row");
  if (!row?.dataset.token) {
    return;
  }

  toggleFrequencyTokenExpansion(row.dataset.token);
}

function handleFrequencyTableKeydown(event: KeyboardEvent): void {
  if (event.key !== "Enter" && event.key !== " ") {
    return;
  }

  const target = event.target;
  if (!(target instanceof HTMLElement) || target.closest(".frequency-detail-row")) {
    return;
  }

  const row = target.closest<HTMLTableRowElement>(".frequency-term-row");
  if (!row?.dataset.token) {
    return;
  }

  event.preventDefault();
  toggleFrequencyTokenExpansion(row.dataset.token);
}

function closeFrequencyFilesModal(): void {
  dom.frequencyFilesModal.classList.add("hidden");
}

async function refreshFrequencyDocumentsPage(resetOffset = true): Promise<void> {
  if (!state.analysisCorpus) {
    return;
  }
  if (resetOffset) {
    frequencyDocumentsOffset = 0;
  }
  const generation = ++frequencyDocumentsGeneration;
  frequencyDocumentsPending = true;
  frequencyDocumentsFailure = null;
  frequencyDocumentsPage = null;
  renderFrequencyFilesModal();
  try {
    const page = await queryAnalysisDocuments(
      frequencyScopeDraft,
      dom.frequencyFilesSearchInput.value,
      frequencyDocumentsOffset,
      FREQUENCY_DOCUMENT_PAGE_SIZE,
      effectiveCleaningConfig(),
    );
    if (frequencyDocumentsGeneration !== generation) {
      return;
    }
    frequencyDocumentsPage = page;
    frequencyDocumentsOffset = page.offset;
  } catch (error) {
    if (frequencyDocumentsGeneration === generation) {
      frequencyDocumentsFailure = `${error}`;
    }
  } finally {
    if (frequencyDocumentsGeneration === generation) {
      frequencyDocumentsPending = false;
      renderFrequencyFilesModal();
    }
  }
}

function scheduleFrequencyDocumentsRefresh(): void {
  if (frequencyDocumentsDebounceTimer !== null) {
    window.clearTimeout(frequencyDocumentsDebounceTimer);
  }
  frequencyDocumentsDebounceTimer = window.setTimeout(() => {
    frequencyDocumentsDebounceTimer = null;
    void refreshFrequencyDocumentsPage();
  }, 150);
}

function renderFrequencyFilesModal(): void {
  const corpus = state.analysisCorpus;
  dom.frequencyFilesList.replaceChildren();

  if (!corpus) {
    dom.frequencyFilesApplyButton.disabled = true;
    dom.frequencyFilesStatus.textContent = "Build an analysis corpus before choosing files.";
    dom.frequencyFilesStatus.classList.add("is-error");
    dom.frequencyFilesPreviousButton.disabled = true;
    dom.frequencyFilesNextButton.disabled = true;
    dom.frequencyFilesPageStatus.textContent = "";
    return;
  }

  const selectedCount = frequencySelectionCount(frequencyScopeDraft, corpus.documentCount);
  dom.frequencyFilesStatus.textContent = selectedCount === 0
    ? "Select at least one file."
    : `${formatNumber(selectedCount)} of ${formatNumber(corpus.documentCount)} files selected`;
  dom.frequencyFilesStatus.classList.toggle("is-error", selectedCount === 0);
  dom.frequencyFilesApplyButton.disabled = selectedCount === 0;

  if (frequencyDocumentsFailure) {
    const failure = document.createElement("p");
    failure.className = "frequency-files-empty";
    failure.textContent = `Files could not be loaded: ${frequencyDocumentsFailure}`;
    dom.frequencyFilesList.appendChild(failure);
    dom.frequencyFilesStatus.textContent = "File query failed.";
    dom.frequencyFilesStatus.classList.add("is-error");
    dom.frequencyFilesPreviousButton.disabled = true;
    dom.frequencyFilesNextButton.disabled = true;
    dom.frequencyFilesPageStatus.textContent = "";
    return;
  }

  const page = frequencyDocumentsPage;
  if (!page) {
    const loading = document.createElement("p");
    loading.className = "frequency-files-empty";
    loading.textContent = frequencyDocumentsPending ? "Loading files..." : "No file window loaded.";
    dom.frequencyFilesList.appendChild(loading);
    dom.frequencyFilesPreviousButton.disabled = true;
    dom.frequencyFilesNextButton.disabled = true;
    dom.frequencyFilesPageStatus.textContent = "";
    if (!frequencyDocumentsPending) {
      void refreshFrequencyDocumentsPage(false);
    }
    return;
  }

  if (page.items.length === 0) {
    const empty = document.createElement("p");
    empty.className = "frequency-files-empty";
    empty.textContent = "No files match the current search.";
    dom.frequencyFilesList.appendChild(empty);
  }

  for (const documentRecord of page.items) {
    const label = document.createElement("label");
    label.className = "frequency-file-option";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.dataset.documentId = documentRecord.id.toString();
    checkbox.checked = frequencySelectionIncludes(frequencyScopeDraft, documentRecord.id);

    const name = document.createElement("span");
    name.className = "frequency-file-name";
    name.title = documentName(documentRecord);
    name.textContent = documentName(documentRecord);

    const tokens = document.createElement("span");
    tokens.className = "frequency-file-tokens";
    tokens.textContent = `${formatNumber(documentRecord.tokenCount)} tokens`;

    label.append(checkbox, name, tokens);
    dom.frequencyFilesList.appendChild(label);
  }

  const first = page.totalCount === 0 ? 0 : page.offset + 1;
  const last = Math.min(page.totalCount, page.offset + page.items.length);
  dom.frequencyFilesPageStatus.textContent =
    `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(page.totalCount)}`;
  dom.frequencyFilesPreviousButton.disabled = page.offset === 0;
  dom.frequencyFilesNextButton.disabled = page.offset + page.items.length >= page.totalCount;
}

function openFrequencyFilesModal(): void {
  if (!state.analysisCorpus) {
    return;
  }

  frequencyScopeDraft = cloneFrequencySelection(frequencyDocumentScope);
  frequencyDocumentsPage = null;
  frequencyDocumentsOffset = 0;
  dom.frequencyFilesSearchInput.value = "";
  dom.frequencyFilesModal.classList.remove("hidden");
  void refreshFrequencyDocumentsPage();
  dom.frequencyFilesSearchInput.focus();
}

function applyFrequencyFileScope(): void {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    closeFrequencyFilesModal();
    return;
  }

  if (frequencySelectionCount(frequencyScopeDraft, corpus.documentCount) === 0) {
    renderFrequencyFilesModal();
    return;
  }

  frequencyDocumentScope = cloneFrequencySelection(frequencyScopeDraft);
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  closeFrequencyFilesModal();
  void refreshFrequencyPage();
}

function handleFrequencyFilesListChange(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLInputElement) || target.dataset.documentId === undefined) {
    return;
  }
  const documentId = Number.parseInt(target.dataset.documentId, 10);
  if (!Number.isFinite(documentId)) {
    return;
  }
  setFrequencySelectionDocument(frequencyScopeDraft, documentId, target.checked);
  renderFrequencyFilesModal();
}

function selectAllFrequencyFiles(): void {
  frequencyScopeDraft = defaultAnalysisDocumentSelection();
  void refreshFrequencyDocumentsPage(false);
}

function clearFrequencyFiles(): void {
  frequencyScopeDraft = { allDocuments: false, ids: [], excludedIds: [] };
  void refreshFrequencyDocumentsPage(false);
}

function showPreviousFrequencyDocumentsPage(): void {
  if (!frequencyDocumentsPage || frequencyDocumentsPage.offset === 0) {
    return;
  }
  frequencyDocumentsOffset = Math.max(
    0,
    frequencyDocumentsPage.offset - frequencyDocumentsPage.limit,
  );
  void refreshFrequencyDocumentsPage(false);
}

function showNextFrequencyDocumentsPage(): void {
  if (!frequencyDocumentsPage
    || frequencyDocumentsPage.offset + frequencyDocumentsPage.items.length >= frequencyDocumentsPage.totalCount) {
    return;
  }
  frequencyDocumentsOffset = frequencyDocumentsPage.offset + frequencyDocumentsPage.limit;
  void refreshFrequencyDocumentsPage(false);
}
function handleFrequencyModalKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && !dom.frequencyFilesModal.classList.contains("hidden")) {
    closeFrequencyFilesModal();
  }
}

export function initWorkbenchSwitcher(): void {
  window.addEventListener("corpuswright:project-sources-updated", () => {
    refreshAnalyseWorkbench();
  });

  dom.workbenchPrepare.addEventListener("click", () => {
    setActiveWorkbench("prepare");
  });
  dom.workbenchAnalyse.addEventListener("click", () => {
    setActiveWorkbench("analyse");
  });
  dom.workbenchPrepare.addEventListener("keydown", handleWorkbenchKeydown);
  dom.workbenchAnalyse.addEventListener("keydown", handleWorkbenchKeydown);

  dom.analyseBuildButton.addEventListener("click", () => {
    void handleBuildAnalysisCorpus();
  });
  dom.analyseToolsList.addEventListener("click", handleAnalysisToolClick);
  dom.frequencySearchInput.addEventListener("input", scheduleFrequencyRefresh);
  dom.frequencyMatchModeSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyMinInput.addEventListener("input", scheduleFrequencyRefresh);
  dom.frequencyGroupBySelect.addEventListener("change", () => {
    syncFrequencyAnnotationControls();
    syncFrequencyCaseFoldToggle();
    void refreshFrequencyPage();
  });
  syncFrequencyAnnotationControls();
  syncFrequencyCaseFoldToggle();
  dom.frequencyCaseFoldInput.addEventListener("change", () => {
    syncFrequencyCaseFoldToggle();
    void refreshFrequencyPage();
  });
  dom.frequencyPosFilterSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyTokenClassSelect.addEventListener("change", () => void refreshFrequencyPage());
  syncFrequencyStopwordsToggle();
  dom.frequencyHideStopwordsInput.addEventListener("change", () => {
    syncFrequencyStopwordsToggle();
    void refreshFrequencyPage();
  });
  dom.frequencyFilesButton.addEventListener("click", openFrequencyFilesModal);
  dom.frequencyTableHead.addEventListener("click", handleFrequencySortClick);
  dom.frequencyTableBody.addEventListener("click", handleFrequencyTableClick);
  dom.frequencyTableBody.addEventListener("keydown", handleFrequencyTableKeydown);
  dom.frequencyPagePreviousButton.addEventListener("click", showPreviousFrequencyPage);
  dom.frequencyPageNextButton.addEventListener("click", showNextFrequencyPage);
  dom.frequencyFilesSearchInput.addEventListener("input", scheduleFrequencyDocumentsRefresh);
  dom.frequencyFilesList.addEventListener("change", handleFrequencyFilesListChange);
  dom.frequencyFilesSelectAllButton.addEventListener("click", selectAllFrequencyFiles);
  dom.frequencyFilesClearAllButton.addEventListener("click", clearFrequencyFiles);
  dom.frequencyFilesPreviousButton.addEventListener("click", showPreviousFrequencyDocumentsPage);
  dom.frequencyFilesNextButton.addEventListener("click", showNextFrequencyDocumentsPage);
  dom.frequencyFilesApplyButton.addEventListener("click", applyFrequencyFileScope);
  dom.frequencyFilesCancelButton.addEventListener("click", closeFrequencyFilesModal);
  dom.frequencyFilesCloseButton.addEventListener("click", closeFrequencyFilesModal);
  document.addEventListener("keydown", handleFrequencyModalKeydown);

  void initialiseAnalysisBuildState();

  refreshStageNavigator();
  renderAnalysisTools();
  renderAnalysisToolPanels();
}
