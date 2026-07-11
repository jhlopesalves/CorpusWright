import {
  expandConcordanceContext,
  queryConcordanceCollocations,
  searchConcordance,
  type ConcordanceCollocateRow,
  type ConcordanceCollocationDirection,
  type ConcordanceCollocationMeasure,
  type ConcordanceCollocationResult,
  type ConcordanceCollocationSortKey,
  type ConcordanceContextFilterDirection,
  type ConcordanceExpansionResult,
  type ConcordanceResult,
  type ConcordanceRow,
  type SortDirection,
} from "../../analysis";
import {
  clampConcordanceCollocationFloor,
  clampConcordanceCollocationSpan,
  concordanceCollocationExplanation,
  concordanceCollocationMeasureLabel,
  concordanceCollocationScore,
  concordanceCollocationScoreBarPercent,
  concordanceCollocationSortHeaderLabel,
  concordanceRowExpansionKey,
  CONCORDANCE_COLLOCATION_LIMIT,
  CONCORDANCE_COLLOCATION_SORT_OPTIONS,
  CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE,
  CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE,
  CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE,
  CONCORDANCE_COLLOCATION_STALE_MESSAGE,
  CONCORDANCE_EXPANSION_WINDOW,
  DEFAULT_CONCORDANCE_COLLOCATION_MEASURE,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY,
  normaliseConcordanceQuery,
  toggleConcordanceExpansionKey,
} from "../../concordance";
import { dom } from "../../dom";
import { effectiveCleaningConfig } from "../../effective-cleaning-config";
import { state } from "../../state";
import { currentFrequencyDocumentScope } from "../frequency";
import { activeProjectLanguageCode, formatFixedNumber, formatNumber } from "../shell";
import {
  activeConcordanceViewId,
  currentConcordanceSearchFailure,
  currentConcordanceSearchResult,
  refreshConcordance,
  renderContextTokens,
  syncConcordanceControls,
} from "./shared";

const CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT = 8;

type ConcordanceCollocationState = "idle_not_computed" | "loading" | "ready" | "stale" | "error";

type CollocateEvidenceSelection = {
  surface: string;
  query: string;
  caseSensitive: boolean;
  span: number;
  direction: ConcordanceContextFilterDirection;
};

let concordanceCollocationState: ConcordanceCollocationState = "idle_not_computed";

let concordanceCollocationResult: ConcordanceCollocationResult | null = null;

let concordanceCollocationPending = false;

let concordanceCollocationFailure: string | null = null;

let concordanceCollocationGeneration = 0;

let concordanceCollocationFreshAggregationKey: string | null = null;

let concordanceCollocationEverComputed = false;

let concordanceCollocationAutoComputedAggregationKeys = new Set<string>();

let concordanceCollocationInFlightRequestKey: string | null = null;

let concordanceCollocationSupersededNotice: string | null = null;

let concordanceCollocationMeasure: ConcordanceCollocationMeasure =
  DEFAULT_CONCORDANCE_COLLOCATION_MEASURE;

let concordanceCollocationSortKey: ConcordanceCollocationSortKey =
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY;

let concordanceCollocationSortDirection: SortDirection =
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION;

let collocateEvidenceSelection: CollocateEvidenceSelection | null = null;

let collocateEvidenceResult: ConcordanceResult | null = null;

let collocateEvidencePending = false;

let collocateEvidenceFailure: string | null = null;

let collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;

let collocateEvidenceGeneration = 0;

let collocateEvidenceExpansionKey: string | null = null;

let collocateEvidenceExpansionPending = false;

let collocateEvidenceExpansionFailure: string | null = null;

let collocateEvidenceExpansionResult: ConcordanceExpansionResult | null = null;

let collocateEvidenceExpansionGeneration = 0;

function currentConcordanceCollocationSpan(): number {
  const span = clampConcordanceCollocationSpan(
    Number.parseInt(dom.concordanceCollocationSpanInput.value, 10),
  );
  dom.concordanceCollocationSpanInput.value = span.toString();
  return span;
}

function currentConcordanceCollocationDirection(): ConcordanceCollocationDirection {
  return dom.concordanceCollocationDirectionSelect.value as ConcordanceCollocationDirection;
}

export function currentConcordanceCollocationMeasure(): ConcordanceCollocationMeasure {
  return concordanceCollocationMeasure;
}

function currentConcordanceCollocationSortDirection(): SortDirection {
  return concordanceCollocationSortDirection;
}

function currentConcordanceCollocationMinO11(): number {
  const floor = clampConcordanceCollocationFloor(
    Number.parseInt(dom.concordanceCollocationMinO11Input.value, 10),
  );
  dom.concordanceCollocationMinO11Input.value = floor.toString();
  return floor;
}

function currentConcordanceCollocationMinFrequency(): number {
  const floor = clampConcordanceCollocationFloor(
    Number.parseInt(dom.concordanceCollocationMinFreqInput.value, 10),
  );
  dom.concordanceCollocationMinFreqInput.value = floor.toString();
  return floor;
}

