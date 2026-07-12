import {
  CONCORDANCE_COLUMNS,
  CONCORDANCE_DEFAULT_WINDOW,
  CONCORDANCE_COLLOCATION_DEFAULT_FLOOR,
  CONCORDANCE_COLLOCATION_DEFAULT_SPAN,
  CONCORDANCE_COLLOCATION_LIMIT,
  CONCORDANCE_COLLOCATION_INITIAL_ROWS,
  CONCORDANCE_COLLOCATION_MEASURE_OPTIONS,
  CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE,
  CONCORDANCE_COLLOCATION_STALE_MESSAGE,
  CONCORDANCE_COLLOCATION_SORT_OPTIONS,
  DEFAULT_CONCORDANCE_COLLOCATION_MEASURE,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION,
  DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY,
  CONCORDANCE_EXPANSION_WINDOW,
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  CONCORDANCE_SORT_OPTIONS,
  DEFAULT_CONCORDANCE_SORT_KEYS,
  clampConcordanceCollocationFloor,
  clampConcordanceCollocationSpan,
  clampConcordancePageSize,
  clampConcordanceWindow,
  concordanceCollocatesCardStatsText,
  concordanceCollocateEvidenceDomId,
  concordanceCollocationDirectionLabel,
  concordanceCollocationExplanation,
  concordanceCollocationMeasureLabel,
  concordanceCollocationScore,
  concordanceCollocationStrengthBar,
  concordanceCollocationSortHeaderLabel,
  concordanceContextTokenSegments,
  concordanceDistributionCardStatsText,
  concordanceDistributionMagnitudePercent,
  concordanceCollocatePositionGlyph,
  concordanceContextSortLevels,
  concordanceExpansionViewModel,
  concordanceIsSparse,
  concordanceLinesCardStatsText,
  concordanceRangePercent,
  concordanceMiddleEllipsis,
  concordancePositionGlyphAccessibleText,
  concordanceRowExpansionKey,
  concordanceRowText,
  concordanceSortLabel,
  concordanceSortSequence,
  concordanceSummaryMetricsText,
  concordanceSummaryText,
  formatConcordanceProgress,
  normaliseConcordanceQuery,
  toggleConcordanceExpansionKey,
  nextConcordanceCollocateDisplayCount,
} from "../src/concordance.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertDeepEqual(actual: unknown, expected: unknown, message = "Values should match"): void {
  assert(JSON.stringify(actual) === JSON.stringify(expected), message);
}

assert(normaliseConcordanceQuery("  alpha ") === "alpha", "Concordance query should be trimmed");
assert(clampConcordanceWindow(Number.NaN) === 10, "Invalid windows should fall back to the default");
assert(clampConcordanceWindow(0) === 10, "Zero windows should fall back to the default");
assert(clampConcordanceWindow(24) === 20, "Windows should be capped at the hard maximum");
assert(clampConcordanceWindow(3.8) === 3, "Windows should use whole tokens");
assert(clampConcordanceWindow(5) === 5, "An explicit five-token window should remain unchanged");
assert(clampConcordanceWindow(15) === 15, "An explicit fifteen-token window should remain unchanged");
assert(clampConcordancePageSize(0) === 100, "Invalid page sizes should fall back to the default");
assert(clampConcordancePageSize(12) === 25, "Page sizes should respect the minimum");
assert(clampConcordancePageSize(900) === 500, "Page sizes should respect the maximum");
assert(clampConcordanceCollocationSpan(Number.NaN) === 5, "Invalid collocation spans should fall back");
assert(clampConcordanceCollocationSpan(24) === 20, "Collocation spans should be capped");
assert(clampConcordanceCollocationFloor(-1) === 5, "Invalid collocation floors should fall back");
assert(clampConcordanceCollocationFloor(3.8) === 3, "Collocation floors should use whole counts");
assertDeepEqual(DEFAULT_CONCORDANCE_SORT_KEYS, ["right1", "right2", "right3"], "Default sort should be 1R / 2R / 3R");
assert(CONCORDANCE_DEFAULT_WINDOW === 10, "Default KWIC context should be ten tokens per side");
assert(DEFAULT_CONCORDANCE_COLLOCATION_MEASURE === "logDice", "Default collocation measure should be logDice");
assert(DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY === "association", "Default collocation sort should be association");
assert(DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION === "desc", "Default collocation sort should descend");

