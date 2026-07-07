import katex from "katex";
import "katex/dist/katex.min.css";

import {
  type AnalysisInputSource,
  type CorpusOverview as AnalysisCorpus,
  type CorpusOverviewFinding,
  type CorpusOverviewFindingSeverity,
} from "../analysis";
import { BOOLEAN_CLEANING_CONFIG_KEYS } from "../config";
import { dom } from "../dom";
import { effectiveCleaningConfig } from "../effective-cleaning-config";
import { DEFAULT_FREQUENCY_RATE_SCALE } from "../frequency";
import {
  overviewGlossary,
  type OverviewGlossaryEntry,
  type OverviewGlossaryId,
} from "../overview-glossary";
import {
  createOverviewFigure,
  createSegmentedControl,
  replaceFigureSvg,
} from "../plots/figure";
import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildLexicalSpectrumGeometry,
  buildRankFrequencyGeometry,
  buildVocabularyGrowthGeometry,
} from "../plots/model";
import {
  renderCoverageSvg,
  renderDispersionSvg,
  renderLexicalSpectrumSvg,
  renderRankFrequencySvg,
  renderVocabularyGrowthSvg,
} from "../plots/render";
import type { ProjectLanguage } from "../generated/ProjectLanguage.js";
import { projectLanguageLabel } from "../project-creation-dialog";
import { sourceHasExtractedText } from "../sources";
import { state, type WorkbenchId } from "../state";
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
  syncConcordanceCollocationStopwordsToggle,
} from "./concordance/collocations";
import {
  collapseConcordanceExpansionFromControlChange,
  handleConcordanceExpansionEscape,
  handleConcordancePageSizeChange,
  handleConcordanceSortChange,
  showNextConcordancePage,
  showPreviousConcordancePage,
} from "./concordance/lines";
import {
  handleConcordanceCaseChange,
  handleConcordanceQueryInput,
  handleConcordanceViewTabClick,
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
    const disclosure = createGlossaryDisclosure(glossaryId, undefined, title);
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
      }, segment.label);
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
    { label: "Tokens", value: formatNumber(corpus.totals.tokens), glossaryId: "token" },
    { label: "Types", value: formatNumber(corpus.totals.types), glossaryId: "type" },
    { label: "Documents", value: formatNumber(corpus.totals.documents), glossaryId: "documentUnit" },
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
  let yMode: "rate" | "raw" = "rate";
  let figure: HTMLElement | null = null;
  const renderSvg = () => renderRankFrequencySvg(
    "overview-rank-frequency",
    buildRankFrequencyGeometry({
      points: rank.points.map((point) => ({
        rank: point.rank,
        token: point.token ?? undefined,
        frequency: point.frequency,
        ratePerMillion: point.ratePerMillion,
      })),
      yMode,
      totalTokens: corpus.totals.tokens,
      zipfExponent: rank.zipfExponent,
      zipfCoefficient: rank.zipfCoefficient,
      fittedRankStart: rank.fittedRankStart,
      fittedRankEnd: rank.fittedRankEnd,
    }),
    yMode === "rate" ? "rate per million" : "raw count",
  );
  const controls = createSegmentedControl(
    "Rank-frequency y axis",
    [
      { value: "rate", label: "per M" },
      { value: "raw", label: "raw" },
    ],
    yMode,
    (value) => {
      yMode = value;
      if (figure) replaceFigureSvg(figure, renderSvg());
    },
  );
  const rarity = rank.zipfExponent === null
    ? "the fitted rarity ratio is unavailable"
    : `ten times deeper in the rank list = ~${formatFixedNumber(10 ** rank.zipfExponent, 1)}x rarer`;
  figure = createOverviewFigure({
    id: "overview-rank-frequency",
    title: "Rank-frequency",
    subtitle: "Zipf fit on log rank and log frequency.",
    controls,
    captionStats: `alpha ${formatOptionalFixed(rank.zipfExponent, 3)} · R-squared ${formatOptionalFixed(rank.rSquared, 3)} · fitted ranks ${formatRankRange(rank.fittedRankStart, rank.fittedRankEnd)}`,
    captionReading: `${rarity}; hollow points are excluded from the fit.`,
    renderSvg,
  });
  return figure;
}

