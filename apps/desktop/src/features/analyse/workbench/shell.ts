import katex from "katex";
import "katex/dist/katex.min.css";

import {
  type AnalysisInputSource,
  type CorpusOverview as AnalysisCorpus,
  type CorpusOverviewFinding,
  type CorpusOverviewFindingSeverity,
  cancelConcordance,
  queryOverviewDistribution,
  queryOverviewDistributionEvidence,
  type OverviewDistributionEvidencePage,
  type OverviewDistributionMetric,
  type OverviewDistributionResult,
} from "../api";
import { BOOLEAN_CLEANING_CONFIG_KEYS } from "../../prepare/cleaning/config";
import { dom } from "../../../app/dom";
import { effectiveCleaningConfig } from "../../prepare/cleaning/effective-config";
import { DEFAULT_FREQUENCY_RATE_SCALE } from "../frequency/model";
import {
  overviewGlossary,
  type OverviewGlossaryEntry,
  type OverviewGlossaryId,
} from "../overview/glossary";
import {
  createOverviewFigure,
  createOverviewRangeControl,
  createOverviewSegmentedControl,
  replaceFigureSvg,
} from "../../../plots/figure";
import {
  FIGURE_INTERACTION_POLICIES,
  OverviewFigureController,
  type FigureControlState,
} from "../../../plots/figure-workbench";
import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildDispersionPlaneGeometry,
  buildDistributionDiagnosticGeometry,
  buildLexicalSpectrumGeometry,
  buildRankFrequencyGeometry,
  buildVocabularyGrowthGeometry,
  deriveCoverageBandShares,
  formatDispersionPlaneStats,
  formatDispersionProfileStats,
  formatUniversalTypeCaption,
} from "../../../plots/model";
import {
  renderCoverageSvg,
  renderDispersionPlaneSvg,
  renderDispersionSvg,
  renderDistributionDiagnosticSvg,
  renderFigureStatusSvg,
  renderLexicalSpectrumSvg,
  renderRankFrequencySvg,
  renderVocabularyGrowthSvg,
} from "../../../plots/render";
import type { ProjectLanguage } from "../../../generated/ProjectLanguage.js";
import { projectLanguageLabel } from "../../project/creation-dialog";
import { sourceHasExtractedText } from "../../prepare/sources";
import { state, type WorkbenchId } from "../../../app/state";
import {
  analysisStatus,
  currentAnalysisNotice,
  handleAnnotationLayerAction,
  handleBuildAnalysisCorpus,
  initialiseAnalysisBuildState,
  renderAnalyseStatus,
  renderAnalysisNotice,
  resetAnalysisBuildStateForWorkbench,
} from "./build";
import {
  applyFrequencyFileScope,
  clearFrequencyFiles,
  closeFrequencyFilesModal,
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
} from "./frequency";
import {
  handleConcordanceCollocationAggregationControlChange,
  handleConcordanceCollocationMeasureChange,
  handleConcordanceCollocationPresentationControlChange,
  handleConcordanceCollocationSortClick,
  handleConcordanceCollocationsShowMore,
  handleConcordancePositionProfileModeClick,
  syncConcordanceCollocationStopwordsToggle,
} from "./concordance/collocations";
import {
  handleConcordanceExpansionEscape,
  handleConcordancePageSizeChange,
  handleConcordanceSortChange,
  handleConcordanceWindowChange,
  showNextConcordancePage,
  showPreviousConcordancePage,
} from "./concordance/lines";
import {
  handleConcordanceCaseChange,
  handleClearConcordanceLinesContextFilter,
  handleConcordanceQueryInput,
  initialiseConcordanceState,
  renderConcordance,
  runConcordanceSearch,
} from "./concordance/shared";

const FUTURE_ANALYSIS_TOOLS = [
  "N-grams",
  "Exports",
];
const BALANCE_ATTENTION_SHARE = 0.15;

type AnalysisToolId = "overview" | "frequency" | "concordance";

interface OverviewMetricRow {
  label: string;
  value: string;
  share?: string;
  title?: string;
  glossaryId?: OverviewGlossaryId;
  labelSuffix?: string;
  description?: string;
  sampleTypes?: string[];
  sampleTypeCount?: number;
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
let overviewSampleControlId = 0;

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

function formatOptionalFixed(value: number | null, fractionDigits: number): string {
  return value === null ? "unavailable" : formatFixedNumber(value, fractionDigits);
}

function formatOptionalPercent(value: number | null, fractionDigits = 1): string {
  return value === null ? "unavailable" : formatPercent(value, fractionDigits);
}

function formatRankRange(start: number | null, end: number | null): string {
  return start === null || end === null
    ? "unavailable"
    : `${formatNumber(start)}-${formatNumber(end)}`;
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
  panel.className = "analysis-panel cw-panel cw-panel--analysis";

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

function createOverviewDisclosureButton(labelText: string, panelId: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "analysis-glossary-term";
  button.setAttribute("aria-expanded", "false");
  button.setAttribute("aria-controls", panelId);

  const label = document.createElement("span");
  label.textContent = labelText;
  const caret = document.createElement("span");
  caret.className = "analysis-glossary-caret";
  caret.setAttribute("aria-hidden", "true");
  caret.textContent = "›";
  button.append(label, caret);

  return button;
}

function createGlossaryDisclosure(
  glossaryId: OverviewGlossaryId,
  onToggle?: (expanded: boolean) => void,
  labelText: string = overviewGlossary[glossaryId].label,
): { button: HTMLButtonElement; panel: HTMLDivElement } {
  const entry: OverviewGlossaryEntry = overviewGlossary[glossaryId];
  const panelId = `overview-glossary-${entry.id}-${++overviewGlossaryControlId}`;

  const button = createOverviewDisclosureButton(labelText, panelId);

  const panel = document.createElement("div");
  panel.id = panelId;
  panel.className = "analysis-glossary-detail";
  panel.hidden = true;

  appendGlossaryEntryContent(panel, entry);

  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(expanded));
    panel.hidden = !expanded;
    onToggle?.(expanded);
  });

  return { button, panel };
}

function appendGlossaryEntryContent(panel: HTMLElement, entry: OverviewGlossaryEntry): void {
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
}

