import {
  buildAnalysisCorpus,
  loadAnalysisInputTexts,
  type AnalysisCorpus,
  type AnalysisInputSource,
  type AnalysisTerm,
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

type FrequencySortKey = "token" | "frequency" | "documentFrequency" | "rate";
type SortDirection = "asc" | "desc";

let frequencySortKey: FrequencySortKey = "frequency";
let frequencySortDirection: SortDirection = "desc";
let analysisNotice: { kind: "info" | "warning"; message: string } | null = null;

function pluralise(count: number, singular: string, plural = `${singular}s`): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
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
    case "ready": return "Ready";
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
    dom.analyseStatusChip.textContent = "Analysis corpus: Ready";
    dom.analyseStatusDetail.textContent =
      `${pluralise(corpus.documents.length, "document")} analysed from ${corpus.inputTextSource} text.`;
    dom.analyseBuildNote.textContent = `Built ${new Date(corpus.builtAt).toLocaleString()}.`;
    return;
  }

  if (status === "stale") {
    dom.analyseStatusChip.textContent = "Analysis corpus: Stale";
    dom.analyseStatusDetail.textContent =
      "Prepare-side inputs changed after the analysis corpus was built. Rebuild to refresh the counts.";
    dom.analyseBuildNote.textContent = "Rebuild keeps the analysis data in memory for this session.";
    return;
  }

  dom.analyseStatusChip.textContent = "Analysis corpus: Not built";
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

  dom.analyseInputBanner.textContent = corpus.inputTextSource === "processed"
    ? "Built from processed text"
    : "Exploratory analysis built from extracted text because processed text is not available yet.";
  dom.analyseInputBanner.classList.toggle("warning", corpus.inputTextSource === "extracted");
  dom.analyseInputBanner.classList.remove("hidden");
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
  const corpusReady = state.analysisCorpus !== null && !analysisIsStale();
  const tools = [
    { name: "Corpus Overview", status: corpusReady ? "Ready" : "Needs build", available: true },
    { name: "Frequency List", status: corpusReady ? "Ready" : "Needs build", available: true },
    ...FUTURE_ANALYSIS_TOOLS.map((name) => ({ name, status: "Coming later", available: false })),
  ];

  for (const tool of tools) {
    const item = document.createElement("li");
    item.className = tool.available
      ? "analyse-tool-item is-available"
      : "analyse-tool-item is-future";
    if (!tool.available) {
      item.setAttribute("aria-disabled", "true");
    }

    const name = document.createElement("span");
    name.textContent = tool.name;

    const status = document.createElement("span");
    status.className = "analyse-tool-status";
    status.textContent = tool.status;

    item.append(name, status);
    dom.analyseToolsList.appendChild(item);
  }
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
    count.textContent = term.frequency.toLocaleString();

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

  const metrics = document.createElement("div");
  metrics.className = "analysis-metric-grid";
  metrics.append(
    createMetric(formatNumber(corpus.documents.length), "documents analysed"),
    createMetric(formatNumber(corpus.sourceCount), "sources analysed"),
    createMetric(formatNumber(corpus.totalTokens), "total tokens"),
    createMetric(formatNumber(corpus.totalTypes), "types"),
    createMetric(formatNumber(averageTokens, 1), "average tokens per document"),
    createMetric(largest ? `${documentName(largest)} (${formatNumber(largest.tokenCount)})` : "None", "largest document"),
    createMetric(smallest ? `${documentName(smallest)} (${formatNumber(smallest.tokenCount)})` : "None", "smallest non-empty document"),
    createMetric(corpus.inputTextSource, "input text source"),
  );
  dom.analyseOverview.appendChild(metrics);

  const topTerms = renderTopTermsPreview(corpus);
  if (topTerms) {
    dom.analyseOverview.appendChild(topTerms);
  }
}

function sortTerms(terms: AnalysisTerm[]): AnalysisTerm[] {
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
      case "documentFrequency":
        comparison = a.documentFrequency - b.documentFrequency;
        break;
      case "rate":
        comparison = a.ratePer10k - b.ratePer10k;
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

function renderFrequencyList(): void {
  updateFrequencySortHeaders();
  dom.frequencyTableBody.replaceChildren();
  dom.frequencyLimitNote.textContent = "";

  const corpus = state.analysisCorpus;
  if (!corpus) {
    dom.frequencyResultCount.textContent = "";
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "frequency-empty";
    cell.textContent = "Build an analysis corpus to populate the frequency list.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  const query = dom.frequencySearchInput.value.trim().toLowerCase();
  const minFrequency = Math.max(1, Number.parseInt(dom.frequencyMinInput.value, 10) || 1);
  const filtered = corpus.terms.filter((term) =>
    term.frequency >= minFrequency &&
    (query.length === 0 || term.token.includes(query))
  );
  const sorted = sortTerms(filtered);
  const visible = sorted.slice(0, FREQUENCY_DISPLAY_LIMIT);
  const maxFrequency = filtered.reduce((max, term) => Math.max(max, term.frequency), 0);

  dom.frequencyResultCount.textContent = `${formatNumber(filtered.length)} of ${formatNumber(corpus.terms.length)} terms`;
  if (sorted.length > visible.length) {
    dom.frequencyLimitNote.textContent =
      `Showing first ${formatNumber(visible.length)} of ${formatNumber(sorted.length)} terms`;
  }

  if (visible.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 5;
    cell.className = "frequency-empty";
    cell.textContent = "No terms match the current filters.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  for (const term of visible) {
    const row = document.createElement("tr");

    const token = document.createElement("td");
    token.className = "frequency-token-cell";
    token.title = term.token;
    token.textContent = term.token;

    const frequency = document.createElement("td");
    frequency.className = "numeric";
    frequency.textContent = formatNumber(term.frequency);

    const documentFrequency = document.createElement("td");
    documentFrequency.className = "numeric";
    documentFrequency.textContent = formatNumber(term.documentFrequency);

    const rate = document.createElement("td");
    rate.className = "numeric";
    rate.textContent = formatNumber(term.ratePer10k, 2);

    const bar = document.createElement("td");
    const track = document.createElement("span");
    track.className = "frequency-bar-track";
    const fill = document.createElement("span");
    fill.className = "frequency-bar-fill";
    fill.style.setProperty("--bar-width", `${maxFrequency > 0 ? Math.max(3, (term.frequency / maxFrequency) * 100) : 0}%`);
    track.appendChild(fill);
    bar.appendChild(track);

    row.append(token, frequency, documentFrequency, rate, bar);
    dom.frequencyTableBody.appendChild(row);
  }
}

export function renderAnalyseWorkbench(): void {
  renderAnalyseStatus();
  renderAnalysisNotice();
  renderProjectSummary();
  renderAnalysisTools();
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

export function resetWorkbenchForProject(): void {
  analysisNotice = null;
  dom.workbenchSwitcher.classList.remove("hidden");
  setActiveWorkbench("prepare");
}

export function hideWorkbenchSwitcher(): void {
  analysisNotice = null;
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
      analysisNotice = {
        kind: "warning",
        message: "No text available for analysis. Go to Prepare, extract/process sources, then return to Analyse.",
      };
      dom.statusBar.textContent = "No text available for analysis.";
      return;
    }

    state.analysisCorpus = buildAnalysisCorpus(result.documents, inputTextSource, buildSignature);
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
  dom.frequencySearchInput.addEventListener("input", renderFrequencyList);
  dom.frequencyMinInput.addEventListener("input", renderFrequencyList);
  dom.frequencyTableHead.addEventListener("click", handleFrequencySortClick);

  refreshStageNavigator();
  renderAnalysisTools();
}
