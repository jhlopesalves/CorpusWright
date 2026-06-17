import { invoke } from "@tauri-apps/api/core";

import {
  dom,
  getArtifactTextModeRadios,
  getSelectedArtifactTextMode,
} from "./dom";
import { state } from "./state";
import type { RemovalRule } from "./generated/CleaningConfig.js";
import type { CandidateTextProfile } from "./generated/CandidateTextProfile.js";
import type { CandidateTextSignalLabel } from "./generated/CandidateTextSignalLabel.js";
import type { PositionSummary } from "./generated/PositionSummary.js";
import type { RepeatedArtifactCandidate } from "./generated/RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanConfig } from "./generated/RepeatedArtifactScanConfig.js";
import type { RepeatedArtifactScanDiagnostics } from "./generated/RepeatedArtifactScanDiagnostics.js";
import type { RepeatedArtifactScanReport } from "./generated/RepeatedArtifactScanReport.js";
import type { RemovalScope } from "./generated/RemovalScope.js";
import {
  formatRemovalRuleText,
  syncRepeatedArtifactRulesSummary,
  syncDraftCustomRemovalsFromConfig,
} from "./custom-removals.js";

interface RepeatedArtifactCallbacks {
  renderCustomRemovals: () => void;
  schedulePreviewUpdate: (delay: number) => void;
}

interface RepeatedArtifactScanScope {
  canScan: boolean;
  indices: number[];
  buttonLabel: string;
  helperText: string;
  modalText: string;
}

const RISK_RANK: Record<string, number> = {
  strong_header_footer_candidate: 5,
  symbol_or_noise_candidate: 4,
  possible_boilerplate: 3,
  common_section_heading_review_carefully: 2,
  ambiguous: 1,
};

function formatKind(kind: string): string {
  switch (kind) {
    case "exact_line":
      return "Exact";
    case "normalized_line":
      return "Pattern";
    case "two_line_block":
      return "2-Block";
    case "three_line_block":
      return "3-Block";
    case "inline_artifact":
      return "Inline";
    default:
      return kind;
  }
}

function formatContentClass(cls: string): string {
  switch (cls) {
    case "text_dominant":
      return "Text";
    case "mixed_text_numbers":
      return "Mixed";
    case "numeric_dominant":
      return "Numeric";
    case "symbol_noise_dominant":
      return "Symbol";
    default:
      return cls;
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
    case "strong_header_footer_candidate":
      return "Header/Footer";
    case "possible_boilerplate":
      return "Boilerplate";
    case "common_section_heading_review_carefully":
      return "Heading";
    case "symbol_or_noise_candidate":
      return "Noise";
    case "ambiguous":
      return "Review";
    default:
      return label;
  }
}

function getCandidateText(cand: RepeatedArtifactCandidate): string {
  return cand.normalized_key && cand.kind === "normalized_line"
    ? cand.normalized_key
    : cand.display_text;
}

function compareStrings(a: string, b: string): number {
  return a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
}

function getPositionScores(cand: RepeatedArtifactCandidate) {
  const ps = cand.position_summary;
  const total = ps.top_count + ps.middle_count + ps.bottom_count;
  if (total === 0) {
    return { primary: 0, secondary: 0, tertiary: cand.occurrence_count };
  }
  const top_ratio = ps.top_count / total;
  const bottom_ratio = ps.bottom_count / total;

  return {
    primary: Math.max(top_ratio, bottom_ratio),
    secondary: top_ratio + bottom_ratio,
    tertiary: cand.occurrence_count,
  };
}

function comparePosition(
  a: RepeatedArtifactCandidate,
  b: RepeatedArtifactCandidate
): number {
  const scoreA = getPositionScores(a);
  const scoreB = getPositionScores(b);

  if (scoreA.primary !== scoreB.primary) {
    return scoreA.primary - scoreB.primary;
  }
  if (scoreA.secondary !== scoreB.secondary) {
    return scoreA.secondary - scoreB.secondary;
  }
  return scoreA.tertiary - scoreB.tertiary;
}

function compareRisk(
  a: RepeatedArtifactCandidate,
  b: RepeatedArtifactCandidate
): number {
  const rankA = RISK_RANK[a.risk_label] || 0;
  const rankB = RISK_RANK[b.risk_label] || 0;
  return rankA - rankB;
}

function compareCandidates(
  a: RepeatedArtifactCandidate,
  b: RepeatedArtifactCandidate,
  column: string
): number {
  switch (column) {
    case "text": {
      const textA = getCandidateText(a);
      const textB = getCandidateText(b);
      return compareStrings(textA, textB);
    }
    case "kind": {
      return compareStrings(formatKind(a.kind), formatKind(b.kind));
    }
    case "content": {
      return compareStrings(
        formatContentClass(a.content_class),
        formatContentClass(b.content_class)
      );
    }
    case "occs": {
      return a.occurrence_count - b.occurrence_count;
    }
    case "files": {
      return a.file_count - b.file_count;
    }
    case "position": {
      return comparePosition(a, b);
    }
    case "risk": {
      return compareRisk(a, b);
    }
    default:
      return 0;
  }
}

function ruleDisplayText(rule: RemovalRule): string {
  return formatRemovalRuleText(rule);
}

function activeCustomRemovalCount(): number {
  return (
    state.activeCleaningConfig.remove_patterns.length +
    state.activeCleaningConfig.removal_rules.filter((rule) => rule.enabled)
      .length
  );
}

function sourceCountLabel(count: number, qualifier: string): string {
  return `${count.toLocaleString()} ${qualifier} source${count === 1 ? "" : "s"}`;
}

function allSourceScopeLabel(count: number): string {
  const qualifier = state.activeProject ? "loaded project" : "loaded";
  return count === 1
    ? `the only ${qualifier} source`
    : `all ${count.toLocaleString()} ${qualifier} sources`;
}

function selectedScanIndices(): number[] {
  return Array.from(state.selectedCorpusIndices)
    .filter((index) => state.allFiles[index] !== undefined)
    .sort((a, b) => a - b);
}

export function getRepeatedArtifactScanScope(): RepeatedArtifactScanScope {
  const loadedCount = state.allFiles.length;
  if (loadedCount === 0) {
    const helperText =
      state.activeProject && state.projectSources.length > 0
        ? "This project has no processable source files to scan."
        : "Add sources before scanning for repeated artefacts.";
    return {
      canScan: false,
      indices: [],
      buttonLabel: "Find repeated artefacts...",
      helperText,
      modalText: helperText,
    };
  }

  const hasSelection = state.activeProject
    ? state.selectedProjectSourceIds.size > 0
    : state.selectedCorpusIndices.size > 0;
  if (hasSelection) {
    const selected = selectedScanIndices();
    if (selected.length === 0) {
      return {
        canScan: false,
        indices: [],
        buttonLabel: "Find repeated artefacts...",
        helperText: "Selected sources cannot be scanned yet.",
        modalText: "Selected sources cannot be scanned yet.",
      };
    }

    const selectedLabel = sourceCountLabel(selected.length, "selected");
    return {
      canScan: true,
      indices: selected,
      buttonLabel: `Find repeated artefacts in ${selectedLabel}...`,
      helperText: "The scan will use the current Source Explorer selection.",
      modalText: `Scope: ${selectedLabel}.`,
    };
  }

  const allIndices = Array.from(
    { length: loadedCount },
    (_, corpusIndex) => corpusIndex
  );
  const scopeLabel = allSourceScopeLabel(loadedCount);
  return {
    canScan: true,
    indices: allIndices,
    buttonLabel: `Find repeated artefacts in ${scopeLabel}...`,
    helperText: `No sources are selected, so the scan will use ${scopeLabel}.`,
    modalText: `Scope: ${scopeLabel}.`,
  };
}

export function syncRepeatedArtifactSettingsSection(): RepeatedArtifactScanScope {
  const scope = getRepeatedArtifactScanScope();
  dom.btnFindRepeatedArtifactsSettings.textContent = scope.buttonLabel;
  dom.btnFindRepeatedArtifactsSettings.disabled = !scope.canScan;
  dom.repeatedArtifactScopeHelp.textContent = scope.helperText;
  syncRepeatedArtifactRulesSummary();
  return scope;
}

function exactLineRuleId(
  candidate: RepeatedArtifactCandidate,
  scope: RemovalScope
): string {
  return `repeated-artifact:${scope}:${candidate.candidate_id}`;
}

