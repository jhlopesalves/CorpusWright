import type { AnalysisDocumentSelection } from "./AnalysisDocumentSelection.js";
import type { ConcordanceCollocationDirection } from "./ConcordanceCollocationDirection.js";
import type { ConcordanceCollocationMeasure } from "./ConcordanceCollocationMeasure.js";

export type ConcordanceCollocationQuery = { selection: AnalysisDocumentSelection, query: string, caseSensitive: boolean, span: number, direction: ConcordanceCollocationDirection, minCooccurrence: number, minCollocateFrequency: number, hideStopwords: boolean, sortMeasure: ConcordanceCollocationMeasure, limit: number, };
