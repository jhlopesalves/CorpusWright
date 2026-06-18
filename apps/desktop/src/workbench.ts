import {
  buildAnalysisCorpus,
  loadAnalysisInputTexts,
  type AnalysisCorpus,
  type AnalysisDocument,
  type AnalysisInputSource,
} from "./analysis";
import { dom } from "./dom";
import { sourceForPath, sourceHasExtractedText } from "./sources";
import { state, type WorkbenchId } from "./state";

const FUTURE_ANALYSIS_TOOLS = [
  "KWIC / Concordance",
  "N-grams",
  "Exports",
];

const FREQUENCY_DISPLAY_LIMIT = 1_000;
const FREQUENCY_TABLE_COLUMN_COUNT = 5;
const FREQUENT_TERM_THRESHOLD = 10;
const HIGH_DISPERSION_THRESHOLD = 0.5;
const LARGE_DOCUMENT_SHARE_THRESHOLD = 0.25;
const LOW_EFFECTIVE_DOCUMENT_SHARE_THRESHOLD = 0.6;
const HIGH_SINGLE_CHARACTER_TYPE_SHARE_THRESHOLD = 0.1;
const HIGH_NUMERIC_ONLY_TYPE_SHARE_THRESHOLD = 0.1;
const OVERVIEW_EXPLANATIONS_STORAGE_KEY = "corpuswright.analysis.showExplanations";
const TOKEN_LETTER_PATTERN = /\p{L}/u;
const TOKEN_NUMBER_PATTERN = /\p{N}/u;
const ENGLISH_STOPWORDS = new Set(
  `
  a about above after again against all am an and any are as at be because been before
  being below between both but by can did do does doing down during each few for from
  further had has have having he her here hers herself him himself his how i if in into
  is it its itself just me more most my myself no nor not now of off on once only or
  other our ours ourselves out over own same she should so some such than that the their
  theirs them themselves then there these they this those through to too under until up
  very was we were what when where which while who whom why will with you your yours
  yourself yourselves
  `.trim().split(/\s+/),
);

type AnalysisToolId = "overview" | "frequency";
type FrequencySortKey = "token" | "frequency" | "range" | "rate" | "dispersion";
type FrequencyMatchMode = "contains" | "exact" | "startsWith" | "regex";
type FrequencyTokenClassFilter = "alphabetic" | "includeSingles" | "includeNumerals" | "all";
type SortDirection = "asc" | "desc";

interface ScopedAnalysisDocument {
  document: AnalysisDocument;
  key: string;
  index: number;
}

interface ScopedFrequencyTerm {
  token: string;
  frequency: number;
  documentFrequency: number;
  ratePer10k: number;
  dispersion: number;
  documentCounts: Map<string, number>;
}

interface MutableScopedTerm {
  frequency: number;
  documentFrequency: number;
  documentCounts: Map<string, number>;
}

interface FrequencySearchMatcher {
  matches: (token: string) => boolean;
  error: string | null;
}

interface CorpusBalanceSummary {
  sortedDocuments: AnalysisDocument[];
  largestDocument: AnalysisDocument | null;
  smallestNonEmptyDocument: AnalysisDocument | null;
  minTokens: number | null;
  maxTokens: number | null;
  medianTokens: number | null;
  q1Tokens: number | null;
  q3Tokens: number | null;
  iqrTokens: number | null;
  largestDocumentShare: number | null;
  topDocumentCount: number;
  topDocumentShare: number | null;
  lengthRatio: number | null;
  effectiveDocumentCount: number | null;
}

interface LexicalNoiseSummary {
  uniqueTypes: number;
  hapaxLegomena: number;
  disLegomena: number;
  repeatedTypes: number;
  numericOnlyTypes: number;
  numericOnlyShare: number | null;
  singleCharacterTypes: number;
  singleCharacterShare: number | null;
  qualityOtherTypes: number;
  typeTokenRatio: number | null;
}

interface DispersionSummary {
  documentCount: number;
  allDocumentTypes: number;
  oneDocumentTypes: number;
  frequentTypes: number;
  medianFrequentDispersion: number | null;
  highDispersionFrequentTypes: number;
}

interface OverviewStat {
  label: string;
  value: string;
  detail?: string;
  title?: string;
}

interface CompositionSegment {
  label: string;
  count: number;
  className: string;
}

interface ReadingFinding {
  label: string;
  text: string;
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
let frequencyDocumentScope: Set<string> | null = null;
let frequencyScopeDraftKeys = new Set<string>();
let expandedFrequencyTokens = new Set<string>();
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
  return state.projectSources.filter((source) =>
    source.status === "processed" || source.status === "processed_with_warnings"
  ).length;
}

function extractedTextSourceCount(): number {
  return state.projectSources.filter(sourceHasExtractedText).length;
}