function normalizedLineRuleId(
  candidate: RepeatedArtifactCandidate,
  scope: RemovalScope
): string {
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

function exactLineRuleDuplicateKey(
  text: string,
  scope: RemovalScope
): string {
  return `${scope}\0${text}`;
}

function normalizedLineRuleDuplicateKey(
  normalizedKey: string,
  scope: RemovalScope
): string {
  return `${scope}\0normalized_line\0${normalizedKey}`;
}

function computeCandidateScope(
  candidate: RepeatedArtifactCandidate
): RemovalScope {
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

function createExactLineRemovalRule(
  candidate: RepeatedArtifactCandidate,
  scope: RemovalScope
): RemovalRule {
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

function createNormalizedLineRemovalRule(
  candidate: RepeatedArtifactCandidate,
  normalizedKey: string,
  scope: RemovalScope
): RemovalRule {
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

interface DecisionEntry {
  decision: "accepted" | "kept" | "ignored";
  candidate_signature: string;
  candidate_text: string;
  scope: RemovalScope;
  matcher_kind: string;
  rule_id: string;
  updated_at: string;
}

interface ProjectDecisions {
  version: number;
  decisions: Record<string, DecisionEntry>;
}

export let projectDecisions: ProjectDecisions = { version: 1, decisions: {} };

async function loadProjectDecisions(
  rootPath: string
): Promise<ProjectDecisions> {
  try {
    const raw = await invoke<string>(
      "load_repeated_artifact_decisions_command",
      { rootPath }
    );
    const parsed = JSON.parse(raw);
    if (
      !parsed ||
      typeof parsed !== "object" ||
      parsed.version === undefined
    ) {
      return { version: 1, decisions: {} };
    }
    return parsed as ProjectDecisions;
  } catch (err) {
    console.error("Failed to load decisions:", err);
    return { version: 1, decisions: {} };
  }
}

async function saveProjectDecisions(
  rootPath: string,
  decisions: ProjectDecisions
): Promise<void> {
  try {
    await invoke("save_repeated_artifact_decisions_command", {
      rootPath,
      content: JSON.stringify(decisions, null, 2),
    });
  } catch (err) {
    console.error("Failed to save decisions:", err);
  }
}

export async function loadDecisionsForProject(
  rootPath: string
): Promise<void> {
  projectDecisions = await loadProjectDecisions(rootPath);
  syncAcceptedRulesIntoConfig();
}

export function clearDecisions(): void {
  projectDecisions = { version: 1, decisions: {} };
}

export function syncAcceptedRulesIntoConfig(): void {
  if (!state.activeProject) return;

  const existingRuleIds = new Set(
    state.activeCleaningConfig.removal_rules.map((rule) => rule.id)
  );
  const existingRuleKeys = new Set(
    state.activeCleaningConfig.removal_rules.map(removalRuleDuplicateKey)
  );

  let addedCount = 0;

  for (const [candId, entry] of Object.entries(
    projectDecisions.decisions
  )) {
    if (entry.decision !== "accepted") continue;

    const ruleId =
      entry.rule_id || `repeated-artifact:${entry.scope}:${candId}`;
    const duplicateKey =
      entry.matcher_kind === "normalized_line"
        ? normalizedLineRuleDuplicateKey(entry.candidate_text, entry.scope)
        : exactLineRuleDuplicateKey(entry.candidate_text, entry.scope);

    if (existingRuleIds.has(ruleId) || existingRuleKeys.has(duplicateKey)) {
      continue;
    }

    const rule: RemovalRule = {
      id: ruleId,
      label: formatRemovalRuleText({
        id: "",
        label: "",
        source: "promoted_repeated_artifact",
        matcher:
          entry.matcher_kind === "normalized_line"
            ? { kind: "normalized_line", normalized_key: entry.candidate_text }
            : { kind: "literal", text: entry.candidate_text },
        scope: entry.scope,
        enabled: true,
      }),
      source: "promoted_repeated_artifact",
      matcher:
        entry.matcher_kind === "normalized_line"
          ? { kind: "normalized_line", normalized_key: entry.candidate_text }
          : { kind: "literal", text: entry.candidate_text },
      scope: entry.scope,
      enabled: true,
    };

    state.activeCleaningConfig.removal_rules.push(rule);
    existingRuleIds.add(ruleId);
    existingRuleKeys.add(duplicateKey);
    addedCount++;
  }

  if (addedCount > 0) {
    syncDraftCustomRemovalsFromConfig(state.activeCleaningConfig);
  }
}

export function syncDecisionsWithActiveRules(): void {
  if (!state.activeProject) return;

  const activeRuleIds = new Set(
    state.activeCleaningConfig.removal_rules.map((rule) => rule.id)
  );
  let modified = false;

  for (const [candId, entry] of Object.entries(
    projectDecisions.decisions
  )) {
    if (entry.decision === "accepted") {
      const ruleId =
        entry.rule_id || `repeated-artifact:${entry.scope}:${candId}`;
      if (!activeRuleIds.has(ruleId)) {
        delete projectDecisions.decisions[candId];
        modified = true;
      }
    }
  }

  if (modified) {
    saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }
}

function removeRulesForCandidate(candidateId: string): void {
  const suffix = ":" + candidateId;
  state.activeCleaningConfig.removal_rules =
    state.activeCleaningConfig.removal_rules.filter(
      (rule) => !rule.id.endsWith(suffix)
    );
  state.tempRemovalRules = state.tempRemovalRules.filter(
    (rule) => !rule.id.endsWith(suffix)
  );
}

function addRuleForCandidate(
  cand: RepeatedArtifactCandidate,
  scope: RemovalScope
): RemovalRule {
  const rule =
    cand.kind === "normalized_line"
      ? createNormalizedLineRemovalRule(
          cand,
          (cand.normalized_key || "").trim(),
          scope
        )
      : createExactLineRemovalRule(cand, scope);

  const ruleKey = removalRuleDuplicateKey(rule);
  const existingRuleIds = new Set(
    state.activeCleaningConfig.removal_rules.map((r) => r.id)
  );
  const existingRuleKeys = new Set(
    state.activeCleaningConfig.removal_rules.map(removalRuleDuplicateKey)
  );

  if (!existingRuleIds.has(rule.id) && !existingRuleKeys.has(ruleKey)) {
    removeRulesForCandidate(cand.candidate_id);
    state.activeCleaningConfig.removal_rules.push(rule);
    if (dom.settingsModal.classList.contains("hidden")) {
      state.tempRemovalRules = state.activeCleaningConfig.removal_rules.map(
        (r) => ({
          ...r,
          matcher: { ...r.matcher },
        })
      );
    } else {
      const tempRuleIds = new Set(state.tempRemovalRules.map((r) => r.id));
      const tempRuleKeys = new Set(
        state.tempRemovalRules.map(removalRuleDuplicateKey)
      );
      const key = removalRuleDuplicateKey(rule);
      if (!tempRuleIds.has(rule.id) && !tempRuleKeys.has(key)) {
        state.tempRemovalRules.push({
          ...rule,
          matcher: { ...rule.matcher },
        });
      }
    }
  }
  return rule;
}

function getBucketForCandidate(
  cand: RepeatedArtifactCandidate,
  decision: string | undefined
): string {
  if (decision === "accepted") {
    return "accepted";
  }
  if (decision === "kept" || decision === "ignored") {
    return "ignored";
  }

  const risk = cand.risk_label;
  const signal = (cand as any).text_signal_label as string | undefined;

  const isSuggested =
    (risk === "strong_header_footer_candidate" ||
      risk === "symbol_or_noise_candidate" ||
      signal === "likely_markup_or_extraction_noise" ||
      signal === "likely_page_label") &&
    !(signal === "likely_natural_text" || signal === "likely_section_heading");

  if (isSuggested) {
    return "suggested";
  }

  const isKeep =
    signal === "likely_natural_text" || signal === "likely_section_heading";
  if (isKeep) {
    return "keep";
  }

  return "review";
}

/* ─── Bulk toolbar rendering ──────────────────────────────────────────── */

function getFilteredCandidatesForActiveBucket(
  candidates: RepeatedArtifactCandidate[],
  activeBucket: string
): RepeatedArtifactCandidate[] {
  return candidates.filter((cand) => {
    const decision = projectDecisions.decisions[cand.candidate_id]?.decision;
    return getBucketForCandidate(cand, decision) === activeBucket;
  });
}

function renderBulkToolbar(
  activeBucket: string,
  filteredCandidates: RepeatedArtifactCandidate[]
): void {
  const toolbar = document.getElementById(
    "repeated-artifact-batch-toolbar"
  ) as HTMLDivElement;
  if (!toolbar) return;

  const selectionCount = state.selectedCandidateIds.size;
  const btnAcceptAll = document.getElementById(
    "btn-accept-all-suggested"
  ) as HTMLButtonElement;
  const btnBulkAccept = document.getElementById(
    "btn-bulk-accept"
  ) as HTMLButtonElement;
  const btnBulkKeep = document.getElementById(
    "btn-bulk-keep"
  ) as HTMLButtonElement;
  const btnBulkIgnore = document.getElementById(
    "btn-bulk-ignore"
  ) as HTMLButtonElement;
  const btnBulkRevert = document.getElementById(
    "btn-bulk-revert"
  ) as HTMLButtonElement;
  const btnClearSel = document.getElementById(
    "btn-clear-selection"
  ) as HTMLButtonElement;
  const countSpan = document.getElementById(
    "bulk-selection-count"
  ) as HTMLSpanElement;

  if (selectionCount > 0) {
    // Show bulk toolbar, hide Accept all suggested
    toolbar.style.display = "flex";
    countSpan.textContent = `${selectionCount} selected`;
    btnClearSel.style.display = "inline-flex";

    // Determine which buttons to show based on active bucket
    const showAccept =
      activeBucket === "suggested" ||
      activeBucket === "review" ||
      activeBucket === "keep" ||
      activeBucket === "ignored";
    const showKeep =
      activeBucket === "suggested" ||
      activeBucket === "review" ||
      activeBucket === "keep";
    const showIgnore =
      activeBucket === "suggested" ||
      activeBucket === "review" ||
      activeBucket === "keep";
    const showRevert =
      activeBucket === "accepted" || activeBucket === "ignored";

    btnBulkAccept.style.display = showAccept ? "inline-flex" : "none";
    btnBulkKeep.style.display = showKeep ? "inline-flex" : "none";
    btnBulkIgnore.style.display = showIgnore ? "inline-flex" : "none";
    btnBulkRevert.style.display = showRevert ? "inline-flex" : "none";

    if (btnAcceptAll) btnAcceptAll.classList.add("hidden");
  } else {
    // Hide bulk toolbar unless Accept all suggested should show
    const showAcceptAll =
      activeBucket === "suggested" && filteredCandidates.length > 0;

    if (showAcceptAll && btnAcceptAll) {
      toolbar.style.display = "flex";
      btnAcceptAll.classList.remove("hidden");
    } else {
      toolbar.style.display = "none";
      if (btnAcceptAll) btnAcceptAll.classList.add("hidden");
    }

    btnBulkAccept.style.display = "none";
    btnBulkKeep.style.display = "none";
    btnBulkIgnore.style.display = "none";
    btnBulkRevert.style.display = "none";
    btnClearSel.style.display = "none";
    if (countSpan) countSpan.textContent = "";
  }
}

/* ─── Select all / Clear selection ────────────────────────────────────── */

function selectAllInTab(
  filteredCandidates: RepeatedArtifactCandidate[]
): void {
  state.selectedCandidateIds.clear();
  for (const cand of filteredCandidates) {
    state.selectedCandidateIds.add(cand.candidate_id);
  }
}

function clearSelection(): void {
  state.selectedCandidateIds.clear();
}

/* ─── Bulk action implementations ─────────────────────────────────────── */

async function bulkAcceptSelected(
  _activeBucket: string,
  callbacks: RepeatedArtifactCallbacks
): Promise<void> {
  const selectedIds = new Set(state.selectedCandidateIds);
  if (selectedIds.size === 0) return;

  const confirmed = window.confirm(
    `Accept ${selectedIds.size} selected candidates?\n\n` +
      `This will create ${selectedIds.size} cleaning rule(s).\n` +
      `Each candidate will use its current suggested scope.\n` +
      `Source files are never modified.\n` +
      `Processed output may change after applying cleaning.`
  );
  if (!confirmed) return;

  for (const id of selectedIds) {
    const cand = state.lastScanCandidates.find((c) => c.candidate_id === id);
    if (!cand) continue;

    const decision = projectDecisions.decisions[cand.candidate_id]?.decision;
    if (decision !== undefined && decision !== "accepted") {
      removeRulesForCandidate(cand.candidate_id);
    }

    const scope = computeCandidateScope(cand);
    const rule = addRuleForCandidate(cand, scope);

    projectDecisions.decisions[cand.candidate_id] = {
      decision: "accepted",
      candidate_signature: cand.candidate_id,
      candidate_text: getCandidateText(cand),
      scope,
      matcher_kind: cand.kind === "normalized_line" ? "normalized_line" : "literal",
      rule_id: rule.id,
      updated_at: new Date().toISOString(),
    };
  }

  if (state.activeProject) {
    await saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }

  callbacks.renderCustomRemovals();
  syncRepeatedArtifactSettingsSection();
  state.selectedCandidateIds.clear();
}

async function bulkKeepSelected(
  _activeBucket: string,
  callbacks: RepeatedArtifactCallbacks
): Promise<void> {
  const selectedIds = new Set(state.selectedCandidateIds);
  if (selectedIds.size === 0) return;

  const confirmed = window.confirm(
    `Mark ${selectedIds.size} candidates as legitimate text?\n\n` +
      `They will be hidden from active review.\n` +
      `No cleaning rules will be created.`
  );
  if (!confirmed) return;

  for (const id of selectedIds) {
    const cand = state.lastScanCandidates.find((c) => c.candidate_id === id);
    if (!cand) continue;
    removeRulesForCandidate(cand.candidate_id);
    projectDecisions.decisions[cand.candidate_id] = {
      decision: "kept",
      candidate_signature: cand.candidate_id,
      candidate_text: getCandidateText(cand),
      scope: computeCandidateScope(cand),
      matcher_kind: cand.kind === "normalized_line" ? "normalized_line" : "literal",
      rule_id: "",
      updated_at: new Date().toISOString(),
    };
  }

  callbacks.renderCustomRemovals();
  syncRepeatedArtifactSettingsSection();

  if (state.activeProject) {
    await saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }

  state.selectedCandidateIds.clear();
}

async function bulkIgnoreSelected(
  _activeBucket: string,
  callbacks: RepeatedArtifactCallbacks
): Promise<void> {
  const selectedIds = new Set(state.selectedCandidateIds);
  if (selectedIds.size === 0) return;

  const confirmed = window.confirm(
    `Ignore ${selectedIds.size} selected candidates?\n\n` +
      `They will be hidden from active review.\n` +
      `No cleaning rules will be created.`
  );
  if (!confirmed) return;

  for (const id of selectedIds) {
    const cand = state.lastScanCandidates.find((c) => c.candidate_id === id);
    if (!cand) continue;
    removeRulesForCandidate(cand.candidate_id);
    projectDecisions.decisions[cand.candidate_id] = {
      decision: "ignored",
      candidate_signature: cand.candidate_id,
      candidate_text: getCandidateText(cand),
      scope: computeCandidateScope(cand),
      matcher_kind: cand.kind === "normalized_line" ? "normalized_line" : "literal",
      rule_id: "",
      updated_at: new Date().toISOString(),
    };
  }

  callbacks.renderCustomRemovals();
  syncRepeatedArtifactSettingsSection();

  if (state.activeProject) {
    await saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }

  state.selectedCandidateIds.clear();
}

async function bulkRevertSelected(
  _activeBucket: string,
  callbacks: RepeatedArtifactCallbacks
): Promise<void> {
  const selectedIds = new Set(state.selectedCandidateIds);
  if (selectedIds.size === 0) return;

  const confirmed = window.confirm(
    `Revert ${selectedIds.size} selected candidates?\n\n` +
      `This will remove or disable their generated cleaning rules where applicable.\n` +
      `The candidates may appear again in future scans.`
  );
  if (!confirmed) return;

  for (const id of selectedIds) {
    const cand = state.lastScanCandidates.find((c) => c.candidate_id === id);
    if (cand) {
      removeRulesForCandidate(cand.candidate_id);
    }
    delete projectDecisions.decisions[id];
  }

  callbacks.renderCustomRemovals();
  syncRepeatedArtifactSettingsSection();

  if (state.activeProject) {
    await saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }

  state.selectedCandidateIds.clear();
}

/* ─── Update select-all header button label ───────────────────────────── */

function updateSelectAllCheckbox(
  filteredCandidates: RepeatedArtifactCandidate[]
): void {
  const chk = document.getElementById("chk-select-all-tab") as HTMLInputElement;
  if (!chk) return;
  const count = filteredCandidates.length;
  if (count === 0) {
    chk.checked = false;
    chk.indeterminate = false;
    chk.disabled = true;
    return;
  }
  chk.disabled = false;
  const selectedCount = [...state.selectedCandidateIds].filter((id) =>
    filteredCandidates.some((c) => c.candidate_id === id)
  ).length;
  if (selectedCount === 0) {
    chk.checked = false;
    chk.indeterminate = false;
  } else if (selectedCount === count) {
    chk.checked = true;
    chk.indeterminate = false;
  } else {
    chk.checked = false;
    chk.indeterminate = true;
  }
}

/* ─── MAIN INIT ───────────────────────────────────────────────────────── */

export function initRepeatedArtifactFinder(
  callbacks: RepeatedArtifactCallbacks
): void {
  const radioModes = getArtifactTextModeRadios();

  if (!dom.menuRepeatedArtifactFinder || !dom.repeatedArtifactModal) return;

  let scanGeneration = 0;
  let scanTimerInterval: ReturnType<typeof setInterval> | null = null;
  let currentAbortController: AbortController | null = null;
  let currentSortColumn: string | null = null;
  let currentSortDirection: "asc" | "desc" = "desc";
  let focusedCandidateId: string | null = null;
  let lastDiagnostics: RepeatedArtifactScanDiagnostics | null = null;
  let activeBucket = "suggested";

  let settingsExpanded = true;
  const configPanel = document.getElementById(
    "repeated-artifact-config-panel"
  );
  const arrowEl = document.getElementById("scan-settings-arrow");
  const btnToggleScanSettings = document.getElementById(
    "btn-toggle-scan-settings"
  );
  const summaryTextEl = document.getElementById(
    "repeated-artifact-summary-text"
  );

  function setSettingsExpanded(expanded: boolean) {
    settingsExpanded = expanded;
    if (!configPanel || !arrowEl) return;
    if (expanded) {
      configPanel.classList.remove("hidden");
      arrowEl.textContent = "▾";
    } else {
      configPanel.classList.add("hidden");
      arrowEl.textContent = "▸";
    }
  }

  btnToggleScanSettings?.addEventListener("click", () => {
    setSettingsExpanded(!settingsExpanded);
  });

  /* ─── Bucket tab click listeners ──────────────────────────────────── */

  const tabsContainer = document.getElementById("artifact-bucket-tabs");
  tabsContainer?.querySelectorAll(".tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      tabsContainer
        .querySelectorAll(".tab")
        .forEach((t) => t.classList.remove("active"));
      tab.classList.add("active");
      activeBucket = tab.getAttribute("data-bucket") || "suggested";
      // Clear selection on tab change
      state.selectedCandidateIds.clear();
      focusedCandidateId = null;
      if (state.lastScanCandidates.length > 0) {
        renderCandidates(state.lastScanCandidates, lastDiagnostics, false);
      }
    });
  });

  /* ─── Select all in tab ───────────────────────────────────────────── */

  const chkSelectAllTab = document.getElementById("chk-select-all-tab") as HTMLInputElement;
  chkSelectAllTab?.addEventListener("change", () => {
    if (state.lastScanCandidates.length === 0) return;
    const filtered = getFilteredCandidatesForActiveBucket(
      state.lastScanCandidates,
      activeBucket
    );
    if (chkSelectAllTab.checked) {
      selectAllInTab(filtered);
    } else {
      clearSelection();
    }
    renderCandidates(state.lastScanCandidates, lastDiagnostics, true);
  });

  /* ─── Clear selection ─────────────────────────────────────────────── */

  const btnClearSel = document.getElementById("btn-clear-selection");
  btnClearSel?.addEventListener("click", () => {
    clearSelection();
    renderCandidates(state.lastScanCandidates, lastDiagnostics, true);
  });

  /* ─── Bulk action buttons ─────────────────────────────────────────── */

  const btnBulkAccept = document.getElementById("btn-bulk-accept");
  btnBulkAccept?.addEventListener("click", async () => {
    await bulkAcceptSelected(activeBucket, callbacks);
    renderCandidates(state.lastScanCandidates, null, false);
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
      `Bulk accept completed.</div>`;
  });

  const btnBulkKeep = document.getElementById("btn-bulk-keep");
  btnBulkKeep?.addEventListener("click", async () => {
    await bulkKeepSelected(activeBucket, callbacks);
    renderCandidates(state.lastScanCandidates, null, false);
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
      `Candidates marked as kept.</div>`;
  });

  const btnBulkIgnore = document.getElementById("btn-bulk-ignore");
  btnBulkIgnore?.addEventListener("click", async () => {
    await bulkIgnoreSelected(activeBucket, callbacks);
    renderCandidates(state.lastScanCandidates, null, false);
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
      `Candidates ignored.</div>`;
  });

  const btnBulkRevert = document.getElementById("btn-bulk-revert");
  btnBulkRevert?.addEventListener("click", async () => {
    await bulkRevertSelected(activeBucket, callbacks);
    renderCandidates(state.lastScanCandidates, null, false);
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
      `Decisions reverted.</div>`;
  });

  /* ─── Accept All Suggested ────────────────────────────────────────── */

  const btnAcceptAllSuggested = document.getElementById(
    "btn-accept-all-suggested"
  );
  btnAcceptAllSuggested?.addEventListener("click", async () => {
    const suggestedCandidatesToAccept = state.lastScanCandidates.filter(
      (cand) => {
        const decision =
          projectDecisions.decisions[cand.candidate_id]?.decision;
        if (decision !== undefined) return false;
        if (getBucketForCandidate(cand, decision) !== "suggested") return false;
        const mid = cand.position_summary?.middle_count ?? 0;
        return mid === 0;
      }
    );

    if (suggestedCandidatesToAccept.length === 0) return;

    const confirmed = window.confirm(
      `Are you sure you want to accept all ${suggestedCandidatesToAccept.length} suggested candidates and create structured removal rules for them?`
    );
    if (!confirmed) return;

    for (const cand of suggestedCandidatesToAccept) {
      const chosenScope = computeCandidateScope(cand);

      const rule =
        cand.kind === "normalized_line"
          ? createNormalizedLineRemovalRule(
              cand,
              (cand.normalized_key || "").trim(),
              chosenScope
            )
          : createExactLineRemovalRule(cand, chosenScope);

      removeRulesForCandidate(cand.candidate_id);

      state.activeCleaningConfig.removal_rules.push(rule);
      if (dom.settingsModal.classList.contains("hidden")) {
        state.tempRemovalRules = state.activeCleaningConfig.removal_rules.map(
          (r) => ({
            ...r,
            matcher: { ...r.matcher },
          })
        );
      } else {
        const tempRuleIds = new Set(state.tempRemovalRules.map((r) => r.id));
        const tempRuleKeys = new Set(
          state.tempRemovalRules.map(removalRuleDuplicateKey)
        );
        const ruleKey = removalRuleDuplicateKey(rule);
        if (!tempRuleIds.has(rule.id) && !tempRuleKeys.has(ruleKey)) {
          state.tempRemovalRules.push({
            ...rule,
            matcher: { ...rule.matcher },
          });
        }
      }

      projectDecisions.decisions[cand.candidate_id] = {
        decision: "accepted",
        candidate_signature: cand.candidate_id,
        candidate_text: getCandidateText(cand),
        scope: chosenScope,
        matcher_kind:
          cand.kind === "normalized_line" ? "normalized_line" : "literal",
        rule_id: rule.id,
        updated_at: new Date().toISOString(),
      };
    }

    if (state.activeProject) {
      await saveProjectDecisions(
        state.activeProject.rootPath,
        projectDecisions
      );
    }

    callbacks.renderCustomRemovals();
    syncRepeatedArtifactSettingsSection();

    renderCandidates(state.lastScanCandidates, null, false);
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
      `Accepted all suggested candidates.</div>`;
  });

  /* ─── Processed warning ───────────────────────────────────────────── */

  function updateProcessedWarning(): void {
    const isProcessed = getSelectedArtifactTextMode()?.value === "processed";
    const count = activeCustomRemovalCount();
    if (isProcessed && count > 0) {
      const legacyRemovals = state.activeCleaningConfig.remove_patterns;
      const ruleRemovals = state.activeCleaningConfig.removal_rules.filter(
        (rule) => rule.enabled
      );
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

  /* ─── Scan status helpers ─────────────────────────────────────────── */

  function setStatus(stage: string, elapsed?: number): void {
    const timeStr = elapsed !== undefined ? ` (${elapsed.toFixed(1)}s)` : "";
    dom.tblArtifactCandidates.innerHTML = `
      <tr>
        <td colspan="6" style="padding: 20px; text-align: center;">
          <div class="spinner" style="margin: 0 auto 10px auto; width: 24px; height: 24px;"></div>
          ${stage}${timeStr}
        </td>
      </tr>
    `;
    dom.artifactDetailsContent.innerHTML =
      `<div style="color: var(--text-muted); text-align: center; margin-top: 40px;">${stage}...</div>`;
  }

  function resetScanControls(): void {
    dom.btnRunArtifactScan.style.display = "inline-block";
    dom.btnRunArtifactScan.disabled = false;
    dom.btnRunArtifactScan.textContent = "Run Scan";
    dom.btnCancelScan.style.display = "none";
    if (scanTimerInterval) {
      clearInterval(scanTimerInterval);
      scanTimerInterval = null;
    }
    syncRepeatedArtifactModalScope();
  }

  function syncRepeatedArtifactModalScope(): RepeatedArtifactScanScope {
    const scope = syncRepeatedArtifactSettingsSection();
    if (dom.btnRunArtifactScan.style.display !== "none") {
      dom.btnRunArtifactScan.disabled = !scope.canScan;
    }
    return scope;
  }

  function openRepeatedArtifactFinder(): void {
    syncRepeatedArtifactModalScope();
    dom.repeatedArtifactModal.classList.remove("hidden");
    updateProcessedWarning();
    if (state.lastScanCandidates && state.lastScanCandidates.length > 0) {
      setSettingsExpanded(false);
    } else {
      setSettingsExpanded(true);
    }
  }

  /* ─── Diagnostics / Summary ───────────────────────────────────────── */

  function renderDiagnostics(
    diags: RepeatedArtifactScanDiagnostics
  ): void {
    const parts: string[] = [];
    parts.push(`Files: ${diags.files_scanned}/${diags.files_requested} scanned`);
    parts.push(`Lines: ${diags.total_raw_lines.toLocaleString()}`);
    parts.push(
      `Candidate keys: ${diags.total_candidate_keys_before_filtering}`
    );
    parts.push(
      `After occurrence filter: ${diags.candidates_after_min_occurrences}`
    );
    parts.push(`After file filter: ${diags.candidates_after_min_files}`);
    parts.push(`Final: ${diags.final_candidates}`);

    const failureParts: string[] = [];
    if (diags.files_failed_extraction > 0) {
      failureParts.push(
        `Extraction failures: ${diags.files_failed_extraction}`
      );
    }
    if (diags.files_empty_after_extraction > 0) {
      failureParts.push(
        `Empty extractions: ${diags.files_empty_after_extraction}`
      );
    }

    let text = parts.join(" · ");
    if (failureParts.length > 0) {
      text += " · " + failureParts.join(" · ");
    }

    if (
      diags.analysed_processed_text &&
      diags.custom_removals_active > 0
    ) {
      text += ` · Processed scan applied ${diags.custom_removals_active} active Custom Removal item(s).`;
    }

    dom.artifactDiagnostics.textContent = text;
    dom.artifactDiagnostics.classList.remove("hidden");

    if (summaryTextEl) {
      const timeStr = dom.lblScanTime.textContent || "";
      const finalParts = [
        `${diags.files_scanned} files scanned`,
        `${diags.total_raw_lines.toLocaleString()} lines`,
        `${diags.final_candidates.toLocaleString()} candidates`,
      ];
      if (timeStr) {
        finalParts.push(timeStr);
      }
      summaryTextEl.textContent = finalParts.join(" · ");
    }
  }

  /* ─── Modal open/close ────────────────────────────────────────────── */

  dom.menuRepeatedArtifactFinder.addEventListener(
    "click",
    openRepeatedArtifactFinder
  );
  dom.btnFindRepeatedArtifactsSettings.addEventListener("click", () => {
    const scope = syncRepeatedArtifactSettingsSection();
    if (!scope.canScan) return;
    openRepeatedArtifactFinder();
  });

  function closeArtifactModal(): void {
    scanGeneration++;
    if (currentAbortController) {
      currentAbortController.abort();
      currentAbortController = null;
    }
    invoke("cancel_repeated_artifacts_command").catch(() => {});
    resetScanControls();
    dom.lblScanTime.style.display = "none";
    state.selectedCandidateIds.clear();
    focusedCandidateId = null;
    currentSortColumn = null;
    currentSortDirection = "desc";
    lastDiagnostics = null;
    updateSortIndicators();
    dom.artifactDiagnostics.classList.add("hidden");
    dom.repeatedArtifactModal.classList.add("hidden");
  }

  dom.btnCloseArtifactModal.addEventListener("click", closeArtifactModal);
  dom.btnCloseArtifactModalTop.addEventListener(
    "click",
    closeArtifactModal
  );

  /* ─── Sortable headers ────────────────────────────────────────────── */

  const tableElement = dom.tblArtifactCandidates.closest("table");
  const sortHeaders = tableElement?.querySelectorAll(".sort-header");
  sortHeaders?.forEach((header) => {
    header.addEventListener("click", () => {
      const column = header.getAttribute("data-column");
      if (!column) return;

      if (currentSortColumn === column) {
        currentSortDirection =
          currentSortDirection === "desc" ? "asc" : "desc";
      } else {
        currentSortColumn = column;
        currentSortDirection = "desc";
      }

      updateSortIndicators();

      if (state.lastScanCandidates.length > 0) {
        renderCandidates(state.lastScanCandidates, lastDiagnostics, true);
      }
    });
  });

  function updateSortIndicators(): void {
    const table = dom.tblArtifactCandidates.closest("table");
    if (!table) return;
    table.querySelectorAll("th").forEach((th) => {
      const header = th.querySelector(".sort-header");
      if (!header) return;
      const column = header.getAttribute("data-column");
      const indicator = header.querySelector(".sort-indicator");

      if (column === currentSortColumn) {
        th.setAttribute(
          "aria-sort",
          currentSortDirection === "desc" ? "descending" : "ascending"
        );
        if (indicator) {
          indicator.textContent =
            currentSortDirection === "desc" ? " ↓" : " ↑";
        }
      } else {
        th.removeAttribute("aria-sort");
        if (indicator) {
          indicator.textContent = "";
        }
      }
    });
  }

  /* ─── Scan execution ──────────────────────────────────────────────── */

  dom.btnRunArtifactScan.addEventListener("click", async () => {
    const scope = syncRepeatedArtifactModalScope();
    if (!scope.canScan) {
      alert(scope.helperText);
      return;
    }

    if (summaryTextEl) {
      summaryTextEl.textContent = "";
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
      min_occurrences:
        parseInt(dom.numArtifactMinOcc.value, 10) || 5,
      min_files: parseInt(dom.numArtifactMinFiles.value, 10) || 1,
      max_candidates:
        parseInt(dom.numArtifactMaxCand.value, 10) || 100,
      max_examples_per_candidate: Math.min(
        Math.max(
          parseInt(dom.numArtifactMaxExamples.value, 10) || 25,
          1
        ),
        100
      ),
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
    const indices = scope.indices;

    try {
      const report = await invoke<RepeatedArtifactScanReport>(
        "scan_repeated_artifacts_command",
        {
          indices,
          corpusVersion: myVersion,
          config: config,
          cleaningConfig: state.activeCleaningConfig,
        }
      );

      if (myVersion !== state.currentCorpusVersion) return;

      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) {
        clearInterval(scanTimerInterval);
        scanTimerInterval = null;
      }

      const totalElapsed = (performance.now() - startTime) / 1000;
      dom.lblScanTime.textContent = `Done in ${totalElapsed.toFixed(1)}s`;

      state.lastScanCandidates = report.candidates;

      renderDiagnostics(report.diagnostics);

      setSettingsExpanded(false);

      dom.artifactDetailsContent.innerHTML =
        `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
        `Select a candidate to inspect examples.</div>`;

      renderCandidates(report.candidates, report.diagnostics);
    } catch (err) {
      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) {
        clearInterval(scanTimerInterval);
        scanTimerInterval = null;
      }

      const errStr = String(err);
      if (
        errStr.includes("cancelled") ||
        errStr.includes("Cancelled") ||
        errStr.includes("cancel")
      ) {
        dom.lblScanTime.textContent = "Cancelled";
        dom.tblArtifactCandidates.innerHTML = `
          <tr>
            <td colspan="6" style="padding: 20px; text-align: center; color: var(--text-muted);">Scan was cancelled.</td>
          </tr>
        `;
      } else {
        dom.lblScanTime.textContent = "Error";
        console.error(err);
        dom.tblArtifactCandidates.innerHTML = "";
        const errorRow = document.createElement("tr");
        const errorCell = document.createElement("td");
        errorCell.colSpan = 6;
        errorCell.style.cssText =
          "padding: 20px; text-align: center; color: #ff5e5e;";
        errorCell.textContent = `Error during scan: ${errStr}`;
        errorRow.appendChild(errorCell);
        dom.tblArtifactCandidates.appendChild(errorRow);
      }
    } finally {
      if (myGen !== scanGeneration) return;
      resetScanControls();
    }
  });

  radioModes.forEach((radio) => {
    radio.addEventListener("change", updateProcessedWarning);
  });

  syncRepeatedArtifactSettingsSection();

  /* ─── Update bucket tab labels and Accept all visibility ──────────── */

  function updateBucketTabs(
    candidates: RepeatedArtifactCandidate[]
  ): void {
    const counts: Record<string, number> = {
      suggested: 0,
      review: 0,
      keep: 0,
      accepted: 0,
      ignored: 0,
    };
    for (const cand of candidates) {
      const decision =
        projectDecisions.decisions[cand.candidate_id]?.decision;
      const bucket = getBucketForCandidate(cand, decision);
      if (counts[bucket] !== undefined) counts[bucket]++;
    }

    const tabsContainer = document.getElementById("artifact-bucket-tabs");
    if (!tabsContainer) return;

    const btnSuggested = tabsContainer.querySelector(
      '[data-bucket="suggested"]'
    );
    if (btnSuggested)
      btnSuggested.textContent = `Suggested Removals (${counts.suggested})`;

    const btnReview = tabsContainer.querySelector('[data-bucket="review"]');
    if (btnReview)
      btnReview.textContent = `Review Carefully (${counts.review})`;

    const btnKeep = tabsContainer.querySelector('[data-bucket="keep"]');
    if (btnKeep)
      btnKeep.textContent = `Probably Keep (${counts.keep})`;

    const btnAccepted = tabsContainer.querySelector(
      '[data-bucket="accepted"]'
    );
    if (btnAccepted)
      btnAccepted.textContent = `Accepted Rules (${counts.accepted})`;

    const btnIgnored = tabsContainer.querySelector(
      '[data-bucket="ignored"]'
    );
    if (btnIgnored)
      btnIgnored.textContent = `Ignored / Kept (${counts.ignored})`;

    // Update Accept All Suggested button text and visibility
    // (visibility is handled by renderBulkToolbar)
    const btnAcceptAll = document.getElementById(
      "btn-accept-all-suggested"
    ) as HTMLButtonElement;
    if (btnAcceptAll) {
      const suggestedCandidatesToAccept = candidates.filter((cand) => {
        const decision =
          projectDecisions.decisions[cand.candidate_id]?.decision;
        if (decision !== undefined) return false;
        if (getBucketForCandidate(cand, decision) !== "suggested")
          return false;
        const mid = cand.position_summary?.middle_count ?? 0;
        return mid === 0;
      });

      if (suggestedCandidatesToAccept.length > 0) {
        btnAcceptAll.textContent =
          `Accept all safe suggestions (${suggestedCandidatesToAccept.length})...`;
      }
    }
  }

  /* ─── Render candidate table ──────────────────────────────────────── */

  function renderCandidates(
    candidates: RepeatedArtifactCandidate[],
    diagnostics: RepeatedArtifactScanDiagnostics | null,
    isSortUpdate = false
  ): void {
    if (!isSortUpdate) {
      focusedCandidateId = null;
    }

    updateBucketTabs(candidates);

    const filteredCandidates = getFilteredCandidatesForActiveBucket(
      candidates,
      activeBucket
    );

    updateSelectAllCheckbox(filteredCandidates);
    renderBulkToolbar(activeBucket, filteredCandidates);

    dom.tblArtifactCandidates.innerHTML = "";
    dom.lblArtifactResultsCount.textContent = `${filteredCandidates.length} found`;

    if (filteredCandidates.length === 0) {
      let msg = "No candidates in this bucket.";
      if (activeBucket === "suggested") {
        if (diagnostics && diagnostics.total_raw_lines === 0) {
          msg =
            "No candidates found because no text lines were extracted. Check whether the documents are scanned/image-only PDFs or whether extraction failed.";
          if (diagnostics.files_failed_extraction > 0) {
            msg += ` Extraction failures were reported for ${diagnostics.files_failed_extraction} file(s).`;
          }
        } else if (
          diagnostics &&
          diagnostics.total_candidate_keys_before_filtering > 0 &&
          diagnostics.candidates_after_min_occurrences === 0
        ) {
          msg =
            "Candidate keys were found, but none met Min Occurrences. Try lowering Min Occurrences.";
        } else if (
          diagnostics &&
          diagnostics.candidates_after_min_occurrences > 0 &&
          diagnostics.candidates_after_min_files === 0
        ) {
          msg =
            "Candidates were found, but none appeared in enough distinct files. Try Min Files = 1 for repeated artefacts inside a single PDF/book.";
        } else {
          msg = "No suggested candidates meeting the thresholds.";
        }

        if (state.scanWasProcessed && state.removalCountAtScanStart > 0) {
          msg +=
            " Processed scans apply current Custom Removals, so already-removed artefacts will not appear. Scan Original extracted text to rediscover raw artefacts.";
        }
      }

      dom.tblArtifactCandidates.innerHTML = `
        <tr>
          <td colspan="6" style="padding: 20px; text-align: center; color: var(--text-muted);">${msg}</td>
        </tr>
      `;
      return;
    }

    let displayCandidates = filteredCandidates;
    if (currentSortColumn) {
      const candidatesWithIndex = filteredCandidates.map(
        (cand, idx) => ({ cand, idx })
      );
      candidatesWithIndex.sort((a, b) => {
        const res = compareCandidates(
          a.cand,
          b.cand,
          currentSortColumn!
        );
        if (res !== 0) {
          return currentSortDirection === "asc" ? res : -res;
        }
        return a.idx - b.idx;
      });
      displayCandidates = candidatesWithIndex.map((item) => item.cand);
    }

    displayCandidates.forEach((cand) => {
      const tr = document.createElement("tr");
      tr.dataset.id = cand.candidate_id;

      if (cand.candidate_id === focusedCandidateId) {
        tr.classList.add("is-focused");
      }
      if (state.selectedCandidateIds.has(cand.candidate_id)) {
        tr.classList.add("is-selected");
      }

      const tdCheck = document.createElement("td");
      tdCheck.style.padding = "6px 8px";
      tdCheck.style.textAlign = "center";
      const chk = document.createElement("input");
      chk.type = "checkbox";
      chk.checked = state.selectedCandidateIds.has(cand.candidate_id);
      const isNormalized = cand.kind === "normalized_line";
      const normalizedKey = (cand.normalized_key || "").trim();
      if (
        isNormalized &&
        !normalizedKey &&
        (!cand.raw_variants || cand.raw_variants.length === 0)
      ) {
        chk.disabled = true;
        chk.title =
          "This grouped pattern has no normalised key or actionable raw variants to add.";
      } else if (cand.kind === "exact_line") {
        chk.title = "Selecting adds a structured whole-line removal rule.";
      } else if (isNormalized && normalizedKey) {
        chk.title =
          "Selecting adds a structured normalised whole-line removal rule.";
      } else if (isNormalized) {
        const cappedNote = cand.raw_variant_count_is_capped
          ? " This fallback group is capped; additional untracked variants may remain after literal removal."
          : "";
        chk.title =
          `Selecting falls back to adding the ${cand.raw_variants.length} tracked raw variant${cand.raw_variants.length === 1 ? "" : "s"} to Custom Removals.${cappedNote}`;
      }
      chk.addEventListener("click", (e) => {
        e.stopPropagation();
        if (chk.checked) {
          state.selectedCandidateIds.add(cand.candidate_id);
        } else {
          state.selectedCandidateIds.delete(cand.candidate_id);
        }
        // Update visual state immediately
        if (chk.checked) {
          tr.classList.add("is-selected");
        } else {
          tr.classList.remove("is-selected");
        }
        renderBulkToolbar(
          activeBucket,
          getFilteredCandidatesForActiveBucket(
            state.lastScanCandidates,
            activeBucket
          )
        );
        updateSelectAllCheckbox(
          getFilteredCandidatesForActiveBucket(
            state.lastScanCandidates,
            activeBucket
          )
        );
      });
      tdCheck.appendChild(chk);

      const tdText = document.createElement("td");
      tdText.className = "candidate-text-cell";

      const patternWrapper = document.createElement("div");
      patternWrapper.className = "candidate-pattern-wrapper";

      const textSpan = document.createElement("span");
      textSpan.className = "candidate-text-display";
      textSpan.textContent =
        cand.normalized_key && cand.kind === "normalized_line"
          ? cand.normalized_key
          : cand.display_text;
      textSpan.title = cand.display_text;

      const badgesDiv = document.createElement("div");
      badgesDiv.className = "candidate-meta-badges";

      const kindChip = document.createElement("span");
      const kindClass = cand.kind
        .replace("_line", "")
        .replace("_block", "block");
      kindChip.className = `meta-badge-chip kind-${kindClass}`;
      kindChip.textContent = formatKind(cand.kind);

      const contentChip = document.createElement("span");
      const contentClass = cand.content_class.split("_")[0];
      contentChip.className = `meta-badge-chip content-${contentClass}`;
      contentChip.textContent = formatContentClass(cand.content_class);
      contentChip.title = formatTextSignalSummary(cand);

      badgesDiv.appendChild(kindChip);
      badgesDiv.appendChild(contentChip);
      patternWrapper.appendChild(textSpan);
      patternWrapper.appendChild(badgesDiv);
      tdText.appendChild(patternWrapper);

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
      tr.appendChild(tdOcc);
      tr.appendChild(tdFiles);
      tr.appendChild(tdPos);
      tr.appendChild(tdRisk);

      tr.onclick = () => {
        // Focus this candidate — show details
        dom.tblArtifactCandidates
          .querySelectorAll("tr.is-focused")
          .forEach((r) => r.classList.remove("is-focused"));
        tr.classList.add("is-focused");
        focusedCandidateId = cand.candidate_id;
        dom.artifactDetailsContent.innerHTML = "";
        showCandidateDetails(cand);
      };

      dom.tblArtifactCandidates.appendChild(tr);
    });
  }

  /* ─── Text signal helpers ─────────────────────────────────────────── */

  function getTextProfile(
    cand: RepeatedArtifactCandidate
  ): CandidateTextProfile | null {
    const candidate = cand as RepeatedArtifactCandidate & {
      text_profile?: CandidateTextProfile;
    };
    return candidate.text_profile ?? null;
  }

  function getTextSignalLabel(
    cand: RepeatedArtifactCandidate
  ): CandidateTextSignalLabel | null {
    const candidate = cand as RepeatedArtifactCandidate & {
      text_signal_label?: CandidateTextSignalLabel;
    };
    return candidate.text_signal_label ?? null;
  }

  function getTextSignalReasons(
    cand: RepeatedArtifactCandidate
  ): string[] {
    const candidate = cand as RepeatedArtifactCandidate & {
      text_signal_reasons?: string[];
    };
    return Array.isArray(candidate.text_signal_reasons)
      ? candidate.text_signal_reasons
      : [];
  }

  function formatTextSignalLabel(
    label: CandidateTextSignalLabel | null
  ): string {
    switch (label) {
      case "likely_natural_text":
        return "likely natural text";
      case "likely_section_heading":
        return "likely section heading";
      case "likely_page_label":
        return "likely page label";
      case "likely_table_or_numeric_row":
        return "likely table/statistical row";
      case "likely_formula_or_code":
        return "likely formula/code-like";
      case "likely_markup_or_extraction_noise":
        return "likely extraction noise";
      case "ambiguous":
        return "ambiguous";
      default:
        return "unavailable";
    }
  }

  function formatRatio(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function formatTextSignalSummary(
    cand: RepeatedArtifactCandidate
  ): string {
    const label = getTextSignalLabel(cand);
    const profile = getTextProfile(cand);
    const parts = [`Text signal: ${formatTextSignalLabel(label)}`];

    if (profile) {
      parts.push(
        `${profile.token_count} token${profile.token_count === 1 ? "" : "s"}`
      );
      parts.push(`${formatRatio(profile.alphabetic_ratio)} letters`);
      parts.push(`${formatRatio(profile.digit_ratio)} digits`);
      parts.push(`${formatRatio(profile.symbol_ratio)} symbols`);
      if (profile.max_repeated_char_run >= 4) {
        parts.push("repeated character run");
      }
    }

    return parts.join(" · ");
  }

  function formatTextSignalReason(reason: string): string {
    switch (reason) {
      case "common_section_heading":
        return "common section heading";
      case "page_label_pattern":
        return "page-label pattern";
      case "high_symbol_ratio":
        return "symbol-heavy";
      case "high_digit_ratio":
        return "digit-heavy";
      case "formula_or_code_symbols":
        return "formula/code symbols";
      case "markup_entity_or_tag":
        return "markup tag or entity";
      case "cid_marker":
        return "CID marker";
      case "long_repeated_character_run":
        return "long repeated character run";
      case "page_edge_repetition":
        return "appears at page edge";
      case "mostly_alphabetic":
        return "mostly alphabetic";
      case "multi_token_text":
        return "multi-token text";
      case "table_or_numeric_row":
        return "table/statistical row pattern";
      default:
        return reason.replace(/_/g, " ");
    }
  }

  /* ─── Show candidate detail panel (single candidate) ───────────────── */

  function showCandidateDetails(
    cand: RepeatedArtifactCandidate
  ): void {
    dom.artifactDetailsContent.innerHTML = "";

    const activeEntry =
      projectDecisions.decisions[cand.candidate_id];
    const decision = activeEntry?.decision;
    const activeScope = activeEntry?.scope;

    /* ─── Multi-selection note ──────────────────────────────────────── */

    const selectionCount = state.selectedCandidateIds.size;
    if (selectionCount > 1) {
      const note = document.createElement("div");
      note.className = "detail-note detail-note-info";
      note.textContent =
        `${selectionCount} candidates selected for bulk action.`;
      dom.artifactDetailsContent.appendChild(note);
    } else if (selectionCount === 1) {
      const note = document.createElement("div");
      note.className = "detail-note detail-note-info";
      note.textContent = `1 candidate selected for bulk action.`;
      dom.artifactDetailsContent.appendChild(note);
    }

    /* ─── Triage bar ────────────────────────────────────────────────── */

    const triageBar = document.createElement("div");
    triageBar.className = "candidate-triage-bar";

    const triageControls = document.createElement("div");
    triageControls.style.cssText =
      "display: flex; gap: 8px; align-items: center; width: 100%;";

    const acceptBtn = document.createElement("button");
    acceptBtn.type = "button";
    acceptBtn.className = "triage-btn accept-btn";
    acceptBtn.textContent = "Accept";
    if (decision === "accepted") {
      acceptBtn.classList.add("selected");
    }

    const keepBtn = document.createElement("button");
    keepBtn.type = "button";
    keepBtn.className = "triage-btn keep-btn";
    keepBtn.textContent = "Keep";
    if (decision === "kept") {
      keepBtn.classList.add("selected");
    }

    const ignoreBtn = document.createElement("button");
    ignoreBtn.type = "button";
    ignoreBtn.className = "triage-btn ignore-btn";
    ignoreBtn.textContent = "Ignore";
    if (decision === "ignored") {
      ignoreBtn.classList.add("selected");
    }

    const revertBtn = document.createElement("button");
    revertBtn.type = "button";
    revertBtn.className = "triage-btn revert-btn";
    revertBtn.textContent = "Revert";
    if (decision === undefined) {
      revertBtn.classList.add("hidden");
    }

    triageControls.appendChild(acceptBtn);
    triageControls.appendChild(keepBtn);
    triageControls.appendChild(ignoreBtn);
    triageControls.appendChild(revertBtn);

    const scopeContainer = document.createElement("div");
    scopeContainer.style.cssText =
      "margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 0.8rem;";

    const scopeLabel = document.createElement("label");
    scopeLabel.htmlFor = "select-triage-scope";
    scopeLabel.textContent = "Scope:";
    scopeContainer.appendChild(scopeLabel);

    const selectScope = document.createElement("select");
    selectScope.id = "select-triage-scope";
    selectScope.style.cssText =
      "background: var(--bg-color); color: var(--text-color); border: 1px solid var(--border-color); border-radius: 3px; padding: 2px 4px; font-size: 0.8rem;";

    if (cand.position_summary_is_page_based) {
      selectScope.innerHTML = `
        <option value="whole_line">Whole Line</option>
        <option value="page_top">Page Top</option>
        <option value="page_bottom">Page Bottom</option>
        <option value="page_top_or_bottom">Page Top/Bottom</option>
      `;
    } else {
      selectScope.innerHTML = `
        <option value="whole_line">Whole Line</option>
      `;
    }

    const defaultScope = computeCandidateScope(cand);
    selectScope.value = activeScope || defaultScope;

    if (decision === "accepted") {
      selectScope.disabled = true;
    }

    scopeContainer.appendChild(selectScope);
    triageControls.appendChild(scopeContainer);
    triageBar.appendChild(triageControls);
    dom.artifactDetailsContent.appendChild(triageBar);

    /* ─── Single-candidate action handlers ───────────────────────────── */

    acceptBtn.addEventListener("click", async () => {
      const chosenScope = selectScope.value as RemovalScope;
      const rule = addRuleForCandidate(cand, chosenScope);

      callbacks.renderCustomRemovals();
      syncRepeatedArtifactSettingsSection();

      projectDecisions.decisions[cand.candidate_id] = {
        decision: "accepted",
        candidate_signature: cand.candidate_id,
        candidate_text: getCandidateText(cand),
        scope: chosenScope,
        matcher_kind:
          cand.kind === "normalized_line" ? "normalized_line" : "literal",
        rule_id: rule.id,
        updated_at: new Date().toISOString(),
      };

      if (state.activeProject) {
        await saveProjectDecisions(
          state.activeProject.rootPath,
          projectDecisions
        );
      }

      renderCandidates(state.lastScanCandidates, null, true);
      const tr = dom.tblArtifactCandidates.querySelector(
        `tr[data-id="${cand.candidate_id}"]`
      );
      if (tr) tr.classList.add("is-focused");
      showCandidateDetails(cand);
    });

    keepBtn.addEventListener("click", async () => {
      removeRulesForCandidate(cand.candidate_id);

      callbacks.renderCustomRemovals();
      syncRepeatedArtifactSettingsSection();

      projectDecisions.decisions[cand.candidate_id] = {
        decision: "kept",
        candidate_signature: cand.candidate_id,
        candidate_text: getCandidateText(cand),
        scope: defaultScope,
        matcher_kind:
          cand.kind === "normalized_line" ? "normalized_line" : "literal",
        rule_id: "",
        updated_at: new Date().toISOString(),
      };

      if (state.activeProject) {
        await saveProjectDecisions(
          state.activeProject.rootPath,
          projectDecisions
        );
      }

      renderCandidates(state.lastScanCandidates, null, true);
      dom.artifactDetailsContent.innerHTML =
        `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
        `Candidate kept.</div>`;
    });

    ignoreBtn.addEventListener("click", async () => {
      removeRulesForCandidate(cand.candidate_id);

      callbacks.renderCustomRemovals();
      syncRepeatedArtifactSettingsSection();

      projectDecisions.decisions[cand.candidate_id] = {
        decision: "ignored",
        candidate_signature: cand.candidate_id,
        candidate_text: getCandidateText(cand),
        scope: defaultScope,
        matcher_kind:
          cand.kind === "normalized_line" ? "normalized_line" : "literal",
        rule_id: "",
        updated_at: new Date().toISOString(),
      };

      if (state.activeProject) {
        await saveProjectDecisions(
          state.activeProject.rootPath,
          projectDecisions
        );
      }

      renderCandidates(state.lastScanCandidates, null, true);
      dom.artifactDetailsContent.innerHTML =
        `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
        `Candidate ignored.</div>`;
    });

    revertBtn.addEventListener("click", async () => {
      removeRulesForCandidate(cand.candidate_id);

      callbacks.renderCustomRemovals();
      syncRepeatedArtifactSettingsSection();

      delete projectDecisions.decisions[cand.candidate_id];

      if (state.activeProject) {
        await saveProjectDecisions(
          state.activeProject.rootPath,
          projectDecisions
        );
      }

      renderCandidates(state.lastScanCandidates, null, true);
      dom.artifactDetailsContent.innerHTML =
        `<div style="color: var(--text-muted); text-align: center; margin-top: 40px; font-size: 0.9rem;">` +
        `Decision reverted.</div>`;
    });

    const isLiteral =
      cand.kind === "exact_line" ||
      cand.kind === "inline_artifact" ||
      cand.kind === "two_line_block" ||
      cand.kind === "three_line_block";

    const isNormalized = cand.kind === "normalized_line";
    const normalizedKey = (cand.normalized_key || "").trim();

    /* ─── Summary section ────────────────────────────────────────────── */

    const summaryTitle = document.createElement("div");
    summaryTitle.className = "detail-section-title";
    summaryTitle.textContent = "Summary";
    dom.artifactDetailsContent.appendChild(summaryTitle);

    const summaryGrid = document.createElement("div");
    summaryGrid.className = "detail-summary-grid";

    let rawVariantsLabel = "0 tracked";
    if (isNormalized) {
      const cappedNote = cand.raw_variant_count_is_capped
        ? " (capped)"
        : "";
      rawVariantsLabel = `${cand.raw_variant_count} tracked${cappedNote}`;
    }

    summaryGrid.innerHTML = `
      <div><strong>Kind:</strong> <span>${formatKind(cand.kind)}</span></div>
      <div><strong>Content:</strong> <span>${formatContentClass(cand.content_class)}</span></div>
      <div><strong>Occurrences:</strong> <span>${cand.occurrence_count}</span></div>
      <div><strong>Files:</strong> <span>${cand.file_count}</span></div>
      <div><strong>Raw variants:</strong> <span>${rawVariantsLabel}</span></div>
      <div><strong>Risk:</strong> <span>${formatRiskLabel(cand.risk_label)}</span></div>
    `;
    dom.artifactDetailsContent.appendChild(summaryGrid);

    /* ─── Evidence section ───────────────────────────────────────────── */

    const evidenceTitle = document.createElement("div");
    evidenceTitle.className = "detail-section-title";
    evidenceTitle.textContent = "Evidence";
    dom.artifactDetailsContent.appendChild(evidenceTitle);

    const textSignalDiv = document.createElement("div");
    textSignalDiv.className = "detail-note detail-note-info";
    textSignalDiv.textContent = formatTextSignalSummary(cand);
    dom.artifactDetailsContent.appendChild(textSignalDiv);

    const textSignalReasons = getTextSignalReasons(cand).map(
      formatTextSignalReason
    );
    if (textSignalReasons.length > 0) {
      const textSignalReasonsDiv = document.createElement("div");
      textSignalReasonsDiv.className = "detail-note detail-note-info";
      textSignalReasonsDiv.textContent = `Why: ${textSignalReasons.join("; ")}`;
      dom.artifactDetailsContent.appendChild(textSignalReasonsDiv);
    }

    if (cand.content_class === "numeric_dominant") {
      const numericCaution = document.createElement("div");
      numericCaution.style.fontSize = "0.8rem";
      numericCaution.style.color = "#e8a000";
      numericCaution.style.background = "rgba(232, 160, 0, 0.08)";
      numericCaution.style.padding = "6px 8px";
      numericCaution.style.borderRadius = "4px";
      numericCaution.style.borderLeft = "3px solid #e8a000";
      numericCaution.textContent =
        "Numeric-dominant candidate — review carefully. These may group unrelated tables, formulas, axis ticks, or statistical output.";
      dom.artifactDetailsContent.appendChild(numericCaution);
    }

    if (isLiteral) {
      const dispBlock = document.createElement("div");
      dispBlock.className = "candidate-display-block";
      const dispHeader = document.createElement("div");
      dispHeader.className = "candidate-display-header";
      dispHeader.textContent = "Candidate Text";
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
          setTimeout(() => {
            copyBtn.textContent = "Copy Candidate";
          }, 2000);
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
      dispHeader.textContent = "Normalised Grouping Key";
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
        variantsActionDiv.style.background =
          "rgba(80, 200, 120, 0.06)";
        variantsActionDiv.style.borderRadius = "4px";
        variantsActionDiv.style.borderLeft = "3px solid #50c878";

        let variantsLabel = normalizedKey
          ? `Tracked raw variants are shown for review. Selecting this candidate adds one normalised whole-line rule for "${normalizedKey}"`
          : `Selecting this candidate falls back to the ${cand.raw_variants.length} tracked raw variant${cand.raw_variants.length === 1 ? "" : "s"}`;
        if (cand.raw_variant_count_is_capped) {
          variantsLabel +=
            ", and this diagnostic list is capped";
        }
        variantsLabel += ".";
        const variantsLabelP = document.createElement("div");
        variantsLabelP.textContent = variantsLabel;
        variantsLabelP.style.marginBottom = "6px";
        variantsActionDiv.appendChild(variantsLabelP);

        const toggleBtn = document.createElement("button");
        toggleBtn.textContent = "Show raw variants";
        toggleBtn.style.cssText =
          "font-size: 0.8rem; padding: 3px 10px; border-radius: 3px; cursor: pointer; background: var(--bg-secondary); color: var(--text-color); border: 1px solid var(--border-color);";
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
          varLine.style.borderBottom =
            "1px solid var(--border-color)";
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
        noVariantsNote.className = normalizedKey
          ? "detail-note detail-note-info"
          : "detail-note detail-note-warning";
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

      const total =
        cand.position_summary.top_count +
        cand.position_summary.middle_count +
        cand.position_summary.bottom_count;
      const topPct =
        total > 0
          ? Math.round(
              (cand.position_summary.top_count / total) * 100
            )
          : 0;
      const midPct =
        total > 0
          ? Math.round(
              (cand.position_summary.middle_count / total) * 100
            )
          : 0;
      const botPct =
        total > 0
          ? Math.round(
              (cand.position_summary.bottom_count / total) * 100
            )
          : 0;

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

    /* ─── Risk advisory ──────────────────────────────────────────────── */

    const riskAdvisoryDiv = document.createElement("div");
    riskAdvisoryDiv.style.fontSize = "0.8rem";
    riskAdvisoryDiv.style.color = "var(--text-muted)";
    riskAdvisoryDiv.style.lineHeight = "1.4";
    riskAdvisoryDiv.style.padding = "6px 8px";
    riskAdvisoryDiv.style.background =
      "rgba(255, 255, 255, 0.02)";
    riskAdvisoryDiv.style.borderLeft =
      "2px solid var(--border-color)";

    let advisoryText = "";
    switch (cand.risk_label) {
      case "strong_header_footer_candidate":
        advisoryText =
          "Notice (Advisory Only): This sequence is heavily concentrated at page headers or footers. It is a very strong candidate for cleanup.";
        break;
      case "possible_boilerplate":
        advisoryText =
          "Notice (Advisory Only): This sequence occurs across multiple documents in similar forms, suggesting boilerplate (e.g. copyright notices or publisher headers).";
        break;
      case "common_section_heading_review_carefully":
        advisoryText =
          "Notice (Advisory Only): This matches common academic sections (e.g. Introduction). Review carefully as removing this may alter document structure.";
        break;
      case "symbol_or_noise_candidate":
        advisoryText =
          "Notice (Advisory Only): This contains a high proportion of non-alphanumeric symbols. It is highly likely to be extraction noise or page dividers.";
        break;
      default:
        advisoryText =
          "Notice (Advisory Only): Classification is ambiguous. Review occurrences below to check if it represents noise or content.";
    }
    riskAdvisoryDiv.textContent = advisoryText;
    dom.artifactDetailsContent.appendChild(riskAdvisoryDiv);

    /* ─── Examples ───────────────────────────────────────────────────── */

    if (cand.examples && cand.examples.length > 0) {
      const examplesTitle = document.createElement("div");
      examplesTitle.className = "detail-section-title";
      if (cand.occurrence_count > cand.examples.length) {
        examplesTitle.textContent = `Sample Occurrences (showing ${cand.examples.length} of ${cand.occurrence_count})`;
      } else {
        examplesTitle.textContent = `Sample Occurrences (showing all ${cand.examples.length})`;
      }
      dom.artifactDetailsContent.appendChild(examplesTitle);

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
        const pageStr =
          ex.page_number !== null && ex.page_number !== undefined
            ? `Page ~${ex.page_number} (Approx)`
            : "";
        const lineStr =
          ex.line_number !== null && ex.line_number !== undefined
            ? `Line ${ex.line_number}`
            : "";
        posSpan.textContent = [pageStr, lineStr]
          .filter(Boolean)
          .join(", ");

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

      dom.artifactDetailsContent.appendChild(examplesListDiv);
    }
  }
}
