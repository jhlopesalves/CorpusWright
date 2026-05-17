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

export type RankFitReasonCode =
  | "top-three"
  | "frequency-one-tail"
  | "outside-fitted-interval"
  | "fit-unavailable-or-invalid";

export type RankFitMembership =
  | {
      kind: "included-in-fit";
      fitted: number;
      residual: number;
    }
  | {
      kind: "shown-not-fitted";
      reasonCode: RankFitReasonCode;
      reason: string;
      fitted: number | null;
    };

export interface RankFrequencyPointGeometry extends PlotPointGeometry<RankFrequencyDatum> {
  fitMembership: RankFitMembership;
}

export interface RankFrequencyGeometry {
  box: PlotBox;
  xScale: PlotScale;
  yScale: PlotScale;
  xTicks: PlotTick[];
  yTicks: PlotTick[];
  points: RankFrequencyPointGeometry[];
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
  className: string;
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

export function padPositiveLogDomain(domain: [number, number], paddingFraction = 0.025): [number, number] {
  let [min, max] = domain;
  if (!Number.isFinite(min) || min <= 0) min = 1;
  if (!Number.isFinite(max) || max <= 0) max = min;
  if (min > max) [min, max] = [max, min];
  const logSpan = Math.log10(max) - Math.log10(min);
  const paddingDecades = logSpan > 0
    ? clamp(logSpan * paddingFraction, 0.015, 0.06)
    : 0.04;
  const factor = 10 ** paddingDecades;
  let paddedMin = Math.max(LOG_MINIMUM, min / factor);
  let paddedMax = max * factor;
  const lowerLabelledDecade = 10 ** Math.floor(Math.log10(min));
  const upperLabelledDecade = 10 ** Math.ceil(Math.log10(max));
  if (min > lowerLabelledDecade && paddedMin <= lowerLabelledDecade) {
    paddedMin = lowerLabelledDecade * (1 + 1e-9);
  }
  if (max < upperLabelledDecade && paddedMax >= upperLabelledDecade) {
    paddedMax = upperLabelledDecade * (1 - 1e-9);
  }
  if (![paddedMin, paddedMax].every(Number.isFinite) || paddedMin <= 0 || paddedMax <= paddedMin) {
    return [Math.max(LOG_MINIMUM, min / 1.1), max * 1.1];
  }
  return [paddedMin, paddedMax];
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
  const analysed = visible.map((point) => ({
    point,
    membership: classifyRankFitMembership(
      point,
      fittedStart,
      fittedEnd,
      options.zipfExponent,
      options.zipfCoefficient,
    ),
  }));
  const fittedPopulation = analysed.filter((entry): entry is {
    point: RankFrequencyDatum;
    membership: Extract<RankFitMembership, { kind: "included-in-fit" }>;
  } => entry.membership.kind === "included-in-fit");
  const plottedEntries = view === "residuals" ? fittedPopulation : analysed;
  const plottedSource = plottedEntries.map((entry) => entry.point);
  const xDomain = extent(plottedSource.map((point) => point.rank));
  const xScale = createScale("log", xDomain, [box.margin.left, box.width - box.margin.right]);
  const residualMagnitude = Math.max(0, ...fittedPopulation.map((entry) => Math.abs(entry.membership.residual)));
  const residualExtent = residualMagnitude < 1e-9 ? 0.05 : residualMagnitude * 1.08;
  const yDomain = view === "residuals"
    ? [-residualExtent, residualExtent] as [number, number]
    : extent(visible.map((point) => rankCurveValue(point, options.yMode)));
  const yScale = createScale(view === "residuals" ? "linear" : "log", yDomain, [box.height - box.margin.bottom, box.margin.top]);
  const points: RankFrequencyPointGeometry[] = plottedEntries.map(({ point, membership }) => {
    const rawY = view === "residuals"
      ? membership.kind === "included-in-fit" ? membership.residual : 0
      : rankCurveValue(point, options.yMode);
    const key = `rank:${point.rank}:${point.token ?? ""}`;
    const shownNotFitted = membership.kind === "shown-not-fitted";
    const fitted = membership.fitted;
    return {
      x: scaleValue(xScale, point.rank),
      y: scaleValue(yScale, rawY),
      rawX: point.rank,
      rawY,
      datum: point,
      rampIndex: 2,
      excluded: shownNotFitted,
      className: shownNotFitted ? "cw-plot-shown-not-fitted" : "cw-plot-included-in-fit",
      fitMembership: membership,
      key,
      pinned: options.pinnedKeys?.has(key) ?? false,
      tooltipTitle: point.token ?? `rank ${formatInteger(point.rank)}`,
      tooltipRows: [
        `rank ${formatInteger(point.rank)}`,
        `${formatInteger(point.frequency)} tokens`,
        `${formatFixed(point.ratePerMillion, 2)} per million`,
        membership.kind === "included-in-fit" ? "fit membership: included in fit" : "fit membership: shown, not fitted",
        membership.kind === "shown-not-fitted" ? `reason: ${membership.reason}` : "",
        fitted === null ? "" : `fitted raw ${formatFixed(fitted, 2)}`,
        fitted === null || options.totalTokens <= 0 ? "" : `fitted ${formatFixed(fitted / options.totalTokens * 1_000_000, 2)} per million`,
        membership.kind === "included-in-fit" ? `log residual ${formatFixed(membership.residual, 3)}` : "",
      ].filter(Boolean),
    };
  });
  const actualFittedStart = fittedPopulation[0]?.point.rank ?? null;
  const actualFittedEnd = fittedPopulation[fittedPopulation.length - 1]?.point.rank ?? null;
  const fitPoints = view === "curve" ? fitPointsForRank({
    ...options,
    fittedRankStart: actualFittedStart,
    fittedRankEnd: actualFittedEnd,
  }, xScale, yScale) : [];
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
        { dx: -12, dy: -28, anchor: "end" },
        { dx: -12, dy: -42, anchor: "end" },
      ],
      key: point.key,
      pinned: point.pinned,
      priority: point.pinned ? 100 : 10 - point.datum.rank,
    })),
    [
      ...pointLabelObstacles(points, 7),
      ...polylineLabelObstacles(fitPoints, 4),
    ],
    plotLabelBounds(box),
    8,
  );
  const fittedRange = view === "curve" && fitVisible && actualFittedStart !== null && actualFittedEnd !== null
    ? {
        x1: scaleValue(xScale, Math.max(actualFittedStart, xScale.domain[0])),
        x2: scaleValue(xScale, Math.min(actualFittedEnd, xScale.domain[1])),
        y: box.margin.top,
        height: box.innerHeight,
        labelX: scaleValue(xScale, Math.max(actualFittedStart, xScale.domain[0])) + 6,
        labelY: box.margin.top + 13,
        label: `Model fitted over ranks ${formatInteger(actualFittedStart)}–${formatInteger(actualFittedEnd)}`,
      }
    : null;
  const fitAnnotations: PlotLabelGeometry[] = view === "curve" && fitVisible && options.labelsMode !== "off"
    ? [
        ...(analysed.some((entry) => entry.membership.kind === "shown-not-fitted" && entry.membership.reasonCode === "top-three")
          ? [{
              x: box.margin.left + 10,
              y: box.margin.top + 18,
              text: "Top three ranks not fitted",
              anchor: "start" as const,
              className: "cw-plot-label-muted cw-plot-fit-membership-annotation",
            }]
          : []),
        ...(analysed.some((entry) => entry.membership.kind === "shown-not-fitted" && entry.membership.reasonCode === "frequency-one-tail")
          ? [{
              x: box.width - box.margin.right - 10,
              y: box.height - box.margin.bottom - 10,
              text: "Frequency-one tail not fitted",
              anchor: "end" as const,
              className: "cw-plot-label-muted cw-plot-fit-membership-annotation",
            }]
          : []),
      ]
    : [];
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
        { label: "Included in fit", kind: "point", shape: "circle" },
        ...(fitPath ? [{ label: "Zipf fit", kind: "line" as const, className: "cw-plot-fit-line" }] : []),
        { label: "Shown, not fitted", kind: "point", shape: "circle", hollow: true, className: "cw-plot-shown-not-fitted" },
      ],
    labels: [...headLabels, ...fitAnnotations],
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

