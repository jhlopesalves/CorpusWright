import {
  ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE,
  DEFAULT_FREQUENCY_RATE_SCALE,
  annotatedDispersionLevel,
  annotatedFrequencyModeEnabled,
  frequencyColumnLabels,
  frequencyControlVisibility,
  frequencyDistributionBarValue,
  frequencyDistributionColumnLabels,
  frequencyRateValue,
  formatDocumentFrequency,
  frequencyModeLabel,
} from "../src/features/analyse/frequency/model.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertSameArray(actual: string[], expected: string[], message: string): void {
  assert(
    actual.length === expected.length && actual.every((value, index) => value === expected[index]),
    `${message}: expected ${expected.join(" | ")}, got ${actual.join(" | ")}`,
  );
}

function assertNoDefaultOptionalColumns(labels: string[], context: string): void {
  for (const optional of ["Per 10k", "Per 100k", "Per million", "Top-doc share"]) {
    assert(!labels.includes(optional), `${context} should not render ${optional} by default`);
  }
}

assert(annotatedFrequencyModeEnabled("surface", "missing"), "Surface mode should remain enabled");
assert(!annotatedFrequencyModeEnabled("lemmaPos", "stale"), "Lemma + POS should require Ready annotation");
assert(annotatedFrequencyModeEnabled("pos", "ready"), "POS mode should be enabled for Ready annotation");

assert(formatDocumentFrequency(3, 12) === "3 / 12", "Document frequency should render as N / total");

assert(annotatedDispersionLevel("DP < 0.30 (fixed reference)") === "low", "Broad labels should map to low display level");
assert(
  annotatedDispersionLevel("0.30 ≤ DP < 0.70 (fixed references)") === "medium",
  "Moderate labels should map to medium display level",
);
assert(
  annotatedDispersionLevel("DP ≥ 0.70 (fixed reference)") === "high",
  "Concentrated labels should map to high display level",
);

assert(frequencyModeLabel("surface") === "Surface tokens", "Surface tab label should be stable");
assert(frequencyModeLabel("lemmaPos") === "Lemmas + POS", "Lemma + POS tab label should be stable");
assert(frequencyModeLabel("pos") === "POS tags", "POS tab label should be stable");

assertSameArray(
  frequencyColumnLabels("surface"),
  ["Token", "Frequency", "Range (docs)", "Dispersion (DP)"],
  "Surface defaults should render the core analytical columns",
);
assertSameArray(
  frequencyColumnLabels("lemmaPos"),
  ["Lemma", "UPOS", "Frequency", "Range (docs)", "Dispersion (DP)"],
  "Lemma + POS defaults should render the core analytical columns",
);
assertSameArray(
  frequencyColumnLabels("pos"),
  ["UPOS", "Frequency", "Range (docs)", "Dispersion (DP)"],
  "POS defaults should render the core analytical columns",
);
assertNoDefaultOptionalColumns(frequencyColumnLabels("surface"), "Surface defaults");
assertNoDefaultOptionalColumns(frequencyColumnLabels("lemmaPos"), "Lemma + POS defaults");
assertNoDefaultOptionalColumns(frequencyColumnLabels("pos"), "POS defaults");

assert(DEFAULT_FREQUENCY_RATE_SCALE === "off", "Rate control should default to Off");
for (const mode of ["surface", "lemmaPos", "pos"] as const) {
  assert(
    frequencyColumnLabels(mode, "per10k").filter((label) => label === "Per 10k").length === 1,
    `${frequencyModeLabel(mode)} should render exactly one Per 10k column`,
  );
  assert(
    frequencyColumnLabels(mode, "per100k").filter((label) => label === "Per 100k").length === 1,
    `${frequencyModeLabel(mode)} should render exactly one Per 100k column`,
  );
  assert(
    frequencyColumnLabels(mode, "perMillion").filter((label) => label === "Per million").length === 1,
    `${frequencyModeLabel(mode)} should render exactly one Per million column`,
  );
}
assert(frequencyRateValue("per10k", { per10k: 12.5, perMillion: 1250 }) === 12.5, "per 10k should use the per-10k value");
assert(frequencyRateValue("per100k", { per10k: 12.5, perMillion: 1250 }) === 125, "per 100k should derive from per 10k");
assert(frequencyRateValue("perMillion", { per10k: 12.5, perMillion: 1250 }) === 1250, "per million should use the per-million value when present");

assert(frequencyControlVisibility("surface").tokenClass, "Surface should show token-class filtering");
assert(!frequencyControlVisibility("surface").posFilter, "Surface should hide POS filtering");
assert(!frequencyControlVisibility("lemmaPos").tokenClass, "Lemma + POS should hide surface token class");
assert(!frequencyControlVisibility("lemmaPos").groupBy, "Lemma + POS should hide surface grouping");
assert(frequencyControlVisibility("lemmaPos").posFilter, "Lemma + POS should show POS filtering");
assert(!frequencyControlVisibility("pos").caseFold, "POS tags should hide case folding");
assert(!frequencyControlVisibility("pos").hideStopwords, "POS tags should hide stopword filtering");
assert(!frequencyControlVisibility("pos").tokenClass, "POS tags should hide token-class filtering");
assert(!frequencyControlVisibility("pos").groupBy, "POS tags should hide surface grouping");
assert(!frequencyControlVisibility("pos").posFilter, "POS tags should hide POS filtering");

assertSameArray(
  frequencyDistributionColumnLabels(),
  ["Document/File", "Count", "Document tokens", "Bar"],
  "Default drill-down should render document tokens without a rate column",
);
assertSameArray(
  frequencyDistributionColumnLabels("per100k"),
  ["Document/File", "Count", "Document tokens", "Per 100k", "Bar"],
  "Rate-enabled drill-down should render the selected rate column",
);
assert(
  frequencyDistributionBarValue("off", { count: 7, per10k: 3.5, perMillion: 350 }) === 7,
  "Default drill-down bar should use raw count",
);
assert(
  frequencyDistributionBarValue("perMillion", { count: 7, per10k: 3.5, perMillion: 350 }) === 350,
  "Rate-enabled drill-down bar should use the selected rate",
);
assert(
  ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE === "Build annotations to enable lemma and POS frequency.",
  "Unavailable annotation copy should point to building annotations",
);
