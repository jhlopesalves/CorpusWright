import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  FIGURE_INTERACTION_POLICIES,
  OverviewFigureController,
  normalizeFigureState,
} from "../src/plots/figure-workbench.ts";
import {
  buildCompositionStripGeometry,
  buildDispersionGeometry,
  buildLexicalSpectrumGeometry,
} from "../src/plots/model.ts";

assert.deepEqual(normalizeFigureState("overview-mattr-distribution", {
  view: "histogram",
  histogramRule: "sturges",
  manualBins: 500,
  xScale: "log",
  labels: "off",
}), { view: "histogram", histogramRule: "auto", manualBins: 100, xScale: "linear" });
assert.deepEqual(normalizeFigureState("overview-dispersion-plane", {
  density: "linear",
  marginals: "hide",
}), {});
assert.deepEqual(normalizeFigureState("overview-rank-frequency", {
  view: "residuals",
  frequency: "raw",
  fit: "hide",
}), { view: "residuals" });
assert.deepEqual(normalizeFigureState("overview-dispersion-roster", {
  sort: "bad",
  stopwords: "bad",
  rows: 500,
}), { sort: "frequency", stopwords: "show", rows: 100 });

const distribution = new OverviewFigureController({
  initialControls: { view: "histogram", histogramRule: "auto", manualBins: 24, xScale: "log" },
  policy: FIGURE_INTERACTION_POLICIES["overview-length-histogram"],
  exportMetadata: (controls) => Object.keys(controls).sort(),
});
distribution.togglePin("distribution-bin:0:1:2");
assert.equal(distribution.pins.size, 1);
distribution.updateControls({ view: "ecdf" });
assert.equal(distribution.pins.size, 0, "histogram-only pins must not become phantom ECDF pins");
assert.equal(distribution.togglePin("distribution-qq:1"), false, "an incompatible pin cannot enter state");
assert(!distribution.exportMetadata().includes("labels"), "removed state must not enter export metadata");

const rank = new OverviewFigureController({
  initialControls: { view: "curve" },
  policy: FIGURE_INTERACTION_POLICIES["overview-rank-frequency"],
});
rank.togglePin("rank:4:corpus");
rank.updateControls({ view: "residuals" });
assert(rank.pins.has("rank:4:corpus"), "rank datum pins survive compatible view changes");
rank.clearPins();
assert.equal(rank.pins.size, 0);

const profilePins = new Set(["dispersion-reach-0"]);
const profile = buildDispersionGeometry({
  buckets: [
    { key: "one", label: "1", lowerBound: 1, upperBound: 1, count: 80, share: 0.8 },
    { key: "many", label: "2+", lowerBound: 2, upperBound: null, count: 20, share: 0.2 },
  ],
  totalTypes: 100,
  totalDocuments: 10,
  pinnedKeys: profilePins,
});
assert(profile.points.some((point) => point.key === "dispersion-reach-0" && point.pinned));
assert(profile.compositionStrip?.segments.some((segment) => segment.datum.key === "dispersion-reach-0" && segment.pinned));

const labels = buildLexicalSpectrumGeometry({
  points: [
    { frequency: 1, typeCount: 50 },
    { frequency: 2, typeCount: 25 },
    { frequency: 3, typeCount: 10 },
    { frequency: 10, typeCount: 15 },
  ],
  hapaxCount: 50,
  disLegomenaCount: 25,
  band3To9Count: 10,
  band10plusCount: 15,
  totalTypes: 100,
  labelsMode: "authored",
  pinnedKeys: new Set(["spectrum:3"]),
}).labels.map((label) => label.text);
assert(labels.some((label) => label.startsWith("hapax")));
assert(labels.some((label) => label.startsWith("dis")));
assert(labels.some((label) => label.startsWith("f=3")));

const zeroStrip = buildCompositionStripGeometry({
  total: 10,
  x: 0,
  y: 0,
  width: 100,
  segments: [
    { key: "zero", label: "Zero", count: 0, rampIndex: 0 },
    { key: "ten", label: "Ten", count: 10, rampIndex: 1 },
  ],
});
assert.equal(zeroStrip.segments[0]?.width, 0);

const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const workbench = readFileSync(new URL("../src/plots/figure-workbench.ts", import.meta.url), "utf8");
assert.equal((render.match(/tabindex: "0"/g) ?? []).length, 1, "only the plot owner enters ordinary Tab order");
assert.match(render, /tabindex: interactive \? "-1" : undefined/);
assert.match(render, /role: point\.key \? "button" : undefined/);
assert.match(render, /role: cell\.count > 0 \? "gridcell" : undefined/);
assert.match(render, /spatialGridTarget/);
assert.match(render, /event\.key === "Escape"/);
assert.match(render, /Enter or Space to pin evidence/);
assert.match(figure, /target\.removeAttribute\("data-pin-key"\)/);
assert.doesNotMatch(workbench, /FIGURE_PALETTE_STORAGE_KEY|readFigurePalette|writeFigurePalette|applyFigurePalette/);
