import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildDispersionPlaneGeometry,
  buildDispersionRosterGeometry,
  buildLengthHistogramGeometry,
  buildLexicalSpectrumGeometry,
  buildMattrDistributionGeometry,
  buildRankFrequencyGeometry,
  buildVocabularyGrowthGeometry,
  createPlotBox,
  createScale,
  estimateLabelBox,
  findFirstCoverageCrossing,
  findMilestoneShare,
  formatDispersionPlaneStats,
  formatDispersionProfileStats,
  formatDispersionRosterStats,
  formatLengthHistogramStats,
  formatMattrDistributionStats,
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

const lexicalSpectrum = buildLexicalSpectrumGeometry({
  points: [
    { frequency: 1, typeCount: 42 },
    { frequency: 2, typeCount: 20 },
    { frequency: 3, typeCount: 11 },
    { frequency: 9, typeCount: 3 },
    { frequency: 10, typeCount: 2 },
    { frequency: 100, typeCount: 1 },
  ],
  hapaxCount: 42,
  disLegomenaCount: 20,
  box: createPlotBox(600, 320),
});
assert(
  lexicalSpectrum.bands.map((band) => band.label).join(",") === "HAPAX,DIS,3–9,10+",
  "Lexical spectrum should render its four exclusive frequency-class bands",
);
assert(
  lexicalSpectrum.points.map((point) => point.rampIndex).join(",") === "0,1,2,2,4,4",
  "Lexical spectrum points should take their colour from the frequency class",
);
assert(
  lexicalSpectrum.points.at(-1)?.fillOpacity === 0.42
    && lexicalSpectrum.points[0]?.fillOpacity === 0.6,
  "Frequency-singleton tail points should have lower opacity than the main sequence",
);
assert(
  lexicalSpectrum.labels.some((label) => label.text.startsWith("hapax"))
    && lexicalSpectrum.labels.some((label) => label.text.startsWith("dis")),
  "Lexical spectrum should retain both hapax and dis witnesses through label placement",
);

