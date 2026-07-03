import katex from "katex";
import "katex/dist/katex.min.css";

import {
  expandConcordanceContext,
  queryConcordanceCollocations,
  searchConcordance,
  type AnalysisInputSource,
  type ConcordanceContextFilterDirection,
  type ConcordanceExpansionResult,
  type ConcordanceCollocateRow,
  type ConcordanceCollocationDirection,
  type ConcordanceCollocationMeasure,
  type ConcordanceCollocationResult,
  type ConcordanceCollocationSortKey,
  type ConcordanceResult,
  type ConcordanceRow,
  type ConcordanceSortKey,
  type CorpusOverview as AnalysisCorpus,
  type CorpusOverviewFinding,
  type CorpusOverviewFindingSeverity,
  type SortDirection,
} from "./analysis";
import { BOOLEAN_CLEANING_CONFIG_KEYS } from "./config";
import {
  clampConcordanceWindow,
  clampConcordanceCollocationFloor,
  clampConcordanceCollocationSpan,
  clampConcordancePageSize,
  concordanceCollocationExplanation,
  concordanceCollocationMeasureLabel,
  concordanceCollocationScore,
  concordanceCollocationScoreBarPercent,
  concordanceCollocationSortHeaderLabel,
  concordanceContextTokenSegments,
  concordanceExpansionViewModel,
  concordanceSummaryMetricsText,
  concordanceRowExpansionKey,
  concordanceRowText,
  CONCORDANCE_COLLOCATION_LIMIT,
  CONCORDANCE_COLLOCATION_SORT_OPTIONS,
  CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE,
  CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE,
  CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE,
  CONCORDANCE_COLLOCATION_STALE_MESSAGE,
  CONCORDANCE_EXPANSION_WINDOW,
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  DEFAULT_CONCORDANCE_COLLOCATION_MEASURE,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY,
  DEFAULT_CONCORDANCE_SORT_KEY,
  CONCORDANCE_COLUMNS,
  toggleConcordanceExpansionKey,
  type ConcordanceExpansionStateSnapshot,
  normaliseConcordanceQuery,
} from "./concordance";
import { dom } from "./dom";
import { effectiveCleaningConfig } from "./effective-cleaning-config";
import { DEFAULT_FREQUENCY_RATE_SCALE } from "./frequency";
import {
  overviewGlossary,
  type OverviewGlossaryEntry,
  type OverviewGlossaryId,
} from "./overview-glossary";
import type { ProjectLanguage } from "./generated/ProjectLanguage.js";
import { projectLanguageLabel } from "./project-creation-dialog";
import { sourceHasExtractedText } from "./sources";
import { state, type WorkbenchId } from "./state";
import {
  analysisStatus,
  currentAnalysisNotice,
  handleAnnotationLayerAction,
  handleBuildAnalysisCorpus,
  initialiseAnalysisBuildState,
  renderAnalyseStatus,
  renderAnalysisNotice,
  resetAnalysisBuildStateForWorkbench,
} from "./workbench/build";
import {
  applyFrequencyFileScope,
  clearFrequencyFiles,
  closeFrequencyFilesModal,
  currentFrequencyDocumentScope,
  handleFrequencyFilesListChange,
  handleFrequencyModalKeydown,
  handleFrequencyModeClick,
  handleFrequencyRateScaleChange,
  handleFrequencySortClick,
  handleFrequencyTableClick,
  handleFrequencyTableKeydown,
  openFrequencyFilesModal,
  refreshFrequencyPage,
  renderFrequencyList,
  renderFrequencyScopeControl,
  resetFrequencyCorpusState,
  scheduleFrequencyDocumentsRefresh,
  scheduleFrequencyRefresh,
  selectAllFrequencyFiles,
  showNextFrequencyDocumentsPage,
  showNextFrequencyPage,
  showPreviousFrequencyDocumentsPage,
  showPreviousFrequencyPage,
  syncFrequencyAnnotationControls,
  syncFrequencyCaseFoldToggle,
  syncFrequencyStopwordsToggle,
} from "./workbench/frequency";
import {
  CONCORDANCE_DISTRIBUTION_PAGE_SIZE,
  currentConcordanceDistributionOffset,
  renderConcordanceDistribution,
  resetConcordanceDistributionOffset,
} from "./workbench/concordance/distribution";

const FUTURE_ANALYSIS_TOOLS = [
  "N-grams",
  "Exports",
];
const CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT = 8;
type AnalysisToolId = "overview" | "frequency" | "concordance";
type ConcordanceViewId = "lines" | "collocates" | "distribution";
type ConcordanceCollocationState = "idle_not_computed" | "loading" | "ready" | "stale" | "error";

type CollocateEvidenceSelection = {
  surface: string;
  query: string;
  caseSensitive: boolean;
  span: number;
  direction: ConcordanceContextFilterDirection;
};

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
  { id: "concordance", name: "Concordance", panelId: "concordance-pane" },
];

let activeAnalysisTool: AnalysisToolId = "overview";
let overviewGlossaryControlId = 0;
let concordanceResult: ConcordanceResult | null = null;
let activeConcordanceView: ConcordanceViewId = "lines";
let concordanceOffset = 0;
let concordanceSortKey: ConcordanceSortKey = DEFAULT_CONCORDANCE_SORT_KEY;
let concordanceQueryGeneration = 0;
let concordancePending = false;
let concordanceFailure: string | null = null;
let concordanceExpansionKey: string | null = null;
let concordanceExpansionPending = false;
let concordanceExpansionFailure: string | null = null;
let concordanceExpansionResult: ConcordanceExpansionResult | null = null;
let concordanceExpansionGeneration = 0;
let concordanceCollocationState: ConcordanceCollocationState = "idle_not_computed";
let concordanceCollocationResult: ConcordanceCollocationResult | null = null;
let concordanceCollocationPending = false;
let concordanceCollocationFailure: string | null = null;
let concordanceCollocationGeneration = 0;
let concordanceCollocationFreshAggregationKey: string | null = null;
let concordanceCollocationEverComputed = false;
let concordanceCollocationAutoComputedAggregationKeys = new Set<string>();
let concordanceCollocationInFlightRequestKey: string | null = null;
let concordanceCollocationSupersededNotice: string | null = null;
let concordanceCollocationMeasure: ConcordanceCollocationMeasure =
  DEFAULT_CONCORDANCE_COLLOCATION_MEASURE;
let concordanceCollocationSortKey: ConcordanceCollocationSortKey =
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY;
let concordanceCollocationSortDirection: SortDirection =
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION;
let collocateEvidenceSelection: CollocateEvidenceSelection | null = null;
let collocateEvidenceResult: ConcordanceResult | null = null;
let collocateEvidencePending = false;
let collocateEvidenceFailure: string | null = null;
let collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;
let collocateEvidenceGeneration = 0;
let collocateEvidenceExpansionKey: string | null = null;
let collocateEvidenceExpansionPending = false;
let collocateEvidenceExpansionFailure: string | null = null;
let collocateEvidenceExpansionResult: ConcordanceExpansionResult | null = null;
let collocateEvidenceExpansionGeneration = 0;

export function pluralise(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
}

function activeAnalysisToolName(): string {
  return ANALYSIS_TOOLS.find((tool) => tool.id === activeAnalysisTool)?.name ?? "Corpus Overview";
}

export function activeProjectLanguageCode(): ProjectLanguage | null {
  return state.activeProject?.language ?? null;
}

export function activeProjectLanguageLabel(): string {
  const language = activeProjectLanguageCode();
  return language ? projectLanguageLabel(language) : "Not available";
}

