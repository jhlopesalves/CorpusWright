import { listen } from "@tauri-apps/api/event";

import {
  buildAnalysisCorpus,
  cancelAnalysisBuild,
  defaultAnalysisDocumentSelection,
  getAnalysisBuildStatus,
  queryAnalysisDocuments,
  queryFrequencyList,
  queryTermDistribution,
  type AnalysisBalanceSummary as CorpusBalanceSummary,
  type AnalysisBuildProgress,
  type AnalysisDispersionSummary as DispersionSummary,
  type AnalysisDocumentPage,
  type AnalysisDocumentSelection,
  type AnalysisInputSource,
  type AnalysisLexicalSummary as LexicalNoiseSummary,
  type AnalysisOverview as AnalysisCorpus,
  type FrequencyMatchMode,
  type FrequencyPage,
  type FrequencySortKey,
  type FrequencyTermItem,
  type FrequencyTokenClass as FrequencyTokenClassFilter,
  type SortDirection,
  type TermDistributionPage,
} from "./analysis";
import { BOOLEAN_CLEANING_CONFIG_KEYS } from "./config";
import { dom } from "./dom";
import { sourceForPath, sourceHasExtractedText } from "./sources";
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
const FREQUENT_TERM_THRESHOLD = 10;
const HIGH_DISPERSION_THRESHOLD = 0.5;
const LARGE_DOCUMENT_SHARE_THRESHOLD = 0.25;
const LOW_EFFECTIVE_DOCUMENT_SHARE_THRESHOLD = 0.6;
const HIGH_SINGLE_CHARACTER_TYPE_SHARE_THRESHOLD = 0.1;
const HIGH_NUMERIC_ONLY_TYPE_SHARE_THRESHOLD = 0.1;
const OVERVIEW_EXPLANATIONS_STORAGE_KEY = "corpuswright.analysis.showExplanations";
type AnalysisToolId = "overview" | "frequency";

interface OverviewMetricRow {
  label: string;
  value: string;
  share?: string;
  title?: string;
}

interface CompositionSegment {
  label: string;
  count: number;
  className: string;
}

interface ReadingFinding {
  label: string;
  status: string;
  statusKind: "note" | "caution" | "fix" | "exploratory";
  text: string;
  explanation?: string;
}

