import { listen } from "@tauri-apps/api/event";
import {
  searchConcordance,
  type ConcordanceContextFilterDirection,
  type ConcordanceProgress,
  type ConcordanceResult,
} from "../../analysis";
import {
  concordanceContextTokenSegments,
  concordanceDistributionCardStatsText,
  concordanceLinesCardStatsText,
  concordanceSummaryMetricsText,
  CONCORDANCE_NO_QUERY_MESSAGE,
  formatConcordanceProgress,
  normaliseConcordanceQuery,
} from "../../concordance";
import { dom } from "../../dom";
import { effectiveCleaningConfig } from "../../effective-cleaning-config";
import { state } from "../../state";
import { currentFrequencyDocumentScope, renderFrequencyScopeControl } from "../frequency";
import {
  concordanceCollocationStatsText,
  concordanceCollocationsArePending,
  makeConcordanceCollocationsDormant,
  markConcordanceCollocationsForAggregationChange,
  renderConcordanceCollocations,
  resetConcordanceCollocationCorpusState,
  syncConcordanceCollocationStopwordsToggle,
} from "./collocations";
import {
  CONCORDANCE_DISTRIBUTION_PAGE_SIZE,
  currentConcordanceDistributionOffset,
  renderConcordanceDistribution,
  resetConcordanceDistributionOffset,
} from "./distribution";
import {
  collapseConcordanceExpansion,
  collapseConcordanceExpansionFromControlChange,
  currentConcordanceLineOffset,
  currentConcordanceLinePageSize,
  currentConcordanceLineSortKey,
  currentConcordanceLineWindow,
  renderConcordanceLines,
  resetConcordanceLineOffset,
  resetConcordanceLinesCorpusState,
  setConcordanceLineOffset,
  syncConcordanceLineSortKeyFromControl,
} from "./lines";
import {
  acceptConcordanceSectionResponse,
  applyConcordanceLinesContextFilter,
  beginConcordanceCardRequest,
  clearConcordanceLinesContextFilter,
  collapseConcordanceCard,
  concordanceCardRequestKey,
  createConcordanceWorkbenchState,
  expandConcordanceCard,
  readConcordanceCollapsedCards,
  rejectConcordanceCardResponse,
  updateConcordanceSharedState,
  writeConcordanceCollapsedCards,
  type ConcordanceCardId,
} from "./state";

const workbenchState = createConcordanceWorkbenchState(
  {
    query: "",
    caseSensitive: false,
    fileScopeKey: "",
    corpusVersion: 0,
    cleaningConfigKey: "",
  },
  readConcordanceCollapsedCards(),
);

let concordanceProgress: ConcordanceProgress | null = null;
let concordanceProgressUnlisten: (() => void) | null = null;

function resultFor(card: "lines" | "distribution"): ConcordanceResult | null {
  return workbenchState[card].data as ConcordanceResult | null;
}

function syncConcordanceCaseToggle(): void {
  const isCaseSensitive = dom.concordanceCaseSensitiveInput.checked;
  dom.concordanceCaseSensitiveInput.setAttribute("aria-checked", isCaseSensitive ? "true" : "false");
  dom.concordanceCaseState.textContent = isCaseSensitive ? "Sensitive" : "Insensitive";
}

export function syncConcordanceControls(): void {
  syncConcordanceCaseToggle();
  syncConcordanceCollocationStopwordsToggle();
  dom.concordanceSortSelect.value = currentConcordanceLineSortKey();
}

export function concordanceCardIsExpanded(card: ConcordanceCardId): boolean {
  return !workbenchState[card].collapsed;
}

export function currentConcordanceSearchResult(): ConcordanceResult | null {
  return resultFor("lines") ?? resultFor("distribution");
}

export function currentConcordanceSearchFailure(): string | null {
  return workbenchState.lines.error ?? workbenchState.distribution.error;
}

export function renderContextTokens(
  tokens: string[],
  activeFilter: string | null,
  caseSensitive: boolean,
): DocumentFragment {
  const fragment = document.createDocumentFragment();
  const segments = concordanceContextTokenSegments(tokens, activeFilter, caseSensitive);
  segments.forEach((segment, index) => {
    if (index > 0) fragment.appendChild(document.createTextNode(" "));
    if (!segment.collocate) {
      fragment.appendChild(document.createTextNode(segment.text));
      return;
    }
    const token = document.createElement("span");
    token.className = "concordance-kwic-pair";
    token.textContent = segment.text;
    fragment.appendChild(token);
  });
  return fragment;
}