function createGroupedMethodDisclosure(
  labelText: string,
  glossaryIds: OverviewGlossaryId[],
): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "analysis-method-disclosure";
  const panelId = `overview-glossary-group-${++overviewGlossaryControlId}`;
  const button = createOverviewDisclosureButton(labelText, panelId);
  const panel = document.createElement("div");
  panel.id = panelId;
  panel.className = "analysis-glossary-detail analysis-method-disclosure-detail";
  panel.hidden = true;

  for (const glossaryId of glossaryIds) {
    const entry = overviewGlossary[glossaryId];
    const section = document.createElement("section");
    section.className = "analysis-glossary-group-entry";
    const heading = document.createElement("h4");
    heading.className = "analysis-glossary-entry-title";
    heading.textContent = entry.label;
    section.appendChild(heading);
    appendGlossaryEntryContent(section, entry);
    panel.appendChild(section);
  }

  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(expanded));
    panel.hidden = !expanded;
  });

  wrapper.append(button, panel);
  return wrapper;
}

function createFigureExplanation(...glossaryIds: OverviewGlossaryId[]): HTMLElement {
  const explanation = document.createElement("div");
  explanation.className = "cw-overview-figure-teaching";
  for (const glossaryId of glossaryIds) {
    const entry = overviewGlossary[glossaryId];
    const disclosure = createGlossaryDisclosure(
      glossaryId,
      undefined,
      `Explain ${entry.label}`,
    );
    explanation.append(disclosure.button, disclosure.panel);
  }
  return explanation;
}

function createSampleTypesDisclosure(
  sampleTypes: string[],
  sampleTypeCount: number,
  colSpan = 3,
): { button: HTMLButtonElement; row: HTMLTableRowElement; panel: HTMLDivElement } {
  const panelId = `overview-sample-types-${++overviewSampleControlId}`;
  const button = createOverviewDisclosureButton("Show types", panelId);
  button.classList.add("analysis-dispersion-sample-toggle");

  const row = document.createElement("tr");
  row.className = "analysis-dispersion-sample-row";
  row.hidden = true;

  const detailCell = document.createElement("td");
  detailCell.colSpan = colSpan;

  const panel = document.createElement("div");
  panel.id = panelId;
  panel.className = "analysis-dispersion-sample-detail";
  panel.hidden = true;

  const types = document.createElement("p");
  types.className = "analysis-dispersion-sample-types";
  types.textContent = sampleTypes.join(", ");
  panel.appendChild(types);

  if (sampleTypeCount > sampleTypes.length) {
    const caption = document.createElement("p");
    caption.className = "analysis-dispersion-sample-caption";
    caption.textContent = `Top 20 by frequency of ${formatNumber(sampleTypeCount)} types.`;
    panel.appendChild(caption);
  }

  button.addEventListener("click", () => {
    const expanded = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(expanded));
    row.hidden = !expanded;
    panel.hidden = !expanded;
  });

  detailCell.appendChild(panel);
  row.appendChild(detailCell);
  return { button, row, panel };
}

function createMetricTable(rows: OverviewMetricRow[], className = ""): HTMLTableElement {
  const table = document.createElement("table");
  const hasShareColumn = rows.some((row) => Boolean(row.share));
  table.className = [
    "analysis-metric-table",
    hasShareColumn ? "has-share-column" : "has-no-share-column",
    className,
  ].filter(Boolean).join(" ");

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
      }, row.label);
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
    const valueText = document.createElement("span");
    valueText.textContent = row.value;
    value.appendChild(valueText);

    let sampleRow: HTMLTableRowElement | null = null;
    if (row.sampleTypes?.length && row.sampleTypeCount !== undefined) {
      const sampleDisclosure = createSampleTypesDisclosure(
        row.sampleTypes,
        row.sampleTypeCount,
        hasShareColumn ? 3 : 2,
      );
      value.appendChild(sampleDisclosure.button);
      sampleRow = sampleDisclosure.row;
    }

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
      detailCell.colSpan = hasShareColumn ? 3 : 2;
      detailCell.appendChild(glossaryPanel);
      glossaryRow.appendChild(detailCell);
      body.appendChild(glossaryRow);
    }
    if (sampleRow) {
      body.appendChild(sampleRow);
    }
  }

  table.appendChild(body);
  return table;
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

