export type PlotScaleKind = "linear" | "log";
export type PlotAnchor = "start" | "middle" | "end";
export type PlotPointShape = "circle" | "square" | "diamond" | "triangle";
export type PlotLegendKind = "line" | "point" | "density" | "composition" | "threshold";

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
  kind?: PlotLegendKind;
  shape?: PlotPointShape;
  hollow?: boolean;
}

export interface PlotPointGeometry<TDatum = unknown> {
  x: number;
  y: number;
  radius?: number;
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
  shape?: PlotPointShape;
  key?: string;
  pinned?: boolean;
  ariaLabel?: string;
}

export interface PlotLabelGeometry {
  x: number;
  y: number;
  text: string;
  anchor: PlotAnchor;
  className?: string;
  lines?: string[];
  fontSize?: number;
  key?: string;
  pinned?: boolean;
  priority?: number;
  leader?: PlotLineGeometry;
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
  key?: string;
  pinned?: boolean;
  priority?: number;
}

export interface CompositionStripDatum {
  key: string;
  label: string;
  count: number;
  rampIndex: number;
  className?: string;
  tooltipRows?: string[];
  staticDetail?: string;
}

export interface CompositionStripSegmentGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  datum: CompositionStripDatum;
  share: number;
  label: string | null;
  labelBelow: boolean;
  tooltipTitle: string;
  tooltipRows: string[];
  staticDetail: string;
  pinned: boolean;
}

export interface CompositionStripGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  total: number;
  compact: boolean;
  segments: CompositionStripSegmentGeometry[];
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

export interface PlotRectGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
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
  key?: string;
  pinned?: boolean;
  className?: string;
  ariaLabel?: string;
}

export interface PlotFittedRangeGeometry {
  x1: number;
  x2: number;
  y: number;
  height: number;
  labelX: number;
  labelY: number;
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
  zeroLine: PlotLineGeometry | null;
  legend: PlotLegendItem[];
  labels: PlotLabelGeometry[];
  fittedRange: PlotFittedRangeGeometry | null;
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
  compositionStrip: CompositionStripGeometry | null;
  yAxisLabel: string;
}

export interface CoverageDatum {
  k: number;
  share: number;
}

export interface CoverageMilestone {
  k: number;
  share: number | null;
}

export interface CoverageBandShare {
  key: string;
  label: string;
  share: number;
  rampIndex: number;
}

export interface CoverageCrossing {
  k: number;
  share: number;
  x: number;
  y: number;
  targetY: number;
  targetShare: number;
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
  compositionStrip: CompositionStripGeometry | null;
}

export interface DispersionBucketDatum {
  key: string;
  label: string;
  lowerBound?: number;
  upperBound?: number | null;
  count: number;
  share: number | null;
  sampleTypes?: string[];
}

export interface DispersionCalloutGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  title: string;
  detail: string;
}

export interface DispersionGeometry {
  box: PlotBox;
  xScale: PlotScale;
  xTicks: PlotTick[];
  points: Array<PlotPointGeometry<DispersionBucketDatum>>;
  stems: PlotLineGeometry[];
  dividers: PlotLineGeometry[];
  compositionStrip: CompositionStripGeometry | null;
  labels: PlotLabelGeometry[];
  cumulativeCallout: DispersionCalloutGeometry | null;
  xAxisLabel: string;
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
  column: number;
  row: number;
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

export interface DispersionPlaneMarginalGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  count: number;
}

export interface DispersionDensityLegendGeometry {
  mode: "log" | "linear";
  minimum: number;
  maximum: number;
}

export interface DispersionPlaneGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  cells: DispersionPlaneCellGeometry[];
  frequencyMarginal: DispersionPlaneMarginalGeometry[];
  dpMarginal: DispersionPlaneMarginalGeometry[];
  thresholdLines: PlotLineGeometry[];
  thresholdLabels: PlotLabelGeometry[];
  quadrantLabels: PlotLabelGeometry[];
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>;
  labels: PlotLabelGeometry[];
  witnessObstacles: LabelObstacle[];
  densityLegend: DispersionDensityLegendGeometry;
  xAxisLabel: string;
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

export type DistributionDiagnosticView = "histogram" | "ecdf" | "qq";
export type DistributionYMode = "count" | "share";

export interface DistributionResultPayload {
  xScale: "linear" | "log";
  population: number;
  excludedCount: number;
  minimum: number | null;
  firstQuartile: number | null;
  median: number | null;
  thirdQuartile: number | null;
  maximum: number | null;
  histogramBins: Array<{
    lowerBound: number;
    upperBound: number;
    count: number;
    share: number;
    cumulativeShare: number;
  }>;
  ecdfPoints: Array<{
    value: number;
    cumulativeCount: number;
    cumulativeShare: number;
  }>;
  qqPoints: Array<{
    expectedQuantile: number;
    observedValue: number;
    observationRank: number;
    plottingPosition: number;
  }>;
  qqLineIntercept: number | null;
  qqLineSlope: number | null;
  lowWitnesses: Array<{
    documentId: number;
    documentName: string;
    tokenCount: number | bigint;
    metricValue: number;
  }>;
  highWitnesses: Array<{
    documentId: number;
    documentName: string;
    tokenCount: number | bigint;
    metricValue: number;
  }>;
}

export interface DistributionDiagnosticGeometry {
  box: PlotBox;
  view: DistributionDiagnosticView;
  crosshairEnabled: boolean;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  xAxisLabel: string;
  yAxisLabel: string;
  bars: Array<PlotBarGeometry>;
  linePath: PlotPathGeometry | null;
  points: Array<PlotPointGeometry>;
  witnesses: Array<PlotPointGeometry>;
  referencePath: PlotPathGeometry | null;
  medianLine: PlotLineGeometry | null;
  iqrBand: PlotRectGeometry | null;
  labels: PlotLabelGeometry[];
  legend: PlotLegendItem[];
  compositionStrip: CompositionStripGeometry | null;
  zeroTokenSummary: { count: number; x: number; y: number; width: number; height: number } | null;
}

export const DEFAULT_PLOT_WIDTH = 720;
export const DEFAULT_PLOT_HEIGHT = 360;
export const DEFAULT_PLOT_MARGIN: PlotMargin = {
  top: 32,
  right: 40,
  bottom: 60,
  left: 76,
};

const LOG_MINIMUM = 1e-9;
export const AXIS_TITLE_FONT_SIZE = 12;
export const TICK_LABEL_FONT_SIZE = 11.5;
export const ANNOTATION_LABEL_FONT_SIZE = 11.5;
export const MONO_SMALL_LABEL_FONT_SIZE = ANNOTATION_LABEL_FONT_SIZE;
const LABEL_WIDTH_FACTOR = 0.62;
const LABEL_LINE_HEIGHT_FACTOR = 1.3;
const DENSITY_CELL_LABEL_OBSTACLE_T = 0.52;
const WITNESS_KIND_CONTRACT: Record<DispersionWitnessKind, {
  label: string;
}> = {
  core: { label: "Core vocabulary" },
  loudButLocal: { label: "Loud but local" },
  quietAndEven: { label: "Quiet and even" },
  rareAndLocal: { label: "Rare and local" },
};

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
  maximumLabels = Number.POSITIVE_INFINITY,
): PlotLabelGeometry[] {
  const placed: PlotLabelGeometry[] = [];
  const activeObstacles = [...obstacles];
  const rankedLabels = labels
    .map((label, index) => ({ label, index }))
    .sort((left, right) => Number(Boolean(right.label.pinned)) - Number(Boolean(left.label.pinned))
      || (right.label.priority ?? 0) - (left.label.priority ?? 0)
      || left.index - right.index)
    .slice(0, Math.max(0, maximumLabels));
  for (const { label } of rankedLabels) {
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
    const displacement = Math.hypot(placedLabel.x - label.x, placedLabel.y - label.y);
    const withPolicy: PlotLabelGeometry = {
      ...placedLabel,
      key: label.key,
      pinned: label.pinned,
      priority: label.priority,
      leader: displacement >= 24
        ? {
            x1: label.x,
            y1: label.y,
            x2: placedLabel.x,
            y2: placedLabel.y,
            className: "cw-plot-label-leader",
          }
        : undefined,
    };
    placed.push(withPolicy);
    activeObstacles.push({ kind: "box", box: estimateLabelBox(withPolicy) });
  }
  return placed;
}

export function buildCompositionStripGeometry(options: {
  segments: CompositionStripDatum[];
  total: number;
  x: number;
  y: number;
  width: number;
  height?: number;
  compact?: boolean;
  pinnedKeys?: ReadonlySet<string>;
}): CompositionStripGeometry {
  if (options.segments.length < 2 || options.segments.length > 12) {
    throw new RangeError("Composition strips require between two and twelve segments");
  }
  if (options.total < 0 || options.segments.some((segment) => segment.count < 0)) {
    throw new RangeError("Composition strip counts must be non-negative");
  }
  const suppliedTotal = options.segments.reduce((sum, segment) => sum + segment.count, 0);
  if (Math.abs(suppliedTotal - options.total) > 1e-9) {
    throw new RangeError(`Composition strip segments total ${suppliedTotal}, expected ${options.total}`);
  }
  const height = options.height ?? 24;
  let cursor = options.x;
  const segments = options.segments.map((datum, index) => {
    const share = options.total === 0 ? 0 : datum.count / options.total;
    const width = index + 1 === options.segments.length
      ? options.x + options.width - cursor
      : options.width * share;
    const labelBelow = Boolean(options.compact) || width < 92;
    const segment: CompositionStripSegmentGeometry = {
      x: cursor,
      y: options.y,
      width,
      height,
      datum,
      share,
      label: share > 0 ? `${Math.round(share * 100)}%` : null,
      labelBelow,
      tooltipTitle: datum.label,
      tooltipRows: datum.tooltipRows ?? [
        `${formatInteger(datum.count)} of ${formatInteger(options.total)}`,
        formatPercent(share),
      ],
      staticDetail: datum.staticDetail ?? `${formatInteger(datum.count)} · ${formatPercent(share)}`,
      pinned: options.pinnedKeys?.has(datum.key) ?? false,
    };
    cursor += width;
    return segment;
  });
  return {
    x: options.x,
    y: options.y,
    width: options.width,
    height,
    total: options.total,
    compact: Boolean(options.compact),
    segments,
  };
}

