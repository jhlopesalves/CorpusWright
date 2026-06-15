import type { DocumentType } from "./DocumentType.js";
import type { ProjectSourceExtractionRoute } from "./ProjectSourceExtractionRoute.js";
import type { PdfOcrQuality } from "./PdfOcrQuality.js";

export type DocumentRecord = { source_path: string, relative_path: string, document_type: DocumentType, size_bytes: number, extraction_route: ProjectSourceExtractionRoute | null, ocr_quality: PdfOcrQuality | null, };