function countWithInlineShareRow(
  label: string,
  count: number,
  share: number | null,
): OverviewMetricRow {
  return {
    label,
    value: formatCountWithShare(count, share),
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
    { label: "Types", value: formatNumber(corpus.totals.types) },
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
  const universe = document.createElement("p");
  universe.className = "analysis-panel-note";
  universe.textContent = "Universe: all analysis types - independent of Frequency List filters.";
  panel.appendChild(universe);
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

  if (corpus.documentsTruncated) {
    const header = document.createElement("div");
    header.className = "document-length-header";
    header.setAttribute("aria-hidden", "true");

    const documentLabel = document.createElement("span");
    documentLabel.className = "document-length-name";
    documentLabel.textContent = "Document";

    const shareOfTokensLabel = document.createElement("span");
    shareOfTokensLabel.className = "document-length-track";
    shareOfTokensLabel.textContent = "Share of tokens";

    const tokensLabel = document.createElement("span");
    tokensLabel.className = "document-length-count";
    tokensLabel.textContent = "Tokens";

    const percentLabel = document.createElement("span");
    percentLabel.className = "document-length-share";
    percentLabel.textContent = "%";

    header.append(documentLabel, shareOfTokensLabel, tokensLabel, percentLabel);
    distribution.appendChild(header);
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

function renderLengthHistogramFigure(corpus: AnalysisCorpus): HTMLElement | null {
  if (corpus.totals.documents === 0) return null;
  return renderDistributionDiagnosticFigure(corpus, "documentLength");
}

interface DistributionFigureControls extends FigureControlState {
  view: "histogram" | "ecdf" | "qq";
  histogramRule: "auto" | "manual";
  manualBins: number;
  xScale: "linear" | "log";
}

function renderDistributionDiagnosticFigure(
  corpus: AnalysisCorpus,
  metric: OverviewDistributionMetric,
): HTMLElement {
  const isLength = metric === "documentLength";
  const id = isLength ? "overview-length-histogram" : "overview-mattr-distribution";
  const initial: DistributionFigureControls = {
    view: "histogram",
    histogramRule: "auto",
    manualBins: 24,
    xScale: isLength ? "log" : "linear",
  };
  const controller = new OverviewFigureController<DistributionFigureControls>({
    initialControls: initial,
    policy: FIGURE_INTERACTION_POLICIES[id],
    exportMetadata: (controls) => [distributionActiveState(metric, controls, result)],
  });
  const evidenceController = new OverviewFigureController({
    initialControls: { offset: 0 },
  });
  let result: OverviewDistributionResult | null = null;
  let figure: HTMLElement | null = null;
  let manualTimer: number | null = null;
  let selectedInterval: { key: string; lower: number; upper: number; upperInclusive: boolean } | null = null;

  const controls = document.createElement("div");
  controls.className = "cw-overview-distribution-controls";
  const viewControl = createOverviewSegmentedControl(
    "View",
    [
      { value: "histogram", label: "Histogram" },
      { value: "ecdf", label: "ECDF" },
      { value: "qq", label: "Q–Q" },
    ],
    initial.view,
    (view) => {
      controller.updateControls({ view });
      syncControls();
      refreshFigure();
    },
  );
  const binsControl = createOverviewSegmentedControl(
    "Binning",
    [
      { value: "auto", label: "Automatic" },
      { value: "manual", label: "Custom" },
    ],
    initial.histogramRule,
    (histogramRule) => {
      controller.updateControls({ histogramRule });
      syncControls();
      void requestDistribution();
    },
  );
  const manualControl = createOverviewRangeControl("Bins", {
    minimum: 8,
    maximum: 100,
    step: 1,
    initialValue: initial.manualBins,
    onInput: (manualBins) => {
      controller.updateControls({ manualBins });
      if (manualTimer !== null) window.clearTimeout(manualTimer);
      manualTimer = window.setTimeout(() => void requestDistribution(), 180);
    },
    onChange: (manualBins) => {
      controller.updateControls({ manualBins });
      if (manualTimer !== null) window.clearTimeout(manualTimer);
      void requestDistribution();
    },
  });
  const scaleControl = createOverviewSegmentedControl(
    "Scale",
    [{ value: "log", label: "Logarithmic" }, { value: "linear", label: "Linear" }],
    initial.xScale,
    (xScale) => {
      controller.updateControls({ xScale });
      void requestDistribution();
    },
  );
  controls.append(viewControl, binsControl, manualControl, ...(isLength ? [scaleControl] : []));

  const evidence = createDistributionEvidenceSection();

  function syncControls(): void {
    const state = controller.controls;
    binsControl.hidden = state.view !== "histogram";
    manualControl.hidden = state.view !== "histogram" || state.histogramRule !== "manual";
    scaleControl.hidden = !isLength || state.view === "qq";
  }

  function renderSvg(): SVGSVGElement {
    if (!result) {
      const message = controller.error
        ? `Distribution unavailable: ${controller.error}`
        : "Loading exact distribution diagnostics…";
      return renderFigureStatusSvg(id, message);
    }
    return renderDistributionDiagnosticSvg(id, buildDistributionDiagnosticGeometry({
      result,
      metric,
      view: controller.controls.view,
      yMode: "share",
      qqReference: isLength ? "lognormal" : "normal",
      labelsMode: "extremes",
      mattrWindow: corpus.lexicalSpectrum.mattrWindow,
      pinnedKeys: controller.pins,
    }));
  }

  function refreshFigure(): void {
    if (!figure) return;
    replaceFigureSvg(figure, renderSvg(), {
      captionStats: distributionCaptionStats(metric, result, corpus),
      captionReading: distributionReading(metric, controller.controls.view),
    });
  }

  async function requestDistribution(): Promise<void> {
    result = null;
    refreshFigure();
    const state = controller.controls;
    const applied = await controller.loadLatest(
      () => queryOverviewDistribution({
        metric,
        histogramRule: state.histogramRule,
        manualBinCount: state.manualBins,
        xScale: isLength ? state.xScale : "linear",
        qqReference: isLength ? "lognormal" : "normal",
        maximumReturnedPointCount: 800,
      }, effectiveCleaningConfig()),
      (next) => { result = next; },
    );
    if (applied) refreshFigure();
  }

  async function loadEvidence(offset: number): Promise<void> {
    if (!selectedInterval) return;
    evidence.hidden = false;
    evidence.dataset.loading = "true";
    renderEvidenceMessage(evidence, "Loading matching documents…");
    const interval = selectedInterval;
    const applied = await evidenceController.loadLatest(
      () => queryOverviewDistributionEvidence({
        metric,
        lowerBound: interval.lower,
        upperBound: interval.upper,
        lowerInclusive: true,
        upperInclusive: interval.upperInclusive,
        descending: false,
        offset,
        limit: 25,
      }, effectiveCleaningConfig()),
      (page) => renderDistributionEvidencePage(evidence, page, metric, loadEvidence),
    );
    if (applied) delete evidence.dataset.loading;
  }

  function handlePin(key: string, pinned: boolean): void {
    if (!result) return;
    if (!pinned && selectedInterval?.key === key) {
      selectedInterval = null;
      evidence.hidden = true;
      evidenceController.invalidatePendingLoad();
      return;
    }
    if (!pinned) return;
    if (key.startsWith("distribution-witness:")) {
      const documentId = Number(key.split(":")[1]);
      const witness = [...result.lowWitnesses, ...result.highWitnesses]
        .find((candidate) => candidate.documentId === documentId);
      if (!witness) return;
      selectedInterval = {
        key,
        lower: witness.metricValue,
        upper: witness.metricValue,
        upperInclusive: true,
      };
      void loadEvidence(0);
      return;
    }
    if (!key.startsWith("distribution-bin:")) return;
    const index = Number(key.split(":")[1]);
    const bin = result.histogramBins[index];
    if (!bin) return;
    selectedInterval = {
      key,
      lower: bin.lowerBound,
      upper: bin.upperBound,
      upperInclusive: index + 1 === result.histogramBins.length,
    };
    void loadEvidence(0);
  }

  syncControls();
  figure = createOverviewFigure({
    id,
    title: isLength ? "Document-length distribution" : "MATTR distribution",
    subtitle: isLength
      ? "How unevenly is the corpus divided among documents?"
      : "How much does lexical diversity vary between documents?",
    controls,
    controller,
    onPinChange: handlePin,
    onClearPins: () => {
      selectedInterval = null;
      evidence.hidden = true;
      evidenceController.invalidatePendingLoad();
    },
    captionStats: distributionCaptionStats(metric, result, corpus),
    captionReading: distributionReading(metric, controller.controls.view),
    explanation: createFigureExplanation(
      ...(isLength ? ["distributionDiagnostics" as const] : ["mattr" as const, "distributionDiagnostics" as const]),
    ),
    renderSvg,
  });
  figure.appendChild(evidence);
  void requestDistribution();
  return figure;
}

function distributionActiveState(
  metric: OverviewDistributionMetric,
  controls: DistributionFigureControls,
  result: OverviewDistributionResult | null,
): string {
  if (controls.view === "qq") {
    return `Q–Q diagnostic · ${metric === "documentLength" ? "lognormal" : "normal"} reference`;
  }
  if (controls.view === "ecdf") {
    const scale = metric === "documentLength" ? controls.xScale : "linear";
    return `ECDF · ${scale} scale${result?.ecdfCompressed ? " · deterministically compressed" : ""}`;
  }
  const rule = result?.effectiveRule ?? controls.histogramRule;
  const bins = result?.effectiveBinCount ?? controls.manualBins;
  const scale = metric === "documentLength" ? controls.xScale : "linear";
  return `Histogram · ${histogramRuleLabel(rule)} · ${formatNumber(bins)} bins · ${scale} scale · share of documents`;
}

function distributionCaptionStats(
  metric: OverviewDistributionMetric,
  result: OverviewDistributionResult | null,
  corpus: AnalysisCorpus,
): string {
  const population = result?.population ?? (metric === "mattr"
    ? corpus.lexicalSpectrum.mattrDocsUsed
    : corpus.totals.documents);
  const identity = metric === "mattr"
    ? `Document MATTR · window ${formatNumber(corpus.lexicalSpectrum.mattrWindow)} · ${formatNumber(population)} eligible documents`
    : `${formatNumber(population)} documents`;
  return identity;
}

function distributionReading(
  metric: OverviewDistributionMetric,
  view: DistributionFigureControls["view"],
): string {
  if (metric === "documentLength" && view === "qq") {
    return "A near-straight lognormal Q–Q pattern means log document lengths resemble a normal reference. It does not mean raw lengths are normal or that the corpus is balanced.";
  }
  if (metric === "mattr" && view === "qq") {
    return "The normal reference is a diagnostic comparison, not an assumption that bounded MATTR values ought to follow a normal distribution.";
  }
  if (view === "ecdf") {
    return "Read any point as the share of eligible documents at or below that value. Deterministic compression preserves exact cumulative counts at retained checkpoints.";
  }
  return metric === "documentLength"
    ? "True numeric bin widths show the complete document population; the selected bin rule and x scale affect appearance, so read the median and IQR alongside the bars."
    : "MATTR compares lexical variety within equal 100-token windows. Higher is not automatically better, and documents shorter than the window remain visible in the eligibility strip.";
}

function histogramRuleLabel(rule: string): string {
  switch (rule) {
    case "auto": return "automatic bins";
    case "manual": return "custom bins";
    case "sturges": return "Sturges";
    case "scott": return "Scott";
    case "freedmanDiaconis": return "Freedman–Diaconis";
    default: return rule;
  }
}

function createDistributionEvidenceSection(): HTMLElement {
  const section = document.createElement("section");
  section.className = "cw-overview-distribution-evidence";
  section.hidden = true;
  section.setAttribute("aria-live", "polite");
  return section;
}

function renderEvidenceMessage(section: HTMLElement, message: string): void {
  const heading = document.createElement("h5");
  heading.textContent = "Documents in the selected interval";
  const status = document.createElement("p");
  status.textContent = message;
  section.replaceChildren(heading, status);
}

function renderDistributionEvidencePage(
  section: HTMLElement,
  page: OverviewDistributionEvidencePage,
  metric: OverviewDistributionMetric,
  loadPage: (offset: number) => Promise<void>,
): void {
  const heading = document.createElement("h5");
  heading.textContent = `${formatNumber(page.totalMatchingDocuments)} matching documents`;
  const list = document.createElement("ul");
  list.className = "cw-overview-evidence-list";
  for (const item of page.items) {
    const row = document.createElement("li");
    row.className = "cw-overview-evidence-row";
    const name = document.createElement("strong");
    name.textContent = item.documentName;
    name.title = item.relativePath;
    const tokens = document.createElement("span");
    tokens.textContent = `${formatNumber(Number(item.tokenCount))} tokens`;
    const value = document.createElement("span");
    value.textContent = metric === "mattr"
      ? `MATTR ${formatFixedNumber(item.metricValue, 3)}`
      : `${formatNumber(item.metricValue)} tokens`;
    row.append(name, tokens, value);
    list.appendChild(row);
  }
  const pagination = document.createElement("div");
  pagination.className = "cw-overview-evidence-pagination";
  const previous = document.createElement("button");
  previous.type = "button";
  previous.className = "cw-overview-figure-action";
  previous.textContent = "Previous";
  previous.disabled = page.offset === 0;
  previous.addEventListener("click", () => void loadPage(Math.max(0, page.offset - page.limit)));
  const next = document.createElement("button");
  next.type = "button";
  next.className = "cw-overview-figure-action";
  next.textContent = "Next";
  next.disabled = page.offset + page.items.length >= page.totalMatchingDocuments;
  next.addEventListener("click", () => void loadPage(page.offset + page.limit));
  pagination.append(previous, next);
  section.replaceChildren(heading, list, pagination);
}

function renderDocumentBalance(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Document balance");
  const balance = corpus.balance;
  panel.appendChild(renderDocumentLengthDistribution(corpus));
  const lengthHistogramFigure = renderLengthHistogramFigure(corpus);
  if (lengthHistogramFigure) panel.appendChild(lengthHistogramFigure);

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
    });
  }
  if (balance.coefficientOfVariation !== null) {
    rows.push({
      label: "Coefficient of variation",
      value: formatFixedNumber(balance.coefficientOfVariation, 3),
    });
  }
  if (balance.effectiveDocuments !== null) {
    rows.push({
      label: "Effective documents",
      value: `${formatFixedNumber(balance.effectiveDocuments, 1)} of ${formatNumber(corpus.totals.documents)}`,
    });
  }
  if (rows.length > 0) {
    panel.appendChild(createMetricTable(rows));
  }
  if ([balance.gini, balance.coefficientOfVariation, balance.effectiveDocuments].some((value) => value !== null)) {
    panel.appendChild(createGroupedMethodDisclosure("Document balance", [
      "giniCoefficient",
      "coefficientOfVariation",
      "effectiveDocuments",
    ]));
  }
  if (balance.largestShare !== null && balance.largestShare >= BALANCE_ATTENTION_SHARE) {
    const imbalanceWarning = document.createElement("p");
    imbalanceWarning.className = "analysis-panel-note";
    imbalanceWarning.textContent =
      `One document holds ${formatPercent(balance.largestShare)} of all tokens. Corpus-level frequencies may largely reflect that document.`;
    panel.appendChild(imbalanceWarning);
  }
  appendPanelReading(panel, balance.reading);
  return panel;
}