function renderVocabularyGrowthFigure(corpus: AnalysisCorpus): HTMLElement {
  const growth = corpus.vocabularyGrowth;
  const betaYield = growth.heapsBeta === null
    ? "unavailable"
    : `${formatFixedNumber(((2 ** growth.heapsBeta) - 1) * 100, 1)}%`;
  return createOverviewFigure({
    id: "overview-vocabulary-growth",
    title: "Vocabulary growth",
    subtitle: "Heaps-law shape from stable source-order checkpoints.",
    captionStats: `k ${formatOptionalFixed(growth.heapsK, 3)} · beta ${formatOptionalFixed(growth.heapsBeta, 3)} · R-squared ${formatOptionalFixed(growth.rSquared, 3)}`,
    captionReading: `Each doubling of tokens yields ~${betaYield} more types; stable source order, not a ceiling.`,
    renderSvg: () => renderVocabularyGrowthSvg(
      "overview-vocabulary-growth",
      buildVocabularyGrowthGeometry({ points: growth.points }),
    ),
  });
}

function renderLexicalSpectrumFigure(corpus: AnalysisCorpus): HTMLElement {
  const spectrum = corpus.lexicalSpectrum;
  const truncation = spectrum.frequencySpectrumTruncated
    ? `showing ${formatNumber(spectrum.frequencySpectrum.length)} of ${formatNumber(spectrum.frequencySpectrumTotal)} observed frequencies`
    : `${formatNumber(spectrum.frequencySpectrumTotal)} observed frequencies`;
  return createOverviewFigure({
    id: "overview-lexical-spectrum",
    title: "Lexical spectrum",
    subtitle: "Exact frequency classes on log-log axes.",
    captionStats: `hapax ${formatNumber(spectrum.hapaxCount)} · dis ${formatNumber(spectrum.disLegomenaCount)} · ${truncation}`,
    captionReading: "Each point is V(f) for one exact integer frequency; bars are avoided because log-scale bar lengths would imply intervals that were not observed.",
    renderSvg: () => renderLexicalSpectrumSvg(
      "overview-lexical-spectrum",
      buildLexicalSpectrumGeometry({
        points: spectrum.frequencySpectrum,
        hapaxCount: spectrum.hapaxCount,
        disLegomenaCount: spectrum.disLegomenaCount,
      }),
    ),
  });
}

function renderCoverageFigure(corpus: AnalysisCorpus): HTMLElement {
  const concentration = corpus.frequencyConcentration;
  const half = concentration.cumulativeCoverage.find((point) => point.share >= 0.5) ?? null;
  const halfPhrase = half === null
    ? "half-corpus coverage is unavailable"
    : `half the corpus is written with just ${formatNumber(half.k)} of its ${formatNumber(corpus.totals.types)} types`;
  const tenTypes = concentration.top10Coverage === null
    ? "Ten-type coverage is unavailable"
    : `Ten types carry ${formatPercent(concentration.top10Coverage)} of every token`;
  return createOverviewFigure({
    id: "overview-cumulative-coverage",
    title: "Cumulative coverage",
    subtitle: "Token mass carried by the top-ranked vocabulary.",
    captionStats: `top 10 ${formatOptionalPercent(concentration.top10Coverage)} · top 100 ${formatOptionalPercent(concentration.top100Coverage)} · top 1,000 ${formatOptionalPercent(concentration.top1000Coverage)} · top 10,000 ${formatOptionalPercent(concentration.top10000Coverage)}`,
    captionReading: `${tenTypes}; ${halfPhrase}.`,
    renderSvg: () => renderCoverageSvg(
      "overview-cumulative-coverage",
      buildCoverageGeometry({
        points: concentration.cumulativeCoverage,
        milestones: [
          { k: 10, share: concentration.top10Coverage },
          { k: 100, share: concentration.top100Coverage },
          { k: 1_000, share: concentration.top1000Coverage },
          { k: 10_000, share: concentration.top10000Coverage },
        ],
      }),
    ),
  });
}

