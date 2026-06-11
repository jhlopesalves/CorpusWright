import type { DocumentType } from "./DocumentType.js";
import type { PreviewWarning } from "./PreviewWarning.js";

export type FilePreview = { source_path: string, relative_path: string, document_type: DocumentType, text: string, source_size_bytes: number, included_char_count: number, truncated: boolean, warnings: Array<PreviewWarning>, };
