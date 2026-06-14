import { invoke } from "@tauri-apps/api/core";

import {
  dom,
  getArtifactTextModeRadios,
  getSelectedArtifactTextMode,
} from "./dom";
import { state } from "./state";
import type { RemovalRule } from "./generated/CleaningConfig.js";
import type { PositionSummary } from "./generated/PositionSummary.js";
import type { RepeatedArtifactCandidate } from "./generated/RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanConfig } from "./generated/RepeatedArtifactScanConfig.js";
import type { RepeatedArtifactScanDiagnostics } from "./generated/RepeatedArtifactScanDiagnostics.js";
import type { RepeatedArtifactScanReport } from "./generated/RepeatedArtifactScanReport.js";
import type { RemovalScope } from "./generated/RemovalScope.js";
import { formatRemovalRuleText } from "./custom-removals.js";

interface RepeatedArtifactCallbacks {
  renderCustomRemovals: () => void;
  schedulePreviewUpdate: (delay: number) => void;
}

function ruleDisplayText(rule: RemovalRule): string {
  return formatRemovalRuleText(rule);
}

function activeCustomRemovalCount(): number {
  return state.activeCleaningConfig.remove_patterns.length +
    state.activeCleaningConfig.removal_rules.filter(rule => rule.enabled).length;
}

function exactLineRuleId(candidate: RepeatedArtifactCandidate, scope: RemovalScope): string {
  return `repeated-artifact:${scope}:${candidate.candidate_id}`;
}

function normalizedLineRuleId(candidate: RepeatedArtifactCandidate, scope: RemovalScope): string {
  return `repeated-artifact:normalized-line:${scope}:${candidate.candidate_id}`;
}

function removalRuleDuplicateKey(rule: RemovalRule): string {
  if (rule.matcher.kind === "literal") {
    return `${rule.scope}\0${rule.matcher.text}`;
  }
  if (rule.matcher.kind === "normalized_line") {
    return `${rule.scope}\0normalized_line\0${rule.matcher.normalized_key}`;
  }
  return `${rule.scope}\0${rule.id}`;
}

function exactLineRuleDuplicateKey(text: string, scope: RemovalScope): string {
  return `${scope}\0${text}`;
}

function normalizedLineRuleDuplicateKey(normalizedKey: string, scope: RemovalScope): string {
  return `${scope}\0normalized_line\0${normalizedKey}`;
}

function computeCandidateScope(candidate: RepeatedArtifactCandidate): RemovalScope {
  if (
    candidate.position_summary_is_page_based &&
    candidate.position_summary.middle_count === 0
  ) {
    const top = candidate.position_summary.top_count;
    const bot = candidate.position_summary.bottom_count;
    if (top > 0 && bot === 0) {
      return "page_top";
    }
    if (top === 0 && bot > 0) {
      return "page_bottom";
    }
    if (top > 0 && bot > 0) {
      return "page_top_or_bottom";
    }
  }
  return "whole_line";
}

function createExactLineRemovalRule(candidate: RepeatedArtifactCandidate, scope: RemovalScope): RemovalRule {
  const matcherText = candidate.display_text;
  return {
    id: exactLineRuleId(candidate, scope),
    label: formatRemovalRuleText({
      id: "",
      label: "",
      source: "promoted_repeated_artifact",
      matcher: {
        kind: "literal",
        text: matcherText,
      },
      scope,
      enabled: true,
    }),
    source: "promoted_repeated_artifact",
    matcher: {
      kind: "literal",
      text: matcherText,
    },
    scope,
    enabled: true,
  };
}

function createNormalizedLineRemovalRule(candidate: RepeatedArtifactCandidate, normalizedKey: string, scope: RemovalScope): RemovalRule {
  return {
    id: normalizedLineRuleId(candidate, scope),
    label: formatRemovalRuleText({
      id: "",
      label: "",
      source: "promoted_repeated_artifact",
      matcher: {
        kind: "normalized_line",
        normalized_key: normalizedKey,
      },
      scope,
      enabled: true,
    }),
    source: "promoted_repeated_artifact",
    matcher: {
      kind: "normalized_line",
      normalized_key: normalizedKey,
    },
    scope,
    enabled: true,
  };
}

