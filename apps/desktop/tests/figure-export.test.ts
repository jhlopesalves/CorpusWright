import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  expandViewBoxForContent,
  wrapSvgText,
} from "../src/plots/figure-text.ts";
import { hasUnresolvedCssVariables } from "../src/plots/svg-export.ts";

const measure = (value: string): number => Array.from(value).length * 8;
const availableWidth = 96;
const textCases = {
  title: "A deliberately long analytical figure title that must wrap safely",
  subtitle: "A long subtitle preserves every explanatory word in source order",
  statistics: "1,234 tokens across 57 documents with a 98.5% coverage threshold",
  detail: "Detail copy remains complete when the available export width is narrow",
  reading: "Reading guidance can span several lines without clipping or ellipsis",
};
for (const [role, value] of Object.entries(textCases)) {
  const lines = wrapSvgText(value, availableWidth, measure);
  assert(lines.length > 1, `${role} fixture must exercise wrapping`);
  assert(lines.every(line => measure(line) <= availableWidth), `${role} lines must fit the available width`);
  assert.equal(lines.join(" "), value, `${role} words must remain in source order`);
}

const longToken = "supercalifragilisticexpialidocious";
const tokenLines = wrapSvgText(longToken, 40, measure);
assert(tokenLines.length > 1, "unusually long tokens must break safely");
assert(tokenLines.every(line => measure(line) <= 40));
assert.equal(tokenLines.join(""), longToken, "long-token wrapping must not lose characters");
assert.deepEqual(wrapSvgText("   ", availableWidth, measure), [], "empty optional copy must remain absent");
assert.throws(() => wrapSvgText("value", 0, measure), /positive/);

const plot = { x: 0, y: 0, width: 720, height: 360 };
const wrappedTitleLines = wrapSvgText(textCases.title, availableWidth, measure);
const wrappedReadingLines = wrapSvgText(textCases.reading, availableWidth, measure);
const titleBounds = { x: 12, y: -90, width: availableWidth, height: wrappedTitleLines.length * 20 };
const readingBounds = { x: 12, y: 380, width: availableWidth, height: wrappedReadingLines.length * 16 };
const expanded = expandViewBoxForContent(plot, [titleBounds, readingBounds], 8);
assert.equal(expanded.y, titleBounds.y - 8, "dynamic viewBox must include every title line");
assert.equal(
  expanded.y + expanded.height,
  readingBounds.y + readingBounds.height + 8,
  "dynamic viewBox must include every reading line",
);
assert.equal(expanded.x, plot.x);
assert.equal(expanded.width, plot.width, "identity wrapping must not alter plot width or x coordinates");

assert.equal(hasUnresolvedCssVariables('<svg style="fill: rgb(0, 0, 0)"></svg>'), false);
assert.equal(hasUnresolvedCssVariables('<svg style="fill: var(--cw-plot-point)"></svg>'), true);

const figureSource = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const renderSource = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const figureStyles = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");
const interactionStyles = readFileSync(new URL("../src/styles/plots/interactions.css", import.meta.url), "utf8");

assert.doesNotMatch(figureSource, /FIGURE_(?:TITLE|SUBTITLE|STATS|READING)_FONT_SIZE/);
assert.doesNotMatch(figureSource, /font-(?:size|weight)/, "figure identity typography must be CSS-owned");
assert.doesNotMatch(`${figureSource}\n${figureStyles}`, /\b650\b/);
for (const className of [
  "cw-plot-figure-title",
  "cw-plot-figure-subtitle",
  "cw-plot-figure-stats",
  "cw-plot-figure-detail",
  "cw-plot-figure-reading",
]) {
  assert.match(figureStyles, new RegExp(`\\.${className}`), `${className} must be CSS-owned`);
}
assert.match(figureStyles, /\.cw-plot-figure-title\s*\{[^}]*font-size:\s*17px[^}]*font-weight:\s*var\(--cw-weight-semibold\)/s);
assert.match(interactionStyles, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*animation:\s*none/);
assert.match(interactionStyles, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*transition:\s*none/);
assert.match(interactionStyles, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*opacity:\s*1/);
assert.match(interactionStyles, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*stroke-dashoffset:\s*0/);
assert.match(interactionStyles, /\.cw-plot-svg\.cw-plot-export-static[\s\S]*transform:\s*none/);

assert.match(figureSource, /prepareStaticExportClone\(exportedSvg\)/);
assert.match(figureSource, /classList\.remove\("is-hovering"\)/);
assert.match(figureSource, /\.cw-plot-crosshair-x, \.cw-plot-crosshair-y, \.cw-plot-tooltip/);
assert.match(figureSource, /transient\.remove\(\)/);
assert.match(figureSource, /target\.removeAttribute\("tabindex"\)/);
assert.match(figureSource, /\^\(\?:--cw-plot-stagger\|animation\|animation-delay\|transition\)/);
assert.match(figureSource, /decorateFigureSvg\(exportedSvg, options\);[\s\S]*serializeSvgWithResolvedTokens\(exportedSvg\)/);
assert.match(figureSource, /document\.createElementNS\(SVG_NS, "title"\)/);
assert.match(figureSource, /document\.createElementNS\(SVG_NS, "desc"\)/);
assert.match(figureSource, /aria-labelledby/);
assert.match(figureSource, /expandViewBoxForContent\(plot, measuredBounds/);
assert.match(figureSource, /background\.setAttribute\("class", "cw-plot-export-background"\)/);
assert.match(figureSource, /svg\.insertBefore\(background, accessibleDescription\.nextSibling\)/);
assert.match(figureStyles, /\.cw-plot-export-background\s*\{[^}]*fill:\s*var\(--panel-bg\)/s);
assert.doesNotMatch(figureSource, /FIGURE_SVG_(?:TOP|BOTTOM)_INSET/);

for (const property of [
  "fill",
  "stroke",
  "opacity",
  "stroke-dasharray",
  "stroke-dashoffset",
  "font-family",
  "font-size",
  "font-weight",
  "stroke-linecap",
  "stroke-linejoin",
  "stop-color",
  "color",
]) {
  assert(renderSource.includes(`"${property}"`), `standalone serializer must resolve ${property}`);
}
assert.match(renderSource, /if \(hasUnresolvedCssVariables\(serialized\)\)/);
assert.doesNotMatch(renderSource, /"animation(?:-delay)?"/, "live timing must not be serialized");
