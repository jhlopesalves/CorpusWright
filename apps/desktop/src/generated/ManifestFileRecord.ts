import type { DocumentType } from "./DocumentType.js";

export type ManifestFileRecord = { source_path: string, relative_path: string, document_type: DocumentType, output_path: string, source_size_bytes: number, original_char_count: number, processed_char_count: number, source_hash_sha256: string, processed_hash_sha256: string, warnings: Array<string>, extraction_method: string | null, page_count: number | null, };
