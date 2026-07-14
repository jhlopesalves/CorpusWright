import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildDistributionDiagnosticGeometry,
  pleasantShareCeiling,
  type DistributionResultPayload,
} from "../src/plots/model.ts";

const result: DistributionResultPayload = {
  xScale: "linear",
  population: 10,
  excludedCount: 2,
  minimum: 1,
  firstQuartile: 2,
  median: 3,
  thirdQuartile: 5,
  maximum: 9,
  histogramBins: [
    { lowerBound: 1, upperBound: 3, count: 4, share: 0.4, cumulativeShare: 0.4 },
    { lowerBound: 3, upperBound: 6, count: 3, share: 0.3, cumulativeShare: 0.7 },
    { lowerBound: 6, upperBound: 9, count: 3, share: 0.3, cumulativeShare: 1 },
  ],
  ecdfPoints: [
    { value: 1, cumulativeCount: 1, cumulativeShare: 0.1 },
    { value: 3, cumulativeCount: 5, cumulativeShare: 0.5 },
    { value: 9, cumulativeCount: 10, cumulativeShare: 1 },
  ],
  qqPoints: [
    { expectedQuantile: -1, observedValue: 1, observationRank: 1, plottingPosition: 0.05 },
    { expectedQuantile: 0, observedValue: 3, observationRank: 5, plottingPosition: 0.45 },
    { expectedQuantile: 1, observedValue: 9, observationRank: 10, plottingPosition: 0.95 },
  ],
  qqLineIntercept: 3,
  qqLineSlope: 4,
  lowWitnesses: [
    { documentId: 1, documentName: "low.txt", tokenCount: 120, metricValue: 0.31 },
  ],
  highWitnesses: [
    { documentId: 2, documentName: "high.txt", tokenCount: 200, metricValue: 0.81 },
  ],
};

const histogramCount = buildDistributionDiagnosticGeometry({
  result,
  metric: "documentLength",
  view: "histogram",
  yMode: "count",
  qqReference: "normal",
});
const histogramShare = buildDistributionDiagnosticGeometry({
  result,
  metric: "documentLength",
  view: "histogram",
  yMode: "share",
  qqReference: "normal",
});
assert.equal(histogramCount.bars.length, result.histogramBins.length);
assert.equal(histogramCount.crosshairEnabled, false);
assert.equal(histogramShare.bars.length, result.histogramBins.length);
assert.equal(histogramCount.bars[0]?.datum, result.histogramBins[0]);
assert.equal(histogramCount.yScale.domain[1], 4);
assert.equal(histogramShare.yScale.domain[1], 0.5);
assert.equal(pleasantShareCeiling(0.08), 0.1);
assert.equal(pleasantShareCeiling(0.96), 1);
assert(histogramCount.iqrBand && histogramCount.iqrBand.width > 0);
assert(histogramCount.medianLine);
assert(histogramCount.labels.some((label) => label.text.includes("median")));

const ecdf = buildDistributionDiagnosticGeometry({
  result,
  metric: "documentLength",
  view: "ecdf",
  yMode: "share",
  qqReference: "normal",
});
assert.equal(ecdf.bars.length, 0);
assert.equal(ecdf.crosshairEnabled, true);
assert(ecdf.linePath?.d.includes(" L"), "ECDF must use a monotone step path");
assert(ecdf.points.every((point, index, points) => index === 0 || point.y <= points[index - 1]!.y));
assert(ecdf.points.every((point) => point.className === "cw-plot-ecdf-hit-target"));
assert(ecdf.points.every((point) => point.fillOpacity === undefined));
assert(ecdf.points.every((point) => Boolean(point.tooltipTitle) && (point.tooltipRows?.length ?? 0) > 0));

const qq = buildDistributionDiagnosticGeometry({
  result,
  metric: "documentLength",
  view: "qq",
  yMode: "share",
  qqReference: "lognormal",
});
assert.equal(qq.bars.length, 0);
assert.equal(qq.crosshairEnabled, true);
assert.equal(qq.points.length, result.qqPoints.length);
assert(qq.referencePath);
assert(qq.points.every((point) => point.className === "cw-plot-qq-point"));
assert(qq.points.every((point) => point.radius === 2.6));
assert(qq.points.every((point) => point.fillOpacity === 0.84));
assert.equal(qq.yAxisLabel, "Observed log10 document length");
assert(qq.yTicks.every((tick) => /^-?\d+\.\d{2}$/.test(tick.label)));
assert.deepEqual(qq.legend.map((item) => item.label), ["Observed documents", "Robust Q1/Q3 reference"]);
assert.equal(ecdf.points.at(-1)?.rawY, 1, "ECDF must reach its final cumulative value");
assert(ecdf.linePath?.d.endsWith(`${ecdf.points.at(-1)?.x} ${ecdf.points.at(-1)?.y}`));