function renderDispersionFigure(corpus: AnalysisCorpus): HTMLElement {
  const dispersion = corpus.dispersion;
  return createOverviewFigure({
    id: "overview-dispersion-profile",
    title: "Dispersion profile",
    subtitle: "Document-frequency buckets as log-scale lollipops.",
    captionStats: `>=90% count ${formatNumber(dispersion.dfAtLeast90pctCount)} · exactly-one count ${formatNumber(dispersion.dfIn1Count)} (${formatOptionalPercent(dispersion.dfIn1Share)})`,
    captionReading: `Almost everything is local: ${formatOptionalPercent(dispersion.dfIn1Share)} of the vocabulary never leaves its home document.`,
    renderSvg: () => renderDispersionSvg(
      "overview-dispersion-profile",
      buildDispersionGeometry({
        buckets: [
          {
            key: "df-1",
            label: "1 doc",
            count: dispersion.dfIn1Count,
            share: dispersion.dfIn1Share,
          },
          {
            key: "df-2-9",
            label: "2-9 docs",
            count: dispersion.dfIn2To9Count,
            share: dispersion.dfIn2To9Share,
          },
          {
            key: "df-10-99",
            label: "10-99 docs",
            count: dispersion.dfIn10To99Count,
            share: dispersion.dfIn10To99Share,
          },
          {
            key: "df-100-plus",
            label: "100+ docs",
            count: dispersion.dfIn100PlusCount,
            share: dispersion.dfIn100PlusShare,
          },
          {
            key: "df-90pct",
            label: ">=90%",
            count: dispersion.dfAtLeast90pctCount,
            share: dispersion.dfAtLeast90pctShare,
            highlighted: true,
            detail: universalTypeLabel(dispersion),
          },
        ],
      }),
    ),
  });
}

function universalTypeLabel(dispersion: AnalysisCorpus["dispersion"]): string {
  const tokens = dispersion.universalTypes.map((type) => type.token);
  const remaining = Math.max(0, dispersion.dfAtLeast90pctCount - tokens.length);
  const tokenText = tokens.length === 0
    ? ""
    : ` - ${tokens.join(" · ")}${remaining > 0 ? ` · +${formatNumber(remaining)} more` : ""}`;
  return `${formatNumber(dispersion.dfAtLeast90pctCount)}${tokenText}`;
}

