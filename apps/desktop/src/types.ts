import type { DocumentRecord } from "./generated/DocumentRecord.js";
import type { ScanReport } from "./generated/ScanReport.js";

export interface CorpusLoadResult {
  report: ScanReport;
  corpusVersion: number;
}

export interface ProjectCorpusLoadResult {
  summary: ScanReport["summary"];
  corpusVersion: number;
  recordCount: number;
}

export interface VisibleFile {
  corpusIndex: number;
  record: DocumentRecord;
}
