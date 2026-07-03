import {
  defaultAnalysisDocumentSelection,
  queryAnnotatedFrequency,
  queryAnnotatedTermDistribution,
  queryAnalysisDocuments,
  queryFrequencyList,
  queryTermDistribution,
  type AnnotatedFrequencyMode,
  type AnnotatedFrequencyResult,
  type AnnotatedTermSelector,
  type AnalysisDocumentPage,
  type AnalysisDocumentSelection,
  type CorpusOverview as AnalysisCorpus,
  type FrequencyGroupMode,
  type FrequencyMatchMode,
  type FrequencyPage,
  type FrequencySortKey,
  type FrequencyTermItem,
  type FrequencyTokenClass as FrequencyTokenClassFilter,
  type SortDirection,
  type TermDistributionPage,
} from "../analysis";
import { createChevron } from "../chevron";
import { dom } from "../dom";
import { effectiveCleaningConfig } from "../effective-cleaning-config";
import {
  ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE,
  annotatedFrequencyModeEnabled,
  frequencyColumnLabels,
  frequencyControlVisibility,
  frequencyDistributionBarValue,
  frequencyDistributionColumnLabels,
  frequencyRateLabel,
  frequencyRateValue,
  frequencyTableColumnCountForMode,
  formatDocumentFrequency,
  frequencyModeLabel,
  type FrequencyRateScale,
  type FrequencyViewMode,
} from "../frequency";
import { state } from "../state";
import {
  activeProjectLanguageCode,
  activeProjectLanguageLabel,
  formatFixedNumber,
  formatNumber,
  inputTextSourceLabel,
  isFrequencyAnalysisToolActive,
} from "./shell";
import {
  refreshConcordance,
  renderConcordance,
  resetConcordanceCorpusState,
  resetConcordanceForFrequencyScopeChange,
} from "./concordance/shared";
import { normaliseConcordanceQuery } from "../concordance";
import { markConcordanceCollocationsForAggregationChange } from "./concordance/collocations";
import {
  annotationCacheStateForUi,
  ensureAnnotationCacheAvailability,
  renderAnalysisNotice,
  renderFrequencyAnnotationLayer,
  resetAnnotationLayerState,
  setAnalysisNotice,
  updateAnnotationCacheAvailabilityFromFrequencyPage,
} from "./build";

const FREQUENCY_PAGE_SIZE = 200;

const FREQUENCY_DOCUMENT_PAGE_SIZE = 200;

const TERM_DISTRIBUTION_PAGE_SIZE = 100;

let frequencyViewMode: FrequencyViewMode = "surface";

let frequencySortKey: FrequencySortKey = "frequency";

let frequencySortDirection: SortDirection = "desc";

let frequencyDocumentScope = defaultAnalysisDocumentSelection();

let frequencyScopeDraft = defaultAnalysisDocumentSelection();

let frequencyPage: FrequencyPage | null = null;

let annotatedFrequencyPage: AnnotatedFrequencyResult | null = null;

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

let annotatedDistributionSelectors = new Map<string, AnnotatedTermSelector>();

