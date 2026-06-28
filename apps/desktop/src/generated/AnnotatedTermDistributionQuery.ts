import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { AnnotatedTermSelector } from "./AnnotatedTermSelector.js";

export type AnnotatedTermDistributionQuery = { selection: AnalysisDocumentSelection, selector: AnnotatedTermSelector, offset: number, limit: number, };
