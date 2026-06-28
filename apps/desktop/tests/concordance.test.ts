import {
  CONCORDANCE_COLUMNS,
  CONCORDANCE_NO_HITS_MESSAGE,
  CONCORDANCE_NO_QUERY_MESSAGE,
  CONCORDANCE_SORT_OPTIONS,
  DEFAULT_CONCORDANCE_SORT_KEY,
  clampConcordancePageSize,
  clampConcordanceWindow,
  concordanceSortLabel,
  concordanceSummaryText,
  normaliseConcordanceQuery,
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
assert(concordanceSortLabel("left1") === "1L", "Left sort labels should use KWIC shorthand");
assert(concordanceSortLabel("right3") === "3R", "Right sort labels should use KWIC shorthand");
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
