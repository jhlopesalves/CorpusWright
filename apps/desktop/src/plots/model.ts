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
  major: boolean;
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

export const DEFAULT_PLOT_WIDTH = 720;
export const DEFAULT_PLOT_HEIGHT = 360;
export const DEFAULT_PLOT_MARGIN: PlotMargin = {
  top: 28,
  right: 34,
  bottom: 54,
  left: 72,
};

const LOG_MINIMUM = 1e-9;
const DEFAULT_LABEL_FONT_SIZE = 11;
const MONO_SMALL_LABEL_FONT_SIZE = 10.5;
const LABEL_WIDTH_FACTOR = 0.62;
const LABEL_LINE_HEIGHT_FACTOR = 1.3;

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
  return label.className?.includes("cw-plot-label-mono") ? MONO_SMALL_LABEL_FONT_SIZE : DEFAULT_LABEL_FONT_SIZE;
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
