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
  areaPath: PlotPathGeometry | null;
  labels: PlotLabelGeometry[];
}

export interface FrequencySpectrumDatum {
  frequency: number;
  typeCount: number;
}

export interface LexicalSpectrumGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: Array<PlotPointGeometry<FrequencySpectrumDatum>>;
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
  labels: PlotLabelGeometry[];
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
  label: PlotLabelGeometry;
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
    const start = points[index]!;
    const end = points[index + 1]!;
    const distance = Math.hypot(end.x - start.x, end.y - start.y);
    const steps = Math.max(1, Math.ceil(distance / spacing));
    for (let step = 0; step < steps; step += 1) {
      const t = step / steps;
      sampled.push({
        x: start.x + (end.x - start.x) * t,
        y: start.y + (end.y - start.y) * t,
      });
    }
  }
  sampled.push(points[points.length - 1]!);
  return sampled;
}

export function buildRankFrequencyGeometry(options: {
  points: RankFrequencyDatum[];
  yMode: "rate" | "raw";
  totalTokens: number;
  zipfExponent: number | null;
  zipfCoefficient: number | null;
  fittedRankStart: number | null;
  fittedRankEnd: number | null;
  box?: PlotBox;
}): RankFrequencyGeometry {
  const box = options.box ?? createPlotBox();
  const visible = options.points.filter((point) => point.rank > 0 && yValueForRank(point, options.yMode) > 0);
  const xDomain = extent(visible.map((point) => point.rank));
  const yDomain = extent(visible.map((point) => yValueForRank(point, options.yMode)));
  const xScale = createScale("log", xDomain, [box.margin.left, box.width - box.margin.right]);
  const yScale = createScale("log", yDomain, [box.height - box.margin.bottom, box.margin.top]);
  const fittedStart = options.fittedRankStart;
  const fittedEnd = options.fittedRankEnd;
  const points = visible.map((point) => {
    const rawY = yValueForRank(point, options.yMode);
    const excluded = fittedStart === null || fittedEnd === null || point.rank < fittedStart || point.rank > fittedEnd
      || point.frequency <= 1;
    return {
      x: scaleValue(xScale, point.rank),
      y: scaleValue(yScale, rawY),
      rawX: point.rank,
      rawY,
      datum: point,
      rampIndex: rampIndex(point.rank, xScale.domain, true),
      excluded,
      tooltipTitle: point.token ?? `rank ${formatInteger(point.rank)}`,
      tooltipRows: [
        `rank ${formatInteger(point.rank)}`,
        options.yMode === "rate"
          ? `${formatFixed(rawY, 2)} per million`
          : `${formatInteger(point.frequency)} tokens`,
      ],
    };
  });
  const fitPoints = fitPointsForRank(options, xScale, yScale);
  const fitPath = fitPoints.length > 0 ? { d: pointsToPath(fitPoints), className: "cw-plot-fit-line cw-plot-line-glow" } : null;
  const headPoints = points.filter((point) => point.datum.token && point.datum.rank <= 3);
  const headLabels = placeLabels(
    headPoints.map((point) => ({
      x: point.x,
      y: point.y,
      text: point.datum.token ?? "",
      className: "cw-plot-label-muted",
      candidates: [
        { dx: 8, dy: -8, anchor: "start" },
        { dx: 12, dy: 6, anchor: "start" },
        { dx: 8, dy: 18, anchor: "start" },
        { dx: -10, dy: -8, anchor: "end" },
        { dx: -12, dy: 6, anchor: "end" },
      ],
    })),
    [
      ...pointLabelObstacles(headPoints, 7),
      ...polylineLabelObstacles(fitPoints, 4),
    ],
    plotLabelBounds(box),
  );
  const bracket = fittedStart !== null && fittedEnd !== null && fittedEnd >= fittedStart
    ? {
        x1: scaleValue(xScale, fittedStart),
        x2: scaleValue(xScale, fittedEnd),
        y: box.height - box.margin.bottom + 22,
        label: "FITTED RANGE",
      }
    : null;
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: logTicks(yScale),
    points,
    fitPath,
    labels: headLabels,
    bracket,
  };
}

export function buildVocabularyGrowthGeometry(options: {
  points: VocabularyGrowthDatum[];
  box?: PlotBox;
}): VocabularyGrowthGeometry {
  const box = options.box ?? createPlotBox();
  const visible = options.points.filter((point) => point.tokens > 0 && point.types > 0);
  const xScale = createScale("log", extent(visible.map((point) => point.tokens)), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("log", extent(visible.map((point) => point.types)), [
    box.height - box.margin.bottom,
    box.margin.top,
  ]);
  const points = visible.map((point) => ({
    x: scaleValue(xScale, point.tokens),
    y: scaleValue(yScale, point.types),
    rawX: point.tokens,
    rawY: point.types,
    datum: point,
    rampIndex: rampIndex(point.tokens, xScale.domain, true),
    tooltipTitle: `${formatInteger(point.types)} types`,
    tooltipRows: [`${formatInteger(point.tokens)} tokens`, `${formatInteger(point.types)} types`],
  }));
  const linePath = points.length > 0 ? { d: pointsToPath(points), className: "cw-plot-line cw-plot-line-glow" } : null;
  const areaPath = points.length > 0
    ? { d: pointsToAreaPath(points, box.height - box.margin.bottom), className: "cw-plot-area" }
    : null;
  const last = points[points.length - 1];
  const labels = last
    ? placeLabels(
        [{
          x: last.x,
          y: last.y,
          text: `${formatInteger(last.datum.types)} types at ${formatMillions(last.datum.tokens)} tokens`,
          lines: [`${formatInteger(last.datum.types)} types`, `at ${formatMillions(last.datum.tokens)} tokens`],
          className: "cw-plot-label-callout",
          candidates: [
            { dx: -4, dy: 38, anchor: "end" },
            { dx: -14, dy: 50, anchor: "end" },
            { dx: -18, dy: 28, anchor: "end" },
            { dx: -10, dy: 18, anchor: "end" },
            { dx: 12, dy: 38, anchor: "start" },
          ],
        }],
        [
          ...polylineLabelObstacles(points, 5),
          ...pointLabelObstacles(points, 7),
        ],
        plotLabelBounds(box),
      )
    : [];
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: logTicks(yScale),
    points,
    linePath,
    areaPath,
    labels,
  };
}