function renderExtractionQuality(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Extraction quality");
  const quality = corpus.extractionQuality;
  const rows: OverviewMetricRow[] = [
    {
      ...countWithInlineShareRow("Word-like types", quality.wordLikeCount, quality.wordLikeShare),
      description: "Counted as word-like when it contains at least one letter.",
    },
    {
      ...countWithInlineShareRow("Numeric-only types", quality.numericOnlyCount, quality.numericOnlyShare),
      description: "Counted as numeric-only when it contains digits and only numeric separators (. , : - /).",
    },
  ];
  if (quality.singleCharacterCount !== null) {
    rows.push(countWithInlineShareRow(
      "Single-character types",
      quality.singleCharacterCount,
      quality.singleCharacterShare,
    ));
  }

  panel.appendChild(createMetricTable(rows));
  appendPanelReading(panel, quality.reading);
  return panel;
}

function renderRankFrequencyFigure(corpus: AnalysisCorpus): HTMLElement {
  const rank = corpus.rankFrequency;
  interface RankControls extends FigureControlState {
    view: "curve" | "residuals";
  }
  const controller = new OverviewFigureController<RankControls>({
    initialControls: { view: "curve" },
    policy: FIGURE_INTERACTION_POLICIES["overview-rank-frequency"],
    exportMetadata: (controls) => [
      controls.view === "curve"
        ? `Curve · frequency per million · Zipf fit over ranks ${formatRankRange(rank.fittedRankStart, rank.fittedRankEnd)}`
        : `Residuals · log observed minus log fitted · Zipf fit over ranks ${formatRankRange(rank.fittedRankStart, rank.fittedRankEnd)}`,
    ],
  });
  let figure: HTMLElement | null = null;
  const figureStats = () => `Zipf fit f(r) = C r^-alpha · alpha ${formatOptionalFixed(rank.zipfExponent, 3)} · C ${formatOptionalFixed(rank.zipfCoefficient, 2)} · R-squared ${formatOptionalFixed(rank.rSquared, 3)} · fitted ranks ${formatRankRange(rank.fittedRankStart, rank.fittedRankEnd)}`;
  const renderSvg = () => renderRankFrequencySvg(
    "overview-rank-frequency",
    buildRankFrequencyGeometry({
      points: rank.points.map((point) => ({
        rank: point.rank,
        token: point.token ?? undefined,
        frequency: point.frequency,
        ratePerMillion: point.ratePerMillion,
      })),
      yMode: "rate",
      totalTokens: corpus.totals.tokens,
      zipfExponent: rank.zipfExponent,
      zipfCoefficient: rank.zipfCoefficient,
      fittedRankStart: rank.fittedRankStart,
      fittedRankEnd: rank.fittedRankEnd,
      view: controller.controls.view,
      fitVisible: true,
      labelsMode: "examples",
    }),
    controller.controls.view === "residuals"
      ? "Log observed − log fitted frequency"
      : "Frequency per million (log scale)",
  );
  const controls = document.createElement("div");
  const viewControl = createOverviewSegmentedControl(
    "View",
    rank.zipfExponent === null || rank.zipfCoefficient === null
      ? [{ value: "curve", label: "Curve" }]
      : [{ value: "curve", label: "Curve" }, { value: "residuals", label: "Residuals" }],
    "curve",
    (view) => {
      controller.updateControls({ view });
      if (figure) replaceFigureSvg(figure, renderSvg());
    },
  );
  controls.append(viewControl);
  const rarity = rank.zipfExponent === null
    ? "the fitted rarity ratio is unavailable"
    : `ten times deeper in the rank list = ~${formatFixedNumber(10 ** rank.zipfExponent, 1)}x rarer`;
  figure = createOverviewFigure({
    id: "overview-rank-frequency",
    title: "Rank-frequency",
    subtitle: "How quickly does frequency fall as vocabulary rank increases?",
    controls,
    controller,
    captionStats: figureStats(),
    captionReading: `Both axes are logarithmic in Curve view. The top three ranks and frequency-one tail remain visible but are not used to estimate the fit; R-squared is not proof of a universal law. Per-million scaling changes units, not curve shape or alpha; ${rarity}.`,
    explanation: createFigureExplanation("zipfFit"),
    renderSvg,
  });
  return figure;
}

