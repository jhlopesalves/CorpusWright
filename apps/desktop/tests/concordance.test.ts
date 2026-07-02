import {
  CONCORDANCE_COLUMNS,
  CONCORDANCE_COLLOCATION_DEFAULT_FLOOR,
  CONCORDANCE_COLLOCATION_DEFAULT_SPAN,
  CONCORDANCE_COLLOCATION_LIMIT,
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
  DEFAULT_CONCORDANCE_SORT_KEY,
  clampConcordanceCollocationFloor,
  clampConcordanceCollocationSpan,
  clampConcordancePageSize,
  clampConcordanceWindow,
  concordanceCollocationDirectionLabel,
  concordanceCollocationExplanation,
  concordanceCollocationMeasureLabel,
  concordanceCollocationScore,
  concordanceCollocationScoreBarPercent,
  concordanceCollocationSortHeaderLabel,
  concordanceContextTokenSegments,
  concordanceExpansionViewModel,
  concordanceIsSparse,
  concordanceRangePercent,
  concordanceRowExpansionKey,
  concordanceRowText,
  concordanceSortLabel,
  concordanceSummaryMetricsText,
  concordanceSummaryText,
  normaliseConcordanceQuery,
  toggleConcordanceExpansionKey,
} from "../src/concordance.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

assert(normaliseConcordanceQuery("  alpha ") === "alpha", "Concordance query should be trimmed");
assert(clampConcordanceWindow(Number.NaN) === 5, "Invalid windows should fall back to the default");
assert(clampConcordanceWindow(0) === 5, "Zero windows should fall back to the default");
assert(clampConcordanceWindow(24) === 20, "Windows should be capped at the hard maximum");
assert(clampConcordanceWindow(3.8) === 3, "Windows should use whole tokens");
assert(clampConcordancePageSize(0) === 100, "Invalid page sizes should fall back to the default");
assert(clampConcordancePageSize(12) === 25, "Page sizes should respect the minimum");
assert(clampConcordancePageSize(900) === 500, "Page sizes should respect the maximum");
assert(clampConcordanceCollocationSpan(Number.NaN) === 5, "Invalid collocation spans should fall back");
assert(clampConcordanceCollocationSpan(24) === 20, "Collocation spans should be capped");
assert(clampConcordanceCollocationFloor(-1) === 5, "Invalid collocation floors should fall back");
assert(clampConcordanceCollocationFloor(3.8) === 3, "Collocation floors should use whole counts");
assert(DEFAULT_CONCORDANCE_SORT_KEY === "filePosition", "Default sort should be file/position");
assert(DEFAULT_CONCORDANCE_COLLOCATION_MEASURE === "logDice", "Default collocation measure should be logDice");
assert(DEFAULT_CONCORDANCE_COLLOCATION_SORT_KEY === "association", "Default collocation sort should be association");
assert(DEFAULT_CONCORDANCE_COLLOCATION_SORT_DIRECTION === "desc", "Default collocation sort should descend");
assert(CONCORDANCE_COLLOCATION_DEFAULT_SPAN === 5, "Default collocation span should be +/-5");
assert(CONCORDANCE_COLLOCATION_DEFAULT_FLOOR === 5, "Default collocation floors should be 5");
assert(CONCORDANCE_COLLOCATION_LIMIT === 100, "Collocation results should request the default top 100");
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
assert(concordanceCollocationMeasureLabel("g2") === "log-likelihood", "Measure labels should be readable");
assert(concordanceCollocationDirectionLabel("both") === "both sides", "Both-side direction should be readable");
assert(
  concordanceCollocationExplanation("movie", 5, "both", "logDice")
    === 'Collocates near "movie" within 5 tokens (both sides), ranked by Association: logDice. Near query = times the word appears around the query · Corpus frequency = total frequency in the selected corpus.',
  "Collocation explanation should translate the active query, span, direction, measure, and raw counts",
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
assert(concordanceCollocationScoreBarPercent(-1, "logDice") === 0, "logDice bars should clamp below zero");
assert(concordanceCollocationScoreBarPercent(7, "logDice") === 50, "logDice bars should use a fixed 0-14 scale");
assert(concordanceCollocationScoreBarPercent(20, "logDice") === 100, "logDice bars should clamp above 14");
assert(concordanceCollocationScoreBarPercent(null, "logDice") === null, "Null association scores should not render bars");
assert(
  concordanceCollocationScoreBarPercent(4, "mi", [2, 4, 6]) === 50,
  "Unbounded association bars should be relative to the visible scores",
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