export function classifyRankFitMembership(
  point: RankFrequencyDatum,
  fittedStart: number | null,
  fittedEnd: number | null,
  exponent: number | null,
  coefficient: number | null,
): RankFitMembership {
  const fitted = fittedRankFrequency(point.rank, exponent, coefficient);
  const residual = rankResidual(point, exponent, coefficient);
  if (point.rank <= 3) {
    return shownNotFitted("top-three", "top three ranks are shown but not used to estimate the classic Zipf fit", fitted);
  }
  if (point.frequency === 1) {
    return shownNotFitted("frequency-one-tail", "frequency-one tail observations are shown but not fitted", fitted);
  }
  if (fittedStart === null || fittedEnd === null) {
    return shownNotFitted("fit-unavailable-or-invalid", "fit is unavailable or invalid", fitted);
  }
  if (point.rank < fittedStart || point.rank > fittedEnd) {
    return shownNotFitted("outside-fitted-interval", `rank is outside the supplied fitted interval ${formatInteger(fittedStart)}–${formatInteger(fittedEnd)}`, fitted);
  }
  if (fitted === null || !Number.isFinite(fitted) || fitted <= 0 || residual === null || !Number.isFinite(residual)) {
    return shownNotFitted("fit-unavailable-or-invalid", "fit is unavailable or invalid", fitted);
  }
  return { kind: "included-in-fit", fitted, residual };
}

