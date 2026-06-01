import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildDispersionPlaneGeometry,
  dispersionPlaneMarginalCounts,
  estimateLabelBox,
} from "../src/plots/model.ts";
import { layoutBoxesOverlap } from "../src/plots/layout.ts";
import { OverviewFigureController } from "../src/plots/figure-workbench.ts";

const counts = [
  1, 0, 2, 0,
  0, 3, 0, 0,
  4, 0, 0, 5,
];
const witnesses = [
  ...Array.from({ length: 4 }, (_, index) => ({ token: `core-${index}`, frequency: 1_000 + index, documentFrequency: 10, dp: 0.1, kind: "core" as const })),
  ...Array.from({ length: 3 }, (_, index) => ({ token: `loud-${index}`, frequency: 2_000 + index, documentFrequency: 2, dp: 0.7, kind: "loudButLocal" as const })),
  ...Array.from({ length: 2 }, (_, index) => ({ token: `quiet-${index}`, frequency: 20 + index, documentFrequency: 9, dp: 0.15, kind: "quietAndEven" as const })),
  ...Array.from({ length: 2 }, (_, index) => ({ token: `rare-${index}`, frequency: 2 + index, documentFrequency: 1, dp: 0.9, kind: "rareAndLocal" as const })),
];
const plane = {
  xBins: 4,
  yBins: 3,
  freqLog10Max: 3,
  counts,
  maxCellCount: 5,
  witnesses,
  loudButLocalCount: 3,
  loudButLocalMinFrequency: 1_000,
  highDispersionThreshold: 0.5,
  reading: "fixture",
};

const marginalCounts = dispersionPlaneMarginalCounts(plane);
assert.deepEqual(marginalCounts.frequency, [5, 3, 2, 5]);
assert.deepEqual(marginalCounts.dp, [3, 3, 9]);

const logRaw = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  totalTokens: 100_000,
  frequencyMode: "raw",
  densityMode: "log",
  examplesMode: "labelled",
  marginalsVisible: true,
  thresholdsVisible: true,
});
assert.deepEqual(logRaw.frequencyMarginal.map((marginal) => marginal.count), marginalCounts.frequency);
assert.deepEqual(logRaw.dpMarginal.map((marginal) => marginal.count), marginalCounts.dp);
assert.equal(logRaw.cells.length, counts.length);
assert.deepEqual(logRaw.cells.map((cell) => cell.count), counts);
assert(logRaw.cells.filter((cell) => cell.count === 0).every((cell) => cell.opacity === 0));
assert(logRaw.cells.filter((cell) => cell.count > 0).every((cell) => cell.opacity === 1));
assert(new Set(logRaw.cells.filter((cell) => cell.count > 0).map((cell) => cell.fill)).size >= 5);
assert.notEqual(logRaw.cells.find((cell) => cell.count === 1)?.fill, logRaw.cells.find((cell) => cell.count === 5)?.fill);
assert.match(logRaw.cells.find((cell) => cell.count === 5)?.fill ?? "", /--cw-plot-density-5/);
for (let row = 0; row < plane.yBins; row += 1) {
  const rowCells = logRaw.cells.filter((cell) => cell.row === row).sort((left, right) => left.column - right.column);
  for (let index = 1; index < rowCells.length; index += 1) {
    assert(Math.abs(rowCells[index - 1]!.x + rowCells[index - 1]!.width - rowCells[index]!.x) < 1e-9);
  }
}
assert.equal(logRaw.densityLegend.mode, "log");
assert.equal(logRaw.densityLegend.minimum, 1);
assert.equal(logRaw.densityLegend.maximum, 5);
assert.deepEqual(logRaw.yTicks.map((tick) => tick.label), ["0", "0.25", "0.50", "0.75", "1"]);
assert.deepEqual(logRaw.yTicks.filter((tick) => tick.accessibleLabel).map((tick) => tick.accessibleLabel), ["0, more even", "1, more concentrated"]);

