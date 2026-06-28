import type { AnnotatedFrequencyMode } from "./generated/AnnotatedFrequencyMode.js";
import type { AnnotationCacheState } from "./generated/AnnotationCacheState.js";

export type FrequencyViewMode = "surface" | AnnotatedFrequencyMode;

export const ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE =
  "Build annotations to enable lemma and POS frequency.";

export function frequencyModeLabel(mode: FrequencyViewMode): string {
  switch (mode) {
    case "surface":
      return "Surface tokens";
    case "lemmaPos":
      return "Lemmas + POS";
    case "pos":
      return "POS tags";
  }
}

export function annotatedFrequencyModeEnabled(
  mode: FrequencyViewMode,
  cacheState: AnnotationCacheState,
): boolean {
  return mode === "surface" || cacheState === "ready";
}

export function formatDocumentFrequency(docFreq: number, docTotal: number): string {
  return `${docFreq.toLocaleString()} / ${docTotal.toLocaleString()}`;
}

export function annotatedDispersionLevel(
  label: string,
): "low" | "medium" | "high" {
  switch (label) {
    case "broadly dispersed":
      return "low";
    case "moderately concentrated":
      return "medium";
    default:
      return "high";
  }
}