export function processedSourceCount(): number {
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

export function formatNumber(value: number, maximumFractionDigits = 0): string {
  return value.toLocaleString(undefined, { maximumFractionDigits });
}

export function formatFixedNumber(value: number, fractionDigits: number): string {
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

export function inputTextSourceLabel(inputTextSource: AnalysisInputSource): string {
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

export function refreshStageNavigator(): void {
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
  dom.concordanceSection.classList.toggle("hidden", activeAnalysisTool !== "concordance");
  dom.analyseWorkbench.classList.toggle("is-concordance-active", activeAnalysisTool === "concordance");
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
    empty.textContent = "No documents in the surface analysis.";
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
    empty.textContent = currentAnalysisNotice()?.message ??
      "Build surface analysis to see the first-stage corpus diagnostics.";
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

function currentConcordanceWindow(): number {
  return clampConcordanceWindow(Number.parseInt(dom.concordanceWindowInput.value, 10));
}

function currentConcordancePageSize(): number {
  const pageSize = clampConcordancePageSize(Number.parseInt(dom.concordancePageSizeInput.value, 10));
  dom.concordancePageSizeInput.value = pageSize.toString();
  return pageSize;
}

function currentConcordanceSortKey(): ConcordanceSortKey {
  return dom.concordanceSortSelect.value as ConcordanceSortKey;
}

function currentConcordanceCollocationSpan(): number {
  const span = clampConcordanceCollocationSpan(
    Number.parseInt(dom.concordanceCollocationSpanInput.value, 10),
  );
  dom.concordanceCollocationSpanInput.value = span.toString();
  return span;
}

function currentConcordanceCollocationDirection(): ConcordanceCollocationDirection {
  return dom.concordanceCollocationDirectionSelect.value as ConcordanceCollocationDirection;
}

function currentConcordanceCollocationMeasure(): ConcordanceCollocationMeasure {
  return concordanceCollocationMeasure;
}

function currentConcordanceCollocationSortDirection(): SortDirection {
  return concordanceCollocationSortDirection;
}

function currentConcordanceCollocationMinO11(): number {
  const floor = clampConcordanceCollocationFloor(
    Number.parseInt(dom.concordanceCollocationMinO11Input.value, 10),
  );
  dom.concordanceCollocationMinO11Input.value = floor.toString();
  return floor;
}

function currentConcordanceCollocationMinFrequency(): number {
  const floor = clampConcordanceCollocationFloor(
    Number.parseInt(dom.concordanceCollocationMinFreqInput.value, 10),
  );
  dom.concordanceCollocationMinFreqInput.value = floor.toString();
  return floor;
}

function currentConcordanceCollocationAggregationKey(): string | null {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (query.length === 0) {
    return null;
  }
  return JSON.stringify({
    corpusVersion: state.currentCorpusVersion,
    cleaningConfig: effectiveCleaningConfig(),
    selection: currentFrequencyDocumentScope(),
    query,
    caseSensitive: dom.concordanceCaseSensitiveInput.checked,
    span: currentConcordanceCollocationSpan(),
    direction: currentConcordanceCollocationDirection(),
  });
}

function currentConcordanceCollocationRequestKey(): string | null {
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  if (aggregationKey === null) {
    return null;
  }
  return JSON.stringify({
    aggregationKey,
    minCooccurrence: currentConcordanceCollocationMinO11(),
    minCollocateFrequency: currentConcordanceCollocationMinFrequency(),
    hideStopwords: dom.concordanceCollocationHideStopwordsInput.checked,
    sortMeasure: concordanceCollocationMeasure,
    sortKey: concordanceCollocationSortKey,
    sortDirection: currentConcordanceCollocationSortDirection(),
    limit: CONCORDANCE_COLLOCATION_LIMIT,
  });
}

function currentConcordanceCollocationResultIsFresh(): boolean {
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  return aggregationKey !== null
    && concordanceCollocationFreshAggregationKey === aggregationKey
    && concordanceCollocationResult !== null
    && concordanceCollocationState === "ready";
}

function clearCollocateEvidence(): void {
  collocateEvidenceSelection = null;
  collocateEvidenceResult = null;
  collocateEvidencePending = false;
  collocateEvidenceFailure = null;
  collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;
  collocateEvidenceGeneration += 1;
  collocateEvidenceExpansionKey = null;
  collocateEvidenceExpansionPending = false;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  collocateEvidenceExpansionGeneration += 1;
}

function resetCollocateEvidenceForQueryChange(): void {
  clearCollocateEvidence();
}

function syncConcordanceCaseToggle(): void {
  const isCaseSensitive = dom.concordanceCaseSensitiveInput.checked;
  dom.concordanceCaseSensitiveInput.setAttribute("aria-checked", isCaseSensitive ? "true" : "false");
  dom.concordanceCaseState.textContent = isCaseSensitive ? "Sensitive" : "Insensitive";
}

function syncConcordanceCollocationStopwordsToggle(): void {
  const hidden = dom.concordanceCollocationHideStopwordsInput.checked;
  dom.concordanceCollocationHideStopwordsInput.setAttribute("aria-checked", hidden ? "true" : "false");
  dom.concordanceCollocationStopwordState.textContent = hidden ? "Hidden" : "Show";
}

function syncConcordanceControls(): void {
  syncConcordanceCaseToggle();
  syncConcordanceCollocationStopwordsToggle();
  dom.concordanceSortSelect.value = concordanceSortKey;
  dom.concordanceCollocationMeasureSelect.value = concordanceCollocationMeasure;
}

export function markConcordanceCollocationsForAggregationChange(): void {
  resetCollocateEvidenceForQueryChange();
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  const shouldShowStaleCallout = activeConcordanceView === "collocates" && concordanceCollocationEverComputed;
  if (
    aggregationKey !== null
    && concordanceCollocationFreshAggregationKey === aggregationKey
    && concordanceCollocationResult !== null
  ) {
    concordanceCollocationState = "ready";
    concordanceCollocationFailure = null;
    concordanceCollocationSupersededNotice = null;
    renderConcordanceCollocations();
    return;
  }

  concordanceCollocationGeneration += 1;
  concordanceCollocationPending = false;
  concordanceCollocationInFlightRequestKey = null;
  concordanceCollocationFailure = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationResult = null;

  if (aggregationKey === null) {
    concordanceCollocationState = "idle_not_computed";
    concordanceCollocationFreshAggregationKey = null;
  } else {
    concordanceCollocationState = shouldShowStaleCallout ? "stale" : "idle_not_computed";
  }

  renderConcordanceCollocations();
}

function noteSupersededConcordanceCollocationResult(): void {
  if (concordanceCollocationState !== "stale") {
    return;
  }
  concordanceCollocationSupersededNotice =
    "An older collocate result was ignored because the settings changed.";
  renderConcordanceCollocations();
}

function refreshConcordanceCollocationsForPresentationChange(): void {
  if (
    concordanceCollocationPending
    && currentConcordanceCollocationRequestKey() !== concordanceCollocationInFlightRequestKey
  ) {
    void refreshConcordanceCollocations();
    return;
  }
  if (currentConcordanceCollocationResultIsFresh()) {
    void refreshConcordanceCollocations();
  } else {
    renderConcordanceCollocations();
  }
}

function maybeAutoComputeConcordanceCollocations(): void {
  if (activeConcordanceView !== "collocates") {
    return;
  }
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  if (currentConcordanceCollocationResultIsFresh()) {
    renderConcordanceCollocations();
    return;
  }
  if (
    concordanceCollocationState === "idle_not_computed"
    && aggregationKey !== null
    && !concordanceCollocationAutoComputedAggregationKeys.has(aggregationKey)
  ) {
    concordanceCollocationAutoComputedAggregationKeys.add(aggregationKey);
    void refreshConcordanceCollocations();
    return;
  }
  renderConcordanceCollocations();
}

function isConcordanceViewId(value: string | undefined): value is ConcordanceViewId {
  return value === "lines" || value === "collocates" || value === "distribution";
}

function syncConcordanceViewTabs(): void {
  const buttons = dom.concordanceViewTabs.querySelectorAll<HTMLButtonElement>("[data-concordance-view]");
  buttons.forEach((button) => {
    const view = button.dataset.concordanceView;
    if (!isConcordanceViewId(view)) {
      return;
    }
    const active = view === activeConcordanceView;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  });

  const panels: Array<{ view: ConcordanceViewId; element: HTMLElement }> = [
    { view: "lines", element: dom.concordanceLinesPanel },
    { view: "collocates", element: dom.concordanceCollocationsPanel },
    { view: "distribution", element: dom.concordanceDistributionPanel },
  ];
  panels.forEach(({ view, element }) => {
    const active = view === activeConcordanceView;
    element.classList.toggle("hidden", !active);
    element.setAttribute("aria-hidden", active ? "false" : "true");
  });

  if (activeConcordanceView !== "lines") {
    dom.concordanceStatus.classList.add("hidden");
  } else if (concordanceResult) {
    renderConcordanceStatus(null);
  }
}

function setActiveConcordanceView(view: ConcordanceViewId): void {
  activeConcordanceView = view;
  syncConcordanceViewTabs();
}

function handleConcordanceViewTabClick(event: MouseEvent): void {
  const target = event.target instanceof HTMLElement
    ? event.target.closest<HTMLButtonElement>("[data-concordance-view]")
    : null;
  const view = target?.dataset.concordanceView;
  if (!isConcordanceViewId(view)) {
    return;
  }
  setActiveConcordanceView(view);
  if (view === "collocates") {
    maybeAutoComputeConcordanceCollocations();
  }
}

function renderConcordanceStatus(message: string | null, isError = false): void {
  dom.concordanceStatus.replaceChildren(message ?? "");
  dom.concordanceStatus.classList.toggle("hidden", message === null);
  dom.concordanceStatus.classList.toggle("warning", isError);
}

function renderConcordanceCollocationStatus(content: string | Node | null, isError = false): void {
  dom.concordanceCollocationsStatus.replaceChildren();
  if (typeof content === "string") {
    dom.concordanceCollocationsStatus.textContent = content;
  } else if (content) {
    dom.concordanceCollocationsStatus.appendChild(content);
  }
  dom.concordanceCollocationsStatus.classList.toggle("hidden", content === null);
  dom.concordanceCollocationsStatus.classList.toggle("warning", isError);
}

function renderConcordanceCollocationAction(
  message: string,
  detail: string | null,
  label: string,
): HTMLElement {
  const prompt = document.createElement("div");
  prompt.className = "concordance-collocation-action";
  const copy = document.createElement("span");
  copy.className = "concordance-collocation-action-copy";
  copy.textContent = detail ? `${message} ${detail}` : message;
  const action = document.createElement("button");
  action.type = "button";
  action.className = "secondary-btn concordance-collocation-action-button";
  action.textContent = label;
  action.disabled = concordanceCollocationPending;
  action.addEventListener("click", () => {
    void refreshConcordanceCollocations();
  });
  prompt.append(copy, action);
  return prompt;
}

function currentConcordanceExpansionState(): ConcordanceExpansionStateSnapshot {
  return {
    key: concordanceExpansionKey,
    pending: concordanceExpansionPending,
    error: concordanceExpansionFailure,
    result: concordanceExpansionResult,
  };
}

function collapseConcordanceExpansion(): void {
  if (
    concordanceExpansionKey === null
    && !concordanceExpansionPending
    && concordanceExpansionFailure === null
    && concordanceExpansionResult === null
  ) {
    return;
  }
  concordanceExpansionKey = null;
  concordanceExpansionPending = false;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  concordanceExpansionGeneration += 1;
}

function handleConcordanceExpansionEscape(event: KeyboardEvent): void {
  if (event.key !== "Escape" || concordanceExpansionKey === null) {
    return;
  }
  collapseConcordanceExpansion();
  renderConcordance();
}

function concordanceExpansionDomId(rowKey: string): string {
  return `concordance-expansion-${rowKey.replace(/[^a-z0-9_-]/gi, "-")}`;
}

function collocateEvidenceExpansionDomId(rowKey: string): string {
  return `concordance-collocate-evidence-expansion-${rowKey.replace(/[^a-z0-9_-]/gi, "-")}`;
}

async function toggleConcordanceExpansion(row: ConcordanceRow): Promise<void> {
  const rowKey = concordanceRowExpansionKey(row);
  const nextKey = toggleConcordanceExpansionKey(concordanceExpansionKey, rowKey);
  if (nextKey === null) {
    collapseConcordanceExpansion();
    renderConcordance();
    return;
  }

  concordanceExpansionKey = nextKey;
  concordanceExpansionPending = true;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  const generation = ++concordanceExpansionGeneration;
  renderConcordance();

  try {
    const result = await expandConcordanceContext(
      {
        documentIndex: row.documentIndex,
        tokenIndex: row.tokenIndex,
        nodeTokenLength: row.nodeTokenLength,
        windowTokens: CONCORDANCE_EXPANSION_WINDOW,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== concordanceExpansionGeneration) {
      return;
    }
    concordanceExpansionResult = result;
  } catch (error) {
    if (generation !== concordanceExpansionGeneration) {
      return;
    }
    concordanceExpansionFailure = `${error}`;
  } finally {
    if (generation === concordanceExpansionGeneration) {
      concordanceExpansionPending = false;
      renderConcordance();
    }
  }
}

async function toggleCollocateEvidenceExpansion(row: ConcordanceRow): Promise<void> {
  const rowKey = concordanceRowExpansionKey(row);
  const nextKey = toggleConcordanceExpansionKey(collocateEvidenceExpansionKey, rowKey);
  if (nextKey === null) {
    collocateEvidenceExpansionKey = null;
    collocateEvidenceExpansionPending = false;
    renderConcordanceCollocations();
    return;
  }

  collocateEvidenceExpansionKey = nextKey;
  collocateEvidenceExpansionPending = true;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  const generation = ++collocateEvidenceExpansionGeneration;
  renderConcordanceCollocations();

  try {
    const result = await expandConcordanceContext(
      {
        documentIndex: row.documentIndex,
        tokenIndex: row.tokenIndex,
        nodeTokenLength: row.nodeTokenLength,
        windowTokens: CONCORDANCE_EXPANSION_WINDOW,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== collocateEvidenceExpansionGeneration) {
      return;
    }
    collocateEvidenceExpansionResult = result;
  } catch (error) {
    if (generation !== collocateEvidenceExpansionGeneration) {
      return;
    }
    collocateEvidenceExpansionFailure = `${error}`;
  } finally {
    if (generation === collocateEvidenceExpansionGeneration) {
      collocateEvidenceExpansionPending = false;
      renderConcordanceCollocations();
    }
  }
}

function renderConcordanceTableState(message: string): void {
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = CONCORDANCE_COLUMNS.length;
  cell.className = "frequency-empty concordance-empty";
  cell.textContent = message;
  row.appendChild(cell);
  dom.concordanceTableBody.replaceChildren(row);
}

function renderContextTokens(
  tokens: string[],
  activeFilter: string | null,
  caseSensitive: boolean,
): DocumentFragment {
  const fragment = document.createDocumentFragment();
  const segments = concordanceContextTokenSegments(tokens, activeFilter, caseSensitive);
  segments.forEach((segment, index) => {
    if (index > 0) {
      fragment.appendChild(document.createTextNode(" "));
    }
    if (!segment.collocate) {
      fragment.appendChild(document.createTextNode(segment.text));
      return;
    }
    const token = document.createElement("span");
    token.className = "concordance-collocate";
    token.textContent = segment.text;
    fragment.appendChild(token);
  });
  return fragment;
}

function appendConcordanceKwicCells(
  item: HTMLTableRowElement,
  row: ConcordanceRow,
  activeFilter: string | null,
  caseSensitive: boolean,
): void {
  const file = document.createElement("td");
  file.className = "concordance-file";
  file.title = row.displayName || row.sourcePath;
  file.textContent = row.displayName || row.sourcePath;

  const left = document.createElement("td");
  left.className = "concordance-left";
  if (activeFilter === null) {
    left.textContent = concordanceRowText(row.leftContext);
  } else {
    left.appendChild(renderContextTokens(row.leftContext, activeFilter, caseSensitive));
  }

  const nodeCell = document.createElement("td");
  nodeCell.className = "concordance-node-cell";
  const node = document.createElement("mark");
  node.className = "concordance-node";
  node.textContent = row.node;
  nodeCell.appendChild(node);

  const right = document.createElement("td");
  right.className = "concordance-right";
  if (activeFilter === null) {
    right.textContent = concordanceRowText(row.rightContext);
  } else {
    right.appendChild(renderContextTokens(row.rightContext, activeFilter, caseSensitive));
  }

  item.append(file, left, nodeCell, right);
}

function createConcordanceRow(row: ConcordanceRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  const rowKey = concordanceRowExpansionKey(row);
  const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());
  const activeContextFilter = concordanceResult?.contextFilter ?? null;
  const contextFilterCaseSensitive = concordanceResult?.caseSensitive ?? false;
  item.className = "concordance-row";
  item.tabIndex = 0;
  item.setAttribute("role", "button");
  item.setAttribute("aria-expanded", expansion.ariaExpanded);
  item.setAttribute("aria-controls", concordanceExpansionDomId(rowKey));
  item.title = "Show wider context";
  item.addEventListener("click", () => {
    void toggleConcordanceExpansion(row);
  });
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    void toggleConcordanceExpansion(row);
  });
  item.classList.toggle("is-expanded", expansion.expanded);

  appendConcordanceKwicCells(item, row, activeContextFilter, contextFilterCaseSensitive);
  return item;
}

function createConcordanceSourcePreviewPanel(row: ConcordanceRow): HTMLDivElement {
  const rowKey = concordanceRowExpansionKey(row);
  const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());

  const panel = document.createElement("div");
  panel.id = concordanceExpansionDomId(rowKey);
  panel.className = "concordance-expansion-panel";

  const header = document.createElement("div");
  header.className = "concordance-expansion-header";
  const file = document.createElement("span");
  file.className = "concordance-expansion-file";
  file.textContent = row.displayName || row.sourcePath;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "secondary-btn concordance-expansion-close";
  close.textContent = "Close";
  close.addEventListener("click", () => {
    collapseConcordanceExpansion();
    renderConcordance();
  });
  header.append(file, close);
  panel.appendChild(header);

  if (expansion.status === "ready") {
    const context = document.createElement("p");
    context.className = "concordance-expansion-context";
    const left = document.createElement("span");
    left.className = "concordance-expansion-left";
    const activeFilter = concordanceResult?.contextFilter ?? null;
    const caseSensitive = concordanceResult?.caseSensitive ?? false;
    if (concordanceExpansionResult !== null) {
      left.appendChild(renderContextTokens(concordanceExpansionResult.leftContext, activeFilter, caseSensitive));
    } else {
      left.textContent = expansion.leftText;
    }
    const node = document.createElement("mark");
    node.className = "concordance-node concordance-expansion-node";
    node.textContent = expansion.nodeText;
    const right = document.createElement("span");
    right.className = "concordance-expansion-right";
    if (concordanceExpansionResult !== null) {
      right.appendChild(renderContextTokens(concordanceExpansionResult.rightContext, activeFilter, caseSensitive));
    } else {
      right.textContent = expansion.rightText;
    }
    context.append(left, node, right);
    panel.appendChild(context);
  } else {
    const message = document.createElement("p");
    message.className = `concordance-expansion-message is-${expansion.status}`;
    message.textContent = expansion.message;
    panel.appendChild(message);
  }

  return panel;
}

function createConcordanceSourcePreviewRow(row: ConcordanceRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-preview-row";

  const cell = document.createElement("td");
  cell.className = "concordance-preview-cell";
  cell.colSpan = CONCORDANCE_COLUMNS.length;
  cell.appendChild(createConcordanceSourcePreviewPanel(row));

  item.appendChild(cell);
  return item;
}

function renderConcordanceSummary(result: ConcordanceResult): void {
  const query = document.createElement("span");
  query.className = "concordance-summary-query";
  query.textContent = result.query;
  const metrics = document.createElement("span");
  metrics.className = "concordance-summary-scope";
  metrics.textContent = ` · ${concordanceSummaryMetricsText(result)}`;
  dom.concordanceSummary.replaceChildren(query, metrics);
}

function renderConcordanceCollocationTableState(message: string): void {
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = 4;
  cell.className = "frequency-empty concordance-empty";
  cell.textContent = message;
  row.appendChild(cell);
  dom.concordanceCollocationsTableBody.replaceChildren(row);
}

function formatConcordanceCollocationScore(value: number | null): string {
  return value === null ? "n/a" : formatFixedNumber(value, 2);
}

function appendConcordanceCollocationHeader(
  row: HTMLTableRowElement,
  label: string,
  notation = "",
  title = "",
): void {
  const cell = document.createElement("th");
  if (title) {
    cell.title = title;
  }
  const content = document.createElement("span");
  content.className = "concordance-collocation-heading";
  const primary = document.createElement("span");
  primary.className = "concordance-collocation-heading-primary";
  primary.textContent = label;
  content.appendChild(primary);
  if (notation) {
    const sublabel = document.createElement("span");
    sublabel.className = "concordance-collocation-heading-sublabel";
    sublabel.textContent = notation;
    content.appendChild(sublabel);
  }
  cell.appendChild(content);
  row.appendChild(cell);
}

function appendConcordanceCollocationSortHeader(
  row: HTMLTableRowElement,
  sortKey: ConcordanceCollocationSortKey,
  measure: ConcordanceCollocationMeasure,
): void {
  const option = CONCORDANCE_COLLOCATION_SORT_OPTIONS.find((item) => item.value === sortKey);
  const cell = document.createElement("th");
  if (option?.title) {
    cell.title = option.title;
  }
  const button = document.createElement("button");
  button.type = "button";
  button.className = "frequency-sort frequency-sort-right concordance-collocation-sort";
  button.dataset.collocationSort = sortKey;

  const content = document.createElement("span");
  content.className = "concordance-collocation-heading";
  const primary = document.createElement("span");
  primary.className = "concordance-collocation-heading-primary";
  primary.textContent = concordanceCollocationSortHeaderLabel(sortKey, measure);
  content.appendChild(primary);
  if (option?.notation) {
    const sublabel = document.createElement("span");
    sublabel.className = "concordance-collocation-heading-sublabel";
    sublabel.textContent = option.notation;
    content.appendChild(sublabel);
  }
  button.appendChild(content);
  cell.appendChild(button);
  row.appendChild(cell);
}

function renderConcordanceCollocationTableHead(measure: ConcordanceCollocationMeasure): void {
  const row = document.createElement("tr");
  appendConcordanceCollocationHeader(row, "Collocate");
  appendConcordanceCollocationSortHeader(row, "nearQuery", measure);
  appendConcordanceCollocationSortHeader(row, "corpusFrequency", measure);
  appendConcordanceCollocationSortHeader(row, "association", measure);
  dom.concordanceCollocationsTableHead.replaceChildren(row);
  updateConcordanceCollocationSortHeaders();
}

function updateConcordanceCollocationSortHeaders(): void {
  const buttons = dom.concordanceCollocationsTableHead
    .querySelectorAll<HTMLButtonElement>(".concordance-collocation-sort");
  buttons.forEach((button) => {
    const sort = button.dataset.collocationSort as ConcordanceCollocationSortKey | undefined;
    button.removeAttribute("aria-sort");
    if (sort === concordanceCollocationSortKey) {
      button.setAttribute(
        "aria-sort",
        concordanceCollocationSortDirection === "asc" ? "ascending" : "descending",
      );
    }
  });
}

function collocateEvidenceSpanText(span: number, direction: ConcordanceContextFilterDirection): string {
  if (direction === "left") {
    return `span ${formatNumber(span)}L`;
  }
  if (direction === "right") {
    return `span ${formatNumber(span)}R`;
  }
  return `span ${formatNumber(span)}L–${formatNumber(span)}R`;
}

function isActiveCollocateEvidenceRow(row: ConcordanceCollocateRow): boolean {
  const selection = collocateEvidenceSelection;
  if (selection === null) {
    return false;
  }
  return row.surface === selection.surface
    && selection.query === normaliseConcordanceQuery(dom.concordanceQueryInput.value)
    && selection.caseSensitive === dom.concordanceCaseSensitiveInput.checked
    && selection.span === currentConcordanceCollocationSpan()
    && selection.direction === currentConcordanceCollocationDirection();
}

function createCollocateEvidenceSnippet(row: ConcordanceRow): HTMLSpanElement {
  const snippet = document.createElement("span");
  snippet.className = "concordance-collocate-evidence-snippet";
  snippet.setAttribute("role", "cell");
  const selection = collocateEvidenceSelection;
  const activeFilter = selection?.surface ?? null;
  const caseSensitive = selection?.caseSensitive ?? false;

  if (row.leftContext.length > 0) {
    snippet.appendChild(renderContextTokens(row.leftContext, activeFilter, caseSensitive));
    snippet.appendChild(document.createTextNode(" "));
  }

  const node = document.createElement("mark");
  node.className = "concordance-node";
  node.textContent = row.node;
  snippet.appendChild(node);

  if (row.rightContext.length > 0) {
    snippet.appendChild(document.createTextNode(" "));
    snippet.appendChild(renderContextTokens(row.rightContext, activeFilter, caseSensitive));
  }

  return snippet;
}

function createCollocateEvidenceExpansionRow(row: ConcordanceRow): HTMLDivElement {
  const rowKey = concordanceRowExpansionKey(row);
  const item = document.createElement("div");
  item.id = collocateEvidenceExpansionDomId(rowKey);
  item.className = "concordance-collocate-evidence-expansion-row";
  item.setAttribute("role", "row");

  const panel = document.createElement("div");
  panel.className = "concordance-collocate-evidence-expansion";
  panel.setAttribute("role", "cell");

  if (collocateEvidenceExpansionPending) {
    appendCollocateEvidenceMessage(panel, "Loading wider context...");
  } else if (collocateEvidenceExpansionFailure !== null) {
    appendCollocateEvidenceMessage(panel, collocateEvidenceExpansionFailure, true);
  } else if (collocateEvidenceExpansionResult !== null) {
    const context = document.createElement("p");
    context.className = "concordance-collocate-evidence-expansion-context";
    const activeFilter = collocateEvidenceSelection?.surface ?? null;
    const caseSensitive = collocateEvidenceSelection?.caseSensitive ?? false;

    if (collocateEvidenceExpansionResult.leftContext.length > 0) {
      const left = document.createElement("span");
      left.className = "concordance-collocate-evidence-expansion-left";
      left.appendChild(renderContextTokens(
        collocateEvidenceExpansionResult.leftContext,
        activeFilter,
        caseSensitive,
      ));
      context.appendChild(left);
      context.appendChild(document.createTextNode(" "));
    }

    const node = document.createElement("mark");
    node.className = "concordance-node";
    node.textContent = collocateEvidenceExpansionResult.nodeTokens.join(" ");
    context.appendChild(node);

    if (collocateEvidenceExpansionResult.rightContext.length > 0) {
      const right = document.createElement("span");
      right.className = "concordance-collocate-evidence-expansion-right";
      context.appendChild(document.createTextNode(" "));
      right.appendChild(renderContextTokens(
        collocateEvidenceExpansionResult.rightContext,
        activeFilter,
        caseSensitive,
      ));
      context.appendChild(right);
    }

    panel.appendChild(context);
  } else {
    appendCollocateEvidenceMessage(panel, "Loading wider context...");
  }

  item.appendChild(panel);
  return item;
}

function createCollocateEvidenceLine(row: ConcordanceRow): HTMLDivElement {
  const item = document.createElement("div");
  const rowKey = concordanceRowExpansionKey(row);
  const expanded = collocateEvidenceExpansionKey === rowKey;
  item.className = "concordance-collocate-evidence-line";
  item.classList.toggle("is-expanded", expanded);
  item.tabIndex = 0;
  item.setAttribute("role", "row");
  item.setAttribute("aria-expanded", expanded ? "true" : "false");
  item.setAttribute("aria-controls", collocateEvidenceExpansionDomId(rowKey));
  item.title = "Show wider context";
  item.addEventListener("click", () => {
    void toggleCollocateEvidenceExpansion(row);
  });
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    void toggleCollocateEvidenceExpansion(row);
  });

  const file = document.createElement("span");
  file.className = "concordance-collocate-evidence-file";
  file.setAttribute("role", "cell");
  file.title = row.displayName || row.sourcePath;
  file.textContent = row.displayName || row.sourcePath;

  item.append(file, createCollocateEvidenceSnippet(row));
  return item;
}