const linearRaw = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  totalTokens: 100_000,
  densityMode: "linear",
});
assert.deepEqual(linearRaw.cells.map((cell) => cell.count), logRaw.cells.map((cell) => cell.count));
assert.equal(linearRaw.cells.find((cell) => cell.count === 3)?.t, 3 / 5);
assert.notEqual(
  linearRaw.cells.find((cell) => cell.count === 3)?.t,
  logRaw.cells.find((cell) => cell.count === 3)?.t,
);

const perMillion = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  totalTokens: 100_000,
  frequencyMode: "perMillion",
});
for (const [index, cell] of perMillion.cells.entries()) {
  assert(Math.abs(cell.x - logRaw.cells[index]!.x) < 1e-9);
}
assert.equal(perMillion.xScale.domain[0] - logRaw.xScale.domain[0], 1);
assert.equal(perMillion.witnesses[0]!.rawX - logRaw.witnesses[0]!.rawX, 1);
assert.equal(perMillion.xAxisLabel, "Frequency per million (log scale)");
assert(perMillion.cells.find((cell) => cell.count > 0)?.tooltipRows.some((row) => row.startsWith("per million")));

assert(logRaw.witnesses.every((witness) => witness.shape === "circle"));
assert.deepEqual(logRaw.witnesses.map((witness) => witness.datum), witnesses);
assert(logRaw.witnesses.every((witness) => witness.className === "cw-plot-plane-witness-mark"));
assert(logRaw.witnesses.every((witness) => witness.ariaLabel?.includes(witness.datum.token)));
assert(logRaw.witnesses.every((witness) => witness.tooltipRows?.some((row) => row.startsWith("category "))));
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("core-")).length, 1);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("loud-")).length, 1);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("quiet-")).length, 1);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("rare-")).length, 1);
assert(logRaw.labels.length <= 4);
assert(logRaw.witnesses.every((witness) => witness.rampIndex === 2));
assert.deepEqual(logRaw.quadrantLabels.map((label) => label.text).sort(), [
  "Frequent · concentrated",
  "Frequent · even",
  "Rare · concentrated",
  "Rare · even",
]);
for (const label of logRaw.labels.filter((candidate) => !candidate.pinned)) {
  for (const threshold of logRaw.thresholdLabels) {
    assert(!layoutBoxesOverlap(estimateLabelBox(label), estimateLabelBox(threshold), 1));
  }
}
logRaw.frequencyMarginal.forEach((marginal, column) => {
  const cell = logRaw.cells.find((candidate) => candidate.column === column);
  assert(cell);
  assert(Math.abs(marginal.x - cell.x) < 1e-9);
  assert(Math.abs(marginal.width - cell.width) < 1e-9);
});
logRaw.dpMarginal.forEach((marginal, row) => {
  const cell = logRaw.cells.find((candidate) => candidate.row === row);
  assert(cell);
  assert(Math.abs(marginal.y - cell.y) < 1e-9);
  assert(Math.abs(marginal.height - cell.height) < 1e-9);
});

const pinned = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  examplesMode: "labelled",
  pinnedKeys: new Set(["plane:core:core-3"]),
});
assert.equal(pinned.labels[0]?.text, "core-3");
assert.equal(pinned.labels[0]?.pinned, true);
assert.equal(pinned.witnesses.find((witness) => witness.datum.token === "core-3")?.pinned, true);
assert(pinned.labels.filter((label) => !label.pinned).length <= 4);
assert.equal(pinned.labels.filter((label) => label.pinned).length, 1);

const hiddenLayers = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  examplesMode: "off",
  marginalsVisible: false,
  thresholdsVisible: false,
});
assert.equal(hiddenLayers.witnesses.length, 0);
assert.equal(hiddenLayers.labels.length, 0);
assert.equal(hiddenLayers.frequencyMarginal.length, 0);
assert.equal(hiddenLayers.dpMarginal.length, 0);
assert.equal(hiddenLayers.thresholdLines.length, 0);
assert.equal(hiddenLayers.thresholdLabels.length, 0);
assert.deepEqual(logRaw.thresholdLabels.map((label) => label.text), ["DP 0.50", "Raw frequency 1,000"]);

