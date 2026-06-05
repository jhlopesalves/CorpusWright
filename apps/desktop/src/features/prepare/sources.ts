/**
 * Provides ID-based project-source selection, paging, status, and import helpers.
 *
 * Source text preserves imported evidence. Processed text belongs to the active deterministic
 * cleaning configuration, and callers can inspect both through the shared source identity.
 */
import { invoke } from "./ipc";

import type { PdfOcrQuality } from "../../generated/PdfOcrQuality.js";
import type { ProjectSource } from "../../generated/ProjectSource.js";
import type { ProjectSourceExtractionRoute } from "../../generated/ProjectSourceExtractionRoute.js";
import type { ProjectSourceImportReview } from "../../generated/ProjectSourceImportReview.js";
import type { ProjectSourcePage } from "../../generated/ProjectSourcePage.js";
import type { ProjectSourcePdfProfile } from "../../generated/ProjectSourcePdfProfile.js";
import type { ProjectSourceStatus } from "../../generated/ProjectSourceStatus.js";
import type { ProjectSourceSelectionDescriptor } from "../../generated/ProjectSourceSelectionDescriptor.js";
import { state } from "../../app/state";

const PROJECT_SOURCES_UPDATED_EVENT = "corpuswright:project-sources-updated";

export interface ProjectSourcePdfHandlingOverride {
  path: string;
  pdfProfile: ProjectSourcePdfProfile;
  ocrQuality: PdfOcrQuality;
}

export interface ProjectSourceFastImportResult {
  imported: boolean;
  requiresPdfHandling: boolean;
  addedCount: number;
  duplicateCount: number;
  ignoredCount: number;
}

export interface ProjectSourceImportResult {
  addedCount: number;
  duplicateCount: number;
}

export interface ProjectSourceRemovalResult {
  removedCount: number;
}

export interface ProjectSourceSelectionActionSummary {
  totalSelected: number;
  allSelectedArePdfs: boolean;
  usablePdfCount: number;
  extractablePdfCount: number;
  reextractablePdfCount: number;
  failedPdfCount: number;
}

