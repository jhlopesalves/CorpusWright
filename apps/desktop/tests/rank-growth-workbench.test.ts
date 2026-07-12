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
assert.equal(new Set(curve.points.filter((point) => !point.excluded).map((point) => point.rampIndex)).size, 1);
assert(curve.points.filter((point) => point.excluded).every((point) => point.excluded));
assert(curve.fitPath);
assert(curve.legend.some((item) => item.label === "Excluded from fit" && item.hollow));

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
assert(!hiddenFit.legend.some((item) => item.label === "Fitted"));
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
assert(residuals.labels.some((label) => label.text === "analysis" && label.pinned));
assert(residuals.points[0]?.tooltipRows?.some((row) => row.includes("log residual")));

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

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
assert.match(shell, /How quickly does frequency fall as vocabulary rank increases\?/);
assert.match(shell, /How quickly does the corpus encounter new types as tokens accumulate\?/);
assert.match(shell, /Both axes are logarithmic in Curve view/);
assert.match(shell, /stable source order/);
assert.match(shell, /Zipf fit f\(r\) = C r\^-alpha/);
assert.match(shell, /V\(N\) = K N\^beta/);
assert.doesNotMatch(shell, /(?:alpha|beta)[\s\S]{0,80}type\s*=\s*["'](?:range|number)["']/i);
assert.match(marks, /cw-plot-heaps-fit[\s\S]*stroke-dasharray/);
