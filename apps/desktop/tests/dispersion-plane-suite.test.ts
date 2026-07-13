import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildDispersionPlaneGeometry,
  dispersionPlaneMarginalCounts,
} from "../src/plots/model.ts";
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
assert.equal(logRaw.densityLegend.mode, "log");
assert.equal(logRaw.densityLegend.minimum, 1);
assert.equal(logRaw.densityLegend.maximum, 5);

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

const shapeByKind = new Map(logRaw.witnesses.map((witness) => [witness.datum.kind, witness.shape]));
assert.deepEqual(Object.fromEntries(shapeByKind), {
  core: "circle",
  loudButLocal: "diamond",
  quietAndEven: "square",
  rareAndLocal: "triangle",
});
assert.deepEqual(logRaw.witnessLegend.map((item) => [item.label, item.shape]), [
  ["Core vocabulary", "circle"],
  ["Loud but local", "diamond"],
  ["Quiet and even", "square"],
  ["Rare and local", "triangle"],
]);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("core-")).length, 3);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("loud-")).length, 2);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("quiet-")).length, 1);
assert.equal(logRaw.labels.filter((label) => label.text.startsWith("rare-")).length, 1);
assert(logRaw.labels.length <= 7);

const pinned = buildDispersionPlaneGeometry({
  plane,
  totalTypes: 15,
  examplesMode: "labelled",
  pinnedKeys: new Set(["plane:core:core-3"]),
});
assert.equal(pinned.labels[0]?.text, "core-3");
assert.equal(pinned.labels[0]?.pinned, true);
assert.equal(pinned.witnesses.find((witness) => witness.datum.token === "core-3")?.pinned, true);

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

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const figures = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");
const legends = readFileSync(new URL("../src/styles/plots/legends-and-annotations.css", import.meta.url), "utf8");

assert.match(render, /cw-plot-density-legend/);
assert.match(render, /cw-plot-witness-legend/);
assert.match(render, /renderPlaneMarginals\(svg, geometry\)/);
assert.match(render, /data-point-shape/);
assert.match(marks, /data-figure-palette="monochrome"[^}]*cw-plot-plane-witness/);
assert.match(figures, /cw-overview-figure-controls[\s\S]*flex-wrap: wrap/);
assert.match(figures, /cw-overview-figure-body[\s\S]*overflow-x: auto/);
assert.match(legends, /max-width: 700px[\s\S]*is-narrow-optional:not\(\.is-pinned\)/);
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
  "Which frequent types are corpus-wide, and which are locally concentrated?",
]) {
  assert(shell.includes(question), `missing question subtitle: ${question}`);
}
assert.equal(shell.match(/captionReading:/g)?.length, 9);
assert((shell.match(/explanation: createFigureExplanation/g)?.length ?? 0) >= 8);
assert.match(shell, /isLength \? \["distributionDiagnostics" as const\] : \["mattr" as const, "distributionDiagnostics" as const\]/);
assert((shell.match(/exportMetadata:/g)?.length ?? 0) >= 8);
assert.doesNotMatch(shell, /representative sample/i);
assert.doesNotMatch(shell, /DP[^\n]{0,80}(?:statistically significant|significance threshold)/i);
