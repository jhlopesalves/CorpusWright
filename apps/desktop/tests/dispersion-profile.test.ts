import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildDispersionGeometry,
  dispersionReachBounds,
  scaleValue,
} from "../src/plots/model.ts";

const buckets = Array.from({ length: 10 }, (_, index) => ({
  key: `df-${index + 1}`,
  label: String(index + 1),
  lowerBound: index + 1,
  upperBound: index + 1,
  count: 100 - index * 7,
  share: (100 - index * 7) / 685,
  sampleTypes: index === 1
    ? ["sample-1-a", "sample-1-b", "sample-1-c", "sample-1-d", "sample-1-e", "sample-1-f"]
    : [`sample-${index}`],
}));
const countProfile = buildDispersionGeometry({
  buckets,
  totalTypes: 685,
  totalDocuments: 20,
  xMode: "count",
  reachLabels: "documents",
  labelsMode: "countsShares",
  cumulativeCallout: { count: 4, examples: ["the", "and"] },
});
assert.equal(countProfile.compositionStrip?.segments.reduce((sum, segment) => sum + segment.datum.count, 0), 685);
assert.equal(new Set(countProfile.points.map((point) => point.className)).size, 10);
assert.deepEqual(
  countProfile.points.map((point) => point.className),
  Array.from({ length: 10 }, (_, index) => `cw-plot-profile-band-${index}`),
);
assert.deepEqual(
  countProfile.compositionStrip?.segments.map((segment) => segment.datum.className),
  countProfile.points.map((point) => point.className),
);
assert.equal(countProfile.compositionStrip?.segments[0]?.datum.label, "1 document");
assert.equal(countProfile.compositionStrip?.segments[1]?.datum.label, "2 documents");
assert(!countProfile.points.some((point) => point.datum.key.includes("90")));
assert.equal(countProfile.cumulativeCallout?.title, "4 types occur in at least 90% of documents");
assert.match(countProfile.cumulativeCallout?.detail ?? "", /Cumulative threshold, not an exclusive band/);
assert.equal(countProfile.xScale.kind, "log");
assert.equal(countProfile.points[0]?.x, scaleValue(countProfile.xScale, 100));
assert(countProfile.labels.some((label) => label.text === "1 document"));
assert.deepEqual(countProfile.points[1]?.tooltipRows, [
  "93 types",
  `${((93 / 685) * 100).toFixed(1)}% of vocabulary`,
  "Reach: 2 of 20 documents",
  "Examples: sample-1-a, sample-1-b, sample-1-c +90 more",
]);
assert.equal(countProfile.points[1]?.tooltipRows?.find((row) => row.startsWith("Examples:"))?.split(",").length, 3);
assert(!countProfile.points[1]?.tooltipRows?.join(" ").includes("sample-1-d"));
assert.deepEqual(countProfile.compositionStrip?.segments[1]?.tooltipRows, countProfile.points[1]?.tooltipRows);

const shareProfile = buildDispersionGeometry({
  buckets: buckets.slice(0, 3),
  totalTypes: 685,
  totalDocuments: 20,
  xMode: "share",
  reachLabels: "percentage",
  labelsMode: "counts",
});
assert.equal(shareProfile.xScale.kind, "linear");
assert.equal(shareProfile.points[1]?.rawX, buckets[1]!.share);
assert.deepEqual(
  dispersionReachBounds({ ...buckets[2]!, lowerBound: 10, upperBound: null }, 20),
  { documents: "10+ documents", percentage: "50%–100% of documents" },
);

const singularProfile = buildDispersionGeometry({
  buckets: [{ key: "single", label: "1 document", lowerBound: 1, upperBound: 1, count: 1, share: 1, sampleTypes: ["only"] }],
  totalTypes: 1,
  totalDocuments: 1,
});
assert.deepEqual(singularProfile.points[0]?.tooltipRows, [
  "1 type",
  "100.0% of vocabulary",
  "Reach: 1 of 1 document",
  "Examples: only",
]);

const palette = readFileSync(new URL("../src/styles/foundations/figure-palettes.css", import.meta.url), "utf8");
const marks = readFileSync(new URL("../src/styles/plots/marks.css", import.meta.url), "utf8");
const annotations = readFileSync(new URL("../src/styles/plots/legends-and-annotations.css", import.meta.url), "utf8");
const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const model = readFileSync(new URL("../src/plots/model.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const workbench = readFileSync(new URL("../src/plots/figure-workbench.ts", import.meta.url), "utf8");
for (let index = 0; index < 10; index += 1) {
  assert.match(palette, new RegExp(`--cw-plot-profile-${index}:`));
  assert.match(marks, new RegExp(`\\.cw-plot-profile-band-${index}`));
}
assert.match(annotations, /cw-plot-cumulative-callout-title/);
assert.match(annotations, /cw-plot-cumulative-callout-detail/);
assert.match(annotations, /composition-entry\.is-wide\[class\*="cw-plot-profile-band-"\]/);
assert.match(annotations, /composition-entry\.is-medium\[class\*="cw-plot-profile-band-"\][^\n]+is-category/);
assert.doesNotMatch(annotations, /composition-entry\[class\*="cw-plot-profile-band-"\] \.cw-plot-composition-label/);
for (const source of [shell, model, render, workbench, marks, annotations]) {
  assert.doesNotMatch(source, /Dispersion roster|DispersionRoster|dispersion-roster|cw-plot-roster/);
}
assert.match(render, /renderDispersionSvg[\s\S]*renderInteractionLayer\(svg, geometry\.box, false\)/);