function currentConcordanceCollocationAggregationKey(): string | null {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (query.length === 0) {
    return null;
  }
  return JSON.stringify({
    corpusVersion: state.currentCorpusVersion,
    cleaningConfig: effectiveCleaningConfig(),
    selection: currentFrequencyDocumentScope(),
    query,
    caseSensitive: dom.concordanceCaseSensitiveInput.checked,
    span: currentConcordanceCollocationSpan(),
    direction: currentConcordanceCollocationDirection(),
  });
}

function currentConcordanceCollocationRequestKey(): string | null {
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  if (aggregationKey === null) {
    return null;
  }
  return JSON.stringify({
    aggregationKey,
    minCooccurrence: currentConcordanceCollocationMinO11(),
    minCollocateFrequency: currentConcordanceCollocationMinFrequency(),
    hideStopwords: dom.concordanceCollocationHideStopwordsInput.checked,
    language: activeProjectLanguageCode(),
    sortMeasure: concordanceCollocationMeasure,
    sortKey: concordanceCollocationSortKey,
    sortDirection: currentConcordanceCollocationSortDirection(),
    limit: CONCORDANCE_COLLOCATION_LIMIT,
  });
}

function currentConcordanceCollocationResultIsFresh(): boolean {
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  return aggregationKey !== null
    && concordanceCollocationFreshAggregationKey === aggregationKey
    && concordanceCollocationResult !== null
    && concordanceCollocationState === "ready";
}

function clearCollocateEvidence(): void {
  collocateEvidenceSelection = null;
  collocateEvidenceResult = null;
  collocateEvidencePending = false;
  collocateEvidenceFailure = null;
  collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;
  collocateEvidenceGeneration += 1;
  collocateEvidenceExpansionKey = null;
  collocateEvidenceExpansionPending = false;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  collocateEvidenceExpansionGeneration += 1;
}

function resetCollocateEvidenceForQueryChange(): void {
  clearCollocateEvidence();
}

export function syncConcordanceCollocationStopwordsToggle(): void {
  const hidden = dom.concordanceCollocationHideStopwordsInput.checked;
  dom.concordanceCollocationHideStopwordsInput.setAttribute("aria-checked", hidden ? "true" : "false");
  dom.concordanceCollocationStopwordState.textContent = hidden ? "Hidden" : "Show";
}

export function markConcordanceCollocationsForAggregationChange(): void {
  resetCollocateEvidenceForQueryChange();
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  const shouldShowStaleCallout = activeConcordanceViewId() === "collocates" && concordanceCollocationEverComputed;
  if (
    aggregationKey !== null
    && concordanceCollocationFreshAggregationKey === aggregationKey
    && concordanceCollocationResult !== null
  ) {
    concordanceCollocationState = "ready";
    concordanceCollocationFailure = null;
    concordanceCollocationSupersededNotice = null;
    renderConcordanceCollocations();
    return;
  }

  concordanceCollocationGeneration += 1;
  concordanceCollocationPending = false;
  concordanceCollocationInFlightRequestKey = null;
  concordanceCollocationFailure = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationResult = null;

  if (aggregationKey === null) {
    concordanceCollocationState = "idle_not_computed";
    concordanceCollocationFreshAggregationKey = null;
  } else {
    concordanceCollocationState = shouldShowStaleCallout ? "stale" : "idle_not_computed";
  }

  renderConcordanceCollocations();
}

function noteSupersededConcordanceCollocationResult(): void {
  if (concordanceCollocationState !== "stale") {
    return;
  }
  concordanceCollocationSupersededNotice =
    "An older collocate result was ignored because the settings changed.";
  renderConcordanceCollocations();
}

function refreshConcordanceCollocationsForPresentationChange(): void {
  if (
    concordanceCollocationPending
    && currentConcordanceCollocationRequestKey() !== concordanceCollocationInFlightRequestKey
  ) {
    void refreshConcordanceCollocations();
    return;
  }
  if (currentConcordanceCollocationResultIsFresh()) {
    void refreshConcordanceCollocations();
  } else {
    renderConcordanceCollocations();
  }
}

export function maybeAutoComputeConcordanceCollocations(): void {
  if (activeConcordanceViewId() !== "collocates") {
    return;
  }
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  if (currentConcordanceCollocationResultIsFresh()) {
    renderConcordanceCollocations();
    return;
  }
  if (
    concordanceCollocationState === "idle_not_computed"
    && aggregationKey !== null
    && !concordanceCollocationAutoComputedAggregationKeys.has(aggregationKey)
  ) {
    concordanceCollocationAutoComputedAggregationKeys.add(aggregationKey);
    void refreshConcordanceCollocations();
    return;
  }
  renderConcordanceCollocations();
}

