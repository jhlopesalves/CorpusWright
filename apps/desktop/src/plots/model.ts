export type PlotScaleKind = "linear" | "log";
export type PlotAnchor = "start" | "middle" | "end";

export interface PlotMargin {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface PlotBox {
  width: number;
  height: number;
  margin: PlotMargin;
  innerWidth: number;
  innerHeight: number;
}

export interface PlotScale {
  kind: PlotScaleKind;
  domain: [number, number];
  range: [number, number];
}

export interface PlotTick {
  value: number;
  position: number;
  label: string;
  accessibleLabel?: string;
  major: boolean;
}

export interface PlotTickLabelGeometry extends PlotLabelGeometry {
  tick: PlotTick;
}

export interface PlotLegendItem {
  label: string;
  className?: string;
  dashed?: boolean;
}

export interface PlotPointGeometry<TDatum = unknown> {
  x: number;
  y: number;
  rawX: number;
  rawY: number;
  datum: TDatum;
  rampIndex: number;
  excluded?: boolean;
  className?: string;
  fillOpacity?: number;
  label?: string;
  tooltipTitle?: string;
  tooltipRows?: string[];
}

export interface PlotLabelGeometry {
  x: number;
  y: number;
  text: string;
  anchor: PlotAnchor;
  className?: string;
  lines?: string[];
  fontSize?: number;
}

export interface LabelBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LabelBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface LabelPlacementCandidate {
  dx: number;
  dy: number;
  anchor: PlotAnchor;
}

export interface LabelPlacementRequest {
  x: number;
  y: number;
  text: string;
  candidates: LabelPlacementCandidate[];
  className?: string;
  lines?: string[];
  fontSize?: number;
}

export type LabelObstacle =
  | { kind: "box"; box: LabelBox }
  | { kind: "point"; x: number; y: number; radius: number };

export interface PlotPathGeometry {
  d: string;
  className: string;
}

export interface PlotLineGeometry {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  className: string;
}

export interface PlotBarGeometry<TDatum = unknown> {
  x: number;
  y: number;
  width: number;
  height: number;
  datum: TDatum;
  tooltipTitle?: string;
  tooltipRows?: string[];
}

export interface PlotBracketGeometry {
  x1: number;
  x2: number;
  y: number;
  label: string;
}

export interface RankFrequencyDatum {
  rank: number;
  frequency: number;
  ratePerMillion: number;
  token?: string;
}

export interface RankFrequencyGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: Array<PlotPointGeometry<RankFrequencyDatum>>;
  fitPath: PlotPathGeometry | null;
  legend: PlotLegendItem[];
  labels: PlotLabelGeometry[];
  bracket: PlotBracketGeometry | null;
}

export interface VocabularyGrowthDatum {
  tokens: number;
  types: number;
}

export interface VocabularyGrowthGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: Array<PlotPointGeometry<VocabularyGrowthDatum>>;
  linePath: PlotPathGeometry | null;
  fitPath: PlotPathGeometry | null;
  areaPath: PlotPathGeometry | null;
  legend: PlotLegendItem[];
  labels: PlotLabelGeometry[];
}

export interface FrequencySpectrumDatum {
  frequency: number;
  typeCount: number;
}

export interface LexicalSpectrumBandGeometry {
  x: number;
  width: number;
  label: string;
  alternate: boolean;
}

export interface LexicalSpectrumGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: Array<PlotPointGeometry<FrequencySpectrumDatum>>;
  bands: LexicalSpectrumBandGeometry[];
  labels: PlotLabelGeometry[];
}

export interface CoverageDatum {
  k: number;
  share: number;
}

export interface CoverageMilestone {
  k: number;
  share: number | null;
}

export interface CoverageCrossing {
  k: number;
  share: number;
  x: number;
  y: number;
}

export interface CoverageGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: Array<PlotPointGeometry<CoverageDatum>>;
  linePath: PlotPathGeometry | null;
  areaPath: PlotPathGeometry | null;
  milestones: Array<PlotPointGeometry<CoverageDatum>>;
  labels: PlotLabelGeometry[];
  halfCrossing: CoverageCrossing | null;
}

export interface DispersionBucketDatum {
  key: string;
  label: string;
  count: number;
  share: number | null;
  highlighted?: boolean;
  detail?: string;
}