function documentName(document: { relativePath: string; sourcePath: string }): string {
  return document.relativePath || document.sourcePath;
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

export function syncFrequencyStopwordsToggle(): void {
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

export function resetFrequencyCorpusState(resetAnnotationLayer = true): void {
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
  frequencyViewMode = "surface";
  frequencyPage = null;
  annotatedFrequencyPage = null;
  frequencyPageOffset = 0;
  frequencyQueryGeneration += 1;
  frequencyQueryPending = false;
  frequencyQueryFailure = null;
  frequencyDocumentsPage = null;
  frequencyDocumentsOffset = 0;
  frequencyDocumentsGeneration += 1;
  frequencyDocumentsPending = false;
  frequencyDocumentsFailure = null;
  resetConcordanceCorpusState();
  expandedFrequencyTokens = new Set<string>();
  termDistributionPages.clear();
  termDistributionGenerations.clear();
  annotatedDistributionSelectors.clear();
  if (resetAnnotationLayer) {
    resetAnnotationLayerState();
  }
}

export function renderFrequencyScopeControl(corpus: AnalysisCorpus | null): void {
  if (!corpus || corpus.documentCount === 0) {
    dom.frequencyFilesButton.disabled = true;
    dom.frequencyFilesButton.textContent = "Files: 0 of 0";
    dom.concordanceFilesButton.disabled = true;
    dom.concordanceFilesButton.textContent = "Files: 0 of 0";
    const label = "No files available for analysis scoping";
    dom.frequencyFilesButton.setAttribute("aria-label", label);
    dom.concordanceFilesButton.setAttribute("aria-label", label);
    return;
  }

  const selectedCount = frequencySelectionCount(frequencyDocumentScope, corpus.documentCount);
  dom.frequencyFilesButton.disabled = false;
  dom.frequencyFilesButton.textContent = `Files: ${formatNumber(selectedCount)} of ${formatNumber(corpus.documentCount)}`;
  dom.concordanceFilesButton.disabled = false;
  dom.concordanceFilesButton.textContent = dom.frequencyFilesButton.textContent;
  const label = `Analysis file scope: ${formatNumber(selectedCount)} of ${formatNumber(corpus.documentCount)} files selected`;
  dom.frequencyFilesButton.setAttribute("aria-label", label);
  dom.concordanceFilesButton.setAttribute("aria-label", label);
}

function renderFrequencySearchError(message: string | null): void {
  dom.frequencySearchError.textContent = message ?? "";
  dom.frequencySearchError.classList.toggle("hidden", message === null);
}

function renderFrequencyQueryNotice(page: FrequencyPage | null): void {
  const messages = page?.notices.map((notice) => notice.message) ?? [];
  if (state.analysisCorpus && annotationCacheStateForUi(state.analysisCorpus) !== "ready") {
    messages.push(ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE);
  }
  const message = messages.join(" ");
  dom.frequencyQueryNotice.textContent = message;
  dom.frequencyQueryNotice.classList.toggle("hidden", message.length === 0);
}

function currentAnnotatedFrequencyMode(): AnnotatedFrequencyMode | null {
  return frequencyViewMode === "surface" ? null : frequencyViewMode;
}

function currentFrequencyRateScale(): FrequencyRateScale {
  return dom.frequencyRateScaleSelect.value as FrequencyRateScale;
}

function frequencyTableColumnCount(): number {
  return frequencyTableColumnCountForMode(frequencyViewMode, currentFrequencyRateScale());
}

function ensureVisibleFrequencySortKey(): void {
  if (currentFrequencyRateScale() === "off" && frequencySortKey === "rate") {
    frequencySortKey = "frequency";
    frequencySortDirection = "desc";
  }
}

function syncFrequencyModeTabs(corpus: AnalysisCorpus | null = state.analysisCorpus): void {
  const cacheState = annotationCacheStateForUi(corpus);
  const buttons = dom.frequencyModeTabs.querySelectorAll<HTMLButtonElement>("[data-frequency-mode]");
  if (corpus === null || !annotatedFrequencyModeEnabled(frequencyViewMode, cacheState)) {
    frequencyViewMode = "surface";
  }
  buttons.forEach((button) => {
    const mode = button.dataset.frequencyMode as FrequencyViewMode | undefined;
    if (!mode) {
      return;
    }
    const enabled = mode === "surface" || (corpus !== null && annotatedFrequencyModeEnabled(mode, cacheState));
    const active = mode === frequencyViewMode;
    button.textContent = frequencyModeLabel(mode);
    button.disabled = !enabled;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
    button.title = enabled
      ? ""
      : corpus === null
        ? "Build surface analysis first."
        : ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE;
  });
}

function setFrequencyControlVisible(element: HTMLElement, visible: boolean): void {
  const container = element.closest<HTMLElement>("label")
    ?? element.closest<HTMLElement>(".frequency-readonly-control");
  container?.classList.toggle("hidden", !visible);
}

function syncFrequencyControlMatrix(corpus: AnalysisCorpus | null = state.analysisCorpus): void {
  const controls = frequencyControlVisibility(frequencyViewMode);
  const enabled = corpus !== null;
  const annotationAvailable = annotationCacheStateForUi(corpus) === "ready";

  setFrequencyControlVisible(dom.frequencySearchInput, controls.query);
  setFrequencyControlVisible(dom.frequencyMatchModeSelect, controls.matchMode);
  setFrequencyControlVisible(dom.frequencyMinInput, controls.minimumFrequency);
  setFrequencyControlVisible(dom.frequencyGroupBySelect, controls.groupBy);
  setFrequencyControlVisible(dom.frequencyCaseFoldInput, controls.caseFold);
  setFrequencyControlVisible(dom.frequencyTokenClassSelect, controls.tokenClass);
  setFrequencyControlVisible(dom.frequencyPosFilterSelect, controls.posFilter);
  setFrequencyControlVisible(dom.frequencyHideStopwordsInput, controls.hideStopwords);
  setFrequencyControlVisible(dom.frequencyRateScaleSelect, controls.rate);

  dom.frequencySearchInput.disabled = !enabled || !controls.query;
  dom.frequencyMatchModeSelect.disabled = !enabled || !controls.matchMode;
  dom.frequencyMinInput.disabled = !enabled || !controls.minimumFrequency;
  dom.frequencyGroupBySelect.disabled = !enabled || !controls.groupBy;
  dom.frequencyTokenClassSelect.disabled = !enabled || !controls.tokenClass;
  dom.frequencyHideStopwordsInput.disabled = !enabled || !controls.hideStopwords;
  dom.frequencyPosFilterSelect.disabled = !enabled || !controls.posFilter || !annotationAvailable;
  dom.frequencyRateScaleSelect.disabled = !enabled || !controls.rate;
}

export function syncFrequencyAnnotationControls(corpus: AnalysisCorpus | null = state.analysisCorpus): void {
  const annotationAvailable = corpus !== null && annotationCacheStateForUi(corpus) === "ready";
  const lemmaOption = dom.frequencyGroupBySelect.querySelector<HTMLOptionElement>('option[value="lemma"]');
  if (lemmaOption) {
    lemmaOption.disabled = !annotationAvailable;
    lemmaOption.textContent = annotationAvailable ? "Lemma" : "Lemma (annotation unavailable)";
  }
  if (!annotationAvailable && dom.frequencyGroupBySelect.value === "lemma") {
    dom.frequencyGroupBySelect.value = "surface";
  }
  dom.frequencyGroupBySelect.title = annotationAvailable ? "" : ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE;

  const anyPosOption = dom.frequencyPosFilterSelect.querySelector<HTMLOptionElement>('option[value=""]');
  if (anyPosOption) {
    anyPosOption.textContent = annotationAvailable ? "Any POS" : "Annotation unavailable";
  }
  if (!annotationAvailable) {
    dom.frequencyPosFilterSelect.value = "";
  }
  dom.frequencyPosFilterSelect.disabled = !annotationAvailable;
  dom.frequencyPosFilterSelect.title = annotationAvailable ? "" : ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE;
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

function currentAnnotatedFrequencyCaseFold(mode: AnnotatedFrequencyMode): boolean {
  return mode === "lemmaPos" && dom.frequencyCaseFoldInput.checked;
}

function currentFrequencyLanguage(): string | null {
  return activeProjectLanguageCode();
}

function currentFrequencyPosFilter(): string | null {
  const value = dom.frequencyPosFilterSelect.value.trim();
  return value.length === 0 ? null : value;
}

export function syncFrequencyCaseFoldToggle(): void {
  const controls = frequencyControlVisibility(frequencyViewMode);
  const stemsAreSelected = currentFrequencyGroupMode() === "stem";
  const isFolded = frequencyViewMode === "surface"
    ? currentFrequencyCaseFold()
    : currentAnnotatedFrequencyCaseFold(frequencyViewMode);
  dom.frequencyCaseFoldInput.disabled = !controls.caseFold || (frequencyViewMode === "surface" && stemsAreSelected);
  if (controls.caseFold) {
    dom.frequencyCaseFoldInput.checked = isFolded;
  }
  dom.frequencyCaseFoldInput.setAttribute("aria-checked", controls.caseFold && isFolded ? "true" : "false");
  dom.frequencyCaseFoldState.textContent = controls.caseFold && isFolded ? "Folded" : "As typed";
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

function annotatedDistributionKey(selector: AnnotatedTermSelector): string {
  switch (selector.mode) {
    case "pos":
      return `annotated:pos:${encodeURIComponent(selector.upos)}`;
    case "lemmaPos":
      return `annotated:lemmaPos:${encodeURIComponent(selector.lemma)}:${encodeURIComponent(selector.upos)}`;
  }
}

function createExpandableTokenCell(label: string, title = label): HTMLTableCellElement {
  const token = document.createElement("td");
  token.className = "frequency-token-cell frequency-col-term";
  token.title = title;
  const tokenContent = document.createElement("span");
  tokenContent.className = "frequency-token-content";
  const caret = createChevron("frequency-token-caret");
  const tokenText = document.createElement("span");
  tokenText.className = "frequency-token-text";
  tokenText.textContent = label;
  tokenContent.append(caret, tokenText);
  token.appendChild(tokenContent);
  return token;
}

function createTextCell(text: string, className: string): HTMLTableCellElement {
  const cell = document.createElement("td");
  cell.className = className;
  cell.textContent = text;
  return cell;
}

function createNumericCell(text: string, className: string): HTMLTableCellElement {
  return createTextCell(text, `numeric ${className}`);
}

function createRateCell(value: number): HTMLTableCellElement {
  return createNumericCell(formatFixedNumber(value, 2), "frequency-col-rate");
}

function frequencyColumnClass(label: string): string {
  switch (label) {
    case "Token":
    case "Lemma":
      return "frequency-col-term";
    case "UPOS":
      return "frequency-col-upos";
    case "Frequency":
      return "frequency-col-frequency";
    case "Range (docs)":
      return "frequency-col-range";
    case "Per 10k":
    case "Per 100k":
    case "Per million":
      return "frequency-col-rate";
    case "Dispersion (DP)":
      return "frequency-col-dispersion";
    default:
      return "";
  }
}

function frequencyColumnSortKey(label: string): FrequencySortKey | null {
  switch (label) {
    case "Token":
    case "Lemma":
      return "token";
    case "UPOS":
      return frequencyViewMode === "pos" ? "token" : null;
    case "Frequency":
      return "frequency";
    case "Range (docs)":
      return "range";
    case "Per 10k":
    case "Per 100k":
    case "Per million":
      return "rate";
    case "Dispersion (DP)":
      return "dispersion";
    default:
      return null;
  }
}

function frequencyColumnAlignsRight(label: string): boolean {
  return label !== "Token" && label !== "Lemma" && label !== "UPOS";
}

function appendHeaderCell(row: HTMLTableRowElement, label: string, className = ""): void {
  const cell = document.createElement("th");
  cell.className = className;
  cell.textContent = label;
  row.appendChild(cell);
}

function appendSortHeaderCell(
  row: HTMLTableRowElement,
  label: string,
  sort: FrequencySortKey,
  alignRight = false,
  className = "",
): void {
  const cell = document.createElement("th");
  cell.className = className;
  const button = document.createElement("button");
  button.type = "button";
  button.className = alignRight ? "frequency-sort frequency-sort-right" : "frequency-sort";
  button.dataset.sort = sort;
  button.textContent = label;
  cell.appendChild(button);
  row.appendChild(cell);
}

function renderFrequencyTableHead(): void {
  ensureVisibleFrequencySortKey();
  const row = document.createElement("tr");
  for (const label of frequencyColumnLabels(frequencyViewMode, currentFrequencyRateScale())) {
    const sortKey = frequencyColumnSortKey(label);
    const className = frequencyColumnClass(label);
    if (sortKey) {
      appendSortHeaderCell(row, label, sortKey, frequencyColumnAlignsRight(label), className);
    } else {
      appendHeaderCell(row, label, className);
    }
  }
  dom.frequencyTableHead.replaceChildren(row);
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

function createDispersionValueCell(dispersion: number): HTMLTableCellElement {
  const cell = document.createElement("td");
  cell.className = "numeric frequency-dp-cell frequency-col-dispersion";

  const value = document.createElement("span");
  value.className = "frequency-dp-value";
  value.textContent = formatFixedNumber(dispersion, 2);

  const track = document.createElement("span");
  track.className = "frequency-dp-track";
  const fill = document.createElement("span");
  fill.className = "frequency-dp-fill";
  fill.dataset.level = dispersionLevel(dispersion);
  fill.style.setProperty("--bar-width", `${Math.max(0, Math.min(100, dispersion * 100))}%`);
  track.appendChild(fill);

  cell.append(value, track);
  return cell;
}

function createDispersionCell(term: FrequencyTermItem): HTMLTableCellElement {
  return createDispersionValueCell(term.dispersion);
}

function frequencyDistributionColumnClass(label: string): string {
  switch (label) {
    case "Document/File":
      return "frequency-distribution-file-col";
    case "Count":
      return "frequency-distribution-count";
    case "Document tokens":
      return "frequency-distribution-tokens";
    case "Per 10k":
    case "Per 100k":
    case "Per million":
      return "frequency-distribution-rate";
    case "Bar":
      return "frequency-distribution-bar";
    default:
      return "";
  }
}

function frequencyDistributionMaxBarValue(page: TermDistributionPage, rateScale: FrequencyRateScale): number {
  switch (rateScale) {
    case "off":
      return Math.max(0, ...page.items.map((item) => item.count));
    case "per10k":
      return page.maxRatePer10k;
    case "per100k":
      return page.maxRatePer10k * 10;
    case "perMillion":
      return page.maxRatePerMillion;
  }
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
    setAnalysisNotice({ kind: "warning", message: `Term distribution could not be loaded: ${error}` });
    expandedFrequencyTokens.delete(token);
    renderAnalysisNotice();
    renderFrequencyList();
  }
}

async function loadAnnotatedTermDistribution(
  distributionKey: string,
  selector: AnnotatedTermSelector,
  offset: number,
): Promise<void> {
  const generation = (termDistributionGenerations.get(distributionKey) ?? 0) + 1;
  termDistributionGenerations.set(distributionKey, generation);
  termDistributionPages.delete(distributionKey);
  renderFrequencyList();
  try {
    const page = await queryAnnotatedTermDistribution(
      {
        selection: frequencyDocumentScope,
        selector,
        offset,
        limit: TERM_DISTRIBUTION_PAGE_SIZE,
      },
      effectiveCleaningConfig(),
    );
    if (termDistributionGenerations.get(distributionKey) !== generation || !expandedFrequencyTokens.has(distributionKey)) {
      return;
    }
    termDistributionPages.set(distributionKey, page);
    renderFrequencyList();
  } catch (error) {
    if (termDistributionGenerations.get(distributionKey) !== generation) {
      return;
    }
    setAnalysisNotice({ kind: "warning", message: `Term distribution could not be loaded: ${error}` });
    expandedFrequencyTokens.delete(distributionKey);
    renderAnalysisNotice();
    renderFrequencyList();
  }
}

function createFrequencyDistributionPanel(
  distributionKey: string,
  loadPage: (offset: number) => void,
): HTMLElement {
  const rateScale = currentFrequencyRateScale();
  const panel = document.createElement("div");
  panel.className = "frequency-distribution-panel";
  panel.dataset.rateScale = rateScale;

  const title = document.createElement("h3");
  title.textContent = "Per-file distribution (selected files)";
  panel.appendChild(title);

  const page = termDistributionPages.get(distributionKey);
  if (!page) {
    const loading = document.createElement("p");
    loading.className = "analysis-panel-note";
    loading.textContent = "Loading distribution...";
    panel.appendChild(loading);
    return panel;
  }

  const header = document.createElement("div");
  header.className = "frequency-distribution-row frequency-distribution-header";
  for (const label of frequencyDistributionColumnLabels(rateScale)) {
    const column = document.createElement("span");
    column.className = frequencyDistributionColumnClass(label);
    column.textContent = label;
    header.appendChild(column);
  }
  panel.appendChild(header);

  const maxBarValue = frequencyDistributionMaxBarValue(page, rateScale);
  const rateLabel = frequencyRateLabel(rateScale);
  for (const item of page.items) {
    const line = document.createElement("div");
    line.className = "frequency-distribution-row";

    const label = document.createElement("span");
    label.className = "frequency-distribution-file frequency-distribution-file-col";
    label.title = documentName(item);
    label.textContent = documentName(item);

    const count = document.createElement("span");
    count.className = "numeric frequency-distribution-count";
    count.textContent = formatNumber(item.count);

    const documentTokens = document.createElement("span");
    documentTokens.className = "numeric frequency-distribution-tokens";
    documentTokens.textContent = formatNumber(item.tokenCount);

    const track = document.createElement("span");
    track.className = "frequency-distribution-track frequency-distribution-bar";
    const fill = document.createElement("span");
    fill.className = "frequency-distribution-fill";
    const barValue = frequencyDistributionBarValue(rateScale, {
      count: item.count,
      per10k: item.ratePer10k,
      perMillion: item.ratePerMillion,
    });
    const barWidth = maxBarValue > 0 && barValue > 0
      ? Math.max(3, (barValue / maxBarValue) * 100)
      : 0;
    fill.style.setProperty("--bar-width", `${barWidth}%`);
    fill.dataset.barValue = formatFixedNumber(barValue, 2);
    track.title = rateLabel ? `${rateLabel}: ${formatFixedNumber(barValue, 2)}` : `Count: ${formatNumber(item.count)}`;
    track.appendChild(fill);

    line.append(label, count, documentTokens);
    const rateValue = frequencyRateValue(rateScale, {
      per10k: item.ratePer10k,
      perMillion: item.ratePerMillion,
    });
    if (rateValue !== null) {
      const rate = document.createElement("span");
      rate.className = "numeric frequency-distribution-rate";
      rate.textContent = formatFixedNumber(rateValue, 2);
      line.appendChild(rate);
    }
    line.appendChild(track);
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
    loadPage(Math.max(0, page.offset - page.limit));
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
    loadPage(page.offset + page.limit);
  });
  pagination.append(previous, status, next);
  panel.appendChild(pagination);
  return panel;
}

export async function refreshFrequencyPage(resetOffset = true): Promise<void> {
  const corpus = state.analysisCorpus;
  if (!corpus) {
    return;
  }
  ensureVisibleFrequencySortKey();
  syncFrequencyModeTabs(corpus);
  syncFrequencyAnnotationControls(corpus);
  if (resetOffset) {
    frequencyPageOffset = 0;
  }
  const generation = ++frequencyQueryGeneration;
  frequencyQueryPending = true;
  frequencyQueryFailure = null;
  frequencyPage = null;
  annotatedFrequencyPage = null;
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  annotatedDistributionSelectors.clear();
  renderFrequencyList();

  try {
    const annotatedMode = currentAnnotatedFrequencyMode();
    if (annotatedMode) {
      const page = await queryAnnotatedFrequency(
        {
          selection: frequencyDocumentScope,
          mode: annotatedMode,
          query: dom.frequencySearchInput.value.trim(),
          matchMode: dom.frequencyMatchModeSelect.value as FrequencyMatchMode,
          minimumFrequency: Math.max(1, Number.parseInt(dom.frequencyMinInput.value, 10) || 1),
          hideStopwords: annotatedMode === "lemmaPos" && dom.frequencyHideStopwordsInput.checked,
          caseFold: currentAnnotatedFrequencyCaseFold(annotatedMode),
          language: currentFrequencyLanguage(),
          posFilter: annotatedMode === "lemmaPos" ? currentFrequencyPosFilter() : null,
          sortKey: frequencySortKey,
          sortDirection: frequencySortDirection,
          offset: frequencyPageOffset,
          limit: FREQUENCY_PAGE_SIZE,
        },
        state.currentCorpusVersion,
        effectiveCleaningConfig(),
      );
      if (frequencyQueryGeneration !== generation) {
        return;
      }
      annotatedFrequencyPage = page;
      updateAnnotationCacheAvailabilityFromFrequencyPage(page.cache);
      frequencyPageOffset = page.offset;
      return;
    }

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

export function scheduleFrequencyRefresh(): void {
  if (frequencyQueryDebounceTimer !== null) {
    window.clearTimeout(frequencyQueryDebounceTimer);
  }
  frequencyQueryDebounceTimer = window.setTimeout(() => {
    frequencyQueryDebounceTimer = null;
    void refreshFrequencyPage();
  }, 150);
}

function annotatedFrequencyVisibleRowCount(page: AnnotatedFrequencyResult): number {
  return page.mode === "pos" ? page.posRows.length : page.lexRows.length;
}

function annotatedFrequencySummaryText(page: AnnotatedFrequencyResult): string {
  const documentScope = page.selectedDocumentCount === page.totalDocumentCount
    ? `${formatNumber(page.totalDocumentCount)} files`
    : `${formatNumber(page.selectedDocumentCount)} of ${formatNumber(page.totalDocumentCount)} files`;
  return [
    `${formatNumber(page.totalRows)} rows`,
    frequencyModeLabel(page.mode),
    documentScope,
    `${formatNumber(page.totalTokens)} annotated tokens in scope`,
  ].join(" · ");
}

function renderAnnotatedFrequencyList(corpus: AnalysisCorpus): void {
  renderFrequencyQueryNotice(null);
  renderFrequencyScopeControl(corpus);
  if (!annotatedFrequencyPage) {
    dom.frequencySummary.textContent = frequencyQueryFailure
      ? "Frequency query failed."
      : "Querying the analysis...";
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = frequencyQueryFailure ?? "Loading frequency results...";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    dom.frequencyPageStatus.textContent = "";
    dom.frequencyPagePreviousButton.disabled = true;
    dom.frequencyPageNextButton.disabled = true;
    if (isFrequencyAnalysisToolActive() && !frequencyQueryPending && !frequencyQueryFailure) {
      void refreshFrequencyPage(false);
    }
    return;
  }

  const page = annotatedFrequencyPage;
  renderFrequencySearchError(page.queryError);
  const unavailable = page.unavailableMessage ?? (page.cache.state === "ready" ? null : ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE);
  dom.frequencyQueryNotice.textContent = unavailable ?? "";
  dom.frequencyQueryNotice.classList.toggle("hidden", unavailable === null);
  dom.frequencySummary.textContent = annotatedFrequencySummaryText(page);
  dom.frequencyResultCount.textContent = `Built from ${inputTextSourceLabel(corpus.inputTextSource)}`;
  const visibleRows = annotatedFrequencyVisibleRowCount(page);
  const first = page.totalRows === 0 ? 0 : page.offset + 1;
  const last = Math.min(page.totalRows, page.offset + visibleRows);
  dom.frequencyPageStatus.textContent = `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(page.totalRows)}`;
  dom.frequencyPagePreviousButton.disabled = page.offset === 0;
  dom.frequencyPageNextButton.disabled = page.offset + visibleRows >= page.totalRows;

  if (unavailable !== null) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = unavailable;
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  if (visibleRows === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = "No annotated frequency rows are available for the selected files.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  const rateScale = currentFrequencyRateScale();
  if (page.mode === "pos") {
    for (const item of page.posRows) {
      const selector: AnnotatedTermSelector = { mode: "pos", upos: item.upos };
      const distributionKey = annotatedDistributionKey(selector);
      annotatedDistributionSelectors.set(distributionKey, selector);
      const isExpanded = expandedFrequencyTokens.has(distributionKey);
      const detailId = frequencyDetailId(distributionKey);
      const row = document.createElement("tr");
      row.className = "frequency-term-row";
      row.dataset.distributionKey = distributionKey;
      row.tabIndex = 0;
      row.setAttribute("aria-expanded", isExpanded ? "true" : "false");
      row.setAttribute("aria-controls", detailId);
      row.setAttribute("aria-label", `${isExpanded ? "Collapse" : "Expand"} per-file distribution for ${item.upos}`);
      const cells = [
        createExpandableTokenCell(item.upos),
        createNumericCell(formatNumber(item.raw), "frequency-col-frequency"),
        createNumericCell(formatDocumentFrequency(item.docFreq, item.docTotal), "frequency-col-range"),
      ];
      const rateValue = frequencyRateValue(rateScale, { per10k: item.per10k });
      if (rateValue !== null) {
        cells.push(createRateCell(rateValue));
      }
      cells.push(
        createDispersionValueCell(item.dp),
      );
      row.append(...cells);
      dom.frequencyTableBody.appendChild(row);
      if (isExpanded) {
        const detailRow = document.createElement("tr");
        detailRow.className = "frequency-detail-row";
        const detailCell = document.createElement("td");
        detailCell.id = detailId;
        detailCell.colSpan = frequencyTableColumnCount();
        detailCell.appendChild(createFrequencyDistributionPanel(distributionKey, (offset) => {
          void loadAnnotatedTermDistribution(distributionKey, selector, offset);
        }));
        detailRow.appendChild(detailCell);
        dom.frequencyTableBody.appendChild(detailRow);
      }
    }
    return;
  }

  for (const item of page.lexRows) {
    const selector: AnnotatedTermSelector = { mode: "lemmaPos", lemma: item.lemma, upos: item.upos };
    const distributionKey = annotatedDistributionKey(selector);
    annotatedDistributionSelectors.set(distributionKey, selector);
    const isExpanded = expandedFrequencyTokens.has(distributionKey);
    const detailId = frequencyDetailId(distributionKey);
    const label = `${item.lemma} ${item.upos}`;
    const row = document.createElement("tr");
    row.className = "frequency-term-row";
    row.dataset.distributionKey = distributionKey;
    row.tabIndex = 0;
    row.setAttribute("aria-expanded", isExpanded ? "true" : "false");
    row.setAttribute("aria-controls", detailId);
    row.setAttribute("aria-label", `${isExpanded ? "Collapse" : "Expand"} per-file distribution for ${label}`);
    const cells = [
      createExpandableTokenCell(item.lemma),
      createTextCell(item.upos, "frequency-col-upos"),
      createNumericCell(formatNumber(item.raw), "frequency-col-frequency"),
      createNumericCell(formatDocumentFrequency(item.docFreq, item.docTotal), "frequency-col-range"),
    ];
    const rateValue = frequencyRateValue(rateScale, { per10k: item.per10k });
    if (rateValue !== null) {
      cells.push(createRateCell(rateValue));
    }
    cells.push(
      createDispersionValueCell(item.dp),
    );
    row.append(...cells);
    dom.frequencyTableBody.appendChild(row);
    if (isExpanded) {
      const detailRow = document.createElement("tr");
      detailRow.className = "frequency-detail-row";
      const detailCell = document.createElement("td");
      detailCell.id = detailId;
      detailCell.colSpan = frequencyTableColumnCount();
      detailCell.appendChild(createFrequencyDistributionPanel(distributionKey, (offset) => {
        void loadAnnotatedTermDistribution(distributionKey, selector, offset);
      }));
      detailRow.appendChild(detailCell);
      dom.frequencyTableBody.appendChild(detailRow);
    }
  }
}

export function renderFrequencyList(): void {
  renderFrequencyTableHead();
  updateFrequencySortHeaders();
  dom.frequencyTableBody.replaceChildren();
  annotatedDistributionSelectors.clear();
  dom.frequencyLimitNote.textContent = "";
  renderFrequencySearchError(frequencyQueryFailure);

  const corpus = state.analysisCorpus;
  ensureAnnotationCacheAvailability(corpus);
  renderFrequencyAnnotationLayer();
  syncFrequencyModeTabs(corpus);
  syncFrequencyAnnotationControls(corpus);
  syncFrequencyControlMatrix(corpus);
  syncFrequencyCaseFoldToggle();
  renderFrequencyQueryNotice(frequencyPage);
  renderFrequencyScopeControl(corpus);
  if (!corpus) {
    dom.frequencyResultCount.textContent = "";
    dom.frequencySummary.textContent = "Build surface analysis to populate the frequency list.";
    dom.frequencyPageStatus.textContent = "";
    dom.frequencyPagePreviousButton.disabled = true;
    dom.frequencyPageNextButton.disabled = true;
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = "Build surface analysis to populate the frequency list.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  if (frequencyViewMode !== "surface") {
    renderAnnotatedFrequencyList(corpus);
    return;
  }

  if (!frequencyPage) {
    dom.frequencySummary.textContent = frequencyQueryFailure
      ? "Frequency query failed."
      : "Querying the analysis...";
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = frequencyQueryFailure ?? "Loading frequency results...";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    dom.frequencyPageStatus.textContent = "";
    dom.frequencyPagePreviousButton.disabled = true;
    dom.frequencyPageNextButton.disabled = true;
    if (isFrequencyAnalysisToolActive() && !frequencyQueryPending && !frequencyQueryFailure) {
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
    cell.colSpan = frequencyTableColumnCount();
    cell.className = "frequency-empty";
    cell.textContent = "No terms match the current filters.";
    row.appendChild(cell);
    dom.frequencyTableBody.appendChild(row);
    return;
  }

  const rateScale = currentFrequencyRateScale();
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

    const token = createExpandableTokenCell(term.token);

    const cells = [
      token,
      createNumericCell(formatNumber(term.frequency), "frequency-col-frequency"),
      createNumericCell(
        `${formatNumber(term.documentFrequency)} / ${formatNumber(page.selectedDocumentCount)}`,
        "frequency-col-range",
      ),
    ];
    const rateValue = frequencyRateValue(rateScale, {
      per10k: term.ratePer10k,
      perMillion: term.ratePerMillion,
    });
    if (rateValue !== null) {
      cells.push(createRateCell(rateValue));
    }
    cells.push(createDispersionCell(term));
    row.append(...cells);
    dom.frequencyTableBody.appendChild(row);

    if (isExpanded) {
      const detailRow = document.createElement("tr");
      detailRow.className = "frequency-detail-row";
      const detailCell = document.createElement("td");
      detailCell.id = detailId;
      detailCell.colSpan = frequencyTableColumnCount();
      detailCell.appendChild(createFrequencyDistributionPanel(term.token, (offset) => {
        void loadTermDistribution(term.token, offset);
      }));
      detailRow.appendChild(detailCell);
      dom.frequencyTableBody.appendChild(detailRow);
    }
  }
}

export function handleFrequencySortClick(event: Event): void {
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

export function handleFrequencyModeClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-frequency-mode]");
  const mode = button?.dataset.frequencyMode as FrequencyViewMode | undefined;
  if (!button || !mode || button.disabled || mode === frequencyViewMode) {
    return;
  }
  if (!annotatedFrequencyModeEnabled(mode, annotationCacheStateForUi())) {
    return;
  }
  frequencyViewMode = mode;
  frequencyPageOffset = 0;
  frequencyPage = null;
  annotatedFrequencyPage = null;
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  annotatedDistributionSelectors.clear();
  renderFrequencyList();
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
  annotatedDistributionSelectors.clear();
  expandedFrequencyTokens.add(token);
  void loadTermDistribution(token, 0);
}

function toggleAnnotatedFrequencyExpansion(distributionKey: string, selector: AnnotatedTermSelector): void {
  if (expandedFrequencyTokens.has(distributionKey)) {
    expandedFrequencyTokens.delete(distributionKey);
    termDistributionGenerations.set(
      distributionKey,
      (termDistributionGenerations.get(distributionKey) ?? 0) + 1,
    );
    termDistributionPages.delete(distributionKey);
    renderFrequencyList();
    return;
  }
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  expandedFrequencyTokens.add(distributionKey);
  void loadAnnotatedTermDistribution(distributionKey, selector, 0);
}

export function showPreviousFrequencyPage(): void {
  if (frequencyViewMode !== "surface") {
    if (!annotatedFrequencyPage || annotatedFrequencyPage.offset === 0) {
      return;
    }
    frequencyPageOffset = Math.max(0, annotatedFrequencyPage.offset - annotatedFrequencyPage.limit);
    void refreshFrequencyPage(false);
    return;
  }
  if (!frequencyPage || frequencyPage.offset === 0) {
    return;
  }
  frequencyPageOffset = Math.max(0, frequencyPage.offset - frequencyPage.limit);
  void refreshFrequencyPage(false);
}

export function showNextFrequencyPage(): void {
  if (frequencyViewMode !== "surface") {
    if (
      !annotatedFrequencyPage ||
      annotatedFrequencyPage.offset + annotatedFrequencyVisibleRowCount(annotatedFrequencyPage) >= annotatedFrequencyPage.totalRows
    ) {
      return;
    }
    frequencyPageOffset = annotatedFrequencyPage.offset + annotatedFrequencyPage.limit;
    void refreshFrequencyPage(false);
    return;
  }
  if (!frequencyPage || frequencyPage.offset + frequencyPage.items.length >= frequencyPage.totalTerms) {
    return;
  }
  frequencyPageOffset = frequencyPage.offset + frequencyPage.limit;
  void refreshFrequencyPage(false);
}

export function handleFrequencyTableClick(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLElement) || target.closest(".frequency-detail-row")) {
    return;
  }

  const row = target.closest<HTMLTableRowElement>(".frequency-term-row");
  if (!row) {
    return;
  }

  if (frequencyViewMode === "surface") {
    if (!row.dataset.token) {
      return;
    }
    toggleFrequencyTokenExpansion(row.dataset.token);
    return;
  }

  const distributionKey = row.dataset.distributionKey;
  const selector = distributionKey ? annotatedDistributionSelectors.get(distributionKey) : undefined;
  if (!distributionKey || !selector) {
    return;
  }
  toggleAnnotatedFrequencyExpansion(distributionKey, selector);
}

export function handleFrequencyTableKeydown(event: KeyboardEvent): void {
  if (event.key !== "Enter" && event.key !== " ") {
    return;
  }

  const target = event.target;
  if (!(target instanceof HTMLElement) || target.closest(".frequency-detail-row")) {
    return;
  }

  const row = target.closest<HTMLTableRowElement>(".frequency-term-row");
  if (!row) {
    return;
  }

  event.preventDefault();
  if (frequencyViewMode === "surface") {
    if (!row.dataset.token) {
      return;
    }
    toggleFrequencyTokenExpansion(row.dataset.token);
    return;
  }

  const distributionKey = row.dataset.distributionKey;
  const selector = distributionKey ? annotatedDistributionSelectors.get(distributionKey) : undefined;
  if (!distributionKey || !selector) {
    return;
  }
  toggleAnnotatedFrequencyExpansion(distributionKey, selector);
}

export function closeFrequencyFilesModal(): void {
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

export function scheduleFrequencyDocumentsRefresh(): void {
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
    dom.frequencyFilesStatus.textContent = "Build surface analysis before choosing files.";
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

export function openFrequencyFilesModal(): void {
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

export function applyFrequencyFileScope(): void {
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
  annotatedFrequencyPage = null;
  expandedFrequencyTokens.clear();
  termDistributionPages.clear();
  annotatedDistributionSelectors.clear();
  closeFrequencyFilesModal();
  resetConcordanceForFrequencyScopeChange();
  void refreshFrequencyPage();
  if (normaliseConcordanceQuery(dom.concordanceQueryInput.value).length > 0) {
    markConcordanceCollocationsForAggregationChange();
    void refreshConcordance(true, true);
  } else {
    renderConcordance();
    markConcordanceCollocationsForAggregationChange();
  }
}

export function handleFrequencyFilesListChange(event: Event): void {
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

export function selectAllFrequencyFiles(): void {
  frequencyScopeDraft = defaultAnalysisDocumentSelection();
  void refreshFrequencyDocumentsPage(false);
}

export function clearFrequencyFiles(): void {
  frequencyScopeDraft = { allDocuments: false, ids: [], excludedIds: [] };
  void refreshFrequencyDocumentsPage(false);
}

export function showPreviousFrequencyDocumentsPage(): void {
  if (!frequencyDocumentsPage || frequencyDocumentsPage.offset === 0) {
    return;
  }
  frequencyDocumentsOffset = Math.max(
    0,
    frequencyDocumentsPage.offset - frequencyDocumentsPage.limit,
  );
  void refreshFrequencyDocumentsPage(false);
}

export function showNextFrequencyDocumentsPage(): void {
  if (!frequencyDocumentsPage
    || frequencyDocumentsPage.offset + frequencyDocumentsPage.items.length >= frequencyDocumentsPage.totalCount) {
    return;
  }
  frequencyDocumentsOffset = frequencyDocumentsPage.offset + frequencyDocumentsPage.limit;
  void refreshFrequencyDocumentsPage(false);
}

export function handleFrequencyModalKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && !dom.frequencyFilesModal.classList.contains("hidden")) {
    closeFrequencyFilesModal();
  }
}

export function currentFrequencyDocumentScope(): AnalysisDocumentSelection {
  return frequencyDocumentScope;
}

export function handleFrequencyRateScaleChange(): void {
  const previousSortKey = frequencySortKey;
  ensureVisibleFrequencySortKey();
  if (previousSortKey === frequencySortKey) {
    renderFrequencyList();
  } else {
    void refreshFrequencyPage();
  }
}