export function pointShapeArea(shape: PlotPointShape): number {
  switch (shape) {
    case "circle": return Math.PI * 4.3 ** 2;
    case "square": return 7.6 ** 2;
    case "diamond": return 10.8 * 10.8 / 2;
    case "triangle": return 11.6 * 10 / 2;
  }
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
  view?: "curve" | "residuals";
  fitVisible?: boolean;
  labelsMode?: "examples" | "pinned" | "off";
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): RankFrequencyGeometry {
  const box = options.box ?? createPlotBox();
  const visible = options.points.filter((point) => point.rank > 0 && point.frequency > 0);
  const view = options.view ?? "curve";
  const fitVisible = options.fitVisible ?? true;
  const fittedStart = options.fittedRankStart;
  const fittedEnd = options.fittedRankEnd;
  const fittedPopulation = visible.flatMap((point) => {
    const fitted = fittedRankFrequency(point.rank, options.zipfExponent, options.zipfCoefficient);
    const residual = rankResidual(point, options.zipfExponent, options.zipfCoefficient);
    return rankPointBelongsToFit(point, fittedStart, fittedEnd, fitted, residual)
      ? [{ point, residual: residual! }]
      : [];
  });
  const plottedSource = view === "residuals" ? fittedPopulation.map((entry) => entry.point) : visible;
  const xDomain = extent(plottedSource.map((point) => point.rank));
  const xScale = createScale("log", xDomain, [box.margin.left, box.width - box.margin.right]);
  const residualMagnitude = Math.max(0, ...fittedPopulation.map((entry) => Math.abs(entry.residual)));
  const residualExtent = residualMagnitude < 1e-9 ? 0.05 : residualMagnitude * 1.08;
  const yDomain = view === "residuals"
    ? [-residualExtent, residualExtent] as [number, number]
    : extent(visible.map((point) => rankCurveValue(point, options.yMode)));
  const yScale = createScale(view === "residuals" ? "linear" : "log", yDomain, [box.height - box.margin.bottom, box.margin.top]);
  const points = plottedSource.flatMap((point) => {
    const residual = rankResidual(point, options.zipfExponent, options.zipfCoefficient);
    if (view === "residuals" && residual === null) return [];
    const rawY = view === "residuals" ? residual! : rankCurveValue(point, options.yMode);
    const key = `rank:${point.rank}:${point.token ?? ""}`;
    const fitted = fittedRankFrequency(point.rank, options.zipfExponent, options.zipfCoefficient);
    const excluded = !rankPointBelongsToFit(point, fittedStart, fittedEnd, fitted, residual);
    return [{
      x: scaleValue(xScale, point.rank),
      y: scaleValue(yScale, rawY),
      rawX: point.rank,
      rawY,
      datum: point,
      rampIndex: 2,
      excluded,
      key,
      pinned: options.pinnedKeys?.has(key) ?? false,
      tooltipTitle: point.token ?? `rank ${formatInteger(point.rank)}`,
      tooltipRows: [
        `rank ${formatInteger(point.rank)}`,
        `${formatInteger(point.frequency)} tokens`,
        `${formatFixed(point.ratePerMillion, 2)} per million`,
        fitted === null ? "fit unavailable" : `fitted raw ${formatFixed(fitted, 2)}`,
        fitted === null || options.totalTokens <= 0 ? "fitted rate unavailable" : `fitted ${formatFixed(fitted / options.totalTokens * 1_000_000, 2)} per million`,
        residual === null ? "residual unavailable" : `log residual ${formatFixed(residual, 3)}`,
        excluded ? "excluded from fit" : "included in fit",
      ],
    }];
  });
  const fitPoints = view === "curve" ? fitPointsForRank(options, xScale, yScale) : [];
  const fitPath = view === "curve" && fitVisible && fitPoints.length > 0
    ? { d: pointsToPath(fitPoints), className: "cw-plot-fit-line" }
    : null;
  const automaticPoints = options.labelsMode === "off" || options.labelsMode === "pinned"
    ? []
    : view === "curve" ? [
        ...points.filter((point) => point.datum.token && point.datum.rank <= 3),
        ...points.filter((point) => point.datum.token).slice(-1),
      ] : [];
  const headPoints = points.filter((point) => point.pinned || automaticPoints.includes(point));
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
      key: point.key,
      pinned: point.pinned,
      priority: point.pinned ? 100 : 10 - point.datum.rank,
    })),
    [
      ...pointLabelObstacles(headPoints, 7),
      ...polylineLabelObstacles(fitPoints, 4),
    ],
    plotLabelBounds(box),
    8,
  );
  const fittedRange = view === "curve" && fitVisible && fittedStart !== null && fittedEnd !== null && fittedEnd >= fittedStart
    ? {
        x1: scaleValue(xScale, Math.max(fittedStart, xScale.domain[0])),
        x2: scaleValue(xScale, Math.min(fittedEnd, xScale.domain[1])),
        y: box.margin.top,
        height: box.innerHeight,
        labelX: scaleValue(xScale, Math.max(fittedStart, xScale.domain[0])) + 6,
        labelY: box.margin.top + 13,
        label: "Fitted ranks",
      }
    : null;
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: view === "residuals"
      ? linearTicks(yScale, 7).map((tick) => ({ ...tick, label: formatResidualTick(tick.value) }))
      : logTicks(yScale),
    points,
    fitPath,
    zeroLine: view === "residuals" ? {
      x1: box.margin.left,
      y1: scaleValue(yScale, 0),
      x2: box.width - box.margin.right,
      y2: scaleValue(yScale, 0),
      className: "cw-plot-residual-zero",
    } : null,
    legend: view === "residuals" ? [] : [
        { label: "Observed", kind: "point", shape: "circle" },
        ...(fitPath ? [{ label: "Fitted", kind: "line" as const, className: "cw-plot-fit-line" }] : []),
        { label: "Excluded from fit", kind: "point", shape: "circle", hollow: true, className: "cw-plot-excluded" },
      ],
    labels: headLabels,
    fittedRange,
  };
}

export function rankResidual(
  point: RankFrequencyDatum,
  exponent: number | null,
  coefficient: number | null,
): number | null {
  const fitted = fittedRankFrequency(point.rank, exponent, coefficient);
  if (fitted === null || !Number.isFinite(fitted) || fitted <= 0 || point.frequency <= 0) return null;
  const residual = Math.log(point.frequency) - Math.log(fitted);
  return Number.isFinite(residual) ? residual : null;
}

function rankPointBelongsToFit(
  point: RankFrequencyDatum,
  fittedStart: number | null,
  fittedEnd: number | null,
  fitted: number | null,
  residual: number | null,
): boolean {
  return fittedStart !== null
    && fittedEnd !== null
    && point.rank >= fittedStart
    && point.rank <= fittedEnd
    && point.frequency > 1
    && fitted !== null
    && Number.isFinite(fitted)
    && fitted > 0
    && residual !== null
    && Number.isFinite(residual);
}

function fittedRankFrequency(
  rank: number,
  exponent: number | null,
  coefficient: number | null,
): number | null {
  return exponent === null || coefficient === null || rank <= 0
    ? null
    : coefficient * rank ** (-exponent);
}