export function buildLexicalSpectrumGeometry(options: {
  points: FrequencySpectrumDatum[];
  hapaxCount: number;
  disLegomenaCount: number;
  box?: PlotBox;
}): LexicalSpectrumGeometry {
  const box = options.box ?? createPlotBox();
  const visible = options.points.filter((point) => point.frequency > 0 && point.typeCount > 0);
  const xScale = createScale("log", extent(visible.map((point) => point.frequency)), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("log", extent(visible.map((point) => point.typeCount)), [
    box.height - box.margin.bottom,
    box.margin.top,
  ]);
  const points = visible.map((point) => ({
    x: scaleValue(xScale, point.frequency),
    y: scaleValue(yScale, point.typeCount),
    rawX: point.frequency,
    rawY: point.typeCount,
    datum: point,
    rampIndex: rampIndex(point.frequency, xScale.domain, true),
    tooltipTitle: `${formatInteger(point.typeCount)} types`,
    tooltipRows: [`frequency ${formatInteger(point.frequency)}`, `${formatInteger(point.typeCount)} types`],
  }));
  const labelRequests = [
    labelRequestForExactFrequency(points, 1, `hapax ${formatInteger(options.hapaxCount)}`),
    labelRequestForExactFrequency(points, 2, `dis ${formatInteger(options.disLegomenaCount)}`),
  ].filter((label): label is LabelPlacementRequest => label !== null);
  const labels = placeLabels(
    labelRequests,
    pointLabelObstacles(points, 7),
    plotLabelBounds(box),
  );
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: logTicks(yScale),
    points,
    labels,
  };
}

export function buildCoverageGeometry(options: {
  points: CoverageDatum[];
  milestones: CoverageMilestone[];
  box?: PlotBox;
}): CoverageGeometry {
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, DEFAULT_PLOT_HEIGHT, {
    top: 28,
    right: 34,
    bottom: 54,
    left: 76,
  });
  const visible = options.points.filter((point) => point.k > 0 && point.share >= 0);
  const xScale = createScale("log", extent(visible.map((point) => point.k)), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("linear", [0, 1], [box.height - box.margin.bottom, box.margin.top]);
  const points = visible.map((point) => ({
    x: scaleValue(xScale, point.k),
    y: scaleValue(yScale, point.share),
    rawX: point.k,
    rawY: point.share,
    datum: point,
    rampIndex: rampIndex(point.k, xScale.domain, true),
    tooltipTitle: `top ${formatInteger(point.k)}`,
    tooltipRows: [`${formatPercent(point.share)} of tokens`],
  }));
  const linePath = points.length > 0 ? { d: pointsToPath(points), className: "cw-plot-line cw-plot-line-glow" } : null;
  const areaPath = points.length > 0
    ? { d: pointsToAreaPath(points, box.height - box.margin.bottom), className: "cw-plot-area" }
    : null;
  const milestones: Array<PlotPointGeometry<CoverageDatum>> = options.milestones
    .map<PlotPointGeometry<CoverageDatum> | null>((milestone) => {
      const share = milestone.share ?? findMilestoneShare(visible, milestone.k);
      if (share === null) return null;
      const datum = { k: milestone.k, share };
      return {
        x: scaleValue(xScale, datum.k),
        y: scaleValue(yScale, datum.share),
        rawX: datum.k,
        rawY: datum.share,
        datum,
        rampIndex: rampIndex(datum.k, xScale.domain, true),
        tooltipTitle: `top ${formatInteger(datum.k)}`,
        tooltipRows: [`${formatPercent(datum.share)} of tokens`],
      };
    })
    .filter((point): point is PlotPointGeometry<CoverageDatum> => point !== null);
  const crossingDatum = findFirstCoverageCrossing(visible, 0.5);
  const halfCrossing = crossingDatum
    ? {
        k: crossingDatum.k,
        share: crossingDatum.share,
        x: scaleValue(xScale, crossingDatum.k),
        y: scaleValue(yScale, crossingDatum.share),
      }
    : null;
  const labelRequests: LabelPlacementRequest[] = milestones.map((point) => ({
    x: point.x,
    y: point.y,
    text: `top ${formatInteger(point.datum.k)} - ${formatPercent(point.datum.share)}`,
    className: "cw-plot-label-muted",
    candidates: [
      { dx: 10, dy: 18, anchor: "start" },
      { dx: 12, dy: -10, anchor: "start" },
      { dx: -10, dy: 18, anchor: "end" },
      { dx: -10, dy: -10, anchor: "end" },
      { dx: 18, dy: 4, anchor: "start" },
    ],
  }));
  if (halfCrossing) {
    labelRequests.push({
      x: box.margin.left + 10,
      y: halfCrossing.y - 10,
      text: `half the corpus = ${formatInteger(halfCrossing.k)} types`,
      className: "cw-plot-label-callout",
      candidates: [
        { dx: 0, dy: 0, anchor: "start" },
        { dx: 12, dy: -8, anchor: "start" },
        { dx: 0, dy: -18, anchor: "start" },
      ],
    });
  }
  const labels = placeLabels(
    labelRequests,
    [
      ...polylineLabelObstacles(points, 5),
      ...pointLabelObstacles(milestones, 7),
    ],
    plotLabelBounds(box),
  );
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: linearTicks(yScale, 6).map((tick) => ({ ...tick, label: `${Math.round(tick.value * 100)}%` })),
    points,
    linePath,
    areaPath,
    milestones,
    labels,
    halfCrossing,
  };
}