const controller = new OverviewFigureController({
  initialControls: { frequency: "raw", density: "log", examples: "labelled", marginals: "show", thresholds: "show" },
  exportMetadata: (controls) => [
    `frequency ${controls.frequency} · density ${controls.density} · examples ${controls.examples} · marginals ${controls.marginals} · thresholds ${controls.thresholds}`,
  ],
});
controller.updateControls({ frequency: "perMillion", density: "linear", examples: "points", marginals: "hide", thresholds: "hide" });
assert.deepEqual(controller.exportMetadata(), ["frequency perMillion · density linear · examples points · marginals hide · thresholds hide"]);

const shell = readFileSync(new URL("../src/features/analyse/workbench/shell.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const figures = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");
const legends = readFileSync(new URL("../src/styles/plots/legends-and-annotations.css", import.meta.url), "utf8");

assert.match(render, /cw-plot-density-legend/);
assert.doesNotMatch(render, /cw-plot-witness-legend|renderPlaneWitnessLegend/);
assert.match(render, /renderPlaneMarginals\(svg, geometry\)/);
assert.match(render, /rx: 0,[\s\S]*cw-plot-density-cell/);
assert.match(render, /"data-point-shape": "circle"/);
assert.match(render, /cw-plot-plane-witness-pin-ring/);
assert.match(marks, /cw-plot-plane-witness[\s\S]*--cw-plot-witness[\s\S]*fill-opacity: 1/);
assert.doesNotMatch(marks, /cw-plot-witness-(?:core|loud-local|quiet-even|rare-local)/);
assert.match(marks, /data-figure-palette="monochrome"[^}]*cw-plot-plane-witness/);
assert.match(figures, /cw-overview-figure-controls[\s\S]*flex-wrap: wrap/);
assert.match(figures, /cw-overview-figure-body[\s\S]*overflow-x: auto/);
assert.match(legends, /max-width: 700px[\s\S]*is-narrow-optional:not\(\.is-pinned\)/);
assert.match(legends, /cw-plot-quadrant-label/);
assert.doesNotMatch(legends, /cw-plot-quadrant-pill/);
assert.match(figure, /cw-plot-crosshair-x, \.cw-plot-crosshair-y, \.cw-plot-tooltip/);
assert.match(figure, /target\.removeAttribute\("data-tooltip-title"\)/);
assert.doesNotMatch(render, /renderDispersionCompositionBar/);
assert.match(render, /if \(geometry\.compositionStrip\) renderCompositionStrip/);

for (const question of [
  "How unevenly is the corpus divided among documents?",
  "How much does lexical diversity vary between documents?",
  "How quickly does frequency fall as vocabulary rank increases?",
  "How quickly does the corpus encounter new types as tokens accumulate?",
  "How many types occur once, twice, or at each higher frequency?",
  "How much token mass is carried by the most frequent vocabulary?",
  "How widely across documents does the vocabulary reach?",
  "Which types are frequent, and which are evenly distributed across documents?",
]) {
  assert(shell.includes(question), `missing question subtitle: ${question}`);
}
assert.equal(shell.match(/captionReading:/g)?.length, 8);
assert((shell.match(/explanation: createFigureExplanation/g)?.length ?? 0) >= 7);
assert.match(shell, /isLength \? \["distributionDiagnostics" as const\] : \["mattr" as const, "distributionDiagnostics" as const\]/);
assert((shell.match(/exportMetadata:/g)?.length ?? 0) >= 7);
assert.doesNotMatch(shell, /representative sample/i);
assert.doesNotMatch(shell, /DP[^\n]{0,80}(?:statistically significant|significance threshold)/i);
