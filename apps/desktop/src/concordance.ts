import type { ConcordanceCollocateRow } from "./generated/ConcordanceCollocateRow.js";
import type { ConcordanceCollocationDirection } from "./generated/ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure } from "./generated/ConcordanceCollocationMeasure.js";
import type { ConcordanceCollocationSortKey } from "./generated/ConcordanceCollocationSortKey.js";
import type { ConcordanceExpansionResult } from "./generated/ConcordanceExpansionResult.js";
import type { ConcordanceProgress } from "./generated/ConcordanceProgress.js";
import type { ConcordanceRow } from "./generated/ConcordanceRow.js";
import type { ConcordanceSortKey } from "./generated/ConcordanceSortKey.js";
import type { SortDirection } from "./generated/SortDirection.js";

export const CONCORDANCE_MIN_WINDOW = 1;
export const CONCORDANCE_DEFAULT_WINDOW = 10;
export const CONCORDANCE_MAX_WINDOW = 20;
export const CONCORDANCE_EXPANSION_WINDOW = 40;
export const CONCORDANCE_MIN_PAGE_SIZE = 1;
export const CONCORDANCE_DEFAULT_PAGE_SIZE = 24;
export const CONCORDANCE_MAX_PAGE_SIZE = 500;
export const DEFAULT_CONCORDANCE_SORT_KEYS: ConcordanceSortKey[] = ["right1", "right2", "right3"];
export const CONCORDANCE_NO_QUERY_MESSAGE = "Enter a word or phrase to see concordance lines.";
export const CONCORDANCE_NO_HITS_MESSAGE =
  "No concordance lines found for this query in the selected files.";
export const CONCORDANCE_COLUMNS = ["File", "Left context", "Node", "Right context"] as const;
export const CONCORDANCE_COLLOCATION_MIN_SPAN = 1;
export const CONCORDANCE_COLLOCATION_DEFAULT_SPAN = 5;
export const CONCORDANCE_COLLOCATION_MAX_SPAN = 20;
export const CONCORDANCE_COLLOCATION_DEFAULT_FLOOR = 5;
export const CONCORDANCE_COLLOCATION_LIMIT = 500;
export const CONCORDANCE_COLLOCATION_INITIAL_ROWS = 10;
export const CONCORDANCE_COLLOCATION_SHOW_MORE_STEP = 25;
export const CONCORDANCE_COLLOCATION_NO_QUERY_MESSAGE =
  "Enter a word or phrase to build a collocate profile.";
export const CONCORDANCE_COLLOCATION_NO_ROWS_MESSAGE =
  "No collocates above the current floors.";
export const CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE =
  "The collocate profile has not been computed yet.";
export const CONCORDANCE_COLLOCATION_STALE_MESSAGE =
  "Update the collocate profile for the current query.";
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

export function formatConcordanceProgress(progress: ConcordanceProgress | null): string {
  if (!progress) {
    return "Searching...";
  }
  const percent = progress.filesTotal > 0 ? Math.round((progress.filesDone / progress.filesTotal) * 100) : 0;
  let phaseText = "Searching";
  if (progress.currentPhase === "buildingIndex") {
    phaseText = "Reading index";
  } else if (progress.currentPhase === "searching") {
    phaseText = "Searching";
  } else if (progress.currentPhase === "aggregatingCollocations") {
    phaseText = "Aggregating collocates";
  }
  return `${phaseText}... ${percent}%`;
}

export function concordanceRowText(tokens: string[]): string {
  return tokens.join(" ");
}