interface CorpusReading {
  verdict: string;
  findings: ReadingFinding[];
  actions: HTMLElement[];
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
let overviewExplanationsVisible = readOverviewExplanationsVisible();

function readOverviewExplanationsVisible(): boolean {
  try {
    return window.localStorage.getItem(OVERVIEW_EXPLANATIONS_STORAGE_KEY) !== "false";
  } catch {
    return true;
  }
}

function persistOverviewExplanationsVisible(visible: boolean): void {
  try {
    window.localStorage.setItem(OVERVIEW_EXPLANATIONS_STORAGE_KEY, visible ? "true" : "false");
  } catch {
    // The toggle is still useful for the current session when storage is unavailable.
  }
}

function pluralise(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
}

function activeAnalysisToolName(): string {
  return ANALYSIS_TOOLS.find((tool) => tool.id === activeAnalysisTool)?.name ?? "Corpus Overview";
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

function isProcessedSourceForAnalysis(path: string): boolean {
  const source = sourceForPath(path);
  return source?.status === "processed" || source?.status === "processed_with_warnings";
}

function chooseAnalysisInputSource(): AnalysisInputSource {
  if (state.activeProject) {
    const total = state.projectSourceSummary?.total_count ?? 0;
    return total > 0 && processedSourceCount() === total ? "processed" : "extracted";
  }
  if (state.allFiles.length === 0) {
    return "extracted";
  }
  const allLoadedSourcesAreProcessed = state.allFiles.every((record) =>
    isProcessedSourceForAnalysis(record.source_path)
  );
  return allLoadedSourcesAreProcessed ? "processed" : "extracted";
}

function analysisBuildSignature(inputTextSource: AnalysisInputSource): string {
  return JSON.stringify({
    projectRoot: state.activeProject?.rootPath ?? null,
    corpusVersion: state.currentCorpusVersion,
    sourceManifestVersion: state.sourceManifestVersion,
    inputTextSource,
    cleaningConfig: state.activeCleaningConfig,
  });
}

function analysisIsStale(corpus: AnalysisCorpus | null = state.analysisCorpus): boolean {
  if (!corpus) {
    return false;
  }
  return corpus.buildSignature !== analysisBuildSignature(corpus.inputTextSource);
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

function formatOptionalNumber(value: number | null, maximumFractionDigits = 0): string {
  return value === null ? "n/a" : formatNumber(value, maximumFractionDigits);
}

function formatOptionalPercent(value: number | null): string {
  return value === null ? "n/a" : formatPercent(value);
}

function formatCountWithShare(count: number, total: number): string {
  if (total <= 0) {
    return `${formatNumber(count)} of ${formatNumber(total)}`;
  }
  return `${formatNumber(count)} of ${formatNumber(total)} (${formatPercent(count / total)})`;
}

function formatCountAndPercent(count: number, total: number): Pick<OverviewMetricRow, "value" | "share"> {
  return {
    value: `${formatNumber(count)} / ${formatNumber(total)}`,
    share: total > 0 ? formatPercent(count / total) : "n/a",
  };
}

function formatOptionalRange(start: number | null, end: number | null): string {
  return start === null || end === null
    ? "n/a"
    : `${formatNumber(start)} - ${formatNumber(end)}`;
}

function inputTextSourceLabel(inputTextSource: AnalysisInputSource): string {
  return inputTextSource === "processed" ? "processed text" : "native/extracted text";
}

function inputTextSourceDisplayLabel(inputTextSource: AnalysisInputSource): string {
  return inputTextSource === "processed" ? "Processed text" : "Native/extracted text";
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
  const enabledBooleanRules = BOOLEAN_CLEANING_CONFIG_KEYS
    .filter((key) => state.activeCleaningConfig[key])
    .length;
  const enabledStructuredRemovals = state.activeCleaningConfig.removal_rules
    .filter((rule) => rule.enabled)
    .length;
  return enabledBooleanRules
    + state.activeCleaningConfig.remove_patterns.length
    + enabledStructuredRemovals
    + state.activeCleaningConfig.replace_patterns.length;
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
    dom.analyseBuildNote.textContent = "Rebuild updates the Rust analysis index from current project text.";
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
  dom.analyseBuildNote.textContent = "Builds a Rust analysis index from loaded project text.";
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

  if (corpus.inputTextSource === "extracted") {
    dom.analyseInputBanner.textContent =
      "Exploratory — built from extracted/native text before cleaning.";
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

function createOverviewAction(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "secondary-btn analysis-action-button";
  button.textContent = label;
  button.addEventListener("click", onClick);
  return button;
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
    label.textContent = row.label;

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
  }

  table.appendChild(body);
  return table;
}

function createMetricGroup(titleText: string, rows: OverviewMetricRow[], className = ""): HTMLElement {
  const group = document.createElement("div");
  group.className = ["analysis-metric-group", className].filter(Boolean).join(" ");

  const title = document.createElement("h4");
  title.textContent = titleText;

  group.append(title, createMetricTable(rows));
  return group;
}

function createExplanation(text: string): HTMLParagraphElement {
  const explanation = document.createElement("p");
  explanation.className = "analysis-explanation";
  explanation.textContent = text;
  return explanation;
}

function createCompositionBar(
  title: string,
  total: number,
  segments: CompositionSegment[],
): HTMLElement {
  const composition = document.createElement("div");
  composition.className = "analysis-composition";

  const titleEl = document.createElement("h4");
  titleEl.textContent = title;
  composition.appendChild(titleEl);

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
      `${segment.label}: ${formatCountWithShare(segment.count, total)}`
    ).join("; "),
  );

  for (const segment of segments) {
    if (segment.count <= 0) {
      continue;
    }
    const fill = document.createElement("span");
    fill.className = `analysis-composition-segment ${segment.className}`;
    fill.style.setProperty("--segment-width", `${(segment.count / total) * 100}%`);
    bar.appendChild(fill);
  }

  const legend = document.createElement("dl");
  legend.className = "analysis-composition-legend";
  for (const segment of segments) {
    const item = document.createElement("div");
    item.className = `analysis-composition-legend-item ${segment.className}`;

    const label = document.createElement("dt");
    label.textContent = segment.label;

    const value = document.createElement("dd");
    value.textContent = formatCountWithShare(segment.count, total);

    item.append(label, value);
    legend.appendChild(item);
  }

  composition.append(bar, legend);
  return composition;
}

function setOverviewExplanationsVisible(visible: boolean): void {
  overviewExplanationsVisible = visible;
  persistOverviewExplanationsVisible(visible);
  renderCorpusOverview();
}

function extractionQualityNeedsCare(lexical: LexicalNoiseSummary): boolean {
  const numericSignal = lexical.numericOnlyShare !== null &&
    lexical.numericOnlyTypes >= 10 &&
    lexical.numericOnlyShare >= HIGH_NUMERIC_ONLY_TYPE_SHARE_THRESHOLD;
  const singleCharacterSignal = lexical.singleCharacterShare !== null &&
    lexical.singleCharacterTypes >= 10 &&
    lexical.singleCharacterShare >= HIGH_SINGLE_CHARACTER_TYPE_SHARE_THRESHOLD;
  return numericSignal || singleCharacterSignal;
}

function balanceNeedsCare(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): boolean {
  if (corpus.documentCount < 2 || corpus.totalTokens <= 0) {
    return false;
  }
  return (
    (balance.largestDocumentShare !== null &&
      balance.largestDocumentShare >= LARGE_DOCUMENT_SHARE_THRESHOLD) ||
    (balance.topDocumentShare !== null && balance.topDocumentShare >= 0.5) ||
    (balance.effectiveDocumentCount !== null &&
      balance.effectiveDocumentCount <= LOW_EFFECTIVE_DOCUMENT_SHARE_THRESHOLD * corpus.documentCount)
  );
}

function buildCorpusVerdict(
  corpus: AnalysisCorpus,
  balance: CorpusBalanceSummary,
  lexical: LexicalNoiseSummary,
  dispersion: DispersionSummary,
): string {
  if (corpus.documentCount === 0) {
    return "No analysis documents are available.";
  }
  if (corpus.totalTokens === 0) {
    return "Corpus built, but all token counts are zero.";
  }

  const balanceCaution = balanceNeedsCare(corpus, balance);
  const extractionCaution = extractionQualityNeedsCare(lexical);
  const dispersionCaution =
    dispersion.frequentTypes > 0 &&
    dispersion.highDispersionFrequentTypes / dispersion.frequentTypes >= 0.35;

  if (corpus.inputTextSource === "extracted") {
    return extractionCaution
      ? "Usable for exploration, but uncleaned extracted text needs inspection."
      : "Usable for exploration, but built from uncleaned extracted text.";
  }
  if (extractionCaution) {
    return "Usable for exploration, but inspect extraction-quality signals.";
  }
  if (balanceCaution && dispersionCaution) {
    return "Usable for exploration, but balance and dispersion need care.";
  }
  if (balanceCaution) {
    return "Usable for exploration, but document balance needs care.";
  }
  if (dispersionCaution) {
    return "Usable for exploration, but term dispersion needs care.";
  }

  return "Usable for exploration; no headline caution in these counts.";
}

function buildBalanceReading(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): ReadingFinding {
  if (corpus.documentCount === 0) {
    return {
      label: "Balance",
      status: "note",
      statusKind: "note",
      text: "No documents available for balance checks.",
    };
  }
  if (corpus.totalTokens === 0) {
    return {
      label: "Balance",
      status: "caution",
      statusKind: "caution",
      text: "All document token counts are zero.",
    };
  }
  if (corpus.documentCount === 1) {
    return {
      label: "Balance",
      status: "exploratory",
      statusKind: "exploratory",
      text: "Single-document corpus; balance cannot be assessed.",
      explanation: `The one document supplies all ${formatNumber(corpus.totalTokens)} tokens.`,
    };
  }

  const largestShare = balance.largestDocumentShare === null
    ? "n/a"
    : formatPercent(balance.largestDocumentShare);
  const topShare = balance.topDocumentShare === null
    ? "n/a"
    : formatPercent(balance.topDocumentShare);
  const text = `Largest document: ${largestShare}; top ${formatNumber(balance.topDocumentCount)}: ${topShare}.`;

  if (balanceNeedsCare(corpus, balance)) {
    return {
      label: "Balance",
      status: "caution",
      statusKind: "caution",
      text,
      explanation: "Uneven document lengths can let a few files dominate aggregate frequencies.",
    };
  }

  return {
    label: "Balance",
    status: "note",
    statusKind: "note",
    text,
    explanation: "Document-level checks are still useful when comparing terms across files.",
  };
}

function buildExtractionQualityReading(corpus: AnalysisCorpus, lexical: LexicalNoiseSummary): ReadingFinding {
  if (lexical.uniqueTypes === 0) {
    return {
      label: "Extraction quality",
      status: "note",
      statusKind: "note",
      text: "No token types counted.",
    };
  }

  const numericText = formatCountWithShare(lexical.numericOnlyTypes, lexical.uniqueTypes);
  const singleCharacterText = formatCountWithShare(lexical.singleCharacterTypes, lexical.uniqueTypes);
  const text = `Numeric-only: ${numericText}; single-character: ${singleCharacterText}.`;
  if (extractionQualityNeedsCare(lexical)) {
    return {
      label: "Extraction quality",
      status: "fix available",
      statusKind: "fix",
      text,
      explanation: "These token forms can come from tables, page numbers, references, formulae, or OCR leftovers.",
    };
  }
  if (corpus.inputTextSource === "extracted") {
    return {
      label: "Extraction quality",
      status: "exploratory",
      statusKind: "exploratory",
      text,
      explanation: "The corpus is built from extracted text rather than processed text, so inspect and clean before serious analysis.",
    };
  }

  return {
    label: "Extraction quality",
    status: "note",
    statusKind: "note",
    text,
    explanation: "Numeric-only and single-character types are useful data-quality checks, not lexical-richness measures.",
  };
}

function buildLexicalRichnessSummary(lexical: LexicalNoiseSummary): ReadingFinding {
  if (lexical.uniqueTypes === 0) {
    return {
      label: "Lexical richness",
      status: "note",
      statusKind: "note",
      text: "No token types were counted.",
    };
  }

  return {
    label: "Lexical richness",
    status: "note",
    statusKind: "note",
    text: `Hapax: ${formatCountWithShare(lexical.hapaxLegomena, lexical.uniqueTypes)}; dis legomena: ${formatCountWithShare(lexical.disLegomena, lexical.uniqueTypes)}.`,
    explanation: `Raw TTR is ${lexical.typeTokenRatio === null ? "n/a" : formatFixedNumber(lexical.typeTokenRatio, 3)} and is length-sensitive.`,
  };
}

function buildDispersionReading(corpus: AnalysisCorpus, dispersion: DispersionSummary): ReadingFinding {
  if (corpus.documentCount < 2) {
    return {
      label: "Dispersion",
      status: "exploratory",
      statusKind: "exploratory",
      text: "Needs at least two documents.",
    };
  }
  if (dispersion.frequentTypes === 0) {
    return {
      label: "Dispersion",
      status: "note",
      statusKind: "note",
      text: `No types reach frequency ${formatNumber(FREQUENT_TERM_THRESHOLD)}.`,
    };
  }

  const highDpText = formatCountWithShare(
    dispersion.highDispersionFrequentTypes,
    dispersion.frequentTypes,
  );
  if (dispersion.highDispersionFrequentTypes / dispersion.frequentTypes >= 0.35) {
    return {
      label: "Dispersion",
      status: "caution",
      statusKind: "caution",
      text: `High-DP frequent types: ${highDpText}.`,
      explanation: `DP at least ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)} means a frequent type is concentrated rather than evenly distributed.`,
    };
  }

  return {
    label: "Dispersion",
    status: "note",
    statusKind: "note",
    text: `High-DP frequent types: ${highDpText}.`,
    explanation: `DP at least ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)} marks frequent types concentrated in fewer documents.`,
  };
}

function buildCorpusReading(
  corpus: AnalysisCorpus,
  balance: CorpusBalanceSummary,
  lexical: LexicalNoiseSummary,
  dispersion: DispersionSummary,
): CorpusReading {
  const actions: HTMLElement[] = [];
  if (corpus.inputTextSource === "extracted" || extractionQualityNeedsCare(lexical)) {
    actions.push(createOverviewAction("Clean in Prepare", () => setActiveWorkbench("prepare")));
  }
  if (dispersion.frequentTypes > 0) {
    actions.push(createOverviewAction("Open Frequency List", () => setActiveAnalysisTool("frequency")));
  }

  return {
    verdict: buildCorpusVerdict(corpus, balance, lexical, dispersion),
    findings: [
      buildBalanceReading(corpus, balance),
      buildExtractionQualityReading(corpus, lexical),
      buildLexicalRichnessSummary(lexical),
      buildDispersionReading(corpus, dispersion),
    ],
    actions,
  };
}

function createOverviewExplanationsToggle(): HTMLElement {
  const label = document.createElement("label");
  label.className = "frequency-toggle analysis-explanations-toggle";

  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = overviewExplanationsVisible;
  input.addEventListener("change", () => {
    setOverviewExplanationsVisible(input.checked);
  });

  const track = document.createElement("span");
  track.className = "frequency-toggle-track";
  track.setAttribute("aria-hidden", "true");

  const text = document.createElement("span");
  text.textContent = "Show explanations";

  label.append(input, track, text);
  return label;
}

function renderCorpusReading(
  corpus: AnalysisCorpus,
  balance: CorpusBalanceSummary,
  lexical: LexicalNoiseSummary,
  dispersion: DispersionSummary,
): HTMLElement {
  const reading = buildCorpusReading(corpus, balance, lexical, dispersion);
  const panel = document.createElement("section");
  panel.className = "analysis-panel analysis-reading-panel";

  const header = document.createElement("div");
  header.className = "analysis-reading-header";

  const title = document.createElement("h3");
  title.textContent = "Reading of this corpus";

  header.append(title, createOverviewExplanationsToggle());

  const verdict = document.createElement("p");
  verdict.className = "analysis-reading-verdict";
  verdict.textContent = reading.verdict;

  const list = document.createElement("ul");
  list.className = "analysis-reading-findings";
  for (const finding of reading.findings) {
    const item = document.createElement("li");
    item.className = `analysis-reading-finding is-${finding.statusKind}`;

    const dot = document.createElement("span");
    dot.className = "analysis-reading-status-dot";
    dot.setAttribute("aria-hidden", "true");

    const body = document.createElement("div");
    body.className = "analysis-reading-finding-body";

    const heading = document.createElement("span");
    heading.className = "analysis-reading-finding-heading";

    const label = document.createElement("strong");
    label.textContent = finding.label;

    const status = document.createElement("span");
    status.className = "analysis-reading-status";
    status.textContent = ` - ${finding.status}`;

    heading.append(label, status);

    const text = document.createElement("span");
    text.className = "analysis-reading-finding-text";
    text.textContent = finding.text;

    body.append(heading, text);

    if (overviewExplanationsVisible && finding.explanation) {
      const explanation = document.createElement("p");
      explanation.className = "analysis-reading-finding-explanation";
      explanation.textContent = finding.explanation;
      body.appendChild(explanation);
    }

    item.append(dot, body);
    list.appendChild(item);
  }

  panel.append(header, verdict, list);

  if (reading.actions.length > 0) {
    const actions = document.createElement("div");
    actions.className = "analysis-action-row";
    actions.append(...reading.actions);
    panel.appendChild(actions);
  }

  return panel;
}

function renderCorpusSnapshot(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Corpus snapshot");
  panel.appendChild(createMetricTable([
    { label: "Tokens", value: formatNumber(corpus.totalTokens) },
    { label: "Unique types", value: formatNumber(corpus.totalTypes) },
    { label: "Documents", value: formatNumber(corpus.documentCount) },
    { label: "Input source", value: inputTextSourceDisplayLabel(corpus.inputTextSource) },
  ], "analysis-snapshot-table"));
  return panel;
}

function renderDocumentLengthDistribution(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): HTMLElement {
  const distribution = document.createElement("div");
  distribution.className = "document-length-distribution";

  const title = document.createElement("h4");
  title.textContent = corpus.documentLengthsTruncated ? "Longest documents" : "Document length distribution";
  distribution.appendChild(title);

  if (corpus.documentCount === 0) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = "No documents in the analysis corpus.";
    distribution.appendChild(empty);
    return distribution;
  }

  const maxTokens = balance.maxTokens ?? 0;
  for (const documentRecord of corpus.documentLengths) {
    const row = document.createElement("div");
    row.className = "document-length-row";
    row.title = documentName(documentRecord);

    const name = document.createElement("span");
    name.className = "document-length-name";
    name.textContent = documentName(documentRecord);

    const track = document.createElement("span");
    track.className = "document-length-track";
    const fill = document.createElement("span");
    fill.className = "document-length-fill";
    const barWidth = maxTokens > 0 && documentRecord.tokenCount > 0
      ? Math.max(3, (documentRecord.tokenCount / maxTokens) * 100)
      : 0;
    fill.style.setProperty("--bar-width", `${barWidth}%`);
    track.appendChild(fill);

    const tokenCount = document.createElement("span");
    tokenCount.className = "numeric document-length-count";
    tokenCount.textContent = formatNumber(documentRecord.tokenCount);

    const share = document.createElement("span");
    share.className = "numeric document-length-share";
    share.textContent = corpus.totalTokens > 0
      ? formatPercent(documentRecord.tokenCount / corpus.totalTokens)
      : "n/a";

    row.append(name, track, tokenCount, share);
    distribution.appendChild(row);
  }

  if (corpus.documentLengthsTruncated) {
    const note = document.createElement("p");
    note.className = "analysis-panel-note";
    note.textContent = `Showing the ${formatNumber(corpus.documentLengths.length)} longest documents; summary statistics cover all ${formatNumber(corpus.documentCount)} documents.`;
    distribution.appendChild(note);
  }

  return distribution;
}

function renderCorpusBalance(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): HTMLElement {
  const panel = createOverviewPanel("Corpus balance");
  panel.appendChild(renderDocumentLengthDistribution(corpus, balance));

  panel.appendChild(createMetricGroup("Descriptive statistics", [
    { label: "Median length", value: formatOptionalNumber(balance.medianTokens) },
    { label: "IQR (Q1-Q3)", value: formatOptionalRange(balance.q1Tokens, balance.q3Tokens) },
    { label: "Range (min-max)", value: formatOptionalRange(balance.minTokens, balance.maxTokens) },
    {
      label: "Largest / smallest ratio",
      value: balance.lengthRatio === null ? "n/a" : `${formatNumber(balance.lengthRatio, 1)}x`,
    },
    { label: "Largest document share", value: formatOptionalPercent(balance.largestDocumentShare) },
    {
      label: balance.topDocumentCount > 0
        ? `Top-${formatNumber(balance.topDocumentCount)} share`
        : "Top-3 share",
      value: formatOptionalPercent(balance.topDocumentShare),
    },
    {
      label: "Effective documents",
      value: balance.effectiveDocumentCount === null
        ? "n/a"
        : `${formatNumber(balance.effectiveDocumentCount, 1)} / ${formatNumber(corpus.documentCount)}`,
    },
    { label: "Documents", value: formatNumber(corpus.documentCount) },
  ], "analysis-balance-metrics"));

  if (overviewExplanationsVisible) {
    panel.appendChild(createExplanation(
      "Effective document count estimates how many evenly-sized documents this token distribution behaves like. It is computed from document token shares.",
    ));
  }

  return panel;
}

function renderLexicalRichness(lexical: LexicalNoiseSummary): HTMLElement {
  const panel = createOverviewPanel("Lexical richness");
  panel.appendChild(createCompositionBar("Vocabulary by frequency class", lexical.uniqueTypes, [
    { label: "Hapax", count: lexical.hapaxLegomena, className: "is-hapax" },
    { label: "Dis legomena", count: lexical.disLegomena, className: "is-dis" },
    { label: "3+ occurrences", count: lexical.repeatedTypes, className: "is-repeated" },
  ]));

  panel.appendChild(createMetricTable([
    { label: "Unique types", value: formatNumber(lexical.uniqueTypes) },
    {
      label: "Raw TTR",
      value: lexical.typeTokenRatio === null ? "n/a" : formatFixedNumber(lexical.typeTokenRatio, 3),
    },
  ], "analysis-compact-table"));

  if (overviewExplanationsVisible) {
    panel.appendChild(createExplanation(
      "Hapax legomena occur once; dis legomena occur twice. Raw TTR falls as corpus size grows. The frequency-class colour ramp is neutral: no class is better than another.",
    ));
  }

  return panel;
}

function renderExtractionQuality(lexical: LexicalNoiseSummary): HTMLElement {
  const panel = createOverviewPanel("Extraction quality");
  panel.appendChild(createCompositionBar("Type-form quality indicators", lexical.uniqueTypes, [
    { label: "Alphabetic & other", count: lexical.qualityOtherTypes, className: "is-quality-other" },
    { label: "Numeric-only", count: lexical.numericOnlyTypes, className: "is-numeric" },
    { label: "Single-character", count: lexical.singleCharacterTypes, className: "is-single" },
  ]));

  panel.appendChild(createMetricTable([
    {
      label: "Numeric-only types",
      ...formatCountAndPercent(lexical.numericOnlyTypes, lexical.uniqueTypes),
    },
    {
      label: "Single-character types",
      ...formatCountAndPercent(lexical.singleCharacterTypes, lexical.uniqueTypes),
    },
  ], "analysis-compact-table"));

  if (overviewExplanationsVisible) {
    panel.appendChild(createExplanation(
      "Numeric-only and single-character types are data-quality indicators, not lexical-richness measures. High shares may suggest page numbers, table fragments, references, formulae, or OCR leftovers.",
    ));
  }

  return panel;
}

function renderDispersionProfile(corpus: AnalysisCorpus, dispersion: DispersionSummary): HTMLElement {
  const panel = createOverviewPanel("Dispersion profile");

  if (dispersion.documentCount < 2) {
    const note = document.createElement("p");
    note.className = "analysis-panel-note";
    note.textContent = "Dispersion needs at least two documents.";
    panel.appendChild(note);
    return panel;
  }

  panel.appendChild(createMetricGroup(`Across all ${formatNumber(corpus.totalTypes)} types`, [
    {
      label: "Types in one document only",
      ...formatCountAndPercent(dispersion.oneDocumentTypes, corpus.totalTypes),
    },
    {
      label: `Types in all ${formatNumber(dispersion.documentCount)} documents`,
      ...formatCountAndPercent(dispersion.allDocumentTypes, corpus.totalTypes),
    },
    {
      label: `Frequent types (freq >= ${formatNumber(FREQUENT_TERM_THRESHOLD)})`,
      ...formatCountAndPercent(dispersion.frequentTypes, corpus.totalTypes),
    },
  ], "analysis-dispersion-group"));

  if (dispersion.frequentTypes === 0) {
    const note = document.createElement("p");
    note.className = "analysis-panel-note";
    note.textContent = `No terms have frequency at least ${formatNumber(FREQUENT_TERM_THRESHOLD)}; frequent-term DP statistics are unavailable.`;
    panel.appendChild(note);
  } else {
    panel.appendChild(createMetricGroup(`Among the ${formatNumber(dispersion.frequentTypes)} frequent types`, [
      {
        label: `High-DP types (DP >= ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)})`,
        ...formatCountAndPercent(dispersion.highDispersionFrequentTypes, dispersion.frequentTypes),
      },
      {
        label: "Median DP",
        value: dispersion.medianFrequentDispersion === null
          ? "n/a"
          : formatFixedNumber(dispersion.medianFrequentDispersion, 2),
      },
    ], "analysis-dispersion-group"));
  }

  if (overviewExplanationsVisible) {
    panel.appendChild(createExplanation(
      "DP close to 0 means a type is distributed according to document sizes; DP close to 1 means it is concentrated in fewer documents.",
    ));
  }

  return panel;
}

function renderCorpusOverview(): void {
  dom.analyseOverview.replaceChildren();
  const corpus = state.analysisCorpus;
  if (!corpus) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = analysisNotice?.message ??
      "Build an analysis corpus to see corpus size, document balance, lexical richness, extraction quality, and dispersion diagnostics.";
    dom.analyseOverview.appendChild(empty);
    return;
  }