function appendCollocateEvidenceMessage(parent: HTMLElement, message: string, isError = false): void {
  const note = document.createElement("p");
  note.className = `concordance-collocate-evidence-message${isError ? " is-error" : ""}`;
  note.textContent = message;
  parent.appendChild(note);
}

function createCollocateEvidenceList(result: ConcordanceResult): HTMLDivElement {
  const list = document.createElement("div");
  list.className = "concordance-collocate-evidence-list";
  list.setAttribute("role", "table");

  const head = document.createElement("div");
  head.className = "concordance-collocate-evidence-list-head";
  head.setAttribute("role", "row");
  for (const label of ["File", "Context"]) {
    const cell = document.createElement("span");
    cell.setAttribute("role", "columnheader");
    cell.textContent = label;
    head.appendChild(cell);
  }
  list.appendChild(head);

  for (const evidenceRow of result.rows) {
    list.appendChild(createCollocateEvidenceLine(evidenceRow));
    if (collocateEvidenceExpansionKey === concordanceRowExpansionKey(evidenceRow)) {
      list.appendChild(createCollocateEvidenceExpansionRow(evidenceRow));
    }
  }
  return list;
}

function appendCollocateEvidenceShowMore(parent: HTMLElement, result: ConcordanceResult): void {
  const shown = Math.min(result.totalHits, result.rows.length);
  const remaining = Math.max(0, result.totalHits - shown);
  const nextCount = Math.min(CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT, remaining);
  if (remaining <= 0) {
    return;
  }

  const controls = document.createElement("div");
  controls.className = "concordance-collocate-evidence-more";
  const status = document.createElement("span");
  status.textContent = `Showing ${formatNumber(shown)} of ${formatNumber(result.totalHits)}`;

  const more = document.createElement("button");
  more.type = "button";
  more.className = "secondary-btn";
  more.textContent = collocateEvidencePending
    ? "Loading..."
    : remaining <= CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT
      ? `Show remaining ${formatNumber(remaining)}`
      : `Show ${formatNumber(nextCount)} more`;
  more.disabled = collocateEvidencePending;
  more.addEventListener("click", () => {
    collocateEvidenceLimit += nextCount;
    void refreshCollocateEvidence({ preserveResult: true });
  });

  controls.append(status, more);
  parent.appendChild(controls);
}

