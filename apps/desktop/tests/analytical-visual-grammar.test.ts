import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  CONCORDANCE_COLLOCATION_INITIAL_ROWS,
  CONCORDANCE_COLLOCATION_SHOW_MORE_STEP,
  concordanceContextSortLevels,
  nextConcordanceCollocateDisplayCount,
} from "../src/concordance.ts";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const collocations = readFileSync(
  new URL("../src/workbench/concordance/collocations.ts", import.meta.url),
  "utf8",
);
const shared = readFileSync(
  new URL("../src/workbench/concordance/shared.ts", import.meta.url),
  "utf8",
);
const state = readFileSync(
  new URL("../src/workbench/concordance/state.ts", import.meta.url),
  "utf8",
);
const figures = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");

for (const card of ["lines", "collocates", "distribution"]) {
  assert(!html.includes(`id="concordance-${card}-collapse"`), `${card} must not expose a collapse control`);
}
assert(!shared.includes("CollapsedCards"), "main-card collapse persistence must be removed");

assert.match(
  html,
  /id="concordance-lines-controls" class="concordance-card-controls concordance-lines-controls">/,
);
assert.match(
  html,
  /id="concordance-collocates-controls" class="concordance-card-controls concordance-collocation-controls">/,
);
for (const label of ["Window", "Sort", "Rows", "Span", "Direction", "Measure", "Min co-occurrence", "Min corpus frequency", "Stopwords"]) {
  assert(html.includes(`<span>${label}</span>`), `${label} must remain visible`);
}
assert.match(html, /<p id="concordance-lines-stats" class="concordance-card-stats">/);
assert.match(html, /<p id="concordance-collocates-stats" class="concordance-card-stats">/);
assert(!html.includes('data-concordance-card-action="controls"'), "stats must not trigger hidden controls");

const executeStart = state.indexOf("export async function executeConcordanceSharedRequest(");
const executeEnd = state.indexOf("export type ConcordanceSectionRequest", executeStart);
const executeBody = state.slice(executeStart, executeEnd);
assert(executeBody.indexOf("await refreshConcordance()") < executeBody.indexOf("await refreshCollocations()"));
assert(!executeBody.includes("Promise.all"), "fresh Collocates must not run concurrently with shared search");
assert(shared.includes("refreshConcordanceCollocations,"), "shared Search must pass the Collocates continuation");

assert.equal(CONCORDANCE_COLLOCATION_INITIAL_ROWS, 10);
assert.equal(CONCORDANCE_COLLOCATION_SHOW_MORE_STEP, 25);
assert.equal(nextConcordanceCollocateDisplayCount(10, 100), 35);
const showMoreStart = collocations.indexOf("export function handleConcordanceCollocationsShowMore");
const showMoreEnd = collocations.indexOf("async function refreshCollocateEvidence", showMoreStart);
const showMoreBody = collocations.slice(showMoreStart, showMoreEnd);
assert(showMoreBody.includes("nextConcordanceCollocateDisplayCount"));
assert(showMoreBody.includes("renderConcordanceCollocations()"));
assert(!showMoreBody.includes("queryConcordanceCollocations"), "Show more must not rebuild the aggregate");

assert(collocations.includes('evidenceButton.textContent = expanded ? "Hide evidence" : "Inspect evidence";'));
assert(collocations.includes('evidenceButton.setAttribute("aria-expanded", expanded ? "true" : "false");'));
assert(collocations.includes('evidenceButton.addEventListener("click", () => handleConcordanceCollocateClick(row));'));

assert.deepEqual(concordanceContextSortLevels("left2", 5, 5).left, [null, null, 3, 1, 2]);
assert.deepEqual(concordanceContextSortLevels("right2", 5, 5).right, [2, 1, 3, null, null]);
assert.deepEqual(concordanceContextSortLevels("filePosition", 3, 3), {
  left: [null, null, null],
  right: [null, null, null],
});

assert(!styles.includes(".cw-overview-figure:not(.is-collapsed) .cw-overview-figure-title-block"));
assert(figures.includes("titleBlock.append(title, subtitle, stats);"));
assert(figures.indexOf("header.append(titleBlock, actions)") < figures.indexOf("figure.append(header, body, caption)"));
assert(!figures.includes("renderSparklineGlyph"), "expanded figure headers must not include decorative sparklines");

assert(figures.includes('button.setAttribute("aria-pressed", String(option.value === initialValue));'));
assert(styles.includes('.cw-overview-segment[aria-pressed="true"]'));
assert(styles.includes('.cw-overview-figure-action[aria-pressed="true"]'));
assert(shell.includes('const controls = createOverviewSegmentedControl(\n    "Stopword mode"'));

assert(figures.includes("const exportedSvg = svg.cloneNode(true) as SVGSVGElement;"));
assert(figures.includes("decorateFigureSvg(exportedSvg, options);"));
const replaceStart = figures.indexOf("export function replaceFigureSvg(");
const replaceEnd = figures.indexOf("function refreshFigureText", replaceStart);
assert(!figures.slice(replaceStart, replaceEnd).includes("decorateFigureSvg"), "live SVG must remain geometry-only");
