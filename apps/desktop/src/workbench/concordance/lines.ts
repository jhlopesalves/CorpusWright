import {
  expandConcordanceContext,
  type ConcordanceExpansionResult,
  type ConcordanceResult,
  type ConcordanceRow,
  type ConcordanceSortKey,
} from "../../analysis";
import {
  clampConcordancePageSize,
  clampConcordanceWindow,
  concordanceExpansionViewModel,
  concordanceRowExpansionKey,
  concordanceRowText,
  CONCORDANCE_COLUMNS,
  CONCORDANCE_EXPANSION_WINDOW,
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  DEFAULT_CONCORDANCE_SORT_KEY,
  toggleConcordanceExpansionKey,
  type ConcordanceExpansionStateSnapshot,
} from "../../concordance";
import { dom } from "../../dom";
import { effectiveCleaningConfig } from "../../effective-cleaning-config";
import { state } from "../../state";
import { formatNumber } from "../shell";
import {
  currentConcordanceSearchResult,
  refreshConcordance,
  renderConcordance,
  renderContextTokens,
} from "./shared";

let concordanceOffset = 0;
let concordanceSortKey: ConcordanceSortKey = DEFAULT_CONCORDANCE_SORT_KEY;
let concordanceExpansionKey: string | null = null;
let concordanceExpansionPending = false;
let concordanceExpansionFailure: string | null = null;
let concordanceExpansionResult: ConcordanceExpansionResult | null = null;
let concordanceExpansionGeneration = 0;

function currentConcordanceWindow(): number {
  return clampConcordanceWindow(Number.parseInt(dom.concordanceWindowInput.value, 10));
}

function currentConcordancePageSize(): number {
  const pageSize = clampConcordancePageSize(Number.parseInt(dom.concordancePageSizeInput.value, 10));
  dom.concordancePageSizeInput.value = pageSize.toString();
  return pageSize;
}

function currentConcordanceSortKey(): ConcordanceSortKey {
  return dom.concordanceSortSelect.value as ConcordanceSortKey;
}

export function currentConcordanceLineOffset(): number {
  return concordanceOffset;
}

export function currentConcordanceLineSortKey(): ConcordanceSortKey {
  return concordanceSortKey;
}

export function currentConcordanceLineWindow(): number {
  return currentConcordanceWindow();
}

export function currentConcordanceLinePageSize(): number {
  return currentConcordancePageSize();
}

export function resetConcordanceLineOffset(): void {
  concordanceOffset = 0;
}

export function setConcordanceLineOffset(offset: number): void {
  concordanceOffset = offset;
}

export function syncConcordanceLineSortKeyFromControl(): ConcordanceSortKey {
  concordanceSortKey = currentConcordanceSortKey();
  return concordanceSortKey;
}

export function resetConcordanceLinesCorpusState(): void {
  concordanceOffset = 0;
  concordanceSortKey = DEFAULT_CONCORDANCE_SORT_KEY;
  concordanceExpansionKey = null;
  concordanceExpansionPending = false;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  concordanceExpansionGeneration += 1;
}

function currentConcordanceExpansionState(): ConcordanceExpansionStateSnapshot {
  return {
    key: concordanceExpansionKey,
    pending: concordanceExpansionPending,
    error: concordanceExpansionFailure,
    result: concordanceExpansionResult,
  };
}

export function collapseConcordanceExpansion(): void {
  if (
    concordanceExpansionKey === null
    && !concordanceExpansionPending
    && concordanceExpansionFailure === null
    && concordanceExpansionResult === null
  ) {
    return;
  }
  concordanceExpansionKey = null;
  concordanceExpansionPending = false;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  concordanceExpansionGeneration += 1;
}

export function handleConcordanceExpansionEscape(event: KeyboardEvent): void {
  if (event.key !== "Escape" || concordanceExpansionKey === null) {
    return;
  }
  collapseConcordanceExpansion();
  renderConcordance();
}

function concordanceExpansionDomId(rowKey: string): string {
  return `concordance-expansion-${rowKey.replace(/[^a-z0-9_-]/gi, "-")}`;
}