function renderConcordanceCollocationStatus(content: string | Node | null, isError = false): void {
  dom.concordanceCollocationsStatus.replaceChildren();
  if (typeof content === "string") {
    dom.concordanceCollocationsStatus.textContent = content;
  } else if (content) {
    dom.concordanceCollocationsStatus.appendChild(content);
  }
  dom.concordanceCollocationsStatus.classList.toggle("hidden", content === null);
  dom.concordanceCollocationsStatus.classList.toggle("warning", isError);
}

function renderConcordanceCollocationAction(
  message: string,
  detail: string | null,
  label: string,
): HTMLElement {
  const prompt = document.createElement("div");
  prompt.className = "concordance-collocation-action";
  const copy = document.createElement("span");
  copy.className = "concordance-collocation-action-copy";
  copy.textContent = detail ? `${message} ${detail}` : message;
  const action = document.createElement("button");
  action.type = "button";
  action.className = "secondary-btn concordance-collocation-action-button";
  action.textContent = label;
  action.disabled = concordanceCollocationPending;
  action.addEventListener("click", () => {
    void refreshConcordanceCollocations();
  });
  prompt.append(copy, action);
  return prompt;
}

function collocateEvidenceExpansionDomId(rowKey: string): string {
  return `concordance-collocate-evidence-expansion-${rowKey.replace(/[^a-z0-9_-]/gi, "-")}`;
}

