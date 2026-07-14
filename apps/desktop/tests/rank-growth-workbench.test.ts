import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildRankFrequencyGeometry,
  buildVocabularyGrowthGeometry,
  estimateLabelBox,
  rankResidual,
} from "../src/plots/model.ts";

const rankPoints = [
  { rank: 1, frequency: 1_000, ratePerMillion: 100_000, token: "the" },
  { rank: 2, frequency: 650, ratePerMillion: 65_000, token: "of" },
  { rank: 3, frequency: 420, ratePerMillion: 42_000, token: "and" },
  { rank: 4, frequency: 300, ratePerMillion: 30_000, token: "corpus" },
  { rank: 5, frequency: 220, ratePerMillion: 22_000, token: "analysis" },
  { rank: 20, frequency: 1, ratePerMillion: 100, token: "tail" },
];

const curve = buildRankFrequencyGeometry({
  points: rankPoints,
  yMode: "rate",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 5,
  view: "curve",
  fitVisible: true,
  labelsMode: "examples",
});
assert.equal(new Set(curve.points.filter((point) => point.fitMembership.kind === "included-in-fit").map((point) => point.rampIndex)).size, 1);
assert(curve.points.filter((point) => point.fitMembership.kind === "shown-not-fitted").every((point) => point.excluded));
assert(curve.fitPath);
assert(curve.fittedRange);
assert(curve.fittedRange.y >= curve.box.margin.top);
assert(curve.fittedRange.y + curve.fittedRange.height <= curve.box.height - curve.box.margin.bottom);
assert.equal(curve.points[0]?.rawY, 100_000, "authored curve geometry uses frequency per million");
assert.deepEqual(curve.legend.map((item) => item.label), ["Included in fit", "Zipf fit", "Shown, not fitted"]);
assert(curve.legend.some((item) => item.label === "Shown, not fitted" && item.hollow));
assert.equal(curve.fittedRange?.label, "Model fitted over ranks 4–5");
assert.equal(curve.points[0]?.fitMembership.kind, "shown-not-fitted");
assert.equal(curve.points[0]?.fitMembership.kind === "shown-not-fitted" ? curve.points[0].fitMembership.reasonCode : null, "top-three");
assert.equal(curve.points.at(-1)?.fitMembership.kind === "shown-not-fitted" ? curve.points.at(-1)!.fitMembership.reasonCode : null, "frequency-one-tail");
assert(curve.labels.some((label) => label.text === "Top three ranks not fitted"));
assert(curve.labels.some((label) => label.text === "Frequency-one tail not fitted"));

const downsampledCurve = buildRankFrequencyGeometry({
  points: [
    ...rankPoints.slice(0, 3),
    { rank: 205, frequency: 500, ratePerMillion: 500, token: "sample-start" },
    { rank: 24_283, frequency: 2, ratePerMillion: 2, token: "sample-end" },
    { rank: 40_000, frequency: 1, ratePerMillion: 1, token: "tail" },
  ],
  yMode: "rate",
  totalTokens: 1_000_000,
  zipfExponent: 1,
  zipfCoefficient: 100_000,
  fittedRankStart: 4,
  fittedRankEnd: 24_292,
  view: "curve",
  fitVisible: true,
  labelsMode: "examples",
});
assert.equal(downsampledCurve.fittedRange?.label, "Model fitted over ranks 4–24,292");

const denseRankPoints = Array.from({ length: 80 }, (_, index) => ({
  rank: index + 1,
  frequency: Math.max(1, Math.round(100_000 / (index + 1) ** 1.08)),
  ratePerMillion: Math.max(1, Math.round(100_000 / (index + 1) ** 1.08)),
  token: `token-${index + 1}`,
}));
const denseCurve = buildRankFrequencyGeometry({
  points: denseRankPoints,
  yMode: "rate",
  totalTokens: 1_000_000,
  zipfExponent: 1.08,
  zipfCoefficient: 100_000,
  fittedRankStart: 4,
  fittedRankEnd: 70,
  view: "curve",
  labelsMode: "examples",
});
const tailLabel = denseCurve.labels.find((label) => label.text === "token-80")!;
const tailBox = estimateLabelBox(tailLabel);
assert(!denseCurve.points.some((point) =>
  point.x >= tailBox.x && point.x <= tailBox.x + tailBox.width
  && point.y >= tailBox.y && point.y <= tailBox.y + tailBox.height
), "dense tail label must not cover observed points");

const hiddenFit = buildRankFrequencyGeometry({
  points: rankPoints,
  yMode: "raw",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 5,
  view: "curve",
  fitVisible: false,
  labelsMode: "off",
});
assert.equal(hiddenFit.fitPath, null);
assert(!hiddenFit.legend.some((item) => item.label === "Zipf fit"));
assert.equal(hiddenFit.labels.length, 0);

