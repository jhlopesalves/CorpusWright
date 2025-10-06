import type { PreviewWarningKind } from "./PreviewWarningKind.js";

export type PreviewWarning = { source_path: string | null, relative_path: string | null, kind: PreviewWarningKind, message: string, };