const pinnedQq = buildDistributionDiagnosticGeometry({
  result,
  metric: "documentLength",
  view: "qq",
  yMode: "share",
  qqReference: "lognormal",
  pinnedKeys: new Set(["distribution-qq:1"]),
});
assert.equal(pinnedQq.points[0]?.radius, 4.3);
assert.equal(pinnedQq.points[0]?.fillOpacity, 1);
assert.equal(pinnedQq.points[0]?.pinned, true);

const mattr = buildDistributionDiagnosticGeometry({
  result,
  metric: "mattr",
  view: "histogram",
  yMode: "share",
  qqReference: "normal",
  labelsMode: "extremes",
  mattrWindow: 100,
});
assert.equal(mattr.compositionStrip?.total, 12);
assert.deepEqual(mattr.compositionStrip?.segments.map((segment) => segment.datum.count), [10, 2]);
assert.equal(mattr.witnesses.length, 2);
assert(!mattr.labels.some((label) => label.text === "low.txt" || label.text === "high.txt"));
const pinnedMattr = buildDistributionDiagnosticGeometry({
  result,
  metric: "mattr",
  view: "histogram",
  yMode: "share",
  qqReference: "normal",
  labelsMode: "extremes",
  mattrWindow: 100,
  pinnedKeys: new Set(["distribution-witness:1"]),
});
assert(pinnedMattr.labels.some((label) => label.text === "low.txt" && label.pinned));

const logResult = { ...result, xScale: "log" as const };
const logHistogram = buildDistributionDiagnosticGeometry({
  result: logResult,
  metric: "documentLength",
  view: "histogram",
  yMode: "share",
  qqReference: "lognormal",
});
assert.equal(logHistogram.xScale.kind, "log");
assert(logHistogram.bars.every((bar) => bar.width > 0), "returned numeric log boundaries must drive bar widths");

const zeroLogHistogram = buildDistributionDiagnosticGeometry({
  result: {
    ...logResult,
    minimum: 0,
    ecdfPoints: [
      { value: 0, cumulativeCount: 2, cumulativeShare: 0.2 },
      ...result.ecdfPoints,
    ],
  },
  metric: "documentLength",
  view: "histogram",
  yMode: "share",
  qqReference: "lognormal",
});
assert(zeroLogHistogram.xScale.domain[0] >= 1);
assert.equal(zeroLogHistogram.zeroTokenSummary?.count, 2);

const invalidLogMattr = buildDistributionDiagnosticGeometry({
  result: logResult,
  metric: "mattr",
  view: "histogram",
  yMode: "share",
  qqReference: "normal",
});
assert.equal(invalidLogMattr.xScale.kind, "linear", "MATTR geometry normalises stale logarithmic state");

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const interactions = readFileSync(new URL("../src/styles/plots/interactions.css", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
assert.match(shell, /queryOverviewDistribution\(/);
assert.match(shell, /queryOverviewDistributionEvidence\(/);
assert.match(shell, /manualTimer[\s\S]*setTimeout\(\(\) => void requestDistribution\(\), 180\)/);
assert.match(shell, /binsControl\.hidden = state\.view !== "histogram"/);
assert.match(shell, /controls\.append\(viewControl, binsControl, manualControl, \.\.\.\(isLength \? \[scaleControl\] : \[\]\)\)/);
assert.doesNotMatch(shell, /Figure options|optionsDisclosure|createOverviewFigureOptions/);
assert.match(shell, /xScale: isLength \? state\.xScale : "linear"/);
assert.doesNotMatch(shell, /Q–Q reference/);
assert.match(shell, /upperInclusive: index \+ 1 === result\.histogramBins\.length/);
assert.match(render, /renderCompositionStrip\(svg, geometry\.compositionStrip\)/);
assert.match(interactions, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*animation:\s*none/);
assert.match(figure, /prepareStaticExportClone\(exportedSvg\)/);
assert.match(render, /renderPoints\(svg, geometry\.witnesses[\s\S]*if \(geometry\.referencePath\) renderPath/);
assert.match(render, /geometry\.view === "ecdf"\) renderEcdfHitTargets\(svg, geometry\.points\)/);
assert.match(render, /cw-plot-ecdf-hit-target cw-plot-hover-target/);
assert.match(interactions, /\.cw-plot-ecdf-hit-target[\s\S]*fill:\s*transparent;[\s\S]*stroke:\s*none;[\s\S]*opacity:\s*0;[\s\S]*pointer-events:\s*all;/);
assert.match(figure, /cw-plot-ecdf-hit-targets/);
assert.match(marks, /cw-plot-composition-segment\.is-excluded[\s\S]*cw-plot-excluded/);
assert.match(marks, /cw-plot-qq-point:not\(\.is-pinned\)[\s\S]*stroke: none;[\s\S]*animation: none/);
assert.doesNotMatch(`${shell}\n${marks}`, /MATTR[\s\S]{0,80}(?:green|red)|(?:green|red)[\s\S]{0,80}MATTR/i);
