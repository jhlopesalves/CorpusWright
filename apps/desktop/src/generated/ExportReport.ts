import type { ManifestFileRecord } from "./ManifestFileRecord.js";
import type { ExportWarning } from "./ExportWarning.js";
import type { ExportManifest } from "./ExportManifest.js";

export type ExportReport = { output_dir: string, texts_dir: string, manifest_path: string, warnings_path: string, config_path: string, readme_path: string, files_exported: number, warnings_count: number, exported_files: Array<ManifestFileRecord>, warnings: Array<ExportWarning>, manifest: ExportManifest, };