async function toggleCollocateEvidenceExpansion(row: ConcordanceRow): Promise<void> {
  const rowKey = concordanceRowExpansionKey(row);
  const nextKey = toggleConcordanceExpansionKey(collocateEvidenceExpansionKey, rowKey);
  if (nextKey === null) {
    collocateEvidenceExpansionKey = null;
    collocateEvidenceExpansionPending = false;
    renderConcordanceCollocations();
    return;
  }

  collocateEvidenceExpansionKey = nextKey;
  collocateEvidenceExpansionPending = true;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  const generation = ++collocateEvidenceExpansionGeneration;
  renderConcordanceCollocations();

  try {
    const result = await expandConcordanceContext(
      {
        documentIndex: row.documentIndex,
        tokenIndex: row.tokenIndex,
        nodeTokenLength: row.nodeTokenLength,
        windowTokens: CONCORDANCE_EXPANSION_WINDOW,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== collocateEvidenceExpansionGeneration) {
      return;
    }
    collocateEvidenceExpansionResult = result;
  } catch (error) {
    if (generation !== collocateEvidenceExpansionGeneration) {
      return;
    }
    collocateEvidenceExpansionFailure = `${error}`;
  } finally {
    if (generation === collocateEvidenceExpansionGeneration) {
      collocateEvidenceExpansionPending = false;
      renderConcordanceCollocations();
    }
  }
}

function renderConcordanceCollocationTableState(message: string): void {
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = 4;
  cell.className = "frequency-empty concordance-empty";
  cell.textContent = message;
  row.appendChild(cell);
  dom.concordanceCollocationsTableBody.replaceChildren(row);
}

function formatConcordanceCollocationScore(value: number | null): string {
  return value === null ? "n/a" : formatFixedNumber(value, 2);
}

function appendConcordanceCollocationHeader(
  row: HTMLTableRowElement,
  label: string,
  notation = "",
  title = "",
): void {
  const cell = document.createElement("th");
  if (title) {
    cell.title = title;
  }
  const content = document.createElement("span");
  content.className = "concordance-collocation-heading";
  const primary = document.createElement("span");
  primary.className = "concordance-collocation-heading-primary";
  primary.textContent = label;
  content.appendChild(primary);
  if (notation) {
    const sublabel = document.createElement("span");
    sublabel.className = "concordance-collocation-heading-sublabel";
    sublabel.textContent = notation;
    content.appendChild(sublabel);
  }
  cell.appendChild(content);
  row.appendChild(cell);
}

function appendConcordanceCollocationSortHeader(
  row: HTMLTableRowElement,
  sortKey: ConcordanceCollocationSortKey,
  measure: ConcordanceCollocationMeasure,
): void {
  const option = CONCORDANCE_COLLOCATION_SORT_OPTIONS.find((item) => item.value === sortKey);
  const cell = document.createElement("th");
  if (option?.title) {
    cell.title = option.title;
  }
  const button = document.createElement("button");
  button.type = "button";
  button.className = "frequency-sort frequency-sort-right concordance-collocation-sort";
  button.dataset.collocationSort = sortKey;

  const content = document.createElement("span");
  content.className = "concordance-collocation-heading";
  const primary = document.createElement("span");
  primary.className = "concordance-collocation-heading-primary";
  primary.textContent = concordanceCollocationSortHeaderLabel(sortKey, measure);
  content.appendChild(primary);
  if (option?.notation) {
    const sublabel = document.createElement("span");
    sublabel.className = "concordance-collocation-heading-sublabel";
    sublabel.textContent = option.notation;
    content.appendChild(sublabel);
  }
  button.appendChild(content);
  cell.appendChild(button);
  row.appendChild(cell);
}

function renderConcordanceCollocationTableHead(measure: ConcordanceCollocationMeasure): void {
  const row = document.createElement("tr");
  appendConcordanceCollocationHeader(row, "Collocate");
  appendConcordanceCollocationSortHeader(row, "nearQuery", measure);
  appendConcordanceCollocationSortHeader(row, "corpusFrequency", measure);
  appendConcordanceCollocationSortHeader(row, "association", measure);
  dom.concordanceCollocationsTableHead.replaceChildren(row);
  updateConcordanceCollocationSortHeaders();
}

function updateConcordanceCollocationSortHeaders(): void {
  const buttons = dom.concordanceCollocationsTableHead
    .querySelectorAll<HTMLButtonElement>(".concordance-collocation-sort");
  buttons.forEach((button) => {
    const sort = button.dataset.collocationSort as ConcordanceCollocationSortKey | undefined;
    button.removeAttribute("aria-sort");
    if (sort === concordanceCollocationSortKey) {
      button.setAttribute(
        "aria-sort",
        concordanceCollocationSortDirection === "asc" ? "ascending" : "descending",
      );
    }
  });
}

function collocateEvidenceSpanText(span: number, direction: ConcordanceContextFilterDirection): string {
  if (direction === "left") {
    return `span ${formatNumber(span)}L`;
  }
  if (direction === "right") {
    return `span ${formatNumber(span)}R`;
  }
  return `span ${formatNumber(span)}L–${formatNumber(span)}R`;
}

function isActiveCollocateEvidenceRow(row: ConcordanceCollocateRow): boolean {
  const selection = collocateEvidenceSelection;
  if (selection === null) {
    return false;
  }
  return row.surface === selection.surface
    && selection.query === normaliseConcordanceQuery(dom.concordanceQueryInput.value)
    && selection.caseSensitive === dom.concordanceCaseSensitiveInput.checked
    && selection.span === currentConcordanceCollocationSpan()
    && selection.direction === currentConcordanceCollocationDirection();
}

function createCollocateEvidenceSnippet(row: ConcordanceRow): HTMLSpanElement {
  const snippet = document.createElement("span");
  snippet.className = "concordance-collocate-evidence-snippet";
  snippet.setAttribute("role", "cell");
  const selection = collocateEvidenceSelection;
  const activeFilter = selection?.surface ?? null;
  const caseSensitive = selection?.caseSensitive ?? false;

  if (row.leftContext.length > 0) {
    snippet.appendChild(renderContextTokens(row.leftContext, activeFilter, caseSensitive));
    snippet.appendChild(document.createTextNode(" "));
  }

  const node = document.createElement("mark");
  node.className = "concordance-node";
  node.textContent = row.node;
  snippet.appendChild(node);

  if (row.rightContext.length > 0) {
    snippet.appendChild(document.createTextNode(" "));
    snippet.appendChild(renderContextTokens(row.rightContext, activeFilter, caseSensitive));
  }

  return snippet;
}

function createCollocateEvidenceExpansionRow(row: ConcordanceRow): HTMLDivElement {
  const rowKey = concordanceRowExpansionKey(row);
  const item = document.createElement("div");
  item.id = collocateEvidenceExpansionDomId(rowKey);
  item.className = "concordance-collocate-evidence-expansion-row";
  item.setAttribute("role", "row");

  const panel = document.createElement("div");
  panel.className = "concordance-collocate-evidence-expansion";
  panel.setAttribute("role", "cell");

  if (collocateEvidenceExpansionPending) {
    appendCollocateEvidenceMessage(panel, "Loading wider context...");
  } else if (collocateEvidenceExpansionFailure !== null) {
    appendCollocateEvidenceMessage(panel, collocateEvidenceExpansionFailure, true);
  } else if (collocateEvidenceExpansionResult !== null) {
    const context = document.createElement("p");
    context.className = "concordance-collocate-evidence-expansion-context";
    const activeFilter = collocateEvidenceSelection?.surface ?? null;
    const caseSensitive = collocateEvidenceSelection?.caseSensitive ?? false;

    if (collocateEvidenceExpansionResult.leftContext.length > 0) {
      const left = document.createElement("span");
      left.className = "concordance-collocate-evidence-expansion-left";
      left.appendChild(renderContextTokens(
        collocateEvidenceExpansionResult.leftContext,
        activeFilter,
        caseSensitive,
      ));
      context.appendChild(left);
      context.appendChild(document.createTextNode(" "));
    }

    const node = document.createElement("mark");
    node.className = "concordance-node";
    node.textContent = collocateEvidenceExpansionResult.nodeTokens.join(" ");
    context.appendChild(node);

    if (collocateEvidenceExpansionResult.rightContext.length > 0) {
      const right = document.createElement("span");
      right.className = "concordance-collocate-evidence-expansion-right";
      context.appendChild(document.createTextNode(" "));
      right.appendChild(renderContextTokens(
        collocateEvidenceExpansionResult.rightContext,
        activeFilter,
        caseSensitive,
      ));
      context.appendChild(right);
    }

    panel.appendChild(context);
  } else {
    appendCollocateEvidenceMessage(panel, "Loading wider context...");
  }

  item.appendChild(panel);
  return item;
}

function createCollocateEvidenceLine(row: ConcordanceRow): HTMLDivElement {
  const item = document.createElement("div");
  const rowKey = concordanceRowExpansionKey(row);
  const expanded = collocateEvidenceExpansionKey === rowKey;
  item.className = "concordance-collocate-evidence-line";
  item.classList.toggle("is-expanded", expanded);
  item.tabIndex = 0;
  item.setAttribute("role", "row");
  item.setAttribute("aria-expanded", expanded ? "true" : "false");
  item.setAttribute("aria-controls", collocateEvidenceExpansionDomId(rowKey));
  item.title = "Show wider context";
  item.addEventListener("click", () => {
    void toggleCollocateEvidenceExpansion(row);
  });
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    void toggleCollocateEvidenceExpansion(row);
  });

  const file = document.createElement("span");
  file.className = "concordance-collocate-evidence-file";
  file.setAttribute("role", "cell");
  file.title = row.displayName || row.sourcePath;
  file.textContent = row.displayName || row.sourcePath;

  item.append(file, createCollocateEvidenceSnippet(row));
  return item;
}

