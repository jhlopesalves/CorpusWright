import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import { dom } from "./dom";
import { updateFileList } from "./file-list";
import { state } from "./state";
import {
  confirmFilesForActiveProjectSources,
  confirmFolderForActiveProjectSources,
  extractionRouteLabel,
  loadableProjectSourcePaths,
  loadProjectSourcesForActiveProject,
  pdfProfileLabel,
  removeProjectSourcePaths,
  selectedProjectSourcePaths,
  type ProjectSourcePdfHandlingOverride,
  reviewFilesForActiveProjectSources,
  reviewFolderForActiveProjectSources,
  sourceHasExtractedText,
  sourceKindLabel,
} from "./sources";
import type { CleaningConfig } from "./generated/CleaningConfig.js";
import type { CorpusSummary } from "./generated/CorpusSummary.js";
import type { PdfOcrQuality } from "./generated/PdfOcrQuality.js";
import type { PdfAuditResult } from "./generated/PdfAuditResult.js";
import type { PdfAuditSuggestedProfile } from "./generated/PdfAuditSuggestedProfile.js";
import type { ProjectSourceImportReview } from "./generated/ProjectSourceImportReview.js";
import type { ProjectSourcePdfProfile } from "./generated/ProjectSourcePdfProfile.js";
import type { ProjectSource } from "./generated/ProjectSource.js";
import type { ProjectSourceUpdate } from "./generated/ProjectSourceUpdate.js";
import type { CorpusLoadResult } from "./types";

interface CorpusCallbacks {
  updateWordCount: () => void;
}

let callbacks: CorpusCallbacks = {
  updateWordCount: () => {},
};

type PdfIntakeProfileId = "standard" | "layout" | "force-ocr";

interface ImportSourcesChoice {
  pdfHandlingOverrides: ProjectSourcePdfHandlingOverride[];
  extractTextNow: boolean;
}

type ImportSourcesResolve = (choice: ImportSourcesChoice | null) => void;

interface PdfIntakeProfile {
  statusLabel: string;
  apply: (config: CleaningConfig) => void;
}