function createCollocateEvidenceDrawerRow(row: ConcordanceCollocateRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-collocate-evidence-row";

  const cell = document.createElement("td");
  cell.className = "concordance-collocate-evidence-cell";
  cell.colSpan = 4;

  const selection = collocateEvidenceSelection;
  const panel = document.createElement("div");
  panel.className = "concordance-collocate-evidence";

  const header = document.createElement("div");
  header.className = "concordance-collocate-evidence-header";
  const copy = document.createElement("div");
  copy.className = "concordance-collocate-evidence-copy";
  const title = document.createElement("strong");
  title.className = "concordance-collocate-evidence-title";
  const query = selection?.query ?? normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const direction = selection?.direction ?? "both";
  title.textContent = `Co-occurrence lines for "${row.surface}" with "${query}"`;
  const meta = document.createElement("span");
  meta.className = "concordance-collocate-evidence-meta";
  const span = selection?.span ?? currentConcordanceCollocationSpan();
  const spanText = collocateEvidenceSpanText(span, direction);
  if (collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0) {
    const hitCount = collocateEvidenceResult.totalHits;
    const lineText = `${formatNumber(hitCount)} line${hitCount === 1 ? "" : "s"}`;
    meta.textContent = `${lineText} · ${spanText}`;
  } else {
    meta.textContent = spanText;
  }
  copy.append(title, meta);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "secondary-btn concordance-collocate-evidence-close";
  close.textContent = "Close";
  close.addEventListener("click", () => {
    clearCollocateEvidence();
    renderConcordanceCollocations();
  });
  header.append(copy, close);
  panel.appendChild(header);

  if (collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0) {
    panel.appendChild(createCollocateEvidenceList(collocateEvidenceResult));
    appendCollocateEvidenceShowMore(panel, collocateEvidenceResult);
    if (collocateEvidenceFailure !== null) {
      appendCollocateEvidenceMessage(panel, `Could not load more co-occurrence lines: ${collocateEvidenceFailure}`, true);
    }
  } else if (collocateEvidencePending) {
    appendCollocateEvidenceMessage(panel, "Loading co-occurrence lines...");
  } else if (collocateEvidenceFailure !== null) {
    appendCollocateEvidenceMessage(panel, `Could not load co-occurrence lines: ${collocateEvidenceFailure}`, true);
  } else if (collocateEvidenceResult === null || collocateEvidenceResult.totalHits === 0) {
    appendCollocateEvidenceMessage(
      panel,
      "No co-occurrence lines for this collocate with the current span and direction.",
    );
  }

  cell.appendChild(panel);
  item.appendChild(cell);
  return item;
}