async function toggleConcordanceExpansion(row: ConcordanceRow): Promise<void> {
  const rowKey = concordanceRowExpansionKey(row);
  const nextKey = toggleConcordanceExpansionKey(concordanceExpansionKey, rowKey);
  if (nextKey === null) {
    collapseConcordanceExpansion();
    renderConcordance();
    return;
  }

  concordanceExpansionKey = nextKey;
  concordanceExpansionPending = true;
  concordanceExpansionFailure = null;
  concordanceExpansionResult = null;
  const generation = ++concordanceExpansionGeneration;
  renderConcordance();

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
    if (generation !== concordanceExpansionGeneration) {
      return;
    }
    concordanceExpansionResult = result;
  } catch (error) {
    if (generation !== concordanceExpansionGeneration) {
      return;
    }
    concordanceExpansionFailure = `${error}`;
  } finally {
    if (generation === concordanceExpansionGeneration) {
      concordanceExpansionPending = false;
      renderConcordance();
    }
  }
}

function renderConcordanceTableState(message: string): void {
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = CONCORDANCE_COLUMNS.length;
  cell.className = "frequency-empty concordance-empty";
  cell.textContent = message;
  row.appendChild(cell);
  dom.concordanceTableBody.replaceChildren(row);
}

function appendConcordanceKwicCells(
  item: HTMLTableRowElement,
  row: ConcordanceRow,
  activeFilter: string | null,
  caseSensitive: boolean,
): void {
  const file = document.createElement("td");
  file.className = "concordance-file";
  file.title = row.displayName || row.sourcePath;
  file.textContent = row.displayName || row.sourcePath;

  const left = document.createElement("td");
  left.className = "concordance-left";
  if (activeFilter === null) {
    left.textContent = concordanceRowText(row.leftContext);
  } else {
    left.appendChild(renderContextTokens(row.leftContext, activeFilter, caseSensitive));
  }

  const nodeCell = document.createElement("td");
  nodeCell.className = "concordance-node-cell";
  const node = document.createElement("mark");
  node.className = "concordance-node";
  node.textContent = row.node;
  nodeCell.appendChild(node);

  const right = document.createElement("td");
  right.className = "concordance-right";
  if (activeFilter === null) {
    right.textContent = concordanceRowText(row.rightContext);
  } else {
    right.appendChild(renderContextTokens(row.rightContext, activeFilter, caseSensitive));
  }

  item.append(file, left, nodeCell, right);
}

function createConcordanceRow(row: ConcordanceRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  const rowKey = concordanceRowExpansionKey(row);
  const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());
  const concordanceResult = currentConcordanceSearchResult();
  const activeContextFilter = concordanceResult?.contextFilter ?? null;
  const contextFilterCaseSensitive = concordanceResult?.caseSensitive ?? false;
  item.className = "concordance-row";
  item.tabIndex = 0;
  item.setAttribute("role", "button");
  item.setAttribute("aria-expanded", expansion.ariaExpanded);
  item.setAttribute("aria-controls", concordanceExpansionDomId(rowKey));
  item.title = "Show wider context";
  item.addEventListener("click", () => {
    void toggleConcordanceExpansion(row);
  });
  item.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") {
      return;
    }
    event.preventDefault();
    void toggleConcordanceExpansion(row);
  });
  item.classList.toggle("is-expanded", expansion.expanded);

  appendConcordanceKwicCells(item, row, activeContextFilter, contextFilterCaseSensitive);
  return item;
}

function createConcordanceSourcePreviewPanel(row: ConcordanceRow): HTMLDivElement {
  const rowKey = concordanceRowExpansionKey(row);
  const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());

  const panel = document.createElement("div");
  panel.id = concordanceExpansionDomId(rowKey);
  panel.className = "concordance-expansion-panel";

  const header = document.createElement("div");
  header.className = "concordance-expansion-header";
  const file = document.createElement("span");
  file.className = "concordance-expansion-file";
  file.textContent = row.displayName || row.sourcePath;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "secondary-btn concordance-expansion-close";
  close.textContent = "Close";
  close.addEventListener("click", () => {
    collapseConcordanceExpansion();
    renderConcordance();
  });
  header.append(file, close);
  panel.appendChild(header);

  if (expansion.status === "ready") {
    const context = document.createElement("p");
    context.className = "concordance-expansion-context";
    const left = document.createElement("span");
    left.className = "concordance-expansion-left";
    const concordanceResult = currentConcordanceSearchResult();
    const activeFilter = concordanceResult?.contextFilter ?? null;
    const caseSensitive = concordanceResult?.caseSensitive ?? false;
    if (concordanceExpansionResult !== null) {
      left.appendChild(renderContextTokens(concordanceExpansionResult.leftContext, activeFilter, caseSensitive));
    } else {
      left.textContent = expansion.leftText;
    }
    const node = document.createElement("mark");
    node.className = "concordance-node concordance-expansion-node";
    node.textContent = expansion.nodeText;
    const right = document.createElement("span");
    right.className = "concordance-expansion-right";
    if (concordanceExpansionResult !== null) {
      right.appendChild(renderContextTokens(concordanceExpansionResult.rightContext, activeFilter, caseSensitive));
    } else {
      right.textContent = expansion.rightText;
    }
    context.append(left, node, right);
    panel.appendChild(context);
  } else {
    const message = document.createElement("p");
    message.className = `concordance-expansion-message is-${expansion.status}`;
    message.textContent = expansion.message;
    panel.appendChild(message);
  }

  return panel;
}

