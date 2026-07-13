import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const palette = readFileSync(new URL("../src/styles/foundations/figure-palettes.css", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const fixture = readFileSync(new URL("./visual/overview-suite-fixture.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");

const livePalette = palette.match(/\.cw-overview-figure\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
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
assert.match(fixture, /cw-plot-density-cell:not\(\.is-zero\)/);
