import {
  clampConcordanceWindow,
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

const summary = concordanceSummaryText({
  totalHits: 2,
  documentsWithHits: 1,
  windowTokens: 5,
});

assert(summary.includes("2 hits in 1 files"), "Summary should include total hits and files");
assert(summary.includes("window 5"), "Summary should include the window size");