const pdfIntakeProfiles: Record<PdfIntakeProfileId, PdfIntakeProfile> = {
  standard: {
    statusLabel: "standard embedded-text handling",
    apply: (config) => {
      config.pdf_text_source = "EmbeddedText";
      config.pdf_ocr_quality = "Balanced";
      config.pdf_embedded_text_strategy = "PdfiumFlat";
      config.remove_repeated_pdf_headers_footers = false;
      config.remove_pdf_page_labels = false;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
  layout: {
    statusLabel: "layout-aware embedded text handling",
    apply: (config) => {
      config.pdf_text_source = "EmbeddedText";
      config.pdf_ocr_quality = "Balanced";
      config.pdf_embedded_text_strategy = "PdfiumVisualSingleColumn";
      config.remove_repeated_pdf_headers_footers = false;
      config.remove_pdf_page_labels = false;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
  "force-ocr": {
    statusLabel: "full OCR all pages handling",
    apply: (config) => {
      config.pdf_text_source = "ForceOcr";
      config.pdf_ocr_quality = "HighQuality";
      config.pdf_embedded_text_strategy = "PdfiumFlat";
      config.remove_repeated_pdf_headers_footers = false;
      config.remove_pdf_page_labels = false;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
};

const auditProfileToIntakeProfile: Record<PdfAuditSuggestedProfile, PdfIntakeProfileId> = {
  standard: "standard",
  layout_heavy: "layout",
  ocr_rescue: "force-ocr",
  force_ocr: "force-ocr",
};

const profileDisplayNames: Record<PdfIntakeProfileId, string> = {
  standard: "Standard embedded text",
  layout: "Layout-aware embedded text",
  "force-ocr": "Full OCR all pages",
};

type PdfDetectionKind = "embedded_text" | "ocr_required" | "unknown";

interface PendingPdfHandling {
  source: ProjectSource;
  detection: PdfDetectionKind;
  recommendedProfile: ProjectSourcePdfProfile;
  selectedProfile: ProjectSourcePdfProfile;
}

const PDF_HANDLING_OPTIONS: ProjectSourcePdfProfile[] = [
  "standard",
  "layout_heavy",
  "force_ocr",
  "no_automatic_ocr",
];

const OCR_BASED_PDF_HANDLING = new Set<ProjectSourcePdfProfile>(["force_ocr"]);

let pdfIntakeMode: "legacy" | "prepare-sources" = "legacy";

let pdfIntakeSelectedPaths: string[] = [];
let pdfIntakeManualProfileOverride = false;
let settingPdfIntakeProfile = false;
let pdfIntakeMaterializing = false;
let pdfIntakeCancelRequested = false;
let pdfIntakeProgressUnlisten: UnlistenFn | null = null;
let pdfIntakeSessionId = 0;
let pdfIntakeIsForceOcr = false;
let lastPdfIntakeProgressRenderAt = 0;
let lastPdfIntakeRenderedFileIndex = 0;
let importSourcesResolve: ImportSourcesResolve | null = null;
let activeImportCandidates: ProjectSource[] = [];
let pendingPdfHandling = new Map<string, PendingPdfHandling>();

interface PdfIntakeMaterializationProgress {
  currentFile: string;
  currentFileIndex: number;
  totalFiles: number;
  currentPage: number;
  totalPages: number | null;
  profile: string;
  method: string;
  status: string;
  elapsedMs: number;
  estimatedRemainingMs: number | null;
  pagesPerMinute: number | null;
  latestWarning: string | null;
}

export function clearStateForLoad(): void {
  state.currentCorpusVersion = 0;
  state.allFiles = [];
  state.visibleFiles = [];
  state.selectedCorpusIndices.clear();
  state.selectedProjectSourceIds.clear();
  state.currentCorpusRoot = null;
  state.lastSelectedCorpusIndex = null;
  state.lastSelectedProjectSourceId = null;
  state.lastScanCandidates = [];
  state.selectedCandidateIds.clear();
  dom.previewContent.textContent = "Select files from the left panel to preview their contents.";
  dom.processedPreviewContent.textContent = "Select files from the left panel to preview processed text.";
  dom.corpusSummary.replaceChildren();
  updateFileList();
}

function createProjectEmptyState(): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "project-empty-state";

  const title = document.createElement("h2");
  title.textContent = "This corpus project has no source files yet.";

  const body = document.createElement("p");
  body.textContent = "Add PDFs, DOCX, TXT, HTML, or plain text files to begin preparing your corpus.";

  const actions = document.createElement("div");
  actions.className = "project-empty-actions";

  const addFiles = document.createElement("button");
  addFiles.type = "button";
  addFiles.className = "primary-btn";
  addFiles.textContent = "Add Files";
  addFiles.addEventListener("click", () => {
    openFilesFromPicker();
  });

  const addFolder = document.createElement("button");
  addFolder.type = "button";
  addFolder.textContent = "Add Folder";
  addFolder.addEventListener("click", () => {
    openDirectoryFromPicker();
  });

  actions.append(addFiles, addFolder);
  wrapper.append(title, body, actions);
  return wrapper;
}

export function renderProjectEmptyState(): void {
  dom.previewContent.replaceChildren(createProjectEmptyState());
  dom.processedPreviewContent.textContent = "Add source files to preview processed text.";
}

function sourceImportSummaryLine(review: ProjectSourceImportReview): string {
  const count = review.candidates.length;
  const sourceLabel = count === 1 ? "source" : "sources";
  const extras: string[] = [];
  if (review.duplicate_count > 0) {
    const duplicateLabel = review.duplicate_count === 1 ? "duplicate" : "duplicates";
    extras.push(`${review.duplicate_count} ${duplicateLabel} skipped`);
  }
  if (review.ignored_count > 0) {
    const ignoredLabel = review.ignored_count === 1 ? "unsupported file" : "unsupported files";
    extras.push(`${review.ignored_count} ${ignoredLabel} ignored`);
  }
  return extras.length === 0
    ? `${count} ${sourceLabel} found`
    : `${count} ${sourceLabel} found (${extras.join(", ")})`;
}

function groupedImportSources(review: ProjectSourceImportReview): Map<string, number> {
  const groups = new Map<string, number>();
  for (const source of review.candidates) {
    const key = sourceKindLabel(source);
    groups.set(key, (groups.get(key) ?? 0) + 1);
  }
  return groups;
}

function renderImportSourcesGroups(review: ProjectSourceImportReview): void {
  dom.sourceImportGroups.replaceChildren();
  const groups = groupedImportSources(review);
  for (const [kind, count] of groups) {
    const row = document.createElement("div");
    row.className = "source-import-group-row";

    const label = document.createElement("span");
    label.textContent = kind;

    const scopeLabel = document.createElement("span");
    scopeLabel.textContent = "Pending import";

    const countLabel = document.createElement("strong");
    countLabel.textContent = count.toLocaleString();

    row.append(label, scopeLabel, countLabel);
    dom.sourceImportGroups.appendChild(row);
  }
}

function importPathKey(path: string): string {
  return path.replace(/\\/g, "/").toLowerCase();
}

function pdfDetectionForSource(source: ProjectSource): PdfDetectionKind {
  if (source.extraction_route === "embedded_text") {
    return "embedded_text";
  }
  if (
    source.status === "needs_ocr" ||
    source.extraction_route === "ocr_required" ||
    source.extraction_route === "force_ocr" ||
    source.reason_codes.some(code => code === "ocr_required" || code === "pdf_no_text_layer")
  ) {
    return "ocr_required";
  }
  return "unknown";
}

function recommendedProfileForSource(source: ProjectSource): ProjectSourcePdfProfile {
  if (source.pdf_profile) {
    return canonicalPdfProfile(source.pdf_profile);
  }
  switch (source.extraction_route) {
    case "embedded_text":
      return "standard";
    case "force_ocr":
    case "ocr_required":
    case "ocr_if_needed":
      return "force_ocr";
    default:
      return "standard";
  }
}

function canonicalPdfProfile(profile: ProjectSourcePdfProfile): ProjectSourcePdfProfile {
  return profile === "ocr_rescue" ? "force_ocr" : profile;
}

function detectionGroupLabel(detection: PdfDetectionKind, count: number): string {
  const noun = count === 1 ? "PDF" : "PDFs";
  switch (detection) {
    case "embedded_text": return `Embedded-text ${noun}`;
    case "ocr_required": return `OCR-required ${noun}`;
    case "unknown": return `Unknown ${noun}`;
  }
}

function detectionLabel(detection: PdfDetectionKind): string {
  switch (detection) {
    case "embedded_text": return "embedded text";
    case "ocr_required": return "OCR required";
    case "unknown": return "unknown";
  }
}

function resetPendingPdfHandling(review: ProjectSourceImportReview): void {
  pendingPdfHandling = new Map();
  for (const source of review.candidates) {
    if (source.kind !== "pdf") {
      continue;
    }
    const recommendedProfile = recommendedProfileForSource(source);
    pendingPdfHandling.set(importPathKey(source.path), {
      source,
      detection: pdfDetectionForSource(source),
      recommendedProfile,
      selectedProfile: recommendedProfile,
    });
  }
}

function pendingPdfHandlingItems(): PendingPdfHandling[] {
  return Array.from(pendingPdfHandling.values());
}

function createPdfHandlingSelect(selectedProfile: ProjectSourcePdfProfile): HTMLSelectElement {
  const select = document.createElement("select");
  select.className = "settings-select source-import-handling-select";
  for (const profile of PDF_HANDLING_OPTIONS) {
    const option = document.createElement("option");
    option.value = profile;
    option.textContent = pdfProfileLabel(profile);
    option.selected = profile === selectedProfile;
    select.appendChild(option);
  }
  return select;
}

function renderPdfHandlingRecommendations(items: PendingPdfHandling[]): void {
  dom.sourceImportPdfRecommendations.replaceChildren();

  const title = document.createElement("h4");
  title.textContent = "Recommended PDF handling";
  dom.sourceImportPdfRecommendations.appendChild(title);

  const counts = new Map<string, { detection: PdfDetectionKind; profile: ProjectSourcePdfProfile; count: number }>();
  for (const item of items) {
    const key = `${item.detection}|${item.recommendedProfile}`;
    const current = counts.get(key) ?? {
      detection: item.detection,
      profile: item.recommendedProfile,
      count: 0,
    };
    current.count += 1;
    counts.set(key, current);
  }

  for (const { detection, profile, count } of counts.values()) {
    const row = document.createElement("div");
    row.className = "source-import-recommendation-row";
    row.textContent = `${detectionGroupLabel(detection, count)} -> ${pdfProfileLabel(profile)}`;
    dom.sourceImportPdfRecommendations.appendChild(row);
  }
}

function renderPdfHandlingGroupControls(items: PendingPdfHandling[]): void {
  dom.sourceImportPdfGroupControls.replaceChildren();
  const groupOrder: PdfDetectionKind[] = ["embedded_text", "ocr_required", "unknown"];

  for (const detection of groupOrder) {
    const groupItems = items.filter(item => item.detection === detection);
    if (groupItems.length === 0) {
      continue;
    }

    const selectedProfiles = new Set(groupItems.map(item => item.selectedProfile));
    const selectedProfile = selectedProfiles.size === 1 ? groupItems[0].selectedProfile : null;
    const recommendedProfiles = new Set(groupItems.map(item => item.recommendedProfile));
    const recommendedLabel = recommendedProfiles.size === 1
      ? pdfProfileLabel(groupItems[0].recommendedProfile)
      : "mixed recommendations";

    const group = document.createElement("div");
    group.className = "source-import-pdf-group";

    const title = document.createElement("h4");
    title.textContent = detectionGroupLabel(detection, groupItems.length);

    const recommended = document.createElement("p");
    recommended.className = "source-import-help";
    recommended.textContent = `Recommended: ${recommendedLabel}. Applies to ${groupItems.length.toLocaleString()} pending ${groupItems.length === 1 ? "PDF" : "PDFs"}.`;

    const label = document.createElement("label");
    label.className = "source-import-group-select";
    const labelText = document.createElement("span");
    labelText.textContent = `Apply to all ${groupItems.length.toLocaleString()}`;
    const select = createPdfHandlingSelect(selectedProfile ?? groupItems[0].recommendedProfile);
    if (selectedProfile === null) {
      const mixed = document.createElement("option");
      mixed.value = "";
      mixed.textContent = "Mixed handling";
      mixed.disabled = true;
      mixed.selected = true;
      select.insertBefore(mixed, select.firstChild);
    }
    select.addEventListener("change", () => {
      const profile = select.value as ProjectSourcePdfProfile;
      for (const item of groupItems) {
        item.selectedProfile = profile;
      }
      renderImportSourcesPdfHandling();
      renderImportSourcesList(activeImportCandidates);
    });

    label.append(labelText, select);
    group.append(title, recommended, label);
    dom.sourceImportPdfGroupControls.appendChild(group);
  }
}

function syncImportOcrQualityState(items: PendingPdfHandling[]): void {
  const ocrCount = items.filter(item => OCR_BASED_PDF_HANDLING.has(item.selectedProfile)).length;
  const hasOcrHandling = ocrCount > 0;
  dom.sourceImportOcrQuality.disabled = !hasOcrHandling;
  dom.sourceImportOcrQualityRow.classList.toggle("is-disabled", !hasOcrHandling);
  dom.sourceImportOcrQualityHelp.textContent = hasOcrHandling
    ? `Used when ${ocrCount.toLocaleString()} pending ${ocrCount === 1 ? "PDF uses" : "PDFs use"} OCR-based handling. OCR runs during text extraction.`
    : "Not currently used because no pending PDF uses OCR-based handling.";
}

function renderImportSourcesPdfHandling(): void {
  const items = pendingPdfHandlingItems();
  if (items.length === 0) {
    dom.sourceImportPdfHandling.classList.add("hidden");
    return;
  }

  dom.sourceImportPdfHandling.classList.remove("hidden");
  dom.sourceImportPdfScope.textContent = `Applies to ${items.length.toLocaleString()} pending ${items.length === 1 ? "PDF" : "PDFs"}.`;
  renderPdfHandlingRecommendations(items);
  renderPdfHandlingGroupControls(items);
  syncImportOcrQualityState(items);
}

function renderImportSourcesList(sources: ProjectSource[]): void {
  dom.sourceImportList.replaceChildren();
  for (const source of sources) {
    const row = document.createElement("div");
    row.className = "source-import-list-row";

    const name = document.createElement("span");
    name.className = "source-import-list-name";
    name.textContent = source.file_name;

    const meta = document.createElement("span");
    meta.className = "source-import-list-meta";
    const pdfHandling = pendingPdfHandling.get(importPathKey(source.path));
    if (pdfHandling) {
      meta.textContent = `Detected: ${detectionLabel(pdfHandling.detection)}`;
      const label = document.createElement("label");
      label.className = "source-import-row-handling";
      const labelText = document.createElement("span");
      labelText.textContent = "Handling";
      const select = createPdfHandlingSelect(pdfHandling.selectedProfile);
      select.addEventListener("change", () => {
        pdfHandling.selectedProfile = select.value as ProjectSourcePdfProfile;
        renderImportSourcesPdfHandling();
        renderImportSourcesList(activeImportCandidates);
      });
      label.append(labelText, select);
      row.append(name, meta, label);
    } else {
      meta.textContent = `${sourceKindLabel(source)} · ${extractionRouteLabel(source.extraction_route)}`;
      row.append(name, meta);
    }

    dom.sourceImportList.appendChild(row);
  }
}

function resetImportSourcesDialog(review: ProjectSourceImportReview): void {
  activeImportCandidates = review.candidates;
  resetPendingPdfHandling(review);
  const hasPendingPdfs = pendingPdfHandlingItems().length > 0;
  dom.sourceImportSummary.textContent = sourceImportSummaryLine(review);
  dom.sourceImportOcrQuality.value = state.activeCleaningConfig.pdf_ocr_quality;
  dom.sourceImportStatus.textContent = review.candidates.length === 0
    ? "No new sources are available to import."
    : "";
  dom.confirmSourceImportBtn.disabled = review.candidates.length === 0;
  dom.confirmSourceImportExtractBtn.disabled = review.candidates.length === 0 || !hasPendingPdfs;
  dom.confirmSourceImportExtractBtn.classList.toggle("hidden", !hasPendingPdfs);
  renderImportSourcesGroups(review);
  renderImportSourcesPdfHandling();
  renderImportSourcesList(review.candidates);
}

function selectedImportSourcesChoice(extractTextNow: boolean): ImportSourcesChoice {
  const ocrQuality = dom.sourceImportOcrQuality.value as PdfOcrQuality;
  return {
    extractTextNow,
    pdfHandlingOverrides: pendingPdfHandlingItems().map(item => ({
      path: item.source.path,
      pdfProfile: item.selectedProfile,
      ocrQuality,
    })),
  };
}

function closeImportSourcesDialog(choice: ImportSourcesChoice | null): void {
  dom.sourceImportModal.classList.add("hidden");
  const resolve = importSourcesResolve;
  importSourcesResolve = null;
  resolve?.(choice);
}

function showImportSourcesDialog(review: ProjectSourceImportReview): Promise<ImportSourcesChoice | null> {
  if (importSourcesResolve) {
    closeImportSourcesDialog(null);
  }
  resetImportSourcesDialog(review);
  dom.sourceImportModal.classList.remove("hidden");

  return new Promise((resolve) => {
    importSourcesResolve = resolve;
  });
}

function selectedPdfIntakeProfileId(): PdfIntakeProfileId {
  const selected = document.querySelector<HTMLInputElement>('input[name="pdf-intake-profile"]:checked');
  if (selected?.value === "layout" || selected?.value === "force-ocr") {
    return selected.value;
  }
  return "standard";
}

function setPdfIntakeProfileId(profileId: PdfIntakeProfileId): void {
  const input = document.querySelector<HTMLInputElement>(`input[name="pdf-intake-profile"][value="${profileId}"]`);
  if (!input) {
    return;
  }
  settingPdfIntakeProfile = true;
  input.checked = true;
  settingPdfIntakeProfile = false;
}

function pdfProfileToIntakeProfileId(profile: ProjectSourcePdfProfile): PdfIntakeProfileId {
  switch (profile) {
    case "layout_heavy":
      return "layout";
    case "ocr_rescue":
    case "force_ocr":
      return "force-ocr";
    case "standard":
    case "no_automatic_ocr":
      return "standard";
  }
}

function storedProfileForSource(source: ProjectSource): ProjectSourcePdfProfile {
  return canonicalPdfProfile(source.pdf_profile ?? recommendedProfileForSource(source));
}

function selectedProjectPdfSources(paths: string[]): ProjectSource[] {
  const selectedKeys = new Set(paths.map(importPathKey));
  return state.projectSources.filter((source) =>
    source.kind === "pdf" && selectedKeys.has(importPathKey(source.path))
  );
}

function strongestIntakeProfileId(profiles: ProjectSourcePdfProfile[]): PdfIntakeProfileId {
  if (profiles.includes("force_ocr") || profiles.includes("ocr_rescue")) {
    return "force-ocr";
  }
  if (profiles.includes("layout_heavy")) {
    return "layout";
  }
  return "standard";
}

function storedHandlingStatusLabel(paths: string[]): string {
  const sources = selectedProjectPdfSources(paths);
  const profiles = sources.map(storedProfileForSource);
  const uniqueProfiles = new Set(profiles);
  if (uniqueProfiles.size === 0) {
    return "stored handling";
  }
  if (uniqueProfiles.size === 1) {
    return `stored ${profileDisplayNames[pdfProfileToIntakeProfileId(profiles[0])]} handling`;
  }
  return "mixed stored handling";
}

function selectedProjectSourcesUseForceOcr(paths: string[]): boolean {
  return selectedProjectPdfSources(paths)
    .some(source => storedProfileForSource(source) === "force_ocr");
}

function setPdfIntakeHandlingChoicesVisible(visible: boolean): void {
  dom.pdfIntakeProfiles.classList.toggle("hidden", !visible);
  dom.pdfIntakeChangeHandlingBtn.classList.toggle("hidden", visible);
}

function renderStoredPdfHandlingSummary(paths: string[], selectedProfileId: PdfIntakeProfileId): void {
  const sources = selectedProjectPdfSources(paths);
  const counts = new Map<ProjectSourcePdfProfile, number>();
  for (const source of sources) {
    const profile = storedProfileForSource(source);
    counts.set(profile, (counts.get(profile) ?? 0) + 1);
  }

  const selectedCount = paths.length;
  dom.pdfIntakeStoredHandlingSummary.textContent =
    `${selectedCount.toLocaleString()} selected ${selectedCount === 1 ? "PDF" : "PDFs"}.`;
  dom.pdfIntakeStoredHandlingList.replaceChildren();

  if (pdfIntakeManualProfileOverride) {
    const item = document.createElement("li");
    item.textContent =
      `Apply ${profileDisplayNames[selectedProfileId]} to all ` +
      `${selectedCount.toLocaleString()} selected ${selectedCount === 1 ? "PDF" : "PDFs"}.`;
    dom.pdfIntakeStoredHandlingList.appendChild(item);
    return;
  }

  if (counts.size === 0) {
    const item = document.createElement("li");
    item.textContent = "Selected PDFs are not in the current project source list.";
    dom.pdfIntakeStoredHandlingList.appendChild(item);
    return;
  }

  for (const [profile, count] of counts) {
    const item = document.createElement("li");
    item.textContent = `${pdfProfileLabel(profile)}: ${count.toLocaleString()} ${count === 1 ? "PDF" : "PDFs"}`;
    dom.pdfIntakeStoredHandlingList.appendChild(item);
  }

  const selectedProfiles = Array.from(counts.keys());
  const mappedProfiles = new Set(selectedProfiles.map(pdfProfileToIntakeProfileId));
  if (mappedProfiles.size > 1) {
    const item = document.createElement("li");
    item.textContent = "This batch will preserve each source's stored handling.";
    dom.pdfIntakeStoredHandlingList.appendChild(item);
  } else {
    const item = document.createElement("li");
    item.textContent = `This batch will use stored ${profileDisplayNames[pdfProfileToIntakeProfileId(selectedProfiles[0])]}.`;
    dom.pdfIntakeStoredHandlingList.appendChild(item);
  }
}

/**
 * Reset PDF intake session state back to default.
 * This does NOT close the modal — it only resets internal state and UI elements.
 * Safe to call multiple times.
 */
function resetPdfIntakeSession(): void {
  pdfIntakeSelectedPaths = [];
  pdfIntakeManualProfileOverride = false;
  pdfIntakeCancelRequested = false;
  pdfIntakeIsForceOcr = false;
  lastPdfIntakeProgressRenderAt = 0;
  lastPdfIntakeRenderedFileIndex = 0;
  dom.pdfIntakeTitle.textContent = "Open Scanned or Complex PDFs";
  dom.pdfIntakeDescription.textContent = "Choose PDF handling for inspection-oriented processing. Preview before export: OCR may contain errors, layout reconstruction is imperfect, and source files are never modified.";
  dom.pdfIntakeModal.classList.remove("pdf-intake-processing");
  dom.pdfIntakeStoredHandling.classList.add("hidden");
  dom.pdfIntakeStoredHandlingSummary.textContent = "";
  dom.pdfIntakeStoredHandlingList.replaceChildren();
  setPdfIntakeHandlingChoicesVisible(true);
  setPdfIntakeProfileId("standard");
  dom.pdfIntakeStatus.textContent = "";
  dom.pdfIntakeAuditSummary.textContent = "";
  dom.pdfIntakeAuditResults.replaceChildren();
  dom.pdfIntakeAuditPanel.classList.add("hidden");
  resetPdfIntakeMaterializationProgress();
  dom.loadPdfIntakeFilesBtn.classList.add("hidden");
  dom.loadPdfIntakeFilesBtn.disabled = true;
  dom.choosePdfIntakeFilesBtn.classList.remove("hidden");
  dom.choosePdfIntakeFilesBtn.disabled = false;
  dom.choosePdfIntakeFilesBtn.textContent = "Choose PDFs...";
  dom.cancelPdfIntakeBtn.textContent = "Cancel";
  dom.cancelPdfIntakeBtn.disabled = false;
  dom.btnClosePdfIntakeModalTop.disabled = false;
}

/**
 * Close the PDF intake modal and reset all session state.
 * Idempotent — safe to call multiple times.
 */
function closePdfIntakeModal(): void {
  dom.pdfIntakeModal.classList.add("hidden");
  resetPdfIntakeSession();
}

function resetPdfIntakeMaterializationProgress(): void {
  dom.pdfIntakeMaterializationPanel.classList.add("hidden");
  dom.pdfIntakeMaterializationProfile.textContent = "";
  dom.pdfIntakeMaterializationMethod.textContent = "";
  dom.pdfIntakeMaterializationFile.textContent = "";
  dom.pdfIntakeMaterializationPage.textContent = "";
  dom.pdfIntakeMaterializationElapsed.textContent = "";
  dom.pdfIntakeMaterializationRemaining.textContent = "";
  dom.pdfIntakeMaterializationSpeed.textContent = "";
  dom.pdfIntakeMaterializationProgressFill.style.width = "0%";
  dom.pdfIntakeMaterializationWarning.textContent = "";
  dom.pdfIntakeMaterializationWarningRow.classList.add("hidden");
}

function formatDurationMs(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "Calculating";
  const totalSeconds = Math.max(0, Math.round(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatApproxDurationMs(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "estimating...";
  const totalSeconds = Math.max(0, Math.round(ms / 1000));
  if (totalSeconds === 0) return "complete";
  if (totalSeconds < 60) return "less than 1 min";
  const totalMinutes = Math.max(1, Math.round(totalSeconds / 60));
  if (totalMinutes < 60) return `about ${totalMinutes} min`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return minutes === 0 ? `about ${hours} hr` : `about ${hours} hr ${minutes} min`;
}

function renderPdfIntakeMaterializationProgress(progress: PdfIntakeMaterializationProgress): void {
  const totalPages = progress.totalPages === null ? "?" : progress.totalPages.toLocaleString();
  const currentPage = progress.currentPage.toLocaleString();
  dom.pdfIntakeMaterializationPanel.classList.remove("hidden");
  dom.pdfIntakeMaterializationProfile.textContent = progress.profile;
  dom.pdfIntakeMaterializationMethod.textContent = progress.method;
  dom.pdfIntakeMaterializationFile.textContent =
    `${progress.currentFileIndex.toLocaleString()} / ${progress.totalFiles.toLocaleString()} - ${progress.currentFile}`;
  dom.pdfIntakeMaterializationPage.textContent = `${currentPage} / ${totalPages}`;
  dom.pdfIntakeMaterializationElapsed.textContent = formatDurationMs(progress.elapsedMs);
  dom.pdfIntakeMaterializationRemaining.textContent = formatApproxDurationMs(progress.estimatedRemainingMs);
  dom.pdfIntakeMaterializationSpeed.textContent =
    progress.pagesPerMinute === null ? "estimating..." : `about ${progress.pagesPerMinute.toFixed(1)} pages/min`;
  const progressPercent = progress.totalPages === null || progress.totalPages === 0
    ? 0
    : Math.min(100, Math.max(0, (progress.currentPage / progress.totalPages) * 100));
  dom.pdfIntakeMaterializationProgressFill.style.width = `${progressPercent}%`;
  if (progress.latestWarning) {
    dom.pdfIntakeMaterializationWarning.textContent = progress.latestWarning;
    dom.pdfIntakeMaterializationWarningRow.classList.remove("hidden");
  } else {
    dom.pdfIntakeMaterializationWarning.textContent = "";
    dom.pdfIntakeMaterializationWarningRow.classList.add("hidden");
  }
  dom.pdfIntakeStatus.textContent = progress.status;
  dom.statusBar.textContent =
    `${progress.status} ${progress.currentFileIndex}/${progress.totalFiles}: ${progress.currentFile} (${currentPage}/${totalPages})`;
}

async function listenForPdfIntakeProgress(): Promise<void> {
  if (pdfIntakeProgressUnlisten) {
    pdfIntakeProgressUnlisten();
    pdfIntakeProgressUnlisten = null;
  }
  const currentSessionId = pdfIntakeSessionId;
  pdfIntakeProgressUnlisten = await listen<PdfIntakeMaterializationProgress>(
    "pdf-intake-materialization-progress",
    (event) => {
      // Ignore stale events from previous sessions or after cancellation
      if (pdfIntakeSessionId !== currentSessionId || pdfIntakeCancelRequested) {
        return;
      }
      const now = Date.now();
      const isCompletion = event.payload.totalPages !== null
        && event.payload.currentPage >= event.payload.totalPages;
      const isFileTransition = event.payload.currentFileIndex !== lastPdfIntakeRenderedFileIndex;
      // Always render file transitions and completions immediately.
      // Throttle inter-page progress to once per second to avoid flooding DOM.
      if (!isFileTransition && !isCompletion && now - lastPdfIntakeProgressRenderAt < 1000) {
        return;
      }
      lastPdfIntakeProgressRenderAt = now;
      lastPdfIntakeRenderedFileIndex = event.payload.currentFileIndex;
      renderPdfIntakeMaterializationProgress(event.payload);
    },
  );
}

function stopListeningForPdfIntakeProgress(): void {
  if (pdfIntakeProgressUnlisten) {
    pdfIntakeProgressUnlisten();
    pdfIntakeProgressUnlisten = null;
  }
}

export function openPdfIntakeModal(): void {
  pdfIntakeMode = "legacy";
  resetPdfIntakeSession();
  dom.pdfIntakeModal.classList.remove("hidden");
  dom.loadPdfIntakeFilesBtn.textContent = "Load PDFs";
}

export async function openPdfIntakeModalForSources(paths: string[], extractAfterDiagnostics = false): Promise<void> {
  pdfIntakeMode = "prepare-sources";
  resetPdfIntakeSession();
  dom.pdfIntakeModal.classList.remove("hidden");
  dom.pdfIntakeTitle.textContent = "Extract/OCR selected PDFs";
  const selectedSources = selectedProjectPdfSources(paths);
  const storedProfiles = selectedSources.map(storedProfileForSource);
  const storedProfileId = strongestIntakeProfileId(storedProfiles);
  setPdfIntakeProfileId(storedProfileId);
  dom.pdfIntakeStoredHandling.classList.remove("hidden");
  setPdfIntakeHandlingChoicesVisible(false);
  renderStoredPdfHandlingSummary(paths, storedProfileId);
  dom.pdfIntakeDescription.textContent =
    "Using stored PDF handling. Diagnostics check embedded text availability, OCR resources, page counts, and extraction risk before text extraction.";
  dom.loadPdfIntakeFilesBtn.textContent = "Extract text now";
  dom.choosePdfIntakeFilesBtn.classList.add("hidden");

  pdfIntakeSelectedPaths = paths;
  dom.pdfIntakeStatus.textContent = extractAfterDiagnostics
    ? "Checking PDFs before text extraction..."
    : "Checking PDFs...";
  dom.statusBar.textContent = extractAfterDiagnostics
    ? "Checking selected PDFs before text extraction..."
    : "Checking selected PDFs...";
  dom.choosePdfIntakeFilesBtn.disabled = true;
  dom.loadPdfIntakeFilesBtn.disabled = true;
  renderPdfAuditLoading(paths.length);

  try {
    const auditResults = await invoke<PdfAuditResult[]>("audit_pdf_files_command", { paths });
    const suggestedProfile = batchSuggestedProfile(auditResults);

    renderPdfAuditResults(auditResults, suggestedProfile, false);

    dom.pdfIntakeStatus.textContent = "Diagnostics ready.";
    dom.statusBar.textContent =
      `PDF diagnostics ready for ${paths.length.toLocaleString()} selected project ` +
      `${paths.length === 1 ? "PDF" : "PDFs"}. Using ${storedHandlingStatusLabel(paths)}.`;
    dom.loadPdfIntakeFilesBtn.classList.remove("hidden");
    dom.loadPdfIntakeFilesBtn.disabled = false;
    if (extractAfterDiagnostics) {
      await handleLoadPdfIntakeFiles();
    }
  } catch (error) {
    dom.pdfIntakeStatus.textContent = `Audit error: ${error}`;
    dom.statusBar.textContent = `Audit error: ${error}`;
    console.error(error);
  } finally {
    dom.choosePdfIntakeFilesBtn.disabled = false;
  }
}

async function handleCancelPdfIntake(): Promise<void> {
  if (!pdfIntakeMaterializing) {
    closePdfIntakeModal();
    return;
  }

  pdfIntakeCancelRequested = true;
  dom.cancelPdfIntakeBtn.disabled = true;
  const isProjectExtraction = pdfIntakeMode === "prepare-sources";
  // Use accurate wording depending on whether force OCR is running
  if (pdfIntakeIsForceOcr) {
    dom.pdfIntakeStatus.textContent = "Cancelling after the current OCR batch...";
    dom.statusBar.textContent = isProjectExtraction
      ? "Cancelling PDF text extraction after the current OCR batch..."
      : "Cancelling PDF intake after the current OCR batch...";
  } else {
    dom.pdfIntakeStatus.textContent = "Cancelling after the current PDF extraction step...";
    dom.statusBar.textContent = isProjectExtraction
      ? "Cancelling PDF text extraction after the current extraction step..."
      : "Cancelling PDF intake after the current extraction step...";
  }
  try {
    await invoke("cancel_pdf_intake_materialization_command");
  } catch (error) {
    dom.pdfIntakeStatus.textContent = `Cancel error: ${error}`;
    dom.statusBar.textContent = `Cancel error: ${error}`;
  }
}

function isPdfPath(path: string): boolean {
  return path.toLowerCase().endsWith(".pdf");
}

async function handleChoosePdfIntakeFiles(): Promise<void> {
  if (pdfIntakeMode !== "legacy") {
    return;
  }

  const selected = await open({
    multiple: true,
    filters: [{ name: "PDF Documents", extensions: ["pdf"] }],
  });

  if (selected === null || !Array.isArray(selected) || selected.length === 0) {
    dom.pdfIntakeStatus.textContent = "No PDFs selected.";
    dom.statusBar.textContent = "PDF intake cancelled.";
    return;
  }

  if (!selected.every(isPdfPath)) {
    dom.pdfIntakeStatus.textContent = "Choose PDF files only.";
    dom.statusBar.textContent = "PDF intake accepts PDF files only.";
    return;
  }

  try {
    pdfIntakeSelectedPaths = selected;
    dom.pdfIntakeStatus.textContent = "Auditing PDFs...";
    dom.statusBar.textContent = "Auditing selected PDFs...";
    dom.choosePdfIntakeFilesBtn.disabled = true;
    dom.loadPdfIntakeFilesBtn.disabled = true;
    renderPdfAuditLoading(selected.length);

    const auditResults = await invoke<PdfAuditResult[]>("audit_pdf_files_command", { paths: selected });
    const suggestedProfile = batchSuggestedProfile(auditResults);
    const shouldApplySuggestion = !pdfIntakeManualProfileOverride;

    if (shouldApplySuggestion) {
      setPdfIntakeProfileId(suggestedProfile);
    }

    renderPdfAuditResults(auditResults, suggestedProfile, shouldApplySuggestion);
    dom.pdfIntakeStatus.textContent = "Diagnostics ready.";
    dom.statusBar.textContent = `PDF diagnostics ready. Suggested handling: ${profileDisplayNames[suggestedProfile]}.`;
    dom.loadPdfIntakeFilesBtn.classList.remove("hidden");
    dom.loadPdfIntakeFilesBtn.disabled = false;
    dom.choosePdfIntakeFilesBtn.textContent = "Choose different PDFs...";
  } catch (error) {
    dom.pdfIntakeStatus.textContent = `Audit error: ${error}`;
    dom.statusBar.textContent = `Audit error: ${error}`;
    console.error(error);
  } finally {
    dom.choosePdfIntakeFilesBtn.disabled = false;
  }
}

function mapToPdfProfile(id: PdfIntakeProfileId): ProjectSourcePdfProfile {
  switch (id) {
    case "standard": return "standard";
    case "layout": return "layout_heavy";
    case "force-ocr": return "force_ocr";
  }
}

async function handleLoadPdfIntakeFiles(): Promise<void> {
  if (pdfIntakeSelectedPaths.length === 0) {
    dom.pdfIntakeStatus.textContent = "Choose PDFs first.";
    return;
  }

  const selectedProfileId = selectedPdfIntakeProfileId();
  const selectedPdfCount = pdfIntakeSelectedPaths.length;
  const isProjectPreparation = pdfIntakeMode === "prepare-sources" && state.activeProject !== null;
  const profile = pdfIntakeProfiles[selectedProfileId];
  const materializationConfig: CleaningConfig = { ...state.activeCleaningConfig };
  const selectedProfileConfig: CleaningConfig = { ...state.activeCleaningConfig };
  profile.apply(selectedProfileConfig);
  if (!isProjectPreparation) {
    Object.assign(materializationConfig, selectedProfileConfig);
  }

  const profileOverride = isProjectPreparation && pdfIntakeManualProfileOverride
    ? mapToPdfProfile(selectedProfileId)
    : null;
  pdfIntakeIsForceOcr = isProjectPreparation
    ? profileOverride === "force_ocr" || (profileOverride === null && selectedProjectSourcesUseForceOcr(pdfIntakeSelectedPaths))
    : selectedProfileId === "force-ocr";

  // Increment session ID to invalidate any stale progress listeners
  pdfIntakeSessionId += 1;
  const currentSessionId = pdfIntakeSessionId;

  try {
    pdfIntakeMaterializing = true;
    pdfIntakeCancelRequested = false;
    dom.pdfIntakeModal.classList.add("pdf-intake-processing");
    resetPdfIntakeMaterializationProgress();
    await listenForPdfIntakeProgress();
    dom.pdfIntakeStatus.textContent = isProjectPreparation
      ? "Starting text extraction..."
      : "Preparing PDF materialisation...";
    dom.statusBar.textContent = isProjectPreparation
      ? "Starting PDF text extraction..."
      : "Preparing PDF materialisation...";
    dom.choosePdfIntakeFilesBtn.disabled = true;
    dom.loadPdfIntakeFilesBtn.disabled = true;
    dom.cancelPdfIntakeBtn.textContent = isProjectPreparation ? "Cancel extraction" : "Cancel materialisation";
    dom.cancelPdfIntakeBtn.disabled = false;
    dom.btnClosePdfIntakeModalTop.disabled = true;
    clearStateForLoad();

    const result = isProjectPreparation
      ? await invoke<CorpusLoadResult>("prepare_project_pdf_sources_command", {
        rootPath: state.activeProject!.rootPath,
        paths: pdfIntakeSelectedPaths,
        cleaningConfig: materializationConfig,
        profileOverride,
        ocrQuality: selectedProfileConfig.pdf_ocr_quality,
      })
      : await invoke<CorpusLoadResult>("load_pdf_intake_files_command", {
        paths: pdfIntakeSelectedPaths,
        cleaningConfig: materializationConfig,
        profile: profile.statusLabel,
      });

    if (!isProjectPreparation) {
      Object.assign(state.activeCleaningConfig, materializationConfig);
    }

    if (isProjectPreparation) {
      await loadProjectSourcesForActiveProject();
      await loadCorpusFromProjectSources();
    } else {
      state.currentCorpusVersion = result.corpusVersion;
      state.currentCorpusRoot = result.report.root;
      state.allFiles = result.report.files;
      state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));
      renderSummary(result.report.summary);
      updateFileList();
    }


    pdfIntakeMaterializing = false;
    const projectCompletionHandlingLabel = isProjectPreparation
      ? (profileOverride === null
        ? storedHandlingStatusLabel(pdfIntakeSelectedPaths)
        : `${profileDisplayNames[selectedProfileId]} handling`)
      : "";
    closePdfIntakeModal();

    if (isProjectPreparation) {
      const fileLabel = selectedPdfCount === 1 ? "selected PDF" : "selected PDFs";
      dom.statusBar.textContent =
        `Extracted text from ${selectedPdfCount.toLocaleString()} ${fileLabel} with ${projectCompletionHandlingLabel}.`;
    } else {
      const fileLabel = state.allFiles.length === 1 ? "PDF file" : "PDF files";
      dom.statusBar.textContent = `Loaded ${state.allFiles.length} ${fileLabel} with materialised ${profile.statusLabel}.`;
    }
    callbacks.updateWordCount();
  } catch (error) {
    // Only act if this is still the active session
    if (pdfIntakeSessionId !== currentSessionId) {
      return;
    }

    const cancelled = pdfIntakeCancelRequested;
    const message = cancelled
      ? (isProjectPreparation
        ? "PDF text extraction cancelled. No half-extracted source text was kept."
        : "PDF intake cancelled. No half-materialised corpus was loaded.")
      : `Error: ${error}`;
    dom.pdfIntakeStatus.textContent = message;
    dom.statusBar.textContent = message;
    console.error(error);

    // Restore project sources after clearStateForLoad() if project extraction is cancelled.
    if (cancelled && pdfIntakeMode === "prepare-sources" && state.activeProject) {
      try {
        await loadProjectSourcesForActiveProject();
        await loadCorpusFromProjectSources();
        dom.statusBar.textContent = "PDF text extraction cancelled. Sources restored.";
      } catch {
        dom.statusBar.textContent = "PDF text extraction cancelled. Reload project sources to restore the source list.";
      }
    }

    // Close the modal after cancellation — do NOT leave it open with profile choices visible
    closePdfIntakeModal();
  } finally {
    stopListeningForPdfIntakeProgress();
    pdfIntakeMaterializing = false;
    dom.pdfIntakeModal.classList.remove("pdf-intake-processing");
    // Do NOT call resetPdfIntakeSession() here — closePdfIntakeModal() already resets if modal was closed,
    // but if we didn't close (e.g. we were never materializing), reset is still safe.
    // closePdfIntakeModal() was already called in catch or success path; if neither ran,
    // we need to close here. Calling closePdfIntakeModal() again is idempotent.
    closePdfIntakeModal();
    dom.choosePdfIntakeFilesBtn.disabled = false;
    dom.loadPdfIntakeFilesBtn.disabled = true;
    dom.cancelPdfIntakeBtn.textContent = "Cancel";
    dom.cancelPdfIntakeBtn.disabled = false;
    dom.btnClosePdfIntakeModalTop.disabled = false;
  }
}

function renderPdfAuditLoading(fileCount: number): void {
  dom.pdfIntakeAuditPanel.classList.remove("hidden");
  dom.pdfIntakeAuditSummary.textContent =
    `Checking ${fileCount} selected PDF${fileCount === 1 ? "" : "s"} before text extraction.`;
  dom.pdfIntakeAuditResults.replaceChildren();
}

function batchSuggestedProfile(results: PdfAuditResult[]): PdfIntakeProfileId {
  let strongest: PdfAuditSuggestedProfile = "standard";
  for (const result of results) {
    if (result.suggested_profile === "force_ocr") {
      strongest = "force_ocr";
    } else if (result.suggested_profile === "ocr_rescue" && strongest !== "force_ocr") {
      strongest = "ocr_rescue";
    } else if (result.suggested_profile === "layout_heavy" && strongest === "standard") {
      strongest = "layout_heavy";
    }
  }
  return auditProfileToIntakeProfile[strongest];
}

function renderPdfAuditResults(
  results: PdfAuditResult[],
  suggestedProfile: PdfIntakeProfileId,
  suggestionApplied: boolean,
): void {
  dom.pdfIntakeAuditPanel.classList.remove("hidden");
  dom.pdfIntakeAuditResults.replaceChildren();

  const currentProfile = selectedPdfIntakeProfileId();
  let profileMessage: string;
  if (pdfIntakeMode === "prepare-sources" && !pdfIntakeManualProfileOverride) {
    profileMessage = `Using stored PDF handling. The strongest diagnostics suggestion is ${profileDisplayNames[suggestedProfile]}.`;
  } else if (pdfIntakeMode === "prepare-sources" && pdfIntakeManualProfileOverride) {
    profileMessage = `The selected override will apply ${profileDisplayNames[currentProfile]} to every selected PDF.`;
  } else {
    profileMessage = suggestionApplied
      ? `The suggested handling was applied: ${profileDisplayNames[suggestedProfile]}.`
      : `The suggested handling is ${profileDisplayNames[suggestedProfile]}; your selected ${profileDisplayNames[currentProfile]} handling was kept.`;
  }
  dom.pdfIntakeAuditSummary.textContent =
    `${profileMessage} Diagnostics check embedded text availability, OCR resources, page counts, and extraction risk before text extraction.`;

  for (const result of results) {
    dom.pdfIntakeAuditResults.appendChild(renderPdfAuditCard(result));
  }
}

function renderPdfAuditCard(result: PdfAuditResult): HTMLElement {
  const card = document.createElement("div");
  card.className = `pdf-intake-audit-card pdf-intake-audit-card-${result.suggested_profile}`;

  const title = document.createElement("div");
  title.className = "pdf-intake-audit-card-title";
  title.textContent = result.file_name;
  card.appendChild(title);

  const details = document.createElement("div");
  details.className = "pdf-intake-audit-details";
  details.appendChild(renderPdfAuditDetail("Pages", formatOptionalNumber(result.page_count)));
  details.appendChild(renderPdfAuditDetail("Sampled", result.sampled_page_count.toString()));
  details.appendChild(renderPdfAuditDetail("Embedded text", result.embedded_text_detected ? "detected" : "not detected"));
  details.appendChild(renderPdfAuditDetail("Sample chars", result.embedded_text_chars.toString()));
  details.appendChild(renderPdfAuditDetail("Quality", formatAuditQuality(result.quality)));
  details.appendChild(renderPdfAuditDetail("PDFium", result.pdfium_available ? "available" : "unavailable"));
  details.appendChild(
    renderPdfAuditDetail(
      "OCR models",
      result.ocr_model_resources_available
        ? (result.ocr_full_usability_checked ? "available" : "available; full OCR not checked")
        : "unavailable",
    ),
  );
  details.appendChild(renderPdfAuditDetail("Fallback", result.degraded_fallback_used ? "degraded fallback used" : "not used"));
  details.appendChild(renderPdfAuditDetail("Suggested", profileDisplayNames[auditProfileToIntakeProfile[result.suggested_profile]]));
  card.appendChild(details);

  if (result.warnings.length > 0) {
    const warnings = document.createElement("ul");
    warnings.className = "pdf-intake-audit-warnings";
    for (const warning of result.warnings) {
      const item = document.createElement("li");
      item.textContent = warning;
      warnings.appendChild(item);
    }
    card.appendChild(warnings);
  }

  return card;
}

function renderPdfAuditDetail(labelText: string, valueText: string): HTMLElement {
  const detail = document.createElement("div");
  detail.className = "pdf-intake-audit-detail";

  const label = document.createElement("span");
  label.className = "pdf-intake-audit-detail-label";
  label.textContent = labelText;

  const value = document.createElement("span");
  value.className = "pdf-intake-audit-detail-value";
  value.textContent = valueText;

  detail.append(label, value);
  return detail;
}

function formatOptionalNumber(value: number | null): string {
  return value === null ? "unknown" : value.toString();
}

function formatAuditQuality(quality: PdfAuditResult["quality"]): string {
  switch (quality) {
    case "good":
      return "good";
    case "suspicious":
      return "suspicious";
    case "poor":
      return "poor";
    case "empty":
      return "empty";
    case "unknown":
      return "unknown";
  }
}

export async function loadDirectoryPath(selected: string, registerProjectSources = true): Promise<void> {
  if (registerProjectSources && state.activeProject) {
    await registerFolderForActiveProject(selected);
    return;
  }

  try {
    dom.statusBar.textContent = "Scanning...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("scan_directory_command", { path: selected });

    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();
    if (state.allFiles.length === 0) {
      renderProjectEmptyState();
    }

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    renderProjectEmptyState();
    console.error(error);
  }
}

export async function loadFilesIntoCorpus(selected: string[]): Promise<void> {
  try {
    dom.statusBar.textContent = "Loading files...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("load_files_command", { paths: selected });

    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();
    if (state.allFiles.length === 0) {
      renderProjectEmptyState();
    }

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    renderProjectEmptyState();
    console.error(error);
  }
}

export async function loadFilesPaths(selected: string[], registerProjectSources = true): Promise<void> {
  if (registerProjectSources && state.activeProject) {
    await registerFilesForActiveProject(selected);
    return;
  }

  await loadFilesIntoCorpus(selected);
}

function importCompletionMessage(review: ProjectSourceImportReview, update: ProjectSourceUpdate): string {
  const importedIds = new Set(review.candidates.map((source) => source.id));
  const importedSources = update.manifest.sources.filter((source) => importedIds.has(source.id));
  const readyCount = importedSources.filter((source) => source.status === "ready_to_process").length;
  const needsOcrCount = importedSources.filter((source) => source.status === "needs_ocr").length;
  const extractedCount = importedSources.filter(sourceHasExtractedText).length;
  const failedCount = importedSources.filter((source) => source.status === "failed").length;
  const unprocessedCount = importedSources.filter((source) => source.status === "unprocessed").length;
  const parts: string[] = [];

  if (readyCount > 0) {
    parts.push(`${readyCount.toLocaleString()} ready to process`);
  }
  if (needsOcrCount > 0) {
    parts.push(`${needsOcrCount.toLocaleString()} need OCR`);
  }
  if (extractedCount > 0) {
    parts.push(`${extractedCount.toLocaleString()} text extracted`);
  }
  if (unprocessedCount > 0) {
    parts.push(`${unprocessedCount.toLocaleString()} unprocessed`);
  }
  if (failedCount > 0) {
    parts.push(`${failedCount.toLocaleString()} failed`);
  }

  const importedLabel = `${update.added_count.toLocaleString()} source${update.added_count === 1 ? "" : "s"}`;
  const duplicateLabel = update.duplicate_count > 0
    ? ` (${update.duplicate_count.toLocaleString()} duplicate${update.duplicate_count === 1 ? "" : "s"} skipped)`
    : "";
  return parts.length > 0
    ? `Imported ${importedLabel}: ${parts.join(", ")}.${duplicateLabel}`
    : `Imported ${importedLabel}.${duplicateLabel}`;
}

function importedPdfPathsForReview(review: ProjectSourceImportReview, update: ProjectSourceUpdate): string[] {
  const importedIds = new Set(review.candidates.map((source) => source.id));
  return update.manifest.sources
    .filter((source) => importedIds.has(source.id))
    .filter((source) => source.kind === "pdf" && source.status !== "failed")
    .map((source) => source.path);
}

export async function loadCorpusFromProjectSources(): Promise<void> {
  if (!state.activeProject) {
    return;
  }

  const loadablePaths = loadableProjectSourcePaths();
  if (loadablePaths.length === 0) {
    clearStateForLoad();
    renderProjectEmptyState();
    dom.statusBar.textContent = state.projectSources.length === 0
      ? "This project has no source files yet."
      : "This project has no processable source files.";
    return;
  }

  try {
    dom.statusBar.textContent = "Loading project sources...";
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("load_project_source_records_command", {
      rootPath: state.activeProject.rootPath,
    });

    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();
    dom.statusBar.textContent = `Loaded ${state.allFiles.length} project sources.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error loading project sources: ${error}`;
    renderProjectEmptyState();
    console.error(error);
  }
}

export async function registerFilesForActiveProject(selected: string[]): Promise<void> {
  if (!state.activeProject) {
    await loadFilesIntoCorpus(selected);
    return;
  }

  try {
    dom.statusBar.textContent = "Inspecting selected source files...";
    const review = await reviewFilesForActiveProjectSources(selected);
    if (!review) {
      return;
    }

    const choice = await showImportSourcesDialog(review);
    if (!choice) {
      dom.statusBar.textContent = "Source import cancelled.";
      return;
    }

    dom.statusBar.textContent = "Importing source files...";
    const update = await confirmFilesForActiveProjectSources(
      selected,
      choice.pdfHandlingOverrides,
    );
    if (update) {
      if (choice.extractTextNow) {
        const importedPdfPaths = importedPdfPathsForReview(review, update);
        if (importedPdfPaths.length > 0) {
          await openPdfIntakeModalForSources(importedPdfPaths, true);
        } else {
          await loadCorpusFromProjectSources();
          dom.statusBar.textContent = importCompletionMessage(review, update);
        }
      } else {
        await loadCorpusFromProjectSources();
        dom.statusBar.textContent = importCompletionMessage(review, update);
      }
    } else {
      await loadCorpusFromProjectSources();
    }
  } catch (error) {
    dom.statusBar.textContent = `Error importing source files: ${error}`;
    console.error(error);
  }
}

export async function registerFolderForActiveProject(selected: string): Promise<void> {
  if (!state.activeProject) {
    await loadDirectoryPath(selected, false);
    return;
  }

  try {
    dom.statusBar.textContent = "Inspecting source folder...";
    const review = await reviewFolderForActiveProjectSources(selected);
    if (!review) {
      return;
    }

    const choice = await showImportSourcesDialog(review);
    if (!choice) {
      dom.statusBar.textContent = "Source import cancelled.";
      return;
    }

    dom.statusBar.textContent = "Importing source folder...";
    const update = await confirmFolderForActiveProjectSources(
      selected,
      choice.pdfHandlingOverrides,
    );
    if (update) {
      if (choice.extractTextNow) {
        const importedPdfPaths = importedPdfPathsForReview(review, update);
        if (importedPdfPaths.length > 0) {
          await openPdfIntakeModalForSources(importedPdfPaths, true);
        } else {
          await loadCorpusFromProjectSources();
          dom.statusBar.textContent = importCompletionMessage(review, update);
        }
      } else {
        await loadCorpusFromProjectSources();
        dom.statusBar.textContent = importCompletionMessage(review, update);
      }
    } else {
      await loadCorpusFromProjectSources();
    }
  } catch (error) {
    dom.statusBar.textContent = `Error importing source folder: ${error}`;
    console.error(error);
  }
}

export async function openDirectoryFromPicker(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    dom.statusBar.textContent = "Open directory cancelled.";
    return;
  }

  await loadDirectoryPath(selected);
}

export async function openFilesFromPicker(): Promise<void> {
  const selected = await open({
    multiple: true,
    filters: [{ name: "Supported Documents", extensions: ["txt", "html", "htm", "docx", "pdf"] }]
  });
  if (selected === null || !Array.isArray(selected)) {
    dom.statusBar.textContent = "Open files cancelled.";
    return;
  }

  await loadFilesPaths(selected);
}

function renderSummary(summary: CorpusSummary): void {
  const totalFiles = summary.files_supported;
  const sizeMB = summary.total_size_bytes / (1024 * 1024);
  const avgSizeBytes = totalFiles > 0 ? summary.total_size_bytes / totalFiles : 0;

  let avgSizeStr = "0 MB";
  if (avgSizeBytes > 0) {
    if (avgSizeBytes < 1024 * 1024) {
      avgSizeStr = (avgSizeBytes / 1024).toFixed(2) + " KB";
    } else {
      avgSizeStr = (avgSizeBytes / (1024 * 1024)).toFixed(2) + " MB";
    }
  }

  dom.corpusSummary.innerHTML = `
    <div class="summary-header">Corpus Summary</div>
    <div class="summary-grid">
      <div class="summary-metric">
        <div class="summary-value">${totalFiles}</div>
        <div class="summary-label">Total Files</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value">${sizeMB.toFixed(2)} MB</div>
        <div class="summary-label">Total Size</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value">${avgSizeStr}</div>
        <div class="summary-label">Average File</div>
      </div>
      <div class="summary-metric">
        <div class="summary-value" id="summary-total-words">Calculating...</div>
        <div class="summary-label" id="summary-word-label">Cleaned Token Count</div>
      </div>
      <div class="summary-metric full-width">
        <div class="summary-value" id="summary-avg-words">Calculating...</div>
        <div class="summary-label">Avg Words / File</div>
      </div>
    </div>
    <div class="summary-diagnostics">
      Types: TXT ${summary.document_type_counts.text}, HTML ${summary.document_type_counts.html}, DOCX ${summary.document_type_counts.docx}, PDF ${summary.document_type_counts.pdf}
    </div>
  `;
}

async function handleRemoveSelectedSources(): Promise<void> {
  if (!state.activeProject || state.selectedProjectSourceIds.size === 0) {
    return;
  }

  const selectedPaths = selectedProjectSourcePaths();

  if (selectedPaths.length === 0) {
    return;
  }

  const confirmed = window.confirm(
    `Remove ${selectedPaths.length} selected source${selectedPaths.length === 1 ? "" : "s"} from the project inventory?\n\nSource files on disk will not be deleted.`,
  );
  if (!confirmed) {
    return;
  }

  try {
    dom.statusBar.textContent = "Removing selected sources...";
    await removeProjectSourcePaths(selectedPaths);
    state.selectedCorpusIndices.clear();
    state.selectedProjectSourceIds.clear();
    await loadCorpusFromProjectSources();
    dom.statusBar.textContent = `Removed ${selectedPaths.length} source${selectedPaths.length === 1 ? "" : "s"} from project inventory.`;
  } catch (error) {
    dom.statusBar.textContent = `Error removing sources: ${error}`;
    console.error(error);
  }
}

export function initCorpusHandlers(nextCallbacks: CorpusCallbacks): void {
  callbacks = nextCallbacks;
  dom.sourceAddFilesBtn.addEventListener("click", openFilesFromPicker);
  dom.sourceAddFolderBtn.addEventListener("click", openDirectoryFromPicker);
  dom.sourceRemoveBtn.addEventListener("click", handleRemoveSelectedSources);
  dom.cancelSourceImportBtn.addEventListener("click", () => {
    closeImportSourcesDialog(null);
  });
  dom.btnCloseSourceImportModalTop.addEventListener("click", () => {
    closeImportSourcesDialog(null);
  });
  dom.confirmSourceImportBtn.addEventListener("click", () => {
    closeImportSourcesDialog(selectedImportSourcesChoice(false));
  });
  dom.confirmSourceImportExtractBtn.addEventListener("click", () => {
    closeImportSourcesDialog(selectedImportSourcesChoice(true));
  });
  dom.cancelPdfIntakeBtn.addEventListener("click", () => {
    handleCancelPdfIntake();
  });
  dom.btnClosePdfIntakeModalTop.addEventListener("click", () => {
    handleCancelPdfIntake();
  });
  dom.choosePdfIntakeFilesBtn.addEventListener("click", handleChoosePdfIntakeFiles);
  dom.loadPdfIntakeFilesBtn.addEventListener("click", handleLoadPdfIntakeFiles);
  dom.pdfIntakeChangeHandlingBtn.addEventListener("click", () => {
    setPdfIntakeHandlingChoicesVisible(true);
    pdfIntakeManualProfileOverride = true;
    renderStoredPdfHandlingSummary(pdfIntakeSelectedPaths, selectedPdfIntakeProfileId());
    dom.pdfIntakeStatus.textContent = "Choose one PDF handling profile to apply to every selected PDF.";
  });

  dom.sourcePreparePdfsBtn.addEventListener("click", () => {
    const selectedPaths = selectedProjectSourcePaths()
      .filter((path) => path.toLowerCase().endsWith(".pdf"));

    if (selectedPaths.length > 0) {
      openPdfIntakeModalForSources(selectedPaths);
    }
  });

  dom.sourceImportResetPdfHandling.addEventListener("click", () => {
    for (const item of pendingPdfHandling.values()) {
      item.selectedProfile = item.recommendedProfile;
    }
    renderImportSourcesPdfHandling();
    renderImportSourcesList(activeImportCandidates);
  });

  document.querySelectorAll<HTMLInputElement>('input[name="pdf-intake-profile"]').forEach((input) => {
    input.addEventListener("change", () => {
      if (!settingPdfIntakeProfile) {
        pdfIntakeManualProfileOverride = true;
        if (pdfIntakeMode === "prepare-sources") {
          renderStoredPdfHandlingSummary(pdfIntakeSelectedPaths, selectedPdfIntakeProfileId());
        }
      }
    });
  });
}
