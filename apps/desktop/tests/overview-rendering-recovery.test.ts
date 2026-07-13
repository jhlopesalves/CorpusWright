import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const palette = readFileSync(new URL("../src/styles/foundations/figure-palettes.css", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const fixture = readFileSync(new URL("./visual/overview-suite-fixture.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const interactions = readFileSync(new URL("../src/styles/plots/interactions.css", import.meta.url), "utf8");
const figures = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");

const livePalette = palette.match(/\.cw-overview-figure\s*,[^\{]*\{([\s\S]*?)\n\}/)?.[1] ?? "";
for (const role of [
  "observed", "reference", "selected", "attention",
  ...Array.from({ length: 6 }, (_, index) => `density-${index}`),
  ...Array.from({ length: 6 }, (_, index) => `composition-${index}`),
  "witness-core", "witness-loud-local", "witness-quiet-even", "witness-rare-local",
]) {
  assert.match(livePalette, new RegExp(`--cw-plot-${role}:`), `live figure palette must define ${role}`);
}
assert.doesNotMatch(livePalette, /data-figure-palette/);
for (const mode of ["theme", "colorBlind", "monochrome"]) {
  assert.match(palette, new RegExp(`data-figure-palette=["']${mode}["']`));
}
assert.match(livePalette, /--cw-plot-witness-core:\s*var\(--cw-plot-ramp-0\)/);
assert.match(figure, /exportMount\.dataset\.figurePalette = paletteMode/);
assert.doesNotMatch(figure, /figure\.dataset\.figurePalette = paletteMode/);

for (const fallback of ["cw-plot-point", "cw-plot-fit", "cw-plot-ramp-4", "cw-plot-ramp-5"]) {
  assert.match(marks, new RegExp(`var\\(--cw-plot-[^)]+, var\\(--${fallback}\\)\\)`), `missing semantic fallback to ${fallback}`);
}

assert.match(fixture, /getComputedStyle\(figure\)\.getPropertyValue/);
assert.match(fixture, /browser-black computed/);
assert.match(fixture, /insufficient computed fill variation/);
assert.match(fixture, /hidden control occupies layout/);
assert.match(fixture, /cw-plot-ecdf-line/);
assert.doesNotMatch(render, /cw-plot-animated-path|pathLength:\s*"1"/);
assert.doesNotMatch(interactions, /cw-plot-path-draw|stroke-dashoffset:\s*1/);
assert.match(fixture, /cw-plot-density-cell:not\(\.is-zero\)/);

assert.doesNotMatch(interactions, /scale\(1\.15\)/);
assert.match(interactions, /cw-plot-point[\s\S]*transform: scale\(1\.04\)/);
for (const selector of ["cw-plot-density-cell", "cw-plot-bar", "cw-plot-composition-segment"]) {
  const hoverRule = interactions.match(new RegExp(`\\.${selector}[\\s\\S]*?\\{([\\s\\S]*?)\\}`))?.[1] ?? "";
  assert.doesNotMatch(hoverRule, /transform:\s*scale\(/, `${selector} hover must not distort plot geometry`);
}
const lollipopRule = interactions.match(/\.cw-plot-point\.is-lollipop[\s\S]*?\{([\s\S]*?)\}/)?.[1] ?? "";
assert.doesNotMatch(lollipopRule, /transform:\s*scale\(/, "lollipop hover must not move its endpoint off the stem");
assert.match(interactions, /cw-plot-tooltip[\s\S]*transition: opacity 120ms ease/);
assert.match(interactions, /prefers-reduced-motion: reduce[\s\S]*\.cw-plot-tooltip[\s\S]*transition: none/);
assert.equal(render.match(/renderInteractionLayer\(svg, geometry\.box, true\)/g)?.length, 4);
assert.equal(render.match(/renderInteractionLayer\(svg, geometry\.box, false\)/g)?.length, 5);
assert.match(render, /renderInteractionLayer\(svg, geometry\.box, geometry\.crosshairEnabled\)/);
assert.match(figures, /\.cw-overview-figure-teaching \.analysis-glossary-term[\s\S]*border-bottom: 0/);
assert.match(figures, /\.cw-overview-figure-teaching \.analysis-glossary-detail[\s\S]*background: transparent/);
