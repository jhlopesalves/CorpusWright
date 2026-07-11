import { listen } from "@tauri-apps/api/event";
import {
  searchConcordance,
  type ConcordanceProgress,
  type ConcordanceResult,
} from "../../analysis";
import {
  concordanceContextTokenSegments,
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
  currentConcordanceCollocationMeasure,
  markConcordanceCollocationsForAggregationChange,
  maybeAutoComputeConcordanceCollocations,
  renderConcordanceCollocations,
  resetConcordanceCollocationCorpusState,
  runConcordanceSearchForActiveView,
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

type ConcordanceViewId = "lines" | "collocates" | "distribution";

let concordanceResult: ConcordanceResult | null = null;
let activeConcordanceView: ConcordanceViewId = "lines";
let concordanceQueryGeneration = 0;
let concordancePending = false;
let concordanceFailure: string | null = null;
let concordanceProgress: ConcordanceProgress | null = null;
let concordanceProgressUnlisten: (() => void) | null = null;

function syncConcordanceCaseToggle(): void {
  const isCaseSensitive = dom.concordanceCaseSensitiveInput.checked;
  dom.concordanceCaseSensitiveInput.setAttribute("aria-checked", isCaseSensitive ? "true" : "false");
  dom.concordanceCaseState.textContent = isCaseSensitive ? "Sensitive" : "Insensitive";
}

export function syncConcordanceControls(): void {
  syncConcordanceCaseToggle();
  syncConcordanceCollocationStopwordsToggle();
  dom.concordanceSortSelect.value = currentConcordanceLineSortKey();
  dom.concordanceCollocationMeasureSelect.value = currentConcordanceCollocationMeasure();
}

export function activeConcordanceViewId(): ConcordanceViewId {
  return activeConcordanceView;
}

export function currentConcordanceSearchResult(): ConcordanceResult | null {
  return concordanceResult;
}

export function currentConcordanceSearchFailure(): string | null {
  return concordanceFailure;
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

export function handleConcordanceViewTabClick(event: MouseEvent): void {
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

export function renderContextTokens(
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

function renderConcordanceSummary(result: ConcordanceResult): void {
  const query = document.createElement("span");
  query.className = "concordance-summary-query";
  query.textContent = result.query;
  const metrics = document.createElement("span");
  metrics.className = "concordance-summary-scope";
  metrics.textContent = ` · ${concordanceSummaryMetricsText(result)}`;
  dom.concordanceSummary.replaceChildren(query, metrics);
}

export function resetConcordanceForFrequencyScopeChange(): void {
  resetConcordanceLineOffset();
  resetConcordanceDistributionOffset();
  concordanceResult = null;
  collapseConcordanceExpansion();
}

export function resetConcordanceCorpusState(): void {
  concordanceResult = null;
  resetConcordanceLinesCorpusState();
  resetConcordanceDistributionOffset();
  concordanceQueryGeneration += 1;
  concordancePending = false;
  concordanceFailure = null;
  resetConcordanceCollocationCorpusState();
}

export async function initialiseConcordanceState(): Promise<void> {
  if (concordanceProgressUnlisten) {
    concordanceProgressUnlisten();
  }
  concordanceProgressUnlisten = await listen<ConcordanceProgress>("concordance-progress", (event) => {
    if (!concordancePending) {
      return;
    }
    concordanceProgress = event.payload;
    renderConcordance();
  });
}

export function renderConcordance(): void {
  renderFrequencyScopeControl(state.analysisCorpus);
  syncConcordanceControls();
  syncConcordanceViewTabs();

  if (concordancePending) {
    dom.concordanceCancelButton.classList.remove("hidden");
    const progressText = formatConcordanceProgress(concordanceProgress);
    dom.concordanceSummary.textContent = progressText;
    renderConcordanceStatus(null);
    renderConcordanceLines(null, progressText);
    renderConcordanceDistribution(null, progressText);
    renderConcordanceCollocations();
    return;
  }
  dom.concordanceCancelButton.classList.add("hidden");

  if (concordanceFailure) {
    dom.concordanceSummary.textContent = CONCORDANCE_NO_QUERY_MESSAGE;
    renderConcordanceStatus(concordanceFailure, true);
    renderConcordanceLines(null, concordanceFailure);
    renderConcordanceDistribution(null, concordanceFailure);
    renderConcordanceCollocations();
    return;
  }

  const result = concordanceResult;
  if (!result) {
    dom.concordanceSummary.textContent = CONCORDANCE_NO_QUERY_MESSAGE;
    renderConcordanceStatus(null);
    renderConcordanceLines(null, CONCORDANCE_NO_QUERY_MESSAGE);
    renderConcordanceDistribution(null, CONCORDANCE_NO_QUERY_MESSAGE);
    renderConcordanceCollocations();
    return;
  }

  renderConcordanceSummary(result);
  renderConcordanceStatus(null);
  renderConcordanceLines(result);
  renderConcordanceDistribution(result);
  renderConcordanceCollocations();
}

export async function refreshConcordance(
  resetOffset = true,
  resetDistribution = true,
  collapseExpansion = true,
): Promise<void> {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (resetOffset) {
    resetConcordanceLineOffset();
  }
  if (resetDistribution) {
    resetConcordanceDistributionOffset();
  }
  if (collapseExpansion) {
    collapseConcordanceExpansion();
  }
  const concordanceSortKey = syncConcordanceLineSortKeyFromControl();
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
        windowTokens: currentConcordanceLineWindow(),
        offset: currentConcordanceLineOffset(),
        limit: currentConcordanceLinePageSize(),
        includeRows: true,
        distributionOffset: currentConcordanceDistributionOffset(),
        distributionLimit: CONCORDANCE_DISTRIBUTION_PAGE_SIZE,
        includeDistribution: true,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== concordanceQueryGeneration) {
      return;
    }
    concordanceResult = result;
    setConcordanceLineOffset(result.offset);
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

export function runConcordanceSearch(): void {
  void runConcordanceSearchForActiveView();
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
