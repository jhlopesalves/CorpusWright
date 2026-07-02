import type { ConcordanceCollocateRow } from "./generated/ConcordanceCollocateRow.js";
import type { ConcordanceCollocationDirection } from "./generated/ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure } from "./generated/ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationSortKey } from "./generated/ConcordanceCollocationSortKey.js";
import type { ConcordanceExpansionResult } from "./generated/ConcordanceExpansionResult.js";
import type { ConcordanceRow } from "./generated/ConcordanceRow.js";
import type { ConcordanceSortKey } from "./generated/ConcordanceSortKey.js";
import type { SortDirection } from "./generated/SortDirection.js";

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
export const CONCORDANCE_COLLOCATION_MIN_SPAN = 1;
export const CONCORDANCE_COLLOCATION_DEFAULT_SPAN = 5;
export const CONCORDANCE_COLLOCATION_MAX_SPAN = 20;
export const CONCORDANCE_COLLOCATION_DEFAULT_FLOOR = 5;
export const CONCORDANCE_COLLOCATION_LIMIT = 100;
export const CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE =
  "Enter a word or phrase to build a collocate profile.";
export const CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE =
  "No collocates above the current floors.";
export const CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE =
  "The collocate profile has not been computed yet.";
export const CONCORDANCE_COLLOCATION_STALE_MESSAGE =
  "The collocate profile is out of date for the current query.";
export const CONCORDANCE_COLLOCATION_STALE_DETAIL =
  "Update collocate profile for the current query.";
export const DEFAULT_CONCORDANCE_COLLOCATION_MEASURE: ConcordanceCollocationMeasure = "logDice";
export const DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY: ConcordanceCollocationSortKey = "association";
export const DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION: SortDirection = "desc";

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

export const CONCORDANCE_COLLOCATION_MEASURE_OPTIONS: Array<{
  value: ConcordanceCollocationMeasure;
  label: string;
}> = [
  { value: "logDice", label: "logDice" },
  { value: "mi", label: "MI" },
  { value: "mi3", label: "MI3" },
  { value: "tScore", label: "t-score" },
  { value: "zScore", label: "z-score" },
  { value: "g2", label: "log-likelihood" },
  { value: "logRatio", label: "log-ratio" },
];

export const CONCORDANCE_COLLOCATION_SORT_OPTIONS: Array<{
  value: ConcordanceCollocationSortKey;
  label: string;
  notation: string;
  title: string;
}> = [
  {
    value: "nearQuery",
    label: "Near query",
    notation: "O11",
    title: "Co-occurrences within the selected span; O11",
  },
  {
    value: "corpusFrequency",
    label: "Corpus frequency",
    notation: "f_c",
    title: "Total frequency of the collocate in scope; f_c",
  },
  {
    value: "association",
    label: "Association",
    notation: "",
    title: "Association score for the active measure",
  },
];

export function normaliseConcordanceQuery(value: string): string {
  return value.trim();
}

export function concordanceRowText(tokens: string[]): string {
  return tokens.join(" ");
}

export interface ConcordanceContextTokenSegment {
  text: string;
  collocate: boolean;
}

export function concordanceContextTokenSegments(
  tokens: string[],
  activeFilter: string | null,
  caseSensitive: boolean,
): ConcordanceContextTokenSegment[] {
  const comparisonFilter = caseSensitive || activeFilter === null
    ? activeFilter
    : activeFilter.toLowerCase();
  return tokens.map((token) => {
    const comparisonToken = caseSensitive ? token : token.toLowerCase();
    return {
      text: token,
      collocate: comparisonFilter !== null && comparisonToken === comparisonFilter,
    };
  });
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

export function concordanceCollocationMeasureLabel(
  measure: ConcordanceCollocationMeasure,
): string {
  return CONCORDANCE_COLLOCATION_MEASURE_OPTIONS.find((option) => option.value === measure)?.label ?? "logDice";
}

export function concordanceCollocationDirectionLabel(
  direction: ConcordanceCollocationDirection,
): string {
  switch (direction) {
    case "left":
      return "left";
    case "right":
      return "right";
    case "both":
      return "both sides";
  }
}

export function concordanceCollocationExplanation(
  query: string,
  span: number,
  direction: ConcordanceCollocationDirection,
  measure: ConcordanceCollocationMeasure,
): string {
  const directionText = concordanceCollocationDirectionLabel(direction);
  return `Collocates near "${query}" within ${span.toLocaleString()} tokens (${directionText}), ranked by Association: ${concordanceCollocationMeasureLabel(measure)}. Near query = times the word appears around the query · Corpus frequency = total frequency in the selected corpus.`;
}

export function clampConcordanceCollocationSpan(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return CONCORDANCE_COLLOCATION_DEFAULT_SPAN;
  }
  return Math.min(
    CONCORDANCE_COLLOCATION_MAX_SPAN,
    Math.max(CONCORDANCE_COLLOCATION_MIN_SPAN, Math.trunc(value)),
  );
}

