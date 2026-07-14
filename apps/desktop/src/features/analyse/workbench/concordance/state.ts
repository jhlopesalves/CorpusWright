import type {
  ConcordanceCollocationDirection,
  ConcordanceCollocationMeasure,
  ConcordanceCollocationSortKey,
  ConcordanceContextFilterDirection,
  ConcordanceResult,
  ConcordanceSortKey,
  SortDirection,
} from "../../api";
import {
  CONCORDANCE_DEFAULT_PAGE_SIZE,
  CONCORDANCE_DEFAULT_WINDOW,
} from "../../concordance/model.ts";

export type ConcordanceCardId = "lines" | "collocates" | "distribution";
export type ConcordanceDistributionOrdering = "hitsDescending";

export interface ConcordanceWorkbenchSharedState {
  query: string;
  caseSensitive: boolean;
  fileScopeKey: string;
  corpusVersion: number;
  cleaningConfigKey: string;
}

export interface ConcordanceCardState<TData = unknown> {
  loading: boolean;
  error: string | null;
  data: TData | null;
  requestKey: string | null;
  requestGeneration: number;
}

export interface ConcordanceLinesLocalState {
  window: number;
  sort: ConcordanceSortKey[];
  pageSize: number;
  offset: number;
  contextFilter: string | null;
  contextFilterDirection: ConcordanceContextFilterDirection;
}

export interface ConcordanceCollocatesLocalState {
  span: number;
  direction: ConcordanceCollocationDirection;
  measure: ConcordanceCollocationMeasure;
  minimumCooccurrence: number;
  minimumFrequency: number;
  hideStopwords: boolean;
  sortKey: ConcordanceCollocationSortKey;
  sortDirection: SortDirection;
  requestLimit: number;
  displayCount: number;
  selectedEvidenceRow: string | null;
}

export interface ConcordanceDistributionLocalState {
  ordering: ConcordanceDistributionOrdering;
  pageSize: number;
  offset: number;
}

export interface ConcordanceWorkbenchState {
  shared: ConcordanceWorkbenchSharedState;
  lines: ConcordanceCardState & ConcordanceLinesLocalState;
  collocates: ConcordanceCardState & ConcordanceCollocatesLocalState;
  distribution: ConcordanceCardState & ConcordanceDistributionLocalState;
}

export interface ConcordanceSharedRequestPlan {
  concordance: null | {
    includeRows: boolean;
    includeDistribution: boolean;
    cards: Array<"lines" | "distribution">;
  };
  collocates: boolean;
}

export async function executeConcordanceSharedRequest(
  plan: ConcordanceSharedRequestPlan,
  refreshConcordance: () => Promise<boolean>,
  refreshCollocations: () => Promise<void>,
): Promise<void> {
  if (plan.concordance === null) return;
  const concordanceSucceeded = await refreshConcordance();
  if (concordanceSucceeded && plan.collocates) await refreshCollocations();
}

export type ConcordanceSectionRequest = { generation: number; requestKey: string };

function cardState<TData>(): ConcordanceCardState<TData> {
  return {
    loading: false,
    error: null,
    data: null,
    requestKey: null,
    requestGeneration: 0,
  };
}

export function createConcordanceWorkbenchState(
  shared: ConcordanceWorkbenchSharedState,
): ConcordanceWorkbenchState {
  return {
    shared: { ...shared },
    lines: {
      ...cardState(),
      window: CONCORDANCE_DEFAULT_WINDOW,
      sort: ["right1", "right2", "right3"],
      pageSize: CONCORDANCE_DEFAULT_PAGE_SIZE,
      offset: 0,
      contextFilter: null,
      contextFilterDirection: "both",
    },
    collocates: {
      ...cardState(),
      span: 5,
      direction: "both",
      measure: "logDice",
      minimumCooccurrence: 5,
      minimumFrequency: 5,
      hideStopwords: false,
      sortKey: "association",
      sortDirection: "desc",
      requestLimit: 500,
      displayCount: 10,
      selectedEvidenceRow: null,
    },
    distribution: {
      ...cardState(),
      ordering: "hitsDescending",
      pageSize: 10,
      offset: 0,
    },
  };
}

