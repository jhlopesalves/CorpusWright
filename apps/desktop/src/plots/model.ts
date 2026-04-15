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
  const visible = options.points.filter((point) => point.rank > 0 && point.frequency > 0);
  const xDomain = extent(visible.map((point) => point.rank));
  const yDomain = extent(visible.map((point) => point.frequency));
  const xScale = createScale("log", xDomain, [box.margin.left, box.width - box.margin.right]);
  const yScale = createScale("log", yDomain, [box.height - box.margin.bottom, box.margin.top]);
  const fittedStart = options.fittedRankStart;
  const fittedEnd = options.fittedRankEnd;
  const points = visible.map((point) => {
    const rawY = point.frequency;
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
        `${formatInteger(point.frequency)} tokens`,
        `${formatFixed(point.ratePerMillion, 2)} per million`,
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
    yTicks: rankFrequencyTicks(yScale, options.yMode, options.totalTokens),
    points,
    fitPath,
    legend: fittedStart !== null && fittedEnd !== null && fitPath
      ? [
          {
            label: `fit: ranks ${formatInteger(fittedStart)}–${formatInteger(fittedEnd)}`,
            className: "cw-plot-fit-line",
          },
          { label: "excluded: top 3 and f=1 tail", className: "cw-plot-excluded" },
        ]
      : [],
    labels: headLabels,
    bracket,
  };
}

export function buildVocabularyGrowthGeometry(options: {
  points: VocabularyGrowthDatum[];
  heapsK: number | null;
  heapsBeta: number | null;
  box?: PlotBox;
}): VocabularyGrowthGeometry {
  const box = options.box ?? createPlotBox();
  const visible = options.points.filter((point) => point.tokens > 0 && point.types > 0);
  const fittedValues = vocabularyGrowthFitValues(visible, options.heapsK, options.heapsBeta);
  const xScale = createScale("log", extent(visible.map((point) => point.tokens)), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("log", extent([...visible.map((point) => point.types), ...fittedValues.map((point) => point.types)]), [
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
  const fitPath = fittedValues.length > 0
    ? {
        d: pointsToPath(fittedValues.map((point) => ({
          x: scaleValue(xScale, point.tokens),
          y: scaleValue(yScale, point.types),
        }))),
        className: "cw-plot-fit-line cw-plot-heaps-fit",
      }
    : null;
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
    fitPath,
    areaPath,
    legend: fitPath
      ? [
          { label: "observed", className: "cw-plot-line" },
          { label: "Heaps fit", className: "cw-plot-fit-line", dashed: true },
        ]
      : [],
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
    rampIndex: lexicalSpectrumRampIndex(point.frequency),
    className: point.typeCount === 1 ? "is-frequency-singleton" : "",
    fillOpacity: point.typeCount === 1 ? 0.42 : 0.6,
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
    bands: buildLexicalSpectrumBands(xScale),
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
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, Math.max(360, 162 + rowCount * 19), {
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
  const compositionBarHeight = 12;
  const compositionBarY = box.margin.top;
  const rowTop = compositionBarY + compositionBarHeight + 28;
  const rowHeight = Math.max(1, box.height - box.margin.bottom - rowTop);
  let virtualRow = 0;
  const points = buckets.map((bucket, index) => {
    if (bucket.highlighted && index > 0) virtualRow += rowGap;
    const y = buckets.length > 1
      ? rowTop + (virtualRow / lastVirtualRow) * rowHeight
      : rowTop + rowHeight / 2;
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
  const exclusivePoints = points.filter((point) => !point.datum.highlighted);
  const countedTypes = exclusivePoints.reduce((total, point) => total + point.datum.count, 0);
  let compositionX = box.margin.left;
  const compositionSegments = exclusivePoints.map((point, index) => {
    const remainingWidth = box.width - box.margin.right - compositionX;
    const share = point.datum.share ?? (countedTypes > 0 ? point.datum.count / countedTypes : 0);
    const width = index === exclusivePoints.length - 1
      ? remainingWidth
      : Math.max(0, box.innerWidth * share);
    const segment = {
      x: compositionX,
      y: compositionBarY,
      width,
      height: compositionBarHeight,
      label: share >= 0.055 ? `${formatPercent(share, 0)}` : null,
      datum: point.datum,
      rampIndex: point.rampIndex,
      tooltipTitle: point.datum.label,
      tooltipRows: [
        `${formatInteger(point.datum.count)} types`,
        `${formatPercent(share)} of vocabulary`,
      ],
    };
    compositionX += width;
    return segment;
  });
  return {
    box,
    xScale,
    xTicks: logTicks(xScale),
    points,
    stems,
    dividers,
    compositionSegments,
    labels: [...rowLabels, ...countLabels],
  };
}

export function buildDispersionPlaneGeometry(options: {
  plane: DispersionPlanePayload;
  totalTypes: number;
  showWitnessLabels?: boolean;
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
  const witnessLabelRequests = options.showWitnessLabels === false
    ? []
    : buildDispersionPlaneWitnessLabelRequests(permanentDispersionPlaneWitnesses(witnesses));
  const preliminaryWitnessLabels = placeLabels(
    witnessLabelRequests,
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
    witnessLabelRequests,
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
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 92 + (rowCount * DISPERSION_ROSTER_ROW_PITCH), {
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
    const y = box.margin.top + (DISPERSION_ROSTER_ROW_PITCH / 2) + (index * DISPERSION_ROSTER_ROW_PITCH);
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

export function formatDispersionRosterStats(
  shownCount: number,
  shippedCount: number,
  aboveThresholdCount: number,
  highDispersionThreshold: number,
): string {
  const countText = shownCount === shippedCount
    ? `top ${formatInteger(shownCount)} types by frequency`
    : `shown ${formatInteger(shownCount)} of ${formatInteger(shippedCount)} frequent types`;
  return `${countText} · ${formatInteger(aboveThresholdCount)} above DP ${formatDpThreshold(highDispersionThreshold)}`;
}

export function buildLengthHistogramGeometry(options: {
  bins: LengthHistogramBinDatum[];
  documents: number;
  medianTokens: number | null;