assert.equal(rankResidual(rankPoints[3]!, 1, 1_200), 0);
const residuals = buildRankFrequencyGeometry({
  points: rankPoints,
  yMode: "raw",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 5,
  view: "residuals",
  fitVisible: true,
  labelsMode: "pinned",
  pinnedKeys: new Set(["rank:5:analysis"]),
});
assert.equal(residuals.yScale.kind, "linear");
assert(residuals.zeroLine);
assert.equal(residuals.fitPath, null);
assert.deepEqual(residuals.points.map((point) => point.datum.rank), [4, 5]);
assert(residuals.points.every((point) => !point.excluded));
assert(residuals.points.every((point) => point.fitMembership.kind === "included-in-fit"));
assert.equal(residuals.legend.length, 0);
assert.equal(residuals.yScale.domain[0], -residuals.yScale.domain[1]);
assert.equal(residuals.zeroLine?.y1, residuals.box.margin.top + residuals.box.innerHeight / 2);
assert.equal(residuals.labels.length, 1, "residual labels are laid out without sampling an off-domain fitted curve");
assert(residuals.labels.some((label) => label.text === "analysis" && label.pinned));
assert(residuals.points[0]?.tooltipRows?.some((row) => row.includes("log residual")));
assert(!residuals.yTicks.some((tick) => tick.label === "-0"));
const rateResiduals = buildRankFrequencyGeometry({
  points: rankPoints,
  yMode: "rate",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 5,
  view: "residuals",
});
assert.deepEqual(rateResiduals.points.map((point) => point.rawY), residuals.points.map((point) => point.rawY), "residual geometry is unit invariant");

const unavailable = buildRankFrequencyGeometry({
  points: rankPoints,
  yMode: "raw",
  totalTokens: 10_000,
  zipfExponent: null,
  zipfCoefficient: null,
  fittedRankStart: null,
  fittedRankEnd: null,
  view: "residuals",
});
assert.equal(unavailable.points.length, 0);

const growthPoints = [
  { tokens: 1_000, types: 120 },
  { tokens: 10_000, types: 600 },
  { tokens: 100_000, types: 2_200 },
];
const growth = buildVocabularyGrowthGeometry({
  points: growthPoints,
  heapsK: 1.5,
  heapsBeta: 0.7,
  view: "growth",
  fitVisible: true,
  labelsMode: "endpoint",
});
assert.equal(growth.xScale.kind, "linear");
assert.equal(growth.yScale.kind, "linear");
assert(growth.areaPath);
assert(growth.fitPath?.className.includes("cw-plot-heaps-fit"));
const endpointBox = estimateLabelBox(growth.labels[0]!);
assert(endpointBox.x >= growth.box.margin.left);
assert(endpointBox.x + endpointBox.width <= growth.box.width - growth.box.margin.right + 1e-9);

const heaps = buildVocabularyGrowthGeometry({
  points: growthPoints,
  heapsK: 1.5,
  heapsBeta: 0.7,
  view: "heaps",
  fitVisible: true,
  labelsMode: "pinned",
  pinnedKeys: new Set(["growth:10000"]),
});
assert.equal(heaps.xScale.kind, "log");
assert.equal(heaps.yScale.kind, "log");
assert.equal(heaps.areaPath, null);
assert(heaps.labels.some((label) => label.pinned));

const hiddenGrowthFit = buildVocabularyGrowthGeometry({
  points: growthPoints,
  heapsK: 10_000,
  heapsBeta: 0.9,
  view: "growth",
  fitVisible: false,
});
assert.equal(hiddenGrowthFit.fitPath, null);
assert.equal(hiddenGrowthFit.yScale.domain[1], 2_200, "hidden fit values cannot control the visible domain");

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
assert.match(shell, /How quickly does frequency fall as vocabulary rank increases\?/);
assert.match(shell, /top three ranks and frequency-one tail remain visible but are not used to estimate the fit/);
assert.match(shell, /How quickly does the corpus encounter new types as tokens accumulate\?/);
assert.match(shell, /Both axes are logarithmic in Curve view/);
assert.match(shell, /stable source order/);
assert.match(shell, /Zipf fit f\(r\) = C r\^-alpha/);
assert.match(shell, /V\(N\) = K N\^beta/);
assert.doesNotMatch(shell, /(?:alpha|beta)[\s\S]{0,80}type\s*=\s*["'](?:range|number)["']/i);
assert.match(marks, /cw-plot-heaps-fit[\s\S]*stroke-dasharray/);