assert(formatConcordanceProgress(null) === "Searching...", "Progress should fallback to Searching... when null");
assert(
  formatConcordanceProgress({ filesDone: 0, filesTotal: 10, currentPhase: "buildingIndex" }) === "Reading index... 0%",
  "Progress should report reading index"
);
assert(
  formatConcordanceProgress({ filesDone: 5, filesTotal: 10, currentPhase: "searching" }) === "Searching... 50%",
  "Progress should report searching"
);
assert(
  formatConcordanceProgress({ filesDone: 10, filesTotal: 10, currentPhase: "aggregatingCollocations" }) === "Aggregating collocates... 100%",
  "Progress should report aggregating collocates"
);
assert(CONCORDANCE_COLLOCATION_DEFAULT_SPAN === 5, "Default collocation span should be +/-5");
assert(CONCORDANCE_COLLOCATION_DEFAULT_FLOOR === 5, "Default collocation floors should be 5");
assert(CONCORDANCE_COLLOCATION_LIMIT === 500, "Collocation results should request the bounded top 500");
assert(CONCORDANCE_COLLOCATION_INITIAL_ROWS === 10, "Collocates should initially show 10 rows");
assert(
  CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE === "The collocate profile has not been computed yet.",
  "Collocate Profile should expose an explicit not-computed state",
);
assert(
  CONCORDANCE_COLLOCATION_STALE_MESSAGE === "Update the collocate profile for the current query.",
  "Stale collocate copy should be a single action sentence",
);
assert(CONCORDANCE_EXPANSION_WINDOW === 40, "Expansion should request the default wide KWIC window");
assert(concordanceSortLabel("left1") === "1L", "Left sort labels should use KWIC shorthand");
assert(concordanceSortLabel("right3") === "3R", "Right sort labels should use KWIC shorthand");
assertDeepEqual(
  concordanceSortSequence("right3", "right2", "right1"),
  ["right3", "right2", "right1"],
  "The three visible controls should compile into their exact explicit order",
);
assertDeepEqual(concordanceSortSequence("filePosition", "right1", "right2"), ["filePosition"]);
assertDeepEqual(concordanceSortSequence("node", "left1", "left2"), ["node"]);
assertDeepEqual(concordanceSortSequence("right1", "right1", "right2"), ["right1", "right2"]);
assertDeepEqual(
  concordanceContextSortLevels(["left1", "left3", "right2"], 5, 5),
  { left: [null, null, 2, null, 1], right: [null, 3, null, null, null] },
  "Only exact selected positions should receive their explicit priority colour",
);
for (const sort of ["filePosition", "node"] as const) {
  assertDeepEqual(concordanceContextSortLevels([sort], 3, 3), {
    left: [null, null, null],
    right: [null, null, null],
  });
}
assert(
  concordanceMiddleEllipsis("a-very-long-document-filename.txt", 16) === "a-very-l…ame.txt",
  "Long file names should retain recognisable start and end text",
);
assert(concordanceCollocationMeasureLabel("g2") === "log-likelihood", "Measure labels should be readable");
assert(concordanceCollocationDirectionLabel("both") === "both sides", "Both-side direction should be readable");
assert(
  concordanceCollocationExplanation("movie", 5, "both", "logDice")
    === 'Collocates near "movie" · 5-token window · both sides · ranked by logDice',
  "Collocation explanation should translate the active query, span, direction, and measure compactly",
);
assert(
  !concordanceCollocationExplanation("movie", 5, "both", "logDice").includes("Near query =")
    && !concordanceCollocationExplanation("movie", 5, "both", "logDice").includes("Corpus frequency =")
    && !concordanceCollocationExplanation("movie", 5, "both", "logDice").includes("Association:"),
  "Collocation explanation should not include persistent definition copy or Association prefix",
);
assert(concordanceRowText(["not", "very", "good"]) === "not very good", "KWIC token text should use token spacing");
for (const node of ["movie", "this movie", "not very good"]) {
  assert(concordanceRowText(node.split(" ")) === node, `${node} node text should render in full`);
}
const unfilteredSegments = concordanceContextTokenSegments(["not", "movie"], null, false);
assert(
  unfilteredSegments.map((segment) => segment.text).join(" ") === "not movie"
    && unfilteredSegments.every((segment) => !segment.collocate),
  "Unfiltered KWIC context should keep plain token text without collocate highlights",
);
const insensitiveSegments = concordanceContextTokenSegments(["Movie", "night"], "movie", false);
assert(
  insensitiveSegments[0].collocate && !insensitiveSegments[1].collocate,
  "Case-insensitive collocate filters should highlight matching context tokens",
);
const sensitiveSegments = concordanceContextTokenSegments(["Movie", "movie"], "movie", true);
assert(
  !sensitiveSegments[0].collocate && sensitiveSegments[1].collocate,
  "Case-sensitive collocate filters should only highlight exact token matches",
);
const unsafeSegments = concordanceContextTokenSegments(["<img src=x onerror=alert(1)>"], "<img src=x onerror=alert(1)>", true);
assert(
  unsafeSegments[0].text === "<img src=x onerror=alert(1)>" && unsafeSegments[0].collocate,
  "Collocate token segmentation should preserve unsafe-looking token text for text-node rendering",
);
assert(
  CONCORDANCE_SORT_OPTIONS.map((option) => option.value).join("|")
    === "filePosition|node|left1|left2|left3|right1|right2|right3",
  "Sort options should include file, node, and 1-3L/1-3R",
);
assert(
  CONCORDANCE_COLLOCATION_MEASURE_OPTIONS.map((option) => option.value).join("|")
    === "logDice|mi|mi3|tScore|zScore|g2|logRatio",
  "Collocation measures should include all requested association scores",
);
assert(
  CONCORDANCE_COLLOCATION_SORT_OPTIONS.map((option) => option.value).join("|")
    === "nearQuery|corpusFrequency|association",
  "Collocation sort options should expose true backend ranking columns",
);
assert(
  concordanceCollocationSortHeaderLabel("association", "logDice") === "Association: logDice",
  "Association sort header should include the active measure",
);
assert(
  concordanceCollocationSortHeaderLabel("nearQuery", "logDice") === "Near query",
  "Near-query sort header should use human-readable wording",
);
assert(
  CONCORDANCE_COLUMNS.join("|") === "File|Left context|Node|Right context",
  "KWIC table should expose the standard columns",
);
const difficultCollocateSurfaces = ["can't", "can-t", "cant", "A", "a", "é", "é", "’", "'", "東京"];
const difficultCollocateIds = difficultCollocateSurfaces.map(concordanceCollocateEvidenceDomId);
assert(
  new Set(difficultCollocateIds).size === difficultCollocateSurfaces.length,
  "Evidence IDs should distinguish punctuation, case, Unicode, and visually similar surfaces",
);
assert(
  concordanceCollocateEvidenceDomId("can't") === concordanceCollocateEvidenceDomId("can't"),
  "Evidence IDs should be deterministic for the same returned surface",
);
assert(
  CONCORDANCE_NO_QUERY_MESSAGE === "Enter a word or phrase to see concordance lines.",
  "No-query copy should match the alpha prompt",
);
assert(
  CONCORDANCE_NO_HITS_MESSAGE === "No concordance lines found for this query in the selected files.",
  "No-hit copy should be specific to selected files",
);