function appendCollocateEvidenceMessage(parent: HTMLElement, message: string, isError = false): void {
  const note = document.createElement("p");
  note.className = `concordance-collocate-evidence-message${isError ? " is-error" : ""}`;
  note.textContent = message;
  parent.appendChild(note);
}

function createCollocateEvidenceList(result: ConcordanceResult): HTMLDivElement {
  const list = document.createElement("div");
  list.className = "concordance-collocate-evidence-list";
  list.setAttribute("role", "table");

  const head = document.createElement("div");
  head.className = "concordance-collocate-evidence-list-head";
  head.setAttribute("role", "row");
  for (const label of ["File", "Context"]) {
    const cell = document.createElement("span");
    cell.setAttribute("role", "columnheader");
    cell.textContent = label;
    head.appendChild(cell);
  }
  list.appendChild(head);

  for (const evidenceRow of result.rows ?? []) {
    list.appendChild(createCollocateEvidenceLine(evidenceRow));
    if (collocateEvidenceExpansionKey === concordanceRowExpansionKey(evidenceRow)) {
      list.appendChild(createCollocateEvidenceExpansionRow(evidenceRow));
    }
  }
  return list;
}

function appendCollocateEvidenceShowMore(parent: HTMLElement, result: ConcordanceResult): void {
  const shown = Math.min(result.totalHits, result.rows?.length ?? 0);
  const remaining = Math.max(0, result.totalHits - shown);
  const nextCount = Math.min(CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT, remaining);
  if (remaining <= 0) {
    return;
  }

  const controls = document.createElement("div");
  controls.className = "concordance-collocate-evidence-more";
  const status = document.createElement("span");
  status.textContent = `Showing ${formatNumber(shown)} of ${formatNumber(result.totalHits)}`;

  const more = document.createElement("button");
  more.type = "button";
  more.className = "secondary-btn";
  more.textContent = collocateEvidencePending
    ? "Loading..."
    : remaining <= CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT
      ? `Show remaining ${formatNumber(remaining)}`
      : `Show ${formatNumber(nextCount)} more`;
  more.disabled = collocateEvidencePending;
  more.addEventListener("click", () => {
    collocateEvidenceLimit += nextCount;
    void refreshCollocateEvidence({ preserveResult: true });
  });

  controls.append(status, more);
  parent.appendChild(controls);
}

