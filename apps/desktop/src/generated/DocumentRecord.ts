import type { DocumentType } from "./DocumentType.js";

export type DocumentRecord = { source_path: string, relative_path: string, document_type: DocumentType, size_bytes: number, };
