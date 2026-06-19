import type { ProjectSource } from "./ProjectSource.js";
import type { FilePreview } from "./FilePreview.js";

export type ProjectSourcePreviewItem = { source: ProjectSource, preview: FilePreview | null, preview_error: string | null, };