function createCollocateEvidenceDrawerRow(row: ConcordanceCollocateRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-collocate-evidence-row";

  const cell = document.createElement("td");
  cell.className = "concordance-collocate-evidence-cell";
  cell.colSpan = 4;

  const selection = collocateEvidenceSelection;
  const panel = document.createElement("div");
  panel.className = "concordance-collocate-evidence";

  const header = document.createElement("div");
  header.className = "concordance-collocate-evidence-header";
  const copy = document.createElement("div");
  copy.className = "concordance-collocate-evidence-copy";
  const title = document.createElement("strong");
  title.className = "concordance-collocate-evidence-title";
  const query = selection?.query ?? normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const direction = selection?.direction ?? "both";
  title.textContent = `Co-occurrence lines for "${row.surface}" with "${query}"`;
  const meta = document.createElement("span");
  meta.className = "concordance-collocate-evidence-meta";
  const span = selection?.span ?? currentConcordanceCollocationSpan();
  const spanText = collocateEvidenceSpanText(span, direction);
  if (collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0) {
    const hitCount = collocateEvidenceResult.totalHits;
    const lineText = `${formatNumber(hitCount)} line${hitCount === 1 ? "" : "s"}`;
    meta.textContent = `${lineText} · ${spanText}`;
  } else {
    meta.textContent = spanText;
  }
  copy.append(title, meta);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "secondary-btn concordance-collocate-evidence-close";
  close.textContent = "Close";
  close.addEventListener("click", () => {
    clearCollocateEvidence();
    renderConcordanceCollocations();
  });
  header.append(copy, close);
  panel.appendChild(header);

  if (collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0) {
    panel.appendChild(createCollocateEvidenceList(collocateEvidenceResult));
    appendCollocateEvidenceShowMore(panel, collocateEvidenceResult);
    if (collocateEvidenceFailure !== null) {
      appendCollocateEvidenceMessage(panel, `Could not load more co-occurrence lines: ${collocateEvidenceFailure}`, true);
    }
  } else if (collocateEvidencePending) {
    appendCollocateEvidenceMessage(panel, "Loading co-occurrence lines...");
  } else if (collocateEvidenceFailure !== null) {
    appendCollocateEvidenceMessage(panel, `Could not load co-occurrence lines: ${collocateEvidenceFailure}`, true);
  } else if (collocateEvidenceResult === null || collocateEvidenceResult.totalHits === 0) {
    appendCollocateEvidenceMessage(
      panel,
      "No co-occurrence lines for this collocate with the current span and direction.",
    );
  }

  cell.appendChild(panel);
  item.appendChild(cell);
  return item;
}

function handleConcordanceCollocateClick(row: ConcordanceCollocateRow): void {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  if (query.length === 0) {
    return;
  }

  collocateEvidenceSelection = {
    surface: row.surface,
    query,
    caseSensitive: dom.concordanceCaseSensitiveInput.checked,
    span: currentConcordanceCollocationSpan(),
    direction: currentConcordanceCollocationDirection() as ConcordanceContextFilterDirection,
  };
  collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT;
  collocateEvidenceFailure = null;
  collocateEvidenceResult = null;
  void refreshCollocateEvidence();
}

function createConcordanceCollocationRow(
  row: ConcordanceCollocateRow,
  measure: ConcordanceCollocationMeasure,
  visibleScores: Array<number | null>,
): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-collocation-row";
  item.classList.toggle("is-active", isActiveCollocateEvidenceRow(row));
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const actionLabel = `Show co-occurrence lines for "${row.surface}" with "${query}"`;
  item.tabIndex = 0;
  item.setAttribute("role", "button");
  item.setAttribute("aria-label", actionLabel);
  item.title = actionLabel;
  item.addEventListener("click", () => handleConcordanceCollocateClick(row));
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    handleConcordanceCollocateClick(row);
  });

  const surface = document.createElement("td");
  const text = document.createElement("span");
  text.className = "concordance-collocate-text";
  text.textContent = row.surface;
  surface.appendChild(text);

  const o11 = document.createElement("td");
  o11.className = "numeric";
  o11.textContent = formatNumber(row.o11);

  const frequency = document.createElement("td");
  frequency.className = "numeric";
  frequency.textContent = formatNumber(row.collocateFrequency);

  const score = document.createElement("td");
  score.className = "numeric";
  const scoreValue = concordanceCollocationScore(row, measure);
  const value = document.createElement("span");
  value.className = "concordance-collocation-score-value";
  value.textContent = formatConcordanceCollocationScore(scoreValue);
  score.appendChild(value);
  const barPercent = concordanceCollocationScoreBarPercent(scoreValue, measure, visibleScores);
  if (barPercent !== null) {
    const track = document.createElement("span");
    track.className = "concordance-collocation-score-track";
    const fill = document.createElement("span");
    fill.className = "concordance-collocation-score-fill";
    fill.style.setProperty("--bar-width", `${barPercent}%`);
    track.title = `${concordanceCollocationMeasureLabel(measure)}: ${formatConcordanceCollocationScore(scoreValue)}`;
    track.appendChild(fill);
    score.appendChild(track);
  }

  item.append(surface, o11, frequency, score);
  return item;
}

