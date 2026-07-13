import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const plotModules = new Map(
  graph.modules
    .filter(module => module.path.includes(`${path.sep}styles${path.sep}plots${path.sep}`))
    .map(module => [path.basename(module.path), module.contents]),
);
const source = (name: string): string => {
  const contents = plotModules.get(name);
  assert(contents, `${name} must be imported`);
  return contents;
};

const figures = source("figures.css");
const axes = source("axes-and-grids.css");
const marks = source("marks.css");
const annotations = source("legends-and-annotations.css");
const interactions = source("interactions.css");
const allPlots = [...plotModules.values()].join("\n");
const features = graph.modules
  .filter(module => module.path.includes(`${path.sep}styles${path.sep}features${path.sep}`))
  .map(module => module.contents)
  .join("\n");

for (const selector of [
  ".cw-plot-axis-baseline",
  ".cw-plot-axis-tick",
  ".cw-plot-axis-title",
  ".cw-plot-grid line",
  ".cw-plot-grid-value",
  ".cw-plot-grid-context",
  ".cw-plot-tick-label",
  ".cw-plot-reference-line",
  ".cw-plot-threshold-line",
]) {
  assert(axes.includes(selector), `${selector} must be owned by axes-and-grids.css`);
}

for (const selector of [
  ".cw-plot-point",
  ".cw-plot-line",
  ".cw-plot-fit-line",
  ".cw-plot-area",
  ".cw-plot-bar",
  ".cw-plot-lollipop-stem",
  ".cw-plot-density-cell",
  ".cw-plot-composition-segment",
  ".cw-plot-spectrum-band",
  ".cw-plot-roster-dot",
  ".cw-plot-plane-witness",
  ".cw-plot-mattr-witness-tick",
]) {
  assert(marks.includes(selector), `${selector} must be owned by marks.css`);
}

for (const selector of [
  ".cw-plot-legend-bg",
  ".cw-plot-legend-swatch",
  ".cw-plot-legend-label",
  ".cw-plot-label",
  ".cw-plot-label-muted",
  ".cw-plot-label-strong",
  ".cw-plot-label-callout",
  ".cw-plot-fitted-range",
  ".cw-plot-halo",
]) {
  assert(annotations.includes(selector), `${selector} must be owned by legends-and-annotations.css`);
}

for (const selector of [
  ".cw-plot-tooltip",
  ".cw-plot-tooltip-box",
  ".cw-plot-tooltip-text",
  ".cw-plot-hover-target",
  ".cw-plot-crosshair-x",
  ".cw-plot-crosshair-y",
]) {
  assert(interactions.includes(selector), `${selector} must be owned by interactions.css`);
}
assert.match(interactions, /@media \(prefers-reduced-motion: reduce\)/);

for (const selector of [
  ".cw-overview-figure",
  ".cw-plot-svg",
  ".cw-plot-figure-title",
  ".cw-plot-figure-subtitle",
  ".cw-plot-figure-detail",
  ".cw-plot-figure-reading",
  ".cw-plot-figure-stats",
]) {
  assert(figures.includes(selector), `${selector} must be owned by figures.css`);
}

assert.doesNotMatch(features, /\.cw-plot-(?:axis|grid|legend|tooltip)[\w-]*/);

const renderSource = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const figureSource = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
for (const className of [
  "cw-plot-svg",
  "cw-plot-axis-baseline",
  "cw-plot-grid",
  "cw-plot-point",
  "cw-plot-line",
  "cw-plot-bar",
  "cw-plot-legend-bg",
  "cw-plot-tooltip",
  "cw-plot-figure-title",
]) {
  assert(renderSource.includes(className) || figureSource.includes(className), `${className} must remain in plot construction`);
  assert(allPlots.includes(`.${className}`), `${className} must retain a shared style rule`);
}

assert.match(figureSource, /export function overviewFigureSvgString/);
assert.match(figureSource, /serializeSvgWithResolvedTokens/);
assert.match(renderSource, /function renderAxes/);