const rankFrequencyRaw = buildRankFrequencyGeometry({
  points: [
    { rank: 1, frequency: 1_000, ratePerMillion: 100_000, token: "the" },
    { rank: 2, frequency: 700, ratePerMillion: 70_000, token: "of" },
    { rank: 3, frequency: 500, ratePerMillion: 50_000, token: "and" },
    { rank: 4, frequency: 300, ratePerMillion: 30_000, token: "word" },
    { rank: 5, frequency: 1, ratePerMillion: 100, token: "rare" },
  ],
  yMode: "raw",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 4,
});
const rankFrequencyRate = buildRankFrequencyGeometry({
  points: rankFrequencyRaw.points.map((point) => point.datum),
  yMode: "rate",
  totalTokens: 10_000,
  zipfExponent: 1,
  zipfCoefficient: 1_200,
  fittedRankStart: 4,
  fittedRankEnd: 4,
});
assert(
  rankFrequencyRaw.points.map((point) => `${point.x},${point.y}`).join(",")
    === rankFrequencyRate.points.map((point) => `${point.x},${point.y}`).join(","),
  "Rank-frequency unit changes should relabel a single series rather than rescale its geometry",
);
assert(
  rankFrequencyRate.points[0]?.tooltipRows?.join(" ").includes("1,000 tokens")
    && rankFrequencyRate.points[0]?.tooltipRows?.join(" ").includes("100,000.00 per million"),
  "Rank-frequency hover should always expose both raw frequency and rate per million",
);
assert(
  rankFrequencyRaw.points.filter((point) => point.excluded).length === 4
    && rankFrequencyRaw.legend[0]?.label === "fit: ranks 4–4"
    && rankFrequencyRaw.legend[1]?.label === "excluded: top 3 and f=1 tail",
  "Rank-frequency should explicitly disclose the fitted range and hollow exclusions",
);
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
  dispersionProfile.compositionSegments.length === 3
    && dispersionProfile.compositionSegments.reduce((total, segment) => total + segment.width, 0) > 0
    && dispersionProfile.compositionSegments.some((segment) => segment.label !== null),
  "Dispersion profile should add a labelled stacked composition strip for exclusive bands",
);
assert(
  dispersionProfile.stems.at(-1) !== undefined
    && dispersionProfile.points.at(-1)?.datum.highlighted === true,
  "Dispersion profile should preserve the distinct cumulative annotation datum for square rendering",
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

const planeCounts = new Array<number>(48 * 32).fill(0);
planeCounts[0] = 1;
planeCounts[47] = 4;
planeCounts[(16 * 48) + 20] = 9;
const dispersionPlane = buildDispersionPlaneGeometry({
  plane: {
    xBins: 48,
    yBins: 32,
    freqLog10Max: 5.025,
    counts: planeCounts,
    maxCellCount: 9,
    witnesses: [
      { token: "the", frequency: 40_000, documentFrequency: 180, dp: 0.08, kind: "core" },
      { token: "boilerplate", frequency: 3_500, documentFrequency: 12, dp: 0.72, kind: "loudButLocal" },
      { token: "folio", frequency: 2, documentFrequency: 1, dp: 0.91, kind: "rareAndLocal" },
    ],
    loudButLocalCount: 2,
    loudButLocalMinFrequency: 1_000,
    highDispersionThreshold: 0.5,
    reading: "A frequency list cannot separate the core from boilerplate; the plane can.",
  },
  totalTypes: 100,
  box: createPlotBox(720, 420, { top: 34, right: 48, bottom: 62, left: 112 }),
});
assert(dispersionPlane.cells.length === 3, "Dispersion plane should draw exactly one rect for each nonzero cell");
const densityCell = dispersionPlane.cells.find((cell) => cell.count === 4);
assert(densityCell !== undefined, "Dispersion plane fixture should include the count-4 density cell");
const expectedDensityT = Math.log1p(4) / Math.log1p(9);
assertClose(densityCell.t, expectedDensityT, "Dispersion plane density t should use the log-scaled cell count");
assertClose(densityCell.opacity, 0.12 + (0.80 * expectedDensityT), "Dispersion plane opacity should follow the specified formula");
const decadeTickLabels = dispersionPlane.xTicks.filter((tick) => tick.major).map((tick) => tick.label);
assert(
  ["1", "10", "100", "1k", "10k", "100k"].every((label) => decadeTickLabels.includes(label))
    && !decadeTickLabels.includes("1M"),
  "Dispersion plane decade ticks should stop at freqLog10Max",
);
assert(
  dispersionPlane.xTicks.some((tick) => !tick.major && Math.abs(tick.value - Math.log10(20)) < 1e-9),
  "Dispersion plane should emit 2x-9x minor decade ticks in log10 units",
);
assert(
  dispersionPlane.witnessObstacles.length > dispersionPlane.witnesses.length
    && dispersionPlane.witnessObstacles.some((obstacle) => obstacle.kind === "box"),
  "Dispersion plane witness placement should carry point and box obstacles",
);
assert(
  formatDispersionPlaneStats(100, 48, 32, 2, 1_000, 0.5)
    === "100 types · grid 48×32 · 2 loud-but-local (f ≥ 1k, DP ≥ 0.5)",
  "Dispersion plane stats caption should use the exact computed template",
);

const dispersionRoster = buildDispersionRosterGeometry({
  entries: [
    { token: "the", frequency: 1_000, documentFrequency: 10, dp: 0.12 },
    { token: "chapter", frequency: 900, documentFrequency: 2, dp: 0.64 },
    { token: "boilerplate", frequency: 800, documentFrequency: 1, dp: 0.5 },
  ],
  highDispersionThreshold: 0.5,
  box: createPlotBox(520, 150, { top: 34, right: 48, bottom: 58, left: 156 }),
});
assert(
  dispersionRoster.rows.map((row) => row.entry.token).join(",") === "the,chapter,boilerplate",
  "Dispersion roster should preserve descending-frequency payload order",
);
assert(
  dispersionRoster.rows[0]?.point.rampIndex === 2
    && dispersionRoster.rows[1]?.point.rampIndex === 5
    && dispersionRoster.rows[2]?.point.rampIndex === 5,
  "Dispersion roster should use the hottest ramp stop only at or above the DP threshold",
);
assert(dispersionRoster.aboveThresholdCount === 2, "Dispersion roster should count rows at or above DP 0.5");
assert(
  formatDispersionRosterStats(3, dispersionRoster.aboveThresholdCount, 0.5)
    === "top 3 types by frequency · 2 above DP 0.5",
  "Dispersion roster stats caption should report the threshold count",
);

const lengthHistogram = buildLengthHistogramGeometry({
  bins: [
    { lowerTokens: 256, upperTokens: 512, count: 2 },
    { lowerTokens: 512, upperTokens: 1024, count: 5 },
    { lowerTokens: 1024, upperTokens: 2048, count: 3 },
  ],
  documents: 10,
  medianTokens: 800,
  minimumTokens: 260,
  maximumTokens: 1_500,
  box: createPlotBox(520, 300, { top: 34, right: 40, bottom: 72, left: 72 }),
});
assert(lengthHistogram.bars.length === 3, "Length histogram should emit one bar per payload bin");
const medianX = lengthHistogram.medianLine?.x1 ?? 0;
const medianBinBar = lengthHistogram.bars[1]!;
assert(
  medianX >= medianBinBar.x && medianX <= medianBinBar.x + medianBinBar.width,
  "Length histogram median line should land inside the bin containing medianTokens",
);
assert(
  lengthHistogram.xTicks.map((tick) => tick.label).join(",") === "256,,1k,2k",
  "Length histogram should label stable power-of-two bin boundaries without range collisions",
);
assert(
  lengthHistogram.bars[1]?.tooltipRows?.[0] === "512–1,024 tokens",
  "Length histogram hover should retain each bar's exact bin bounds",
);
assert(
  formatLengthHistogramStats(10, 800, 260, 1_500)
    === "10 documents · median 800 · range 260–1,500 tokens",
  "Length histogram stats caption should match the specified template",
);

const mattrCounts = new Array<number>(40).fill(0);
mattrCounts[10] = 2;
mattrCounts[11] = 3;
mattrCounts[14] = 1;
const mattrDistribution = buildMattrDistributionGeometry({
  distribution: {
    binWidth: 0.025,
    counts: mattrCounts,
    usedDocuments: 6,
    excludedDocuments: 1,
    lowWitnesses: [
      { name: "a-very-long-document-name-for-testing.txt", mattr: 0.255, tokens: 1_200 },
    ],
    highWitnesses: [
      { name: "compact.txt", mattr: 0.365, tokens: 900 },
    ],
    reading: "Low-MATTR outliers are where cleaning failures hide.",
  },
  median: 0.3,
  box: createPlotBox(520, 320, { top: 34, right: 40, bottom: 104, left: 72 }),
});
assertClose(mattrDistribution.domain[0], 0.225, "MATTR occupied domain should pad one bin below the first nonzero bin");
assertClose(mattrDistribution.domain[1], 0.4, "MATTR occupied domain should pad one bin above the last nonzero bin");
assert(mattrDistribution.bars.length === 7, "MATTR distribution should emit one bar per visible bin in the occupied window");
assert(
  mattrDistribution.labels.filter((label) => label.text.includes("document") || label.text.includes("compact")).length <= 2
    && mattrDistribution.labels.some((label) => label.text.length === 24 && label.text.includes("…")),
  "MATTR should label only the extreme witnesses, middle-ellipsized to 24 characters",
);
assert(
  mattrDistribution.witnessTicks.length === 2
    && mattrDistribution.witnessTicks.every((tick) => tick.tooltipTitle.length > 0),
  "MATTR witness identities should remain reachable on hover",
);
assert(
  formatMattrDistributionStats(6, 0.3, 1, 100)
    === "6 documents · median 0.300 · 1 excluded (< 100 tokens)",
  "MATTR distribution stats caption should match the specified template",
);

const vocabulary = buildVocabularyGrowthGeometry({
  points: [
    { tokens: 1_000, types: 120 },
    { tokens: 10_000, types: 600 },
    { tokens: 100_000, types: 2_200 },
  ],
  heapsK: 1.5,
  heapsBeta: 0.7,
  box: createPlotBox(420, 260),
});
assert(vocabulary.labels[0]?.lines?.length === 2, "Vocabulary endpoint label should render as two measured lines");
assert(vocabulary.labels[0]?.anchor === "end", "Vocabulary endpoint label should be end anchored near the endpoint");
assert(
  vocabulary.fitPath !== null && vocabulary.legend.map((item) => item.label).join(",") === "observed,Heaps fit",
  "Vocabulary growth should show a distinct Heaps fitted series and legend",
);