export function clampConcordanceCollocationFloor(value: number): number {
  if (!Number.isFinite(value) || value < 0) {
    return CONCORDANCE_COLLOCATION_DEFAULT_FLOOR;
  }
  return Math.trunc(value);
}

export function concordanceRangePercent(result: {
  documentsWithHits: number;
  selectedDocumentsTotal: number;
}): number {
  if (result.selectedDocumentsTotal <= 0) {
    return 0;
  }
  return result.documentsWithHits / result.selectedDocumentsTotal * 100;
}

export function concordanceIsSparse(result: {
  totalHits: number;
  documentsWithHits: number;
  selectedDocumentsTotal: number;
}): boolean {
  return result.totalHits < 20
    || result.documentsWithHits < 10
    || concordanceRangePercent(result) < 1.0;
}

export function concordanceSummaryText(result: {
  query: string;
  totalHits: number;
  documentsWithHits: number;
  selectedDocumentsTotal: number;
  topDocumentShare: number;
  dp: number;
}): string {
  return `${result.query} · ${concordanceSummaryMetricsText(result)}`;
}

export function concordanceSummaryMetricsText(result: {
  totalHits: number;
  documentsWithHits: number;
  selectedDocumentsTotal: number;
  topDocumentShare: number;
  dp: number;
}): string {
  const rangePct = concordanceRangePercent(result).toFixed(1);
  const topFilePct = (result.topDocumentShare * 100).toFixed(1);
  const sparse = concordanceIsSparse(result) ? " · Sparse" : "";
  return `${result.totalHits.toLocaleString()} hits · ${result.documentsWithHits.toLocaleString()} / ${result.selectedDocumentsTotal.toLocaleString()} files · range ${rangePct}% · top file ${topFilePct}% · DP ${result.dp.toFixed(2)}${sparse}`;
}

export function concordanceCollocationScore(
  row: ConcordanceCollocateRow,
  measure: ConcordanceCollocationMeasure,
): number | null {
  switch (measure) {
    case "mi":
      return row.mi;
    case "mi3":
      return row.mi3;
    case "tScore":
      return row.tScore;
    case "zScore":
      return row.zScore;
    case "g2":
      return row.g2;
    case "logDice":
      return row.logDice;
    case "logRatio":
      return row.logRatio;
  }
}

export function concordanceCollocationSortHeaderLabel(
  sortKey: ConcordanceCollocationSortKey,
  measure: ConcordanceCollocationMeasure,
): string {
  if (sortKey === "association") {
    return `Association: ${concordanceCollocationMeasureLabel(measure)}`;
  }
  return CONCORDANCE_COLLOCATION_SORT_OPTIONS.find((option) => option.value === sortKey)?.label
    ?? "Association";
}

export function concordanceCollocationScoreBarPercent(
  value: number | null,
  measure: ConcordanceCollocationMeasure,
  visibleScores: Array<number | null> = [],
): number | null {
  if (value === null || !Number.isFinite(value)) {
    return null;
  }
  if (measure === "logDice") {
    return Math.min(100, Math.max(0, value / 14 * 100));
  }

  const finiteScores = visibleScores.filter((score): score is number =>
    score !== null && Number.isFinite(score)
  );
  if (finiteScores.length === 0) {
    return null;
  }

  const minimum = Math.min(...finiteScores);
  const maximum = Math.max(...finiteScores);
  if (maximum === minimum) {
    return 100;
  }
  return Math.min(100, Math.max(0, (value - minimum) / (maximum - minimum) * 100));
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
