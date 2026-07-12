import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";
import {
  concordanceCollocatePositionProfile,
  concordancePositionProfileAccessibleText,
  concordancePositionProfileAxisLabels,
  concordancePositionProfileDomain,
  concordancePositionProfileSummaryText,
  concordancePositionProfileTooltip,
} from "../src/concordance.ts";

const source = readFileSync(
  new URL("../src/workbench/concordance/collocations.ts", import.meta.url),
  "utf8",
);
const styles = (await loadStyleGraph(new URL("../src/styles.css", import.meta.url))).css;
const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");

const diffuse = { positionCounts: { left: [1, 1], right: [1, 1] } };
const concentrated = { positionCounts: { left: [100, 0], right: [0, 0] } };
const mixed = { positionCounts: { left: [4, 2], right: [1, 3] } };
const loadedRows = [diffuse, mixed, concentrated];

const shapeDomain = concordancePositionProfileDomain(loadedRows, 2, "both", "shape");
const countDomain = concordancePositionProfileDomain(loadedRows, 2, "both", "count");
assert.equal(shapeDomain, 1, "Shape domain must include a concentrated row outside the initial slice");
assert.equal(countDomain, 100, "Count domain must include the largest raw bin in the complete loaded set");
assert.equal(
  concordancePositionProfileDomain(loadedRows.slice(0, 2), 2, "both", "shape"),
  0.4,
  "A visible-only slice would produce a different Shape domain, proving the loaded-set contract matters",
);
assert.equal(
  concordancePositionProfileDomain(loadedRows.slice(0, 2), 2, "both", "count"),
  4,
  "A visible-only slice would produce a different Count domain, proving Show More must not rescale",
);

const shape = concordanceCollocatePositionProfile(
  mixed.positionCounts,
  2,
  "both",
  "shape",
  shapeDomain,
);
assert.deepEqual(shape.bins.map((bin) => bin.label), ["2L", "1L", "1R", "2R"]);
assert.equal(shape.summary.positionalTotal, 10);
assert.equal(shape.bins.find((bin) => bin.label === "1L")?.share, 4 / 10);
assert.equal(shape.bins.find((bin) => bin.label === "1L")?.value, 4 / 10);
assert.equal(shape.bins.find((bin) => bin.label === "1L")?.ratio, 0.4);
assert.notEqual(
  shape.bins.find((bin) => bin.label === "1L")?.ratio,
  1,
  "A row-local maximum must not receive full height under a larger shared domain",
);

const count = concordanceCollocatePositionProfile(
  mixed.positionCounts,
  2,
  "both",
  "count",
  countDomain,
);
assert.equal(count.bins.find((bin) => bin.label === "1L")?.value, 4);
assert.equal(count.bins.find((bin) => bin.label === "1L")?.ratio, 4 / 100);
assert.notEqual(count.bins[0]!.ratio, count.bins[1]!.ratio, "Unequal counts must retain unequal ratios");

const zero = concordanceCollocatePositionProfile(
  { left: [0, 0], right: [0, 0] },
  2,
  "both",
  "shape",
  shapeDomain,
);
assert(zero.bins.every((bin) => bin.ratio === 0), "Zero counts must map to zero height");

assert.deepEqual(shape.summary, {
  leftTotal: 6,
  rightTotal: 4,
  positionalTotal: 10,
  leftShare: 0.6,
  rightShare: 0.4,
  dominantOffset: "1L",
  dominantCount: 4,
  dominantShare: 0.4,
});
assert.equal(concordancePositionProfileSummaryText(shape.summary, "both"), "L 60.0% · R 40.0%");

const leftOnly = concordanceCollocatePositionProfile(mixed.positionCounts, 2, "left", "shape", 1);
assert.deepEqual(leftOnly.bins.map((bin) => bin.label), ["2L", "1L"]);
assert.equal(leftOnly.summary.leftTotal, 6);
assert.equal(leftOnly.summary.rightTotal, 0);
assert.equal(concordancePositionProfileSummaryText(leftOnly.summary, "left"), "L 6 occurrences");
const rightOnly = concordanceCollocatePositionProfile(mixed.positionCounts, 2, "right", "shape", 1);
assert.deepEqual(rightOnly.bins.map((bin) => bin.label), ["1R", "2R"]);
assert.equal(rightOnly.summary.leftTotal, 0);
assert.equal(rightOnly.summary.rightTotal, 4);
assert.equal(concordancePositionProfileSummaryText(rightOnly.summary, "right"), "R 4 occurrences");

assert.deepEqual(concordancePositionProfileAxisLabels(2, "both"), {
  full: "2L 1L | node | 1R 2R",
  compact: "2L…1L | node | 1R…2R",
});
assert.equal(
  concordancePositionProfileAxisLabels(7, "left").full,
  "7L 6L 5L 4L 3L 2L 1L | node",
);

const dominantBin = shape.bins.find((bin) => bin.label === "1L")!;
const tooltip = concordancePositionProfileTooltip(dominantBin, "shape", shapeDomain);
assert.match(tooltip, /^1L: 4 occurrences · 40\.0%/);
assert.match(tooltip, /Shape mode · shared scale: 100\.0% maximum positional share across loaded rows/);
const accessible = concordancePositionProfileAccessibleText("alpha", shape, "shape", shapeDomain);
for (const witness of [
  "Left 6 occurrences, 60.0%",
  "right 4 occurrences, 40.0%",
  "dominant offset 1L, 4 occurrences, 40.0%",
  "Shared scale: 100.0% maximum positional share across all loaded rows",
  "2L 2",
  "2R 3",
]) {
  assert(accessible.includes(witness), `Accessible profile summary must retain ${witness}`);
}

const modeHandler = source.match(
  /export function handleConcordancePositionProfileModeClick\(event: Event\): void \{([\s\S]*?)\n\}/,
)?.[1] ?? "";
assert.match(modeHandler, /concordancePositionProfileMode = nextMode/);
assert.match(modeHandler, /renderConcordanceCollocations\(\)/);
assert.doesNotMatch(
  modeHandler,
  /queryConcordanceCollocations|refreshConcordanceCollocations|markConcordanceCollocationsForAggregationChange|searchConcordance/,
  "Shape/Count switching must be presentation-only",
);
assert.match(source, /concordancePositionProfileDomain\(\s*rows,/);
assert.match(source, /positionLegend\.classList\.toggle\("is-condensed", span > 5\)/);
assert.doesNotMatch(source, /Math\.max\(12, bin\./, "Broad minimum-height clamps must be removed");
assert.doesNotMatch(source, /node\.textContent\s*=\s*["']NODE["']/i, "NODE must not repeat in rows");
assert.match(source, /element\.title = concordancePositionProfileTooltip/);
assert.match(source, /concordancePositionProfileAccessibleText/);
assert.match(styles, /height:\s*calc\(var\(--position-ratio, 0\) \* 100%\)/);
assert.match(styles, /\.concordance-position-bin\.is-zero \{\s*height:\s*0/);
assert.doesNotMatch(styles, /\.concordance-position-bin[^}]*min-height:\s*[2-9]px/s);
assert(html.includes('data-position-profile-mode="shape" aria-pressed="true">Shape</button>'));
assert(html.includes('data-position-profile-mode="count" aria-pressed="false">Count</button>'));
assert(!html.includes("Position profiles are normalised within each collocate row."));