function warningSourceCount(): number {
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

function joinLabels(labels: string[]): string {
  if (labels.length <= 1) {
    return labels[0] ?? "";
  }
  if (labels.length === 2) {
    return `${labels[0]} and ${labels[1]}`;
  }
  return `${labels.slice(0, -1).join(", ")}, and ${labels[labels.length - 1]}`;
}

function inputTextSourceLabel(inputTextSource: AnalysisInputSource): string {
  return inputTextSource === "processed" ? "processed text" : "extracted text";
}

function inputTextSourceDisplayLabel(inputTextSource: AnalysisInputSource): string {
  return inputTextSource === "processed" ? "Processed text" : "Extracted text";
}

function appendSummaryRow(labelText: string, valueText: string): void {
  const label = document.createElement("dt");
  label.textContent = labelText;

  const value = document.createElement("dd");
  value.textContent = valueText;

  dom.analyseProjectSummary.append(label, value);
}

function prepareStageStatus(): string {
  const sourceCount = state.projectSources.length;
  const extractedCount = extractedTextSourceCount();
  const warnings = warningSourceCount();

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

  dom.workbenchPrepare.classList.toggle("active", !analyseIsActive);
  dom.workbenchAnalyse.classList.toggle("active", analyseIsActive);
  dom.workbenchExport.classList.remove("active");

  dom.workbenchPrepare.setAttribute("aria-selected", analyseIsActive ? "false" : "true");
  dom.workbenchAnalyse.setAttribute("aria-selected", analyseIsActive ? "true" : "false");
  dom.workbenchExport.setAttribute("aria-selected", "false");

  dom.workbenchPrepare.dataset.stageState = state.projectSources.length > 0 ? "ready" : "blocked";
  dom.workbenchAnalyse.dataset.stageState = status === "ready" ? "ready" : status === "stale" ? "stale" : "blocked";
  dom.workbenchExport.dataset.stageState = "future";

  dom.stagePrepareStatus.textContent = prepareStageStatus();
  dom.stageAnalyseStatus.textContent = analyseStageStatus();
  dom.stageExportStatus.textContent = "Coming later";
}

function renderAnalyseStatus(): void {
  const status = analysisStatus();
  const corpus = state.analysisCorpus;
  const processedCount = processedSourceCount();

  dom.analyseStatusChip.dataset.status = status;
  dom.analyseBuildButton.textContent = corpus ? "Rebuild Analysis Corpus" : "Build Analysis Corpus";

  if (status === "ready" && corpus) {
    dom.analyseStatusChip.textContent =
      `Built ${new Date(corpus.builtAt).toLocaleString()} from ${inputTextSourceLabel(corpus.inputTextSource)}`;
    dom.analyseStatusDetail.textContent =
      `${pluralise(corpus.documents.length, "document")} - ${formatNumber(corpus.totalTokens)} tokens`;
    dom.analyseBuildNote.textContent = "Rebuild updates the in-memory counts from current project text.";
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
  dom.analyseBuildNote.textContent = "Builds an in-memory analysis corpus from loaded project text.";
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
      "Exploratory - built from extracted text; processed text not available yet.";
    dom.analyseInputBanner.classList.add("warning");
    dom.analyseInputBanner.classList.remove("hidden");
  }
}

function renderProjectSummary(): void {
  const sourceCount = state.projectSources.length;
  const fileCount = state.allFiles.length;
  const processedCount = processedSourceCount();
  const extractedCount = extractedTextSourceCount();

  dom.analyseProjectSummary.replaceChildren();
  appendSummaryRow("Project", state.activeProject?.name ?? "No project open");
  appendSummaryRow("Sources", pluralise(sourceCount, "source"));
  appendSummaryRow("Files loaded", pluralise(fileCount, "file"));
  appendSummaryRow("Text available", `${extractedCount.toLocaleString()} of ${sourceCount.toLocaleString()} sources`);
  appendSummaryRow("Processed sources", pluralise(processedCount, "source"));
  if (state.cleanedTokenCountText) {
    appendSummaryRow("Cleaned token count", state.cleanedTokenCountText);
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

function createStatStrip(stats: OverviewStat[], className = ""): HTMLElement {
  const strip = document.createElement("dl");
  strip.className = ["analysis-stat-strip", className].filter(Boolean).join(" ");

  for (const stat of stats) {
    const item = document.createElement("div");
    item.className = "analysis-stat";
    if (stat.title) {
      item.title = stat.title;
    }

    const label = document.createElement("dt");
    label.textContent = stat.label;

    const value = document.createElement("dd");
    value.className = "analysis-stat-value";
    value.textContent = stat.value;

    item.append(label, value);

    if (stat.detail) {
      const detail = document.createElement("dd");
      detail.className = "analysis-stat-detail";
      detail.textContent = stat.detail;
      item.appendChild(detail);
    }

    strip.appendChild(item);
  }

  return strip;
}

function createOverviewAction(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "secondary-btn analysis-action-button";
  button.textContent = label;
  button.addEventListener("click", onClick);
  return button;
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

function sortedNumbers(values: number[]): number[] {
  return [...values].sort((a, b) => a - b);
}

function percentile(sortedValues: number[], percentileValue: number): number | null {
  if (sortedValues.length === 0) {
    return null;
  }
  const position = (sortedValues.length - 1) * percentileValue;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) {
    return sortedValues[lower];
  }
  const weight = position - lower;
  return sortedValues[lower] * (1 - weight) + sortedValues[upper] * weight;
}

function median(values: number[]): number | null {
  return percentile(sortedNumbers(values), 0.5);
}

function summariseCorpusBalance(corpus: AnalysisCorpus): CorpusBalanceSummary {
  const sortedDocuments = [...corpus.documents].sort((a, b) =>
    b.tokenCount - a.tokenCount || documentName(a).localeCompare(documentName(b))
  );
  const nonEmptyDocuments = sortedDocuments.filter((document) => document.tokenCount > 0);
  const largestDocument = nonEmptyDocuments[0] ?? null;
  const smallestNonEmptyDocument = nonEmptyDocuments.length > 0
    ? nonEmptyDocuments[nonEmptyDocuments.length - 1]
    : null;
  const tokenCounts = corpus.documents.map((document) => document.tokenCount);
  const sortedTokenCounts = sortedNumbers(tokenCounts);
  const minTokens = sortedTokenCounts[0] ?? null;
  const maxTokens = sortedTokenCounts[sortedTokenCounts.length - 1] ?? null;
  const medianTokens = percentile(sortedTokenCounts, 0.5);
  const q1Tokens = percentile(sortedTokenCounts, 0.25);
  const q3Tokens = percentile(sortedTokenCounts, 0.75);
  const iqrTokens = q1Tokens === null || q3Tokens === null ? null : q3Tokens - q1Tokens;
  const largestDocumentShare = largestDocument && corpus.totalTokens > 0
    ? largestDocument.tokenCount / corpus.totalTokens
    : null;
  const topDocumentCount = Math.min(3, sortedDocuments.length);
  const topDocumentShare = corpus.totalTokens > 0 && topDocumentCount > 0
    ? sortedDocuments.slice(0, topDocumentCount).reduce((sum, document) => sum + document.tokenCount, 0) / corpus.totalTokens
    : null;
  const lengthRatio = largestDocument && smallestNonEmptyDocument && smallestNonEmptyDocument.tokenCount > 0
    ? largestDocument.tokenCount / smallestNonEmptyDocument.tokenCount
    : null;
  const hhi = corpus.totalTokens > 0
    ? corpus.documents.reduce((sum, document) => {
      const share = document.tokenCount / corpus.totalTokens;
      return sum + share * share;
    }, 0)
    : 0;
  const effectiveDocumentCount = hhi > 0 ? 1 / hhi : null;

  return {
    sortedDocuments,
    largestDocument,
    smallestNonEmptyDocument,
    minTokens,
    maxTokens,
    medianTokens,
    q1Tokens,
    q3Tokens,
    iqrTokens,
    largestDocumentShare,
    topDocumentCount,
    topDocumentShare,
    lengthRatio,
    effectiveDocumentCount,
  };
}

function isSingleCharacterType(token: string): boolean {
  return Array.from(token).length <= 1;
}

function isNumericOnlyType(token: string): boolean {
  return TOKEN_NUMBER_PATTERN.test(token) && !TOKEN_LETTER_PATTERN.test(token);
}

function summariseLexicalNoise(corpus: AnalysisCorpus): LexicalNoiseSummary {
  let hapaxLegomena = 0;
  let disLegomena = 0;
  let numericOnlyTypes = 0;
  let singleCharacterTypes = 0;
  let qualityOtherTypes = 0;

  for (const term of corpus.terms) {
    if (term.frequency === 1) {
      hapaxLegomena += 1;
    } else if (term.frequency === 2) {
      disLegomena += 1;
    }
    const singleCharacter = isSingleCharacterType(term.token);
    const numericOnly = isNumericOnlyType(term.token);
    if (singleCharacter) {
      singleCharacterTypes += 1;
    } else if (numericOnly) {
      numericOnlyTypes += 1;
    } else {
      qualityOtherTypes += 1;
    }
  }

  const typeShare = (count: number): number | null =>
    corpus.totalTypes > 0 ? count / corpus.totalTypes : null;
  const repeatedTypes = Math.max(0, corpus.totalTypes - hapaxLegomena - disLegomena);

  return {
    uniqueTypes: corpus.totalTypes,
    hapaxLegomena,
    disLegomena,
    repeatedTypes,
    numericOnlyTypes,
    numericOnlyShare: typeShare(numericOnlyTypes),
    singleCharacterTypes,
    singleCharacterShare: typeShare(singleCharacterTypes),
    qualityOtherTypes,
    typeTokenRatio: corpus.totalTokens > 0 ? corpus.totalTypes / corpus.totalTokens : null,
  };
}

function summariseDispersion(corpus: AnalysisCorpus): DispersionSummary {
  const scoped = buildScopedFrequencyTerms(allFrequencyDocuments(corpus));
  const frequentTerms = scoped.terms.filter((term) => term.frequency >= FREQUENT_TERM_THRESHOLD);
  const frequentDispersions = corpus.documents.length >= 2
    ? frequentTerms.map((term) => term.dispersion)
    : [];

  return {
    documentCount: corpus.documents.length,
    allDocumentTypes: corpus.documents.length > 0
      ? corpus.terms.filter((term) => term.documentFrequency === corpus.documents.length).length
      : 0,
    oneDocumentTypes: corpus.terms.filter((term) => term.documentFrequency === 1).length,
    frequentTypes: frequentTerms.length,
    medianFrequentDispersion: median(frequentDispersions),
    highDispersionFrequentTypes: corpus.documents.length >= 2
      ? frequentTerms.filter((term) => term.dispersion >= HIGH_DISPERSION_THRESHOLD).length
      : 0,
  };
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
  if (corpus.documents.length < 2 || corpus.totalTokens <= 0) {
    return false;
  }
  return (
    (balance.largestDocumentShare !== null &&
      balance.largestDocumentShare >= LARGE_DOCUMENT_SHARE_THRESHOLD) ||
    (balance.topDocumentShare !== null && balance.topDocumentShare >= 0.5) ||
    (balance.effectiveDocumentCount !== null &&
      balance.effectiveDocumentCount <= LOW_EFFECTIVE_DOCUMENT_SHARE_THRESHOLD * corpus.documents.length)
  );
}

function buildCorpusVerdict(
  corpus: AnalysisCorpus,
  balance: CorpusBalanceSummary,
  lexical: LexicalNoiseSummary,
  dispersion: DispersionSummary,
): string {
  if (corpus.documents.length === 0) {
    return "No analysis documents are available yet.";
  }
  if (corpus.totalTokens === 0) {
    return "Analysis data is present, but no tokens were counted.";
  }

  const cautions: string[] = [];
  if (corpus.inputTextSource === "extracted") {
    cautions.push("extracted text");
  }
  if (balanceNeedsCare(corpus, balance)) {
    cautions.push("document balance");
  }
  if (extractionQualityNeedsCare(lexical)) {
    cautions.push("extraction quality");
  }
  if (
    dispersion.frequentTypes > 0 &&
    dispersion.highDispersionFrequentTypes / dispersion.frequentTypes >= 0.35
  ) {
    cautions.push("term concentration");
  }

  if (cautions.length > 0) {
    return `Usable for exploration, with cautions about ${joinLabels(cautions)}.`;
  }

  return "Usable for exploratory reading; no headline caution stands out in these counts.";
}

function buildBalanceReading(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): ReadingFinding {
  if (corpus.documents.length === 0) {
    return {
      label: "Balance",
      text: "No documents are available for document-level balance checks.",
    };
  }
  if (corpus.totalTokens === 0) {
    return {
      label: "Balance",
      text: "Documents are present, but all token counts are zero.",
    };
  }
  if (corpus.documents.length === 1) {
    return {
      label: "Balance",
      text: `One document supplies all ${formatNumber(corpus.totalTokens)} tokens, so document imbalance cannot be assessed.`,
    };
  }

  const largestShare = balance.largestDocumentShare === null
    ? "n/a"
    : formatPercent(balance.largestDocumentShare);
  const topShare = balance.topDocumentShare === null
    ? "n/a"
    : formatPercent(balance.topDocumentShare);
  const core = `the largest document is ${largestShare} of tokens and the top ${formatNumber(balance.topDocumentCount)} are ${topShare}`;

  if (balanceNeedsCare(corpus, balance)) {
    return {
      label: "Balance",
      text: `${core}. Treat raw aggregate frequencies with care.`,
    };
  }

  return {
    label: "Balance",
    text: `${core}. Document-level checks are still useful for close reading.`,
  };
}

function buildExtractionQualityReading(lexical: LexicalNoiseSummary): ReadingFinding {
  if (lexical.uniqueTypes === 0) {
    return {
      label: "Extraction quality",
      text: "No token types were counted, so extraction-quality indicators are unavailable.",
    };
  }

  const numericText = formatCountWithShare(lexical.numericOnlyTypes, lexical.uniqueTypes);
  const singleCharacterText = formatCountWithShare(lexical.singleCharacterTypes, lexical.uniqueTypes);
  if (extractionQualityNeedsCare(lexical)) {
    return {
      label: "Extraction quality",
      text: `${numericText} are numeric-only and ${singleCharacterText} are single-character types; extracted text may include table, page, reference, or OCR fragments.`,
    };
  }

  return {
    label: "Extraction quality",
    text: `${numericText} are numeric-only and ${singleCharacterText} are single-character types; no high extraction-noise signal stands out here.`,
  };
}

function buildLexicalRichnessSummary(lexical: LexicalNoiseSummary): ReadingFinding {
  if (lexical.uniqueTypes === 0) {
    return {
      label: "Lexical richness",
      text: "No token types were counted.",
    };
  }

  return {
    label: "Lexical richness",
    text: `${formatCountWithShare(lexical.hapaxLegomena, lexical.uniqueTypes)} are hapax and ${formatCountWithShare(lexical.repeatedTypes, lexical.uniqueTypes)} occur three or more times. Raw TTR is length-sensitive.`,
  };
}

function buildDispersionReading(corpus: AnalysisCorpus, dispersion: DispersionSummary): ReadingFinding {
  if (corpus.documents.length < 2) {
    return {
      label: "Dispersion",
      text: "Dispersion needs at least two documents.",
    };
  }
  if (dispersion.frequentTypes === 0) {
    return {
      label: "Dispersion",
      text: `No terms have frequency at least ${formatNumber(FREQUENT_TERM_THRESHOLD)}, so frequent-term concentration is not summarised.`,
    };
  }

  const highDpText = formatCountWithShare(
    dispersion.highDispersionFrequentTypes,
    dispersion.frequentTypes,
  );
  if (dispersion.highDispersionFrequentTypes > 0) {
    return {
      label: "Dispersion",
      text: `${highDpText} frequent types have DP at least ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)}. Inspect concentrated terms before treating them as corpus-wide vocabulary.`,
    };
  }

  return {
    label: "Dispersion",
    text: `No frequent types have DP at least ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)} in this overview.`,
  };
}

function buildCorpusReading(
  corpus: AnalysisCorpus,
  balance: CorpusBalanceSummary,
  lexical: LexicalNoiseSummary,
  dispersion: DispersionSummary,
): CorpusReading {
  const actions: HTMLElement[] = [];
  if (extractionQualityNeedsCare(lexical)) {
    actions.push(createOverviewAction("Clean in Prepare", () => setActiveWorkbench("prepare")));
  }
  if (dispersion.frequentTypes > 0) {
    actions.push(createOverviewAction("Open Frequency List", () => setActiveAnalysisTool("frequency")));
  }

  return {
    verdict: buildCorpusVerdict(corpus, balance, lexical, dispersion),
    findings: [
      buildBalanceReading(corpus, balance),
      buildExtractionQualityReading(lexical),
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

    const label = document.createElement("strong");
    label.textContent = `${finding.label}: `;

    const text = document.createElement("span");
    text.textContent = finding.text;

    item.append(label, text);
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
  panel.appendChild(createStatStrip([
    { label: "Documents", value: formatNumber(corpus.documents.length) },
    { label: "Tokens", value: formatNumber(corpus.totalTokens) },
    { label: "Unique types", value: formatNumber(corpus.totalTypes) },
    { label: "Sources", value: formatNumber(corpus.sourceCount) },
    { label: "Input", value: inputTextSourceDisplayLabel(corpus.inputTextSource) },
  ], "analysis-snapshot-strip"));
  return panel;
}

function renderDocumentLengthDistribution(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): HTMLElement {
  const distribution = document.createElement("div");
  distribution.className = "document-length-distribution";

  const title = document.createElement("h4");
  title.textContent = "Document length distribution";
  distribution.appendChild(title);

  if (corpus.documents.length === 0) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = "No documents in the analysis corpus.";
    distribution.appendChild(empty);
    return distribution;
  }

  const maxTokens = balance.maxTokens ?? 0;
  for (const documentRecord of balance.sortedDocuments) {
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

  return distribution;
}

function renderCorpusBalance(corpus: AnalysisCorpus, balance: CorpusBalanceSummary): HTMLElement {
  const panel = createOverviewPanel("Corpus balance");
  panel.appendChild(renderDocumentLengthDistribution(corpus, balance));

  panel.appendChild(createStatStrip([
    {
      label: "Largest document",
      value: balance.largestDocument ? formatNumber(balance.largestDocument.tokenCount) : "None",
      detail: balance.largestDocument ? documentName(balance.largestDocument) : "No non-empty documents",
      title: balance.largestDocument ? documentName(balance.largestDocument) : undefined,
    },
    {
      label: "Smallest non-empty",
      value: balance.smallestNonEmptyDocument
        ? formatNumber(balance.smallestNonEmptyDocument.tokenCount)
        : "None",
      detail: balance.smallestNonEmptyDocument
        ? documentName(balance.smallestNonEmptyDocument)
        : "No non-empty documents",
      title: balance.smallestNonEmptyDocument
        ? documentName(balance.smallestNonEmptyDocument)
        : undefined,
    },
    { label: "Median length", value: formatOptionalNumber(balance.medianTokens) },
    {
      label: "IQR length",
      value: formatOptionalNumber(balance.iqrTokens),
      detail: balance.q1Tokens === null || balance.q3Tokens === null
        ? "No quartiles available"
        : `Q1 ${formatNumber(balance.q1Tokens)} / Q3 ${formatNumber(balance.q3Tokens)}`,
    },
    {
      label: "Min / max",
      value: balance.minTokens === null || balance.maxTokens === null
        ? "n/a"
        : `${formatNumber(balance.minTokens)} / ${formatNumber(balance.maxTokens)}`,
    },
    {
      label: "Largest / smallest",
      value: balance.lengthRatio === null ? "n/a" : `${formatNumber(balance.lengthRatio, 1)}x`,
    },
    { label: "Largest share", value: formatOptionalPercent(balance.largestDocumentShare) },
    {
      label: balance.topDocumentCount > 0
        ? `Top ${formatNumber(balance.topDocumentCount)} share`
        : "Top document share",
      value: formatOptionalPercent(balance.topDocumentShare),
    },
    {
      label: "Effective documents",
      value: balance.effectiveDocumentCount === null
        ? "n/a"
        : `${formatNumber(balance.effectiveDocumentCount, 1)} of ${formatNumber(corpus.documents.length)}`,
    },
  ], "analysis-balance-strip"));

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

  panel.appendChild(createStatStrip([
    { label: "Unique types", value: formatNumber(lexical.uniqueTypes) },
    {
      label: "TTR (length-sensitive)",
      value: lexical.typeTokenRatio === null ? "n/a" : formatFixedNumber(lexical.typeTokenRatio, 3),
      detail: "Raw type-token ratio",
    },
  ], "analysis-compact-strip"));

  if (overviewExplanationsVisible) {
    panel.appendChild(createExplanation(
      "Hapax legomena occur once; dis legomena occur twice. Raw TTR falls as corpus size grows, so it is not a fair lexical-diversity comparison across corpora of different lengths.",
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

  panel.appendChild(createStatStrip([
    {
      label: "Numeric-only types",
      value: formatCountWithShare(lexical.numericOnlyTypes, lexical.uniqueTypes),
    },
    {
      label: "Single-character types",
      value: formatCountWithShare(lexical.singleCharacterTypes, lexical.uniqueTypes),
    },
  ], "analysis-compact-strip"));

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

  panel.appendChild(createStatStrip([
    {
      label: "High-DP frequent types",
      value: dispersion.frequentTypes === 0
        ? "n/a"
        : formatCountWithShare(dispersion.highDispersionFrequentTypes, dispersion.frequentTypes),
      detail: `DP at least ${formatFixedNumber(HIGH_DISPERSION_THRESHOLD, 2)}`,
    },
    {
      label: "Types in all documents",
      value: formatCountWithShare(dispersion.allDocumentTypes, corpus.totalTypes),
    },
    {
      label: "Types in one document only",
      value: formatCountWithShare(dispersion.oneDocumentTypes, corpus.totalTypes),
    },
    {
      label: "Frequent types",
      value: formatCountWithShare(dispersion.frequentTypes, corpus.totalTypes),
      detail: `Frequency at least ${formatNumber(FREQUENT_TERM_THRESHOLD)}`,
    },
    {
      label: "Median DP for frequent types",
      value: dispersion.medianFrequentDispersion === null
        ? "n/a"
        : formatFixedNumber(dispersion.medianFrequentDispersion, 2),
    },
  ], "analysis-dispersion-strip"));

  if (dispersion.frequentTypes === 0) {
    const note = document.createElement("p");
    note.className = "analysis-panel-note";
    note.textContent = `No terms have frequency at least ${formatNumber(FREQUENT_TERM_THRESHOLD)}.`;
    panel.appendChild(note);
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

  const balance = summariseCorpusBalance(corpus);
  const lexical = summariseLexicalNoise(corpus);
  const dispersion = summariseDispersion(corpus);

  dom.analyseOverview.append(
    renderCorpusReading(corpus, balance, lexical, dispersion),
    renderCorpusSnapshot(corpus),
    renderCorpusBalance(corpus, balance),
    renderLexicalRichness(lexical),
    renderExtractionQuality(lexical),
    renderDispersionProfile(corpus, dispersion),
  );
}

function resetFrequencyCorpusState(): void {
  frequencyDocumentScope = null;
  frequencyScopeDraftKeys = new Set<string>();
  expandedFrequencyTokens = new Set<string>();
}

function frequencyDocumentKey(document: AnalysisDocument, index: number): string {
  return `${index}\u0000${document.sourcePath}\u0000${document.relativePath}`;
}

function allFrequencyDocuments(corpus: AnalysisCorpus): ScopedAnalysisDocument[] {
  return corpus.documents.map((document, index) => ({
    document,
    index,
    key: frequencyDocumentKey(document, index),
  }));
}

function selectedFrequencyDocuments(corpus: AnalysisCorpus): ScopedAnalysisDocument[] {
  const documents = allFrequencyDocuments(corpus);
  if (!frequencyDocumentScope) {
    return documents;
  }

  const selected = documents.filter((document) => frequencyDocumentScope?.has(document.key));
  if (selected.length === 0 || selected.length === documents.length) {
    frequencyDocumentScope = null;
    return documents;
  }

  return selected;
}

function currentFrequencyScopeKeys(corpus: AnalysisCorpus): Set<string> {
  return new Set(selectedFrequencyDocuments(corpus).map((document) => document.key));
}

function renderFrequencyScopeControl(corpus: AnalysisCorpus | null): void {
  if (!corpus || corpus.documents.length === 0) {
    dom.frequencyFilesButton.disabled = true;
    dom.frequencyFilesButton.textContent = "Files: 0 of 0";
    dom.frequencyFilesButton.setAttribute("aria-label", "No files available for frequency scoping");
    return;
  }

  const selectedCount = selectedFrequencyDocuments(corpus).length;
  const totalCount = corpus.documents.length;
  dom.frequencyFilesButton.disabled = false;
  dom.frequencyFilesButton.textContent = `Files: ${formatNumber(selectedCount)} of ${formatNumber(totalCount)}`;
  dom.frequencyFilesButton.setAttribute(
    "aria-label",
    `Frequency list file scope: ${formatNumber(selectedCount)} of ${formatNumber(totalCount)} files selected`,
  );
}

function buildScopedFrequencyTerms(scopedDocuments: ScopedAnalysisDocument[]): {
  terms: ScopedFrequencyTerm[];
  totalTokens: number;
} {
  const terms = new Map<string, MutableScopedTerm>();
  const totalTokens = scopedDocuments.reduce((sum, document) => sum + document.document.tokenCount, 0);

  for (const scopedDocument of scopedDocuments) {
    for (const [token, count] of Object.entries(scopedDocument.document.termCounts)) {
      if (count <= 0) {
        continue;
      }
      const term = terms.get(token) ?? {
        frequency: 0,
        documentFrequency: 0,
        documentCounts: new Map<string, number>(),
      };
      term.frequency += count;
      term.documentFrequency += 1;
      term.documentCounts.set(scopedDocument.key, count);
      terms.set(token, term);
    }
  }

  return {
    totalTokens,
    terms: Array.from(terms.entries(), ([token, term]) => {
      const dispersion = totalTokens > 0 && term.frequency > 0
        ? 0.5 * scopedDocuments.reduce((sum, scopedDocument) => {
          const tokenCount = term.documentCounts.get(scopedDocument.key) ?? 0;
          const observedTokenShare = tokenCount / term.frequency;
          const documentSizeShare = scopedDocument.document.tokenCount / totalTokens;
          return sum + Math.abs(observedTokenShare - documentSizeShare);
        }, 0)
        : 0;

      return {
        token,
        frequency: term.frequency,
        documentFrequency: term.documentFrequency,
        ratePer10k: totalTokens > 0 ? (term.frequency / totalTokens) * 10_000 : 0,
        dispersion,
        documentCounts: term.documentCounts,
      };
    }),
  };
}

function createFrequencySearchMatcher(): FrequencySearchMatcher {
  const rawQuery = dom.frequencySearchInput.value.trim();
  if (rawQuery.length === 0) {
    return { matches: () => true, error: null };
  }

  const query = rawQuery.toLowerCase();
  const mode = dom.frequencyMatchModeSelect.value as FrequencyMatchMode;
  switch (mode) {
    case "contains":
      return { matches: (token) => token.includes(query), error: null };
    case "exact":
      return { matches: (token) => token === query, error: null };
    case "startsWith":
      return { matches: (token) => token.startsWith(query), error: null };
    case "regex":
      try {
        const regex = new RegExp(query, "u");
        return { matches: (token) => regex.test(token), error: null };
      } catch {
        return { matches: () => false, error: "Invalid regex pattern." };
      }
  }
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

function tokenMatchesClassFilter(token: string, filter: FrequencyTokenClassFilter): boolean {
  const hasLetter = TOKEN_LETTER_PATTERN.test(token);
  const hasNumber = TOKEN_NUMBER_PATTERN.test(token);
  const isSingleCharacter = Array.from(token).length <= 1;

  switch (filter) {
    case "alphabetic":
      return hasLetter && !hasNumber && !isSingleCharacter;
    case "includeSingles":
      return hasLetter && !hasNumber;
    case "includeNumerals":
      return !isSingleCharacter;
    case "all":
      return true;
  }
}

function frequencyDetailId(token: string): string {
  return `frequency-detail-${encodeURIComponent(token)}`;
}

function pruneExpandedFrequencyTokens(visibleTerms: ScopedFrequencyTerm[]): void {
  const visibleTokens = new Set(visibleTerms.map((term) => term.token));
  for (const token of expandedFrequencyTokens) {
    if (!visibleTokens.has(token)) {
      expandedFrequencyTokens.delete(token);
    }
  }
}

function toggleFrequencyTokenExpansion(token: string): void {
  if (expandedFrequencyTokens.has(token)) {
    expandedFrequencyTokens.delete(token);
  } else {
    expandedFrequencyTokens.add(token);
  }
  renderFrequencyList();
}

function sortTerms(terms: ScopedFrequencyTerm[]): ScopedFrequencyTerm[] {
  const direction = frequencySortDirection === "asc" ? 1 : -1;
  return [...terms].sort((a, b) => {
    let comparison = 0;
    switch (frequencySortKey) {
      case "token":
        comparison = a.token.localeCompare(b.token);
        break;
      case "frequency":
        comparison = a.frequency - b.frequency;
        break;
      case "range":
        comparison = a.documentFrequency - b.documentFrequency;
        break;
      case "rate":
        comparison = a.ratePer10k - b.ratePer10k;
        break;
      case "dispersion":
        comparison = a.dispersion - b.dispersion;
        break;
    }
    return comparison === 0
      ? a.token.localeCompare(b.token)
      : comparison * direction;
  });
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

function frequencySummaryText(
  visibleTypeCount: number,
  scopedTypeCount: number,
  selectedDocumentCount: number,
  totalDocumentCount: number,
  scopedTokenCount: number,
  stopwordsHidden: boolean,
  tokenClassFilter: FrequencyTokenClassFilter,
): string {
  const documentScope = selectedDocumentCount === totalDocumentCount
    ? `${formatNumber(totalDocumentCount)} files`
    : `${formatNumber(selectedDocumentCount)} of ${formatNumber(totalDocumentCount)} files`;
  const parts = [
    `${formatNumber(visibleTypeCount)} of ${formatNumber(scopedTypeCount)} types shown`,
  ];
  if (stopwordsHidden) {
    parts.push("English stopwords hidden");
  }
  parts.push(tokenClassSummary(tokenClassFilter), documentScope, `${formatNumber(scopedTokenCount)} tokens in scope`);
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

function createDispersionCell(term: ScopedFrequencyTerm): HTMLTableCellElement {
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

function createFrequencyDistributionPanel(
  term: ScopedFrequencyTerm,
  scopedDocuments: ScopedAnalysisDocument[],
): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "frequency-distribution-panel";

  const title = document.createElement("h3");
  title.textContent = "Per-file distribution (selected files)";
  panel.appendChild(title);

  const header = document.createElement("div");
  header.className = "frequency-distribution-row frequency-distribution-header";
  header.append(
    Object.assign(document.createElement("span"), { textContent: "Document/File" }),
    Object.assign(document.createElement("span"), { textContent: "Count" }),
    Object.assign(document.createElement("span"), { textContent: "Per 10k" }),
    Object.assign(document.createElement("span"), { textContent: "Bar" }),
  );
  panel.appendChild(header);

  const rows = scopedDocuments.map((scopedDocument) => {
    const count = term.documentCounts.get(scopedDocument.key) ?? 0;
    return {
      document: scopedDocument.document,
      count,
      ratePer10k: scopedDocument.document.tokenCount > 0
        ? (count / scopedDocument.document.tokenCount) * 10_000
        : 0,
    };
  });
  const maxRate = rows.reduce((max, row) => Math.max(max, row.ratePer10k), 0);

  for (const row of rows) {
    const line = document.createElement("div");
    line.className = "frequency-distribution-row";

    const label = document.createElement("span");
    label.className = "frequency-distribution-file";
    label.title = documentName(row.document);
    label.textContent = documentName(row.document);

    const count = document.createElement("span");
    count.className = "numeric";
    count.textContent = formatNumber(row.count);

    const rate = document.createElement("span");
    rate.className = "numeric";
    rate.textContent = formatFixedNumber(row.ratePer10k, 2);

    const track = document.createElement("span");
    track.className = "frequency-distribution-track";
    const fill = document.createElement("span");
    fill.className = "frequency-distribution-fill";
    const barWidth = maxRate > 0 && row.ratePer10k > 0 ? Math.max(3, (row.ratePer10k / maxRate) * 100) : 0;
    fill.style.setProperty("--bar-width", `${barWidth}%`);
    track.appendChild(fill);

    line.append(label, count, rate, track);
    panel.appendChild(line);
  }

  return panel;
}

function renderFrequencyList(): void {
  updateFrequencySortHeaders();
  dom.frequencyTableBody.replaceChildren();
  dom.frequencyLimitNote.textContent = "";
  renderFrequencySearchError(null);

  const corpus = state.analysisCorpus;
  renderFrequencyScopeControl(corpus);
  if (!corpus) {
    dom.frequencyResultCount.textContent = "";
    dom.frequencySummary.textContent = "Build an analysis corpus to populate the frequency list.";
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
    cell.className = "frequency-empty";
    cell.textContent = "Build an analysis corpus to populate the frequency list.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  const scopedDocuments = selectedFrequencyDocuments(corpus);
  const scoped = buildScopedFrequencyTerms(scopedDocuments);
  const matcher = createFrequencySearchMatcher();
  renderFrequencySearchError(matcher.error);
  const minFrequency = Math.max(1, Number.parseInt(dom.frequencyMinInput.value, 10) || 1);
  const hideStopwords = dom.frequencyHideStopwordsInput.checked;
  const tokenClassFilter = currentFrequencyTokenClassFilter();
  const filtered = scoped.terms.filter((term) =>
    term.frequency >= minFrequency &&
    matcher.matches(term.token) &&
    tokenMatchesClassFilter(term.token, tokenClassFilter) &&
    (!hideStopwords || !ENGLISH_STOPWORDS.has(term.token))
  );
  const sorted = sortTerms(filtered);
  const visible = sorted.slice(0, FREQUENCY_DISPLAY_LIMIT);
  pruneExpandedFrequencyTokens(visible);

  dom.frequencySummary.textContent = frequencySummaryText(
    filtered.length,
    scoped.terms.length,
    scopedDocuments.length,
    corpus.documents.length,
    scoped.totalTokens,
    hideStopwords,
    tokenClassFilter,
  );
  dom.frequencyResultCount.textContent = `Built from ${inputTextSourceLabel(corpus.inputTextSource)}`;
  if (sorted.length > visible.length) {
    dom.frequencyLimitNote.textContent =
      `Showing first ${formatNumber(visible.length)} of ${formatNumber(sorted.length)} terms`;
  }

  if (visible.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
    cell.className = "frequency-empty";
    cell.textContent = "No terms match the current filters.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  for (const term of visible) {
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
    documentFrequency.textContent = `${formatNumber(term.documentFrequency)} / ${formatNumber(scopedDocuments.length)}`;

    const rate = document.createElement("td");
    rate.className = "numeric";
    rate.textContent = formatFixedNumber(term.ratePer10k, 2);

    const dispersion = createDispersionCell(term);

    row.append(token, frequency, documentFrequency, rate, dispersion);
    dom.frequencyTableBody.appendChild(row);

    if (isExpanded) {
      const detailRow = document.createElement("tr");
      detailRow.className = "frequency-detail-row";
      const detailCell = document.createElement("td");
      detailCell.id = detailId;
      detailCell.colSpan = FREQUENCY_TABLE_COLUMN_COUNT;
      detailCell.appendChild(createFrequencyDistributionPanel(term, scopedDocuments));
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
  refreshStageNavigator();
}

export function resetWorkbenchForProject(): void {
  analysisNotice = null;
  activeAnalysisTool = "overview";
  resetFrequencyCorpusState();
  dom.workbenchSwitcher.classList.remove("hidden");
  setActiveWorkbench("prepare");
}

export function hideWorkbenchSwitcher(): void {
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
  const indices = state.allFiles.map((_, index) => index);
  const inputTextSource = chooseAnalysisInputSource();
  const buildSignature = analysisBuildSignature(inputTextSource);

  analysisNotice = null;
  dom.analyseBuildButton.disabled = true;
  dom.analyseBuildNote.textContent = "Building analysis corpus...";
  dom.statusBar.textContent = "Building analysis corpus...";

  if (indices.length === 0) {
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    analysisNotice = {
      kind: "warning",
      message: "No text available for analysis. Go to Prepare, extract/process sources, then return to Analyse.",
    };
    dom.statusBar.textContent = "No text available for analysis.";
    dom.analyseBuildButton.disabled = false;
    renderAnalyseWorkbench();
    refreshStageNavigator();
    return;
  }

  try {
    const result = await loadAnalysisInputTexts(
      indices,
      state.currentCorpusVersion,
      inputTextSource === "processed",
      state.activeCleaningConfig,
    );

    if (result.documents.length === 0) {
      state.analysisCorpus = null;
      resetFrequencyCorpusState();
      analysisNotice = {
        kind: "warning",
        message: "No text available for analysis. Go to Prepare, extract/process sources, then return to Analyse.",
      };
      dom.statusBar.textContent = "No text available for analysis.";
      return;
    }

    state.analysisCorpus = buildAnalysisCorpus(result.documents, inputTextSource, buildSignature);
    resetFrequencyCorpusState();
    activeAnalysisTool = "overview";
    const sourceMessage = inputTextSource === "processed"
      ? "Built from processed text."
      : "Exploratory analysis built from extracted text because processed text is not available yet.";
    const skipped = result.skippedDocuments > 0
      ? ` ${pluralise(result.skippedDocuments, "document")} without usable text skipped.`
      : "";
    const skippedPdfs = result.skippedPdfDocuments > 0
      ? ` ${pluralise(result.skippedPdfDocuments, "PDF")} had no materialised text.`
      : "";
    analysisNotice = skipped || skippedPdfs
      ? { kind: "warning", message: `${sourceMessage}${skipped}${skippedPdfs}`.trim() }
      : null;
    dom.statusBar.textContent = `Analysis corpus built from ${inputTextSource} text.`;
  } catch (error) {
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    analysisNotice = {
      kind: "warning",
      message: `Analysis corpus could not be built: ${error}`,
    };
    dom.statusBar.textContent = "Analysis corpus build failed.";
  } finally {
    dom.analyseBuildButton.disabled = false;
    renderAnalyseWorkbench();
    refreshStageNavigator();
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
  renderFrequencyList();
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

function renderFrequencyFilesModal(): void {
  const corpus = state.analysisCorpus;
  dom.frequencyFilesList.replaceChildren();

  if (!corpus) {
    dom.frequencyFilesApplyButton.disabled = true;
    dom.frequencyFilesStatus.textContent = "Build an analysis corpus before choosing files.";
    dom.frequencyFilesStatus.classList.add("is-error");
    return;
  }

  const documents = allFrequencyDocuments(corpus);
  const query = dom.frequencyFilesSearchInput.value.trim().toLowerCase();
  const visibleDocuments = documents.filter((document) =>
    documentName(document.document).toLowerCase().includes(query)
  );

  if (visibleDocuments.length === 0) {
    const empty = document.createElement("p");
    empty.className = "frequency-files-empty";
    empty.textContent = "No files match the current search.";
    dom.frequencyFilesList.appendChild(empty);
  }

  for (const scopedDocument of visibleDocuments) {
    const label = document.createElement("label");
    label.className = "frequency-file-option";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.dataset.documentKey = scopedDocument.key;
    checkbox.checked = frequencyScopeDraftKeys.has(scopedDocument.key);

    const name = document.createElement("span");
    name.className = "frequency-file-name";
    name.title = documentName(scopedDocument.document);
    name.textContent = documentName(scopedDocument.document);

    const tokens = document.createElement("span");
    tokens.className = "frequency-file-tokens";
    tokens.textContent = `${formatNumber(scopedDocument.document.tokenCount)} tokens`;

    label.append(checkbox, name, tokens);
    dom.frequencyFilesList.appendChild(label);
  }

  const selectedCount = frequencyScopeDraftKeys.size;
  dom.frequencyFilesStatus.textContent = selectedCount === 0
    ? "Select at least one file."
    : `${formatNumber(selectedCount)} of ${formatNumber(documents.length)} files selected`;
  dom.frequencyFilesStatus.classList.toggle("is-error", selectedCount === 0);
  dom.frequencyFilesApplyButton.disabled = selectedCount === 0;
}

function openFrequencyFilesModal(): void {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    return;
  }

  frequencyScopeDraftKeys = currentFrequencyScopeKeys(corpus);
  dom.frequencyFilesSearchInput.value = "";
  renderFrequencyFilesModal();
  dom.frequencyFilesModal.classList.remove("hidden");
  dom.frequencyFilesSearchInput.focus();
}

function applyFrequencyFileScope(): void {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    closeFrequencyFilesModal();
    return;
  }

  if (frequencyScopeDraftKeys.size === 0) {
    renderFrequencyFilesModal();
    return;
  }

  frequencyDocumentScope = frequencyScopeDraftKeys.size === corpus.documents.length
    ? null
    : new Set(frequencyScopeDraftKeys);
  expandedFrequencyTokens.clear();
  closeFrequencyFilesModal();
  renderFrequencyList();
}

function handleFrequencyFilesListChange(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLInputElement) || !target.dataset.documentKey) {
    return;
  }

  if (target.checked) {
    frequencyScopeDraftKeys.add(target.dataset.documentKey);
  } else {
    frequencyScopeDraftKeys.delete(target.dataset.documentKey);
  }
  renderFrequencyFilesModal();
}

function selectAllFrequencyFiles(): void {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    return;
  }

  frequencyScopeDraftKeys = new Set(allFrequencyDocuments(corpus).map((document) => document.key));
  renderFrequencyFilesModal();
}

function clearFrequencyFiles(): void {
  frequencyScopeDraftKeys.clear();
  renderFrequencyFilesModal();
}

function handleFrequencyModalKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && !dom.frequencyFilesModal.classList.contains("hidden")) {
    closeFrequencyFilesModal();
  }
}

export function initWorkbenchSwitcher(): void {
  dom.workbenchPrepare.addEventListener("click", () => {
    setActiveWorkbench("prepare");
  });
  dom.workbenchAnalyse.addEventListener("click", () => {
    setActiveWorkbench("analyse");
  });
  dom.workbenchPrepare.addEventListener("keydown", handleWorkbenchKeydown);
  dom.workbenchAnalyse.addEventListener("keydown", handleWorkbenchKeydown);

  dom.analyseBuildButton.addEventListener("click", () => {
    handleBuildAnalysisCorpus();
  });
  dom.analyseToolsList.addEventListener("click", handleAnalysisToolClick);
  dom.frequencySearchInput.addEventListener("input", renderFrequencyList);
  dom.frequencyMatchModeSelect.addEventListener("change", renderFrequencyList);
  dom.frequencyMinInput.addEventListener("input", renderFrequencyList);
  dom.frequencyTokenClassSelect.addEventListener("change", renderFrequencyList);
  dom.frequencyHideStopwordsInput.addEventListener("change", renderFrequencyList);
  dom.frequencyFilesButton.addEventListener("click", openFrequencyFilesModal);
  dom.frequencyTableHead.addEventListener("click", handleFrequencySortClick);
  dom.frequencyTableBody.addEventListener("click", handleFrequencyTableClick);
  dom.frequencyTableBody.addEventListener("keydown", handleFrequencyTableKeydown);
  dom.frequencyFilesSearchInput.addEventListener("input", renderFrequencyFilesModal);
  dom.frequencyFilesList.addEventListener("change", handleFrequencyFilesListChange);
  dom.frequencyFilesSelectAllButton.addEventListener("click", selectAllFrequencyFiles);
  dom.frequencyFilesClearAllButton.addEventListener("click", clearFrequencyFiles);
  dom.frequencyFilesApplyButton.addEventListener("click", applyFrequencyFileScope);
  dom.frequencyFilesCancelButton.addEventListener("click", closeFrequencyFilesModal);
  dom.frequencyFilesCloseButton.addEventListener("click", closeFrequencyFilesModal);
  document.addEventListener("keydown", handleFrequencyModalKeydown);

  refreshStageNavigator();
  renderAnalysisTools();
  renderAnalysisToolPanels();
}
