import { invoke } from "@tauri-apps/api/core";

import type { FilePreview } from "./generated/FilePreview.js";
import type { PdfOcrQuality } from "./generated/PdfOcrQuality.js";
import type { PreviewWarning } from "./generated/PreviewWarning.js";
import type { ProjectSource } from "./generated/ProjectSource.js";
import type { ProjectSourceExtractionRoute } from "./generated/ProjectSourceExtractionRoute.js";
import type { ProjectSourceImportReview } from "./generated/ProjectSourceImportReview.js";
import type { ProjectSourceManifest } from "./generated/ProjectSourceManifest.js";
import type { ProjectSourcePdfProfile } from "./generated/ProjectSourcePdfProfile.js";
import type { ProjectSourceStatus } from "./generated/ProjectSourceStatus.js";
import type { ProjectSourceUpdate } from "./generated/ProjectSourceUpdate.js";
import { state } from "./state";

export interface ProjectSourceProcessingResult {
  path: string;
  failed: boolean;
  warning_count: number;
  reason_codes: string[];
}

export interface ProjectSourcePdfHandlingOverride {
  path: string;
  pdfProfile: ProjectSourcePdfProfile;
  ocrQuality: PdfOcrQuality;
}

export const SOURCE_GROUP_ORDER: ProjectSourceStatus[] = [
  "failed",
  "needs_ocr",
  "ready_to_process",
  "unprocessed",
  "text_extracted_with_warnings",
  "text_extracted",
  "processed_with_warnings",
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
    case "ready_to_process": return "Ready to Extract";
    case "needs_ocr": return "Needs OCR";
    case "text_extracted": return "Text Extracted";
    case "text_extracted_with_warnings": return "Text Extracted with Warnings";
    case "processed": return "Processed";
    case "processed_with_warnings": return "Processed with Warnings";
    case "failed": return "Failed";
  }
}

export function sourceHasExtractedText(source: ProjectSource): boolean {
  return source.status === "text_extracted"
    || source.status === "text_extracted_with_warnings"
    || source.status === "processed"
    || source.status === "processed_with_warnings";
}

export function sourceNeedsTextExtraction(source: ProjectSource): boolean {
  return source.kind === "pdf" && !sourceHasExtractedText(source) && source.status !== "failed";
}

export function isSelectableSource(_source: ProjectSource): boolean {
  return true;
}