function handleConcordanceCollocateClick(row: ConcordanceCollocateRow): void {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (query.length === 0) {
    return;
  }

  collocateEvidenceSelection = {
    surface: row.surface,
    query,
    caseSensitive: dom.concordanceCaseSensitiveInput.checked,
    span: currentConcordanceCollocationSpan(),
    direction: currentConcordanceCollocationDirection() as ConcordanceContextFilterDirection,
  };
  collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;
  collocateEvidenceFailure = null;
  collocateEvidenceResult = null;
  void refreshCollocateEvidence();
}

function createConcordanceCollocationRow(
  row: ConcordanceCollocateRow,
  measure: ConcordanceCollocationMeasure,
  visibleScores: Array<number | null>,
): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-collocation-row";
  item.classList.toggle("is-active", isActiveCollocateEvidenceRow(row));
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const actionLabel = `Show co-occurrence lines for "${row.surface}" with "${query}"`;
  item.tabIndex = 0;
  item.setAttribute("role", "button");
  item.setAttribute("aria-label", actionLabel);
  item.title = actionLabel;
  item.addEventListener("click", () => handleConcordanceCollocateClick(row));
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    handleConcordanceCollocateClick(row);
  });

  const surface = document.createElement("td");
  const text = document.createElement("span");
  text.className = "concordance-collocate-text";
  text.textContent = row.surface;
  surface.appendChild(text);

  const o11 = document.createElement("td");
  o11.className = "numeric";
  o11.textContent = formatNumber(row.o11);

  const frequency = document.createElement("td");
  frequency.className = "numeric";
  frequency.textContent = formatNumber(row.collocateFrequency);

  const score = document.createElement("td");
  score.className = "numeric";
  const scoreValue = concordanceCollocationScore(row, measure);
  const value = document.createElement("span");
  value.className = "concordance-collocation-score-value";
  value.textContent = formatConcordanceCollocationScore(scoreValue);
  score.appendChild(value);
  const barPercent = concordanceCollocationScoreBarPercent(scoreValue, measure, visibleScores);
  if (barPercent !== null) {
    const track = document.createElement("span");
    track.className = "concordance-collocation-score-track";
    const fill = document.createElement("span");
    fill.className = "concordance-collocation-score-fill";
    fill.style.setProperty("--bar-width", `${barPercent}%`);
    track.title = `${concordanceCollocationMeasureLabel(measure)}: ${formatConcordanceCollocationScore(scoreValue)}`;
    track.appendChild(fill);
    score.appendChild(track);
  }

  item.append(surface, o11, frequency, score);
  return item;
}

