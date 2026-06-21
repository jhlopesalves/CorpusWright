import type { UnicodeNormalizationForm } from "./UnicodeNormalizationForm.js";

export type TokenizationConfig = { normalization: UnicodeNormalizationForm, retainAsciiHyphenCompounds: boolean, retainApostropheContractions: boolean, splitNumericSeparators: boolean, includePunctuation: boolean, foldAggregationKeys: boolean, };