export interface DispersionGeometry {
  box: PlotBox;
  xScale: PlotScale;
  xTicks: PlotTick[];
  points: Array<PlotPointGeometry<DispersionBucketDatum>>;
  stems: PlotLineGeometry[];
  dividers: PlotLineGeometry[];
  compositionSegments: DispersionCompositionSegmentGeometry[];
  labels: PlotLabelGeometry[];
}

export interface DispersionCompositionSegmentGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  label: string | null;
  datum: DispersionBucketDatum;
  rampIndex: number;
  tooltipTitle: string;
  tooltipRows: string[];
}

export type DispersionWitnessKind = "core" | "loudButLocal" | "quietAndEven" | "rareAndLocal";

export interface DispersionWitnessDatum {
  token: string;
  frequency: number;
  documentFrequency: number;
  dp: number;
  kind: DispersionWitnessKind;
}

export interface DispersionPlanePayload {
  xBins: number;
  yBins: number;
  freqLog10Max: number;
  counts: number[];
  maxCellCount: number;
  witnesses: DispersionWitnessDatum[];
  loudButLocalCount: number;
  loudButLocalMinFrequency: number;
  highDispersionThreshold: number;
  reading: string;
}

export interface DispersionPlaneCellGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  count: number;
  t: number;
  opacity: number;
  fill: string;
  tooltipTitle: string;
  tooltipRows: string[];
}

export interface DispersionPlanePillGeometry {
  label: PlotLabelGeometry;
  box: LabelBox;
}

export interface DispersionPlaneGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  cells: DispersionPlaneCellGeometry[];
  thresholdLine: PlotLineGeometry;
  thresholdLabel: PlotLabelGeometry;
  quadrantPills: DispersionPlanePillGeometry[];
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>;
  labels: PlotLabelGeometry[];
  witnessObstacles: LabelObstacle[];
}

export interface DispersionRosterEntryDatum {
  token: string;
  frequency: number;
  documentFrequency: number;
  dp: number;
}

export interface DispersionRosterRowGeometry {
  entry: DispersionRosterEntryDatum;
  y: number;
  guide: PlotLineGeometry;
  point: PlotPointGeometry<DispersionRosterEntryDatum>;
  tokenLabel: PlotLabelGeometry;
  aboveThreshold: boolean;
}

export interface DispersionRosterGeometry {
  box: PlotBox;
  xScale: PlotScale;
  xTicks: PlotTick[];
  rows: DispersionRosterRowGeometry[];
  thresholdLine: PlotLineGeometry;
  thresholdLabel: PlotLabelGeometry;
  aboveThresholdCount: number;
}

export interface LengthHistogramBinDatum {
  lowerTokens: number;
  upperTokens: number | null;
  count: number;
}

export interface LengthHistogramGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  bars: Array<PlotBarGeometry<LengthHistogramBinDatum>>;
  medianLine: PlotLineGeometry | null;
  labels: PlotLabelGeometry[];
}

export interface MattrWitnessDatum {
  name: string;
  mattr: number;
  tokens: number;
}

export interface MattrDistributionPayload {
  binWidth: number;
  counts: number[];
  usedDocuments: number;
  excludedDocuments: number;
  lowWitnesses: MattrWitnessDatum[];
  highWitnesses: MattrWitnessDatum[];
  reading: string;
}

export interface MattrDistributionBinDatum {
  index: number;
  lower: number;
  upper: number;
  count: number;
}

export interface MattrWitnessTickGeometry {
  x: number;
  y1: number;
  y2: number;
  datum: MattrWitnessDatum;
  tooltipTitle: string;
  tooltipRows: string[];
}

export interface MattrDistributionGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  bars: Array<PlotBarGeometry<MattrDistributionBinDatum>>;
  medianLine: PlotLineGeometry | null;
  labels: PlotLabelGeometry[];
  witnessTicks: MattrWitnessTickGeometry[];
  domain: [number, number];
}

export const DEFAULT_PLOT_WIDTH = 720;
export const DEFAULT_PLOT_HEIGHT = 360;
export const DEFAULT_PLOT_MARGIN: PlotMargin = {
  top: 28,
  right: 34,
  bottom: 54,
  left: 72,
};

const LOG_MINIMUM = 1e-9;
export const AXIS_TITLE_FONT_SIZE = 12;
export const TICK_LABEL_FONT_SIZE = 11.5;
export const ANNOTATION_LABEL_FONT_SIZE = 11.5;
export const MONO_SMALL_LABEL_FONT_SIZE = ANNOTATION_LABEL_FONT_SIZE;
const LABEL_WIDTH_FACTOR = 0.62;
const LABEL_LINE_HEIGHT_FACTOR = 1.3;
const DENSITY_CELL_LABEL_OBSTACLE_OPACITY = 0.52;
const QUADRANT_PILL_HEIGHT = 20;
const QUADRANT_PILL_PADDING_X = 10;
const DISPERSION_ROSTER_ROW_PITCH = 22;