function renderConcordanceCollocations(): void {
  syncConcordanceControls();
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const measure = concordanceCollocationMeasure;
  const span = currentConcordanceCollocationSpan();
  const direction = currentConcordanceCollocationDirection();
  renderConcordanceCollocationTableHead(measure);
  dom.concordanceCollocationsSummary.classList.remove("hidden");

  if (query.length === 0) {
    dom.concordanceCollocationsSummary.textContent = CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE;
    renderConcordanceCollocationStatus(null);
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE);
    return;
  }

  const explanation = concordanceCollocationExplanation(query, span, direction, measure);
  dom.concordanceCollocationsSummary.textContent = explanation;

  if (concordanceCollocationPending || concordanceCollocationState === "loading") {
    renderConcordanceCollocationStatus("Computing collocates...");
    renderConcordanceCollocationTableState("Loading...");
    return;
  }

  if (concordanceCollocationFailure || concordanceCollocationState === "error") {
    const message = concordanceCollocationFailure ?? "Collocate computation failed.";
    renderConcordanceCollocationStatus(message, true);
    renderConcordanceCollocationTableState(message);
    return;
  }

  if (concordanceCollocationState === "stale") {
    const staleMessage = query.length > 0
      ? `Update the collocate profile for "${query}".`
      : CONCORDANCE_COLLOCATION_STALE_MESSAGE;
    renderConcordanceCollocationStatus(
      renderConcordanceCollocationAction(
        staleMessage,
        concordanceCollocationSupersededNotice,
        "Update profile",
      ),
    );
    renderConcordanceCollocationTableState(staleMessage);
    return;
  }

  const result = concordanceCollocationResult;
  if (
    !result
    || concordanceCollocationState === "idle_not_computed"
    || !currentConcordanceCollocationResultIsFresh()
  ) {
    renderConcordanceCollocationStatus(null);
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE);
    return;
  }

  dom.concordanceCollocationsSummary.textContent =
    `${explanation} · ${formatNumber(result.fNode)} nodes · ${formatNumber(result.r1)} slots · ${formatNumber(result.n)} tokens`;
  renderConcordanceCollocationStatus(null);

  const rows = result.rows;
  if (rows.length === 0) {
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE);
    return;
  }

  const visibleScores = rows.map((row) => concordanceCollocationScore(row, measure));
  dom.concordanceCollocationsTableBody.replaceChildren();
  for (const row of rows) {
    const rowElement = createConcordanceCollocationRow(row, measure, visibleScores);
    dom.concordanceCollocationsTableBody.appendChild(rowElement);
    if (isActiveCollocateEvidenceRow(row)) {
      dom.concordanceCollocationsTableBody.appendChild(createCollocateEvidenceDrawerRow(row));
    }
  }
}


export function resetConcordanceForFrequencyScopeChange(): void {
  concordanceOffset = 0;
  resetConcordanceDistributionOffset();
  concordanceResult = null;
  collapseConcordanceExpansion();
}

export function resetConcordanceCorpusState(): void {
  concordanceResult = null;
  concordanceOffset = 0;
  concordanceSortKey = DEFAULT_CONCORDANCE_SORT_KEY;
  resetConcordanceDistributionOffset();
  concordanceQueryGeneration += 1;
  concordancePending = false;
  concordanceFailure = null;
  concordanceExpansionKey = null;
  concordanceExpansionPending = false;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  concordanceExpansionGeneration += 1;
  concordanceCollocationState = "idle_not_computed";
  concordanceCollocationResult = null;
  concordanceCollocationGeneration += 1;
  concordanceCollocationPending = false;
  concordanceCollocationFailure = null;
  concordanceCollocationFreshAggregationKey = null;
  concordanceCollocationEverComputed = false;
  concordanceCollocationAutoComputedAggregationKeys = new Set<string>();
  concordanceCollocationInFlightRequestKey = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationMeasure = DEFAULT_CONCORDANCE_COLLOCATION_MEASURE;
  concordanceCollocationSortKey = DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY;
  concordanceCollocationSortDirection = DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION;
  clearCollocateEvidence();
}