function renderVocabularyGrowthFigure(corpus: AnalysisCorpus): HTMLElement {
  const growth = corpus.vocabularyGrowth;
  interface GrowthControls extends FigureControlState {
    view: "growth" | "heaps";
  }
  const controller = new OverviewFigureController<GrowthControls>({
    initialControls: { view: "growth" },
    policy: FIGURE_INTERACTION_POLICIES["overview-vocabulary-growth"],
    exportMetadata: (controls) => [
      controls.view === "growth" ? "Growth · observed and Heaps fit" : "Heaps fit · observed and fitted model",
    ],
  });
  let figure: HTMLElement | null = null;
  const betaYield = growth.heapsBeta === null
    ? "unavailable"
    : `${formatFixedNumber(((2 ** growth.heapsBeta) - 1) * 100, 1)}%`;
  const renderSvg = () => renderVocabularyGrowthSvg(
    "overview-vocabulary-growth",
    buildVocabularyGrowthGeometry({
      points: growth.points,
      heapsK: growth.heapsK,
      heapsBeta: growth.heapsBeta,
      view: controller.controls.view,
      fitVisible: true,
      labelsMode: "endpoint",
      pinnedKeys: controller.pins,
    }),
  );
  const controls = document.createElement("div");
  controls.append(
    createOverviewSegmentedControl(
      "View",
      [{ value: "growth", label: "Growth" }, { value: "heaps", label: "Heaps fit" }],
      "growth",
      (view) => {
        controller.updateControls({ view });
        if (figure) replaceFigureSvg(figure, renderSvg());
      },
    ),
  );
  figure = createOverviewFigure({
    id: "overview-vocabulary-growth",
    title: "Vocabulary growth",
    subtitle: "How quickly does the corpus encounter new types as tokens accumulate?",
    controls,
    controller,
    captionStats: `k ${formatOptionalFixed(growth.heapsK, 3)} · beta ${formatOptionalFixed(growth.heapsBeta, 3)} · R-squared ${formatOptionalFixed(growth.rSquared, 3)}`,
    captionReading: `V(N) = K N^beta: each doubling of tokens yields ~${betaYield} more types. Checkpoints follow stable source order; the fit describes this snapshot, not a universal vocabulary ceiling.`,
    explanation: createFigureExplanation("heapsFit"),
    renderSvg,
  });
  return figure;
}