export function renderConcordanceCollocations(): void {
  syncConcordanceControls();
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  const measure = concordanceCollocationMeasure;
  const span = currentConcordanceCollocationSpan();
  const direction = currentConcordanceCollocationDirection();
  renderConcordanceCollocationTableHead(measure);
  dom.concordanceCollocationsSummary.classList.remove("hidden");

  if (query.length === 0) {
    dom.concordanceCollocationsSummary.textContent = CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE;
    renderConcordanceCollocationStatus(null);
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE);
    return;
  }

  const explanation = concordanceCollocationExplanation(query, span, direction, measure);
  dom.concordanceCollocationsSummary.textContent = explanation;

  if (concordanceCollocationPending || concordanceCollocationState === "loading") {
    renderConcordanceCollocationStatus("Computing collocates...");
    renderConcordanceCollocationTableState("Loading...");
    return;
  }

  if (concordanceCollocationFailure || concordanceCollocationState === "error") {
    const message = concordanceCollocationFailure ?? "Collocate computation failed.";
    renderConcordanceCollocationStatus(message, true);
    renderConcordanceCollocationTableState(message);
    return;
  }

  if (concordanceCollocationState === "stale") {
    const staleMessage = query.length > 0
      ? `Update the collocate profile for "${query}".`
      : CONCORDANCE_COLLOCATION_STALE_MESSAGE;
    renderConcordanceCollocationStatus(
      renderConcordanceCollocationAction(
        staleMessage,
        concordanceCollocationSupersededNotice,
        "Update profile",
      ),
    );
    renderConcordanceCollocationTableState(staleMessage);
    return;
  }

  const result = concordanceCollocationResult;
  if (
    !result
    || concordanceCollocationState === "idle_not_computed"
    || !currentConcordanceCollocationResultIsFresh()
  ) {
    renderConcordanceCollocationStatus(null);
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE);
    return;
  }

  dom.concordanceCollocationsSummary.textContent =
    `${explanation} · ${formatNumber(result.fNode)} nodes · ${formatNumber(result.r1)} slots · ${formatNumber(result.n)} tokens`;
  renderConcordanceCollocationStatus(null);

  const rows = result.rows;
  if (rows.length === 0) {
    renderConcordanceCollocationTableState(CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE);
    return;
  }

  const visibleScores = rows.map((row) => concordanceCollocationScore(row, measure));
  dom.concordanceCollocationsTableBody.replaceChildren();
  for (const row of rows) {
    const rowElement = createConcordanceCollocationRow(row, measure, visibleScores);
    dom.concordanceCollocationsTableBody.appendChild(rowElement);
    if (isActiveCollocateEvidenceRow(row)) {
      dom.concordanceCollocationsTableBody.appendChild(createCollocateEvidenceDrawerRow(row));
    }
  }
}