export function pathKey(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

export function sourceForPath(path: string): ProjectSource | null {
  const key = pathKey(path);
  return state.projectSources.find((source) => pathKey(source.path) === key)
    ?? Array.from(state.selectedProjectSourcesById.values())
      .find((source) => pathKey(source.path) === key)
    ?? null;
}

export function sourceStatusLabel(status: ProjectSourceStatus): string {
  switch (status) {
    case "unprocessed": return "Unprocessed";
    case "ready_to_process": return "Ready to Process";
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
    case "text_extracted_with_warnings": return "text extracted with warnings";
    case "review_recommended": return "text extracted with warnings";
    case "text_extraction_failed": return "text extraction failed";
    case "extraction_cancelled": return "Extraction cancelled";
    case "processing_warning": return "processing warning";
    case "processed_ok": return "processed";
    case "processed_with_warnings": return "processed with warnings";
    case "low_text_density": return "low text density";
    case "layout_warning": return "layout warning";
    case "many_repeated_artifacts": return "repeated artefacts";
    case "ocr_generated": return "OCR generated";
    case "ocr_review_recommended": return "OCR generated";
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
    case "native_text_available": return "Native text is available for processing.";
    case "pdf_route_undecided": return "PDF diagnostics could not choose a confident extraction route.";
    case "ocr_planned":
    case "force_ocr_planned": return "Full-page OCR is planned for this PDF.";
    case "text_extraction_failed": return "Text extraction failed.";
    case "extraction_cancelled": return "Extraction was cancelled.";
    case "processing_warning": return "Processed text was generated with warnings.";
    case "low_text_density": return "Initial PDF diagnostics marked embedded text quality as suspicious.";
    case "layout_warning": return "Layout-aware extraction was used; inspect line order before export.";
    case "many_repeated_artifacts": return "Repeated extraction artefacts were reported.";
    case "ocr_generated":
    case "ocr_review_recommended": return "OCR-generated text should be reviewed before export.";
    case "layout_heavy_profile": return "Layout-aware extraction was used; inspect line order before export.";
    case "render_size_clamped": return "OCR render size was clamped; some pages may be incomplete.";
    case "text_extracted":
    case "processed_ok": return null;
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

export function selectedProjectSources(): ProjectSource[] {
  return Array.from(state.selectedProjectSourcesById.values());
}

export function sourceDisplayStatusLabel(source: ProjectSource): string {
  if (source.status !== "ready_to_process") {
    return sourceStatusLabel(source.status);
  }
  return source.kind === "pdf" ? "Ready to Extract" : "Text Available";
}

export function projectSourceSelectionCount(): number {
  return state.allMatchingProjectSourcesSelected
    ? Math.max(
      0,
      (state.projectSourceQuerySummary?.total_count ?? 0) - state.deselectedProjectSourceIds.size,
    )
    : state.selectedProjectSourceIds.size;
}

export function projectSourceTotalCount(): number {
  return state.projectSourceSummary?.total_count ?? 0;
}

export function hasProjectSourceSelection(): boolean {
  return projectSourceSelectionCount() > 0;
}

export function isProjectSourceSelected(sourceId: string): boolean {
  return state.allMatchingProjectSourcesSelected
    ? !state.deselectedProjectSourceIds.has(sourceId)
    : state.selectedProjectSourceIds.has(sourceId);
}

export function clearProjectSourceSelection(): void {
  state.selectedProjectSourceIds.clear();
  state.selectedProjectSourcesById.clear();
  state.allMatchingProjectSourcesSelected = false;
  state.deselectedProjectSourceIds.clear();
}

export function selectAllMatchingProjectSources(): void {
  state.selectedProjectSourceIds.clear();
  state.selectedProjectSourcesById.clear();
  state.deselectedProjectSourceIds.clear();
  state.allMatchingProjectSourcesSelected = (state.projectSourceQuerySummary?.total_count ?? 0) > 0;
}

export function projectSourceSelectionDescriptor(): ProjectSourceSelectionDescriptor {
  return {
    all_matching: state.allMatchingProjectSourcesSelected,
    query: state.sourceSearchQuery,
    ids: Array.from(state.selectedProjectSourceIds),
    excluded_ids: Array.from(state.deselectedProjectSourceIds),
  };
}

export async function queryProjectSourceSelectionActionSummary(): Promise<ProjectSourceSelectionActionSummary | null> {
  if (!state.activeProject || !hasProjectSourceSelection()) return null;
  return invoke<ProjectSourceSelectionActionSummary>("query_project_source_selection_action_summary_command", {
    rootPath: state.activeProject.rootPath,
    selection: projectSourceSelectionDescriptor(),
  });
}

export async function resolveSelectedProjectPdfPaths(): Promise<string[]> {
  if (!state.activeProject || !hasProjectSourceSelection()) return [];
  return invoke<string[]>("resolve_project_source_selection_pdf_paths_command", {
    rootPath: state.activeProject.rootPath,
    selection: projectSourceSelectionDescriptor(),
  });
}

export function rememberSelectedProjectSource(source: ProjectSource): void {
  state.selectedProjectSourceIds.add(source.id);
  state.selectedProjectSourcesById.set(source.id, source);
}

export function forgetSelectedProjectSource(sourceId: string): void {
  state.selectedProjectSourceIds.delete(sourceId);
  state.selectedProjectSourcesById.delete(sourceId);
}

export function normalizeProjectSource(source: ProjectSource): ProjectSource {
  const concreteReasonCodes = (source.reason_codes || []).filter(
    (code) =>
      code !== "text_extracted_with_warnings" &&
      code !== "processed_with_warnings" &&
      code !== "review_recommended" &&
      code !== "text_extracted" &&
      code !== "processed_ok"
  );

  let status = source.status;
  if (status === "text_extracted_with_warnings" && concreteReasonCodes.length === 0) {
    status = "text_extracted";
  }
  if (status === "processed_with_warnings" && concreteReasonCodes.length === 0) {
    status = "processed";
  }

  return {
    ...source,
    status,
    reason_codes: concreteReasonCodes,
  };
}

export async function loadProjectSourcesForActiveProject(offset = 0): Promise<ProjectSourcePage | null> {
  if (!state.activeProject) {
    state.projectSources = [];
    state.projectSourceSummary = null;
    state.projectSourceQuerySummary = null;
    state.projectSourcePageOffset = 0;
    return null;
  }
  const generation = ++state.projectSourceQueryGeneration;
  state.projectSourceQueryPending = true;
  window.dispatchEvent(new CustomEvent(PROJECT_SOURCES_UPDATED_EVENT));
  try {
    const page = await invoke<ProjectSourcePage>("query_project_sources_command", {
      rootPath: state.activeProject.rootPath,
      query: state.sourceSearchQuery,
      statusFilter: null,
      kindFilter: null,
      offset,
      limit: state.projectSourcePageLimit,
    });
    if (generation !== state.projectSourceQueryGeneration) {
      return null;
    }
    state.projectSources = page.items.map(normalizeProjectSource);
    state.projectSourceSummary = page.inventory_summary;
    state.projectSourceQuerySummary = page.summary;
    state.projectSourcePageOffset = page.offset;
    return page;
  } finally {
    if (generation === state.projectSourceQueryGeneration) {
      state.projectSourceQueryPending = false;
      window.dispatchEvent(new CustomEvent(PROJECT_SOURCES_UPDATED_EVENT));
    }
  }
}

export async function fastImportFilesToActiveProject(
  paths: string[],
): Promise<ProjectSourceFastImportResult | null> {
  if (!state.activeProject || paths.length === 0) return null;
  return invoke<ProjectSourceFastImportResult>("fast_import_project_source_files_command", {
    rootPath: state.activeProject.rootPath,
    paths,
  });
}

export async function fastImportFolderToActiveProject(
  folderPath: string,
): Promise<ProjectSourceFastImportResult | null> {
  if (!state.activeProject) return null;
  return invoke<ProjectSourceFastImportResult>("fast_import_project_source_folder_command", {
    rootPath: state.activeProject.rootPath,
    folderPath,
  });
}

export async function reviewFilesForActiveProjectSources(paths: string[]): Promise<ProjectSourceImportReview | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  const review = await invoke<ProjectSourceImportReview>("review_project_source_files_import_command", {
    rootPath: state.activeProject.rootPath,
    paths,
  });
  if (review) {
    review.candidates = review.candidates.map(normalizeProjectSource);
  }
  return review;
}

export async function confirmFilesForActiveProjectSources(
  paths: string[],
  pdfHandlingOverrides: ProjectSourcePdfHandlingOverride[],
): Promise<ProjectSourceImportResult | null> {
  if (!state.activeProject || paths.length === 0) {
    return null;
  }
  return invoke<ProjectSourceImportResult>("confirm_project_source_files_import_command", {
    rootPath: state.activeProject.rootPath,
    paths,
    pdfHandlingOverrides,
  });
}

export async function reviewFolderForActiveProjectSources(path: string): Promise<ProjectSourceImportReview | null> {
  if (!state.activeProject) {
    return null;
  }
  const review = await invoke<ProjectSourceImportReview>("review_project_source_folder_import_command", {
    rootPath: state.activeProject.rootPath,
    folderPath: path,
  });
  if (review) {
    review.candidates = review.candidates.map(normalizeProjectSource);
  }
  return review;
}

export async function confirmFolderForActiveProjectSources(
  path: string,
  pdfHandlingOverrides: ProjectSourcePdfHandlingOverride[],
): Promise<ProjectSourceImportResult | null> {
  if (!state.activeProject) {
    return null;
  }
  return invoke<ProjectSourceImportResult>("confirm_project_source_folder_import_command", {
    rootPath: state.activeProject.rootPath,
    folderPath: path,
    pdfHandlingOverrides,
  });
}

export async function removeSelectedProjectSources(): Promise<ProjectSourceRemovalResult | null> {
  if (!state.activeProject || !hasProjectSourceSelection()) return null;
  return invoke<ProjectSourceRemovalResult>("remove_project_source_selection_command", {
    rootPath: state.activeProject.rootPath,
    allMatching: state.allMatchingProjectSourcesSelected,
    query: state.sourceSearchQuery,
    ids: Array.from(state.selectedProjectSourceIds),
    excludedIds: Array.from(state.deselectedProjectSourceIds),
  });
}