export function concordanceCardRequestKey(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): string {
  const shared = state.shared;
  const sharedKey = {
    query: shared.query,
    caseSensitive: shared.caseSensitive,
    fileScopeKey: shared.fileScopeKey,
    corpusVersion: shared.corpusVersion,
    cleaningConfigKey: shared.cleaningConfigKey,
  };
  if (card === "lines") {
    return JSON.stringify({
      card,
      shared: sharedKey,
      window: state.lines.window,
      sort: state.lines.sort,
      pageSize: state.lines.pageSize,
      offset: state.lines.offset,
      contextFilter: state.lines.contextFilter,
      contextFilterDirection: state.lines.contextFilterDirection,
    });
  }
  if (card === "collocates") {
    return JSON.stringify({
      card,
      shared: sharedKey,
      span: state.collocates.span,
      direction: state.collocates.direction,
      measure: state.collocates.measure,
      minimumCooccurrence: state.collocates.minimumCooccurrence,
      minimumFrequency: state.collocates.minimumFrequency,
      hideStopwords: state.collocates.hideStopwords,
      sortKey: state.collocates.sortKey,
      sortDirection: state.collocates.sortDirection,
      requestLimit: state.collocates.requestLimit,
    });
  }
  return JSON.stringify({
    card,
    shared: sharedKey,
    ordering: state.distribution.ordering,
    pageSize: state.distribution.pageSize,
    offset: state.distribution.offset,
  });
}

export function updateConcordanceSharedState(
  state: ConcordanceWorkbenchState,
  shared: ConcordanceWorkbenchSharedState,
): ConcordanceCardId[] {
  const previousKeys = new Map(
    (["lines", "collocates", "distribution"] as const).map((card) => [
      card,
      concordanceCardRequestKey(state, card),
    ]),
  );
  state.shared = { ...shared };
  const changed: ConcordanceCardId[] = [];
  for (const card of ["lines", "collocates", "distribution"] as const) {
    if (previousKeys.get(card) === concordanceCardRequestKey(state, card)) continue;
    state[card].loading = false;
    state[card].error = null;
    state[card].requestGeneration += 1;
    changed.push(card);
  }
  return changed;
}

export function planConcordanceSharedRequest(
  _state: ConcordanceWorkbenchState,
): ConcordanceSharedRequestPlan {
  const cards: Array<"lines" | "distribution"> = ["lines", "distribution"];
  return {
    concordance: {
      includeRows: true,
      includeDistribution: true,
      cards,
    },
    collocates: true,
  };
}

export function beginConcordanceCardRequest(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): { generation: number; requestKey: string } | null {
  const target = state[card];
  target.loading = true;
  target.error = null;
  target.requestGeneration += 1;
  return {
    generation: target.requestGeneration,
    requestKey: concordanceCardRequestKey(state, card),
  };
}

export function acceptConcordanceCardResponse(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
  generation: number,
  requestKey: string,
  data: unknown,
): boolean {
  const target = state[card];
  if (
    generation !== target.requestGeneration
    || requestKey !== concordanceCardRequestKey(state, card)
  ) {
    return false;
  }
  target.loading = false;
  target.error = null;
  target.data = data;
  target.requestKey = requestKey;
  return true;
}

export function acceptConcordanceSectionResponse(
  state: ConcordanceWorkbenchState,
  requests: ReadonlyMap<"lines" | "distribution", ConcordanceSectionRequest>,
  result: ConcordanceResult,
): Array<"lines" | "distribution"> {
  const accepted: Array<"lines" | "distribution"> = [];
  const linesRequest = requests.get("lines");
  if (
    linesRequest
    && result.rows !== null
    && acceptConcordanceCardResponse(
      state,
      "lines",
      linesRequest.generation,
      linesRequest.requestKey,
      result,
    )
  ) accepted.push("lines");
  const distributionRequest = requests.get("distribution");
  if (
    distributionRequest
    && result.distribution !== null
    && acceptConcordanceCardResponse(
      state,
      "distribution",
      distributionRequest.generation,
      distributionRequest.requestKey,
      result,
    )
  ) accepted.push("distribution");
  return accepted;
}

export function rejectConcordanceCardResponse(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
  generation: number,
  requestKey: string,
  error: string,
): boolean {
  const target = state[card];
  if (
    generation !== target.requestGeneration
    || requestKey !== concordanceCardRequestKey(state, card)
  ) {
    return false;
  }
  target.loading = false;
  target.error = error;
  return true;
}

export function invalidateConcordanceCard(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): boolean {
  const target = state[card];
  target.loading = false;
  target.error = null;
  target.requestGeneration += 1;
  return true;
}

export function planConcordanceLocalRequest(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): ConcordanceCardId[] {
  if (card === "collocates") {
    invalidateConcordanceCard(state, card);
    return [];
  }
  return invalidateConcordanceCard(state, card) ? [card] : [];
}

export function applyConcordanceLinesContextFilter(
  state: ConcordanceWorkbenchState,
  contextFilter: string,
  direction: ConcordanceContextFilterDirection,
): void {
  state.lines.contextFilter = contextFilter;
  state.lines.contextFilterDirection = direction;
  state.lines.offset = 0;
  invalidateConcordanceCard(state, "lines");
}

export function clearConcordanceLinesContextFilter(
  state: ConcordanceWorkbenchState,
): void {
  state.lines.contextFilter = null;
  state.lines.contextFilterDirection = "both";
  state.lines.offset = 0;
  invalidateConcordanceCard(state, "lines");
}