export function renderConcordance(): void {
  renderFrequencyScopeControl(state.analysisCorpus);
  syncConcordanceControls();
  syncConcordanceViewTabs();

  if (concordancePending) {
    dom.concordanceSummary.textContent = "Searching...";
    renderConcordanceStatus(null);
    renderConcordanceTableState("Searching...");
    renderConcordanceDistribution(null, "Searching...");
    renderConcordanceCollocations();
    dom.concordancePagePreviousButton.disabled = true;
    dom.concordancePageNextButton.disabled = true;
    dom.concordancePageStatus.textContent = "";
    return;
  }

  if (concordanceFailure) {
    dom.concordanceSummary.textContent = CONCORDANCE_NO_QUERY_MESSAGE;
    renderConcordanceStatus(concordanceFailure, true);
    renderConcordanceTableState(concordanceFailure);
    renderConcordanceDistribution(null, concordanceFailure);
    renderConcordanceCollocations();
    dom.concordancePagePreviousButton.disabled = true;
    dom.concordancePageNextButton.disabled = true;
    dom.concordancePageStatus.textContent = "";
    return;
  }

  const result = concordanceResult;
  if (!result) {
    dom.concordanceSummary.textContent = CONCORDANCE_NO_QUERY_MESSAGE;
    renderConcordanceStatus(null);
    renderConcordanceTableState(CONCORDANCE_NO_QUERY_MESSAGE);
    renderConcordanceDistribution(null, CONCORDANCE_NO_QUERY_MESSAGE);
    renderConcordanceCollocations();
    dom.concordancePagePreviousButton.disabled = true;
    dom.concordancePageNextButton.disabled = true;
    dom.concordancePageStatus.textContent = "";
    return;
  }

  renderConcordanceSummary(result);
  renderConcordanceStatus(null);

  if (result.rows.length === 0) {
    renderConcordanceTableState(CONCORDANCE_NO_HITS_MESSAGE);
  } else {
    dom.concordanceTableBody.replaceChildren();
    for (const row of result.rows) {
      const rowElement = createConcordanceRow(row);
      dom.concordanceTableBody.appendChild(rowElement);
      const rowKey = concordanceRowExpansionKey(row);
      const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());
      if (expansion.expanded) {
        dom.concordanceTableBody.appendChild(createConcordanceSourcePreviewRow(row));
      }
    }
  }
  renderConcordanceDistribution(result);
  renderConcordanceCollocations();

  const first = result.totalHits === 0 ? 0 : result.offset + 1;
  const last = Math.min(result.totalHits, result.offset + result.rows.length);
  dom.concordancePageStatus.textContent =
    result.totalHits === 0 ? "0 of 0" : `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(result.totalHits)}`;
  dom.concordancePagePreviousButton.disabled = result.offset === 0;
  dom.concordancePageNextButton.disabled = result.offset + result.rows.length >= result.totalHits;
}

