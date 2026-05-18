import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  OverviewFigureController,
  evidenceKey,
} from "../src/plots/figure-workbench.ts";
import {
  buildCompositionStripGeometry,
  placeLabels,
  pointShapeArea,
} from "../src/plots/model.ts";

const controller = new OverviewFigureController({
  initialControls: { view: "curve", fit: true },
  exportMetadata: (controls) => [`view ${controls.view}`, `fit ${controls.fit}`],
});
controller.togglePin("rank:1:the");
controller.updateControls({ view: "residuals" });
assert(controller.isPinned("rank:1:the"), "pins must survive control-driven rerenders");
assert.deepEqual(controller.exportMetadata(), ["view residuals", "fit true"]);
controller.clearPins();
assert.equal(controller.pins.size, 0);

let resolveOlder!: (value: string) => void;
let resolveNewer!: (value: string) => void;
const applied: string[] = [];
const older = controller.loadLatest(
  () => new Promise<string>((resolve) => { resolveOlder = resolve; }),
  (value) => applied.push(value),
);
const newer = controller.loadLatest(
  () => new Promise<string>((resolve) => { resolveNewer = resolve; }),
  (value) => applied.push(value),
);
resolveNewer("newer");
assert.equal(await newer, true);
resolveOlder("older");
assert.equal(await older, false);
assert.deepEqual(applied, ["newer"], "stale async results must not overwrite newer figure state");

const strip = buildCompositionStripGeometry({
  total: 100,
  x: 10,
  y: 20,
  width: 500,
  segments: [
    { key: "a", label: "A", count: 20, rampIndex: 0 },
    { key: "b", label: "B", count: 30, rampIndex: 2 },
    { key: "c", label: "C", count: 50, rampIndex: 5 },
  ],
  pinnedKeys: new Set(["b"]),
});
assert.equal(strip.segments.reduce((sum, segment) => sum + segment.width, 0), 500);
assert.equal(strip.segments.reduce((sum, segment) => sum + segment.datum.count, 0), 100);
assert.equal(strip.segments[1]?.pinned, true);
assert.throws(
  () => buildCompositionStripGeometry({
    total: 4,
    x: 0,
    y: 0,
    width: 100,
    segments: [
      { key: "a", label: "A", count: 1, rampIndex: 0 },
      { key: "b", label: "B", count: 2, rampIndex: 1 },
    ],
  }),
  /expected 4/,
);

const areas = ["circle", "square", "diamond", "triangle"].map((shape) =>
  pointShapeArea(shape as "circle" | "square" | "diamond" | "triangle")
);
assert(Math.max(...areas) / Math.min(...areas) < 1.02, "point shapes must have comparable apparent area");

const placed = placeLabels(
  [
    { x: 20, y: 20, text: "automatic", priority: 1, candidates: [{ dx: 0, dy: 0, anchor: "start" }] },
    { x: 50, y: 20, text: "pinned", pinned: true, priority: 0, candidates: [{ dx: 0, dy: 0, anchor: "start" }] },
    { x: 80, y: 20, text: "priority", priority: 10, candidates: [{ dx: 0, dy: 0, anchor: "start" }] },
  ],
  [],
  { left: 0, top: 0, right: 200, bottom: 100 },
  2,
);
assert.deepEqual(placed.map((label) => label.text), ["pinned", "priority"]);
assert.equal(evidenceKey("rank", 1, "the/of"), "rank:1:the%2Fof");

const shellSource = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const figureSource = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const renderSource = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const modelSource = readFileSync(new URL("../src/plots/model.ts", import.meta.url), "utf8");
const paletteSource = readFileSync(new URL("../src/styles/foundations/figure-palettes.css", import.meta.url), "utf8");

assert.equal((shellSource.match(/createFigurePaletteToolbar\(dom\.analyseOverview\)/g) ?? []).length, 0);
assert.doesNotMatch(shellSource, /Figure palette/);
assert.doesNotMatch(shellSource, /type=["']color["']|createElement\(["']input["']\)[\s\S]{0,120}\.type\s*=\s*["']color["']/);
for (const mode of ["theme", "colorBlind", "monochrome"]) {
  assert.match(paletteSource, new RegExp(`data-figure-palette=["']${mode}["']`));
}
for (const shape of ["circle", "square", "diamond", "triangle"]) {
  assert.match(`${renderSource}\n${modelSource}`, new RegExp(`(?:shape === ["']${shape}["']|case ["']${shape}["'])`));
}
assert.match(renderSource, /data-pin-key/);
assert.match(figureSource, /paletteMode !== "theme"[\s\S]*metadata\.push/);
assert.match(figureSource, /cw-overview-figure-explanation/);