export function createPlotBox(
  width = DEFAULT_PLOT_WIDTH,
  height = DEFAULT_PLOT_HEIGHT,
  margin: PlotMargin = DEFAULT_PLOT_MARGIN,
): PlotBox {
  return {
    width,
    height,
    margin,
    innerWidth: Math.max(1, width - margin.left - margin.right),
    innerHeight: Math.max(1, height - margin.top - margin.bottom),
  };
}

export function createScale(kind: PlotScaleKind, domain: [number, number], range: [number, number]): PlotScale {
  const normalised = kind === "log" ? normaliseLogDomain(domain) : normaliseLinearDomain(domain);
  return { kind, domain: normalised, range };
}

export function scaleValue(scale: PlotScale, value: number): number {
  const [domainMin, domainMax] = scale.domain;
  const [rangeMin, rangeMax] = scale.range;
  if (domainMin === domainMax) {
    return (rangeMin + rangeMax) / 2;
  }
  const domainValue = scale.kind === "log" ? Math.log10(Math.max(value, LOG_MINIMUM)) : value;
  const start = scale.kind === "log" ? Math.log10(domainMin) : domainMin;
  const end = scale.kind === "log" ? Math.log10(domainMax) : domainMax;
  const t = (domainValue - start) / (end - start);
  return rangeMin + t * (rangeMax - rangeMin);
}

export function logTicks(scale: PlotScale): PlotTick[] {
  if (scale.kind !== "log") {
    throw new Error("logTicks requires a log scale");
  }
  const [min, max] = scale.domain;
  const startPower = Math.floor(Math.log10(min));
  const endPower = Math.ceil(Math.log10(max));
  const ticks: PlotTick[] = [];
  for (let power = startPower; power <= endPower; power += 1) {
    const decade = 10 ** power;
    for (let multiplier = 1; multiplier <= 9; multiplier += 1) {
      const value = multiplier * decade;
      if (value < min || value > max) continue;
      ticks.push({
        value,
        position: scaleValue(scale, value),
        label: multiplier === 1 ? formatTickNumber(value) : "",
        major: multiplier === 1,
      });
    }
  }
  return ticks;
}

export function linearTicks(scale: PlotScale, targetCount = 5): PlotTick[] {
  if (scale.kind !== "linear") {
    throw new Error("linearTicks requires a linear scale");
  }
  const [min, max] = scale.domain;
  if (min === max) {
    return [{ value: min, position: scaleValue(scale, min), label: formatTickNumber(min), major: true }];
  }
  const span = niceNumber(max - min, false);
  const step = niceNumber(span / Math.max(1, targetCount - 1), true);
  const tickMin = Math.ceil(min / step) * step;
  const tickMax = Math.floor(max / step) * step;
  const ticks: PlotTick[] = [];
  for (let value = tickMin; value <= tickMax + step / 2; value += step) {
    const rounded = Number(value.toPrecision(12));
    ticks.push({
      value: rounded,
      position: scaleValue(scale, rounded),
      label: formatTickNumber(rounded),
      major: true,
    });
  }
  return ticks;
}

export function pointsToPath(points: Array<{ x: number; y: number }>): string {
  return points
    .map((point, index) => `${index === 0 ? "M" : "L"}${roundSvg(point.x)} ${roundSvg(point.y)}`)
    .join(" ");
}

export function pointsToAreaPath(points: Array<{ x: number; y: number }>, baselineY: number): string {
  if (points.length === 0) return "";
  const line = pointsToPath(points);
  const last = points[points.length - 1];
  const first = points[0];
  return `${line} L${roundSvg(last.x)} ${roundSvg(baselineY)} L${roundSvg(first.x)} ${roundSvg(baselineY)} Z`;
}

export function findMilestoneShare(points: CoverageDatum[], k: number): number | null {
  return points.find((point) => point.k === k)?.share ?? null;
}

export function findFirstCoverageCrossing(points: CoverageDatum[], targetShare: number): CoverageDatum | null {
  return points.find((point) => point.share >= targetShare) ?? null;
}