export function buildDispersionGeometry(options: {
  buckets: DispersionBucketDatum[];
  box?: PlotBox;
}): DispersionGeometry {
  const rowCount = Math.max(1, options.buckets.length);
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, Math.max(360, 132 + rowCount * 19), {
    top: 30,
    right: 36,
    bottom: 48,
    left: 156,
  });
  const buckets = options.buckets;
  const positiveCounts = buckets.filter((bucket) => bucket.count > 0).map((bucket) => bucket.count);
  const xScale = createScale("log", extent(positiveCounts), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const hasDistinctFinalRow = buckets.some((bucket) => bucket.highlighted);
  const rowGap = hasDistinctFinalRow ? 0.8 : 0;
  const lastVirtualRow = Math.max(1, buckets.length - 1 + rowGap);
  let virtualRow = 0;
  const points = buckets.map((bucket, index) => {
    if (bucket.highlighted && index > 0) virtualRow += rowGap;
    const y = buckets.length > 1
      ? box.margin.top + (virtualRow / lastVirtualRow) * box.innerHeight
      : box.margin.top + box.innerHeight / 2;
    virtualRow += 1;
    const plotCount = Math.max(1, bucket.count);
    return {
      x: bucket.count > 0 ? scaleValue(xScale, bucket.count) : scaleValue(xScale, xScale.domain[0]),
      y,
      rawX: bucket.count,
      rawY: index,
      datum: bucket,
      rampIndex: buckets.length <= 1 ? 2 : rampIndex(plotCount, xScale.domain, true),
      excluded: bucket.count === 0,
      tooltipTitle: bucket.label,
      tooltipRows: [
        `${formatInteger(bucket.count)} types`,
        bucket.share === null ? "" : `${formatPercent(bucket.share)} of vocabulary`,
      ].filter(Boolean),
    };
  });
  const stems = points.map((point) => ({
    x1: box.margin.left,
    y1: point.y,
    x2: point.x,
    y2: point.y,
    className: `cw-plot-lollipop-stem cw-plot-ramp-${point.rampIndex}`,
  }));
  const distinctPoint = points.find((point) => point.datum.highlighted);
  const previousPoint = distinctPoint ? points[points.indexOf(distinctPoint) - 1] : undefined;
  const dividers = distinctPoint && previousPoint
    ? [{
        x1: box.margin.left,
        y1: (distinctPoint.y + previousPoint.y) / 2,
        x2: box.width - box.margin.right,
        y2: (distinctPoint.y + previousPoint.y) / 2,
        className: "cw-plot-reference-line",
      }]
    : [];
  const dotAndStemObstacles = [
    ...pointLabelObstacles(points, 8),
    ...lineLabelObstacles(stems, 5),
  ];
  const rowLabels = placeLabels(
    points.map((point) => ({
      x: box.margin.left - 10,
      y: point.y + 4,
      text: point.datum.label,
      className: point.datum.highlighted ? "cw-plot-label-strong" : "cw-plot-label-muted",
      candidates: [
        { dx: 0, dy: 0, anchor: "end" },
        { dx: -4, dy: 0, anchor: "end" },
        { dx: 0, dy: -8, anchor: "end" },
        { dx: 0, dy: 10, anchor: "end" },
      ],
    })),
    dotAndStemObstacles,
    {
      left: 10,
      top: box.margin.top - 8,
      right: box.margin.left - 8,
      bottom: box.height - box.margin.bottom + 8,
    },
  );
  const countLabels = placeLabels(
    points.map((point) => ({
      x: Math.min(point.x + 10, box.width - box.margin.right - 10),
      y: point.y + 4,
      text: point.datum.detail ?? formatInteger(point.datum.count),
      className: "cw-plot-label-mono",
      candidates: [
        { dx: 0, dy: 0, anchor: "start" },
        { dx: 10, dy: -8, anchor: "start" },
        { dx: 10, dy: 10, anchor: "start" },
        { dx: -10, dy: -8, anchor: "end" },
        { dx: -10, dy: 10, anchor: "end" },
      ],
    })),
    [
      ...dotAndStemObstacles,
      ...lineLabelObstacles(dividers, 4),
      ...rowLabels.map((label) => ({ kind: "box" as const, box: estimateLabelBox(label) })),
    ],
    plotLabelBounds(box),
  );
  return {
    box,
    xScale,
    xTicks: logTicks(xScale),
    points,
    stems,
    dividers,
    labels: [...rowLabels, ...countLabels],
  };
}