function renderLexicalSpectrumFigure(corpus: AnalysisCorpus): HTMLElement {
  const spectrum = corpus.lexicalSpectrum;
  const controller = new OverviewFigureController({
    initialControls: {},
    policy: FIGURE_INTERACTION_POLICIES["overview-lexical-spectrum"],
    exportMetadata: () => ["Exact frequency spectrum · logarithmic axes"],
  });
  let figure: HTMLElement | null = null;
  const truncation = spectrum.frequencySpectrumTruncated
    ? `showing ${formatNumber(spectrum.frequencySpectrum.length)} of ${formatNumber(spectrum.frequencySpectrumTotal)} observed frequencies`
    : `${formatNumber(spectrum.frequencySpectrumTotal)} observed frequencies`;
  const renderSvg = () => renderLexicalSpectrumSvg(
    "overview-lexical-spectrum",
    buildLexicalSpectrumGeometry({
      points: spectrum.frequencySpectrum,
      hapaxCount: spectrum.hapaxCount,
      disLegomenaCount: spectrum.disLegomenaCount,
      band3To9Count: spectrum.band3To9Count,
      band10plusCount: spectrum.band10plusCount,
      totalTypes: spectrum.types,
      yMode: "count",
      bandsVisible: true,
      labelsMode: "authored",
      pinnedKeys: controller.pins,
    }),
  );
  figure = createOverviewFigure({
    id: "overview-lexical-spectrum",
    title: "Lexical spectrum",
    subtitle: "How many types occur once, twice, or at each higher frequency?",
    controller,
    captionStats: `hapax ${formatNumber(spectrum.hapaxCount)} · dis ${formatNumber(spectrum.disLegomenaCount)} · ${truncation}`,
    captionReading: "Each point is V(f) for one exact integer frequency; bars are avoided because log-scale bar lengths would imply intervals that were not observed.",
    explanation: createFigureExplanation("exactLexicalSpectrum"),
    renderSvg,
  });
  return figure;
}

function renderMattrDistributionFigure(corpus: AnalysisCorpus): HTMLElement {
  return renderDistributionDiagnosticFigure(corpus, "mattr");
}

function renderCoverageFigure(corpus: AnalysisCorpus): HTMLElement {
  const concentration = corpus.frequencyConcentration;
  interface CoverageControls extends FigureControlState {
    target: "0.5" | "0.8" | "0.9" | "0.95";
  }
  const controller = new OverviewFigureController<CoverageControls>({
    initialControls: { target: "0.5" },
    policy: FIGURE_INTERACTION_POLICIES["overview-cumulative-coverage"],
    exportMetadata: (controls) => [
      `Cumulative token coverage · ${formatPercent(Number(controls.target), 0)} target`,
    ],
  });
  let figure: HTMLElement | null = null;
  const targetCrossing = () => concentration.exactTargetCrossings.find(
    (crossing) => Math.abs(crossing.targetShare - Number(controller.controls.target)) < 1e-9,
  ) ?? null;
  const tenTypes = concentration.top10Coverage === null
    ? "Ten-type coverage is unavailable"
    : `Ten types carry ${formatPercent(concentration.top10Coverage)} of every token`;
  const renderSvg = () => renderCoverageSvg(
    "overview-cumulative-coverage",
    buildCoverageGeometry({
      points: concentration.cumulativeCoverage,
      milestones: [
        { k: 10, share: concentration.top10Coverage },
        { k: 100, share: concentration.top100Coverage },
        { k: 1_000, share: concentration.top1000Coverage },
        { k: 10_000, share: concentration.top10000Coverage },
      ],
      totalTypes: corpus.totals.types,
      xMode: "topK",
      targetShare: Number(controller.controls.target),
      targetCrossing: targetCrossing()
        ? { k: targetCrossing()!.firstRank, share: targetCrossing()!.actualCumulativeShare }
        : null,
      labelsMode: "milestones",
      bandShares: deriveCoverageBandShares({
        totalTypes: corpus.totals.types,
        cumulative: [
          { k: 10, share: concentration.top10Coverage },
          { k: 100, share: concentration.top100Coverage },
          { k: 1_000, share: concentration.top1000Coverage },
          { k: 10_000, share: concentration.top10000Coverage },
        ],
      }),
      pinnedKeys: controller.pins,
    }),
  );
  const controls = document.createElement("div");
  controls.append(
    createOverviewSegmentedControl(
      "Coverage target",
      [
        { value: "0.5", label: "50%" },
        { value: "0.8", label: "80%" },
        { value: "0.9", label: "90%" },
        { value: "0.95", label: "95%" },
      ],
      "0.5",
      (target) => {
        controller.updateControls({ target });
        if (figure) replaceFigureSvg(figure, renderSvg());
      },
    ),
  );
  figure = createOverviewFigure({
    id: "overview-cumulative-coverage",
    title: "Cumulative coverage",
    subtitle: "How much token mass is carried by the most frequent vocabulary?",
    controls,
    controller,
    captionStats: `top 10 ${formatOptionalPercent(concentration.top10Coverage)} · top 100 ${formatOptionalPercent(concentration.top100Coverage)} · top 1,000 ${formatOptionalPercent(concentration.top1000Coverage)} · top 10,000 ${formatOptionalPercent(concentration.top10000Coverage)}`,
    captionReading: `${tenTypes}. The curve is cumulative: a steep head indicates concentration of token mass, not semantic completeness; function words often dominate the head.`,
    explanation: createFigureExplanation("topNCoverage"),
    renderSvg,
  });
  return figure;
}