function createConcordanceSourcePreviewRow(row: ConcordanceRow): HTMLTableRowElement {
  const item = document.createElement("tr");
  item.className = "concordance-preview-row";

  const cell = document.createElement("td");
  cell.className = "concordance-preview-cell";
  cell.colSpan = CONCORDANCE_COLUMNS.length;
  cell.appendChild(createConcordanceSourcePreviewPanel(row));

  item.appendChild(cell);
  return item;
}

export function renderConcordanceLines(
  result: ConcordanceResult | null,
  emptyMessage = CONCORDANCE_NO_QUERY_MESSAGE,
): void {
  if (!result) {
    renderConcordanceTableState(emptyMessage);
    dom.concordancePagePreviousButton.disabled = true;
    dom.concordancePageNextButton.disabled = true;
    dom.concordancePageStatus.textContent = "";
    return;
  }
  const rows = result.rows;
  if (rows === null) {
    renderConcordanceTableState("Concordance lines were not included in this response.");
    dom.concordancePagePreviousButton.disabled = true;
    dom.concordancePageNextButton.disabled = true;
    dom.concordancePageStatus.textContent = "";
    return;
  }

  if (rows.length === 0) {
    renderConcordanceTableState(CONCORDANCE_NO_HITS_MESSAGE);
  } else {
    dom.concordanceTableBody.replaceChildren();
    for (const row of rows) {
      const rowElement = createConcordanceRow(row);
      dom.concordanceTableBody.appendChild(rowElement);
      const rowKey = concordanceRowExpansionKey(row);
      const expansion = concordanceExpansionViewModel(rowKey, currentConcordanceExpansionState());
      if (expansion.expanded) {
        dom.concordanceTableBody.appendChild(createConcordanceSourcePreviewRow(row));
      }
    }
  }

  const first = result.totalHits === 0 ? 0 : result.offset + 1;
  const last = Math.min(result.totalHits, result.offset + rows.length);
  dom.concordancePageStatus.textContent =
    result.totalHits === 0 ? "0 of 0" : `${formatNumber(first)}-${formatNumber(last)} of ${formatNumber(result.totalHits)}`;
  dom.concordancePagePreviousButton.disabled = result.offset === 0;
  dom.concordancePageNextButton.disabled = result.offset + rows.length >= result.totalHits;
}

export function showPreviousConcordancePage(): void {
  const concordanceResult = currentConcordanceSearchResult();
  const pageSize = concordanceResult?.limit ?? currentConcordancePageSize();
  concordanceOffset = Math.max(0, concordanceOffset - pageSize);
  void refreshConcordance(false, false);
}

export function showNextConcordancePage(): void {
  const concordanceResult = currentConcordanceSearchResult();
  const pageSize = concordanceResult?.limit ?? currentConcordancePageSize();
  concordanceOffset += pageSize;
  void refreshConcordance(false, false);
}

export function handleConcordanceSortChange(): void {
  concordanceSortKey = currentConcordanceSortKey();
  void refreshConcordance(true, true);
}

export function handleConcordancePageSizeChange(): void {
  currentConcordancePageSize();
  void refreshConcordance(true, true);
}

export function collapseConcordanceExpansionFromControlChange(): void {
  collapseConcordanceExpansion();
  renderConcordance();
}
