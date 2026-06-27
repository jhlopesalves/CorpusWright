import type { AnnotatedFrequencyMode } from "./AnnotatedFrequencyMode.js";
import type { AnnotationCacheAvailability } from "./AnnotationCacheAvailability.js";
import type { PosFrequencyRow } from "./PosFrequencyRow.js";
import type { LexFrequencyRow } from "./LexFrequencyRow.js";

export type AnnotatedFrequencyResult = { mode: AnnotatedFrequencyMode, cache: AnnotationCacheAvailability, unavailableMessage: string | null, totalTokens: number, selectedDocumentCount: number, totalDocumentCount: number, totalRows: number, offset: number, limit: number, posRows: Array<PosFrequencyRow>, lexRows: Array<LexFrequencyRow>, };
