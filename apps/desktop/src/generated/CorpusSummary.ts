import type { DocumentTypeCounts } from "./DocumentTypeCounts.js";

export type CorpusSummary = { root: string, files_discovered: number, files_supported: number, files_ignored: number, total_size_bytes: number, document_type_counts: DocumentTypeCounts, };