export function concordanceCollocateEvidenceDomId(surface: string): string {
  const codePoints = Array.from(surface, (character) =>
    character.codePointAt(0)!.toString(16).padStart(6, "0")
  );
  return `concordance-collocate-evidence-${codePoints.join("-") || "empty"}`;
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

function formatConcordanceCount(value: number): string {
  return value.toLocaleString();
}

export function concordanceLinesCardStatsText(options: {
  windowTokens: number;
  sortKeys: ConcordanceSortKey[];
  shown: number;
  totalHits: number;
  contextFilter: string | null;
}): string {
  const sortLabels = options.sortKeys.map(concordanceSortLabel);
  const sortText = sortLabels.length === 1
    ? `${sortLabels[0]![0]!.toLowerCase()}${sortLabels[0]!.slice(1)}`
    : sortLabels.join(" / ");
  const parts = [
    `window ${options.windowTokens}L–${options.windowTokens}R`,
    sortText,
  ];
  if (options.contextFilter !== null) parts.push(`context “${options.contextFilter}”`);
  parts.push(`showing ${formatConcordanceCount(options.shown)} of ${formatConcordanceCount(options.totalHits)}`);
  return parts.join(" · ");
}

export function concordanceCollocatesCardStatsText(options: {
  span: number;
  direction: ConcordanceCollocationDirection;
  measure: ConcordanceCollocationMeasure;
  minimumFrequency: number;
  shown: number;
  loadedRows: number;
  totalCandidates: number;
}): string {
  const spanText = options.direction === "left"
    ? `${options.span}L`
    : options.direction === "right"
      ? `${options.span}R`
      : `${options.span}L–${options.span}R`;
  const parts = [
    `span ${spanText}`,
    concordanceCollocationMeasureLabel(options.measure),
    `min freq ${formatConcordanceCount(options.minimumFrequency)}`,
  ];
  if (options.totalCandidates > options.loadedRows) {
    parts.push(
      `showing ${formatConcordanceCount(options.shown)} of ${formatConcordanceCount(options.loadedRows)} available rows`,
      `${formatConcordanceCount(options.totalCandidates)} qualifying collocates`,
    );
  } else {
    parts.push(`showing ${formatConcordanceCount(options.shown)} of ${formatConcordanceCount(options.totalCandidates)}`);
  }
  return parts.join(" · ");
}

export function concordanceDistributionCardStatsText(options: {
  documentsWithHits: number;
  selectedDocumentsTotal: number;
  shown: number;
  totalDocuments: number;
}): string {
  return `${formatConcordanceCount(options.documentsWithHits)} of ${formatConcordanceCount(options.selectedDocumentsTotal)} documents with hits · showing ${formatConcordanceCount(options.shown)} of ${formatConcordanceCount(options.totalDocuments)}`;
}

export type ConcordanceSortLevel = 1 | 2 | 3 | null;
export type ConcordanceContextSortKey = Exclude<ConcordanceSortKey, "filePosition" | "node">;

export interface ConcordanceContextSortLevels {
  left: ConcordanceSortLevel[];
  right: ConcordanceSortLevel[];
}

export function concordanceSortSequence(
  primary: ConcordanceSortKey,
  secondary: ConcordanceContextSortKey | "" = "",
  tertiary: ConcordanceContextSortKey | "" = "",
): ConcordanceSortKey[] {
  if (primary === "filePosition" || primary === "node") return [primary];
  const ordered: ConcordanceSortKey[] = [];
  for (const key of [primary, secondary, tertiary]) {
    if (key !== "" && !ordered.includes(key)) ordered.push(key);
  }
  return ordered;
}

export function concordanceContextSortLevels(
  sortKeys: ConcordanceSortKey[],
  leftLength: number,
  rightLength: number,
): ConcordanceContextSortLevels {
  const left = Array<ConcordanceSortLevel>(leftLength).fill(null);
  const right = Array<ConcordanceSortLevel>(rightLength).fill(null);
  for (const [index, sortKey] of sortKeys.entries()) {
    const match = /^(left|right)([123])$/.exec(sortKey);
    if (!match || index > 2) continue;
    const distance = Number.parseInt(match[2]!, 10) - 1;
    const level = (index + 1) as 1 | 2 | 3;
    if (match[1] === "left") {
      const tokenIndex = leftLength - distance - 1;
      if (tokenIndex >= 0) left[tokenIndex] = level;
    } else if (distance < rightLength) {
      right[distance] = level;
    }
  }
  return { left, right };
}

export function concordanceMiddleEllipsis(value: string, maximumLength = 28): string {
  if (value.length <= maximumLength || maximumLength < 5) return value;
  const remaining = maximumLength - 1;
  const startLength = Math.ceil(remaining / 2);
  const endLength = Math.floor(remaining / 2);
  return `${value.slice(0, startLength)}…${value.slice(value.length - endLength)}`;
}

export function concordanceCollocationMeasureLabel(
  measure: ConcordanceCollocationMeasure,
): string {
  return CONCORDANCE_COLLOCATION_MEASURE_OPTIONS.find((option) => option.value === measure)?.label ?? "logDice";
}

export type ConcordanceStrengthDirection = "negative" | "zero" | "positive" | "nonNegative";

export interface ConcordanceStrengthBarModel {
  signed: boolean;
  direction: ConcordanceStrengthDirection;
  magnitudePercent: number;
}

export function concordanceCollocationMeasureIsSigned(
  measure: ConcordanceCollocationMeasure,
): boolean {
  return measure === "mi"
    || measure === "mi3"
    || measure === "tScore"
    || measure === "zScore"
    || measure === "logRatio";
}

export function concordanceCollocationStrengthBar(
  value: number | null,
  measure: ConcordanceCollocationMeasure,
  loadedScores: Array<number | null>,
): ConcordanceStrengthBarModel | null {
  if (value === null || !Number.isFinite(value)) return null;
  const finite = loadedScores.filter((score): score is number => score !== null && Number.isFinite(score));
  const signed = concordanceCollocationMeasureIsSigned(measure);
  const maximum = signed
    ? Math.max(0, ...finite.map((score) => Math.abs(score)))
    : Math.max(0, ...finite);
  const magnitudePercent = maximum === 0
    ? 0
    : Math.min(100, Math.abs(value) / maximum * 100);
  const direction: ConcordanceStrengthDirection = signed
    ? value < 0
      ? "negative"
      : value > 0
        ? "positive"
        : "zero"
    : "nonNegative";
  return { signed, direction, magnitudePercent };
}

export function nextConcordanceCollocateDisplayCount(
  current: number,
  loadedRows: number,
): number {
  return Math.min(loadedRows, Math.max(CONCORDANCE_COLLOCATION_INITIAL_ROWS, current) + CONCORDANCE_COLLOCATION_SHOW_MORE_STEP);
}

export interface ConcordancePositionGlyphBin {
  label: `L${1 | 2 | 3 | 4 | 5}` | `R${1 | 2 | 3 | 4 | 5}`;
  count: number;
  active: boolean;
  intensity: number;
}

export function concordanceCollocatePositionGlyph(
  left: number[],
  right: number[],
  span: number,
  direction: ConcordanceCollocationDirection,
): ConcordancePositionGlyphBin[] {
  const countAt = (values: number[], distance: number) => values[distance - 1] ?? 0;
  const leftActive = direction === "left" || direction === "both";
  const rightActive = direction === "right" || direction === "both";
  const bins: ConcordancePositionGlyphBin[] = [];
  for (const distance of [5, 4, 3, 2, 1] as const) {
    bins.push({
      label: `L${distance}`,
      count: countAt(left, distance),
      active: leftActive && distance <= span,
      intensity: 0,
    });
  }
  for (const distance of [1, 2, 3, 4, 5] as const) {
    bins.push({
      label: `R${distance}`,
      count: countAt(right, distance),
      active: rightActive && distance <= span,
      intensity: 0,
    });
  }
  const maximum = Math.max(0, ...bins.filter((bin) => bin.active).map((bin) => bin.count));
  return bins.map((bin) => ({
    ...bin,
    intensity: !bin.active || maximum === 0 ? 0 : bin.count / maximum,
  }));
}

export function concordancePositionGlyphAccessibleText(
  surface: string,
  bins: ConcordancePositionGlyphBin[],
): string {
  return `${surface}: ${bins.map((bin) => `${bin.label} ${bin.count}`).join(", ")}`;
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
  return `Collocates near "${query}" · ${span.toLocaleString()}-token window · ${directionText} · ranked by ${concordanceCollocationMeasureLabel(measure)}`;
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

export function concordanceDistributionMagnitudePercent(
  value: number,
  maximum: number,
): number {
  if (!Number.isFinite(value) || !Number.isFinite(maximum) || maximum <= 0) return 0;
  return Math.min(100, Math.max(0, value / maximum * 100));
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
