import type { CompareDocumentCell } from "./CompareDocumentCell.js";

export type CompareDocumentRow = { documentId: number, sourcePath: string, displayName: string, tokenCount: number, includedInRate: boolean, cells: Array<CompareDocumentCell>, };