export function buildDispersionPlaneGeometry(options: {
  plane: DispersionPlanePayload;
  totalTypes: number;
  box?: PlotBox;
}): DispersionPlaneGeometry {
  const plane = options.plane;
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 420, {
    top: 34,
    right: 48,
    bottom: 62,
    left: 112,
  });
  const freqLog10Max = Math.max(0, plane.freqLog10Max);
  const xScale = createScale("linear", [0, freqLog10Max], [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("linear", [0, 1], [box.height - box.margin.bottom, box.margin.top]);
  const cells = buildDispersionPlaneCells(plane, xScale, yScale);
  const thresholdY = scaleValue(yScale, plane.highDispersionThreshold);
  const thresholdLine = {
    x1: box.margin.left,
    y1: thresholdY,
    x2: box.width - box.margin.right,
    y2: thresholdY,
    className: "cw-plot-threshold-line",
  };
  const thresholdLabel = {
    x: box.width - box.margin.right - 4,
    y: thresholdY - 8,
    text: `DP ${formatFixed(plane.highDispersionThreshold, 1)} · high-dispersion threshold`,
    anchor: "end" as const,
    className: "cw-plot-label-mono cw-plot-label-muted",
    fontSize: 10.5,
  };
  const witnesses = plane.witnesses.map((witness) => {
    const rawX = Math.log10(Math.max(1, witness.frequency));
    const rawY = clamp(witness.dp, 0, 1);
    return {
      x: scaleValue(xScale, rawX),
      y: scaleValue(yScale, rawY),
      rawX,
      rawY,
      datum: witness,
      rampIndex: 4,
      tooltipTitle: witness.token,
      tooltipRows: [
        `frequency ${formatInteger(witness.frequency)}`,
        `document frequency ${formatInteger(witness.documentFrequency)}`,
        `DP ${formatFixed(witness.dp, 3)}`,
      ],
    };
  });
  const denseCellObstacles = cells
    .filter((cell) => cell.opacity >= DENSITY_CELL_LABEL_OBSTACLE_OPACITY)
    .map((cell) => ({ kind: "box" as const, box: { x: cell.x, y: cell.y, width: cell.width, height: cell.height } }));
  const witnessDotObstacles = pointLabelObstacles(witnesses, 9);
  const thresholdObstacles: LabelObstacle[] = [
    { kind: "box", box: estimateLabelBox(thresholdLabel) },
    ...lineLabelObstacles([thresholdLine], 4),
  ];
  const preliminaryWitnessLabels = placeLabels(
    buildDispersionPlaneWitnessLabelRequests(witnesses),
    [
      ...denseCellObstacles,
      ...witnessDotObstacles,
      ...thresholdObstacles,
    ],
    plotLabelBounds(box),
  );
  const quadrantPills = buildDispersionPlaneQuadrantPills(
    box,
    [
      ...witnessDotObstacles,
      ...preliminaryWitnessLabels.map((label) => ({ kind: "box" as const, box: estimateLabelBox(label) })),
      ...thresholdObstacles,
    ],
  );
  const pillObstacles = quadrantPills.map((pill) => ({ kind: "box" as const, box: pill.box }));
  const witnessObstacles = [
    ...denseCellObstacles,
    ...witnessDotObstacles,
    ...thresholdObstacles,
    ...pillObstacles,
  ];
  const labels = placeLabels(
    buildDispersionPlaneWitnessLabelRequests(witnesses),
    witnessObstacles,
    plotLabelBounds(box),
  );
  return {
    box,
    xScale,
    yScale,
    xTicks: dispersionPlaneDecadeTicks(freqLog10Max, xScale),
    yTicks: [0, 0.25, 0.5, 0.75, 1].map((value) => ({
      value,
      position: scaleValue(yScale, value),
      label: formatFixed(value, 2),
      major: true,
    })),
    cells,
    thresholdLine,
    thresholdLabel,
    quadrantPills,
    witnesses,
    labels,
    witnessObstacles,
  };
}

export function formatDispersionPlaneStats(
  totalTypes: number,
  xBins: number,
  yBins: number,
  loudButLocalCount: number,
  loudButLocalMinFrequency: number,
  highDispersionThreshold: number,
): string {
  return `${formatInteger(totalTypes)} types · grid ${formatInteger(xBins)}×${formatInteger(yBins)} · ${formatInteger(loudButLocalCount)} loud-but-local (f ≥ ${formatCompactFrequency(loudButLocalMinFrequency)}, DP ≥ ${formatFixed(highDispersionThreshold, 1)})`;
}

export function buildDispersionRosterGeometry(options: {
  entries: DispersionRosterEntryDatum[];
  highDispersionThreshold: number;
  box?: PlotBox;
}): DispersionRosterGeometry {
  const rowCount = Math.max(1, options.entries.length);
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 92 + (rowCount * 16), {
    top: 34,
    right: 48,
    bottom: 58,
    left: 156,
  });
  const xScale = createScale("linear", [0, 1], [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const thresholdX = scaleValue(xScale, options.highDispersionThreshold);
  const thresholdLine = {
    x1: thresholdX,
    y1: box.margin.top,
    x2: thresholdX,
    y2: box.height - box.margin.bottom,
    className: "cw-plot-threshold-line",
  };
  const thresholdLabel = {
    x: thresholdX,
    y: box.margin.top - 10,
    text: `DP ${formatDpThreshold(options.highDispersionThreshold)}`,
    anchor: "middle" as const,
    className: "cw-plot-label-mono cw-plot-label-muted",
    fontSize: 10.5,
  };
  let aboveThresholdCount = 0;
  const rows = options.entries.map((entry, index) => {
    const y = box.margin.top + 8 + (index * 16);
    const aboveThreshold = entry.dp >= options.highDispersionThreshold;
    if (aboveThreshold) aboveThresholdCount += 1;
    const point = {
      x: scaleValue(xScale, clamp(entry.dp, 0, 1)),
      y,
      rawX: entry.dp,
      rawY: index,
      datum: entry,
      rampIndex: aboveThreshold ? 5 : 2,
      tooltipTitle: entry.token,
      tooltipRows: [
        `frequency ${formatInteger(entry.frequency)}`,
        `document frequency ${formatInteger(entry.documentFrequency)}`,
        `DP ${formatFixed(entry.dp, 3)}`,
      ],
    };
    return {
      entry,
      y,
      guide: {
        x1: box.margin.left,
        y1: y,
        x2: box.width - box.margin.right,
        y2: y,
        className: "cw-plot-roster-guide",
      },
      point,
      tokenLabel: {
        x: box.margin.left - 10,
        y: y + 4,
        text: entry.token,
        anchor: "end" as const,
        className: "cw-plot-label-mono cw-plot-label-muted",
        fontSize: 10.5,
      },
      aboveThreshold,
    };
  });
  return {
    box,
    xScale,
    xTicks: [0, 0.25, 0.5, 0.75, 1].map((value) => ({
      value,
      position: scaleValue(xScale, value),
      label: formatDpTick(value),
      major: true,
    })),
    rows,
    thresholdLine,
    thresholdLabel,
    aboveThresholdCount,
  };
}

export function formatDispersionRosterStats(topCount: number, aboveThresholdCount: number, highDispersionThreshold: number): string {
  return `top ${formatInteger(topCount)} types by frequency · ${formatInteger(aboveThresholdCount)} above DP ${formatDpThreshold(highDispersionThreshold)}`;
}

export function buildLengthHistogramGeometry(options: {
  bins: LengthHistogramBinDatum[];
  documents: number;
  medianTokens: number | null;
  minimumTokens: number | null;
  maximumTokens: number | null;
  box?: PlotBox;
}): LengthHistogramGeometry {
  const binCount = Math.max(1, options.bins.length);
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 360, {
    top: 34,
    right: 40,
    bottom: 72,
    left: 72,
  });
  const xScale = createScale("linear", [0, binCount], [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const maxCount = Math.max(1, ...options.bins.map((bin) => bin.count));
  const yScale = createScale("linear", [0, maxCount], [box.height - box.margin.bottom, box.margin.top]);
  const slotWidth = box.innerWidth / binCount;
  const barGap = Math.min(2, slotWidth * 0.28);
  // Bars are truthful here because the encoded value is linear document count; the power-of-two ranges are categorical slots, not log-scaled bar lengths.
  const bars = options.bins.map((bin, index) => {
    const x = box.margin.left + (index * slotWidth) + (barGap / 2);
    const y = scaleValue(yScale, bin.count);
    return {
      x,
      y,
      width: Math.max(1, slotWidth - barGap),
      height: Math.max(0, box.height - box.margin.bottom - y),
      datum: bin,
      tooltipTitle: `${formatInteger(bin.count)} documents`,
      tooltipRows: [`${formatExactLengthBinBounds(bin)} tokens`],
    };
  });
  const median = options.medianTokens;
  const medianIndex = median === null ? -1 : findLengthHistogramBinIndex(options.bins, median);
  const medianLine = medianIndex < 0
    ? null
    : {
        x1: lengthHistogramMedianX(options.bins[medianIndex]!, median!, medianIndex, slotWidth, box.margin.left),
        y1: box.margin.top,
        x2: lengthHistogramMedianX(options.bins[medianIndex]!, median!, medianIndex, slotWidth, box.margin.left),
        y2: box.height - box.margin.bottom,
        className: "cw-plot-threshold-line",
      };
  const labels = medianLine === null || median === null
    ? []
    : placeLabels(
        [{
          x: medianLine.x1,
          y: box.margin.top + 15,
          text: `median ${formatInteger(median)} tokens`,
          className: "cw-plot-label-mono",
          fontSize: 10.5,
          candidates: [
            { dx: 8, dy: 0, anchor: "start" },
            { dx: -8, dy: 0, anchor: "end" },
            { dx: 0, dy: 18, anchor: "middle" },
          ],
        }],
        bars.map((bar) => ({ kind: "box" as const, box: { x: bar.x, y: bar.y, width: bar.width, height: bar.height } })),
        plotLabelBounds(box),
      );
  return {
    box,
    xScale,
    yScale,
    xTicks: lengthHistogramBoundaryTicks(options.bins, slotWidth, box.margin.left),
    yTicks: linearTicks(yScale, 5),
    bars,
    medianLine,
    labels,
  };
}

export function formatLengthHistogramStats(documents: number, medianTokens: number | null, minimumTokens: number | null, maximumTokens: number | null): string {
  const median = medianTokens === null ? "n/a" : formatInteger(medianTokens);
  const range = minimumTokens === null || maximumTokens === null
    ? "n/a"
    : `${formatInteger(minimumTokens)}–${formatInteger(maximumTokens)}`;
  return `${formatInteger(documents)} documents · median ${median} · range ${range} tokens`;
}

export function buildMattrDistributionGeometry(options: {
  distribution: MattrDistributionPayload;
  median: number | null;
  box?: PlotBox;
}): MattrDistributionGeometry {
  const distribution = options.distribution;
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 380, {
    top: 34,
    right: 40,
    bottom: 104,
    left: 72,
  });
  const domain = mattrOccupiedDomain(distribution.counts, distribution.binWidth);
  const xScale = createScale("linear", domain, [box.margin.left, box.width - box.margin.right]);
  const visibleBins = visibleMattrBins(distribution.counts, distribution.binWidth, domain);
  const maxCount = Math.max(1, ...visibleBins.map((bin) => bin.count));
  const yScale = createScale("linear", [0, maxCount], [box.height - box.margin.bottom, box.margin.top]);
  const bars = visibleBins.map((bin) => {
    const x1 = scaleValue(xScale, bin.lower);
    const x2 = scaleValue(xScale, bin.upper);
    const gap = Math.min(2, Math.max(0, (x2 - x1) * 0.28));
    const y = scaleValue(yScale, bin.count);
    return {
      x: x1 + (gap / 2),
      y,
      width: Math.max(1, x2 - x1 - gap),
      height: Math.max(0, box.height - box.margin.bottom - y),
      datum: bin,
      tooltipTitle: `${formatInteger(bin.count)} documents`,
      tooltipRows: [`MATTR ${formatFixed(bin.lower, 3)}–${formatFixed(bin.upper, 3)}`],
    };
  });
  const median = options.median;
  const medianX = median === null ? null : scaleValue(xScale, clamp(median, domain[0], domain[1]));
  const medianLine = medianX === null
    ? null
    : {
        x1: medianX,
        y1: box.margin.top,
        x2: medianX,
        y2: box.height - box.margin.bottom,
        className: "cw-plot-threshold-line",
      };
  const witnessRequests = [...distribution.lowWitnesses, ...distribution.highWitnesses].map((witness) => ({
    x: scaleValue(xScale, clamp(witness.mattr, domain[0], domain[1])),
    y: box.height - box.margin.bottom + 43,
    text: middleEllipsis(witness.name, 24),
    className: "cw-plot-label-mono cw-plot-label-muted",
    fontSize: 10,
    candidates: [
      { dx: 0, dy: 0, anchor: "middle" as const },
      { dx: 0, dy: 13, anchor: "middle" as const },
      { dx: -8, dy: 0, anchor: "end" as const },
      { dx: 8, dy: 0, anchor: "start" as const },
    ],
  }));
  const witnessLabels = placeLabels(
    witnessRequests,
    [],
    {
      left: box.margin.left,
      top: box.height - box.margin.bottom + 26,
      right: box.width - box.margin.right,
      bottom: box.height - 30,
    },
  );
  const witnessTicks = [...distribution.lowWitnesses, ...distribution.highWitnesses].map((witness, index) => ({
    x: scaleValue(xScale, clamp(witness.mattr, domain[0], domain[1])),
    y1: box.height - box.margin.bottom + 5,
    y2: box.height - box.margin.bottom + 15,
    datum: witness,
    label: witnessLabels[index]!,
    tooltipTitle: witness.name,
    tooltipRows: [
      `MATTR ${formatFixed(witness.mattr, 3)}`,
      `${formatInteger(witness.tokens)} tokens`,
    ],
  }));
  const medianLabel = medianLine === null || median === null
    ? []
    : placeLabels(
        [{
          x: medianLine.x1,
          y: box.margin.top + 15,
          text: `median ${formatFixed(median, 3)}`,
          className: "cw-plot-label-mono",
          fontSize: 10.5,
          candidates: [
            { dx: 8, dy: 0, anchor: "start" },
            { dx: -8, dy: 0, anchor: "end" },
            { dx: 0, dy: 18, anchor: "middle" },
          ],
        }],
        bars.map((bar) => ({ kind: "box" as const, box: { x: bar.x, y: bar.y, width: bar.width, height: bar.height } })),
        plotLabelBounds(box),
      );
  return {
    box,
    xScale,
    yScale,
    xTicks: mattrTicks(domain, xScale),
    yTicks: linearTicks(yScale, 5),
    bars,
    medianLine,
    labels: [...medianLabel, ...witnessLabels],
    witnessTicks,
    domain,
  };
}

export function formatMattrDistributionStats(
  usedDocuments: number,
  median: number | null,
  excludedDocuments: number,
  windowTokens: number,
): string {
  return `${formatInteger(usedDocuments)} documents · median ${median === null ? "n/a" : formatFixed(median, 3)} · ${formatInteger(excludedDocuments)} excluded (< ${formatInteger(windowTokens)} tokens)`;
}

export function formatDispersionProfileStats(totalTypes: number, reachBandCount: number, universalCount: number): string {
  return `${formatInteger(totalTypes)} types · ${formatInteger(reachBandCount)} reach bands · ${formatInteger(universalCount)} types in ≥90% of documents`;
}

export function formatUniversalTypeCaption(tokens: string[], totalCount: number): string | undefined {
  const visibleTokens = tokens.slice(0, 3);
  if (visibleTokens.length === 0) return undefined;
  const remaining = Math.max(0, totalCount - 3);
  return `${visibleTokens.join(" · ")}${remaining > 0 ? ` +${formatInteger(remaining)} more` : ""}`;
}

export function formatInteger(value: number): string {
  return Math.round(value).toLocaleString("en-US");
}

export function formatFixed(value: number, digits: number): string {
  return value.toLocaleString("en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  });
}

export function formatPercent(value: number, digits = 1): string {
  return `${(value * 100).toLocaleString("en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  })}%`;
}

export function formatMillions(value: number): string {
  return `${(value / 1_000_000).toLocaleString("en-US", {
    maximumFractionDigits: 1,
    minimumFractionDigits: 1,
  })}M`;
}

function normaliseLinearDomain(domain: [number, number]): [number, number] {
  let [min, max] = domain;
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [0, 1];
  if (min === max) {
    const padding = Math.abs(min) > 0 ? Math.abs(min) * 0.1 : 1;
    min -= padding;
    max += padding;
  }
  if (min > max) return [max, min];
  return [min, max];
}

function normaliseLogDomain(domain: [number, number]): [number, number] {
  let [min, max] = domain;
  min = Number.isFinite(min) ? Math.max(min, LOG_MINIMUM) : 1;
  max = Number.isFinite(max) ? Math.max(max, min) : min * 10;
  if (min === max) {
    min = Math.max(LOG_MINIMUM, min / 10);
    max *= 10;
  }
  if (min > max) return [max, min];
  return [min, max];
}

function niceNumber(value: number, round: boolean): number {
  const exponent = Math.floor(Math.log10(value));
  const fraction = value / (10 ** exponent);
  let niceFraction: number;
  if (round) {
    if (fraction < 1.5) niceFraction = 1;
    else if (fraction < 3) niceFraction = 2;
    else if (fraction < 7) niceFraction = 5;
    else niceFraction = 10;
  } else if (fraction <= 1) niceFraction = 1;
  else if (fraction <= 2) niceFraction = 2;
  else if (fraction <= 5) niceFraction = 5;
  else niceFraction = 10;
  return niceFraction * (10 ** exponent);
}

function formatTickNumber(value: number): string {
  if (value >= 1_000_000) return `${formatFixed(value / 1_000_000, value % 1_000_000 === 0 ? 0 : 1)}M`;
  if (value >= 1_000) return `${formatFixed(value / 1_000, value % 1_000 === 0 ? 0 : 1)}k`;
  if (value >= 1) return formatInteger(value);
  return value.toLocaleString("en-US", { maximumFractionDigits: 3 });
}

function extent(values: number[]): [number, number] {
  if (values.length === 0) return [1, 10];
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    if (!Number.isFinite(value)) continue;
    min = Math.min(min, value);
    max = Math.max(max, value);
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [1, 10];
  return [min, max];
}

function buildDispersionPlaneCells(
  plane: DispersionPlanePayload,
  xScale: PlotScale,
  yScale: PlotScale,
): DispersionPlaneCellGeometry[] {
  const cells: DispersionPlaneCellGeometry[] = [];
  const maxCellCount = Math.max(1, plane.maxCellCount);
  for (let index = 0; index < plane.counts.length; index += 1) {
    const count = plane.counts[index] ?? 0;
    if (count <= 0) continue;
    const xi = index % plane.xBins;
    const yi = Math.floor(index / plane.xBins);
    const freqLowerLog = (xi / plane.xBins) * Math.max(0, plane.freqLog10Max);
    const freqUpperLog = ((xi + 1) / plane.xBins) * Math.max(0, plane.freqLog10Max);
    const dpLower = yi / plane.yBins;
    const dpUpper = (yi + 1) / plane.yBins;
    const x = scaleValue(xScale, freqLowerLog);
    const x2 = scaleValue(xScale, freqUpperLog);
    const y = scaleValue(yScale, dpUpper);
    const y2 = scaleValue(yScale, dpLower);
    const t = Math.log1p(count) / Math.log1p(maxCellCount);
    const opacity = 0.12 + (0.80 * t);
    const frequencyRange = `${formatFrequencyRangeBoundary(10 ** freqLowerLog)}-${formatFrequencyRangeBoundary(10 ** freqUpperLog)}`;
    const dpRange = `${formatFixed(dpLower, 2)}-${formatFixed(dpUpper, 2)}`;
    cells.push({
      x,
      y,
      width: Math.max(1, x2 - x),
      height: Math.max(1, y2 - y),
      count,
      t,
      opacity,
      fill: densityRampFill(t),
      tooltipTitle: `${formatInteger(count)} types · f ${frequencyRange} · DP ${dpRange}`,
      tooltipRows: [],
    });
  }
  return cells;
}

function buildDispersionPlaneWitnessLabelRequests(
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>,
): LabelPlacementRequest[] {
  return witnesses.map((witness) => ({
    x: witness.x,
    y: witness.y,
    text: witness.datum.token,
    className: "cw-plot-label-mono",
    fontSize: 11,
    candidates: [
      { dx: 10, dy: -8, anchor: "start" },
      { dx: 10, dy: 14, anchor: "start" },
      { dx: -10, dy: -8, anchor: "end" },
      { dx: -10, dy: 14, anchor: "end" },
      { dx: 0, dy: -16, anchor: "middle" },
      { dx: 0, dy: 24, anchor: "middle" },
    ],
  }));
}

function buildDispersionPlaneQuadrantPills(
  box: PlotBox,
  obstacles: LabelObstacle[],
): DispersionPlanePillGeometry[] {
  const bounds = plotLabelBounds(box);
  const pillLabels = placeLabels(
    [
      {
        x: box.margin.left + 12,
        y: box.margin.top + 19,
        text: "RARE & LOCAL",
        className: "cw-plot-quadrant-pill-label",
        fontSize: 10,
        candidates: [
          { dx: 0, dy: 0, anchor: "start" },
          { dx: 0, dy: 24, anchor: "start" },
          { dx: 18, dy: 0, anchor: "start" },
        ],
      },
      {
        x: box.width - box.margin.right - 12,
        y: box.margin.top + 19,
        text: "LOUD BUT LOCAL",
        className: "cw-plot-quadrant-pill-label",
        fontSize: 10,
        candidates: [
          { dx: 0, dy: 0, anchor: "end" },
          { dx: 0, dy: 24, anchor: "end" },
          { dx: -18, dy: 0, anchor: "end" },
        ],
      },
      {
        x: box.margin.left + 12,
        y: box.height - box.margin.bottom - 12,
        text: "QUIET & EVEN",
        className: "cw-plot-quadrant-pill-label",
        fontSize: 10,
        candidates: [
          { dx: 0, dy: 0, anchor: "start" },
          { dx: 0, dy: -24, anchor: "start" },
          { dx: 18, dy: -8, anchor: "start" },
        ],
      },
      {
        x: box.width - box.margin.right - 12,
        y: box.height - box.margin.bottom - 30,
        text: "CORE VOCABULARY",
        className: "cw-plot-quadrant-pill-label",
        fontSize: 10,
        candidates: [
          { dx: 0, dy: 0, anchor: "end" },
          { dx: 0, dy: -26, anchor: "end" },
          { dx: -18, dy: -42, anchor: "end" },
          { dx: 0, dy: -58, anchor: "end" },
        ],
      },
    ],
    obstacles,
    bounds,
  );
  return pillLabels.map((label) => {
    const labelBox = estimateLabelBox(label);
    const box = {
      x: labelBox.x - QUADRANT_PILL_PADDING_X,
      y: label.y - (QUADRANT_PILL_HEIGHT / 2) - 3,
      width: labelBox.width + (QUADRANT_PILL_PADDING_X * 2),
      height: QUADRANT_PILL_HEIGHT,
    };
    return { label, box };
  });
}

function dispersionPlaneDecadeTicks(freqLog10Max: number, scale: PlotScale): PlotTick[] {
  const ticks: PlotTick[] = [];
  const maxPower = Math.floor(Math.max(0, freqLog10Max));
  for (let power = 0; power <= maxPower; power += 1) {
    const decade = 10 ** power;
    const majorValue = Math.log10(decade);
    if (majorValue <= freqLog10Max + 1e-9) {
      ticks.push({
        value: majorValue,
        position: scaleValue(scale, majorValue),
        label: formatTickNumber(decade),
        major: true,
      });
    }
    for (let multiplier = 2; multiplier <= 9; multiplier += 1) {
      const minorValue = Math.log10(multiplier * decade);
      if (minorValue > freqLog10Max + 1e-9) continue;
      ticks.push({
        value: minorValue,
        position: scaleValue(scale, minorValue),
        label: "",
        major: false,
      });
    }
  }
  return ticks.sort((left, right) => left.value - right.value);
}

function densityRampFill(t: number): string {
  const clamped = clamp(t, 0, 1);
  if (clamped <= 0) return "var(--cw-plot-ramp-1)";
  if (clamped >= 1) return "var(--cw-plot-ramp-5)";
  const scaled = clamped * 4;
  const lower = Math.floor(scaled) + 1;
  const upper = Math.min(5, lower + 1);
  const upperWeight = (scaled - Math.floor(scaled)) * 100;
  const lowerWeight = 100 - upperWeight;
  return `color-mix(in srgb, var(--cw-plot-ramp-${lower}) ${formatFixed(lowerWeight, 2)}%, var(--cw-plot-ramp-${upper}) ${formatFixed(upperWeight, 2)}%)`;
}

function formatCompactFrequency(value: number): string {
  if (value >= 1_000_000 && value % 1_000_000 === 0) return `${formatInteger(value / 1_000_000)}M`;
  if (value >= 1_000 && value % 1_000 === 0) return `${formatInteger(value / 1_000)}k`;
  return formatInteger(value);
}

function formatDpThreshold(value: number): string {
  return formatFixed(value, value === 0 || value === 1 ? 0 : 1);
}

function formatDpTick(value: number): string {
  if (value === 0 || value === 1) return formatFixed(value, 0);
  if (value === 0.5) return formatFixed(value, 1);
  return formatFixed(value, 2);
}

function formatFrequencyRangeBoundary(value: number): string {
  if (value >= 1_000_000) return `${formatFixed(value / 1_000_000, value % 1_000_000 === 0 ? 0 : 1)}M`;
  if (value >= 1_000) return `${formatFixed(value / 1_000, value % 1_000 === 0 ? 0 : 1)}k`;
  if (value >= 100) return formatInteger(value);
  if (value >= 10) return formatFixed(value, value % 1 === 0 ? 0 : 1);
  return formatFixed(value, value % 1 === 0 ? 0 : 2);
}

function formatExactLengthBinBounds(bin: LengthHistogramBinDatum): string {
  const upper = bin.upperTokens ?? bin.lowerTokens;
  return `${formatInteger(bin.lowerTokens)}–${formatInteger(upper)}`;
}

function lengthHistogramBoundaryTicks(
  bins: LengthHistogramBinDatum[],
  slotWidth: number,
  left: number,
): PlotTick[] {
  if (bins.length === 0) return [];
  const boundaries = [bins[0]!.lowerTokens, ...bins.map((bin) => bin.upperTokens ?? bin.lowerTokens)];
  return boundaries.map((boundary, index) => ({
    value: index,
    position: left + (index * slotWidth),
    label: index === 0 || index === boundaries.length - 1 || index % 2 === 0
      ? formatCompactTokenBoundary(boundary)
      : "",
    accessibleLabel: formatCompactTokenBoundary(boundary),
    major: true,
  }));
}

function formatCompactTokenBoundary(value: number): string {
  if (value >= 1_048_576 && value % 1_048_576 === 0) return `${formatInteger(value / 1_048_576)}M`;
  if (value >= 1_024 && value % 1_024 === 0) return `${formatInteger(value / 1_024)}k`;
  if (value >= 1_000_000) return `${formatFixed(value / 1_000_000, 1)}M`;
  if (value >= 1_000) return `${formatFixed(value / 1_000, 1)}k`;
  return formatInteger(value);
}

function findLengthHistogramBinIndex(bins: LengthHistogramBinDatum[], medianTokens: number): number {
  const exact = bins.findIndex((bin) => medianTokens >= bin.lowerTokens && medianTokens < (bin.upperTokens ?? Number.POSITIVE_INFINITY));
  if (exact >= 0) return exact;
  if (bins.length === 0) return -1;
  if (medianTokens < bins[0]!.lowerTokens) return 0;
  return bins.length - 1;
}

function lengthHistogramMedianX(
  bin: LengthHistogramBinDatum,
  medianTokens: number,
  index: number,
  slotWidth: number,
  plotLeft: number,
): number {
  const upper = bin.upperTokens ?? bin.lowerTokens + 1;
  const span = Math.max(1, upper - bin.lowerTokens);
  const t = clamp((medianTokens - bin.lowerTokens) / span, 0.08, 0.92);
  return plotLeft + (index + t) * slotWidth;
}

function mattrOccupiedDomain(counts: number[], binWidth: number): [number, number] {
  const first = counts.findIndex((count) => count > 0);
  if (first < 0) return [0, 1];
  let last = counts.length - 1;
  while (last > first && (counts[last] ?? 0) <= 0) last -= 1;
  const lower = Math.max(0, (first - 1) * binWidth);
  const upper = Math.min(1, (last + 2) * binWidth);
  return lower === upper ? [Math.max(0, lower - binWidth), Math.min(1, upper + binWidth)] : [lower, upper];
}

function visibleMattrBins(counts: number[], binWidth: number, domain: [number, number]): MattrDistributionBinDatum[] {
  const bins: MattrDistributionBinDatum[] = [];
  for (let index = 0; index < counts.length; index += 1) {
    const lower = index * binWidth;
    const upper = Math.min(1, lower + binWidth);
    if (upper <= domain[0] || lower >= domain[1]) continue;
    bins.push({
      index,
      lower,
      upper,
      count: counts[index] ?? 0,
    });
  }
  return bins;
}

function mattrTicks(domain: [number, number], scale: PlotScale): PlotTick[] {
  const ticks: PlotTick[] = [];
  const start = Math.ceil((domain[0] - 1e-9) / 0.05) * 0.05;
  const end = Math.floor((domain[1] + 1e-9) / 0.05) * 0.05;
  for (let value = start; value <= end + 1e-9; value += 0.05) {
    const rounded = Number(value.toFixed(2));
    ticks.push({
      value: rounded,
      position: scaleValue(scale, rounded),
      label: formatFixed(rounded, 2),
      major: true,
    });
  }
  return ticks;
}

function middleEllipsis(value: string, maxLength: number): string {
  if (value.length <= maxLength) return value;
  const keep = Math.max(1, maxLength - 1);
  const head = Math.ceil(keep / 2);
  const tail = Math.floor(keep / 2);
  return `${value.slice(0, head)}…${value.slice(value.length - tail)}`;
}

function roundSvg(value: number): string {
  return Number(value.toFixed(2)).toString();
}

function rampIndex(value: number, domain: [number, number], log: boolean): number {
  const [min, max] = domain;
  const start = log ? Math.log10(Math.max(min, LOG_MINIMUM)) : min;
  const end = log ? Math.log10(Math.max(max, LOG_MINIMUM)) : max;
  const current = log ? Math.log10(Math.max(value, LOG_MINIMUM)) : value;
  if (start === end) return 2;
  return Math.max(0, Math.min(4, Math.floor(((current - start) / (end - start)) * 5)));
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function yValueForRank(point: RankFrequencyDatum, mode: "rate" | "raw"): number {
  return mode === "rate" ? point.ratePerMillion : point.frequency;
}

function fitPointsForRank(
  options: {
    yMode: "rate" | "raw";
    totalTokens: number;
    zipfExponent: number | null;
    zipfCoefficient: number | null;
    fittedRankStart: number | null;
    fittedRankEnd: number | null;
  },
  xScale: PlotScale,
  yScale: PlotScale,
): Array<{ x: number; y: number }> {
  if (
    options.zipfExponent === null
    || options.zipfCoefficient === null
    || options.fittedRankStart === null
    || options.fittedRankEnd === null
    || options.fittedRankEnd < options.fittedRankStart
  ) {
    return [];
  }
  const start = Math.max(options.fittedRankStart, xScale.domain[0]);
  const end = Math.min(options.fittedRankEnd, xScale.domain[1]);
  const ranks = start === end ? [start] : [start, Math.sqrt(start * end), end];
  return ranks.map((rank) => {
    const raw = options.zipfCoefficient === null || options.zipfExponent === null
      ? 0
      : options.zipfCoefficient * (rank ** -options.zipfExponent);
    const yValue = options.yMode === "rate" && options.totalTokens > 0
      ? raw / options.totalTokens * 1_000_000
      : raw;
    return {
      x: scaleValue(xScale, rank),
      y: scaleValue(yScale, yValue),
    };
  });
}

function labelRequestForExactFrequency(
  points: Array<PlotPointGeometry<FrequencySpectrumDatum>>,
  frequency: number,
  text: string,
): LabelPlacementRequest | null {
  const point = points.find((candidate) => candidate.datum.frequency === frequency);
  if (!point) return null;
  return {
    x: point.x,
    y: point.y,
    text,
    className: "cw-plot-label-muted",
    candidates: [
      { dx: 10, dy: 4, anchor: "start" },
      { dx: 10, dy: -10, anchor: "start" },
      { dx: -10, dy: 4, anchor: "end" },
      { dx: -10, dy: -10, anchor: "end" },
    ],
  };
}