export function isPreviewableSource(source: ProjectSource): boolean {
  if (source.kind === "unknown" || source.status === "failed") {
    return false;
  }
  return source.kind !== "pdf" || sourceHasExtractedText(source);
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

export function extractionRouteLabel(route: ProjectSourceExtractionRoute): string {
  switch (route) {
    case "embedded_text": return "embedded text";
    case "ocr_if_needed": return "legacy mixed OCR route";
    case "force_ocr": return "full OCR all pages";
    case "ocr_required": return "OCR required";
    case "plain_text": return "plain text";
    case "docx_extract": return "DOCX extraction";
    case "html_extract": return "HTML extraction";
    case "unsupported": return "unsupported";
    case "undecided": return "route undecided";
  }
}

export function pdfProfileLabel(profile: ProjectSourcePdfProfile): string {
  switch (profile) {
    case "standard": return "Standard embedded text";
    case "layout_heavy": return "Layout-aware embedded text (experimental)";
    case "ocr_rescue": return "Full OCR all pages";
    case "force_ocr": return "Full OCR all pages";
    case "no_automatic_ocr": return "Do not OCR automatically / embedded text only";
  }
}

export function pdfProfileShortLabel(profile: ProjectSourcePdfProfile): string {
  switch (profile) {
    case "standard": return "standard embedded text";
    case "layout_heavy": return "layout-aware embedded text";
    case "ocr_rescue": return "full OCR all pages";
    case "force_ocr": return "full OCR all pages";
    case "no_automatic_ocr": return "embedded text only";
  }
}

export function pdfHandlingLabel(source: ProjectSource): string | null {
  if (source.kind !== "pdf") {
    return usefulRouteLabel(source);
  }
  if (source.pdf_profile) {
    return pdfProfileShortLabel(source.pdf_profile);
  }
  return usefulRouteLabel(source);
}

export function usefulRouteLabel(source: ProjectSource): string | null {
  switch (source.extraction_route) {
    case "embedded_text":
    case "ocr_if_needed":
    case "force_ocr":
    case "ocr_required":
    case "undecided":
      return extractionRouteLabel(source.extraction_route);
    default:
      return null;
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
    case "pdf_route_undecided": return "route undecided";
    case "ocr_planned": return "OCR planned";
    case "force_ocr_planned": return "full OCR all pages planned";
    case "text_extracted": return "text extracted";
    case "text_extracted_with_warnings": return "review recommended";
    case "review_recommended": return "review recommended";
    case "text_extraction_failed": return "text extraction failed";
    case "extraction_cancelled": return "Extraction cancelled";
    case "processed_ok": return "processed";
    case "processed_with_warnings": return "review recommended";
    case "low_text_density": return "low text density";
    case "layout_warning": return "layout warning";
    case "many_repeated_artifacts": return "repeated artefacts";
    case "ocr_generated": return "OCR generated";
    case "ocr_review_recommended": return "OCR review recommended";
    case "layout_heavy_profile": return "layout-aware extraction";
    case "render_size_clamped": return "render size clamped";
    case "output_write_failed": return "write failed";
    case "parse_failed": return "parse failed";
    default: return code.replace(/_/g, " ");
  }
}

export function reasonDescription(code: string): string | null {
  switch (code) {
    case "missing_file": return "The source file is missing.";
    case "permission_denied": return "The source file could not be read because permission was denied.";
    case "unsupported_file_type": return "This file type is not supported for corpus sources.";
    case "pdf_encrypted": return "The PDF is encrypted or password protected.";
    case "pdf_open_failed": return "The PDF could not be opened.";
    case "pdf_no_text_layer": return "The PDF has no usable embedded text layer.";
    case "ocr_required": return "OCR is required before source text can be previewed.";
    case "ocr_failed": return "OCR text extraction failed.";
    case "native_text_available": return "Native text is available for extraction.";
    case "pdf_route_undecided": return "PDF diagnostics could not choose a confident extraction route.";
    case "ocr_planned":
    case "force_ocr_planned": return "Full-page OCR is planned for this PDF.";
    case "text_extracted_with_warnings":
    case "review_recommended": return "Text extraction reported warnings; review source text before export.";
    case "text_extraction_failed": return "Text extraction failed.";
    case "extraction_cancelled": return "Extraction was cancelled.";
    case "processed_with_warnings": return "Processing reported warnings; review processed text before export.";
    case "low_text_density": return "Initial PDF diagnostics marked embedded text quality as suspicious.";
    case "layout_warning": return "Layout-aware extraction reported a warning.";
    case "many_repeated_artifacts": return "Repeated extraction artefacts were reported.";
    case "ocr_generated":
    case "ocr_review_recommended": return "OCR-generated text should be reviewed.";
    case "layout_heavy_profile": return "Layout-aware embedded extraction was used.";
    case "render_size_clamped": return "PDF render size was clamped during OCR.";
    case "output_write_failed": return "Processed output could not be written.";
    case "parse_failed": return "The source could not be parsed.";
    default: return null;
  }
}

export function sourceReasonDescriptions(source: ProjectSource): string[] {
  const descriptions: string[] = [];
  const seen = new Set<string>();
  for (const code of source.reason_codes) {
    const description = reasonDescription(code);
    if (description && !seen.has(description)) {
      descriptions.push(description);
      seen.add(description);
    }
  }
  return descriptions;
}

export function primaryReasonLabel(source: ProjectSource): string | null {
  const code = source.reason_codes[0];
  return code ? reasonLabel(code) : null;
}

export function isLoadableSource(source: ProjectSource): boolean {
  return source.kind !== "unknown" && source.status !== "failed";
}

export function selectedProjectSources(): ProjectSource[] {
  return state.projectSources.filter((source) => state.selectedProjectSourceIds.has(source.id));
}

export function selectedProjectSourcePaths(): string[] {
  return selectedProjectSources().map((source) => source.path);
}

export function loadableProjectSourcePaths(): string[] {
  return state.projectSources
    .filter(isLoadableSource)
    .map((source) => source.path);
}

export function replaceProjectSources(manifest: ProjectSourceManifest): void {
  state.sourceManifestVersion = manifest.version;
  state.projectSources = manifest.sources;
  const sourceIds = new Set(manifest.sources.map((source) => source.id));
  for (const selectedId of state.selectedProjectSourceIds) {
    if (!sourceIds.has(selectedId)) {
      state.selectedProjectSourceIds.delete(selectedId);
    }
  }
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

export async function reviewFilesForActiveProjectSources(paths: string[]): Promise<ProjectSourceImportReview | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  return invoke<ProjectSourceImportReview>("review_project_source_files_import_command", {
    rootPath: state.activeProject.rootPath,
    paths,
  });
}

export async function confirmFilesForActiveProjectSources(
  paths: string[],
  pdfHandlingOverrides: ProjectSourcePdfHandlingOverride[],
): Promise<ProjectSourceUpdate | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  const update = await invoke<ProjectSourceUpdate>("confirm_project_source_files_import_command", {
    rootPath: state.activeProject.rootPath,
    paths,
    pdfHandlingOverrides,
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

export async function reviewFolderForActiveProjectSources(path: string): Promise<ProjectSourceImportReview | null> {
  if (!state.activeProject) {
    return null;
  }
  return invoke<ProjectSourceImportReview>("review_project_source_folder_import_command", {
    rootPath: state.activeProject.rootPath,
    folderPath: path,
  });
}

export async function confirmFolderForActiveProjectSources(
  path: string,
  pdfHandlingOverrides: ProjectSourcePdfHandlingOverride[],
): Promise<ProjectSourceUpdate | null> {
  if (!state.activeProject) {
    return null;
  }
  const update = await invoke<ProjectSourceUpdate>("confirm_project_source_folder_import_command", {
    rootPath: state.activeProject.rootPath,
    folderPath: path,
    pdfHandlingOverrides,
  });
  replaceProjectSources(update.manifest);
  return update;
}

export async function removeProjectSourcePaths(paths: string[]): Promise<ProjectSourceManifest | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  const manifest = await invoke<ProjectSourceManifest>("remove_project_source_paths_command", {
    rootPath: state.activeProject.rootPath,
    paths,
  });
  replaceProjectSources(manifest);
  return manifest;
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
