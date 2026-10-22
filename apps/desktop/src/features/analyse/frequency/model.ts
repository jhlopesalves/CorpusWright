import type { AnnotatedFrequencyMode } from "../../../generated/AnnotatedFrequencyMode.js";
import type { AnnotationCacheState } from "../../../generated/AnnotationCacheState.js";

export type FrequencyViewMode = "surface" | AnnotatedFrequencyMode;
export type FrequencyRateScale = "off" | "per10k" | "per100k" | "perMillion";

export interface FrequencyControlVisibility {
  query: boolean;
  matchMode: boolean;
  minimumFrequency: boolean;
  groupBy: boolean;
  caseFold: boolean;
  tokenClass: boolean;
  posFilter: boolean;
  hideStopwords: boolean;
  rate: boolean;
}

export const ANNOTATED_FREQUENCY_UNAVAILABLE_MESSAGE =
  "Build annotations to enable lemma and POS frequency.";
export const DEFAULT_FREQUENCY_RATE_SCALE: FrequencyRateScale = "off";
export const FREQUENCY_RATE_HELP_TEXT =
  "Normalised rates are optional and can be unstable in very short files; inspect raw counts and document tokens.";

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
        tokenClass: true,
        posFilter: false,
        hideStopwords: true,
        rate: true,
      };
    case "lemmaPos":
      return {
        query: true,
        matchMode: true,
        minimumFrequency: true,
        groupBy: false,
        caseFold: true,
        tokenClass: false,
        posFilter: true,
        hideStopwords: true,
        rate: true,
      };
    case "pos":
      return {
        query: true,
        matchMode: true,
        minimumFrequency: true,
        groupBy: false,
        caseFold: false,
        tokenClass: false,
        posFilter: false,
        hideStopwords: false,
        rate: true,
      };
  }
}

export function frequencyRateLabel(scale: FrequencyRateScale): string | null {
  switch (scale) {
    case "off":
      return null;
    case "per10k":
      return "Per 10k";
    case "per100k":
      return "Per 100k";
    case "perMillion":
      return "Per million";
  }
}

export function frequencyColumnLabels(
  mode: FrequencyViewMode,
  rateScale: FrequencyRateScale = DEFAULT_FREQUENCY_RATE_SCALE,
): string[] {
  const rateLabel = frequencyRateLabel(rateScale);
  switch (mode) {
    case "surface":
      return [
        "Token",
        "Frequency",
        "Range (docs)",
        ...(rateLabel ? [rateLabel] : []),
        "Dispersion (DP)",
      ];
    case "lemmaPos":
      return [
        "Lemma",
        "UPOS",
        "Frequency",
        "Range (docs)",
        ...(rateLabel ? [rateLabel] : []),
        "Dispersion (DP)",
      ];
    case "pos":
      return [
        "UPOS",
        "Frequency",
        "Range (docs)",
        ...(rateLabel ? [rateLabel] : []),
        "Dispersion (DP)",
      ];
  }
}

export function frequencyTableColumnCountForMode(
  mode: FrequencyViewMode,
  rateScale: FrequencyRateScale = DEFAULT_FREQUENCY_RATE_SCALE,
): number {
  return frequencyColumnLabels(mode, rateScale).length;
}

export interface FrequencyRateValues {
  per10k: number;
  perMillion?: number;
}

export function frequencyRateValue(
  scale: FrequencyRateScale,
  values: FrequencyRateValues,
): number | null {
  switch (scale) {
    case "off":
      return null;
    case "per10k":
      return values.per10k;
    case "per100k":
      return values.per10k * 10;
    case "perMillion":
      return values.perMillion ?? values.per10k * 100;
  }
}

export function frequencyDistributionColumnLabels(
  rateScale: FrequencyRateScale = DEFAULT_FREQUENCY_RATE_SCALE,
): string[] {
  const rateLabel = frequencyRateLabel(rateScale);
  return [
    "Document/File",
    "Count",
    "Document tokens",
    ...(rateLabel ? [rateLabel] : []),
    "Bar",
  ];
}

export interface FrequencyDistributionBarValues extends FrequencyRateValues {
  count: number;
}

export function frequencyDistributionBarValue(
  scale: FrequencyRateScale,
  values: FrequencyDistributionBarValues,
): number {
  return scale === "off" ? values.count : frequencyRateValue(scale, values) ?? 0;
}

export function formatDocumentFrequency(docFreq: number, docTotal: number): string {
  return `${docFreq.toLocaleString()} / ${docTotal.toLocaleString()}`;
}

export function annotatedDispersionLevel(
  label: string,
): "low" | "medium" | "high" {
  switch (label) {
    case "DP < 0.30 (fixed reference)":
      return "low";
    case "0.30 ≤ DP < 0.70 (fixed references)":
      return "medium";
    default:
      return "high";
  }
}