function renderConcordanceLinesContextFilter(): void {
  const filter = workbenchState.lines.contextFilter;
  dom.concordanceLinesFilter.classList.toggle("hidden", filter === null);
  if (filter === null) {
    dom.concordanceLinesFilterSummary.textContent = "";
    return;
  }
  const window = currentConcordanceLineWindow();
  const direction = workbenchState.lines.contextFilterDirection;
  const location = direction === "left"
    ? `within ${window} tokens to the left`
    : direction === "right"
      ? `within ${window} tokens to the right`
      : `within ${window} tokens`;
  dom.concordanceLinesFilterSummary.textContent = `Context filter: “${filter}” ${location}`;
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

function renderConcordanceStatus(message: string | null, isError = false): void {
  dom.concordanceStatus.replaceChildren(message ?? "");
  dom.concordanceStatus.classList.toggle("hidden", message === null);
  dom.concordanceStatus.classList.toggle("warning", isError);
}

function cardElements(card: ConcordanceCardId) {
  if (card === "lines") {
    return {
      panel: dom.concordanceLinesPanel,
      stats: dom.concordanceLinesStats,
      collapse: dom.concordanceLinesCollapse,
      controls: dom.concordanceLinesControls,
      body: dom.concordanceLinesBody,
    };
  }
  if (card === "collocates") {
    return {
      panel: dom.concordanceCollocationsPanel,
      stats: dom.concordanceCollocatesStats,
      collapse: dom.concordanceCollocatesCollapse,
      controls: dom.concordanceCollocatesControls,
      body: dom.concordanceCollocatesBody,
    };
  }
  return {
    panel: dom.concordanceDistributionPanel,
    stats: dom.concordanceDistributionStats,
    collapse: dom.concordanceDistributionCollapse,
    controls: dom.concordanceDistributionControls,
    body: dom.concordanceDistributionBody,
  };
}

function linesStatsText(): string {
  const result = resultFor("lines");
  const shown = result?.rows?.length ?? 0;
  return concordanceLinesCardStatsText({
    windowTokens: currentConcordanceLineWindow(),
    sortKey: currentConcordanceLineSortKey(),
    shown,
    totalHits: result?.totalHits ?? 0,
    contextFilter: workbenchState.lines.contextFilter,
  });
}

function distributionStatsText(): string {
  const result = resultFor("distribution");
  const distribution = result?.distribution;
  return concordanceDistributionCardStatsText({
    documentsWithHits: result?.documentsWithHits ?? 0,
    selectedDocumentsTotal: result?.selectedDocumentsTotal ?? 0,
    shown: distribution?.items.length ?? 0,
    totalDocuments: distribution?.totalDocuments ?? 0,
  });
}

function syncConcordanceCardChrome(): void {
  const labels: Record<ConcordanceCardId, string> = {
    lines: "Concordance Lines",
    collocates: "Collocates",
    distribution: "Distribution",
  };
  for (const card of ["lines", "collocates", "distribution"] as const) {
    const cardState = workbenchState[card];
    const elements = cardElements(card);
    elements.panel.classList.toggle("is-collapsed", cardState.collapsed);
    elements.body.classList.toggle("hidden", cardState.collapsed);
    elements.controls.classList.toggle("hidden", cardState.collapsed || !cardState.controlsOpen);
    elements.stats.setAttribute("aria-expanded", cardState.controlsOpen ? "true" : "false");
    elements.collapse.textContent = cardState.collapsed ? "Expand" : "Collapse";
    elements.collapse.setAttribute("aria-expanded", cardState.collapsed ? "false" : "true");
    elements.collapse.setAttribute(
      "aria-label",
      `${cardState.collapsed ? "Expand" : "Collapse"} ${labels[card]}`,
    );
  }
  dom.concordanceLinesStats.textContent = linesStatsText();
  renderConcordanceLinesContextFilter();
  dom.concordanceCollocatesStats.textContent = concordanceCollocationStatsText();
  dom.concordanceDistributionStats.textContent = distributionStatsText();
}

function syncLiveRequestState(query: string): void {
  updateConcordanceSharedState(workbenchState, {
    query,
    caseSensitive: dom.concordanceCaseSensitiveInput.checked,
    fileScopeKey: JSON.stringify(currentFrequencyDocumentScope()),
    corpusVersion: state.currentCorpusVersion,
    cleaningConfigKey: JSON.stringify(effectiveCleaningConfig()),
  });
  workbenchState.lines.window = currentConcordanceLineWindow();
  workbenchState.lines.sort = syncConcordanceLineSortKeyFromControl();
  workbenchState.lines.pageSize = currentConcordanceLinePageSize();
  workbenchState.lines.offset = currentConcordanceLineOffset();
  workbenchState.distribution.pageSize = CONCORDANCE_DISTRIBUTION_PAGE_SIZE;
  workbenchState.distribution.offset = currentConcordanceDistributionOffset();
}

export function resetConcordanceForFrequencyScopeChange(): void {
  resetConcordanceLineOffset();
  resetConcordanceDistributionOffset();
  collapseConcordanceExpansion();
  syncLiveRequestState(normaliseConcordanceQuery(dom.concordanceQueryInput.value));
  renderConcordance();
}

export function resetConcordanceCorpusState(): void {
  resetConcordanceLinesCorpusState();
  resetConcordanceDistributionOffset();
  for (const card of ["lines", "distribution"] as const) {
    workbenchState[card].data = null;
    workbenchState[card].requestKey = null;
    workbenchState[card].requestGeneration += 1;
    workbenchState[card].loading = false;
    workbenchState[card].error = null;
  }
  resetConcordanceCollocationCorpusState();
}

export async function initialiseConcordanceState(): Promise<void> {
  concordanceProgressUnlisten?.();
  concordanceProgressUnlisten = await listen<ConcordanceProgress>("concordance-progress", (event) => {
    if (
      !workbenchState.lines.loading
      && !workbenchState.distribution.loading
      && !concordanceCollocationsArePending()
    ) return;
    concordanceProgress = event.payload;
    renderConcordance();
  });
  syncConcordanceCardChrome();
}

export function renderConcordance(): void {
  renderFrequencyScopeControl(state.analysisCorpus);
  syncConcordanceControls();
  syncConcordanceCardChrome();

  const pending = workbenchState.lines.loading
    || workbenchState.distribution.loading
    || concordanceCollocationsArePending();
  dom.concordanceCancelButton.classList.toggle("hidden", !pending);
  dom.concordanceCancelButton.disabled = !pending;
  dom.concordanceCancelButton.setAttribute("aria-hidden", pending ? "false" : "true");
  dom.concordanceSection.setAttribute("aria-busy", pending ? "true" : "false");

  const summaryResult = currentConcordanceSearchResult();
  if (pending) {
    dom.concordanceSummary.textContent = formatConcordanceProgress(concordanceProgress);
  } else if (summaryResult) {
    renderConcordanceSummary(summaryResult);
  } else {
    dom.concordanceSummary.textContent = CONCORDANCE_NO_QUERY_MESSAGE;
  }

  const error = currentConcordanceSearchFailure();
  renderConcordanceStatus(error, error !== null);

  if (concordanceCardIsExpanded("lines")) {
    renderConcordanceLines(
      workbenchState.lines.loading ? null : resultFor("lines"),
      workbenchState.lines.loading
        ? formatConcordanceProgress(concordanceProgress)
        : workbenchState.lines.error ?? CONCORDANCE_NO_QUERY_MESSAGE,
    );
  }
  if (concordanceCardIsExpanded("distribution")) {
    renderConcordanceDistribution(
      workbenchState.distribution.loading ? null : resultFor("distribution"),
      workbenchState.distribution.loading
        ? formatConcordanceProgress(concordanceProgress)
        : workbenchState.distribution.error ?? CONCORDANCE_NO_QUERY_MESSAGE,
    );
  }
  renderConcordanceCollocations();
  syncConcordanceCardChrome();
}

async function refreshConcordanceCards(
  requestedCards: ReadonlyArray<"lines" | "distribution">,
): Promise<void> {
  if (
    requestedCards.includes("lines")
    && requestedCards.includes("distribution")
    && workbenchState.lines.contextFilter !== null
  ) {
    await Promise.all([
      refreshConcordanceCards(["lines"]),
      refreshConcordanceCards(["distribution"]),
    ]);
    return;
  }
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  syncLiveRequestState(query);
  const cards = requestedCards.filter((card) => concordanceCardIsExpanded(card));
  if (query.length === 0 || cards.length === 0) {
    for (const card of cards) {
      workbenchState[card].data = null;
      workbenchState[card].requestKey = null;
      workbenchState[card].loading = false;
      workbenchState[card].error = null;
    }
    renderConcordance();
    return;
  }

  const requests = new Map<"lines" | "distribution", { generation: number; requestKey: string }>();
  for (const card of cards) {
    const request = beginConcordanceCardRequest(workbenchState, card);
    if (request) requests.set(card, request);
  }
  if (requests.size === 0) return;
  concordanceProgress = null;
  renderConcordance();

  const includeRows = requests.has("lines");
  const includeDistribution = requests.has("distribution");
  try {
    const result = await searchConcordance(
      {
        selection: currentFrequencyDocumentScope(),
        query,
        contextFilter: includeDistribution ? null : workbenchState.lines.contextFilter,
        contextFilterDirection: workbenchState.lines.contextFilterDirection,
        caseSensitive: dom.concordanceCaseSensitiveInput.checked,
        sortKey: workbenchState.lines.sort,
        windowTokens: workbenchState.lines.window,
        offset: workbenchState.lines.offset,
        limit: workbenchState.lines.pageSize,
        includeRows,
        distributionOffset: workbenchState.distribution.offset,
        distributionLimit: workbenchState.distribution.pageSize,
        includeDistribution,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    const accepted = acceptConcordanceSectionResponse(workbenchState, requests, result);
    if (accepted.includes("lines")) setConcordanceLineOffset(result.offset);
  } catch (error) {
    for (const [card, request] of requests) {
      rejectConcordanceCardResponse(
        workbenchState,
        card,
        request.generation,
        request.requestKey,
        `${error}`,
      );
    }
  } finally {
    renderConcordance();
  }
}

export async function refreshConcordanceLines(): Promise<void> {
  await refreshConcordanceCards(["lines"]);
}

export function openCollocateInConcordanceLines(
  surface: string,
  direction: ConcordanceContextFilterDirection,
): void {
  collapseConcordanceExpansion();
  resetConcordanceLineOffset();
  applyConcordanceLinesContextFilter(workbenchState, surface, direction);
  writeConcordanceCollapsedCards(workbenchState);
  void refreshConcordanceLines();
  requestAnimationFrame(() => {
    dom.concordanceLinesPanel.focus({ preventScroll: true });
    dom.concordanceLinesPanel.scrollIntoView({ behavior: "smooth", block: "start" });
  });
}

export function handleClearConcordanceLinesContextFilter(): void {
  collapseConcordanceExpansion();
  resetConcordanceLineOffset();
  clearConcordanceLinesContextFilter(workbenchState);
  void refreshConcordanceLines();
}

export async function refreshConcordanceDistribution(): Promise<void> {
  await refreshConcordanceCards(["distribution"]);
}

export async function refreshConcordance(): Promise<void> {
  await refreshConcordanceCards(["lines", "distribution"]);
}

export function runConcordanceSearch(): void {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  resetConcordanceLineOffset();
  resetConcordanceDistributionOffset();
  collapseConcordanceExpansion();
  workbenchState.lines.contextFilter = null;
  workbenchState.lines.contextFilterDirection = "both";
  syncLiveRequestState(query);
  markConcordanceCollocationsForAggregationChange();
  if (concordanceCardIsExpanded("lines") || concordanceCardIsExpanded("distribution")) {
    void refreshConcordance().finally(renderConcordance);
  } else {
    renderConcordance();
  }
}

export function handleConcordanceCardAction(event: MouseEvent): void {
  const button = event.target instanceof HTMLElement
    ? event.target.closest<HTMLButtonElement>("[data-concordance-card-action]")
    : null;
  const card = button?.dataset.concordanceCard as ConcordanceCardId | undefined;
  const action = button?.dataset.concordanceCardAction;
  if (!button || !card || !(["lines", "collocates", "distribution"] as const).includes(card)) return;

  if (action === "controls") {
    workbenchState[card].controlsOpen = !workbenchState[card].controlsOpen;
    renderConcordance();
    return;
  }
  if (action !== "collapse") return;

  if (workbenchState[card].collapsed) {
    syncLiveRequestState(normaliseConcordanceQuery(dom.concordanceQueryInput.value));
    const disposition = expandConcordanceCard(workbenchState, card);
    writeConcordanceCollapsedCards(workbenchState);
    renderConcordance();
    if (disposition === "query") {
      if (card === "lines") void refreshConcordanceLines();
      if (card === "distribution") void refreshConcordanceDistribution();
    }
    cardElements(card).panel.focus({ preventScroll: true });
    return;
  }

  collapseConcordanceCard(workbenchState, card);
  if (card === "lines") dom.concordanceTableBody.replaceChildren();
  if (card === "distribution") dom.concordanceDistribution.replaceChildren();
  if (card === "collocates") {
    makeConcordanceCollocationsDormant();
    dom.concordanceCollocationsTableBody.replaceChildren();
  }
  writeConcordanceCollapsedCards(workbenchState);
  renderConcordance();
}

export function handleConcordanceQueryInput(): void {
  collapseConcordanceExpansionFromControlChange();
  markConcordanceCollocationsForAggregationChange();
}

export function handleConcordanceCaseChange(): void {
  syncConcordanceCaseToggle();
  collapseConcordanceExpansionFromControlChange();
  markConcordanceCollocationsForAggregationChange();
}

export function currentConcordanceCardRequestKey(card: ConcordanceCardId): string {
  return concordanceCardRequestKey(workbenchState, card);
}