export function buildVocabularyGrowthGeometry(options: {
  points: VocabularyGrowthDatum[];
  heapsK: number | null;
  heapsBeta: number | null;
  view?: "growth" | "heaps";
  fitVisible?: boolean;
  labelsMode?: "endpoint" | "checkpoints" | "pinned" | "off";
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): VocabularyGrowthGeometry {
  const box = options.box ?? createPlotBox();
  const view = options.view ?? "growth";
  const fitVisible = options.fitVisible ?? true;
  const visible = options.points.filter((point) => point.tokens > 0 && point.types > 0);
  const fittedValues = vocabularyGrowthFitValues(visible, options.heapsK, options.heapsBeta);
  const visibleFittedValues = fitVisible ? fittedValues : [];
  const xValues = visible.map((point) => point.tokens);
  const yValues = [...visible.map((point) => point.types), ...visibleFittedValues.map((point) => point.types)];
  const xScale = createScale(view === "heaps" ? "log" : "linear", extent(view === "heaps" ? xValues : [0, ...xValues]), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale(view === "heaps" ? "log" : "linear", extent(view === "heaps" ? yValues : [0, ...yValues]), [
    box.height - box.margin.bottom,
    box.margin.top,
  ]);
  const points = visible.map((point) => {
    const key = `growth:${point.tokens}`;
    const fitted = options.heapsK !== null && options.heapsBeta !== null
      ? options.heapsK * point.tokens ** options.heapsBeta
      : null;
    const difference = fitted === null ? null : point.types - fitted;
    return {
    x: scaleValue(xScale, point.tokens),
    y: scaleValue(yScale, point.types),
    rawX: point.tokens,
    rawY: point.types,
    datum: point,
    rampIndex: 2,
    key,
    pinned: options.pinnedKeys?.has(key) ?? false,
    tooltipTitle: `${formatInteger(point.types)} types`,
    tooltipRows: [
      `${formatInteger(point.tokens)} tokens`,
      `${formatInteger(point.types)} observed types`,
      fitted === null ? "fit unavailable" : `${formatFixed(fitted, 1)} fitted types`,
      difference === null ? "difference unavailable" : `${formatFixed(difference, 1)} absolute difference`,
      difference === null || fitted === null || fitted === 0 ? "relative difference unavailable" : `${formatFixed(difference / fitted * 100, 1)}% relative difference`,
    ],
  };
  });
  const linePath = points.length > 0 ? { d: pointsToPath(points), className: "cw-plot-line" } : null;
  const fitPath = visibleFittedValues.length > 0
    ? {
        d: pointsToPath(visibleFittedValues.map((point) => ({
          x: scaleValue(xScale, point.tokens),
          y: scaleValue(yScale, point.types),
        }))),
        className: "cw-plot-fit-line cw-plot-heaps-fit",
      }
    : null;
  const areaPath = view === "growth" && points.length > 0
    ? { d: pointsToAreaPath(points, scaleValue(yScale, 0)), className: "cw-plot-area" }
    : null;
  const last = points[points.length - 1];
  const labelPoints = points.filter((point, index) => point.pinned
    || options.labelsMode === "checkpoints" && (index === 0 || index + 1 === points.length || index % Math.max(1, Math.floor(points.length / 3)) === 0)
    || (options.labelsMode ?? "endpoint") === "endpoint" && index + 1 === points.length);
  const labels = last && options.labelsMode !== "off"
    ? placeLabels(
        labelPoints.map((point) => ({
          x: point.x,
          y: point.y,
          text: `${formatInteger(point.datum.types)} types at ${formatMillions(point.datum.tokens)} tokens`,
          lines: [`${formatInteger(point.datum.types)} types`, `at ${formatMillions(point.datum.tokens)} tokens`],
          className: "cw-plot-label-callout",
          key: point.key,
          pinned: point.pinned,
          priority: point.pinned ? 100 : point === last ? 20 : 5,
          candidates: [
            { dx: -4, dy: 38, anchor: "end" },
            { dx: -14, dy: 50, anchor: "end" },
            { dx: -18, dy: 28, anchor: "end" },
            { dx: -10, dy: 18, anchor: "end" },
            { dx: 12, dy: 38, anchor: "start" },
          ],
        })),
        [
          ...polylineLabelObstacles(points, 5),
          ...pointLabelObstacles(points, 7),
        ],
        plotLabelBounds(box),
        7,
      )
    : [];
  return {
    box,
    xScale,
    yScale,
    xTicks: view === "heaps" ? logTicks(xScale) : linearTicks(xScale, 6),
    yTicks: view === "heaps" ? logTicks(yScale) : linearTicks(yScale, 6),
    points,
    linePath,
    fitPath,
    areaPath,
    legend: fitPath
      ? [
          { label: "Observed", className: "cw-plot-line" },
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
  band3To9Count?: number;
  band10plusCount?: number;
  totalTypes?: number;
  yMode?: "count" | "share";
  bandsVisible?: boolean;
  labelsMode?: "authored" | "pinned" | "off";
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): LexicalSpectrumGeometry {
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, options.totalTypes === undefined ? 360 : 450, {
    top: options.totalTypes === undefined ? 32 : 138,
    right: 40,
    bottom: 60,
    left: 78,
  });
  const visible = options.points.filter((point) => point.frequency > 0 && point.typeCount > 0);
  const xScale = createScale("log", extent(visible.map((point) => point.frequency)), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yValue = (point: FrequencySpectrumDatum) => options.yMode === "share" && options.totalTypes
    ? point.typeCount / options.totalTypes
    : point.typeCount;
  const yScale = createScale("log", extent(visible.map(yValue)), [
    box.height - box.margin.bottom,
    box.margin.top,
  ]);
  const points = visible.map((point) => {
    const key = `spectrum:${point.frequency}`;
    return {
    x: scaleValue(xScale, point.frequency),
    y: scaleValue(yScale, yValue(point)),
    rawX: point.frequency,
    rawY: yValue(point),
    datum: point,
    rampIndex: lexicalSpectrumRampIndex(point.frequency),
    className: point.typeCount === 1 ? "is-frequency-singleton" : "",
    fillOpacity: point.typeCount === 1 ? 0.78 : 0.9,
    key,
    pinned: options.pinnedKeys?.has(key) ?? false,
    tooltipTitle: `${formatInteger(point.typeCount)} types`,
    tooltipRows: [
      `exact frequency ${formatInteger(point.frequency)}`,
      `${formatInteger(point.typeCount)} types`,
      options.totalTypes ? `${formatPercent(point.typeCount / options.totalTypes)} of vocabulary` : "",
      `class ${lexicalSpectrumClassLabel(point.frequency)}`,
    ].filter(Boolean),
  };
  });
  const authored = options.labelsMode === "off" || options.labelsMode === "pinned" ? [] : [
    labelRequestForExactFrequency(points, 1, `hapax · f = 1\n${formatInteger(options.hapaxCount)} types`),
    labelRequestForExactFrequency(points, 2, `dis legomena · f = 2\n${formatInteger(options.disLegomenaCount)} types`),
  ].filter((label): label is LabelPlacementRequest => label !== null);
  const pinned = points.filter((point) => point.pinned).map((point) => ({
    x: point.x,
    y: point.y,
    text: `f=${formatInteger(point.datum.frequency)} · ${formatInteger(point.datum.typeCount)} types`,
    className: `cw-plot-label-callout cw-plot-ramp-${point.rampIndex}`,
    pinned: true,
    priority: 100,
    candidates: [
      { dx: 8, dy: -8, anchor: "start" as const },
      { dx: -8, dy: -8, anchor: "end" as const },
      { dx: 8, dy: 16, anchor: "start" as const },
    ],
  }));
  const labelRequests = [...pinned, ...authored];
  const labels = placeLabels(
    labelRequests,
    pointLabelObstacles(points, 7),
    plotLabelBounds(box),
    7,
  );
  const compositionStrip = options.totalTypes === undefined ? null : buildCompositionStripGeometry({
    total: options.totalTypes,
    x: box.margin.left,
    y: 18,
    width: box.innerWidth,
    height: 22,
    segments: [
      { key: "lexical-hapax", label: "Hapax (1)", count: options.hapaxCount, rampIndex: 0 },
      { key: "lexical-dis", label: "Dis legomena (2)", count: options.disLegomenaCount, rampIndex: 1 },
      { key: "lexical-3-9", label: "3–9 occurrences", count: options.band3To9Count ?? 0, rampIndex: 2 },
      { key: "lexical-10plus", label: "10+ occurrences", count: options.band10plusCount ?? 0, rampIndex: 4 },
    ],
    pinnedKeys: options.pinnedKeys,
  });
  return {
    box,
    xScale,
    yScale,
    xTicks: logTicks(xScale),
    yTicks: logTicks(yScale),
    points,
    bands: options.bandsVisible === false ? [] : buildLexicalSpectrumBands(xScale),
    labels,
    compositionStrip,
    yAxisLabel: options.yMode === "share" ? "Share of vocabulary (log scale)" : "Type count (log scale)",
  };
}

function lexicalSpectrumClassLabel(frequency: number): string {
  if (frequency === 1) return "hapax";
  if (frequency === 2) return "dis legomena";
  if (frequency < 10) return "3–9 occurrences";
  return "10+ occurrences";
}

export function deriveCoverageBandShares(options: {
  totalTypes: number;
  cumulative: CoverageMilestone[];
}): CoverageBandShare[] {
  const suppliedAt = (rank: number): number => {
    if (options.totalTypes === 0) return 0;
    if (options.totalTypes <= rank) return 1;
    return options.cumulative.find((point) => point.k === rank)?.share ?? 0;
  };
  const at10 = suppliedAt(10);
  const at100 = Math.max(at10, suppliedAt(100));
  const at1000 = Math.max(at100, suppliedAt(1_000));
  const at10000 = Math.max(at1000, suppliedAt(10_000));
  const values = [at10, at100 - at10, at1000 - at100, at10000 - at1000, 1 - at10000]
    .map((share) => Math.max(0, Math.min(1, share)));
  const correction = (options.totalTypes === 0 ? 0 : 1) - values.reduce((sum, share) => sum + share, 0);
  values[values.length - 1] = Math.max(0, values[values.length - 1]! + correction);
  return [
    { key: "coverage-top10", label: "Top 10", share: values[0]!, rampIndex: 5 },
    { key: "coverage-11-100", label: "Ranks 11–100", share: values[1]!, rampIndex: 4 },
    { key: "coverage-101-1000", label: "Ranks 101–1,000", share: values[2]!, rampIndex: 3 },
    { key: "coverage-1001-10000", label: "Ranks 1,001–10,000", share: values[3]!, rampIndex: 2 },
    { key: "coverage-tail", label: "Remaining vocabulary", share: values[4]!, rampIndex: 0 },
  ];
}

export function buildCoverageGeometry(options: {
  points: CoverageDatum[];
  milestones: CoverageMilestone[];
  totalTypes?: number;
  xMode?: "topK" | "vocabularyShare";
  targetShare?: number;
  targetCrossing?: { k: number; share: number } | null;
  labelsMode?: "milestones" | "target" | "off";
  bandShares?: CoverageBandShare[];
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): CoverageGeometry {
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, options.bandShares ? 420 : 360, {
    top: options.bandShares ? 112 : 32,
    right: 40,
    bottom: 60,
    left: 78,
  });
  const visible = options.points.filter((point) => point.k > 0 && point.share >= 0);
  const targetShare = options.targetShare ?? 0.5;
  const crossingDatum = options.targetCrossing !== undefined
    ? options.targetCrossing
    : findFirstCoverageCrossing(visible, targetShare);
  const displaySeries = insertCoverageTargetCrossing(visible, crossingDatum);
  const xValue = (k: number) => options.xMode === "vocabularyShare" && options.totalTypes
    ? k / options.totalTypes
    : k;
  const xScale = createScale(options.xMode === "vocabularyShare" ? "linear" : "log", extent(displaySeries.map((point) => xValue(point.k))), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("linear", [0, 1], [box.height - box.margin.bottom, box.margin.top]);
  const points = visible.map((point, index) => ({
    x: scaleValue(xScale, xValue(point.k)),
    y: scaleValue(yScale, point.share),
    rawX: point.k,
    rawY: point.share,
    datum: point,
    rampIndex: 2,
    key: `coverage:${point.k}`,
    pinned: options.pinnedKeys?.has(`coverage:${point.k}`) ?? false,
    tooltipTitle: `top ${formatInteger(point.k)}`,
    tooltipRows: [
      `${formatPercent(point.share)} of tokens`,
      options.totalTypes ? `${formatPercent(point.k / options.totalTypes)} of vocabulary` : "",
      index === 0
        ? `${formatPercent(point.share)} gained at this checkpoint`
        : `${formatPercent(Math.max(0, point.share - visible[index - 1]!.share))} gained since previous checkpoint`,
    ].filter(Boolean),
  }));
  const displayPoints = displaySeries.map((point) => ({
    x: scaleValue(xScale, xValue(point.k)),
    y: scaleValue(yScale, point.share),
  }));
  const linePath = displayPoints.length > 0 ? { d: pointsToPath(displayPoints), className: "cw-plot-line" } : null;
  const areaPath = displayPoints.length > 0
    ? { d: pointsToAreaPath(displayPoints, scaleValue(yScale, 0)), className: "cw-plot-area" }
    : null;
  const milestones: Array<PlotPointGeometry<CoverageDatum>> = options.milestones
    .map<PlotPointGeometry<CoverageDatum> | null>((milestone) => {
      const share = milestone.share ?? findMilestoneShare(visible, milestone.k);
      if (share === null) return null;
      const datum = { k: milestone.k, share };
      return {
        x: scaleValue(xScale, xValue(datum.k)),
        y: scaleValue(yScale, datum.share),
        rawX: datum.k,
        rawY: datum.share,
        datum,
        rampIndex: datum.k <= 10 ? 5
          : datum.k <= 100 ? 4
          : datum.k <= 1000 ? 3
          : datum.k <= 10000 ? 2
          : 0,
        key: `coverage:${datum.k}`,
        pinned: options.pinnedKeys?.has(`coverage:${datum.k}`) ?? false,
        tooltipTitle: `top ${formatInteger(datum.k)}`,
        tooltipRows: [`${formatPercent(datum.share)} of tokens`],
      };
    })
    .filter((point): point is PlotPointGeometry<CoverageDatum> => point !== null);
  const halfCrossing = crossingDatum
    ? {
        k: crossingDatum.k,
        share: crossingDatum.share,
        x: scaleValue(xScale, xValue(crossingDatum.k)),
        y: scaleValue(yScale, crossingDatum.share),
        targetY: scaleValue(yScale, targetShare),
        targetShare,
      }
    : null;
  const labelRequests: LabelPlacementRequest[] = options.labelsMode === "off" || options.labelsMode === "target" ? [] : milestones.map((point) => ({
    x: point.x,
    y: point.y,
    text: `top ${formatInteger(point.datum.k)} - ${formatPercent(point.datum.share)}`,
    className: `cw-plot-label-muted cw-plot-ramp-${point.rampIndex}`,
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
      y: halfCrossing.targetY - 10,
      text: `${formatPercent(targetShare, 0)} target = ${formatInteger(halfCrossing.k)} types (${formatPercent(halfCrossing.share)})`,
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
      ...polylineLabelObstacles(displayPoints, 5),
      ...pointLabelObstacles(milestones, 7),
    ],
    plotLabelBounds(box),
    7,
  );
  const bandTotal = options.bandShares?.reduce((sum, band) => sum + band.share, 0) ?? 0;
  const compositionStrip = options.bandShares && bandTotal > 0 ? buildCompositionStripGeometry({
    total: bandTotal,
    x: box.margin.left,
    y: 18,
    width: box.innerWidth,
    height: 22,
    segments: options.bandShares.map((band) => ({
      key: band.key,
      label: band.label,
      count: band.share,
      rampIndex: band.rampIndex,
      tooltipRows: [`${formatPercent(band.share)} of token mass`],
      staticDetail: `${formatPercent(band.share)} token share`,
    })),
    pinnedKeys: options.pinnedKeys,
  }) : null;
  return {
    box,
    xScale,
    yScale,
    xTicks: xScale.kind === "log" ? logTicks(xScale) : linearTicks(xScale, 6).map((tick) => ({ ...tick, label: formatPercent(tick.value) })),
    yTicks: linearTicks(yScale, 6).map((tick) => ({ ...tick, label: `${Math.round(tick.value * 100)}%` })),
    points,
    linePath,
    areaPath,
    milestones,
    labels,
    halfCrossing,
    compositionStrip,
  };
}

export function insertCoverageTargetCrossing(
  points: CoverageDatum[],
  crossing: { k: number; share: number } | null,
): CoverageDatum[] {
  const display = points.map((point) => ({ ...point }));
  if (crossing && crossing.k > 0 && crossing.share >= 0) {
    const existing = display.findIndex((point) => point.k === crossing.k);
    if (existing >= 0) display[existing] = { k: crossing.k, share: crossing.share };
    else display.push({ k: crossing.k, share: crossing.share });
  }
  return display.sort((left, right) => left.k - right.k);
}

export function buildDispersionGeometry(options: {
  buckets: DispersionBucketDatum[];
  totalTypes?: number;
  totalDocuments?: number;
  xMode?: "count" | "share";
  reachLabels?: "documents" | "percentage";
  labelsMode?: "counts" | "countsShares" | "off";
  cumulativeCallout?: { count: number; examples: string[] } | null;
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): DispersionGeometry {
  const rowCount = Math.max(1, options.buckets.length);
  const stripRailRows = Math.ceil(options.buckets.length / 2);
  const calloutY = 54 + (stripRailRows * 16);
  const plotTop = calloutY + (options.cumulativeCallout ? 58 : 20);
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, plotTop + Math.max(140, rowCount * 26) + 58, {
    top: plotTop,
    right: 36,
    bottom: 48,
    left: 156,
  });
  const buckets = options.buckets;
  const totalTypes = options.totalTypes ?? buckets.reduce((sum, bucket) => sum + bucket.count, 0);
  const value = (bucket: DispersionBucketDatum) => options.xMode === "share"
    ? (bucket.share ?? (totalTypes > 0 ? bucket.count / totalTypes : 0))
    : bucket.count;
  const positiveValues = buckets.filter((bucket) => value(bucket) > 0).map(value);
  const xScale = createScale(options.xMode === "share" ? "linear" : "log", options.xMode === "share" ? [0, 1] : extent(positiveValues), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const rowTop = box.margin.top;
  const rowHeight = Math.max(1, box.height - box.margin.bottom - rowTop);
  const points = buckets.map((bucket, index) => {
    const step = buckets.length > 0 ? rowHeight / buckets.length : rowHeight;
    const y = rowTop + (index + 0.5) * step;
    const plotValue = value(bucket);
    const share = bucket.share ?? (totalTypes > 0 ? bucket.count / totalTypes : 0);
    const documentBounds = dispersionReachBounds(bucket, options.totalDocuments);
    const profileIndex = buckets.length <= 1 ? 5 : Math.round((index / Math.max(1, buckets.length - 1)) * 9);
    const rampIndex = Math.round((profileIndex / 9) * 5);
    const key = `dispersion-reach-${bucket.key}`;
    const examples = (bucket.sampleTypes ?? []).slice(0, Math.min(3, bucket.count));
    const remainingExamples = Math.max(0, bucket.count - examples.length);
    const typeLabel = bucket.count === 1 ? "type" : "types";
    const documentLabel = options.totalDocuments === 1 ? "document" : "documents";
    const reach = bucket.lowerBound === undefined
      ? documentBounds.documents
      : bucket.upperBound === null
        ? `${formatInteger(bucket.lowerBound)}+`
        : bucket.lowerBound === bucket.upperBound
          ? formatInteger(bucket.lowerBound)
          : `${formatInteger(bucket.lowerBound)}–${formatInteger(bucket.upperBound ?? bucket.lowerBound)}`;
    return {
      x: plotValue > 0 ? scaleValue(xScale, plotValue) : scaleValue(xScale, xScale.domain[0]),
      y,
      rawX: plotValue,
      rawY: index,
      datum: bucket,
      rampIndex,
      className: `cw-plot-profile-band-${profileIndex}`,
      excluded: bucket.count === 0,
      key,
      pinned: options.pinnedKeys?.has(key) ?? false,
      tooltipTitle: bucket.label,
      tooltipRows: [
        `${formatInteger(bucket.count)} ${typeLabel}`,
        `${formatPercent(share)} of vocabulary`,
        options.totalDocuments && bucket.lowerBound !== undefined
          ? `Reach: ${reach} of ${formatInteger(options.totalDocuments)} ${documentLabel}`
          : `Reach: ${documentBounds.documents}`,
        examples.length
          ? `Examples: ${examples.join(", ")}${remainingExamples > 0 ? ` +${formatInteger(remainingExamples)} more` : ""}`
          : "",
      ].filter(Boolean),
    };
  });
  const stems = points.map((point) => ({
    x1: box.margin.left,
    y1: point.y,
    x2: point.x,
    y2: point.y,
    className: `cw-plot-lollipop-stem cw-plot-ramp-${point.rampIndex} ${point.className ?? ""}`.trim(),
  }));
  const dividers: PlotLineGeometry[] = [];
  const dotAndStemObstacles = [
    ...pointLabelObstacles(points, 8),
    ...lineLabelObstacles(stems, 5),
  ];
  const rowLabelX = box.margin.left - 12;
  const rowLabels: PlotLabelGeometry[] = points.map((point) => ({
    x: rowLabelX,
    y: point.y + 4,
    text: options.reachLabels === "percentage"
      ? dispersionReachBounds(point.datum, options.totalDocuments).percentage
      : dispersionReachBounds(point.datum, options.totalDocuments).documents,
    anchor: "end",
    className: "cw-plot-label-muted cw-plot-axis-row-label",
  }));
  const valueLabels = options.labelsMode === "off" ? [] : placeLabels(
    points.map((point) => ({
      x: Math.min(point.x + 10, box.width - box.margin.right - 10),
      y: point.y + 4,
      text: options.labelsMode === "countsShares"
        ? `${formatInteger(point.datum.count)} · ${formatPercent(point.datum.share ?? (totalTypes > 0 ? point.datum.count / totalTypes : 0))}`
        : formatInteger(point.datum.count),
      className: "cw-plot-label-mono",
      key: point.key,
      pinned: point.pinned,
      priority: point.pinned ? 100 : 30,
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
  const countedTypes = points.reduce((total, point) => total + point.datum.count, 0);
  const compositionStrip = points.length >= 2 && countedTypes > 0
    ? buildCompositionStripGeometry({
        total: countedTypes,
        x: 32,
        y: 18,
        width: box.width - 32 - box.margin.right,
        height: 22,
        segments: points.map((point) => {
          const bucket = point.datum;
          return {
            key: point.key!,
            label: dispersionReachBounds(bucket, options.totalDocuments).documents,
            count: bucket.count,
            rampIndex: point.rampIndex,
            className: point.className,
            tooltipRows: point.tooltipRows,
          };
        }),
        pinnedKeys: options.pinnedKeys,
      })
    : null;
  const calloutExamples = options.cumulativeCallout?.examples.slice(0, 3) ?? [];
  const cumulativeCallout = options.cumulativeCallout ? {
    x: box.margin.left,
    y: calloutY,
    width: box.innerWidth,
    height: 42,
    title: `${formatInteger(options.cumulativeCallout.count)} types occur in at least 90% of documents`,
    detail: `Cumulative threshold, not an exclusive band${calloutExamples.length ? ` · examples: ${calloutExamples.join(", ")}` : ""}`,
  } : null;
  return {
    box,
    xScale,
    xTicks: xScale.kind === "log"
      ? logTicks(xScale)
      : linearTicks(xScale, 6).map((tick) => ({ ...tick, label: formatPercent(tick.value) })),
    points,
    stems,
    dividers,
    compositionStrip,
    labels: [...rowLabels, ...valueLabels],
    cumulativeCallout,
    xAxisLabel: options.xMode === "share" ? "Share of vocabulary" : "Type count (log scale)",
  };
}

export function dispersionReachBounds(
  bucket: DispersionBucketDatum,
  totalDocuments = 0,
): { documents: string; percentage: string } {
  const lower = bucket.lowerBound ?? 0;
  const upper = bucket.upperBound ?? null;
  const documents = lower > 0
    ? (upper === null ? `${formatInteger(lower)}+ documents` : lower === upper ? `${formatInteger(lower)} ${lower === 1 ? "document" : "documents"}` : `${formatInteger(lower)}–${formatInteger(upper)} documents`)
    : bucket.label;
  if (totalDocuments <= 0 || lower <= 0) return { documents, percentage: "percentage unavailable" };
  const lowShare = lower / totalDocuments;
  const highShare = upper === null ? 1 : Math.min(1, upper / totalDocuments);
  const percentage = upper !== null && lower === upper
    ? `${formatPercent(lowShare, 0)} of documents`
    : `${formatPercent(lowShare, 0)}–${formatPercent(highShare, 0)} of documents`;
  return { documents, percentage };
}

export function buildDispersionPlaneGeometry(options: {
  plane: DispersionPlanePayload;
  totalTypes: number;
  totalTokens?: number;
  frequencyMode?: "raw" | "perMillion";
  densityMode?: "log" | "linear";
  examplesMode?: "labelled" | "points" | "off";
  marginalsVisible?: boolean;
  thresholdsVisible?: boolean;
  pinnedKeys?: ReadonlySet<string>;
  box?: PlotBox;
}): DispersionPlaneGeometry {
  const plane = options.plane;
  const box = options.box ?? createPlotBox(DEFAULT_PLOT_WIDTH, 430, {
    top: 94,
    right: 108,
    bottom: 70,
    left: 164,
  });
  const freqLog10Max = Math.max(0, plane.freqLog10Max);
  const frequencyMode = options.frequencyMode ?? "raw";
  const frequencyOffset = frequencyMode === "perMillion" && options.totalTokens
    ? Math.log10(1_000_000 / options.totalTokens)
    : 0;
  const xScale = createScale("linear", [frequencyOffset, freqLog10Max + frequencyOffset], [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const yScale = createScale("linear", [0, 1], [box.height - box.margin.bottom, box.margin.top]);
  const densityMode = options.densityMode ?? "log";
  const cells = buildDispersionPlaneCells(
    plane,
    xScale,
    yScale,
    densityMode,
    frequencyOffset,
    options.totalTypes,
    options.totalTokens,
    frequencyMode,
  );
  const thresholdY = scaleValue(yScale, plane.highDispersionThreshold);
  const frequencyThresholdX = scaleValue(xScale, Math.log10(1_000) + frequencyOffset);
  const thresholdLines: PlotLineGeometry[] = options.thresholdsVisible === false ? [] : [
    {
      x1: box.margin.left,
      y1: thresholdY,
      x2: box.width - box.margin.right,
      y2: thresholdY,
      className: "cw-plot-threshold-line",
    },
    ...(freqLog10Max >= 3 ? [{
      x1: frequencyThresholdX,
      y1: box.margin.top,
      x2: frequencyThresholdX,
      y2: box.height - box.margin.bottom,
      className: "cw-plot-threshold-line",
    }] : []),
  ];
  const thresholdLabels: PlotLabelGeometry[] = options.thresholdsVisible === false ? [] : [
    {
      x: box.width - box.margin.right - 4,
      y: thresholdY - 8,
      text: `DP ${formatFixed(plane.highDispersionThreshold, 2)}`,
      anchor: "end" as const,
      className: "cw-plot-label-mono cw-plot-label-muted",
      fontSize: 10.5,
    },
    ...(freqLog10Max >= 3 ? [{
      x: frequencyThresholdX + 6,
      y: box.margin.top + 14,
      text: "Raw frequency 1,000",
      anchor: "start" as const,
      className: "cw-plot-label-mono cw-plot-label-muted",
      fontSize: 10.5,
    }] : []),
  ];
  const examplesMode = options.examplesMode ?? "labelled";
  const witnesses = (examplesMode === "off" ? [] : plane.witnesses).map((witness) => {
    const rawX = Math.log10(Math.max(1, witness.frequency)) + frequencyOffset;
    const rawY = clamp(witness.dp, 0, 1);
    const key = `plane:${witness.kind}:${witness.token}`;
    return {
      x: scaleValue(xScale, rawX),
      y: scaleValue(yScale, rawY),
      rawX,
      rawY,
      datum: witness,
      rampIndex: 2,
      shape: "circle" as const,
      className: "cw-plot-plane-witness-mark",
      key,
      pinned: options.pinnedKeys?.has(key) ?? false,
      ariaLabel: `${witness.token}, ${witnessKindLabel(witness.kind)}`,
      tooltipTitle: witness.token,
      tooltipRows: [
        `frequency ${formatInteger(witness.frequency)}`,
        options.totalTokens ? `rate ${formatFixed((witness.frequency / options.totalTokens) * 1_000_000, 2)} per million` : "",
        `document frequency ${formatInteger(witness.documentFrequency)}`,
        `DP ${formatFixed(witness.dp, 3)}`,
        `category ${witnessKindLabel(witness.kind)}`,
      ].filter(Boolean),
    };
  });
  const denseCellObstacles = cells
    .filter((cell) => cell.t >= DENSITY_CELL_LABEL_OBSTACLE_T)
    .map((cell) => ({ kind: "box" as const, box: { x: cell.x, y: cell.y, width: cell.width, height: cell.height } }));
  const witnessDotObstacles = pointLabelObstacles(witnesses, 9);
  const thresholdObstacles: LabelObstacle[] = [
    ...thresholdLabels.map((label) => ({ kind: "box" as const, box: estimateLabelBox(label) })),
    ...lineLabelObstacles(thresholdLines, 4),
  ];
  const pinnedWitnesses = witnesses.filter((witness) => witness.pinned);
  const automaticWitnesses = examplesMode === "labelled"
    ? automaticDispersionPlaneWitnesses(
        witnesses.filter((witness) => !witness.pinned),
        frequencyThresholdX,
        thresholdY,
      )
    : [];
  const witnessLabelRequests = buildDispersionPlaneWitnessLabelRequests([
    ...pinnedWitnesses,
    ...automaticWitnesses,
  ]);
  const preliminaryWitnessLabels = placeLabels(
    witnessLabelRequests,
    [
      ...denseCellObstacles,
      ...witnessDotObstacles,
      ...thresholdObstacles,
    ],
    plotLabelBounds(box),
  );
  const quadrantLabels = buildDispersionPlaneQuadrantLabels(
    box,
    [
      ...witnessDotObstacles,
      ...preliminaryWitnessLabels.map((label) => ({ kind: "box" as const, box: estimateLabelBox(label) })),
      ...thresholdObstacles,
    ],
  );
  const quadrantObstacles = quadrantLabels.map((label) => ({ kind: "box" as const, box: estimateLabelBox(label) }));
  const witnessObstacles = [
    ...denseCellObstacles,
    ...witnessDotObstacles,
    ...thresholdObstacles,
    ...quadrantObstacles,
  ];
  const labels = placeLabels(
    witnessLabelRequests,
    witnessObstacles,
    plotLabelBounds(box),
  );
  const marginalCounts = dispersionPlaneMarginalCounts(plane);
  const marginals = options.marginalsVisible === false
    ? { frequency: [], dp: [] }
    : buildDispersionPlaneMarginalGeometry(marginalCounts, plane, box, xScale, yScale, frequencyOffset);
  return {
    box,
    xScale,
    yScale,
    xTicks: dispersionPlaneDecadeTicks(freqLog10Max, xScale, frequencyOffset),
    yTicks: [0, 0.25, 0.5, 0.75, 1].map((value) => ({
      value,
      position: scaleValue(yScale, value),
      label: value === 0 || value === 1 ? String(value) : formatFixed(value, 2),
      accessibleLabel: value === 0 ? "0, more even" : value === 1 ? "1, more concentrated" : undefined,
      major: true,
    })),
    cells,
    frequencyMarginal: marginals.frequency,
    dpMarginal: marginals.dp,
    thresholdLines,
    thresholdLabels,
    quadrantLabels,
    witnesses,
    labels,
    witnessObstacles,
    densityLegend: {
      mode: densityMode,
      minimum: plane.counts.some((count) => count > 0) ? Math.min(...plane.counts.filter((count) => count > 0)) : 0,
      maximum: plane.maxCellCount,
    },
    xAxisLabel: frequencyMode === "perMillion"
      ? "Frequency per million (log scale)"
      : "Raw frequency (log scale)",
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

export function buildDistributionDiagnosticGeometry(options: {
  result: DistributionResultPayload;
  metric: "documentLength" | "mattr";
  view: DistributionDiagnosticView;
  yMode: DistributionYMode;
  qqReference: "normal" | "lognormal";
  labelsMode?: "extremes" | "pinned" | "off";
  mattrWindow?: number;
  pinnedKeys?: ReadonlySet<string>;
}): DistributionDiagnosticGeometry {
  const hasComposition = options.metric === "mattr";
  const box = createPlotBox(DEFAULT_PLOT_WIDTH, hasComposition ? 420 : 360, {
    top: hasComposition ? 112 : 34,
    right: 42,
    bottom: 62,
    left: 78,
  });
  const result = options.result;
  const effectiveXScale = options.metric === "mattr" ? "linear" : result.xScale;
  const rawValues = [
    result.minimum ?? 0,
    result.maximum ?? 1,
    ...result.histogramBins.flatMap((bin) => [bin.lowerBound, bin.upperBound]),
  ];
  const rawDomain = extent(effectiveXScale === "log" ? rawValues.filter((value) => value > 0) : rawValues);
  if (effectiveXScale === "log") rawDomain[0] = Math.max(1, rawDomain[0]);
  const xKind: PlotScaleKind = options.view === "qq" ? "linear" : effectiveXScale;
  const qqXDomain = extent(result.qqPoints.map((point) => point.expectedQuantile));
  const qqYDomain = extent(result.qqPoints.map((point) => point.observedValue));
  const xScale = createScale(
    xKind,
    options.view === "qq" ? qqXDomain : rawDomain,
    [box.margin.left, box.width - box.margin.right],
  );
  const observedHistogramMaximum = Math.max(0, ...result.histogramBins.map((bin) =>
    options.yMode === "count" ? bin.count : bin.share
  ));
  const histogramMaximum = options.yMode === "share"
    ? pleasantShareCeiling(observedHistogramMaximum)
    : Math.max(1, observedHistogramMaximum);
  const yScale = createScale(
    "linear",
    options.view === "qq" ? qqYDomain : [0, options.view === "histogram" ? histogramMaximum : 1],
    [box.height - box.margin.bottom, box.margin.top],
  );
  const bars = options.view === "histogram"
    ? result.histogramBins.map((bin, index) => {
        const lower = effectiveXScale === "log" ? Math.max(1, bin.lowerBound) : bin.lowerBound;
        const upper = effectiveXScale === "log" ? Math.max(lower, bin.upperBound) : bin.upperBound;
        const x1 = scaleValue(xScale, lower);
        const x2 = scaleValue(xScale, upper);
        const value = options.yMode === "count" ? bin.count : bin.share;
        const y = scaleValue(yScale, value);
        const key = `distribution-bin:${index}:${bin.lowerBound}:${bin.upperBound}`;
        return {
          x: x1 + 1,
          y,
          width: Math.max(1, x2 - x1 - 2),
          height: box.height - box.margin.bottom - y,
          datum: bin,
          key,
          pinned: options.pinnedKeys?.has(key) ?? false,
          tooltipTitle: `${formatDistributionValue(bin.lowerBound, options.metric)}–${formatDistributionValue(bin.upperBound, options.metric)}`,
          tooltipRows: [
            `${formatInteger(bin.count)} documents`,
            formatPercent(bin.share),
            `${formatPercent(bin.cumulativeShare)} cumulative`,
          ],
        };
      })
    : [];
  const ecdfPoints = options.view === "ecdf"
    ? result.ecdfPoints.map((point) => ({
        x: scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, point.value) : point.value),
        y: scaleValue(yScale, point.cumulativeShare),
        rawX: point.value,
        rawY: point.cumulativeShare,
        datum: point,
        rampIndex: 2,
        tooltipTitle: formatDistributionValue(point.value, options.metric),
        tooltipRows: [
          `${formatInteger(point.cumulativeCount)} documents`,
          `${formatPercent(point.cumulativeShare)} cumulative`,
          `${formatPercent(point.cumulativeShare)} percentile`,
        ],
        className: "cw-plot-ecdf-hit-target",
      }))
    : [];
  const qqPoints = options.view === "qq"
    ? result.qqPoints.map((point) => {
        const key = `distribution-qq:${point.observationRank}`;
        return {
          x: scaleValue(xScale, point.expectedQuantile),
          y: scaleValue(yScale, point.observedValue),
          rawX: point.expectedQuantile,
          rawY: point.observedValue,
          datum: point,
          rampIndex: 2,
          radius: options.pinnedKeys?.has(key) ? 4.3 : 2.6,
          key,
          pinned: options.pinnedKeys?.has(key) ?? false,
          className: "cw-plot-qq-point",
          fillOpacity: options.pinnedKeys?.has(key) ? 1 : 0.84,
          tooltipTitle: `quantile ${formatFixed(point.expectedQuantile, 2)}`,
          tooltipRows: [
            `observed ${formatDistributionValue(point.observedValue, options.metric, options.qqReference)}`,
            `plotting position ${formatFixed(point.plottingPosition, 4)}`,
          ],
        };
      })
    : [];
  const points = options.view === "qq" ? qqPoints : ecdfPoints;
  const linePath = options.view === "ecdf" && ecdfPoints.length > 0
    ? { d: pointsToStepPath(ecdfPoints), className: "cw-plot-line cw-plot-ecdf-line" }
    : null;
  const referencePath = options.view === "qq"
    && result.qqLineIntercept !== null
    && result.qqLineSlope !== null
    ? {
        d: pointsToPath(xScale.domain.map((x) => ({
          x: scaleValue(xScale, x),
          y: scaleValue(yScale, result.qqLineIntercept! + result.qqLineSlope! * x),
        }))),
        className: "cw-plot-fit-line cw-plot-qq-reference",
      }
    : null;
  const medianLine = options.view !== "qq" && result.median !== null
    ? {
        x1: scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, result.median) : result.median),
        y1: box.margin.top,
        x2: scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, result.median) : result.median),
        y2: box.height - box.margin.bottom,
        className: "cw-plot-median-line",
      }
    : null;
  const iqrBand = options.view === "histogram"
    && result.firstQuartile !== null
    && result.thirdQuartile !== null
    ? {
        x: scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, result.firstQuartile) : result.firstQuartile),
        y: box.margin.top,
        width: Math.max(0, scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, result.thirdQuartile) : result.thirdQuartile)
          - scaleValue(xScale, effectiveXScale === "log" ? Math.max(1, result.firstQuartile) : result.firstQuartile)),
        height: box.innerHeight,
        className: "cw-plot-iqr-band",
      }
    : null;
  const witnessRows = [...result.lowWitnesses, ...result.highWitnesses]
    .filter((witness, index, rows) => rows.findIndex((row) => row.documentId === witness.documentId) === index);
  const witnesses = options.metric === "mattr" && options.view === "histogram"
    ? witnessRows.map((witness, index) => {
        const key = `distribution-witness:${witness.documentId}`;
        return {
          x: scaleValue(xScale, witness.metricValue),
          y: box.height - box.margin.bottom - 5,
          rawX: witness.metricValue,
          rawY: 0,
          datum: witness,
          rampIndex: 3,
          shape: index < result.lowWitnesses.length ? "triangle" as const : "diamond" as const,
          key,
          pinned: options.pinnedKeys?.has(key) ?? false,
          tooltipTitle: witness.documentName,
          tooltipRows: [
            `MATTR ${formatFixed(witness.metricValue, 3)}`,
            `${formatInteger(Number(witness.tokenCount))} tokens`,
          ],
        };
      })
    : [];
  const labels: PlotLabelGeometry[] = [];
  if (medianLine && result.median !== null) {
    labels.push({
      x: medianLine.x1 + 6,
      y: box.margin.top + 14,
      text: `median ${formatDistributionValue(result.median, options.metric)}`,
      anchor: "start",
      className: "cw-plot-label-callout",
    });
  }
  for (const bar of bars.filter((candidate) => candidate.pinned)) {
    labels.push({
      x: bar.x + bar.width / 2,
      y: Math.max(box.margin.top + 16, bar.y - 8),
      text: `${formatDistributionValue(bar.datum.lowerBound, options.metric)}–${formatDistributionValue(bar.datum.upperBound, options.metric)}`,
      anchor: "middle",
      className: "cw-plot-label-callout",
      pinned: true,
    });
  }
  for (const point of qqPoints.filter((candidate) => candidate.pinned)) {
    labels.push({
      x: point.x + 8,
      y: point.y - 8,
      text: `observed ${formatDistributionValue(point.datum.observedValue, options.metric, options.qqReference)}`,
      anchor: "start",
      className: "cw-plot-label-callout",
      pinned: true,
    });
  }
  const witnessLabelCandidates = witnesses.filter((point) => point.pinned);
  labels.push(...placeLabels(
    witnessLabelCandidates.map((witness) => ({
      x: witness.x,
      y: witness.y,
      text: witness.datum.documentName,
      className: "cw-plot-label-mono",
      pinned: witness.pinned,
      priority: witness.pinned ? 100 : 10,
      candidates: [
        { dx: 0, dy: -14, anchor: "middle" },
        { dx: 9, dy: -18, anchor: "start" },
        { dx: -9, dy: -18, anchor: "end" },
      ],
    })),
    pointLabelObstacles(witnesses, 7),
    plotLabelBounds(box),
    6,
  ));
  const compositionStrip = hasComposition
    ? buildCompositionStripGeometry({
        total: result.population + result.excludedCount,
        x: box.margin.left,
        y: 18,
        width: box.innerWidth,
        height: 22,
        segments: [
          { key: "mattr-eligible", label: `Used · window ${options.mattrWindow ?? 100}`, count: result.population, rampIndex: 2 },
          { key: "mattr-excluded", label: "Excluded · shorter than window", count: result.excludedCount, rampIndex: 5, className: "is-excluded" },
        ],
        pinnedKeys: options.pinnedKeys,
      })
    : null;
  const zeroTokenCount = options.metric === "documentLength" && effectiveXScale === "log"
    ? Math.max(0, ...result.ecdfPoints.filter((point) => point.value === 0).map((point) => point.cumulativeCount))
    : 0;
  const zeroTokenSummary = zeroTokenCount > 0 && options.view !== "qq" ? {
    count: zeroTokenCount,
    x: 8,
    y: box.margin.top + 8,
    width: Math.max(54, box.margin.left - 16),
    height: 34,
  } : null;
  const xTicks = options.view === "qq"
    ? linearTicks(xScale, 6)
    : xScale.kind === "log" ? logTicks(xScale) : linearTicks(xScale, 6);
  const yTicks = linearTicks(yScale, 6).map((tick) => ({
    ...tick,
    label: options.view !== "qq" && options.yMode === "share" ? formatPercent(tick.value) : tick.label,
  }));
  return {
    box,
    view: options.view,
    crosshairEnabled: options.view !== "histogram",
    xScale,
    yScale,
    xTicks,
    yTicks,
    xAxisLabel: options.view === "qq"
      ? "Expected normal quantile"
      : options.metric === "documentLength" ? "Tokens per document" : "Document MATTR",
    yAxisLabel: options.view === "qq"
      ? options.qqReference === "lognormal" ? "Observed log10 document length" : `Observed ${options.metric === "mattr" ? "MATTR" : "document length"}`
      : options.view === "ecdf" ? "Cumulative share of documents"
        : options.yMode === "count" ? "Documents" : "Share of documents",
    bars,
    linePath,
    points,
    witnesses,
    referencePath,
    medianLine,
    iqrBand,
    labels,
    legend: options.view === "qq"
      ? [
          { label: "Observed documents", kind: "point", shape: "circle" },
          { label: "Robust Q1/Q3 reference", kind: "line", className: "cw-plot-fit-line" },
        ]
      : [],
    compositionStrip,
    zeroTokenSummary,
  };
}

export function pleasantShareCeiling(observedMaximum: number): number {
  if (!(observedMaximum > 0)) return 0.1;
  if (observedMaximum >= 0.95) return 1;
  return Math.min(1, niceNumber(observedMaximum * 1.05, false));
}

function pointsToStepPath(points: Array<{ x: number; y: number }>): string {
  return points.map((point, index) => {
    if (index === 0) return `M${roundSvg(point.x)} ${roundSvg(point.y)}`;
    const previous = points[index - 1]!;
    return `L${roundSvg(point.x)} ${roundSvg(previous.y)} L${roundSvg(point.x)} ${roundSvg(point.y)}`;
  }).join(" ");
}

function formatDistributionValue(
  value: number,
  metric: "documentLength" | "mattr",
  qqReference?: "normal" | "lognormal",
): string {
  if (metric === "mattr") return formatFixed(value, 3);
  if (qqReference === "lognormal") return `${formatFixed(value, 3)} log10 tokens`;
  return `${formatInteger(value)} tokens`;
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
    bottom: 72,
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
  const witnessTicks = [...distribution.lowWitnesses, ...distribution.highWitnesses].map((witness) => ({
    x: scaleValue(xScale, clamp(witness.mattr, domain[0], domain[1])),
    y1: box.height - box.margin.bottom + 5,
    y2: box.height - box.margin.bottom + 15,
    datum: witness,
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
    labels: medianLabel,
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
  densityMode: "log" | "linear",
  frequencyOffset: number,
  totalTypes: number,
  totalTokens: number | undefined,
  frequencyMode: "raw" | "perMillion",
): DispersionPlaneCellGeometry[] {
  const cells: DispersionPlaneCellGeometry[] = [];
  const maxCellCount = Math.max(1, plane.maxCellCount);
  for (let index = 0; index < plane.counts.length; index += 1) {
    const count = plane.counts[index] ?? 0;
    const xi = index % plane.xBins;
    const yi = Math.floor(index / plane.xBins);
    const freqLowerLog = (xi / plane.xBins) * Math.max(0, plane.freqLog10Max);
    const freqUpperLog = ((xi + 1) / plane.xBins) * Math.max(0, plane.freqLog10Max);
    const dpLower = yi / plane.yBins;
    const dpUpper = (yi + 1) / plane.yBins;
    const x = scaleValue(xScale, freqLowerLog + frequencyOffset);
    const x2 = scaleValue(xScale, freqUpperLog + frequencyOffset);
    const y = scaleValue(yScale, dpUpper);
    const y2 = scaleValue(yScale, dpLower);
    const t = densityMode === "linear"
      ? count / maxCellCount
      : Math.log1p(count) / Math.log1p(maxCellCount);
    const opacity = count === 0 ? 0 : 1;
    const rawLower = 10 ** freqLowerLog;
    const rawUpper = 10 ** freqUpperLog;
    const frequencyRange = `${formatFrequencyRangeBoundary(rawLower)}–${formatFrequencyRangeBoundary(rawUpper)}`;
    const dpRange = `${formatFixed(dpLower, 2)}–${formatFixed(dpUpper, 2)}`;
    const rateRange = totalTokens
      ? `${formatFixed((rawLower / totalTokens) * 1_000_000, 2)}–${formatFixed((rawUpper / totalTokens) * 1_000_000, 2)}`
      : null;
    cells.push({
      column: xi,
      row: yi,
      x,
      y,
      width: Math.max(1, x2 - x),
      height: Math.max(1, y2 - y),
      count,
      t,
      opacity,
      fill: densityRampFill(t),
      tooltipTitle: count === 0 ? "No types in this cell" : `${formatInteger(count)} types`,
      tooltipRows: [
        `raw frequency ${frequencyRange}`,
        frequencyMode === "perMillion" && rateRange ? `per million ${rateRange}` : "",
        `DP ${dpRange}`,
        `${formatInteger(count)} types`,
        `${formatPercent(totalTypes > 0 ? count / totalTypes : 0)} of vocabulary`,
        `${densityMode === "log" ? "Log" : "Linear"} density display; underlying count unchanged`,
      ].filter(Boolean),
    });
  }
  return cells;
}

export function dispersionPlaneMarginalCounts(
  plane: DispersionPlanePayload,
): { frequency: number[]; dp: number[] } {
  const frequency = new Array<number>(plane.xBins).fill(0);
  const dp = new Array<number>(plane.yBins).fill(0);
  plane.counts.forEach((count, index) => {
    const xi = index % plane.xBins;
    const yi = Math.floor(index / plane.xBins);
    if (xi < frequency.length && yi < dp.length) {
      frequency[xi] += count;
      dp[yi] += count;
    }
  });
  return { frequency, dp };
}

function buildDispersionPlaneMarginalGeometry(
  counts: { frequency: number[]; dp: number[] },
  plane: DispersionPlanePayload,
  box: PlotBox,
  xScale: PlotScale,
  yScale: PlotScale,
  frequencyOffset: number,
): { frequency: DispersionPlaneMarginalGeometry[]; dp: DispersionPlaneMarginalGeometry[] } {
  const maxFrequency = Math.max(1, ...counts.frequency);
  const maxDp = Math.max(1, ...counts.dp);
  const frequency = counts.frequency.map((count, index) => {
    const x = scaleValue(xScale, (index / plane.xBins) * plane.freqLog10Max + frequencyOffset);
    const x2 = scaleValue(xScale, ((index + 1) / plane.xBins) * plane.freqLog10Max + frequencyOffset);
    const height = (count / maxFrequency) * 28;
    return { x, y: box.margin.top - 8 - height, width: Math.max(1, x2 - x), height, count };
  });
  const dp = counts.dp.map((count, index) => {
    const y = scaleValue(yScale, (index + 1) / plane.yBins);
    const y2 = scaleValue(yScale, index / plane.yBins);
    const width = (count / maxDp) * 44;
    return { x: box.width - box.margin.right + 8, y, width, height: Math.max(1, y2 - y), count };
  });
  return { frequency, dp };
}

function buildDispersionPlaneWitnessLabelRequests(
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>,
): LabelPlacementRequest[] {
  return witnesses.map((witness) => ({
    x: witness.x,
    y: witness.y,
    text: witness.datum.token,
    className: "cw-plot-label-mono cw-plot-plane-witness-label",
    key: witness.key,
    pinned: witness.pinned,
    priority: witness.pinned ? 100 : witness.datum.kind === "core" ? 40 : witness.datum.kind === "loudButLocal" ? 30 : 20,
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

function automaticDispersionPlaneWitnesses(
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>,
  frequencyThresholdX: number,
  thresholdY: number,
): Array<PlotPointGeometry<DispersionWitnessDatum>> {
  const quadrants = new Map<string, PlotPointGeometry<DispersionWitnessDatum>>();
  for (const witness of witnesses) {
    const quadrant = `${witness.x >= frequencyThresholdX ? "frequent" : "rare"}-${witness.y <= thresholdY ? "concentrated" : "even"}`;
    const current = quadrants.get(quadrant);
    const distance = Math.hypot(witness.x - frequencyThresholdX, witness.y - thresholdY);
    const currentDistance = current ? Math.hypot(current.x - frequencyThresholdX, current.y - thresholdY) : -1;
    if (!current || distance > currentDistance) quadrants.set(quadrant, witness);
  }
  return [
    "rare-concentrated",
    "frequent-concentrated",
    "rare-even",
    "frequent-even",
  ].flatMap((quadrant) => quadrants.get(quadrant) ?? []);
}

function witnessKindLabel(kind: DispersionWitnessKind): string {
  return WITNESS_KIND_CONTRACT[kind].label;
}

function buildDispersionPlaneQuadrantLabels(
  box: PlotBox,
  obstacles: LabelObstacle[],
): PlotLabelGeometry[] {
  const bounds = plotLabelBounds(box);
  const labels = placeLabels(
    [
      {
        x: box.margin.left + 12,
        y: box.margin.top + 19,
        text: "Rare · concentrated",
        className: "cw-plot-quadrant-label",
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
        text: "Frequent · concentrated",
        className: "cw-plot-quadrant-label",
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
        text: "Rare · even",
        className: "cw-plot-quadrant-label",
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
        text: "Frequent · even",
        className: "cw-plot-quadrant-label",
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
  return labels;
}

function dispersionPlaneDecadeTicks(
  freqLog10Max: number,
  scale: PlotScale,
  frequencyOffset = 0,
): PlotTick[] {
  const ticks: PlotTick[] = [];
  const maxPower = Math.floor(Math.max(0, freqLog10Max));
  for (let power = 0; power <= maxPower; power += 1) {
    const decade = 10 ** power;
    const rawMajorValue = Math.log10(decade);
    const majorValue = rawMajorValue + frequencyOffset;
    if (rawMajorValue <= freqLog10Max + 1e-9) {
      ticks.push({
        value: majorValue,
        position: scaleValue(scale, majorValue),
        label: formatTickNumber(10 ** majorValue),
        accessibleLabel: frequencyOffset === 0
          ? `${formatInteger(decade)} raw occurrences`
          : `${formatFixed(10 ** majorValue, 2)} per million`,
        major: true,
      });
    }
    for (let multiplier = 2; multiplier <= 9; multiplier += 1) {
      const rawMinorValue = Math.log10(multiplier * decade);
      if (rawMinorValue > freqLog10Max + 1e-9) continue;
      const minorValue = rawMinorValue + frequencyOffset;
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
  if (clamped <= 0) return "var(--cw-plot-density-0, var(--cw-plot-ramp-0))";
  if (clamped >= 1) return "var(--cw-plot-density-5, var(--cw-plot-ramp-5))";
  const scaled = clamped * 5;
  const lower = Math.floor(scaled);
  const upper = Math.min(5, lower + 1);
  const upperWeight = (scaled - Math.floor(scaled)) * 100;
  const lowerWeight = 100 - upperWeight;
  return `color-mix(in srgb, var(--cw-plot-density-${lower}, var(--cw-plot-ramp-${lower})) ${formatFixed(lowerWeight, 2)}%, var(--cw-plot-density-${upper}, var(--cw-plot-ramp-${upper})) ${formatFixed(upperWeight, 2)}%)`;
}

function formatCompactFrequency(value: number): string {
  if (value >= 1_000_000 && value % 1_000_000 === 0) return `${formatInteger(value / 1_000_000)}M`;
  if (value >= 1_000 && value % 1_000 === 0) return `${formatInteger(value / 1_000)}k`;
  return formatInteger(value);
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

function roundSvg(value: number): string {
  return Number(value.toFixed(2)).toString();
}

function lexicalSpectrumRampIndex(frequency: number): number {
  if (frequency <= 1) return 0;
  if (frequency === 2) return 1;
  if (frequency <= 9) return 2;
  return 4;
}

function vocabularyGrowthFitValues(
  observed: VocabularyGrowthDatum[],
  heapsK: number | null,
  heapsBeta: number | null,
): VocabularyGrowthDatum[] {
  if (observed.length === 0 || heapsK === null || heapsBeta === null || heapsK <= 0 || heapsBeta <= 0) {
    return [];
  }
  const start = Math.min(...observed.map((point) => point.tokens));
  const end = Math.max(...observed.map((point) => point.tokens));
  if (start <= 0 || end < start) return [];
  const count = Math.max(2, Math.min(48, observed.length * 4));
  return Array.from({ length: count }, (_, index) => {
    const t = index / (count - 1);
    const tokens = start * ((end / start) ** t);
    return { tokens, types: heapsK * (tokens ** heapsBeta) };
  });
}

function buildLexicalSpectrumBands(scale: PlotScale): LexicalSpectrumBandGeometry[] {
  const [domainStart, domainEnd] = scale.domain;
  const boundaries = [
    { start: 1, end: Math.SQRT2, label: "HAPAX", alternate: false },
    { start: Math.SQRT2, end: Math.sqrt(6), label: "DIS", alternate: true },
    { start: Math.sqrt(6), end: Math.sqrt(90), label: "3–9", alternate: false },
    { start: Math.sqrt(90), end: domainEnd, label: "10+", alternate: true },
  ];
  return boundaries.flatMap((band) => {
    const start = Math.max(domainStart, band.start);
    const end = Math.min(domainEnd, band.end);
    if (end <= start) return [];
    const x = scaleValue(scale, start);
    const right = scaleValue(scale, end);
    return [{
      x,
      width: Math.max(0, right - x),
      label: band.label,
      alternate: band.alternate,
    }];
  });
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function fitPointsForRank(
  options: {
    zipfExponent: number | null;
    zipfCoefficient: number | null;
    fittedRankStart: number | null;
    fittedRankEnd: number | null;
    yMode: "rate" | "raw";
    totalTokens: number;
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
    const value = options.yMode === "rate" && options.totalTokens > 0
      ? raw / options.totalTokens * 1_000_000
      : raw;
    return {
      x: scaleValue(xScale, rank),
      y: scaleValue(yScale, value),
    };
  });
}

function rankCurveValue(point: RankFrequencyDatum, mode: "rate" | "raw"): number {
  return mode === "rate" ? point.ratePerMillion : point.frequency;
}

function formatResidualTick(value: number): string {
  if (Math.abs(value) < 1e-12) return "0";
  return value.toLocaleString("en-US", { maximumFractionDigits: 6, minimumFractionDigits: 4 });
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
    className: `cw-plot-label-mono cw-plot-label-muted cw-plot-ramp-${point.rampIndex}`,
    candidates: [
      { dx: 10, dy: 4, anchor: "start" },
      { dx: 10, dy: -10, anchor: "start" },
      { dx: -10, dy: 4, anchor: "end" },
      { dx: -10, dy: -10, anchor: "end" },
    ],
  };
}
