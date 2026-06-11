import type { ExportWarningKind } from "./ExportWarningKind.js";

export type ExportWarning = { source_path: string | null, output_path: string | null, kind: ExportWarningKind, message: string, };
