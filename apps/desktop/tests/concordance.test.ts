import {
  CONCORDANCE_COLUMNS,
  CONCORDANCE_EXPANSION_WINDOW,
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  CONCORDANCE_SORT_OPTIONS,
  DEFAULT_CONCORDANCE_SORT_KEY,
  clampConcordancePageSize,
  clampConcordanceWindow,
  concordanceExpansionViewModel,
  concordanceRowExpansionKey,
  concordanceRowText,
  concordanceSortLabel,
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
assert(DEFAULT_CONCORDANCE_SORT_KEY === "filePosition", "Default sort should be file/position");
assert(CONCORDANCE_EXPANSION_WINDOW === 40, "Expansion should request the default wide KWIC window");
assert(concordanceSortLabel("left1") === "1L", "Left sort labels should use KWIC shorthand");
assert(concordanceSortLabel("right3") === "3R", "Right sort labels should use KWIC shorthand");
assert(concordanceRowText(["not", "very", "good"]) === "not very good", "KWIC token text should use token spacing");
for (const node of ["movie", "this movie", "not very good"]) {
  assert(concordanceRowText(node.split(" ")) === node, `${node} node text should render in full`);
}
assert(
  CONCORDANCE_SORT_OPTIONS.map((option) => option.value).join("|")
    === "filePosition|node|left1|left2|left3|right1|right2|right3",
  "Sort options should include file, node, and 1-3L/1-3R",
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
  totalHits: 2,
  documentsWithHits: 1,
  dp: 0.57,
});

assert(summary.includes("good movie"), "Summary should include the query");
assert(summary.includes("2 hits"), "Summary should include total hits");
assert(summary.includes("1 files with hits"), "Summary should include files with hits");
assert(summary.includes("DP 0.57"), "Summary should include DP");

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
