import type { DocumentRecord } from "./DocumentRecord.js";
import type { CorpusSummary } from "./CorpusSummary.js";

export type ScanReport = { root: string, files: Array<DocumentRecord>, files_discovered: number, files_supported: number, files_ignored: number, total_size_bytes: number, summary: CorpusSummary, };
