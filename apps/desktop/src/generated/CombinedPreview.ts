import type { FilePreview } from "./FilePreview.js";
import type { PreviewWarning } from "./PreviewWarning.js";
import type { CleaningReport } from "./CleaningReport.js";

export type CombinedPreview = { files: Array<FilePreview>, combined_text: string, total_files_previewed: number, total_characters_included: number, warnings: Array<PreviewWarning>, cleaning_report: CleaningReport | null, };
