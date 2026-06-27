import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { AnnotatedFrequencyMode } from "./AnnotatedFrequencyMode.js";

export type AnnotatedFrequencyQuery = { selection: AnalysisDocumentSelection, mode: AnnotatedFrequencyMode, offset: number, limit: number, };
