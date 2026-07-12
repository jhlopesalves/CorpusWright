import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  buildDispersionGeometry,
  buildDispersionRosterGeometry,
  dispersionReachBounds,
  scaleValue,
  selectDispersionRosterEntries,
} from "../src/plots/model.ts";
import { OverviewFigureController } from "../src/plots/figure-workbench.ts";

const buckets = [
  { key: "df-1", label: "1", lowerBound: 1, upperBound: 1, count: 50, share: 0.5, sampleTypes: ["local"] },
  { key: "df-2-9", label: "2–9", lowerBound: 2, upperBound: 9, count: 30, share: 0.3, sampleTypes: ["regional"] },
  { key: "df-10-plus", label: "10+", lowerBound: 10, upperBound: null, count: 20, share: 0.2, sampleTypes: ["general"] },
];
const countProfile = buildDispersionGeometry({
  buckets,
  totalTypes: 100,
  totalDocuments: 20,
  xMode: "count",
  reachLabels: "documents",
  labelsMode: "countsShares",
  cumulativeCallout: { count: 4, examples: ["the", "and"] },
});
assert.equal(countProfile.compositionStrip?.segments.reduce((sum, segment) => sum + segment.datum.count, 0), 100);
assert.deepEqual(
  countProfile.compositionStrip?.segments.map((segment) => segment.datum.rampIndex),
  [...new Set(countProfile.points.map((point) => point.rampIndex))],
);
assert(!countProfile.points.some((point) => point.datum.key.includes("90")));
assert.equal(countProfile.cumulativeCallout?.title, "4 types occur in at least 90% of documents");
assert.match(countProfile.cumulativeCallout?.detail ?? "", /Cumulative threshold, not an exclusive band/);
assert.equal(countProfile.xScale.kind, "log");
assert.equal(countProfile.points[0]?.x, scaleValue(countProfile.xScale, 50));
assert(countProfile.labels.some((label) => label.text === "1 document"));
assert(countProfile.points[1]?.tooltipRows?.some((row) => row.includes("examples regional")));

const shareProfile = buildDispersionGeometry({
  buckets,
  totalTypes: 100,
  totalDocuments: 20,
  xMode: "share",
  reachLabels: "percentage",
  labelsMode: "counts",
});
assert.equal(shareProfile.xScale.kind, "linear");
assert.equal(shareProfile.points[1]?.rawX, 0.3);
assert.equal(shareProfile.points[1]?.x, scaleValue(shareProfile.xScale, 0.3));
assert(shareProfile.labels.some((label) => label.text === "10%–45% of documents"));
assert.deepEqual(
  dispersionReachBounds(buckets[2]!, 20),
  { documents: "10+ documents", percentage: "50%–100% of documents" },
);

const rosterEntries = [
  { token: "zeta", frequency: 100, documentFrequency: 8, dp: 0.2 },
  { token: "alpha", frequency: 100, documentFrequency: 4, dp: 0.6 },
  { token: "beta", frequency: 80, documentFrequency: 8, dp: 0.6 },
  ...Array.from({ length: 30 }, (_, index) => ({
    token: `token-${String(index).padStart(2, "0")}`,
    frequency: 70 - index,
    documentFrequency: (index % 9) + 1,
    dp: index / 100,
  })),
];
assert.equal(selectDispersionRosterEntries({ entries: rosterEntries, sort: "frequency", rows: 20 }).length, 20);
assert.deepEqual(
  selectDispersionRosterEntries({ entries: rosterEntries, sort: "frequency", rows: 3 }).map((entry) => entry.token),
  ["alpha", "zeta", "beta"],
);
assert.deepEqual(
  selectDispersionRosterEntries({ entries: rosterEntries, sort: "dp", rows: 2 }).map((entry) => entry.token),
  ["alpha", "beta"],
);
assert.deepEqual(
  selectDispersionRosterEntries({ entries: rosterEntries, sort: "documentFrequency", rows: 2 }).map((entry) => entry.token),
  ["token-08", "token-17"],
);

const roster = buildDispersionRosterGeometry({
  entries: rosterEntries.slice(0, 3),
  highDispersionThreshold: 0.5,
  totalTokens: 1_000,
  totalDocuments: 10,
  pinnedKeys: new Set(["roster:alpha"]),
});
assert.equal(roster.rows[0]?.point.className, "is-dp-observed");
assert.equal(roster.rows[1]?.point.className, "is-dp-attention");
assert.equal(roster.rows[2]?.point.className, "is-dp-attention");
assert.equal(roster.rows[1]?.point.pinned, true);
assert.equal(roster.rows[1]?.tokenLabel.pinned, true);
assert(roster.rows[0]?.point.tooltipRows?.some((row) => row.includes("100,000.00 per million")));
assert(roster.rows[0]?.point.tooltipRows?.some((row) => row.includes("document reach 80")));
assert.deepEqual(roster.headers.map((header) => header.text), ["TOKEN", "DP PLOTTING LANE", "FREQUENCY", "DOCUMENTS"]);
assert.deepEqual(roster.legend.map((item) => item.label), [
  "Below descriptive DP threshold",
  "At or above descriptive DP threshold",
]);
assert.equal(roster.thresholdLabel.text, "DP 0.50 descriptive threshold");

const controller = new OverviewFigureController({
  initialControls: { stopwords: "show", sort: "frequency", rows: "20" },
  exportMetadata: (controls) => [
    `Top ${controls.rows} frequent types · sorted by ${controls.sort} · stopwords ${controls.stopwords === "hide" ? "hidden" : "shown"}`,
  ],
});
controller.updateControls({ stopwords: "hide", sort: "dp", rows: "50" });
controller.togglePin("roster:alpha");
assert.deepEqual(controller.exportMetadata(), ["Top 50 frequent types · sorted by dp · stopwords hidden"]);
assert(controller.pins.has("roster:alpha"));

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const render = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles/plots/legends-and-annotations.css", import.meta.url), "utf8");
assert.match(shell, /initialControls: \{ stopwords: "show", sort: "frequency", rows: "20" \}/);
assert.match(shell, /Which frequent types are corpus-wide, and which are locally concentrated\?/);
assert.match(shell, /Stopwords hidden/);
assert.doesNotMatch(shell, /DP 0\.5 is (?:a )?significance/i);
assert.match(render, /data-pin-key": point\.key/);
assert.match(render, /frequencyLabel, row\.documentsLabel/);
assert.match(styles, /cw-plot-roster-number[\s\S]*font-variant-numeric: tabular-nums[\s\S]*font-weight: var\(--cw-weight-regular\)/);
