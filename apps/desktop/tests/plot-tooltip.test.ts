import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  PLOT_TOOLTIP_MAX_WIDTH,
  wrapPlotTooltipRows,
} from "../src/plots/figure-text.ts";

const measure = (value: string): number => Array.from(value).length * 7;
const maximumContentWidth = 140;
const lines = wrapPlotTooltipRows([
  "2 documents",
  "179,227 types",
  "62.3% of vocabulary",
  "Reach: 2 of 528 documents",
  "Examples: the, and, of +179,224 more",
], maximumContentWidth, measure);

assert.equal(PLOT_TOOLTIP_MAX_WIDTH, 300);
assert(lines.length > 5, "representative long tooltip row must wrap");
assert(lines.every((line) => measure(line.text) <= maximumContentWidth));
assert(lines.filter((line) => line.title).every((line) => line.sourceRow === 0));
assert.equal(lines.map((line) => line.text).join(" "), "2 documents 179,227 types 62.3% of vocabulary Reach: 2 of 528 documents Examples: the, and, of +179,224 more");

const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const interactions = readFileSync(new URL("../src/styles/plots/interactions.css", import.meta.url), "utf8");
assert.match(render, /getComputedTextLength\(\)/);
assert.match(render, /wrapPlotTooltipRows\(rows, maximumContentWidth, measure\)/);
assert.match(render, /Math\.min\(\s*PLOT_TOOLTIP_MAX_WIDTH/);
assert.match(render, /Math\.min\(plotWidth - width - edgeInset, Math\.max\(edgeInset, preferredX\)\)/);
assert.match(render, /Math\.min\(plotHeight - height - edgeInset, Math\.max\(edgeInset, preferredY\)\)/);
assert.match(interactions, /\.cw-plot-tooltip-title[\s\S]*font-weight: var\(--cw-weight-semibold\)/);