const summary = concordanceSummaryText({
  query: "good movie",
  totalHits: 24,
  documentsWithHits: 12,
  selectedDocumentsTotal: 24,
  topDocumentShare: 0.375,
  dp: 0.57,
});

assert(summary.includes("good movie"), "Summary should include the query");
assert(summary.includes("24 hits"), "Summary should include total hits");
assert(summary.includes("12 / 24 files"), "Summary should include files with hits and selected files");
assert(summary.includes("range 50.0%"), "Summary should lead with range percentage");
assert(summary.includes("top file 37.5%"), "Summary should include top-file share");
assert(summary.includes("DP 0.57"), "Summary should include DP");
assert(!summary.includes("Sparse"), "Broad summaries should omit the sparse caveat");
const summaryMetrics = concordanceSummaryMetricsText({
  totalHits: 24,
  documentsWithHits: 12,
  selectedDocumentsTotal: 24,
  topDocumentShare: 0.375,
  dp: 0.57,
});
assert(!summaryMetrics.includes("good movie"), "Summary metrics should omit the query token");
assert(summaryMetrics.includes("24 hits"), "Summary metrics should include hit count");
assert(concordanceRangePercent({ documentsWithHits: 3, selectedDocumentsTotal: 12 }) === 25, "Range percent should divide hits by selected files");
assert(concordanceRangePercent({ documentsWithHits: 3, selectedDocumentsTotal: 0 }) === 0, "Range percent should guard zero selected files");
assert(concordanceDistributionMagnitudePercent(10, 40) === 25, "Distribution bars should scale against the payload maximum");
assert(concordanceDistributionMagnitudePercent(40, 40) === 100, "The maximum distribution count should fill its bar");
assert(concordanceDistributionMagnitudePercent(80, 40) === 100, "Distribution bars should clamp unexpected values");
assert(concordanceDistributionMagnitudePercent(4, 0) === 0, "Distribution bars should guard an empty maximum");
const linesStats = concordanceLinesCardStatsText({
  windowTokens: 5,
  sortKeys: ["filePosition"],
  shown: 100,
  totalHits: 8412,
  contextFilter: null,
});
assert(
  linesStats === `window 5L–5R · file/position · showing 100 of ${(8412).toLocaleString()}`,
  "Lines stats should compose window, sort, and result counts truthfully",
);
assert(
  concordanceLinesCardStatsText({
    windowTokens: 9,
    sortKeys: ["right3", "right2", "right1"],
    shown: 25,
    totalHits: 60,
    contextFilter: "pair",
  }) === "window 9L–9R · 3R / 2R / 1R · context “pair” · showing 25 of 60",
  "Filtered Lines stats should expose the full sort sequence and local context filter",
);
assert(
  concordanceCollocatesCardStatsText({
    span: 5,
    direction: "both",
    measure: "logDice",
    minimumFrequency: 5,
    shown: 10,
    loadedRows: 214,
    totalCandidates: 214,
  }) === "span 5L–5R · logDice · min freq 5 · showing 10 of 214",
  "Uncapped Collocates stats should use the qualifying total directly",
);
assert(
  concordanceCollocatesCardStatsText({
    span: 2,
    direction: "left",
    measure: "mi",
    minimumFrequency: 5,
    shown: 35,
    loadedRows: 500,
    totalCandidates: 2143,
  }) === `span 2L · MI · min freq 5 · showing 35 of 500 available rows · ${(2143).toLocaleString()} qualifying collocates`,
  "Capped Collocates stats should distinguish loaded rows from all qualifying candidates",
);
assert(
  concordanceDistributionCardStatsText({
    documentsWithHits: 47,
    selectedDocumentsTotal: 529,
    shown: 10,
    totalDocuments: 47,
  }) === "47 of 529 documents with hits · showing 10 of 47",
  "Distribution stats should distinguish selected documents, hit documents, and page rows",
);
assert(
  concordanceIsSparse({ totalHits: 19, documentsWithHits: 12, selectedDocumentsTotal: 12 }),
  "Thin hit counts should be sparse",
);
assert(
  concordanceSummaryText({
    query: "rare",
    totalHits: 2,
    documentsWithHits: 1,
    selectedDocumentsTotal: 500,
    topDocumentShare: 1,
    dp: 0.99,
  }).endsWith(" · Sparse"),
  "Sparse caveat should be appended as plain summary text",
);