export function labelTextLines(label: Pick<PlotLabelGeometry, "lines" | "text">): string[] {
  return label.lines && label.lines.length > 0 ? label.lines : label.text.split("\n");
}

export function labelFontSize(label: Pick<PlotLabelGeometry, "className" | "fontSize">): number {
  if (label.fontSize !== undefined) return label.fontSize;
  return label.className?.includes("cw-plot-label-mono") ? MONO_SMALL_LABEL_FONT_SIZE : ANNOTATION_LABEL_FONT_SIZE;
}

export function labelLineHeight(label: Pick<PlotLabelGeometry, "className" | "fontSize">): number {
  return labelFontSize(label) * LABEL_LINE_HEIGHT_FACTOR;
}

export function estimateLabelBox(label: Pick<PlotLabelGeometry, "x" | "y" | "text" | "anchor" | "className" | "fontSize" | "lines">): LabelBox {
  const fontSize = labelFontSize(label);
  const lines = labelTextLines(label);
  const width = Math.max(1, ...lines.map((line) => line.length)) * fontSize * LABEL_WIDTH_FACTOR;
  const height = lines.length * labelLineHeight(label);
  const x = label.anchor === "end" ? label.x - width : label.anchor === "middle" ? label.x - width / 2 : label.x;
  return {
    x,
    y: label.y - fontSize,
    width,
    height,
  };
}

export function placeLabels(
  labels: LabelPlacementRequest[],
  obstacles: LabelObstacle[],
  bounds: LabelBounds,
): PlotLabelGeometry[] {
  const placed: PlotLabelGeometry[] = [];
  const activeObstacles = [...obstacles];
  for (const label of labels) {
    let chosen: PlotLabelGeometry | null = null;
    let fallback: { label: PlotLabelGeometry; score: number } | null = null;
    for (const candidate of label.candidates) {
      const proposed = applyLabelCandidate(label, candidate);
      const proposedBox = estimateLabelBox(proposed);
      const outsideBounds = boxLeavesBounds(proposedBox, bounds);
      const clamped = clampLabelToBounds(proposed, bounds);
      const clampedBox = estimateLabelBox(clamped);
      const overlapScore = scoreLabelBox(clampedBox, activeObstacles);
      const candidateScore = overlapScore + (outsideBounds ? 1_000_000 : 0);
      if (!fallback || candidateScore < fallback.score) {
        fallback = { label: clamped, score: candidateScore };
      }
      if (!outsideBounds && overlapScore === 0) {
        chosen = proposed;
        break;
      }
    }
    const placedLabel = chosen ?? fallback?.label ?? applyLabelCandidate(label, {
      dx: 0,
      dy: 0,
      anchor: "start",
    });
    placed.push(placedLabel);
    activeObstacles.push({ kind: "box", box: estimateLabelBox(placedLabel) });
  }
  return placed;
}

export function placeAxisTickLabels(
  ticks: PlotTick[],
  axis: "x" | "y",
  box: PlotBox,
): PlotTickLabelGeometry[] {
  const labelledTicks = ticks.filter((tick) => tick.major && tick.label);
  if (labelledTicks.length === 0) return [];
  const requests = labelledTicks.map((tick) => ({
    x: axis === "x" ? tick.position : box.margin.left - 10,
    y: axis === "x" ? box.height - box.margin.bottom + 19 : tick.position + 4,
    text: middleEllipsise(tick.label, axis === "x" ? 16 : 10),
    className: "cw-plot-tick-label",
    fontSize: TICK_LABEL_FONT_SIZE,
    candidates: [{ dx: 0, dy: 0, anchor: axis === "x" ? "middle" as const : "end" as const }],
  }));
  const placed = placeLabels(requests, [], {
    left: 0,
    top: 0,
    right: box.width,
    bottom: box.height,
  });
  const anchors = new Set<number>([0, placed.length - 1]);
  const accepted: PlotTickLabelGeometry[] = [];
  for (const index of anchors) {
    const label = placed[index];
    const tick = labelledTicks[index];
    if (label && tick) accepted.push({ ...label, tick });
  }
  for (let index = 1; index < placed.length - 1; index += 1) {
    const label = placed[index];
    const tick = labelledTicks[index];
    if (!label || !tick) continue;
    const candidateBox = estimateLabelBox(label);
    if (accepted.every((existing) => boxOverlapArea(candidateBox, estimateLabelBox(existing)) === 0)) {
      accepted.push({ ...label, tick });
    }
  }
  return accepted.sort((left, right) => left.tick.position - right.tick.position);
}

