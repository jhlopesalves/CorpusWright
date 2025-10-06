import type { CleaningConfig } from "./CleaningConfig.js";
import type { ManifestFileRecord } from "./ManifestFileRecord.js";

export type ExportManifest = { app_name: string, app_version: string | null, export_timestamp: string, files_exported: number, warnings_count: number, cleaning_config: CleaningConfig, files: Array<ManifestFileRecord>, };
