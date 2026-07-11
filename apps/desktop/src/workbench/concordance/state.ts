import type {
  ConcordanceCollocationDirection,
  ConcordanceCollocationMeasure,
  ConcordanceCollocationSortKey,
  ConcordanceContextFilterDirection,
  ConcordanceResult,
  ConcordanceSortKey,
  SortDirection,
} from "../../analysis";

export const CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY =
  "corpuswright.concordance-workbench.collapsed-cards.v1";

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
  collapsed: boolean;
  controlsOpen: boolean;
  loading: boolean;
  error: string | null;
  data: TData | null;
  requestKey: string | null;
  requestGeneration: number;
}

export interface ConcordanceLinesLocalState {
  window: number;
  sort: ConcordanceSortKey;
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

export type ConcordanceSectionRequest = { generation: number; requestKey: string };

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function cardState<TData>(collapsed: boolean): ConcordanceCardState<TData> {
  return {
    collapsed,
    controlsOpen: false,
    loading: false,
    error: null,
    data: null,
    requestKey: null,
    requestGeneration: 0,
  };
}

function availableStorage(): StorageLike | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function readConcordanceCollapsedCards(
  storage: StorageLike | null = availableStorage(),
): Set<ConcordanceCardId> | null {
  if (!storage) return null;
  try {
    const raw = storage.getItem(CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY);
    if (raw === null) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    return new Set(
      parsed.filter(
        (value): value is ConcordanceCardId =>
          value === "lines" || value === "collocates" || value === "distribution",
      ),
    );
  } catch {
    return null;
  }
}

export function writeConcordanceCollapsedCards(
  state: ConcordanceWorkbenchState,
  storage: StorageLike | null = availableStorage(),
): void {
  if (!storage) return;
  const collapsed = (["lines", "collocates", "distribution"] as const)
    .filter((card) => state[card].collapsed)
    .sort();
  try {
    storage.setItem(CONCORDANCE_WORKBENCH_COLLAPSE_STORAGE_KEY, JSON.stringify(collapsed));
  } catch {
    // Collapse persistence is best-effort; the workbench remains usable without storage.
  }
}

export function createConcordanceWorkbenchState(
  shared: ConcordanceWorkbenchSharedState,
  collapsedCards: ReadonlySet<ConcordanceCardId> | null = readConcordanceCollapsedCards(),
): ConcordanceWorkbenchState {
  const initialCollapsedCards = collapsedCards ?? new Set<ConcordanceCardId>(["collocates"]);
  return {
    shared: { ...shared },
    lines: {
      ...cardState(initialCollapsedCards.has("lines")),
      window: 5,
      sort: "filePosition",
      pageSize: 100,
      offset: 0,
      contextFilter: null,
      contextFilterDirection: "both",
    },
    collocates: {
      ...cardState(initialCollapsedCards.has("collocates")),
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
      ...cardState(initialCollapsedCards.has("distribution")),
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
  state: ConcordanceWorkbenchState,
): ConcordanceSharedRequestPlan {
  const cards: Array<"lines" | "distribution"> = [];
  if (!state.lines.collapsed) cards.push("lines");
  if (!state.distribution.collapsed) cards.push("distribution");
  return {
    concordance: cards.length === 0
      ? null
      : {
          includeRows: cards.includes("lines"),
          includeDistribution: cards.includes("distribution"),
          cards,
        },
    collocates: false,
  };
}

export function beginConcordanceCardRequest(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): { generation: number; requestKey: string } | null {
  const target = state[card];
  if (target.collapsed) return null;
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
    target.collapsed
    || generation !== target.requestGeneration
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
    target.collapsed
    || generation !== target.requestGeneration
    || requestKey !== concordanceCardRequestKey(state, card)
  ) {
    return false;
  }
  target.loading = false;
  target.error = error;
  return true;
}

export function collapseConcordanceCard(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): void {
  const target = state[card];
  target.collapsed = true;
  target.loading = false;
  target.requestGeneration += 1;
}

export function expandConcordanceCard(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): "cache" | "query" {
  const target = state[card];
  target.collapsed = false;
  return target.data !== null && target.requestKey === concordanceCardRequestKey(state, card)
    ? "cache"
    : "query";
}

export function invalidateConcordanceCard(
  state: ConcordanceWorkbenchState,
  card: ConcordanceCardId,
): boolean {
  const target = state[card];
  target.loading = false;
  target.error = null;
  target.requestGeneration += 1;
  return !target.collapsed;
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
  state.lines.collapsed = false;
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
