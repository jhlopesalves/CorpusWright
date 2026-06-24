import type { DetectedLanguage } from "./DetectedLanguage.js";
import type { LanguageResolution } from "./LanguageResolution.js";

export type DocumentLanguage = {
/**
 * The raw detected language, absent when an override short-circuits detection.
 */
detected?: DetectedLanguage,
/**
 * Manual override ISO 639-1 code, if set.
 */
overrideLanguage?: string,
/**
 * The resolved language after override logic.
 */
resolution: LanguageResolution, };