function shownNotFitted(
  reasonCode: RankFitReasonCode,
  reason: string,
  fitted: number | null,
): Extract<RankFitMembership, { kind: "shown-not-fitted" }> {
  return { kind: "shown-not-fitted", reasonCode, reason, fitted };
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
    { key: "coverage-top10", label: "Top 10", share: values[0]!, rampIndex: 5, className: "cw-plot-coverage-band-top10" },
    { key: "coverage-11-100", label: "Ranks 11–100", share: values[1]!, rampIndex: 4, className: "cw-plot-coverage-band-11-100" },
    { key: "coverage-101-1000", label: "Ranks 101–1,000", share: values[2]!, rampIndex: 3, className: "cw-plot-coverage-band-101-1000" },
    { key: "coverage-1001-10000", label: "Ranks 1,001–10,000", share: values[3]!, rampIndex: 2, className: "cw-plot-coverage-band-1001-10000" },
    { key: "coverage-tail", label: "Remaining vocabulary", share: values[4]!, rampIndex: 0, className: "cw-plot-coverage-band-tail" },
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
  const terminalMilestone = options.totalTypes === undefined
    ? null
    : visible.find((point) => point.k === options.totalTypes && point.k > 10_000) ?? null;
  const milestoneSource = terminalMilestone && !options.milestones.some((point) => point.k === terminalMilestone.k)
    ? [...options.milestones, terminalMilestone]
    : options.milestones;
  const labelledMilestoneRanks = new Set(options.milestones.map((milestone) => milestone.k));
  const milestones: Array<PlotPointGeometry<CoverageDatum>> = milestoneSource
    .map<PlotPointGeometry<CoverageDatum> | null>((milestone) => {
      const share = milestone.share ?? findMilestoneShare(visible, milestone.k);
      if (share === null) return null;
      const datum = { k: milestone.k, share };
      const band = coverageBandForRank(datum.k);
      return {
        x: scaleValue(xScale, xValue(datum.k)),
        y: scaleValue(yScale, datum.share),
        rawX: datum.k,
        rawY: datum.share,
        datum,
        rampIndex: band.rampIndex,
        className: band.className,
        radius: 5.8,
        fillOpacity: 1,
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
  const labelRequests: LabelPlacementRequest[] = options.labelsMode === "off" || options.labelsMode === "target" ? [] : milestones
    .filter((point) => labelledMilestoneRanks.has(point.datum.k))
    .map((point) => ({
    x: point.x,
    y: point.y,
    text: `top ${formatInteger(point.datum.k)} - ${formatPercent(point.datum.share)}`,
    className: `cw-plot-label-muted cw-plot-ramp-${point.rampIndex} ${point.className ?? ""}`.trim(),
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
      className: band.className,
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
  const countDomain = positiveValues.length > 0 ? extent(positiveValues) : [1, 1] as [number, number];
  const xScale = createScale(options.xMode === "share" ? "linear" : "log", options.xMode === "share" ? [0, 1] : padPositiveLogDomain(countDomain), [
    box.margin.left,
    box.width - box.margin.right,
  ]);
  const rowInset = 8;
  const rowTop = box.margin.top + rowInset;
  const rowHeight = Math.max(1, box.height - box.margin.bottom - rowTop - rowInset);
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
  const extremeWitnesses = [distribution.lowWitnesses[0], distribution.highWitnesses[0]]
    .filter((witness): witness is MattrWitnessDatum => witness !== undefined)
    .filter((witness, index, values) => values.findIndex((candidate) => candidate.name === witness.name) === index);
  const witnessLabels = placeLabels(extremeWitnesses.map((witness) => ({
    x: scaleValue(xScale, clamp(witness.mattr, domain[0], domain[1])),
    y: box.height - box.margin.bottom - 8,
    text: middleEllipsis(witness.name, 24),
    className: "cw-plot-label-mono cw-plot-label-muted",
    candidates: [
      { dx: 0, dy: 0, anchor: "middle" as const },
      { dx: 0, dy: -16, anchor: "middle" as const },
      { dx: -8, dy: -8, anchor: "end" as const },
      { dx: 8, dy: -8, anchor: "start" as const },
    ],
  })), bars.map((bar) => ({ kind: "box" as const, box: { x: bar.x, y: bar.y, width: bar.width, height: bar.height } })), plotLabelBounds(box));
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

function permanentDispersionPlaneWitnesses(
  witnesses: Array<PlotPointGeometry<DispersionWitnessDatum>>,
): Array<PlotPointGeometry<DispersionWitnessDatum>> {
  const core = witnesses.filter((witness) => witness.datum.kind === "core").slice(0, 3);
  const loudButLocal = witnesses.filter((witness) => witness.datum.kind === "loudButLocal").slice(0, 2);
  const quietAndEven = witnesses.filter((witness) => witness.datum.kind === "quietAndEven").slice(0, 1);
  return [...core, ...loudButLocal, ...quietAndEven].slice(0, 6);
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
    return {
      x: scaleValue(xScale, rank),
      y: scaleValue(yScale, raw),
    };
  });
}

function rankFrequencyTicks(scale: PlotScale, mode: "rate" | "raw", totalTokens: number): PlotTick[] {
  return logTicks(scale).map((tick) => ({
    ...tick,
    label: !tick.major || mode === "raw"
      ? tick.label
      : formatRatePerMillion(tick.value, totalTokens),
  }));
}

function formatRatePerMillion(frequency: number, totalTokens: number): string {
  if (totalTokens <= 0) return "0";
  return formatTickNumber(frequency / totalTokens * 1_000_000);
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
    className: "cw-plot-label-mono cw-plot-label-muted",
    candidates: [
      { dx: 10, dy: 4, anchor: "start" },
      { dx: 10, dy: -10, anchor: "start" },
      { dx: -10, dy: 4, anchor: "end" },
      { dx: -10, dy: -10, anchor: "end" },
    ],
  };
}
