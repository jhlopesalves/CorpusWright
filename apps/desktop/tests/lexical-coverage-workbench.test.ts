import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildCoverageGeometry,
  buildLexicalSpectrumGeometry,
  deriveCoverageBandShares,
  scaleValue,
} from "../src/plots/model.ts";
import { OverviewFigureController } from "../src/plots/figure-workbench.ts";

const spectrumPoints = [
  { frequency: 1, typeCount: 40 },
  { frequency: 2, typeCount: 20 },
  { frequency: 3, typeCount: 15 },
  { frequency: 9, typeCount: 5 },
  { frequency: 10, typeCount: 10 },
  { frequency: 100, typeCount: 10 },
];
const spectrum = buildLexicalSpectrumGeometry({
  points: spectrumPoints,
  hapaxCount: 40,
  disLegomenaCount: 20,
  band3To9Count: 20,
  band10plusCount: 20,
  totalTypes: 100,
  yMode: "count",
  bandsVisible: true,
});
assert.equal(spectrum.compositionStrip?.segments.reduce((sum, segment) => sum + segment.datum.count, 0), 100);
assert.deepEqual(
  spectrum.compositionStrip?.segments.map((segment) => segment.datum.rampIndex),
  [0, 1, 2, 4],
);
assert.deepEqual(
  [...new Set(spectrum.points.map((point) => point.rampIndex))],
  [0, 1, 2, 4],
);
assert.deepEqual(spectrum.points.map((point) => point.rawX), spectrumPoints.map((point) => point.frequency));
assert.equal(spectrum.linePath, undefined, "the exact spectrum contract must not introduce a line");
assert(spectrum.points.every((point) => point.tooltipRows?.some((row) => row.includes("exact frequency"))));
assert(spectrum.compositionStrip?.segments.every((segment) => segment.tooltipRows.length >= 2));
assert(spectrum.labels.some((label) => label.text.startsWith("hapax · f = 1\n")));
assert(spectrum.labels.some((label) => label.text.startsWith("dis legomena · f = 2\n")));

const spectrumShare = buildLexicalSpectrumGeometry({
  points: spectrumPoints,
  hapaxCount: 40,
  disLegomenaCount: 20,
  band3To9Count: 20,
  band10plusCount: 20,
  totalTypes: 100,
  yMode: "share",
  bandsVisible: false,
});
assert.deepEqual(spectrumShare.points.map((point) => point.rawY), spectrumPoints.map((point) => point.typeCount / 100));
assert.equal(spectrumShare.bands.length, 0);
assert.deepEqual(spectrumShare.points.map((point) => point.rawX), spectrum.points.map((point) => point.rawX));

const tinyHapax = buildLexicalSpectrumGeometry({
  points: [{ frequency: 1, typeCount: 1 }, { frequency: 2, typeCount: 1 }, { frequency: 10, typeCount: 1_000_000 }],
  hapaxCount: 1,
  disLegomenaCount: 1,
  band3To9Count: 0,
  band10plusCount: 1_000_000,
  totalTypes: 1_000_002,
});
assert.equal(tinyHapax.compositionStrip?.segments[0]?.datum.label, "Hapax (1)");
assert(tinyHapax.compositionStrip?.segments[0]?.width && tinyHapax.compositionStrip.segments[0].width > 0);

const bands = deriveCoverageBandShares({
  totalTypes: 20_000,
  cumulative: [
    { k: 10, share: 0.3 },
    { k: 100, share: 0.5 },
    { k: 1_000, share: 0.7 },
    { k: 10_000, share: 0.9 },
  ],
});
assert(Math.abs(bands.reduce((sum, band) => sum + band.share, 0) - 1) < 1e-9);
for (const [index, expected] of [0.3, 0.2, 0.2, 0.2, 0.1].entries()) {
  assert(Math.abs(bands[index]!.share - expected) < 1e-9);
}

const smallVocabularyBands = deriveCoverageBandShares({
  totalTypes: 8,
  cumulative: [
    { k: 10, share: null },
    { k: 100, share: null },
    { k: 1_000, share: null },
    { k: 10_000, share: null },
  ],
});
assert.deepEqual(smallVocabularyBands.map((band) => band.share), [1, 0, 0, 0, 0]);

const coveragePoints = [
  { k: 1, share: 0.2 },
  { k: 5, share: 0.45 },
  { k: 10, share: 0.65 },
  { k: 100, share: 0.9 },
];
const coverage = buildCoverageGeometry({
  points: coveragePoints,
  milestones: [{ k: 10, share: 0.65 }, { k: 100, share: 0.9 }],
  totalTypes: 1_000,
  xMode: "topK",
  targetShare: 0.5,
  targetCrossing: { k: 7, share: 0.53 },
  labelsMode: "milestones",
  bandShares: bands,
});
assert.equal(coverage.halfCrossing?.k, 7, "the supplied exact Rust crossing must replace checkpoint inference");
assert.equal(coverage.halfCrossing?.share, 0.53);
assert.equal(coverage.halfCrossing?.targetY, scaleValue(coverage.yScale, 0.5));
assert(coverage.labels.some((label) => label.text.includes("50% target = 7 types")));
assert(coverage.points[1]?.tooltipRows?.some((row) => row.includes("gained since previous checkpoint")));
assert.match(coverage.linePath?.d ?? "", /^M[^CQ]+(?:L[^CQ]+)+$/);
assert((coverage.linePath?.d ?? "").includes(`${coverage.halfCrossing!.x.toFixed(2)} ${coverage.halfCrossing!.y.toFixed(2)}`), "display path must pass through the exact target crossing");
assert.equal(coverage.compositionStrip?.segments.length, 5);

const coverageShareX = buildCoverageGeometry({
  points: coveragePoints,
  milestones: [],
  totalTypes: 1_000,
  xMode: "vocabularyShare",
  targetShare: 0.8,
  targetCrossing: { k: 60, share: 0.81 },
  labelsMode: "target",
});
assert.equal(coverage.xScale.kind, "log");
assert.equal(coverageShareX.xScale.kind, "linear");
assert.equal(coverageShareX.points[1]?.x, scaleValue(coverageShareX.xScale, 5 / 1_000));
assert(coverageShareX.labels.some((label) => label.text.includes("80% target = 60 types")));

const unavailableExactCrossing = buildCoverageGeometry({
  points: coveragePoints,
  milestones: [],
  targetShare: 0.5,
  targetCrossing: null,
});
assert.equal(unavailableExactCrossing.halfCrossing, null, "an unavailable Rust crossing must not fall back to a bounded checkpoint");

const controller = new OverviewFigureController({
  initialControls: { target: "0.5" },
  exportMetadata: (controls) => [`target ${Number(controls.target) * 100}%`],
});
controller.updateControls({ target: "0.9" });
assert.deepEqual(controller.exportMetadata(), ["target 90%"]);

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
assert.doesNotMatch(shell, /Frequency-class spectrum/);
assert.match(shell, /How many types occur once, twice, or at each higher frequency\?/);
assert.match(shell, /How much token mass is carried by the most frequent vocabulary\?/);
assert.match(shell, /exactTargetCrossings\.find/);
assert.match(render, /renderCompositionStrip\(svg, geometry\.compositionStrip\)/);
assert.doesNotMatch(render, /class: "cw-plot-spectrum-band-label"/);
assert.match(render, /geometry\.halfCrossing\.targetY/);
assert.match(marks, /data-figure-palette="monochrome"[^}]*cw-plot-composition-strip/);