function renderDispersionFigure(corpus: AnalysisCorpus): HTMLElement {
  const dispersion = corpus.dispersion;
  const controller = new OverviewFigureController({
    initialControls: {},
    policy: FIGURE_INTERACTION_POLICIES["overview-dispersion-profile"],
    exportMetadata: () => ["Exclusive document-reach bands · type-count scale"],
  });
  let figure: HTMLElement | null = null;
  const buckets = dispersion.profileBuckets.map((bucket) => ({
    key: `df-profile-${bucket.lowerBound}-${bucket.upperBound ?? "plus"}`,
    label: bucket.label,
    lowerBound: bucket.lowerBound,
    upperBound: bucket.upperBound,
    count: bucket.count,
    share: bucket.share,
    sampleTypes: bucket.sampleTypes,
  }));
  const renderSvg = () => renderDispersionSvg(
    "overview-dispersion-profile",
    buildDispersionGeometry({
      buckets,
      totalTypes: corpus.totals.types,
      totalDocuments: corpus.totals.documents,
      xMode: "count",
      reachLabels: "documents",
      labelsMode: "countsShares",
      cumulativeCallout: {
        count: dispersion.dfAtLeast90pctCount,
        examples: dispersion.universalTypes.map((type) => type.token),
      },
      pinnedKeys: controller.pins,
    }),
  );
  figure = createOverviewFigure({
    id: "overview-dispersion-profile",
    title: "Dispersion profile",
    subtitle: "How widely across documents does the vocabulary reach?",
    controller,
    layoutFamily: "profile",
    captionStats: formatDispersionProfileStats(corpus.totals.types, dispersion.profileBuckets.length, dispersion.dfAtLeast90pctCount),
    captionDetail: formatUniversalTypeCaption(
      dispersion.universalTypes.map((type) => type.token),
      dispersion.dfAtLeast90pctCount,
    ),
    captionReading: "Document frequency counts documents containing a type. Profile bands are mutually exclusive; the ≥90% callout is cumulative. Reach is not raw frequency, while Gries's DP also accounts for unequal document sizes.",
    explanation: createFigureExplanation("documentFrequencySpectrum", "griesDp"),
    renderSvg,
  });
  return figure;
}

function renderDispersionPlaneFigure(corpus: AnalysisCorpus): HTMLElement | null {
  const plane = corpus.dispersion.plane;
  if (plane === null) return null;
  const controller = new OverviewFigureController({
    initialControls: {},
    policy: FIGURE_INTERACTION_POLICIES["overview-dispersion-plane"],
    exportMetadata: () => ["Raw frequency against Gries's DP · logarithmic density"],
  });
  let figure: HTMLElement | null = null;
  const renderSvg = () => renderDispersionPlaneSvg(
    "overview-dispersion-plane",
    buildDispersionPlaneGeometry({
      plane,
      totalTypes: corpus.totals.types,
      totalTokens: corpus.totals.tokens,
      frequencyMode: "raw",
      densityMode: "log",
      examplesMode: "labelled",
      marginalsVisible: true,
      thresholdsVisible: true,
      pinnedKeys: controller.pins,
    }),
  );
  figure = createOverviewFigure({
    id: "overview-dispersion-plane",
    title: "Dispersion plane",
    subtitle: "Which types are frequent, and which are evenly distributed across documents?",
    controller,
    layoutFamily: "plane",
    captionStats: formatDispersionPlaneStats(
      corpus.totals.types,
      plane.xBins,
      plane.yBins,
      plane.loudButLocalCount,
      plane.loudButLocalMinFrequency,
      plane.highDispersionThreshold,
    ),
    captionReading: "Density colour represents the number of types in each frequency–DP region. Witnesses are deliberately selected examples, not a random sample. Lower DP indicates more even distribution; higher DP indicates concentration. Log density changes visual contrast, not counts; thresholds are descriptive orientation conventions and no significance test is performed.",
    explanation: createFigureExplanation("dispersionPlaneDensity", "griesDp"),
    renderSvg,
  });
  return figure;
}

function renderLexicalSpectrum(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Lexical spectrum");
  const spectrum = corpus.lexicalSpectrum;
  const band3To9Note = document.createElement("p");
  band3To9Note.className = "analysis-panel-note";
  band3To9Note.textContent = "3-9: recurring but still rare types.";
  const band10PlusNote = document.createElement("p");
  band10PlusNote.className = "analysis-panel-note";
  band10PlusNote.textContent = "10+: frequent types - the same threshold feeds the dispersion profile.";
  panel.append(band3To9Note, band10PlusNote);
  panel.appendChild(renderLexicalSpectrumFigure(corpus));

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
      description: "median across eligible documents",
    },
    {
      label: "MTLD",
      value: mtldValue,
      glossaryId: "mtld",
      description: "median across eligible documents",
    },
  ];
  lexicalDiversity.append(lexicalDiversityTitle, createMetricTable(diversityRows));
  panel.appendChild(lexicalDiversity);
  const mattrDistributionFigure = renderMattrDistributionFigure(corpus);
  panel.appendChild(mattrDistributionFigure);
  appendPanelReading(panel, spectrum.reading);
  return panel;
}