  const balance = corpus.balance;
  const lexical = corpus.lexical;
  const dispersion = corpus.dispersion;

  dom.analyseOverview.append(
    renderCorpusReading(corpus, balance, lexical, dispersion),
    renderCorpusSnapshot(corpus),
    renderCorpusBalance(corpus, balance),
    renderLexicalRichness(lexical),
    renderExtractionQuality(lexical),
    renderDispersionProfile(corpus, dispersion),
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

function currentFrequencyTokenClassFilter(): FrequencyTokenClassFilter {
  return dom.frequencyTokenClassSelect.value as FrequencyTokenClassFilter;
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
      offset,
      TERM_DISTRIBUTION_PAGE_SIZE,
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
    rate.textContent = formatFixedNumber(item.ratePer10k, 2);

    const track = document.createElement("span");
    track.className = "frequency-distribution-track";
    const fill = document.createElement("span");
    fill.className = "frequency-distribution-fill";
    const barWidth = page.maxRatePer10k > 0 && item.ratePer10k > 0
      ? Math.max(3, (item.ratePer10k / page.maxRatePer10k) * 100)
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
    const page = await queryFrequencyList({
      selection: frequencyDocumentScope,
      query: dom.frequencySearchInput.value.trim(),
      matchMode: dom.frequencyMatchModeSelect.value as FrequencyMatchMode,
      minimumFrequency: Math.max(1, Number.parseInt(dom.frequencyMinInput.value, 10) || 1),
      tokenClass: currentFrequencyTokenClassFilter(),
      hideStopwords: dom.frequencyHideStopwordsInput.checked,
      sortKey: frequencySortKey,
      sortDirection: frequencySortDirection,
      offset: frequencyPageOffset,
      limit: FREQUENCY_PAGE_SIZE,
    });
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
      : "Querying the Rust analysis index...";
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
    const caret = document.createElement("span");
    caret.className = "frequency-token-caret";
    caret.setAttribute("aria-hidden", "true");
    caret.textContent = isExpanded ? "▾" : "▸";
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
    rate.textContent = formatFixedNumber(term.ratePer10k, 2);
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
  const inputTextSource = chooseAnalysisInputSource();
  const buildSignature = analysisBuildSignature(inputTextSource);

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
    const overview = await buildAnalysisCorpus(
      indices,
      state.currentCorpusVersion,
      inputTextSource,
      buildSignature,
      state.activeCleaningConfig,
    );
    if (analysisBuildGeneration !== buildGeneration) {
      return;
    }
    state.analysisCorpus = overview;
    resetFrequencyCorpusState();
    activeAnalysisTool = "overview";
    const sourceMessage = inputTextSource === "processed"
      ? "Built from processed text."
      : "Exploratory analysis built from extracted/native text because processed text is not available yet.";
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
      `Analysis corpus built from ${inputTextSource} text. ${formatNumber(overview.documentCount)} documents · ${formatNumber(overview.totalTokens)} tokens · ${formatNumber(overview.totalTypes)} types.`;
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
    const status = await getAnalysisBuildStatus();
    analysisBuildInProgress = status.state === "building";
    analysisBuildProgress = status.progress;
    if (status.state === "ready" && status.overview && !state.analysisCorpus) {
      state.analysisCorpus = status.overview;
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
  dom.frequencyTokenClassSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyHideStopwordsInput.addEventListener("change", () => void refreshFrequencyPage());
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