export function middleEllipsise(value: string, maxLength: number): string {
  if (value.length <= maxLength || maxLength < 2) return value;
  const preserved = maxLength - 1;
  const start = Math.ceil(preserved / 2);
  const end = Math.floor(preserved / 2);
  return `${value.slice(0, start)}…${value.slice(value.length - end)}`;
}

function applyLabelCandidate(label: LabelPlacementRequest, candidate: LabelPlacementCandidate): PlotLabelGeometry {
  return {
    x: label.x + candidate.dx,
    y: label.y + candidate.dy,
    text: label.text,
    anchor: candidate.anchor,
    className: label.className,
    lines: label.lines,
    fontSize: label.fontSize,
  };
}

function clampLabelToBounds(label: PlotLabelGeometry, bounds: LabelBounds): PlotLabelGeometry {
  const box = estimateLabelBox(label);
  const maxWidth = bounds.right - bounds.left;
  const maxHeight = bounds.bottom - bounds.top;
  let dx = 0;
  let dy = 0;
  if (box.width > maxWidth) {
    dx = bounds.left - box.x;
  } else if (box.x < bounds.left) {
    dx = bounds.left - box.x;
  } else if (box.x + box.width > bounds.right) {
    dx = bounds.right - (box.x + box.width);
  }
  if (box.height > maxHeight) {
    dy = bounds.top - box.y;
  } else if (box.y < bounds.top) {
    dy = bounds.top - box.y;
  } else if (box.y + box.height > bounds.bottom) {
    dy = bounds.bottom - (box.y + box.height);
  }
  return dx === 0 && dy === 0 ? label : { ...label, x: label.x + dx, y: label.y + dy };
}

function boxLeavesBounds(box: LabelBox, bounds: LabelBounds): boolean {
  return box.x < bounds.left || box.y < bounds.top || box.x + box.width > bounds.right || box.y + box.height > bounds.bottom;
}

function scoreLabelBox(box: LabelBox, obstacles: LabelObstacle[]): number {
  return obstacles.reduce((total, obstacle) => total + labelObstacleOverlap(box, obstacle), 0);
}

function labelObstacleOverlap(box: LabelBox, obstacle: LabelObstacle): number {
  if (obstacle.kind === "box") return boxOverlapArea(box, obstacle.box);
  const closestX = Math.max(box.x, Math.min(obstacle.x, box.x + box.width));
  const closestY = Math.max(box.y, Math.min(obstacle.y, box.y + box.height));
  const distance = Math.hypot(obstacle.x - closestX, obstacle.y - closestY);
  return distance >= obstacle.radius ? 0 : (obstacle.radius - distance) ** 2;
}

function boxOverlapArea(left: LabelBox, right: LabelBox): number {
  const width = Math.min(left.x + left.width, right.x + right.width) - Math.max(left.x, right.x);
  const height = Math.min(left.y + left.height, right.y + right.height) - Math.max(left.y, right.y);
  return width <= 0 || height <= 0 ? 0 : width * height;
}

function plotLabelBounds(box: PlotBox): LabelBounds {
  return {
    left: box.margin.left,
    top: box.margin.top,
    right: box.width - box.margin.right,
    bottom: box.height - box.margin.bottom,
  };
}

function pointLabelObstacles(points: Array<{ x: number; y: number }>, radius = 7): LabelObstacle[] {
  return points.map((point) => ({ kind: "point" as const, x: point.x, y: point.y, radius }));
}

function polylineLabelObstacles(points: Array<{ x: number; y: number }>, radius = 5, spacing = 7): LabelObstacle[] {
  return samplePolylinePoints(points, spacing).map((point) => ({ kind: "point" as const, x: point.x, y: point.y, radius }));
}

function lineLabelObstacles(lines: PlotLineGeometry[], thickness = 6): LabelObstacle[] {
  return lines.map((line) => ({
    kind: "box" as const,
    box: {
      x: Math.min(line.x1, line.x2),
      y: Math.min(line.y1, line.y2) - thickness / 2,
      width: Math.max(1, Math.abs(line.x2 - line.x1)),
      height: Math.max(1, Math.abs(line.y2 - line.y1) + thickness),
    },
  }));
}

function samplePolylinePoints(points: Array<{ x: number; y: number }>, spacing: number): Array<{ x: number; y: number }> {
  if (points.length <= 1) return points;
  const sampled: Array<{ x: number; y: number }> = [];
  for (let index = 0; index < points.length - 1; index += 1) {