const collocates = [
  { surface: "beta", o11: 5, o12: 10, o21: 2, o22: 40, collocateFrequency: 7, mi: 2, mi3: 4, tScore: 1, zScore: 1, g2: 9, logDice: 10, logRatio: 2 },
  { surface: "alpha", o11: 5, o12: 10, o21: 1, o22: 41, collocateFrequency: 6, mi: 3, mi3: 5, tScore: 2, zScore: 2, g2: 8, logDice: 10, logRatio: 3 },
  { surface: "gamma", o11: 4, o12: 11, o21: 1, o22: 41, collocateFrequency: 5, mi: null, mi3: null, tScore: null, zScore: null, g2: null, logDice: null, logRatio: null },
];
assert(concordanceCollocationScore(collocates[0], "mi") === 2, "Score lookup should use selected measure");
assertDeepEqual(concordanceCollocationStrengthBar(5, "logDice", [1, 5, 10]), {
  signed: false,
  direction: "nonNegative",
  magnitudePercent: 50,
});
assertDeepEqual(concordanceCollocationStrengthBar(4, "mi", [-8, 0, 4]), {
  signed: true,
  direction: "positive",
  magnitudePercent: 50,
});
assertDeepEqual(concordanceCollocationStrengthBar(-8, "mi", [-8, 0, 4]), {
  signed: true,
  direction: "negative",
  magnitudePercent: 100,
});
assertDeepEqual(concordanceCollocationStrengthBar(0, "tScore", [-2, 0, 1]), {
  signed: true,
  direction: "zero",
  magnitudePercent: 0,
});
assert(
  concordanceCollocationStrengthBar(4, "mi", [-8, 0, 4])?.magnitudePercent
    === concordanceCollocationStrengthBar(4, "mi", [-8, 0, 4, 2, 1])?.magnitudePercent,
  "Strength scaling should remain stable when only the visible row count changes",
);
assert(nextConcordanceCollocateDisplayCount(10, 100) === 35);
assert(nextConcordanceCollocateDisplayCount(35, 100) === 60);
assert(nextConcordanceCollocateDisplayCount(60, 100) === 85);
assert(nextConcordanceCollocateDisplayCount(85, 90) === 90);

