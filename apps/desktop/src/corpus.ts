import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { dom } from "./dom";
import { updateFileList } from "./file-list";
import { state } from "./state";
import type { CleaningConfig } from "./generated/CleaningConfig.js";
import type { CorpusSummary } from "./generated/CorpusSummary.js";
import type { PdfAuditResult } from "./generated/PdfAuditResult.js";
import type { PdfAuditSuggestedProfile } from "./generated/PdfAuditSuggestedProfile.js";
import type { CorpusLoadResult } from "./types";

interface CorpusCallbacks {
  updateWordCount: () => void;
}

let callbacks: CorpusCallbacks = {
  updateWordCount: () => {},
};

type PdfIntakeProfileId = "standard" | "layout" | "ocr";
type PdfAuditSeverity = PdfAuditSuggestedProfile;

interface PdfIntakeProfile {
  statusLabel: string;
  apply: (config: CleaningConfig) => void;
}

const pdfIntakeProfiles: Record<PdfIntakeProfileId, PdfIntakeProfile> = {
  standard: {
    statusLabel: "standard embedded-text profile",
    apply: (config) => {
      config.pdf_text_source = "EmbeddedText";
      config.pdf_embedded_text_strategy = "PdfiumFlat";
      config.remove_repeated_pdf_headers_footers = false;
      config.remove_pdf_page_labels = false;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
  layout: {
    statusLabel: "layout-heavy profile",
    apply: (config) => {
      config.pdf_text_source = "EmbeddedText";
      config.pdf_embedded_text_strategy = "PdfiumVisualSingleColumn";
      config.remove_repeated_pdf_headers_footers = true;
      config.remove_pdf_page_labels = true;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
  ocr: {
    statusLabel: "scanned/OCR rescue profile",
    apply: (config) => {
      config.pdf_text_source = "Ocr";
      config.pdf_embedded_text_strategy = "PdfiumFlat";
      config.remove_repeated_pdf_headers_footers = true;
      config.remove_pdf_page_labels = true;
      config.remove_pdf_symbol_heavy_artifacts = false;
      config.remove_pdf_code_like_blocks = false;
      config.remove_pdf_formula_like_lines = false;
    },
  },
};

const auditProfileToIntakeProfile: Record<PdfAuditSuggestedProfile, PdfIntakeProfileId> = {
  standard: "standard",
  layout_heavy: "layout",
  ocr_rescue: "ocr",
};

const profileDisplayNames: Record<PdfIntakeProfileId, string> = {
  standard: "Standard",
  layout: "Layout-heavy",
  ocr: "OCR rescue",
};

let pdfIntakeSelectedPaths: string[] = [];
let pdfIntakeManualProfileOverride = false;
let settingPdfIntakeProfile = false;

function clearStateForLoad(): void {
  state.currentCorpusVersion = 0;
  state.allFiles = [];
  state.visibleFiles = [];
  state.selectedCorpusIndices.clear();
  dom.previewContent.textContent = "Select files from the left panel to preview their contents.";
  dom.processedPreviewContent.textContent = "Select files from the left panel to preview processed text.";
  updateFileList();
}

function selectedPdfIntakeProfileId(): PdfIntakeProfileId {
  const selected = document.querySelector<HTMLInputElement>('input[name="pdf-intake-profile"]:checked');
  if (selected?.value === "layout" || selected?.value === "ocr") {
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

function resetPdfIntakeSession(): void {
  pdfIntakeSelectedPaths = [];
  pdfIntakeManualProfileOverride = false;
  setPdfIntakeProfileId("standard");
  dom.pdfIntakeStatus.textContent = "";
  dom.pdfIntakeAuditSummary.textContent = "";
  dom.pdfIntakeAuditResults.replaceChildren();
  dom.pdfIntakeAuditPanel.classList.add("hidden");
  dom.loadPdfIntakeFilesBtn.classList.add("hidden");
  dom.loadPdfIntakeFilesBtn.disabled = true;
  dom.choosePdfIntakeFilesBtn.disabled = false;
  dom.choosePdfIntakeFilesBtn.textContent = "Choose PDFs...";
}

function closePdfIntakeModal(): void {
  dom.pdfIntakeStatus.textContent = "";
  dom.pdfIntakeModal.classList.add("hidden");
}

function openPdfIntakeModal(): void {
  resetPdfIntakeSession();
  dom.pdfIntakeModal.classList.remove("hidden");
}

function isPdfPath(path: string): boolean {
  return path.toLowerCase().endsWith(".pdf");
}

async function handleChoosePdfIntakeFiles(): Promise<void> {
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
    dom.statusBar.textContent = `PDF diagnostics ready. Batch/global suggestion: ${profileDisplayNames[suggestedProfile]}.`;
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

async function handleLoadPdfIntakeFiles(): Promise<void> {
  if (pdfIntakeSelectedPaths.length === 0) {
    dom.pdfIntakeStatus.textContent = "Choose PDFs first.";
    return;
  }

  const profile = pdfIntakeProfiles[selectedPdfIntakeProfileId()];

  try {
    dom.pdfIntakeStatus.textContent = "Loading PDFs...";
    dom.statusBar.textContent = "Loading PDFs...";
    dom.choosePdfIntakeFilesBtn.disabled = true;
    dom.loadPdfIntakeFilesBtn.disabled = true;
    clearStateForLoad();

    const result = await invoke<CorpusLoadResult>("load_files_command", { paths: pdfIntakeSelectedPaths });

    profile.apply(state.activeCleaningConfig);
    state.currentCorpusVersion = result.corpusVersion;
    state.currentCorpusRoot = result.report.root;
    state.allFiles = result.report.files;
    state.visibleFiles = state.allFiles.map((record, i) => ({ corpusIndex: i, record }));

    renderSummary(result.report.summary);
    updateFileList();
    closePdfIntakeModal();

    const fileLabel = state.allFiles.length === 1 ? "PDF file" : "PDF files";
    dom.statusBar.textContent = `Loaded ${state.allFiles.length} ${fileLabel} with ${profile.statusLabel}.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.pdfIntakeStatus.textContent = `Error: ${error}`;
    dom.statusBar.textContent = `Error: ${error}`;
    console.error(error);
  } finally {
    dom.choosePdfIntakeFilesBtn.disabled = false;
    dom.loadPdfIntakeFilesBtn.disabled = pdfIntakeSelectedPaths.length === 0;
  }
}

function renderPdfAuditLoading(fileCount: number): void {
  dom.pdfIntakeAuditPanel.classList.remove("hidden");
  dom.pdfIntakeAuditSummary.textContent = `Checking ${fileCount} selected PDF${fileCount === 1 ? "" : "s"} without running OCR.`;
  dom.pdfIntakeAuditResults.replaceChildren();
}

function batchSuggestedProfile(results: PdfAuditResult[]): PdfIntakeProfileId {
  let strongest: PdfAuditSeverity = "standard";
  for (const result of results) {
    if (result.suggested_profile === "ocr_rescue") {
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
  const profileMessage = suggestionApplied
    ? `The batch/global suggestion was applied: ${profileDisplayNames[suggestedProfile]}.`
    : `The batch/global suggestion is ${profileDisplayNames[suggestedProfile]}; your selected ${profileDisplayNames[currentProfile]} profile was kept.`;
  dom.pdfIntakeAuditSummary.textContent = `${profileMessage} Audit checks embedded text and OCR model files only; it does not run OCR or prove OCR will succeed.`;

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

async function handleOpenDir(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
  });
  if (selected === null || Array.isArray(selected)) {
    dom.statusBar.textContent = "Open directory cancelled.";
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

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
}

async function handleOpenFiles(): Promise<void> {
  const selected = await open({
    multiple: true,
    filters: [{ name: "Supported Documents", extensions: ["txt", "html", "htm", "docx", "pdf"] }]
  });
  if (selected === null || !Array.isArray(selected)) {
    dom.statusBar.textContent = "Open files cancelled.";
    return;
  }

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

    dom.statusBar.textContent = `Loaded ${state.allFiles.length} files.`;
    callbacks.updateWordCount();
  } catch (error) {
    dom.statusBar.textContent = `Error: ${error}`;
    console.error(error);
  }
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

export function initCorpusHandlers(nextCallbacks: CorpusCallbacks): void {
  callbacks = nextCallbacks;
  dom.menuOpenDir.addEventListener("click", handleOpenDir);
  dom.menuOpenFiles.addEventListener("click", handleOpenFiles);
  dom.menuOpenPdfIntake.addEventListener("click", openPdfIntakeModal);
  dom.cancelPdfIntakeBtn.addEventListener("click", closePdfIntakeModal);
  dom.btnClosePdfIntakeModalTop.addEventListener("click", closePdfIntakeModal);
  dom.choosePdfIntakeFilesBtn.addEventListener("click", handleChoosePdfIntakeFiles);
  dom.loadPdfIntakeFilesBtn.addEventListener("click", handleLoadPdfIntakeFiles);
  document.querySelectorAll<HTMLInputElement>('input[name="pdf-intake-profile"]').forEach((input) => {
    input.addEventListener("change", () => {
      if (!settingPdfIntakeProfile) {
        pdfIntakeManualProfileOverride = true;
      }
    });
  });
}