export function initRepeatedArtifactFinder(callbacks: RepeatedArtifactCallbacks): void {
  const radioModes = getArtifactTextModeRadios();

  if (!dom.menuRepeatedArtifactFinder || !dom.repeatedArtifactModal) return;

  let scanGeneration = 0;
  let scanTimerInterval: ReturnType<typeof setInterval> | null = null;
  let currentAbortController: AbortController | null = null;

  function updateProcessedWarning(): void {
    const isProcessed = getSelectedArtifactTextMode()?.value === "processed";
    const count = activeCustomRemovalCount();
    if (isProcessed && count > 0) {
      const legacyRemovals = state.activeCleaningConfig.remove_patterns;
      const ruleRemovals = state.activeCleaningConfig.removal_rules.filter(rule => rule.enabled);
      let previewText = `Processed scans apply ${count} active Custom Removal item(s).`;
      if (count > 0) {
        const examples = [
          ...legacyRemovals,
          ...ruleRemovals.map(ruleDisplayText),
        ].slice(0, 3);
        previewText += ` Active removals include: ${examples.join(", ")}${count > 3 ? ", ..." : ""}.`;
      }
      dom.artifactProcessedWarning.textContent = previewText;
      dom.artifactProcessedWarning.style.display = "block";
    } else {
      dom.artifactProcessedWarning.style.display = "none";
    }
  }

  function setStatus(stage: string, elapsed?: number): void {
    const timeStr = elapsed !== undefined ? ` (${elapsed.toFixed(1)}s)` : "";
    dom.tblArtifactCandidates.innerHTML = `
      <tr>
        <td colspan="8" style="padding: 20px; text-align: center;">
          <div class="spinner" style="margin: 0 auto 10px auto; width: 24px; height: 24px;"></div>
          ${stage}${timeStr}
        </td>
      </tr>
    `;
    dom.artifactDetailsContent.innerHTML = `<div style="color: var(--text-muted); text-align: center; margin-top: 40px;">${stage}...</div>`;
  }

  function resetScanControls(): void {
    dom.btnRunArtifactScan.style.display = "inline-block";
    dom.btnRunArtifactScan.disabled = false;
    dom.btnRunArtifactScan.textContent = "Run Scan";
    dom.btnCancelScan.style.display = "none";
    if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }
  }

  function updateAddSelectedButtonState(): void {
    dom.btnAddSelectedRemovals.disabled = state.selectedCandidateIds.size === 0;
  }

  function renderDiagnostics(diags: RepeatedArtifactScanDiagnostics): void {
    const parts: string[] = [];
    parts.push(`Files: ${diags.files_scanned}/${diags.files_requested} scanned`);
    parts.push(`Lines: ${diags.total_raw_lines.toLocaleString()}`);
    parts.push(`Candidate keys: ${diags.total_candidate_keys_before_filtering}`);
    parts.push(`After occurrence filter: ${diags.candidates_after_min_occurrences}`);
    parts.push(`After file filter: ${diags.candidates_after_min_files}`);
    parts.push(`Final: ${diags.final_candidates}`);

    const failureParts: string[] = [];
    if (diags.files_failed_extraction > 0) {
      failureParts.push(`Extraction failures: ${diags.files_failed_extraction}`);
    }
    if (diags.files_empty_after_extraction > 0) {
      failureParts.push(`Empty extractions: ${diags.files_empty_after_extraction}`);
    }

    let text = parts.join(" · ");
    if (failureParts.length > 0) {
      text += " · " + failureParts.join(" · ");
    }

    if (diags.analysed_processed_text && diags.custom_removals_active > 0) {
      text += ` · Processed scan applied ${diags.custom_removals_active} active Custom Removal item(s).`;
    }

    dom.artifactDiagnostics.textContent = text;
    dom.artifactDiagnostics.classList.remove("hidden");
  }

  dom.menuRepeatedArtifactFinder.addEventListener("click", () => {
    dom.repeatedArtifactModal.classList.remove("hidden");
    updateProcessedWarning();
  });

  function closeArtifactModal(): void {
    scanGeneration++;
    if (currentAbortController) { currentAbortController.abort(); currentAbortController = null; }
    invoke("cancel_repeated_artifacts_command").catch(() => {});
    resetScanControls();
    dom.lblScanTime.style.display = "none";
    state.selectedCandidateIds.clear();
    dom.artifactDiagnostics.classList.add("hidden");
    dom.repeatedArtifactModal.classList.add("hidden");
  }

  dom.btnCloseArtifactModal.addEventListener("click", closeArtifactModal);
  dom.btnCloseArtifactModalTop.addEventListener("click", closeArtifactModal);

  dom.btnAddSelectedRemovals.addEventListener("click", () => {
    const count = state.selectedCandidateIds.size;
    if (count === 0) return;

    const existingSet = new Set(state.activeCleaningConfig.remove_patterns);
    const existingRuleIds = new Set(state.activeCleaningConfig.removal_rules.map(rule => rule.id));
    const existingRuleKeys = new Set(state.activeCleaningConfig.removal_rules.map(removalRuleDuplicateKey));
    let literalRemovalsAdded = 0;
    let pageTopRulesAdded = 0;
    let pageBottomRulesAdded = 0;
    let pageTopBottomRulesAdded = 0;
    let wholeLineRulesAdded = 0;
    let normalizedFamilyRulesAdded = 0;
    let skippedDuplicates = 0;
    let normalizedFallbackCandidates = 0;
    let cappedNormalizedFallbackCandidates = 0;
    let trackedRawVariantsAddedFromFallback = 0;

    for (const id of state.selectedCandidateIds) {
      const cand = state.lastScanCandidates.find(c => c.candidate_id === id);
      if (!cand) continue;

      const isNormalized = cand.kind === "normalized_line";
      const scope = computeCandidateScope(cand);

      if (cand.kind === "exact_line") {
        const text = cand.display_text;
        if (!text) continue;
        const rule = createExactLineRemovalRule(cand, scope);
        const ruleKey = exactLineRuleDuplicateKey(text, scope);
        if (existingRuleIds.has(rule.id) || existingRuleKeys.has(ruleKey)) {
          skippedDuplicates++;
          continue;
        }

        state.activeCleaningConfig.removal_rules.push(rule);
        existingRuleIds.add(rule.id);
        existingRuleKeys.add(ruleKey);

        if (scope === "page_top") {
          pageTopRulesAdded++;
        } else if (scope === "page_bottom") {
          pageBottomRulesAdded++;
        } else if (scope === "page_top_or_bottom") {
          pageTopBottomRulesAdded++;
        } else {
          wholeLineRulesAdded++;
        }
        continue;
      }

      if (isNormalized) {
        const normalizedKey = (cand.normalized_key || "").trim();
        if (normalizedKey) {
          const rule = createNormalizedLineRemovalRule(cand, normalizedKey, scope);
          const ruleKey = normalizedLineRuleDuplicateKey(normalizedKey, scope);
          if (existingRuleIds.has(rule.id) || existingRuleKeys.has(ruleKey)) {
            skippedDuplicates++;
            continue;
          }

          state.activeCleaningConfig.removal_rules.push(rule);
          existingRuleIds.add(rule.id);
          existingRuleKeys.add(ruleKey);

          if (scope === "page_top") {
            pageTopRulesAdded++;
          } else if (scope === "page_bottom") {
            pageBottomRulesAdded++;
          } else if (scope === "page_top_or_bottom") {
            pageTopBottomRulesAdded++;
          } else {
            normalizedFamilyRulesAdded++;
          }
          continue;
        }

        if (!cand.raw_variants || cand.raw_variants.length === 0) continue;
        normalizedFallbackCandidates++;
        if (cand.raw_variant_count_is_capped) {
          cappedNormalizedFallbackCandidates++;
        }
        for (const variant of cand.raw_variants) {
          if (!existingSet.has(variant)) {
            state.activeCleaningConfig.remove_patterns.push(variant);
            existingSet.add(variant);
            literalRemovalsAdded++;
            trackedRawVariantsAddedFromFallback++;
          } else {
            skippedDuplicates++;
          }
        }
      } else {
        const text = cand.display_text;
        if (!existingSet.has(text)) {
          state.activeCleaningConfig.remove_patterns.push(text);
          existingSet.add(text);
          literalRemovalsAdded++;
        } else {
          skippedDuplicates++;
        }
      }
    }

    const rulesAdded = pageTopRulesAdded + pageBottomRulesAdded + pageTopBottomRulesAdded + wholeLineRulesAdded + normalizedFamilyRulesAdded;
    if (literalRemovalsAdded > 0 || rulesAdded > 0) {
      state.tempRemovePatterns = [...state.activeCleaningConfig.remove_patterns];
      state.tempRemovalRules = state.activeCleaningConfig.removal_rules.map((rule) => ({
        ...rule,
        matcher: { ...rule.matcher },
      }));
      callbacks.renderCustomRemovals();
    }

    const statusParts: string[] = [];
    const addedParts: string[] = [];
    if (pageTopRulesAdded > 0) {
      addedParts.push(`${pageTopRulesAdded} page-top rule${pageTopRulesAdded === 1 ? "" : "s"}`);
    }
    if (pageBottomRulesAdded > 0) {
      addedParts.push(`${pageBottomRulesAdded} page-bottom rule${pageBottomRulesAdded === 1 ? "" : "s"}`);
    }
    if (pageTopBottomRulesAdded > 0) {
      addedParts.push(`${pageTopBottomRulesAdded} page-top/bottom rule${pageTopBottomRulesAdded === 1 ? "" : "s"}`);
    }
    if (wholeLineRulesAdded > 0) {
      addedParts.push(`${wholeLineRulesAdded} whole-line rule${wholeLineRulesAdded === 1 ? "" : "s"}`);
    }
    if (normalizedFamilyRulesAdded > 0) {
      addedParts.push(`${normalizedFamilyRulesAdded} normalised family rule${normalizedFamilyRulesAdded === 1 ? "" : "s"}`);
    }
    if (literalRemovalsAdded > 0) {
      addedParts.push(`${literalRemovalsAdded} literal sequence${literalRemovalsAdded === 1 ? "" : "s"}`);
    }
    if (addedParts.length > 0) {
      const lastPart = addedParts[addedParts.length - 1];
      const leadingParts = addedParts.slice(0, -1);
      const addedText = leadingParts.length > 0
        ? `${leadingParts.join(", ")} and ${lastPart}`
        : lastPart;
      statusParts.push(`Added ${addedText} to Custom Removals`);
    }
    if (normalizedFallbackCandidates > 0) {
      const addedTrackedText = trackedRawVariantsAddedFromFallback > 0
        ? `; ${trackedRawVariantsAddedFromFallback} tracked raw variant${trackedRawVariantsAddedFromFallback === 1 ? "" : "s"} added from them`
        : "";
      statusParts.push(`${normalizedFallbackCandidates} normalised candidate${normalizedFallbackCandidates === 1 ? "" : "s"} had no normalised key and fell back to tracked raw variants${addedTrackedText}`);
    }
    if (skippedDuplicates > 0) {
      statusParts.push(`${skippedDuplicates} duplicate${skippedDuplicates === 1 ? "" : "s"} skipped`);
    }
    if (cappedNormalizedFallbackCandidates > 0) {
      statusParts.push(`Warning: ${cappedNormalizedFallbackCandidates} fallback normalised candidate${cappedNormalizedFallbackCandidates === 1 ? " was" : "s were"} capped, so only the tracked variants were added`);
    }
    dom.lblArtifactAddStatus.textContent = statusParts.length > 0
      ? statusParts.join(". ") + "."
      : "No new removals added.";
    setTimeout(() => { dom.lblArtifactAddStatus.textContent = ""; }, 5000);
  });

  dom.btnRunArtifactScan.addEventListener("click", async () => {
    if (state.allFiles.length === 0) {
      alert("No files loaded in the corpus. Please open a directory or load files first.");
      return;
    }

    const textMode = getSelectedArtifactTextMode()?.value || "original";
    const analyseProcessed = textMode === "processed";

    state.scanWasProcessed = analyseProcessed;
    state.removalCountAtScanStart = activeCustomRemovalCount();

    const config: RepeatedArtifactScanConfig = {
      analyse_processed_text: analyseProcessed,
      include_exact_lines: dom.chkArtifactExact.checked,
      include_normalized_lines: dom.chkArtifactNorm.checked,
      include_inline_artifacts: dom.chkArtifactInline.checked,
      include_two_line_blocks: dom.chkArtifact2Line.checked,
      include_three_line_blocks: dom.chkArtifact3Line.checked,
      include_text_dominant: dom.chkArtifactText.checked,
      include_mixed_text_numbers: dom.chkArtifactMixed.checked,
      include_numeric_dominant: dom.chkArtifactNumeric.checked,
      include_symbol_noise: dom.chkArtifactSymbol.checked,
      min_occurrences: parseInt(dom.numArtifactMinOcc.value, 10) || 5,
      min_files: parseInt(dom.numArtifactMinFiles.value, 10) || 1,
      max_candidates: parseInt(dom.numArtifactMaxCand.value, 10) || 100,
      max_examples_per_candidate: Math.min(Math.max(parseInt(dom.numArtifactMaxExamples.value, 10) || 25, 1), 100),
      min_line_chars: 4,
      max_line_chars: 300,
    };

    dom.btnRunArtifactScan.style.display = "none";
    dom.btnCancelScan.style.display = "inline-block";
    dom.btnCancelScan.disabled = false;
    dom.btnCancelScan.textContent = "Cancel";
    dom.lblScanTime.style.display = "inline";
    dom.lblScanTime.textContent = "0.0s";

    const myGen = ++scanGeneration;
    const startTime = performance.now();

    setStatus("Preparing files...", 0);

    if (scanTimerInterval) clearInterval(scanTimerInterval);
    scanTimerInterval = setInterval(() => {
      const elapsed = (performance.now() - startTime) / 1000;
      dom.lblScanTime.textContent = `${elapsed.toFixed(1)}s`;
      if (elapsed < 2) {
        setStatus("Preparing files...", elapsed);
      } else if (elapsed < 3) {
        setStatus("Extracting text...", elapsed);
      } else if (elapsed < 8) {
        setStatus("Scanning lines...", elapsed);
      } else if (elapsed < 30) {
        setStatus("Merging candidates...", elapsed);
      } else {
        setStatus("Ranking candidates...", elapsed);
      }
    }, 500);

    const myVersion = state.currentCorpusVersion;
    const indices = Array.from(
      { length: state.allFiles.length },
      (_, corpusIndex) => corpusIndex
    );

    try {
      const report = await invoke<RepeatedArtifactScanReport>("scan_repeated_artifacts_command", {
        indices,
        corpusVersion: myVersion,
        config: config,
        cleaningConfig: state.activeCleaningConfig,
      });

      if (myVersion !== state.currentCorpusVersion) return;

      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }

      const totalElapsed = (performance.now() - startTime) / 1000;
      dom.lblScanTime.textContent = `Done in ${totalElapsed.toFixed(1)}s`;

      state.lastScanCandidates = report.candidates;

      renderDiagnostics(report.diagnostics);

      dom.artifactDetailsContent.innerHTML = `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">Select a candidate to inspect examples.</div>`;

      renderCandidates(report.candidates, report.diagnostics);
    } catch (err) {
      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }

      const errStr = String(err);
      if (errStr.includes("cancelled") || errStr.includes("Cancelled") || errStr.includes("cancel")) {
        dom.lblScanTime.textContent = "Cancelled";
        dom.tblArtifactCandidates.innerHTML = `
          <tr>
            <td colspan="8" style="padding: 20px; text-align: center; color: var(--text-muted);">Scan was cancelled.</td>
          </tr>
        `;
      } else {
        dom.lblScanTime.textContent = "Error";
        console.error(err);
        dom.tblArtifactCandidates.innerHTML = "";
        const errorRow = document.createElement("tr");
        const errorCell = document.createElement("td");
        errorCell.colSpan = 8;
        errorCell.style.cssText = "padding: 20px; text-align: center; color: #ff5e5e;";
        errorCell.textContent = `Error during scan: ${errStr}`;
        errorRow.appendChild(errorCell);
        dom.tblArtifactCandidates.appendChild(errorRow);
      }

    } finally {
      if (myGen !== scanGeneration) return;
      resetScanControls();
    }
  });

  radioModes.forEach(radio => {
    radio.addEventListener("change", updateProcessedWarning);
  });

  function renderCandidates(candidates: RepeatedArtifactCandidate[], diagnostics: RepeatedArtifactScanDiagnostics | null): void {
    state.selectedCandidateIds.clear();
    updateAddSelectedButtonState();

    dom.tblArtifactCandidates.innerHTML = "";
    dom.lblArtifactResultsCount.textContent = `${candidates.length} found`;

    if (candidates.length === 0) {
      let msg: string;

      if (diagnostics && diagnostics.total_raw_lines === 0) {
        msg = "No candidates found because no text lines were extracted. Check whether the documents are scanned/image-only PDFs or whether extraction failed.";
        if (diagnostics.files_failed_extraction > 0) {
          msg += ` Extraction failures were reported for ${diagnostics.files_failed_extraction} file(s).`;
        }
      } else if (diagnostics && diagnostics.total_candidate_keys_before_filtering > 0 && diagnostics.candidates_after_min_occurrences === 0) {
        msg = "Candidate keys were found, but none met Min Occurrences. Try lowering Min Occurrences.";
      } else if (diagnostics && diagnostics.candidates_after_min_occurrences > 0 && diagnostics.candidates_after_min_files === 0) {
        msg = "Candidates were found, but none appeared in enough distinct files. Try Min Files = 1 for repeated artefacts inside a single PDF/book.";
      } else {
        msg = "No candidates found meeting the thresholds.";
      }

      if (state.scanWasProcessed && state.removalCountAtScanStart > 0) {
        msg += " Processed scans apply current Custom Removals, so already-removed artefacts will not appear. Scan Original extracted text to rediscover raw artefacts.";
      }

      dom.tblArtifactCandidates.innerHTML = `
        <tr>
          <td colspan="8" style="padding: 20px; text-align: center; color: var(--text-muted);">${msg}</td>
        </tr>
      `;
      return;
    }

    candidates.forEach((cand) => {
      const tr = document.createElement("tr");
      tr.dataset.id = cand.candidate_id;

      const tdCheck = document.createElement("td");
      tdCheck.style.padding = "6px 8px";
      tdCheck.style.textAlign = "center";
      const chk = document.createElement("input");
      chk.type = "checkbox";
      const isNormalized = cand.kind === "normalized_line";
      const normalizedKey = (cand.normalized_key || "").trim();
      if (isNormalized && !normalizedKey && (!cand.raw_variants || cand.raw_variants.length === 0)) {
        chk.disabled = true;
        chk.title = "This grouped pattern has no normalised key or actionable raw variants to add.";
      } else if (cand.kind === "exact_line") {
        chk.title = "Selecting adds a structured whole-line removal rule.";
      } else if (isNormalized && normalizedKey) {
        chk.title = "Selecting adds a structured normalised whole-line removal rule.";
      } else if (isNormalized) {
        const cappedNote = cand.raw_variant_count_is_capped
          ? " This fallback group is capped; additional untracked variants may remain after literal removal."
          : "";
        chk.title = `Selecting falls back to adding the ${cand.raw_variants.length} tracked raw variant${cand.raw_variants.length === 1 ? "" : "s"} to Custom Removals.${cappedNote}`;
      }
      chk.addEventListener("click", (e) => {
        e.stopPropagation();
        if (chk.checked) {
          state.selectedCandidateIds.add(cand.candidate_id);
        } else {
          state.selectedCandidateIds.delete(cand.candidate_id);
        }
        updateAddSelectedButtonState();
      });
      tdCheck.appendChild(chk);

      const tdText = document.createElement("td");
      tdText.className = "candidate-text-cell";
      tdText.textContent = cand.normalized_key && cand.kind === "normalized_line"
        ? cand.normalized_key
        : cand.display_text;
      tdText.title = cand.display_text;

      const tdKind = document.createElement("td");
      tdKind.textContent = formatKind(cand.kind);

      const tdContent = document.createElement("td");
      tdContent.textContent = formatContentClass(cand.content_class);
      tdContent.style.fontSize = "0.8rem";

      const tdOcc = document.createElement("td");
      tdOcc.textContent = cand.occurrence_count.toString();
      tdOcc.style.textAlign = "right";

      const tdFiles = document.createElement("td");
      tdFiles.textContent = cand.file_count.toString();
      tdFiles.style.textAlign = "right";

      const tdPos = document.createElement("td");
      tdPos.textContent = formatPosition(cand.position_summary);
      tdPos.style.fontSize = "0.8rem";

      const tdRisk = document.createElement("td");
      const spanBadge = document.createElement("span");
      spanBadge.className = `risk-badge ${cand.risk_label}`;
      spanBadge.textContent = formatRiskLabel(cand.risk_label);
      tdRisk.appendChild(spanBadge);

      tr.appendChild(tdCheck);
      tr.appendChild(tdText);
      tr.appendChild(tdKind);
      tr.appendChild(tdContent);
      tr.appendChild(tdOcc);
      tr.appendChild(tdFiles);
      tr.appendChild(tdPos);
      tr.appendChild(tdRisk);

      tr.onclick = () => {
        dom.tblArtifactCandidates.querySelectorAll("tr").forEach((r) => r.classList.remove("selected"));
        tr.classList.add("selected");
        dom.artifactDetailsContent.innerHTML = "";
        showCandidateDetails(cand);
      };

      dom.tblArtifactCandidates.appendChild(tr);
    });

    const noteContainer = dom.tblArtifactCandidates.closest(".repeated-artifact-table-container");
    if (noteContainer) {
      const existingNote = noteContainer.querySelector(".artifact-dedup-note");
      if (existingNote) existingNote.remove();
    }
  }

  function formatKind(kind: string): string {
    switch (kind) {
      case "exact_line": return "Exact";
      case "normalized_line": return "Pattern";
      case "two_line_block": return "2-Block";
      case "three_line_block": return "3-Block";
      case "inline_artifact": return "Inline";
      default: return kind;
    }
  }

  function formatContentClass(cls: string): string {
    switch (cls) {
      case "text_dominant": return "Text";
      case "mixed_text_numbers": return "Mixed";
      case "numeric_dominant": return "Numeric";
      case "symbol_noise_dominant": return "Symbol";
      default: return cls;
    }
  }

  function formatPosition(ps: PositionSummary): string {
    const total = ps.top_count + ps.middle_count + ps.bottom_count;
    if (total === 0) return "Unknown";
    const topPct = Math.round((ps.top_count / total) * 100);
    const botPct = Math.round((ps.bottom_count / total) * 100);
    return `${topPct}% / ${botPct}%`;
  }

  function formatRiskLabel(label: string): string {
    switch (label) {
      case "strong_header_footer_candidate": return "Header/Footer";
      case "possible_boilerplate": return "Boilerplate";
      case "common_section_heading_review_carefully": return "Heading";
      case "symbol_or_noise_candidate": return "Noise";
      case "ambiguous": return "Review";
      default: return label;
    }
  }

  function showCandidateDetails(cand: RepeatedArtifactCandidate): void {
    dom.artifactDetailsContent.innerHTML = "";

    const isLiteral = cand.kind === "exact_line" ||
      cand.kind === "inline_artifact" ||
      cand.kind === "two_line_block" ||
      cand.kind === "three_line_block";

    const isNormalized = cand.kind === "normalized_line";
    const normalizedKey = (cand.normalized_key || "").trim();

    const metaDiv = document.createElement("div");
    metaDiv.style.display = "flex";
    metaDiv.style.flexWrap = "wrap";
    metaDiv.style.gap = "12px";
    metaDiv.style.fontSize = "0.85rem";
    metaDiv.style.padding = "8px";
    metaDiv.style.background = "var(--bg-color)";
    metaDiv.style.borderRadius = "4px";
    metaDiv.style.border = "1px solid var(--border-color)";

    const metaParts = [
      `<span><strong>Kind:</strong> ${formatKind(cand.kind)}</span>`,
      `<span><strong>Content:</strong> ${formatContentClass(cand.content_class)}</span>`,
      `<span><strong>Occurrences:</strong> ${cand.occurrence_count}</span>`,
      `<span><strong>Files:</strong> ${cand.file_count}</span>`,
    ];
    if (isNormalized) {
      const cappedNote = cand.raw_variant_count_is_capped
        ? " (capped; actual count may be higher)"
        : "";
      metaParts.push(`<span><strong>Raw variants:</strong> ${cand.raw_variant_count} tracked${cappedNote}</span>`);
    }
    metaParts.push(`<span><strong>Risk:</strong> ${formatRiskLabel(cand.risk_label)}</span>`);

    metaDiv.innerHTML = metaParts.join("");
    dom.artifactDetailsContent.appendChild(metaDiv);

    if (cand.content_class === "numeric_dominant") {
      const numericCaution = document.createElement("div");
      numericCaution.style.fontSize = "0.8rem";
      numericCaution.style.color = "#e8a000";
      numericCaution.style.background = "rgba(232, 160, 0, 0.08)";
      numericCaution.style.padding = "6px 8px";
      numericCaution.style.borderRadius = "4px";
      numericCaution.style.borderLeft = "3px solid #e8a000";
      numericCaution.textContent = "Numeric-dominant candidate — review carefully. These may group unrelated tables, formulas, axis ticks, or statistical output.";
      dom.artifactDetailsContent.appendChild(numericCaution);
    }

    if (isLiteral) {
      const dispBlock = document.createElement("div");
      dispBlock.className = "candidate-display-block";
      const dispHeader = document.createElement("div");
      dispHeader.className = "candidate-display-header";
      dispHeader.textContent = "Candidate";
      const dispText = document.createElement("div");
      dispText.className = "candidate-display-text";
      dispText.textContent = cand.display_text;
      dispBlock.appendChild(dispHeader);
      dispBlock.appendChild(dispText);
      dom.artifactDetailsContent.appendChild(dispBlock);

      const copyBtn = document.createElement("button");
      copyBtn.className = "btn-copy-candidate";
      copyBtn.textContent = "Copy Candidate";
      copyBtn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(cand.display_text);
          copyBtn.textContent = "Copied";
          setTimeout(() => { copyBtn.textContent = "Copy Candidate"; }, 2000);
        } catch (_) {
          copyBtn.textContent = "Copy failed";
        }
      });
      dom.artifactDetailsContent.appendChild(copyBtn);
    }

    if (isNormalized) {
      const dispBlock = document.createElement("div");
      dispBlock.className = "candidate-display-block";
      const dispHeader = document.createElement("div");
      dispHeader.className = "candidate-display-header";
      dispHeader.textContent = "Normalised grouping key";
      const dispText = document.createElement("div");
      dispText.className = "candidate-display-text";
      dispText.textContent = normalizedKey || cand.display_text;
      dispBlock.appendChild(dispHeader);
      dispBlock.appendChild(dispText);
      dom.artifactDetailsContent.appendChild(dispBlock);

      const normNote = document.createElement("div");
      normNote.className = "detail-note detail-note-info";
      normNote.textContent = normalizedKey
        ? "Selecting this candidate creates one normalised whole-line rule. Lines are removed only when their normalised whole-line form matches this key."
        : "This candidate has no normalised key. Selecting it falls back to tracked raw variants when they are available.";
      dom.artifactDetailsContent.appendChild(normNote);

      if (cand.raw_variants && cand.raw_variants.length > 0) {
        const variantsActionDiv = document.createElement("div");
        variantsActionDiv.style.fontSize = "0.85rem";
        variantsActionDiv.style.padding = "8px";
        variantsActionDiv.style.background = "rgba(80, 200, 120, 0.06)";
        variantsActionDiv.style.borderRadius = "4px";
        variantsActionDiv.style.borderLeft = "3px solid #50c878";

        let variantsLabel = normalizedKey
          ? `Tracked raw variants are shown for review. Selecting this candidate adds one normalised whole-line rule for "${normalizedKey}"`
          : `Selecting this candidate falls back to the ${cand.raw_variants.length} tracked raw variant${cand.raw_variants.length === 1 ? "" : "s"}`;
        if (cand.raw_variant_count_is_capped) {
          variantsLabel += ", and this diagnostic list is capped";
        }
        variantsLabel += ".";
        const variantsLabelP = document.createElement("div");
        variantsLabelP.textContent = variantsLabel;
        variantsLabelP.style.marginBottom = "6px";
        variantsActionDiv.appendChild(variantsLabelP);

        const toggleBtn = document.createElement("button");
        toggleBtn.textContent = "Show raw variants";
        toggleBtn.style.cssText = "font-size: 0.8rem; padding: 3px 10px; border-radius: 3px; cursor: pointer; background: var(--bg-secondary); color: var(--text-color); border: 1px solid var(--border-color);";
        variantsActionDiv.appendChild(toggleBtn);

        const variantsList = document.createElement("div");
        variantsList.style.display = "none";
        variantsList.style.marginTop = "6px";
        variantsList.style.maxHeight = "200px";
        variantsList.style.overflowY = "auto";
        variantsList.style.fontFamily = "monospace";
        variantsList.style.fontSize = "0.8rem";
        variantsList.style.lineHeight = "1.5";

        for (const variant of cand.raw_variants) {
          const varLine = document.createElement("div");
          varLine.style.padding = "2px 4px";
          varLine.style.borderBottom = "1px solid var(--border-color)";
          varLine.style.whiteSpace = "pre-wrap";
          varLine.style.wordBreak = "break-all";
          varLine.textContent = variant;
          variantsList.appendChild(varLine);
        }
        variantsActionDiv.appendChild(variantsList);

        toggleBtn.addEventListener("click", () => {
          if (variantsList.style.display === "none") {
            variantsList.style.display = "block";
            toggleBtn.textContent = "Hide raw variants";
          } else {
            variantsList.style.display = "none";
            toggleBtn.textContent = "Show raw variants";
          }
        });

        dom.artifactDetailsContent.appendChild(variantsActionDiv);
      } else {
        const noVariantsNote = document.createElement("div");
        noVariantsNote.className = normalizedKey ? "detail-note detail-note-info" : "detail-note detail-note-warning";
        noVariantsNote.textContent = normalizedKey
          ? "No raw variants were tracked for this candidate. The normalised whole-line rule can still use the grouping key."
          : "No raw variants were tracked for this candidate, and it has no normalised key to add as a rule.";
        dom.artifactDetailsContent.appendChild(noVariantsNote);
      }
    }

    if (cand.position_summary) {
      const posSummaryDiv = document.createElement("div");
      posSummaryDiv.style.fontSize = "0.85rem";
      posSummaryDiv.style.display = "flex";
      posSummaryDiv.style.flexDirection = "column";
      posSummaryDiv.style.gap = "4px";
      posSummaryDiv.style.background = "var(--bg-color)";
      posSummaryDiv.style.padding = "8px";
      posSummaryDiv.style.borderRadius = "4px";
      posSummaryDiv.style.border = "1px solid var(--border-color)";

      const total = cand.position_summary.top_count + cand.position_summary.middle_count + cand.position_summary.bottom_count;
      const topPct = total > 0 ? Math.round((cand.position_summary.top_count / total) * 100) : 0;
      const midPct = total > 0 ? Math.round((cand.position_summary.middle_count / total) * 100) : 0;
      const botPct = total > 0 ? Math.round((cand.position_summary.bottom_count / total) * 100) : 0;

      posSummaryDiv.innerHTML = `
        <strong>Estimated Layout Positions (Approximate):</strong>
        <div style="display: flex; justify-content: space-between; margin-top: 4px;">
          <span>Top: ${cand.position_summary.top_count} (${topPct}%)</span>
          <span>Body: ${cand.position_summary.middle_count} (${midPct}%)</span>
          <span>Bottom: ${cand.position_summary.bottom_count} (${botPct}%)</span>
        </div>
      `;
      dom.artifactDetailsContent.appendChild(posSummaryDiv);
    }

    const riskAdvisoryDiv = document.createElement("div");
    riskAdvisoryDiv.style.fontSize = "0.8rem";
    riskAdvisoryDiv.style.color = "var(--text-muted)";
    riskAdvisoryDiv.style.lineHeight = "1.4";
    riskAdvisoryDiv.style.padding = "6px 8px";
    riskAdvisoryDiv.style.background = "rgba(255, 255, 255, 0.02)";
    riskAdvisoryDiv.style.borderLeft = "2px solid var(--border-color)";

    let advisoryText = "";
    switch (cand.risk_label) {
      case "strong_header_footer_candidate":
        advisoryText = "Notice (Advisory Only): This sequence is heavily concentrated at page headers or footers. It is a very strong candidate for cleanup.";
        break;
      case "possible_boilerplate":
        advisoryText = "Notice (Advisory Only): This sequence occurs across multiple documents in similar forms, suggesting boilerplate (e.g. copyright notices or publisher headers).";
        break;
      case "common_section_heading_review_carefully":
        advisoryText = "Notice (Advisory Only): This matches common academic sections (e.g. Introduction). Review carefully as removing this may alter document structure.";
        break;
      case "symbol_or_noise_candidate":
        advisoryText = "Notice (Advisory Only): This contains a high proportion of non-alphanumeric symbols. It is highly likely to be extraction noise or page dividers.";
        break;
      default:
        advisoryText = "Notice (Advisory Only): Classification is ambiguous. Review occurrences below to check if it represents noise or content.";
    }
    riskAdvisoryDiv.textContent = advisoryText;
    dom.artifactDetailsContent.appendChild(riskAdvisoryDiv);

    if (cand.examples && cand.examples.length > 0) {
      const examplesTitle = document.createElement("strong");
      examplesTitle.style.fontSize = "0.9rem";
      examplesTitle.style.marginTop = "8px";
      if (cand.occurrence_count > cand.examples.length) {
        examplesTitle.textContent = `Sample Occurrences (showing ${cand.examples.length} of ${cand.occurrence_count})`;
      } else {
        examplesTitle.textContent = `Sample Occurrences (showing all ${cand.examples.length})`;
      }

      const examplesListDiv = document.createElement("div");
      examplesListDiv.style.display = "flex";
      examplesListDiv.style.flexDirection = "column";
      examplesListDiv.style.gap = "8px";

      cand.examples.forEach((ex, idx) => {
        const card = document.createElement("div");
        card.className = "artifact-example-card";

        const cardHeader = document.createElement("div");
        cardHeader.className = "artifact-example-header";

        const docSpan = document.createElement("span");
        docSpan.textContent = `Instance ${idx + 1}: ${ex.file_name}`;
        docSpan.style.fontWeight = "600";
        docSpan.style.overflow = "hidden";
        docSpan.style.textOverflow = "ellipsis";
        docSpan.title = ex.file_path;

        const posSpan = document.createElement("span");
        const pageStr = ex.page_number !== null && ex.page_number !== undefined ? `Page ~${ex.page_number} (Approx)` : "";
        const lineStr = ex.line_number !== null && ex.line_number !== undefined ? `Line ${ex.line_number}` : "";
        posSpan.textContent = [pageStr, lineStr].filter(Boolean).join(", ");

        cardHeader.appendChild(docSpan);
        cardHeader.appendChild(posSpan);

        const cardBody = document.createElement("div");
        cardBody.className = "artifact-example-body";

        if (ex.context_before) {
          const lineBefore = document.createElement("div");
          lineBefore.className = "artifact-context-line";
          lineBefore.textContent = ex.context_before;
          cardBody.appendChild(lineBefore);
        }

        const lineMatched = document.createElement("div");
        lineMatched.className = "artifact-matched-line";
        lineMatched.textContent = ex.matched_text;
        cardBody.appendChild(lineMatched);

        if (ex.context_after) {
          const lineAfter = document.createElement("div");
          lineAfter.className = "artifact-context-line";
          lineAfter.textContent = ex.context_after;
          cardBody.appendChild(lineAfter);
        }

        card.appendChild(cardHeader);
        card.appendChild(cardBody);
        examplesListDiv.appendChild(card);
      });

      dom.artifactDetailsContent.appendChild(examplesTitle);
      dom.artifactDetailsContent.appendChild(examplesListDiv);
    }
  }
}