function renderLexicalSpectrum(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Lexical spectrum");
  const spectrum = corpus.lexicalSpectrum;
  panel.appendChild(createMetricTable([
    { label: "Types", value: formatNumber(spectrum.types), glossaryId: "type" },
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
      glossaryId: "mattr",
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
  appendPanelReading(panel, spectrum.reading);
  return panel;
}

function renderFrequencyConcentration(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Frequency concentration");
  const concentration = corpus.frequencyConcentration;
  const rows: OverviewMetricRow[] = [];
  if (concentration.top100Coverage !== null) {
    rows.push({
      label: "Top 100 types cover",
      value: formatPercent(concentration.top100Coverage),
      glossaryId: "topNCoverage",
    });
  }
  if (concentration.top1000Coverage !== null) {
    rows.push({
      label: "Top 1,000 types cover",
      value: formatPercent(concentration.top1000Coverage),
      glossaryId: "topNCoverage",
    });
  }
  if (concentration.top10000Coverage !== null) {
    rows.push({
      label: "Top 10,000 types cover",
      value: formatPercent(concentration.top10000Coverage),
      glossaryId: "topNCoverage",
    });
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
      glossaryId: "shannonEntropy",
    });
  }
  if (concentration.effectiveVocabularyEntropy !== null) {
    rows.push({
      label: "Effective vocabulary (perplexity)",
      value: `≈ ${formatNumber(concentration.effectiveVocabularyEntropy)} types`,
      glossaryId: "effectiveVocabulary",
    });
  }
  if (rows.length > 0) {
    panel.appendChild(createMetricTable(rows));
  }
  panel.appendChild(renderCoverageFigure(corpus));
  appendPanelReading(panel, concentration.reading);
  return panel;
}

function renderRankFrequency(corpus: AnalysisCorpus): HTMLElement {
  const panel = createOverviewPanel("Rank–frequency fit");
  const rank = corpus.rankFrequency;
  const rows: OverviewMetricRow[] = [
    { label: "Sampled rank points", value: formatNumber(rank.points.length), glossaryId: "zipfFit" },
  ];
  if (rank.zipfExponent !== null) {
    rows.push({
      label: "Zipf exponent",
      value: formatFixedNumber(rank.zipfExponent, 3),
      glossaryId: "zipfFit",
    });
  }
  if (rank.zipfCoefficient !== null) {
    rows.push({
      label: "Zipf coefficient (raw-frequency scale)",
      value: formatFixedNumber(rank.zipfCoefficient, 2),
      glossaryId: "zipfFit",
    });
  }
  if (rank.rSquared !== null) {
    rows.push({ label: "Fit R²", value: formatFixedNumber(rank.rSquared, 3), glossaryId: "zipfFit" });
  }
  if (rank.fittedRankStart !== null && rank.fittedRankEnd !== null) {
    rows.push({
      label: "Fitted ranks",
      value: `${formatNumber(rank.fittedRankStart)}–${formatNumber(rank.fittedRankEnd)}`,
      glossaryId: "zipfFit",
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
    { label: "Vocabulary-growth checkpoints", value: formatNumber(growth.points.length), glossaryId: "heapsFit" },
  ];
  if (finalPoint) {
    rows.push({
      label: "Final observed point",
      value: `${formatNumber(finalPoint.types)} types / ${formatNumber(finalPoint.tokens)} tokens`,
      glossaryId: "heapsFit",
    });
  }
  if (growth.heapsK !== null) {
    rows.push({ label: "Heaps K", value: formatFixedNumber(growth.heapsK, 3), glossaryId: "heapsFit" });
  }
  if (growth.heapsBeta !== null) {
    rows.push({ label: "Heaps β", value: formatFixedNumber(growth.heapsBeta, 3), glossaryId: "heapsFit" });
  }
  if (growth.rSquared !== null) {
    rows.push({ label: "Fit R²", value: formatFixedNumber(growth.rSquared, 3), glossaryId: "heapsFit" });
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

  if (corpus.totals.types > 0) {
    const documentUnit = corpus.totals.documents === 1 ? "document" : "documents";
    const frequencyRows: OverviewMetricRow[] = [
      {
        ...countWithInlineShareRow("Types in 1 document only", dispersion.dfIn1Count, dispersion.dfIn1Share),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfIn1SampleTypes,
        sampleTypeCount: dispersion.dfIn1Count,
      },
      {
        ...countWithInlineShareRow("Types in 2–9 documents", dispersion.dfIn2To9Count, dispersion.dfIn2To9Share),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfIn2To9SampleTypes,
        sampleTypeCount: dispersion.dfIn2To9Count,
      },
      {
        ...countWithInlineShareRow("Types in 10–99 documents", dispersion.dfIn10To99Count, dispersion.dfIn10To99Share),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfIn10To99SampleTypes,
        sampleTypeCount: dispersion.dfIn10To99Count,
      },
      {
        ...countWithInlineShareRow("Types in 100+ documents", dispersion.dfIn100PlusCount, dispersion.dfIn100PlusShare),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfIn100PlusSampleTypes,
        sampleTypeCount: dispersion.dfIn100PlusCount,
      },
      {
        ...countWithInlineShareRow(
          "Types in at least 1% of documents",
          dispersion.dfAtLeast1pctCount,
          dispersion.dfAtLeast1pctShare,
        ),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfAtLeast1pctSampleTypes,
        sampleTypeCount: dispersion.dfAtLeast1pctCount,
      },
      {
        ...countWithInlineShareRow(
          "Types in at least 10% of documents",
          dispersion.dfAtLeast10pctCount,
          dispersion.dfAtLeast10pctShare,
        ),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfAtLeast10pctSampleTypes,
        sampleTypeCount: dispersion.dfAtLeast10pctCount,
      },
      {
        ...countWithInlineShareRow(
          "Types in at least 50% of documents",
          dispersion.dfAtLeast50pctCount,
          dispersion.dfAtLeast50pctShare,
        ),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfAtLeast50pctSampleTypes,
        sampleTypeCount: dispersion.dfAtLeast50pctCount,
      },
      {
        ...countWithInlineShareRow(
          "Types in at least 90% of documents",
          dispersion.dfAtLeast90pctCount,
          dispersion.dfAtLeast90pctShare,
        ),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfAtLeast90pctSampleTypes,
        sampleTypeCount: dispersion.dfAtLeast90pctCount,
      },
      {
        ...countWithInlineShareRow(
          `Types in all ${formatNumber(corpus.totals.documents)} ${documentUnit}`,
          dispersion.dfInAllCount,
          dispersion.dfInAllShare,
        ),
        glossaryId: "documentFrequencySpectrum",
        sampleTypes: dispersion.dfInAllSampleTypes,
        sampleTypeCount: dispersion.dfInAllCount,
      },
    ];
    if (dispersion.medianDocumentsPerType !== null) {
      frequencyRows.push({
        label: "Median documents per type",
        value: formatNumber(dispersion.medianDocumentsPerType, 1),
        glossaryId: "documentFrequencySpectrum",
      });
    }
    panel.appendChild(createMetricTable(frequencyRows));
  }

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
