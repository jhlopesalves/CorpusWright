import type { AnnotatedFrequencyMode } from "./generated/AnnotatedFrequencyMode.js";
import type { AnnotationCacheState } from "./generated/AnnotationCacheState.js";

export type FrequencyViewMode = "surface" | AnnotatedFrequencyMode;

export interface FrequencyControlVisibility {
  query: boolean;
  matchMode: boolean;
  minimumFrequency: boolean;
  groupBy: boolean;
  caseFold: boolean;
  language: boolean;
  tokenClass: boolean;
  posFilter: boolean;
  hideStopwords: boolean;
}

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

export function frequencyControlVisibility(mode: FrequencyViewMode): FrequencyControlVisibility {
  switch (mode) {
    case "surface":
      return {
        query: true,
        matchMode: true,
        minimumFrequency: true,
        groupBy: true,
        caseFold: true,
        language: true,
        tokenClass: true,
        posFilter: true,
        hideStopwords: true,
      };
    case "lemmaPos":
      return {
        query: true,
        matchMode: true,
        minimumFrequency: true,
        groupBy: false,
        caseFold: true,
        language: false,
        tokenClass: false,
        posFilter: true,
        hideStopwords: true,
      };
    case "pos":
      return {
        query: true,
        matchMode: true,
        minimumFrequency: true,
        groupBy: false,
        caseFold: false,
        language: false,
        tokenClass: false,
        posFilter: false,
        hideStopwords: false,
      };
  }
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