function renderFrequencyConcentration(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Frequency concentration");
  const concentration = corpus.frequencyConcentration;
  const rows: OverviewMetricRow[] = [];
  if (concentration.evennessJ !== null) {
    rows.push({
      label: "Normalised entropy / evenness",
      value: formatFixedNumber(concentration.evennessJ, 3),
    });
  }
  if (concentration.shannonEntropyBits !== null) {
    rows.push({
      label: "Shannon entropy",
      labelSuffix: "(bits)",
      value: formatFixedNumber(concentration.shannonEntropyBits, 3),
    });
  }
  if (concentration.effectiveVocabularyEntropy !== null) {
    rows.push({
      label: "Effective vocabulary (perplexity)",
      value: `≈ ${formatNumber(concentration.effectiveVocabularyEntropy)} types`,
    });
  }
  if (rows.length > 0) {
    panel.appendChild(createMetricTable(rows));
    panel.appendChild(createGroupedMethodDisclosure("Frequency concentration", [
      "shannonEntropy",
      "normalisedEntropy",
      "effectiveVocabulary",
    ]));
  }
  panel.appendChild(renderCoverageFigure(corpus));
  appendPanelReading(panel, concentration.reading);
  return panel;
}

function renderRankFrequency(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Rank–frequency fit");
  const rank = corpus.rankFrequency;
  const rows: OverviewMetricRow[] = [
    { label: "Sampled rank points", value: formatNumber(rank.points.length) },
  ];
  if (rank.zipfExponent !== null) {
    rows.push({
      label: "Zipf exponent",
      value: formatFixedNumber(rank.zipfExponent, 3),
    });
  }
  if (rank.zipfCoefficient !== null) {
    rows.push({
      label: "Zipf coefficient (raw-frequency scale)",
      value: formatFixedNumber(rank.zipfCoefficient, 2),
    });
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
  panel.appendChild(renderRankFrequencyFigure(corpus));
  appendPanelReading(panel, rank.reading);
  return panel;
}

function renderVocabularyGrowth(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Vocabulary growth");
  const growth = corpus.vocabularyGrowth;
  const finalPoint = growth.points[growth.points.length - 1];
  const rows: OverviewMetricRow[] = [
    { label: "Vocabulary-growth checkpoints", value: formatNumber(growth.points.length) },
  ];
  if (finalPoint) {
    rows.push({
      label: "Final observed point",
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
  panel.appendChild(renderVocabularyGrowthFigure(corpus));
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

  if (corpus.balance.largestShare !== null && corpus.balance.largestShare >= BALANCE_ATTENTION_SHARE) {
    const imbalanceWarning = document.createElement("p");
    imbalanceWarning.className = "analysis-panel-note";
    imbalanceWarning.textContent =
      `One document holds ${formatPercent(corpus.balance.largestShare)} of all tokens. DP has a floor of about half that share for any type absent from it - a high median DP here may reflect imbalance, not clumpy vocabulary.`;
    panel.appendChild(imbalanceWarning);
  }
  const universe = document.createElement("p");
  universe.className = "analysis-panel-note";
  universe.textContent = "Universe: all analysis types - independent of Frequency List filters.";
  panel.appendChild(universe);
  panel.appendChild(renderDispersionFigure(corpus));
  const planeFigure = renderDispersionPlaneFigure(corpus);
  if (planeFigure) panel.appendChild(planeFigure);

  appendPanelReading(panel, dispersion.reading);
  return panel;
}

function renderBuildDetails(corpus: AnalysisCorpus): HTMLElement {
  const details = document.createElement("details");
  details.className = "analysis-panel analysis-build-details cw-panel cw-panel--analysis";

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
  dom.concordanceSearchButton.addEventListener("click", runConcordanceSearch);
  dom.concordanceCancelButton.addEventListener("click", () => {
    void cancelConcordance();
  });
  dom.concordanceQueryInput.addEventListener("input", handleConcordanceQueryInput);
  dom.concordanceQueryInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      runConcordanceSearch();
    }
  });
  dom.concordanceWindowInput.addEventListener("change", handleConcordanceWindowChange);
  dom.concordanceCaseSensitiveInput.addEventListener("change", handleConcordanceCaseChange);
  dom.concordanceSort1Select.addEventListener("change", () => handleConcordanceSortChange(1));
  dom.concordanceSort2Select.addEventListener("change", () => handleConcordanceSortChange(2));
  dom.concordanceSort3Select.addEventListener("change", () => handleConcordanceSortChange(3));
  dom.concordancePageSizeInput.addEventListener("change", handleConcordancePageSizeChange);
  dom.concordanceFilesButton.addEventListener("click", openFrequencyFilesModal);
  dom.concordancePagePreviousButton.addEventListener("click", showPreviousConcordancePage);
  dom.concordancePageNextButton.addEventListener("click", showNextConcordancePage);
  dom.concordanceLinesFilterClear.addEventListener("click", handleClearConcordanceLinesContextFilter);
  dom.concordanceCollocationSpanInput.addEventListener("change", handleConcordanceCollocationAggregationControlChange);
  dom.concordanceCollocationDirectionSelect.addEventListener("change", handleConcordanceCollocationAggregationControlChange);
  dom.concordanceCollocationMeasureSelect.addEventListener("change", handleConcordanceCollocationMeasureChange);
  dom.concordanceCollocationMinO11Input.addEventListener("change", handleConcordanceCollocationPresentationControlChange);
  dom.concordanceCollocationMinFreqInput.addEventListener("change", handleConcordanceCollocationPresentationControlChange);
  dom.concordanceCollocationsTableHead.addEventListener("click", handleConcordanceCollocationSortClick);
  dom.concordanceCollocationsTableHead.addEventListener("click", handleConcordancePositionProfileModeClick);
  dom.concordanceCollocationsMoreButton.addEventListener("click", handleConcordanceCollocationsShowMore);
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
  void initialiseConcordanceState();

  refreshStageNavigator();
  renderAnalysisTools();
  renderAnalysisToolPanels();
}
