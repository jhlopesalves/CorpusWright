export type LanguageResolution = {
/**
 * Effective ISO 639-1 code used for annotation/stemming.
 */
effectiveLanguage: string,
/**
 * Whether the language was set by manual override.
 */
isOverridden: boolean,
/**
 * Whether detection confidence was below the threshold.
 */
isLowConfidence: boolean,
/**
 * Confidence from detection (None if overridden without detection).
 */
confidence?: number, };
