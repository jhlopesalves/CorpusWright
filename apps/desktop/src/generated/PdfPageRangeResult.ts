import type { PdfPageRangePage } from "./PdfPageRangePage.js";

export type PdfPageRangeResult = { page_count: number, start_page_index: number, end_page_index: number, pages: Array<PdfPageRangePage>, warnings: Array<string>, };
