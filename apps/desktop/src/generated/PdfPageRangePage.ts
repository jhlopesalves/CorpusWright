import type { PdfPageExtractionMethod } from "./PdfPageExtractionMethod.js";

export type PdfPageRangePage = { page_index: number, page_number: number, text: string, char_count: number, method: PdfPageExtractionMethod, warnings: Array<string>, render_clamped: boolean, error: string | null, };
