import { invoke } from "@tauri-apps/api/core";

import type { FilePreview } from "./generated/FilePreview.js";
import type { PreviewWarning } from "./generated/PreviewWarning.js";
import type { ProjectSource } from "./generated/ProjectSource.js";
import type { ProjectSourceManifest } from "./generated/ProjectSourceManifest.js";
import type { ProjectSourceStatus } from "./generated/ProjectSourceStatus.js";
import type { ProjectSourceUpdate } from "./generated/ProjectSourceUpdate.js";
import { state } from "./state";

interface ProjectSourceProcessingResult {
  path: string;
  failed: boolean;
  warning_count: number;
  reason_codes: string[];
}

export const SOURCE_GROUP_ORDER: ProjectSourceStatus[] = [
  "failed",
  "needs_ocr",
  "processed_with_warnings",
  "ready_to_process",
  "unprocessed",
  "processed",
];

export function pathKey(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

export function sourceForPath(path: string): ProjectSource | null {
  const key = pathKey(path);
  return state.projectSources.find((source) => pathKey(source.path) === key) ?? null;
}

export function sourceStatusLabel(status: ProjectSourceStatus): string {
  switch (status) {
    case "unprocessed": return "Unprocessed";
    case "ready_to_process": return "Ready to Process";
    case "needs_ocr": return "Needs OCR";
    case "processed": return "Processed";
    case "processed_with_warnings": return "Processed with Warnings";
    case "failed": return "Failed";
  }
}

export function sourceKindLabel(source: ProjectSource): string {
  switch (source.kind) {
    case "pdf": return "PDF";
    case "docx": return "DOCX";
    case "txt": return "TXT";
    case "html": return "HTML";
    case "unknown": return (source.extension ?? "file").toUpperCase();
  }
}

export function reasonLabel(code: string): string {
  switch (code) {
    case "missing_file": return "missing file";
    case "permission_denied": return "permission denied";
    case "unsupported_file_type": return "unsupported";
    case "pdf_encrypted": return "encrypted";
    case "pdf_open_failed": return "PDF open failed";
    case "pdf_no_text_layer": return "no text layer";
    case "ocr_required": return "OCR required";
    case "ocr_failed": return "OCR failed";
    case "native_text_available": return "native text";
    case "processed_ok": return "processed";
    case "processed_with_warnings": return "review";
    case "low_text_density": return "low text density";
    case "layout_warning": return "layout warning";
    case "many_repeated_artifacts": return "repeated artefacts";
    case "output_write_failed": return "write failed";
    case "parse_failed": return "parse failed";
    default: return code.replace(/_/g, " ");
  }
}

export function primaryReasonLabel(source: ProjectSource): string | null {
  const code = source.reason_codes[0];
  return code ? reasonLabel(code) : null;
}

export function isLoadableSource(source: ProjectSource): boolean {
  return source.kind !== "unknown" && source.status !== "failed";
}

export function loadableProjectSourcePaths(): string[] {
  return state.projectSources
    .filter(isLoadableSource)
    .map((source) => source.path);
}

export function replaceProjectSources(manifest: ProjectSourceManifest): void {
  state.sourceManifestVersion = manifest.version;
  state.projectSources = manifest.sources;
}

export async function loadProjectSourcesForActiveProject(): Promise<ProjectSourceManifest> {
  if (!state.activeProject) {
    const empty = { version: 1, sources: [] };
    replaceProjectSources(empty);
    return empty;
  }
  const manifest = await invoke<ProjectSourceManifest>("load_project_sources_command", {
    rootPath: state.activeProject.rootPath,
  });
  replaceProjectSources(manifest);
  return manifest;
}

export async function addFilesToActiveProjectSources(paths: string[]): Promise<ProjectSourceUpdate | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  const update = await invoke<ProjectSourceUpdate>("add_project_source_files_command", {
    rootPath: state.activeProject.rootPath,
    paths,
  });
  replaceProjectSources(update.manifest);
  return update;
}

export async function addFolderToActiveProjectSources(path: string): Promise<ProjectSourceUpdate | null> {
  if (!state.activeProject) {
    return null;
  }
  const update = await invoke<ProjectSourceUpdate>("add_project_source_folder_command", {
    rootPath: state.activeProject.rootPath,
    folderPath: path,
  });
  replaceProjectSources(update.manifest);
  return update;
}

export async function persistProcessedPreviewStatuses(files: FilePreview[]): Promise<void> {
  if (!state.activeProject || files.length === 0) {
    return;
  }

  const results = files
    .filter((file) => sourceForPath(file.source_path) !== null)
    .map((file) => processingResultForPreview(file));

  if (results.length === 0) {
    return;
  }

  try {
    const manifest = await invoke<ProjectSourceManifest>("update_project_source_processing_results_command", {
      rootPath: state.activeProject.rootPath,
      results,
    });
    replaceProjectSources(manifest);
  } catch (error) {
    console.error("Could not update project source statuses", error);
  }
}

function processingResultForPreview(file: FilePreview): ProjectSourceProcessingResult {
  const extractionWarnings = file.warnings.filter((warning) =>
    warning.kind === "extraction_warning" || warning.kind === "extraction_error"
  );
  const failed = extractionWarnings.some((warning) => warning.kind === "extraction_error");
  return {
    path: file.source_path,
    failed,
    warning_count: extractionWarnings.length,
    reason_codes: reasonCodesForWarnings(extractionWarnings, failed),
  };
}

function reasonCodesForWarnings(warnings: PreviewWarning[], failed: boolean): string[] {
  if (warnings.length === 0) {
    return [];
  }

  const codes = new Set<string>();
  for (const warning of warnings) {
    const message = warning.message.toLowerCase();
    if (message.includes("ocr")) {
      codes.add(failed ? "ocr_failed" : "ocr_required");
    }
    if (message.includes("low-quality") || message.includes("low quality")) {
      codes.add("low_text_density");
    }
    if (message.includes("layout")) {
      codes.add("layout_warning");
    }
    if (message.includes("repeated")) {
      codes.add("many_repeated_artifacts");
    }
  }

  if (codes.size === 0) {
    codes.add(failed ? "parse_failed" : "processed_with_warnings");
  }

  return Array.from(codes);
}