export async function refreshConcordance(
  resetOffset = true,
  resetDistribution = true,
  collapseExpansion = true,
): Promise<void> {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (resetOffset) {
    concordanceOffset = 0;
  }
  if (resetDistribution) {
    resetConcordanceDistributionOffset();
  }
  if (collapseExpansion) {
    collapseConcordanceExpansion();
  }
  concordanceSortKey = currentConcordanceSortKey();
  if (query.length === 0) {
    concordanceResult = null;
    concordanceFailure = null;
    concordancePending = false;
    renderConcordance();
    return;
  }

  const generation = ++concordanceQueryGeneration;
  concordancePending = true;
  concordanceFailure = null;
  concordanceResult = null;
  renderConcordance();

  try {
    const result = await searchConcordance(
      {
        selection: currentFrequencyDocumentScope(),
        query,
        contextFilter: null,
        contextFilterDirection: "both",
        caseSensitive: dom.concordanceCaseSensitiveInput.checked,
        sortKey: concordanceSortKey,
        windowTokens: currentConcordanceWindow(),
        offset: concordanceOffset,
        limit: currentConcordancePageSize(),
        distributionOffset: currentConcordanceDistributionOffset(),
        distributionLimit: CONCORDANCE_DISTRIBUTION_PAGE_SIZE,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== concordanceQueryGeneration) {
      return;
    }
    concordanceResult = result;
    concordanceOffset = result.offset;
  } catch (error) {
    if (generation !== concordanceQueryGeneration) {
      return;
    }
    concordanceFailure = `${error}`;
  } finally {
    if (generation === concordanceQueryGeneration) {
      concordancePending = false;
      renderConcordance();
    }
  }
}

async function refreshCollocateEvidence(
  options: { preserveResult?: boolean } = {},
): Promise<void> {
  collocateEvidenceExpansionKey = null;
  collocateEvidenceExpansionPending = false;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  collocateEvidenceExpansionGeneration += 1;

  const selection = collocateEvidenceSelection;
  if (selection === null) {
    collocateEvidenceResult = null;
    collocateEvidenceFailure = null;
    collocateEvidencePending = false;
    renderConcordanceCollocations();
    return;
  }

  const generation = ++collocateEvidenceGeneration;
  collocateEvidencePending = true;
  collocateEvidenceFailure = null;
  if (!options.preserveResult) {
    collocateEvidenceResult = null;
  }
  renderConcordanceCollocations();

  try {
    const result = await searchConcordance(
      {
        selection: currentFrequencyDocumentScope(),
        query: selection.query,
        contextFilter: selection.surface,
        contextFilterDirection: selection.direction,
        caseSensitive: selection.caseSensitive,
        sortKey: "filePosition",
        windowTokens: selection.span,
        offset: 0,
        limit: collocateEvidenceLimit,
        distributionOffset: 0,
        distributionLimit: 1,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== collocateEvidenceGeneration) {
      return;
    }
    collocateEvidenceResult = result;
  } catch (error) {
    if (generation !== collocateEvidenceGeneration) {
      return;
    }
    collocateEvidenceFailure = `${error}`;
  } finally {
    if (generation === collocateEvidenceGeneration) {
      collocateEvidencePending = false;
      renderConcordanceCollocations();
    }
  }
}

async function refreshConcordanceCollocations(): Promise<void> {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  concordanceCollocationMeasure = currentConcordanceCollocationMeasure();
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  const requestKey = currentConcordanceCollocationRequestKey();
  if (query.length === 0 || aggregationKey === null || requestKey === null) {
    concordanceCollocationState = "idle_not_computed";
    concordanceCollocationResult = null;
    concordanceCollocationFailure = null;
    concordanceCollocationPending = false;
    concordanceCollocationFreshAggregationKey = null;
    concordanceCollocationInFlightRequestKey = null;
    renderConcordanceCollocations();
    return;
  }
  if (concordanceCollocationPending && concordanceCollocationInFlightRequestKey === requestKey) {
    return;
  }

  const generation = ++concordanceCollocationGeneration;
  concordanceCollocationPending = true;
  concordanceCollocationState = "loading";
  concordanceCollocationFailure = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationResult = null;
  concordanceCollocationInFlightRequestKey = requestKey;
  renderConcordanceCollocations();

  try {
    const result = await queryConcordanceCollocations(
      {
        selection: currentFrequencyDocumentScope(),
        query,
        caseSensitive: dom.concordanceCaseSensitiveInput.checked,
        span: currentConcordanceCollocationSpan(),
        direction: currentConcordanceCollocationDirection(),
        minCooccurrence: currentConcordanceCollocationMinO11(),
        minCollocateFrequency: currentConcordanceCollocationMinFrequency(),
        hideStopwords: dom.concordanceCollocationHideStopwordsInput.checked,
        sortMeasure: concordanceCollocationMeasure,
        sortKey: concordanceCollocationSortKey,
        sortDirection: currentConcordanceCollocationSortDirection(),
        limit: CONCORDANCE_COLLOCATION_LIMIT,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (
      generation !== concordanceCollocationGeneration
      || requestKey !== currentConcordanceCollocationRequestKey()
      || aggregationKey !== currentConcordanceCollocationAggregationKey()
    ) {
      noteSupersededConcordanceCollocationResult();
      return;
    }
    concordanceCollocationResult = result;
    concordanceCollocationFreshAggregationKey = aggregationKey;
    concordanceCollocationEverComputed = true;
    concordanceCollocationAutoComputedAggregationKeys.add(aggregationKey);
    concordanceCollocationState = "ready";
  } catch (error) {
    if (generation !== concordanceCollocationGeneration) {
      noteSupersededConcordanceCollocationResult();
      return;
    }
    concordanceCollocationState = "error";
    concordanceCollocationFailure = `${error}`;
  } finally {
    if (generation === concordanceCollocationGeneration) {
      concordanceCollocationPending = false;
      concordanceCollocationInFlightRequestKey = null;
      renderConcordanceCollocations();
    }
  }
}

function showPreviousConcordancePage(): void {
  const pageSize = concordanceResult?.limit ?? currentConcordancePageSize();
  concordanceOffset = Math.max(0, concordanceOffset - pageSize);
  void refreshConcordance(false, false);
}

function showNextConcordancePage(): void {
  const pageSize = concordanceResult?.limit ?? currentConcordancePageSize();
  concordanceOffset += pageSize;
  void refreshConcordance(false, false);
}

function handleConcordanceSortChange(): void {
  concordanceSortKey = currentConcordanceSortKey();
  void refreshConcordance(true, true);
}

function runConcordanceSearch(): void {
  void runConcordanceSearchForActiveView();
}

async function runConcordanceSearchForActiveView(): Promise<void> {
  const viewAtSearch = activeConcordanceView;
  const queryAtSearch = normaliseConcordanceQuery(dom.concordanceQueryInput.value);

  clearCollocateEvidence();
  markConcordanceCollocationsForAggregationChange();
  if (viewAtSearch === "collocates" && queryAtSearch.length > 0) {
    concordanceCollocationPending = true;
    concordanceCollocationState = "loading";
    concordanceCollocationFailure = null;
    concordanceCollocationResult = null;
    renderConcordanceCollocations();
  }
  await refreshConcordance();

  if (
    viewAtSearch === "collocates"
    && activeConcordanceView === "collocates"
    && queryAtSearch.length > 0
    && normaliseConcordanceQuery(dom.concordanceQueryInput.value) === queryAtSearch
    && concordanceResult !== null
    && concordanceFailure === null
  ) {
    await refreshConcordanceCollocations();
  }
}

function handleConcordancePageSizeChange(): void {
  currentConcordancePageSize();
  void refreshConcordance(true, true);
}

function collapseConcordanceExpansionFromControlChange(): void {
  collapseConcordanceExpansion();
  renderConcordance();
}

function handleConcordanceQueryInput(): void {
  collapseConcordanceExpansionFromControlChange();
  markConcordanceCollocationsForAggregationChange();
}

function handleConcordanceCaseChange(): void {
  syncConcordanceCaseToggle();
  collapseConcordanceExpansionFromControlChange();
  markConcordanceCollocationsForAggregationChange();
}

function handleConcordanceCollocationMeasureChange(): void {
  concordanceCollocationMeasure = dom.concordanceCollocationMeasureSelect.value as ConcordanceCollocationMeasure;
  refreshConcordanceCollocationsForPresentationChange();
}

function handleConcordanceCollocationSortClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(".concordance-collocation-sort");
  if (!button) {
    return;
  }
  const nextKey = button.dataset.collocationSort as ConcordanceCollocationSortKey | undefined;
  if (!nextKey) {
    return;
  }
  if (nextKey === concordanceCollocationSortKey) {
    concordanceCollocationSortDirection = concordanceCollocationSortDirection === "asc" ? "desc" : "asc";
  } else {
    concordanceCollocationSortKey = nextKey;
    concordanceCollocationSortDirection = "desc";
  }
  refreshConcordanceCollocationsForPresentationChange();
}

function handleConcordanceCollocationAggregationControlChange(): void {
  markConcordanceCollocationsForAggregationChange();
}

function handleConcordanceCollocationPresentationControlChange(): void {
  clearCollocateEvidence();
  refreshConcordanceCollocationsForPresentationChange();
}
export function renderAnalyseWorkbench(): void {
  renderAnalyseStatus();
  renderAnalysisNotice();
  renderProjectSummary();
  renderAnalysisTools();
  renderAnalysisToolPanels();
  renderCorpusOverview();
  renderFrequencyList();
  renderConcordance();
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

export function setActiveAnalysisTool(tool: AnalysisToolId): void {
  activeAnalysisTool = tool;
  renderAnalysisTools();
  renderAnalysisToolPanels();
  if (tool === "frequency") {
    renderFrequencyList();
  } else if (tool === "concordance") {
    renderConcordance();
  }
  refreshStageNavigator();
}


export function setActiveAnalysisToolForBuild(tool: AnalysisToolId): void {
  activeAnalysisTool = tool;
}

export function isFrequencyAnalysisToolActive(): boolean {
  return activeAnalysisTool === "frequency";
}

export function resetWorkbenchForProject(): void {
  resetAnalysisBuildStateForWorkbench();
  activeAnalysisTool = "overview";
  resetFrequencyCorpusState();
  dom.workbenchSwitcher.classList.remove("hidden");
  setActiveWorkbench("prepare");
}

export function hideWorkbenchSwitcher(): void {
  resetAnalysisBuildStateForWorkbench();
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

function handleAnnotationLayerClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-annotation-layer-action]");
  if (!button || button.disabled) {
    return;
  }
  void handleAnnotationLayerAction();
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
  dom.concordanceViewTabs.addEventListener("click", handleConcordanceViewTabClick);
  dom.concordanceSearchButton.addEventListener("click", runConcordanceSearch);
  dom.concordanceQueryInput.addEventListener("input", handleConcordanceQueryInput);
  dom.concordanceQueryInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      runConcordanceSearch();
    }
  });
  dom.concordanceWindowInput.addEventListener("change", collapseConcordanceExpansionFromControlChange);
  dom.concordanceCaseSensitiveInput.addEventListener("change", handleConcordanceCaseChange);
  dom.concordanceSortSelect.addEventListener("change", handleConcordanceSortChange);
  dom.concordancePageSizeInput.addEventListener("change", handleConcordancePageSizeChange);
  dom.concordanceFilesButton.addEventListener("click", openFrequencyFilesModal);
  dom.concordancePagePreviousButton.addEventListener("click", showPreviousConcordancePage);
  dom.concordancePageNextButton.addEventListener("click", showNextConcordancePage);
  dom.concordanceCollocationSpanInput.addEventListener("change", handleConcordanceCollocationAggregationControlChange);
  dom.concordanceCollocationDirectionSelect.addEventListener("change", handleConcordanceCollocationAggregationControlChange);
  dom.concordanceCollocationMeasureSelect.addEventListener("change", handleConcordanceCollocationMeasureChange);
  dom.concordanceCollocationMinO11Input.addEventListener("change", handleConcordanceCollocationPresentationControlChange);
  dom.concordanceCollocationMinFreqInput.addEventListener("change", handleConcordanceCollocationPresentationControlChange);
  dom.concordanceCollocationsTableHead.addEventListener("click", handleConcordanceCollocationSortClick);
  document.addEventListener("keydown", handleConcordanceExpansionEscape);
  syncConcordanceCollocationStopwordsToggle();
  dom.concordanceCollocationHideStopwordsInput.addEventListener("change", () => {
    syncConcordanceCollocationStopwordsToggle();
    handleConcordanceCollocationPresentationControlChange();
  });
  dom.frequencySearchInput.addEventListener("input", scheduleFrequencyRefresh);
  dom.frequencyMatchModeSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyMinInput.addEventListener("input", scheduleFrequencyRefresh);
  dom.frequencyGroupBySelect.addEventListener("change", () => {
    syncFrequencyAnnotationControls();
    syncFrequencyCaseFoldToggle();
    void refreshFrequencyPage();
  });
  dom.frequencyRateScaleSelect.value = DEFAULT_FREQUENCY_RATE_SCALE;
  syncFrequencyAnnotationControls();
  syncFrequencyCaseFoldToggle();
  dom.frequencyCaseFoldInput.addEventListener("change", () => {
    syncFrequencyCaseFoldToggle();
    void refreshFrequencyPage();
  });
  dom.frequencyPosFilterSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyTokenClassSelect.addEventListener("change", () => void refreshFrequencyPage());
  dom.frequencyRateScaleSelect.addEventListener("change", handleFrequencyRateScaleChange);
  syncFrequencyStopwordsToggle();
  dom.frequencyHideStopwordsInput.addEventListener("change", () => {
    syncFrequencyStopwordsToggle();
    void refreshFrequencyPage();
  });
  dom.frequencyFilesButton.addEventListener("click", openFrequencyFilesModal);
  dom.frequencyModeTabs.addEventListener("click", handleFrequencyModeClick);
  dom.frequencyAnnotationLayer.addEventListener("click", handleAnnotationLayerClick);
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
