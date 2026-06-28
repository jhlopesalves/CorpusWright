import type { ConcordanceExpansionResult } from "./generated/ConcordanceExpansionResult.js";
import type { ConcordanceRow } from "./generated/ConcordanceRow.js";
import type { ConcordanceSortKey } from "./generated/ConcordanceSortKey.js";

export const CONCORDANCE_MIN_WINDOW = 1;
export const CONCORDANCE_DEFAULT_WINDOW = 5;
export const CONCORDANCE_MAX_WINDOW = 20;
export const CONCORDANCE_EXPANSION_WINDOW = 40;
export const CONCORDANCE_MIN_PAGE_SIZE = 25;
export const CONCORDANCE_DEFAULT_PAGE_SIZE = 100;
export const CONCORDANCE_MAX_PAGE_SIZE = 500;
export const DEFAULT_CONCORDANCE_SORT_KEY: ConcordanceSortKey = "filePosition";
export const CONCORDANCE_NO_QUERY_MESSAGE = "Enter a word or phrase to see concordance lines.";
export const CONCORDANCE_NO_HITS_MESSAGE =
  "No concordance lines found for this query in the selected files.";
export const CONCORDANCE_COLUMNS = ["File", "Left context", "Node", "Right context"] as const;

export const CONCORDANCE_SORT_OPTIONS: Array<{ value: ConcordanceSortKey; label: string }> = [
  { value: "filePosition", label: "File/position" },
  { value: "node", label: "Node" },
  { value: "left1", label: "1L" },
  { value: "left2", label: "2L" },
  { value: "left3", label: "3L" },
  { value: "right1", label: "1R" },
  { value: "right2", label: "2R" },
  { value: "right3", label: "3R" },
];

export function normaliseConcordanceQuery(value: string): string {
  return value.trim();
}

export function concordanceRowText(tokens: string[]): string {
  return tokens.join(" ");
}

export function clampConcordanceWindow(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return CONCORDANCE_DEFAULT_WINDOW;
  }
  return Math.min(CONCORDANCE_MAX_WINDOW, Math.max(CONCORDANCE_MIN_WINDOW, Math.trunc(value)));
}

export function clampConcordancePageSize(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return CONCORDANCE_DEFAULT_PAGE_SIZE;
  }
  return Math.min(
    CONCORDANCE_MAX_PAGE_SIZE,
    Math.max(CONCORDANCE_MIN_PAGE_SIZE, Math.trunc(value)),
  );
}

export function concordanceSortLabel(sortKey: ConcordanceSortKey): string {
  return CONCORDANCE_SORT_OPTIONS.find((option) => option.value === sortKey)?.label ?? "File/position";
}

export function concordanceSummaryText(result: {
  query: string;
  totalHits: number;
  documentsWithHits: number;
  dp: number;
}): string {
  return `${result.query} · ${result.totalHits.toLocaleString()} hits · ${result.documentsWithHits.toLocaleString()} files with hits · DP ${result.dp.toFixed(2)}`;
}

export interface ConcordanceExpansionStateSnapshot {
  key: string | null;
  pending: boolean;
  error: string | null;
  result: ConcordanceExpansionResult | null;
}

export interface ConcordanceExpansionViewModel {
  expanded: boolean;
  ariaExpanded: "true" | "false";
  status: "collapsed" | "loading" | "error" | "ready";
  message: string;
  leftText: string;
  nodeText: string;
  rightText: string;
}

export function concordanceRowExpansionKey(
  row: Pick<ConcordanceRow, "documentIndex" | "tokenIndex" | "nodeTokenLength">,
): string {
  return `${row.documentIndex}:${row.tokenIndex}:${row.nodeTokenLength}`;
}

export function toggleConcordanceExpansionKey(
  currentKey: string | null,
  rowKey: string,
): string | null {
  return currentKey === rowKey ? null : rowKey;
}

export function concordanceExpansionViewModel(
  rowKey: string,
  state: ConcordanceExpansionStateSnapshot,
): ConcordanceExpansionViewModel {
  if (state.key !== rowKey) {
    return {
      expanded: false,
      ariaExpanded: "false",
      status: "collapsed",
      message: "",
      leftText: "",
      nodeText: "",
      rightText: "",
    };
  }
  if (state.pending) {
    return {
      expanded: true,
      ariaExpanded: "true",
      status: "loading",
      message: "Loading wider context...",
      leftText: "",
      nodeText: "",
      rightText: "",
    };
  }
  if (state.error) {
    return {
      expanded: true,
      ariaExpanded: "true",
      status: "error",
      message: state.error,
      leftText: "",
      nodeText: "",
      rightText: "",
    };
  }
  if (state.result) {
    return {
      expanded: true,
      ariaExpanded: "true",
      status: "ready",
      message: "",
      leftText: concordanceRowText(state.result.leftContext),
      nodeText: state.result.nodeTokens.join(" "),
      rightText: concordanceRowText(state.result.rightContext),
    };
  }
  return {
    expanded: true,
    ariaExpanded: "true",
    status: "loading",
    message: "Loading wider context...",
    leftText: "",
    nodeText: "",
    rightText: "",
  };
}
