import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  CONCORDANCE_DEFAULT_WINDOW,
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
assert.equal(CONCORDANCE_DEFAULT_WINDOW, 10);
assert.match(html, /id="concordance-window-input"[^>]+value="10"/);
assert(state.includes("window: CONCORDANCE_DEFAULT_WINDOW"));

assert.match(
  html,
  /id="concordance-lines-controls" class="concordance-card-controls concordance-lines-controls">/,
);
assert.match(
  html,
  /id="concordance-lines-panel" class="concordance-card concordance-kwic-panel"/,
);
const kwicPanelCss = styles.match(/\.concordance-kwic-panel \{([\s\S]*?)\n\}/)?.[1] ?? "";
assert(
  !kwicPanelCss.includes("border-color: transparent")
    && !kwicPanelCss.includes("background: transparent")
    && !kwicPanelCss.includes("box-shadow: none"),
  "Concordance Lines should inherit the shared result-card surface without a transparent exception",
);
const kwicHeader = html.slice(
  html.indexOf('<thead class="concordance-table-head">'),
  html.indexOf('id="concordance-table-body"'),
);
assert.equal(kwicHeader.match(/<th(?:\s|>)/g)?.length, 4);
assert(!styles.includes(".concordance-table-head {\n  position: sticky;"));
assert(
  html.includes('class="concordance-table-wrap cw-result-table-wrap"')
    && html.includes('class="concordance-collocations-table-wrap cw-result-table-wrap"'),
  "KWIC and Collocates wrappers should consume the shared result-table shell",
);
assert(
  styles.includes(".concordance-table-wrap,\n.concordance-collocations-table-wrap,\n.concordance-distribution-table-wrap {\n  border: 1px solid var(--cw-result-table-border);\n  border-radius: 4px;\n  background: var(--cw-result-table-surface);"),
  "Concordance table wrappers should use the shared border and surface tokens",
);
assert(
  styles.includes(".concordance-table th {\n  height: 22px;\n  background: var(--cw-result-table-header-surface);")
    && styles.includes(".concordance-collocations-table > thead > tr > th {\n  background: var(--cw-result-table-header-surface);"),
  "KWIC and Collocates headers must share the result-table header surface",
);
assert(
  styles.includes(".concordance-table th {\n  height: 22px;\n  background: var(--cw-result-table-header-surface);\n  border-bottom: 1px solid var(--cw-result-table-separator);")
    && styles.includes(".concordance-collocations-table > thead > tr > th {\n  background: var(--cw-result-table-header-surface);\n  border-bottom: 1px solid var(--cw-result-table-separator);"),
  "KWIC and Collocates header separators must share the semantic separator",
);
assert(
  styles.includes(".concordance-row:nth-of-type(even) td {\n  background: var(--cw-result-table-row-alt-surface);")
    && styles.includes(".concordance-collocation-row:nth-child(even) td {\n  background: var(--cw-result-table-row-alt-surface);"),
  "KWIC and Collocates zebra rows must use the same surface token",
);
assert(
  styles.includes(".concordance-context-token.is-sort-level-1")
    && styles.includes(".concordance-context-token.is-sort-level-2")
    && styles.includes(".concordance-context-token.is-sort-level-3")
    && styles.includes(".concordance-node {"),
  "KWIC semantic node and sort-level classes must remain unchanged",
);
assert.match(
  html,
  /id="concordance-collocates-controls" class="concordance-card-controls concordance-collocation-controls">/,
);
for (const label of ["Window", "Sort 1", "Sort 2", "Sort 3", "Rows", "Span", "Direction", "Measure", "Min co-occurrence", "Min corpus frequency", "Stopwords"]) {
  assert(new RegExp(`<span[^>]*>[^<]*(?:<i[^>]*></i>)?${label}</span>`).test(html), `${label} must remain visible`);
}
assert.match(html, /<p id="concordance-lines-stats" class="concordance-card-stats cw-result-table-note">/);
assert.match(html, /<p id="concordance-collocates-stats" class="concordance-card-stats cw-result-table-note">/);
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

const collocateTableHead = html.slice(
  html.indexOf('id="concordance-collocations-table-head"'),
  html.indexOf('id="concordance-collocations-table-body"'),
);
assert.equal(collocateTableHead.match(/<th(?:\s|>)/g)?.length, 5);
assert(!collocateTableHead.includes("Evidence"));
assert(html.includes("Select any collocate to inspect its co-occurrence lines."));
assert(collocations.includes('item.setAttribute("aria-expanded", expanded ? "true" : "false");'));
assert(collocations.includes('item.setAttribute("aria-controls", concordanceCollocateEvidenceDomId(row.surface));'));
assert(collocations.includes('item.addEventListener("click", () => handleConcordanceCollocateClick(row));'));
assert(collocations.includes('if (event.key !== "Enter" && event.key !== " ")'));
assert(collocations.includes("item.append(surface, o11, frequency, score, position);"));
assert(collocations.includes("cell.colSpan = 5;"));
assert(collocations.includes("text.textContent = row.surface;"));
assert(!collocations.includes('appendConcordanceCollocationHeader(row, "Evidence")'));
assert(!collocations.includes("evidenceButton"));
for (const width of ["6rem", "7rem", "11rem", "15rem"]) {
  assert(styles.includes(`width: ${width};`), `five-column Collocates layout should include ${width}`);
}
assert(styles.includes("padding: 5px 7px;"));
assert(!collocations.includes('node.textContent = "NODE"'));
assert(!collocations.includes("is-ramp-"));
assert(styles.includes(".concordance-collocation-row:nth-child(even) td"));
assert(!styles.includes("min-width: 1040px"));
assert(!styles.includes("text-decoration-style: double;"));
assert(!styles.includes("text-decoration-style: dotted;"));

assert.deepEqual(concordanceContextSortLevels(["left2", "left1", "left3"], 5, 5).left, [null, null, 3, 1, 2]);
assert.deepEqual(concordanceContextSortLevels(["right2", "right1", "right3"], 5, 5).right, [2, 1, 3, null, null]);
assert.deepEqual(concordanceContextSortLevels(["filePosition"], 3, 3), {
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
