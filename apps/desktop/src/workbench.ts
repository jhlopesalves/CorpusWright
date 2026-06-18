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

function inputTextSourceLabel(inputTextSource: AnalysisInputSource): string {
  return inputTextSource === "processed" ? "processed text" : "extracted text";
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

function createMetric(value: string, label: string): HTMLElement {
  const metric = document.createElement("div");
  metric.className = "analysis-metric";

  const valueEl = document.createElement("span");
  valueEl.className = "analysis-metric-value";
  valueEl.textContent = value;

  const labelEl = document.createElement("span");
  labelEl.className = "analysis-metric-label";
  labelEl.textContent = label;

  metric.append(valueEl, labelEl);
  return metric;
}

function documentName(document: { relativePath: string; sourcePath: string }): string {
  return document.relativePath || document.sourcePath;
}

function renderTopTermsPreview(corpus: AnalysisCorpus): HTMLElement | null {
  const topTerms = corpus.terms.slice(0, 10);
  if (topTerms.length === 0) {
    return null;
  }

  const section = document.createElement("div");
  section.className = "top-terms-preview";

  const title = document.createElement("h3");
  title.textContent = "Top terms";
  section.appendChild(title);

  const maxFrequency = topTerms[0].frequency;
  for (const term of topTerms) {
    const row = document.createElement("div");
    row.className = "top-term-row";

    const token = document.createElement("span");
    token.className = "top-term-token";
    token.title = term.token;
    token.textContent = term.token;

    const track = document.createElement("span");
    track.className = "term-bar-track";
    const fill = document.createElement("span");
    fill.className = "term-bar-fill";
    fill.style.setProperty("--bar-width", `${Math.max(4, (term.frequency / maxFrequency) * 100)}%`);
    track.appendChild(fill);

    const count = document.createElement("span");
    count.className = "top-term-count";
    count.textContent = formatNumber(term.frequency);

    row.append(token, track, count);
    section.appendChild(row);
  }

  return section;
}

function renderCorpusOverview(): void {
  dom.analyseOverview.replaceChildren();
  const corpus = state.analysisCorpus;
  if (!corpus) {
    const empty = document.createElement("p");
    empty.className = "analysis-empty-state";
    empty.textContent = analysisNotice?.message ??
      "Build an analysis corpus to see document counts, token totals, type counts, and top terms.";
    dom.analyseOverview.appendChild(empty);
    return;
  }

  const nonEmptyDocuments = corpus.documents.filter((document) => document.tokenCount > 0);
  const largest = nonEmptyDocuments.reduce<typeof nonEmptyDocuments[number] | null>(
    (current, document) => current === null || document.tokenCount > current.tokenCount ? document : current,
    null,
  );
  const smallest = nonEmptyDocuments.reduce<typeof nonEmptyDocuments[number] | null>(
    (current, document) => current === null || document.tokenCount < current.tokenCount ? document : current,
    null,
  );
  const averageTokens = corpus.documents.length > 0
    ? corpus.totalTokens / corpus.documents.length
    : 0;
  const typeTokenRatio = corpus.totalTokens > 0
    ? corpus.totalTypes / corpus.totalTokens
    : 0;

  const metrics = document.createElement("div");
  metrics.className = "analysis-metric-grid";
  metrics.append(
    createMetric(formatNumber(corpus.documents.length), "documents"),
    createMetric(formatNumber(corpus.totalTokens), "total tokens"),
    createMetric(formatNumber(corpus.totalTypes), "types"),
    createMetric(formatNumber(averageTokens, 1), "average tokens per document"),
    createMetric(formatFixedNumber(typeTokenRatio, 3), "type-token ratio"),
  );
  dom.analyseOverview.appendChild(metrics);

  if (largest || smallest) {
    const documentRange = document.createElement("div");
    documentRange.className = "analysis-document-range";
    documentRange.append(
      createMetric(largest ? `${documentName(largest)} (${formatNumber(largest.tokenCount)})` : "None", "largest document"),
      createMetric(smallest ? `${documentName(smallest)} (${formatNumber(smallest.tokenCount)})` : "None", "smallest non-empty document"),
    );
    dom.analyseOverview.appendChild(documentRange);
  }

  const topTerms = renderTopTermsPreview(corpus);
  if (topTerms) {
    dom.analyseOverview.appendChild(topTerms);
  }
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