const positionBins = concordanceCollocatePositionGlyph([1, 2, 3, 4, 5], [6, 7, 8, 9, 10], 5, "both");
assertDeepEqual(positionBins.map((bin) => bin.label), ["L5", "L4", "L3", "L2", "L1", "R1", "R2", "R3", "R4", "R5"]);
assert(positionBins[9]!.intensity === 1 && positionBins[0]!.intensity === 0.5);
assertDeepEqual(
  concordanceCollocatePositionGlyph([5, 0, 0, 0, 0], [5, 0, 0, 0, 0], 5, "both")
    .filter((bin) => bin.active && bin.count > 0 && bin.intensity === 1)
    .map((bin) => bin.label),
  ["L1", "R1"],
  "Tied strongest non-zero position bins should share the dominant treatment",
);
assertDeepEqual(
  concordanceCollocatePositionGlyph([1, 2, 3, 4, 5], [6, 7, 8, 9, 10], 2, "left").map((bin) => bin.active),
  [false, false, false, true, true, false, false, false, false, false],
);
assertDeepEqual(
  concordanceCollocatePositionGlyph([1, 2, 3, 4, 5], [6, 7, 8, 9, 10], 2, "right").map((bin) => bin.active),
  [false, false, false, false, false, true, true, false, false, false],
  "Right-only profiles should activate R1 and R2 for a two-token span",
);
assert(
  concordanceCollocatePositionGlyph([0, 0, 0, 0, 0], [0, 0, 0, 0, 0], 5, "both")
    .every((bin) => bin.intensity === 0),
  "All-zero positional profiles should remain finite",
);
assert(
  concordancePositionGlyphAccessibleText("alpha", positionBins).includes("L5 5")
    && concordancePositionGlyphAccessibleText("alpha", positionBins).includes("R5 10"),
  "Exact positional counts should remain available without hover",
);

const row = {
  documentIndex: 2,
  sourcePath: "review.txt",
  displayName: "review.txt",
  tokenIndex: 8,
  nodeTokenLength: 3,
  leftContext: ["it", "was"],
  node: "not very good",
  rightContext: ["at", "all"],
};
const rowKey = concordanceRowExpansionKey(row);
const otherKey = "3:1:1";
assert(rowKey === "2:8:3", "Expansion key should use typed hit-position data");
assert(toggleConcordanceExpansionKey(null, rowKey) === rowKey, "Clicking a collapsed row should expand it");
assert(toggleConcordanceExpansionKey(rowKey, rowKey) === null, "Clicking the expanded row again should collapse it");
assert(toggleConcordanceExpansionKey(rowKey, otherKey) === otherKey, "Opening another row should close the previous one");

const loadingView = concordanceExpansionViewModel(rowKey, {
  key: rowKey,
  pending: true,
  error: null,
  result: null,
});
assert(loadingView.expanded, "Loading expansion should render an expanded row");
assert(loadingView.ariaExpanded === "true", "Loading expansion should expose aria-expanded=true");
assert(loadingView.status === "loading", "Loading state should be modelled");

const errorView = concordanceExpansionViewModel(rowKey, {
  key: rowKey,
  pending: false,
  error: "Expansion failed",
  result: null,
});
assert(errorView.status === "error", "Error state should be modelled");
assert(errorView.message === "Expansion failed", "Error state should show a plain message");

const readyView = concordanceExpansionViewModel(rowKey, {
  key: rowKey,
  pending: false,
  error: null,
  result: {
    documentIndex: 2,
    sourcePath: "review.txt",
    displayName: "review.txt",
    tokenIndex: 8,
    nodeTokenLength: 3,
    windowTokens: 40,
    leftContext: ["the", "film", "was"],
    nodeTokens: ["not", "very", "good"],
    node: "not very good",
    rightContext: ["despite", "the", "cast"],
  },
});
assert(readyView.status === "ready", "Ready expansion should render context");
assert(readyView.leftText === "the film was", "Expanded context should include left tokens");
assert(readyView.nodeText === "not very good", "Expanded context should highlight the full node");
assert(readyView.rightText === "despite the cast", "Expanded context should include right tokens");

const collapsedView = concordanceExpansionViewModel(rowKey, {
  key: null,
  pending: false,
  error: null,
  result: null,
});
assert(!collapsedView.expanded, "Collapsed rows should not render expanded context");
assert(collapsedView.ariaExpanded === "false", "Collapsed rows should expose aria-expanded=false");
