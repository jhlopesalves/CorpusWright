import {
  ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE,
  annotatedDispersionLevel,
  annotatedFrequencyModeEnabled,
  formatDocumentFrequency,
  frequencyModeLabel,
} from "../src/frequency.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

assert(annotatedFrequencyModeEnabled("surface", "missing"), "Surface mode should remain enabled");
assert(!annotatedFrequencyModeEnabled("lemmaPos", "stale"), "Lemma + POS should require Ready annotation");
assert(annotatedFrequencyModeEnabled("pos", "ready"), "POS mode should be enabled for Ready annotation");

assert(formatDocumentFrequency(3, 12) === "3 / 12", "Document frequency should render as N / total");

assert(annotatedDispersionLevel("broadly dispersed") === "low", "Broad labels should map to low display level");
assert(
  annotatedDispersionLevel("moderately concentrated") === "medium",
  "Moderate labels should map to medium display level",
);
assert(
  annotatedDispersionLevel("highly concentrated") === "high",
  "Concentrated labels should map to high display level",
);

assert(frequencyModeLabel("surface") === "Surface tokens", "Surface tab label should be stable");
assert(frequencyModeLabel("lemmaPos") === "Lemmas + POS", "Lemma + POS tab label should be stable");
assert(frequencyModeLabel("pos") === "POS tags", "POS tab label should be stable");
assert(
  ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE === "Build annotations to enable lemma and POS frequency.",
  "Unavailable annotation copy should point to building annotations",
);