async function refreshCollocateEvidence(
  options: { preserveResult?: boolean } = {},
): Promise<void> {
  collocateEvidenceExpansionKey = null;
  collocateEvidenceExpansionPending = false;
  collocateEvidenceExpansionFailure = null;
  collocateEvidenceExpansionResult = null;
  collocateEvidenceExpansionGeneration += 1;

  const selection = collocateEvidenceSelection;
  if (selection === null) {
    collocateEvidenceResult = null;
    collocateEvidenceFailure = null;
    collocateEvidencePending = false;
    renderConcordanceCollocations();
    return;
  }

  const generation = ++collocateEvidenceGeneration;
  collocateEvidencePending = true;
  collocateEvidenceFailure = null;
  if (!options.preserveResult) {
    collocateEvidenceResult = null;
  }
  renderConcordanceCollocations();

  try {
    const result = await searchConcordance(
      {
        selection: currentFrequencyDocumentScope(),
        query: selection.query,
        contextFilter: selection.surface,
        contextFilterDirection: selection.direction,
        caseSensitive: selection.caseSensitive,
        sortKey: "filePosition",
        windowTokens: selection.span,
        offset: 0,
        limit: collocateEvidenceLimit,
        includeRows: true,
        distributionOffset: 0,
        distributionLimit: 1,
        includeDistribution: false,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (generation !== collocateEvidenceGeneration) {
      return;
    }
    collocateEvidenceResult = result;
  } catch (error) {
    if (generation !== collocateEvidenceGeneration) {
      return;
    }
    collocateEvidenceFailure = `${error}`;
  } finally {
    if (generation === collocateEvidenceGeneration) {
      collocateEvidencePending = false;
      renderConcordanceCollocations();
    }
  }
}

export async function refreshConcordanceCollocations(): Promise<void> {
  const query = normaliseConcordanceQuery(dom.concordanceQueryInput.value);
  concordanceCollocationMeasure = currentConcordanceCollocationMeasure();
  const aggregationKey = currentConcordanceCollocationAggregationKey();
  const requestKey = currentConcordanceCollocationRequestKey();
  if (query.length === 0 || aggregationKey === null || requestKey === null) {
    concordanceCollocationState = "idle_not_computed";
    concordanceCollocationResult = null;
    concordanceCollocationFailure = null;
    concordanceCollocationPending = false;
    concordanceCollocationFreshAggregationKey = null;
    concordanceCollocationInFlightRequestKey = null;
    renderConcordanceCollocations();
    return;
  }
  if (concordanceCollocationPending && concordanceCollocationInFlightRequestKey === requestKey) {
    return;
  }

  const generation = ++concordanceCollocationGeneration;
  concordanceCollocationPending = true;
  concordanceCollocationState = "loading";
  concordanceCollocationFailure = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationResult = null;
  concordanceCollocationInFlightRequestKey = requestKey;
  renderConcordanceCollocations();

  try {
    const result = await queryConcordanceCollocations(
      {
        selection: currentFrequencyDocumentScope(),
        query,
        caseSensitive: dom.concordanceCaseSensitiveInput.checked,
        span: currentConcordanceCollocationSpan(),
        direction: currentConcordanceCollocationDirection(),
        minCooccurrence: currentConcordanceCollocationMinO11(),
        minCollocateFrequency: currentConcordanceCollocationMinFrequency(),
        hideStopwords: dom.concordanceCollocationHideStopwordsInput.checked,
        language: activeProjectLanguageCode(),
        sortMeasure: concordanceCollocationMeasure,
        sortKey: concordanceCollocationSortKey,
        sortDirection: currentConcordanceCollocationSortDirection(),
        limit: CONCORDANCE_COLLOCATION_LIMIT,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    if (
      generation !== concordanceCollocationGeneration
      || requestKey !== currentConcordanceCollocationRequestKey()
      || aggregationKey !== currentConcordanceCollocationAggregationKey()
    ) {
      noteSupersededConcordanceCollocationResult();
      return;
    }
    concordanceCollocationResult = result;
    concordanceCollocationFreshAggregationKey = aggregationKey;
    concordanceCollocationEverComputed = true;
    concordanceCollocationAutoComputedAggregationKeys.add(aggregationKey);
    concordanceCollocationState = "ready";
  } catch (error) {
    if (generation !== concordanceCollocationGeneration) {
      noteSupersededConcordanceCollocationResult();
      return;
    }
    concordanceCollocationState = "error";
    concordanceCollocationFailure = `${error}`;
  } finally {
    if (generation === concordanceCollocationGeneration) {
      concordanceCollocationPending = false;
      concordanceCollocationInFlightRequestKey = null;
      renderConcordanceCollocations();
    }
  }
}

export async function runConcordanceSearchForActiveView(): Promise<void> {
  const viewAtSearch = activeConcordanceViewId();
  const queryAtSearch = normaliseConcordanceQuery(dom.concordanceQueryInput.value);

  clearCollocateEvidence();
  markConcordanceCollocationsForAggregationChange();
  if (viewAtSearch === "collocates" && queryAtSearch.length > 0) {
    concordanceCollocationPending = true;
    concordanceCollocationState = "loading";
    concordanceCollocationFailure = null;
    concordanceCollocationResult = null;
    renderConcordanceCollocations();
  }
  await refreshConcordance();

  if (
    viewAtSearch === "collocates"
    && activeConcordanceViewId() === "collocates"
    && queryAtSearch.length > 0
    && normaliseConcordanceQuery(dom.concordanceQueryInput.value) === queryAtSearch
    && currentConcordanceSearchResult() !== null
    && currentConcordanceSearchFailure() === null
  ) {
    await refreshConcordanceCollocations();
  }
}

export function handleConcordanceCollocationMeasureChange(): void {
  concordanceCollocationMeasure = dom.concordanceCollocationMeasureSelect.value as ConcordanceCollocationMeasure;
  refreshConcordanceCollocationsForPresentationChange();
}

export function handleConcordanceCollocationSortClick(event: Event): void {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(".concordance-collocation-sort");
  if (!button) {
    return;
  }
  const nextKey = button.dataset.collocationSort as ConcordanceCollocationSortKey | undefined;
  if (!nextKey) {
    return;
  }
  if (nextKey === concordanceCollocationSortKey) {
    concordanceCollocationSortDirection = concordanceCollocationSortDirection === "asc" ? "desc" : "asc";
  } else {
    concordanceCollocationSortKey = nextKey;
    concordanceCollocationSortDirection = "desc";
  }
  refreshConcordanceCollocationsForPresentationChange();
}

export function handleConcordanceCollocationAggregationControlChange(): void {
  markConcordanceCollocationsForAggregationChange();
}

export function handleConcordanceCollocationPresentationControlChange(): void {
  clearCollocateEvidence();
  refreshConcordanceCollocationsForPresentationChange();
}

export function resetConcordanceCollocationCorpusState(): void {
  concordanceCollocationState = "idle_not_computed";
  concordanceCollocationResult = null;
  concordanceCollocationGeneration += 1;
  concordanceCollocationPending = false;
  concordanceCollocationFailure = null;
  concordanceCollocationFreshAggregationKey = null;
  concordanceCollocationEverComputed = false;
  concordanceCollocationAutoComputedAggregationKeys = new Set<string>();
  concordanceCollocationInFlightRequestKey = null;
  concordanceCollocationSupersededNotice = null;
  concordanceCollocationMeasure = DEFAULT_CONCORDANCE_COLLOCATION_MEASURE;
  concordanceCollocationSortKey = DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY;
  concordanceCollocationSortDirection = DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION;
  clearCollocateEvidence();
}
