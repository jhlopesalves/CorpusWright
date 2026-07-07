import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildVocabularyGrowthGeometry,
  createPlotBox,
  createScale,
  estimateLabelBox,
  findFirstCoverageCrossing,
  findMilestoneShare,
  formatDispersionProfileStats,
  formatUniversalTypeCaption,
  linearTicks,
  logTicks,
  placeLabels,
  scaleValue,
} from "../src/plots/model.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertClose(actual: number, expected: number, message: string): void {
  assert(Math.abs(actual - expected) < 1e-9, `${message}: expected ${expected}, got ${actual}`);
}

const linear = createScale("linear", [0, 100], [10, 210]);
assertClose(scaleValue(linear, 50), 110, "Linear scale should interpolate midway");

const log = createScale("log", [1, 1000], [0, 300]);
assertClose(scaleValue(log, 10), 100, "Log scale should place powers evenly");

const logTickValues = logTicks(log).map((tick) => tick.value);
for (const value of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 90, 100, 1000]) {
  assert(logTickValues.includes(value), `Log ticks should include ${value}`);
}
assert(
  logTicks(log).some((tick) => tick.value === 20 && !tick.major && tick.label === ""),
  "Log ticks should mark 2..9 decade multiples as minor ticks",
);

const linearTickLabels = linearTicks(createScale("linear", [0, 1], [100, 0]), 6).map((tick) => tick.label);
assert(
  linearTickLabels.includes("0.4") && linearTickLabels.includes("1"),
  "Linear ticks should use readable round numbers",
);

const coverage = [
  { k: 1, share: 0.12 },
  { k: 2, share: 0.21 },
  { k: 5, share: 0.38 },
  { k: 10, share: 0.55 },
  { k: 100, share: 0.72 },
];
assertClose(findMilestoneShare(coverage, 5) ?? 0, 0.38, "Milestone lookup should return exact shares");
assert(findMilestoneShare(coverage, 1000) === null, "Missing milestones should stay null");
assert(findFirstCoverageCrossing(coverage, 0.5)?.k === 10, "50% crossing should use first real checkpoint");

const labelBox = estimateLabelBox({
  x: 100,
  y: 50,
  text: "hello",
  anchor: "end",
  fontSize: 10,
});
assertClose(labelBox.width, 31, "Label width estimate should use character count and font size");
assertClose(labelBox.height, 13, "Label height estimate should use the line-height multiplier");
assertClose(labelBox.x, 69, "End-anchored label boxes should extend left from x");
assertClose(labelBox.y, 40, "Label box top should account for the baseline");

const rejected = placeLabels(
  [{
    x: 20,
    y: 20,
    text: "hit",
    candidates: [
      { dx: 0, dy: 0, anchor: "start" },
      { dx: 50, dy: 0, anchor: "start" },
    ],
  }],
  [{ kind: "point", x: 25, y: 15, radius: 8 }],
  { left: 0, top: 0, right: 120, bottom: 80 },
);
assert(rejected[0]?.x === 70, "Placement should reject a candidate that intersects an obstacle");

const clamped = placeLabels(
  [{
    x: 116,
    y: 40,
    text: "wide label",
    candidates: [{ dx: 0, dy: 0, anchor: "start" }],
  }],
  [],
  { left: 0, top: 0, right: 120, bottom: 80 },
);
const clampedBox = estimateLabelBox(clamped[0]!);
assert(clampedBox.x + clampedBox.width <= 120 + 1e-9, "Fallback labels should be clamped inside bounds");

const fallback = placeLabels(
  [{
    x: 20,
    y: 20,
    text: "fallback",
    candidates: [
      { dx: 0, dy: 0, anchor: "start" },
      { dx: 60, dy: 0, anchor: "start" },
    ],
  }],
  [
    { kind: "box", box: { x: 18, y: 8, width: 60, height: 26 } },
    { kind: "box", box: { x: 80, y: 8, width: 8, height: 26 } },
  ],
  { left: 0, top: 0, right: 160, bottom: 80 },
);
assert(fallback[0]?.x === 80, "When all candidates collide, placement should keep the least-overlapping one");

const geometry = buildCoverageGeometry({
  points: coverage,
  milestones: [{ k: 10, share: null }],
  box: createPlotBox(420, 260),
});
assert(geometry.halfCrossing?.k === 10, "Coverage geometry should expose the half-corpus crossing");
assert(geometry.milestones.length === 1, "Coverage geometry should emit milestone dots");
assert(
  geometry.labels.some((label) => label.text.startsWith("top 10") && label.x > geometry.milestones[0]!.x && label.y > geometry.milestones[0]!.y),
  "Coverage milestone labels should try the below-right empty region first",
);
assert(
  geometry.labels.some((label) => label.text.startsWith("half the corpus") && label.y < geometry.halfCrossing!.y),
  "Half-corpus label should sit above the 50% reference",
);

const dispersionProfile = buildDispersionGeometry({
  buckets: [
    { key: "df-1", label: "1", count: 50, share: 0.5 },
    { key: "df-2", label: "2", count: 0, share: 0 },
    { key: "df-3-4", label: "3–4", count: 10, share: 0.1 },
    {
      key: "df-90pct",
      label: "≥90% of documents",
      count: 3,
      share: 0.03,
      highlighted: true,
      detail: "3",
    },
  ],
  box: createPlotBox(500, 280, { top: 30, right: 36, bottom: 48, left: 156 }),
});
assert(dispersionProfile.xScale.kind === "log", "Dispersion profile should use a log count axis");
assert(dispersionProfile.xScale.domain[0] > 0, "Dispersion log domain should stay positive");
assert(dispersionProfile.points.length === 4, "Dispersion profile should keep one point per emitted bucket plus the overlay row");
assert(
  dispersionProfile.points.filter((point) => !point.datum.highlighted).length === 3,
  "Dispersion profile should keep all emitted reach bands without coarsening",
);
assert(
  dispersionProfile.points.some((point) => point.rawX === 0 && point.excluded),
  "Zero-count reach bands should remain truthful rows anchored at the log-axis floor",
);
assert(
  dispersionProfile.points.at(-1)?.datum.highlighted === true && dispersionProfile.dividers.length === 1,
  "The ≥90% overlay row should be distinct and divided from the ladder",
);
assert(
  dispersionProfile.labels.some((label) => label.text === "3")
    && !dispersionProfile.labels.some((label) => label.text.includes("the · of")),
  "Universal type tokens must not be emitted as axis-adjacent labels",
);
assert(
  formatDispersionProfileStats(587_151, 13, 101)
    === "587,151 types · 13 reach bands · 101 types in ≥90% of documents",
  "Dispersion profile stats caption should be assembled from payload counts",
);
assert(
  formatUniversalTypeCaption(["the", "of", "and", "to"], 101) === "the · of · and +98 more",
  "Universal type caption should show the first three tokens and the remaining count",
);

const vocabulary = buildVocabularyGrowthGeometry({
  points: [
    { tokens: 1_000, types: 120 },
    { tokens: 10_000, types: 600 },
    { tokens: 100_000, types: 2_200 },
  ],
  box: createPlotBox(420, 260),
});
assert(vocabulary.labels[0]?.lines?.length === 2, "Vocabulary endpoint label should render as two measured lines");
assert(vocabulary.labels[0]?.anchor === "end", "Vocabulary endpoint label should be end anchored near the endpoint");
