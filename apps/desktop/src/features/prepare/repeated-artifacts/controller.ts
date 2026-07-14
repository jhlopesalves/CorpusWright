/**
 * Coordinates repeated-artefact scans, review decisions, and promotion into cleaning rules.
 *
 * Scan progress belongs to one corpus generation. Candidate decisions are project-scoped and
 * survive presentation changes until their source identity changes.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { createChevron } from "../../../shared/chevron";
import {
  dom,
  getArtifactTextModeRadios,
  getSelectedArtifactTextMode,
} from "../../../app/dom";
import { state } from "../../../app/state";
import {
  hasProjectSourceSelection,
  projectSourceSelectionCount,
  selectedProjectSources,
} from "../sources";
import type { RemovalRule } from "../../../generated/CleaningConfig.js";
import type { CandidateTextProfile } from "../../../generated/CandidateTextProfile.js";
import type { CandidateTextSignalLabel } from "../../../generated/CandidateTextSignalLabel.js";
import type { PositionSummary } from "../../../generated/PositionSummary.js";
import type { RepeatedArtifactCandidate } from "../../../generated/RepeatedArtifactCandidate.js";
import type { RepeatedArtifactScanConfig } from "../../../generated/RepeatedArtifactScanConfig.js";
import type { RepeatedArtifactScanDiagnostics } from "../../../generated/RepeatedArtifactScanDiagnostics.js";
import type { RepeatedArtifactScanReport } from "../../../generated/RepeatedArtifactScanReport.js";
import type { RepeatedArtifactPromotion } from "../../../generated/RepeatedArtifactPromotion.js";
import type { RepeatedArtifactMatcherKind } from "../../../generated/RepeatedArtifactMatcherKind.js";
import type { RemovalScope } from "../../../generated/RemovalScope.js";
import {
  effectivePersistedDecisionScope,
  emptyRemovalRuleMigrationSummary,
  mergeRemovalRuleMigrationSummaries,
  migrateRepeatedArtifactDecisionRule,
  persistedDecisionRuleId,
  type RemovalRuleMigrationSummary,
} from "../cleaning/removal-rule-migration.ts";
import {
  formatRemovalRuleText,
  syncDraftCustomRemovalsFromConfig,
  syncRepeatedArtifactRulesSummary,
} from "../cleaning/custom-removals.js";
import { removalRuleDuplicateKey } from "../cleaning/removal-rule-utils.js";
import {
  ARTIFACT_CATEGORIES,
  ARTIFACT_CATEGORY_LABELS,
  DECISION_FILTER_LABELS,
  categoryForCandidate,
  compareCandidatesByEvidence,
  decisionStateForCandidate,
  filterCandidatesForReview,
  riskRank,
  type ArtifactCategory,
  type DecisionFilter,
  type ReviewDecision,
} from "./review.js";

interface RepeatedArtifactCallbacks {
  renderCustomRemovals: () => void;
  schedulePreviewUpdate: (delay: number) => void;
  updateWordCount: () => void;
}

interface RepeatedArtifactScanScope {
  canScan: boolean;
  indices: number[];
  buttonLabel: string;
  helperText: string;
  modalText: string;
}

type MemoryBoundedRepeatedArtifactScanConfig = RepeatedArtifactScanConfig & {
  memory_ceiling_bytes: number;
};

type RepeatedArtifactScanProgress = {
  phase: "pass_1_scanning" | "filtering" | "pass_2_materialising";
  files_completed: number;
  files_total: number;
};

type RepeatedArtifactScanError =
  | { type: "cancelled" }
  | {
    type: "corpusTooLarge";
    stage: "pass_1_counting" | "survivor_materialisation";
    ceiling_bytes: number;
    estimated_bytes: number;
    suggestions: string[];
  }
  | { type: "scanFailed"; message: string };

const REPEATED_ARTIFACT_MEMORY_CEILING_BYTES = 1_288_490_188;

function repeatedArtifactProgressLabel(progress: RepeatedArtifactScanProgress): string {
  switch (progress.phase) {
    case "pass_1_scanning":
      return `Counting candidates... ${progress.files_completed.toLocaleString()}/${progress.files_total.toLocaleString()} files`;
    case "filtering":
      return "Filtering candidates...";
    case "pass_2_materialising":
      return `Materialising candidates... ${progress.files_completed.toLocaleString()}/${progress.files_total.toLocaleString()} files`;
  }
}

function structuredRepeatedArtifactError(error: unknown): RepeatedArtifactScanError | null {
  if (typeof error !== "object" || error === null || !("type" in error)) return null;
  const type = (error as { type?: unknown }).type;
  if (type === "cancelled" || type === "corpusTooLarge" || type === "scanFailed") {
    return error as RepeatedArtifactScanError;
  }
  return null;
}

function repeatedArtifactErrorMessage(error: unknown): string {
  const structured = structuredRepeatedArtifactError(error);
  if (structured?.type === "corpusTooLarge") {
    const ceilingMb = Math.round(structured.ceiling_bytes / (1024 * 1024));
    return `Corpus too large for this configuration (${ceilingMb.toLocaleString()} MB scan ceiling). ${structured.suggestions.join(" ")}`;
  }
  if (structured?.type === "scanFailed") return structured.message;
  return String(error);
}

interface DecisionEntry {
  decision: ReviewDecision;
  candidate_signature: string;
  candidate_text: string;
  scope: RemovalScope;
  matcher_kind: RepeatedArtifactMatcherKind;
  rule_id: string;
  updated_at: string;
}

interface ProjectDecisions {
  version: 1;
  decisions: Record<string, DecisionEntry>;
}

export let projectDecisions: ProjectDecisions = { version: 1, decisions: {} };
let promotionsByCandidateId = new Map<string, RepeatedArtifactPromotion>();

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
    tertiary: cand.occurrence_count
  };
}

function comparePosition(a: RepeatedArtifactCandidate, b: RepeatedArtifactCandidate): number {
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

function compareRisk(a: RepeatedArtifactCandidate, b: RepeatedArtifactCandidate): number {
  const rankA = riskRank(a);
  const rankB = riskRank(b);
  return rankA - rankB;
}

function compareCandidates(a: RepeatedArtifactCandidate, b: RepeatedArtifactCandidate, column: string): number {
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
      return compareStrings(formatContentClass(a.content_class), formatContentClass(b.content_class));
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
  return state.activeCleaningConfig.remove_patterns.length +
    state.activeCleaningConfig.removal_rules.filter(rule => rule.enabled).length;
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
    .filter(index => state.allFiles[index] !== undefined)
    .sort((a, b) => a - b);
}

export function getRepeatedArtifactScanScope(): RepeatedArtifactScanScope {
  const loadedCount = state.activeProject ? state.projectCorpusFileCount : state.allFiles.length;
  if (loadedCount === 0) {
    const helperText = state.activeProject && (state.projectSourceSummary?.total_count ?? 0) > 0
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
    ? hasProjectSourceSelection()
    : state.selectedCorpusIndices.size > 0;
  if (hasSelection) {
    const selected = state.activeProject ? [] : selectedScanIndices();
    const selectedCount = state.activeProject ? projectSourceSelectionCount() : selected.length;

    const selectedLabel = sourceCountLabel(selectedCount, "selected");
    return {
      canScan: true,
      indices: selected,
      buttonLabel: `Find repeated artefacts in ${selectedLabel}...`,
      helperText: "The scan will use the current Source Explorer selection.",
      modalText: `Scope: ${selectedLabel}.`,
    };
  }

  const allIndices = state.activeProject
    ? []
    : Array.from({ length: loadedCount }, (_, corpusIndex) => corpusIndex);
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

function promotionForCandidate(candidate: RepeatedArtifactCandidate): RepeatedArtifactPromotion | undefined {
  return promotionsByCandidateId.get(candidate.candidate_id);
}

function canAcceptCandidate(candidate: RepeatedArtifactCandidate): boolean {
  return promotionForCandidate(candidate)?.can_promote === true;
}

function removeRepeatedArtifactRulesForCandidate(
  rules: RemovalRule[],
  candidateId: string
): RemovalRule[] {
  const suffix = `:${candidateId}`;
  return rules.filter(rule => !rule.id.endsWith(suffix));
}

function emptyProjectDecisions(): ProjectDecisions {
  return { version: 1, decisions: {} };
}

interface ParsedProjectDecisions {
  decisions: ProjectDecisions;
  migrationSummary: RemovalRuleMigrationSummary;
}

function parseProjectDecisions(raw: string): ParsedProjectDecisions {
  const parsed = JSON.parse(raw) as unknown;
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { decisions: emptyProjectDecisions(), migrationSummary: emptyRemovalRuleMigrationSummary() };
  }
  const object = parsed as Record<string, unknown>;
  if (object.version !== 1 || object.decisions === null || typeof object.decisions !== "object" || Array.isArray(object.decisions)) {
    return { decisions: emptyProjectDecisions(), migrationSummary: emptyRemovalRuleMigrationSummary() };
  }

  const decisions: Record<string, DecisionEntry> = {};
  for (const [candidateId, entry] of Object.entries(object.decisions as Record<string, unknown>)) {
    if (entry === null || typeof entry !== "object" || Array.isArray(entry)) continue;
    const record = entry as Record<string, unknown>;
    const storedDecision = record.decision;
    const matcherKind = record.matcher_kind;
    const scope = record.scope;
    if (
      storedDecision !== "accepted" &&
      storedDecision !== "dismissed" &&
      storedDecision !== "kept" &&
      storedDecision !== "ignored"
    ) {
      continue;
    }
    if (matcherKind !== "literal" && matcherKind !== "normalized_line") continue;
    if (
      scope !== "anywhere" &&
      scope !== "whole_line" &&
      scope !== "page_top" &&
      scope !== "page_bottom" &&
      scope !== "page_top_or_bottom"
    ) {
      continue;
    }
    if (
      typeof record.candidate_signature !== "string" ||
      typeof record.candidate_text !== "string" ||
      typeof record.rule_id !== "string" ||
      typeof record.updated_at !== "string"
    ) {
      continue;
    }
    decisions[candidateId] = {
      decision: storedDecision === "accepted" ? "accepted" : "dismissed",
      candidate_signature: record.candidate_signature,
      candidate_text: record.candidate_text,
      scope,
      matcher_kind: matcherKind,
      rule_id: record.rule_id,
      updated_at: record.updated_at,
    };
  }

  const projectDecisions = { version: 1 as const, decisions };
  const migrationSummary = mergeRemovalRuleMigrationSummaries(
    Object.entries(decisions)
      .filter(([, entry]) => entry.decision === "accepted")
      .map(([candidateId, entry]) => createRuleFromDecision(candidateId, entry).migrationSummary)
  );
  return { decisions: projectDecisions, migrationSummary };
}

async function loadProjectDecisions(rootPath: string): Promise<ParsedProjectDecisions> {
  try {
    const raw = await invoke<string>("load_repeated_artifact_decisions_command", { rootPath });
    return parseProjectDecisions(raw);
  } catch (error) {
    console.error("Failed to load repeated artefact decisions:", error);
    return { decisions: emptyProjectDecisions(), migrationSummary: emptyRemovalRuleMigrationSummary() };
  }
}

async function saveProjectDecisions(rootPath: string, decisions: ProjectDecisions): Promise<void> {
  try {
    await invoke("save_repeated_artifact_decisions_command", {
      rootPath,
      content: JSON.stringify(decisions, null, 2),
    });
  } catch (error) {
    console.error("Failed to save repeated artefact decisions:", error);
  }
}

function isRepeatedArtifactRule(rule: RemovalRule): boolean {
  return rule.source === "promoted_repeated_artifact";
}

function effectiveDecisionScope(candidateId: string, entry: DecisionEntry): RemovalScope {
  return effectivePersistedDecisionScope(candidateId, entry);
}

function ruleIdForDecision(candidateId: string, entry: DecisionEntry, scope: RemovalScope): string {
  return persistedDecisionRuleId(candidateId, entry, scope);
}

function removeRepeatedArtifactRulesFromConfig(): void {
  state.activeCleaningConfig.removal_rules = state.activeCleaningConfig.removal_rules.filter(rule => !isRepeatedArtifactRule(rule));
  state.tempRemovalRules = state.tempRemovalRules.filter(rule => !isRepeatedArtifactRule(rule));
}

interface DecisionRuleResult {
  rule: RemovalRule | null;
  migrationSummary: RemovalRuleMigrationSummary;
}

function createRuleFromDecision(candidateId: string, entry: DecisionEntry): DecisionRuleResult {
  const migrated = migrateRepeatedArtifactDecisionRule(candidateId, entry);
  if (migrated.rule) {
    migrated.rule.label = formatRemovalRuleText(migrated.rule);
  }
  return { rule: migrated.rule, migrationSummary: migrated.summary };
}

export async function loadDecisionsForProject(rootPath: string): Promise<RemovalRuleMigrationSummary> {
  const loaded = await loadProjectDecisions(rootPath);
  projectDecisions = loaded.decisions;
  syncAcceptedRulesIntoConfig();
  return loaded.migrationSummary;
}

export function clearDecisions(): void {
  projectDecisions = emptyProjectDecisions();
  promotionsByCandidateId = new Map();
}

export function syncAcceptedRulesIntoConfig(): void {
  if (!state.activeProject) return;

  removeRepeatedArtifactRulesFromConfig();
  const existingRuleIds = new Set(state.activeCleaningConfig.removal_rules.map(rule => rule.id));
  const existingRuleKeys = new Set(state.activeCleaningConfig.removal_rules.map(removalRuleDuplicateKey));
  let migratedDecision = false;

  for (const [candidateId, entry] of Object.entries(projectDecisions.decisions)) {
    if (entry.decision !== "accepted") continue;
    const scope = effectiveDecisionScope(candidateId, entry);
    const ruleId = ruleIdForDecision(candidateId, entry, scope);
    if (entry.scope !== scope || entry.rule_id !== ruleId) {
      entry.scope = scope;
      entry.rule_id = ruleId;
      migratedDecision = true;
    }
    const rule = createRuleFromDecision(candidateId, entry).rule;
    if (!rule) continue;
    const ruleKey = removalRuleDuplicateKey(rule);
    if (existingRuleIds.has(rule.id) || existingRuleKeys.has(ruleKey)) continue;
    state.activeCleaningConfig.removal_rules.push(rule);
    existingRuleIds.add(rule.id);
    existingRuleKeys.add(ruleKey);
  }

  if (migratedDecision) {
    void saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }
  syncDraftCustomRemovalsFromConfig(state.activeCleaningConfig);
}

export function syncDecisionsWithActiveRules(): void {
  if (!state.activeProject) return;

  const activeRuleIds = new Set(state.activeCleaningConfig.removal_rules.map(rule => rule.id));
  let changed = false;
  for (const [candidateId, entry] of Object.entries(projectDecisions.decisions)) {
    if (entry.decision !== "accepted") continue;
    const scope = effectiveDecisionScope(candidateId, entry);
    const ruleId = ruleIdForDecision(candidateId, entry, scope);
    if (!activeRuleIds.has(ruleId)) {
      delete projectDecisions.decisions[candidateId];
      changed = true;
    }
  }

  if (changed) {
    void saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
  }
}

function removeRulesForCandidate(candidateId: string): void {
  state.activeCleaningConfig.removal_rules = removeRepeatedArtifactRulesForCandidate(
    state.activeCleaningConfig.removal_rules,
    candidateId
  );
  state.tempRemovalRules = removeRepeatedArtifactRulesForCandidate(
    state.tempRemovalRules,
    candidateId
  );
}

function cloneRemovalRule(rule: RemovalRule): RemovalRule {
  return {
    ...rule,
    matcher: { ...rule.matcher },
  };
}

function addRuleForCandidate(candidate: RepeatedArtifactCandidate): RemovalRule | null {
  const promotedRule = promotionForCandidate(candidate)?.rule;
  if (!promotedRule) return null;
  const rule = cloneRemovalRule(promotedRule);

  removeRulesForCandidate(candidate.candidate_id);

  const ruleKey = removalRuleDuplicateKey(rule);
  const existingRuleIds = new Set(state.activeCleaningConfig.removal_rules.map(existingRule => existingRule.id));
  const existingRuleKeys = new Set(state.activeCleaningConfig.removal_rules.map(removalRuleDuplicateKey));
  if (!existingRuleIds.has(rule.id) && !existingRuleKeys.has(ruleKey)) {
    state.activeCleaningConfig.removal_rules.push(rule);
  }

  const tempRuleIds = new Set(state.tempRemovalRules.map(existingRule => existingRule.id));
  const tempRuleKeys = new Set(state.tempRemovalRules.map(removalRuleDuplicateKey));
  if (!tempRuleIds.has(rule.id) && !tempRuleKeys.has(ruleKey)) {
    state.tempRemovalRules.push(cloneRemovalRule(rule));
  }

  return rule;
}

function buildDecisionEntry(
  candidate: RepeatedArtifactCandidate,
  promotion: RepeatedArtifactPromotion,
  decision: ReviewDecision
): DecisionEntry {
  return {
    decision,
    candidate_signature: candidate.candidate_id,
    candidate_text: promotion.rule_text,
    scope: promotion.scope,
    matcher_kind: promotion.matcher_kind,
    rule_id: decision === "accepted" ? promotion.rule_id : "",
    updated_at: new Date().toISOString(),
  };
}

function markCandidateForRemoval(candidate: RepeatedArtifactCandidate): boolean {
  if (!canAcceptCandidate(candidate)) return false;
  const promotion = promotionForCandidate(candidate);
  if (!promotion) return false;
  const rule = addRuleForCandidate(candidate);
  if (!rule) return false;
  projectDecisions.decisions[candidate.candidate_id] = buildDecisionEntry(candidate, promotion, "accepted");
  return true;
}

function unmarkCandidateForRemoval(candidate: RepeatedArtifactCandidate): boolean {
  const entry = projectDecisions.decisions[candidate.candidate_id];
  if (entry?.decision !== "accepted") return false;
  removeRulesForCandidate(candidate.candidate_id);
  delete projectDecisions.decisions[candidate.candidate_id];
  return true;
}

function dismissCandidate(candidate: RepeatedArtifactCandidate): boolean {
  const existing = projectDecisions.decisions[candidate.candidate_id];
  if (existing?.decision === "dismissed") return false;
  const promotion = promotionForCandidate(candidate);
  if (!promotion) return false;
  removeRulesForCandidate(candidate.candidate_id);
  projectDecisions.decisions[candidate.candidate_id] = buildDecisionEntry(candidate, promotion, "dismissed");
  return true;
}

function restoreCandidate(candidate: RepeatedArtifactCandidate): boolean {
  const entry = projectDecisions.decisions[candidate.candidate_id];
  if (entry?.decision !== "dismissed") return false;
  delete projectDecisions.decisions[candidate.candidate_id];
  return true;
}

export function initRepeatedArtifactFinder(callbacks: RepeatedArtifactCallbacks): void {
  const radioModes = getArtifactTextModeRadios();

  if (!dom.prepareRepeatedArtifactFinder || !dom.repeatedArtifactModal) return;

  let scanGeneration = 0;
  let scanTimerInterval: ReturnType<typeof setInterval> | null = null;
  let currentAbortController: AbortController | null = null;
  let currentSortColumn: string | null = null;
  let currentSortDirection: "asc" | "desc" = "desc";
  let focusedCandidateId: string | null = null;
  let lastDiagnostics: RepeatedArtifactScanDiagnostics | null = null;
  let activeCategory: ArtifactCategory = "page_labels";
  let activeDecisionFilter: DecisionFilter = "all";

  const configPanel = document.getElementById("repeated-artifact-config-panel") as HTMLDivElement | null;
  const toggleScanSettingsButton = document.getElementById("btn-toggle-scan-settings") as HTMLButtonElement | null;
  const scanSettingsArrow = document.getElementById("scan-settings-arrow") as HTMLSpanElement | null;
  const typeTabs = document.getElementById("artifact-type-tabs");
  const decisionFilters = document.getElementById("artifact-decision-filters");
  const selectAllCheckbox = document.getElementById("chk-select-all-tab") as HTMLInputElement | null;
  const decisionSummary = document.getElementById("repeated-artifact-decision-summary") as HTMLSpanElement | null;
  const applyDecisionsButton = document.getElementById("btn-apply-artifact-decisions") as HTMLButtonElement | null;
  const candidateTableContainer = dom.tblArtifactCandidates.closest(".repeated-artifact-table-container") as HTMLElement | null;

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
      dom.artifactProcessedWarning.classList.remove("hidden");
    } else {
      dom.artifactProcessedWarning.classList.add("hidden");
    }
  }

  function setStatus(stage: string, elapsed?: number): void {
    const timeStr = elapsed !== undefined ? ` (${elapsed.toFixed(1)}s)` : "";
    dom.tblArtifactCandidates.innerHTML = `
      <tr>
        <td colspan="6" class="artifact-table-message">
          <div class="spinner artifact-status-spinner"></div>
          ${stage}${timeStr}
        </td>
      </tr>
    `;
    dom.artifactDetailsContent.innerHTML = `<div class="artifact-details-empty">${stage}...</div>`;
  }

  function resetScanControls(): void {
    dom.btnRunArtifactScan.classList.remove("hidden");
    dom.btnRunArtifactScan.disabled = false;
    dom.btnRunArtifactScan.textContent = "Run Scan";
    dom.btnCancelScan.classList.add("hidden");
    if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }
    syncRepeatedArtifactModalScope();
  }

  function syncRepeatedArtifactModalScope(): RepeatedArtifactScanScope {
    const scope = syncRepeatedArtifactSettingsSection();
    dom.lblArtifactScanScope.textContent = scope.modalText;
    if (!dom.btnRunArtifactScan.classList.contains("hidden")) {
      dom.btnRunArtifactScan.disabled = !scope.canScan;
    }
    return scope;
  }

  function openRepeatedArtifactFinder(): void {
    syncRepeatedArtifactModalScope();
    updateDecisionSummary();
    dom.repeatedArtifactModal.classList.remove("hidden");
    updateProcessedWarning();
  }

  function renderDiagnostics(diags: RepeatedArtifactScanDiagnostics): void {
    const parts: string[] = [];
    parts.push(`Files: ${diags.files_scanned}/${diags.files_requested} scanned`);
    parts.push(`Lines: ${diags.total_raw_lines.toLocaleString()}`);
    parts.push(`Candidate keys: ${diags.total_candidate_keys_before_filtering}`);
    parts.push(`After occurrence filter: ${diags.candidates_after_min_occurrences}`);
    parts.push(`After file filter: ${diags.candidates_after_min_files}`);
    parts.push(`Before cap: ${diags.candidates_before_cap}`);
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
    if (diags.candidates_truncated > 0) {
      text += ` · ${diags.candidates_truncated.toLocaleString()} more not shown because Max Candidates limited the result.`;
    }

    dom.artifactDiagnostics.textContent = text;
    dom.artifactDiagnostics.classList.remove("hidden");
  }

  dom.prepareRepeatedArtifactFinder.addEventListener("click", openRepeatedArtifactFinder);
  dom.btnFindRepeatedArtifactsSettings.addEventListener("click", () => {
    const scope = syncRepeatedArtifactSettingsSection();
    if (!scope.canScan) return;
    openRepeatedArtifactFinder();
  });

  function closeArtifactModal(): void {
    scanGeneration++;
    if (currentAbortController) { currentAbortController.abort(); currentAbortController = null; }
    invoke("cancel_repeated_artifacts_command").catch(() => {});
    resetScanControls();
    dom.lblScanTime.classList.add("hidden");
    focusedCandidateId = null;
    currentSortColumn = null;
    currentSortDirection = "desc";
    lastDiagnostics = null;
    updateSortIndicators();
    updateSelectAllCheckbox([]);
    dom.artifactDiagnostics.classList.add("hidden");
    dom.repeatedArtifactModal.classList.add("hidden");
  }

  dom.btnCloseArtifactModal.addEventListener("click", closeArtifactModal);
  dom.btnCloseArtifactModalTop.addEventListener("click", closeArtifactModal);

  // Setup sortable headers click listeners
  const tableElement = dom.tblArtifactCandidates.closest("table");
  const sortHeaders = tableElement?.querySelectorAll(".sort-header");
  sortHeaders?.forEach((header) => {
    header.addEventListener("click", () => {
      const column = header.getAttribute("data-column");
      if (!column) return;

      if (currentSortColumn === column) {
        currentSortDirection = currentSortDirection === "desc" ? "asc" : "desc";
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
        th.setAttribute("aria-sort", currentSortDirection === "desc" ? "descending" : "ascending");
        if (indicator) {
          indicator.textContent = currentSortDirection === "desc" ? " ↓" : " ↑";
        }
      } else {
        th.removeAttribute("aria-sort");
        if (indicator) {
          indicator.textContent = "";
        }
      }
    });
  }

  function setTransientStatus(message: string): void {
    dom.lblArtifactAddStatus.textContent = message;
    if (message) {
      setTimeout(() => {
        if (dom.lblArtifactAddStatus.textContent === message) {
          dom.lblArtifactAddStatus.textContent = "";
        }
      }, 5000);
    }
  }

  function setActiveReviewControls(): void {
    typeTabs?.querySelectorAll<HTMLElement>(".tab").forEach(tab => {
      tab.classList.toggle("active", tab.dataset.category === activeCategory);
    });
    decisionFilters?.querySelectorAll<HTMLElement>(".decision-filter-chip").forEach(chip => {
      const isActive = chip.dataset.decisionFilter === activeDecisionFilter;
      chip.classList.toggle("active", isActive);
      chip.setAttribute("aria-pressed", isActive ? "true" : "false");
    });
  }

  function resetCandidateTableScroll(): void {
    if (candidateTableContainer) {
      candidateTableContainer.scrollTop = 0;
    }
  }

  function currentReviewCandidates(): RepeatedArtifactCandidate[] {
    return filterCandidatesForReview(
      state.lastScanCandidates,
      activeCategory,
      activeDecisionFilter,
      projectDecisions.decisions
    );
  }

  function updateReviewControls(candidates: RepeatedArtifactCandidate[]): void {
    const categoryCounts: Record<ArtifactCategory, number> = {
      page_labels: 0,
      headings: 0,
      headers_footers: 0,
      boilerplate: 0,
      symbol_noise: 0,
      other: 0,
    };

    for (const candidate of candidates) {
      categoryCounts[categoryForCandidate(candidate)]++;
    }

    typeTabs?.querySelectorAll<HTMLButtonElement>(".tab").forEach(tab => {
      const category = tab.dataset.category as ArtifactCategory | undefined;
      if (!category || categoryCounts[category] === undefined) return;
      tab.textContent = `${ARTIFACT_CATEGORY_LABELS[category]} (${categoryCounts[category]})`;
    });

    const categoryCandidates = candidates.filter(candidate => categoryForCandidate(candidate) === activeCategory);
    const decisionCounts: Record<DecisionFilter, number> = {
      all: categoryCandidates.length,
      unreviewed: 0,
      marked: 0,
      dismissed: 0,
    };
    for (const candidate of categoryCandidates) {
      const decision = projectDecisions.decisions[candidate.candidate_id]?.decision;
      decisionCounts[decisionStateForCandidate(decision)]++;
    }
    decisionFilters?.querySelectorAll<HTMLButtonElement>(".decision-filter-chip").forEach(chip => {
      const filter = chip.dataset.decisionFilter as DecisionFilter | undefined;
      if (!filter || decisionCounts[filter] === undefined) return;
      chip.textContent = `${DECISION_FILTER_LABELS[filter]} (${decisionCounts[filter]})`;
    });
  }

  function updateSelectAllCheckbox(filteredCandidates: RepeatedArtifactCandidate[]): void {
    if (!selectAllCheckbox) return;
    const acceptableCandidates = filteredCandidates.filter(candidate => (
      projectDecisions.decisions[candidate.candidate_id]?.decision !== "dismissed" &&
      canAcceptCandidate(candidate)
    ));
    const acceptedCount = acceptableCandidates.filter(candidate => (
      projectDecisions.decisions[candidate.candidate_id]?.decision === "accepted"
    )).length;

    selectAllCheckbox.disabled = acceptableCandidates.length === 0;
    if (acceptableCandidates.length === 0 || acceptedCount === 0) {
      selectAllCheckbox.checked = false;
      selectAllCheckbox.indeterminate = false;
      return;
    }

    selectAllCheckbox.checked = acceptedCount === acceptableCandidates.length;
    selectAllCheckbox.indeterminate = acceptedCount > 0 && acceptedCount < acceptableCandidates.length;
  }

  function syncDecisionControls(filteredCandidates = currentReviewCandidates()): void {
    updateSelectAllCheckbox(filteredCandidates);
    updateDecisionSummary();
  }

  function updateDecisionSummary(): void {
    if (!decisionSummary) return;
    const decisions = Object.values(projectDecisions.decisions);
    const markedCount = decisions.filter(entry => entry.decision === "accepted").length;
    const dismissedCount = decisions.filter(entry => entry.decision === "dismissed").length;
    const reviewedInScan = state.lastScanCandidates.filter(candidate => (
      projectDecisions.decisions[candidate.candidate_id]?.decision !== undefined
    )).length;
    const progress = state.lastScanCandidates.length > 0
      ? ` · ${reviewedInScan} of ${state.lastScanCandidates.length} reviewed`
      : "";
    decisionSummary.textContent = `${markedCount} marked for removal · ${dismissedCount} dismissed${progress}`;
  }

  async function persistDecisions(): Promise<void> {
    if (state.activeProject) {
      await saveProjectDecisions(state.activeProject.rootPath, projectDecisions);
    }
  }

  function scheduleProcessedPreviewUpdate(): void {
    callbacks.schedulePreviewUpdate(150);
  }

  async function refreshAfterDecision(message: string, refreshPreview: boolean): Promise<void> {
    await persistDecisions();
    callbacks.renderCustomRemovals();
    syncRepeatedArtifactSettingsSection();
    if (refreshPreview) {
      scheduleProcessedPreviewUpdate();
    }
    renderCandidates(state.lastScanCandidates, lastDiagnostics, true);
    if (focusedCandidateId) {
      const focusedCandidate = state.lastScanCandidates.find(candidate => candidate.candidate_id === focusedCandidateId);
      if (focusedCandidate) showCandidateDetails(focusedCandidate);
    }
    setTransientStatus(message);
  }

  async function finishAndClose(): Promise<void> {
    await persistDecisions();
    callbacks.renderCustomRemovals();
    syncRepeatedArtifactSettingsSection();
    if (state.selectedCorpusIndices.size > 0 || hasProjectSourceSelection()) {
      scheduleProcessedPreviewUpdate();
    }
    closeArtifactModal();
  }

  if (scanSettingsArrow) {
    scanSettingsArrow.replaceChildren(createChevron("scan-settings-chevron"));
  }
  if (toggleScanSettingsButton && configPanel) {
    toggleScanSettingsButton.setAttribute(
      "aria-expanded",
      configPanel.classList.contains("hidden") ? "false" : "true",
    );
  }

  toggleScanSettingsButton?.addEventListener("click", () => {
    if (!configPanel) return;
    const isHidden = configPanel.classList.toggle("hidden");
    toggleScanSettingsButton.setAttribute("aria-expanded", isHidden ? "false" : "true");
  });

  typeTabs?.querySelectorAll<HTMLButtonElement>(".tab").forEach(tab => {
    tab.addEventListener("click", () => {
      const category = tab.dataset.category as ArtifactCategory | undefined;
      if (!category || category === activeCategory) return;
      activeCategory = category;
      focusedCandidateId = null;
      resetCandidateTableScroll();
      renderCandidates(state.lastScanCandidates, lastDiagnostics);
      dom.artifactDetailsContent.innerHTML = '<div class="artifact-details-empty">Select a candidate to inspect examples.</div>';
    });
  });

  decisionFilters?.querySelectorAll<HTMLButtonElement>(".decision-filter-chip").forEach(chip => {
    chip.addEventListener("click", () => {
      const filter = chip.dataset.decisionFilter as DecisionFilter | undefined;
      if (!filter || filter === activeDecisionFilter) return;
      activeDecisionFilter = filter;
      focusedCandidateId = null;
      resetCandidateTableScroll();
      renderCandidates(state.lastScanCandidates, lastDiagnostics);
      dom.artifactDetailsContent.innerHTML = '<div class="artifact-details-empty">Select a candidate to inspect examples.</div>';
    });
  });

  selectAllCheckbox?.addEventListener("change", (event) => {
    event.stopPropagation();
    const candidates = currentReviewCandidates().filter(candidate => (
      projectDecisions.decisions[candidate.candidate_id]?.decision !== "dismissed" &&
      canAcceptCandidate(candidate)
    ));
    let changedCount = 0;
    for (const candidate of candidates) {
      const changed = selectAllCheckbox.checked
        ? markCandidateForRemoval(candidate)
        : unmarkCandidateForRemoval(candidate);
      if (changed) changedCount++;
    }
    if (changedCount === 0) return;
    const action = selectAllCheckbox.checked ? "Marked" : "Unmarked";
    void refreshAfterDecision(
      `${action} ${changedCount} candidate${changedCount === 1 ? "" : "s"}.`,
      true,
    );
  });

  applyDecisionsButton?.addEventListener("click", () => { void finishAndClose(); });

  dom.btnRunArtifactScan.addEventListener("click", async () => {
    const scope = syncRepeatedArtifactModalScope();
    if (!scope.canScan) {
      alert(scope.helperText);
      return;
    }

    const textMode = getSelectedArtifactTextMode()?.value || "original";
    const analyseProcessed = textMode === "processed";

    state.scanWasProcessed = analyseProcessed;
    state.removalCountAtScanStart = activeCustomRemovalCount();

    const config: MemoryBoundedRepeatedArtifactScanConfig = {
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
      min_occurrences: parseInt(dom.numArtifactMinOcc.value, 10) || 3,
      min_files: parseInt(dom.numArtifactMinFiles.value, 10) || 1,
      max_candidates: Number.isFinite(parseInt(dom.numArtifactMaxCand.value, 10))
        ? Math.max(parseInt(dom.numArtifactMaxCand.value, 10), 1)
        : null,
      max_examples_per_candidate: Math.min(Math.max(parseInt(dom.numArtifactMaxExamples.value, 10) || 25, 1), 100),
      min_line_chars: 2,
      max_line_chars: 600,
      memory_ceiling_bytes: REPEATED_ARTIFACT_MEMORY_CEILING_BYTES,
    };

    dom.btnRunArtifactScan.classList.add("hidden");
    dom.btnCancelScan.classList.remove("hidden");
    dom.btnCancelScan.disabled = false;
    dom.btnCancelScan.textContent = "Cancel";
    dom.lblScanTime.classList.remove("hidden");
    dom.lblScanTime.textContent = "0.0s";

    const myGen = ++scanGeneration;
    const startTime = performance.now();

    setStatus("Preparing files...", 0);

    if (scanTimerInterval) clearInterval(scanTimerInterval);
    scanTimerInterval = setInterval(() => {
      const elapsed = (performance.now() - startTime) / 1000;
      dom.lblScanTime.textContent = `${elapsed.toFixed(1)}s`;
    }, 500);

    const myVersion = state.currentCorpusVersion;
    const indices = state.activeProject ? null : scope.indices;

    let unlistenProgress: UnlistenFn | null = null;
    try {
      unlistenProgress = await listen<RepeatedArtifactScanProgress>(
        "repeated-artifact-scan-progress",
        (event) => {
          if (myGen !== scanGeneration) return;
          setStatus(repeatedArtifactProgressLabel(event.payload));
        },
      );
      const report = await invoke<RepeatedArtifactScanReport>("scan_repeated_artifacts_command", {
        indices,
        sourcePaths: state.activeProject && hasProjectSourceSelection() && !state.allMatchingProjectSourcesSelected
          ? selectedProjectSources().map((source) => source.path)
          : null,
        rootPath: state.allMatchingProjectSourcesSelected ? state.activeProject?.rootPath : null,
        allMatchingQuery: state.allMatchingProjectSourcesSelected ? state.sourceSearchQuery : null,
        excludedSourceIds: state.allMatchingProjectSourcesSelected
          ? Array.from(state.deselectedProjectSourceIds)
          : null,
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
      promotionsByCandidateId = new Map(
        report.promotions.map(promotion => [promotion.candidate_id, promotion])
      );
      activeCategory = ARTIFACT_CATEGORIES.find(category => (
        report.candidates.some(candidate => categoryForCandidate(candidate) === category)
      )) ?? "page_labels";
      activeDecisionFilter = "all";
      lastDiagnostics = report.diagnostics;

      renderDiagnostics(report.diagnostics);

      dom.artifactDetailsContent.innerHTML = '<div class="artifact-details-empty">Select a candidate to inspect examples.</div>';

      renderCandidates(report.candidates, report.diagnostics);
    } catch (err) {
      if (myGen !== scanGeneration) return;

      if (scanTimerInterval) { clearInterval(scanTimerInterval); scanTimerInterval = null; }

      const structuredError = structuredRepeatedArtifactError(err);
      const errStr = repeatedArtifactErrorMessage(err);
      if (structuredError?.type === "cancelled" || errStr.toLowerCase().includes("cancel")) {
        dom.lblScanTime.textContent = "Cancelled";
        dom.tblArtifactCandidates.innerHTML = `
          <tr>
            <td colspan="6" class="artifact-table-message">Scan was cancelled.</td>
          </tr>
        `;
      } else {
        dom.lblScanTime.textContent = "Error";
        console.error(err);
        dom.tblArtifactCandidates.innerHTML = "";
        const errorRow = document.createElement("tr");
        const errorCell = document.createElement("td");
        errorCell.colSpan = 6;
        errorCell.className = "artifact-table-message artifact-table-message--error";
        errorCell.textContent = `Error during scan: ${errStr}`;
        errorRow.appendChild(errorCell);
        dom.tblArtifactCandidates.appendChild(errorRow);
      }

    } finally {
      unlistenProgress?.();
      if (myGen !== scanGeneration) return;
      resetScanControls();
    }
  });

  radioModes.forEach(radio => {
    radio.addEventListener("change", updateProcessedWarning);
  });

  syncRepeatedArtifactSettingsSection();

  function renderCandidates(
    candidates: RepeatedArtifactCandidate[],
    diagnostics: RepeatedArtifactScanDiagnostics | null,
    isSortUpdate = false
  ): void {
    if (!isSortUpdate) {
      focusedCandidateId = null;
    }

    updateReviewControls(candidates);
    setActiveReviewControls();

    const filteredCandidates = filterCandidatesForReview(
      candidates,
      activeCategory,
      activeDecisionFilter,
      projectDecisions.decisions
    );
    syncDecisionControls(filteredCandidates);

    dom.tblArtifactCandidates.innerHTML = "";
    const reviewedCount = candidates.filter(candidate => (
      projectDecisions.decisions[candidate.candidate_id]?.decision !== undefined
    )).length;
    dom.lblArtifactResultsCount.textContent =
      `${filteredCandidates.length.toLocaleString()} shown · ${reviewedCount.toLocaleString()} of ${candidates.length.toLocaleString()} reviewed`;

    if (filteredCandidates.length === 0) {
      let msg = "No candidates in this type and decision filter.";

      if (candidates.length === 0) {
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
          msg = "No candidates meeting the thresholds.";
        }

        if (state.scanWasProcessed && state.removalCountAtScanStart > 0) {
          msg += " Processed scans apply current Custom Removals, so already-removed artefacts will not appear. Scan Original extracted text to rediscover raw artefacts.";
        }
      }

      dom.tblArtifactCandidates.innerHTML = `
        <tr>
          <td colspan="6" class="artifact-table-message">${msg}</td>
        </tr>
      `;
      return;
    }

    let displayCandidates = [...filteredCandidates];
    if (currentSortColumn) {
      displayCandidates.sort((a, b) => {
        const res = compareCandidates(a, b, currentSortColumn!);
        if (res !== 0) {
          return currentSortDirection === "asc" ? res : -res;
        }
        return compareCandidatesByEvidence(a, b);
      });
    }

    if (!isSortUpdate) {
      resetCandidateTableScroll();
    }
    displayCandidates.forEach((cand) => {
      dom.tblArtifactCandidates.appendChild(renderCandidateRow(cand));
    });

    const noteContainer = dom.tblArtifactCandidates.closest(".repeated-artifact-table-container");
    if (noteContainer) {
      const existingNote = noteContainer.querySelector(".artifact-dedup-note");
      if (existingNote) existingNote.remove();
    }
  }

  function renderCandidateRow(cand: RepeatedArtifactCandidate): HTMLTableRowElement {
    const tr = document.createElement("tr");
    tr.dataset.id = cand.candidate_id;
    if (cand.candidate_id === focusedCandidateId) {
      tr.classList.add("is-focused");
    }

    const tdCheck = document.createElement("td");
    tdCheck.className = "artifact-candidate-select";
    const decision = projectDecisions.decisions[cand.candidate_id]?.decision;
    if (decision !== "dismissed") {
      const chk = document.createElement("input");
      chk.type = "checkbox";
      chk.checked = decision === "accepted";
      chk.disabled = !canAcceptCandidate(cand);
      chk.title = chk.disabled
        ? "This candidate cannot form a removal rule."
        : "Mark this candidate for removal.";
      chk.setAttribute("aria-label", chk.title);
      chk.addEventListener("click", (event) => {
        event.stopPropagation();
      });
      chk.addEventListener("change", (event) => {
        event.stopPropagation();
        const changed = chk.checked
          ? markCandidateForRemoval(cand)
          : unmarkCandidateForRemoval(cand);
        if (!changed) {
          chk.checked = projectDecisions.decisions[cand.candidate_id]?.decision === "accepted";
          return;
        }
        const message = chk.checked
          ? "Candidate marked for removal."
          : "Candidate no longer marked for removal.";
        void refreshAfterDecision(message, true);
      });
      tdCheck.appendChild(chk);
    }

    const tdText = document.createElement("td");
    tdText.className = "candidate-text-cell";
    const patternWrapper = document.createElement("div");
    patternWrapper.className = "candidate-pattern-wrapper";

    const textSpan = document.createElement("span");
    textSpan.className = "candidate-text-display";
    textSpan.textContent = cand.normalized_key && cand.kind === "normalized_line"
      ? cand.normalized_key
      : cand.display_text;
    textSpan.title = cand.display_text;

    const badges = document.createElement("div");
    badges.className = "candidate-meta-badges";

    const kindChip = document.createElement("span");
    const kindClass = cand.kind === "normalized_line" ? "normalized" : cand.kind.replace("_line", "").replace("_block", "-block");
    kindChip.className = `meta-badge-chip kind-${kindClass}`;
    kindChip.textContent = formatKind(cand.kind);

    const contentChip = document.createElement("span");
    const contentClass = cand.content_class.split("_")[0];
    contentChip.className = `meta-badge-chip content-${contentClass}`;
    contentChip.textContent = formatContentClass(cand.content_class);
    contentChip.title = formatTextSignalSummary(cand);

    badges.append(kindChip, contentChip);
    if (decision !== "accepted") {
      const decisionAction = document.createElement("button");
      decisionAction.type = "button";
      decisionAction.className = "candidate-decision-action";
      const isDismissed = decision === "dismissed";
      decisionAction.textContent = isDismissed ? "Restore" : "Dismiss";
      decisionAction.title = isDismissed
        ? "Restore - show this candidate in scan results again."
        : "Dismiss - stop suggesting this candidate on future scans.";
      decisionAction.addEventListener("click", (event) => {
        event.stopPropagation();
        const changed = isDismissed
          ? restoreCandidate(cand)
          : dismissCandidate(cand);
        if (!changed) return;
        void refreshAfterDecision(
          isDismissed ? "Candidate restored." : "Candidate dismissed.",
          false,
        );
      });
      badges.appendChild(decisionAction);
    }
    patternWrapper.append(textSpan, badges);
    tdText.appendChild(patternWrapper);

    const tdOcc = document.createElement("td");
    tdOcc.className = "artifact-candidate-number";
    tdOcc.textContent = cand.occurrence_count.toString();

    const tdFiles = document.createElement("td");
    tdFiles.className = "artifact-candidate-number";
    tdFiles.textContent = cand.file_count.toString();

    const tdPos = document.createElement("td");
    tdPos.className = "artifact-candidate-position";
    tdPos.textContent = formatPosition(cand.position_summary);

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
      dom.tblArtifactCandidates.querySelectorAll("tr.is-focused").forEach((row) => row.classList.remove("is-focused"));
      tr.classList.add("is-focused");
      focusedCandidateId = cand.candidate_id;
      dom.artifactDetailsContent.innerHTML = "";
      showCandidateDetails(cand);
    };

    return tr;
  }

  function getTextProfile(cand: RepeatedArtifactCandidate): CandidateTextProfile | null {
    const candidate = cand as RepeatedArtifactCandidate & { text_profile?: CandidateTextProfile };
    return candidate.text_profile ?? null;
  }

  function getTextSignalLabel(cand: RepeatedArtifactCandidate): CandidateTextSignalLabel | null {
    const candidate = cand as RepeatedArtifactCandidate & { text_signal_label?: CandidateTextSignalLabel };
    return candidate.text_signal_label ?? null;
  }

  function getTextSignalReasons(cand: RepeatedArtifactCandidate): string[] {
    const candidate = cand as RepeatedArtifactCandidate & { text_signal_reasons?: string[] };
    return Array.isArray(candidate.text_signal_reasons) ? candidate.text_signal_reasons : [];
  }

  function formatTextSignalLabel(label: CandidateTextSignalLabel | null): string {
    switch (label) {
      case "likely_natural_text": return "likely natural text";
      case "likely_section_heading": return "likely section heading";
      case "likely_page_label": return "likely page label";
      case "likely_table_or_numeric_row": return "likely table/statistical row";
      case "likely_formula_or_code": return "likely formula/code-like";
      case "likely_markup_or_extraction_noise": return "likely extraction noise";
      case "ambiguous": return "ambiguous";
      default: return "unavailable";
    }
  }

  function formatRatio(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function formatTextSignalSummary(cand: RepeatedArtifactCandidate): string {
    const label = getTextSignalLabel(cand);
    const profile = getTextProfile(cand);
    const parts = [`Text signal: ${formatTextSignalLabel(label)}`];

    if (profile) {
      parts.push(`${profile.token_count} token${profile.token_count === 1 ? "" : "s"}`);
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
      case "common_section_heading": return "common section heading";
      case "page_label_pattern": return "page-label pattern";
      case "high_symbol_ratio": return "symbol-heavy";
      case "high_digit_ratio": return "digit-heavy";
      case "formula_or_code_symbols": return "formula/code symbols";
      case "markup_entity_or_tag": return "markup tag or entity";
      case "cid_marker": return "CID marker";
      case "long_repeated_character_run": return "long repeated character run";
      case "page_edge_repetition": return "appears at page edge";
      case "mostly_alphabetic": return "mostly alphabetic";
      case "multi_token_text": return "multi-token text";
      case "table_or_numeric_row": return "table/statistical row pattern";
      default: return reason.replace(/_/g, " ");
    }
  }

  function showCandidateDetails(cand: RepeatedArtifactCandidate): void {
    dom.artifactDetailsContent.innerHTML = "";

    const decision = projectDecisions.decisions[cand.candidate_id]?.decision;
    const decisionNote = document.createElement("div");
    decisionNote.className = "detail-note detail-note-info";
    if (decision === "accepted") {
      decisionNote.textContent = "While this candidate is marked for removal, it acts as a repeated-artefact removal rule on processed text. Unticking removes that rule. Source files are never modified.";
    } else if (decision === "dismissed") {
      decisionNote.textContent = "This candidate is dismissed. Choose the Dismissed filter in its type tab to restore it. Source files are never modified.";
    } else {
      decisionNote.textContent = "Tick the row to mark this candidate for removal from processed text. Source files are never modified.";
    }
    dom.artifactDetailsContent.appendChild(decisionNote);

    const isLiteral = cand.kind === "exact_line" ||
      cand.kind === "inline_artifact" ||
      cand.kind === "two_line_block" ||
      cand.kind === "three_line_block";

    const isNormalized = cand.kind === "normalized_line";
    const normalizedKey = (cand.normalized_key || "").trim();

    const metaDiv = document.createElement("div");
    metaDiv.className = "artifact-detail-meta";

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
    metaParts.push(`<span><strong>Text signal:</strong> ${formatTextSignalLabel(getTextSignalLabel(cand))}</span>`);

    metaDiv.innerHTML = metaParts.join("");
    dom.artifactDetailsContent.appendChild(metaDiv);

    const textSignalDiv = document.createElement("div");
    textSignalDiv.className = "detail-note detail-note-info";
    textSignalDiv.textContent = formatTextSignalSummary(cand);
    dom.artifactDetailsContent.appendChild(textSignalDiv);

    const textSignalReasons = getTextSignalReasons(cand).map(formatTextSignalReason);
    if (textSignalReasons.length > 0) {
      const textSignalReasonsDiv = document.createElement("div");
      textSignalReasonsDiv.className = "detail-note detail-note-info";
      textSignalReasonsDiv.textContent = `Why: ${textSignalReasons.join("; ")}`;
      dom.artifactDetailsContent.appendChild(textSignalReasonsDiv);
    }

    if (cand.content_class === "numeric_dominant") {
      const numericCaution = document.createElement("div");
      numericCaution.className = "artifact-numeric-caution";
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
        ? "Marking this candidate creates one normalised whole-line rule. Lines are removed only when their normalised whole-line form matches this key."
        : "This candidate has no normalised key. Marking it falls back to tracked raw variants when they are available.";
      dom.artifactDetailsContent.appendChild(normNote);

      if (cand.raw_variants && cand.raw_variants.length > 0) {
        const variantsActionDiv = document.createElement("div");
        variantsActionDiv.className = "artifact-variants-action";

        let variantsLabel = normalizedKey
          ? `Tracked raw variants are shown for review. Marking this candidate adds one normalised whole-line rule for "${normalizedKey}"`
          : `Marking this candidate falls back to the ${cand.raw_variants.length} tracked raw variant${cand.raw_variants.length === 1 ? "" : "s"}`;
        if (cand.raw_variant_count_is_capped) {
          variantsLabel += ", and this diagnostic list is capped";
        }
        variantsLabel += ".";
        const variantsLabelP = document.createElement("div");
        variantsLabelP.className = "artifact-variants-label";
        variantsLabelP.textContent = variantsLabel;
        variantsActionDiv.appendChild(variantsLabelP);

        const toggleBtn = document.createElement("button");
        toggleBtn.className = "artifact-variants-toggle";
        toggleBtn.textContent = "Show raw variants";
        variantsActionDiv.appendChild(toggleBtn);

        const variantsList = document.createElement("div");
        variantsList.className = "artifact-variants-list hidden";

        for (const variant of cand.raw_variants) {
          const varLine = document.createElement("div");
          varLine.className = "artifact-variant-line";
          varLine.textContent = variant;
          variantsList.appendChild(varLine);
        }
        variantsActionDiv.appendChild(variantsList);

        toggleBtn.addEventListener("click", () => {
          if (variantsList.classList.contains("hidden")) {
            variantsList.classList.remove("hidden");
            toggleBtn.textContent = "Hide raw variants";
          } else {
            variantsList.classList.add("hidden");
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
      posSummaryDiv.className = "artifact-position-summary";

      const total = cand.position_summary.top_count + cand.position_summary.middle_count + cand.position_summary.bottom_count;
      const topPct = total > 0 ? Math.round((cand.position_summary.top_count / total) * 100) : 0;
      const midPct = total > 0 ? Math.round((cand.position_summary.middle_count / total) * 100) : 0;
      const botPct = total > 0 ? Math.round((cand.position_summary.bottom_count / total) * 100) : 0;

      posSummaryDiv.innerHTML = `
        <strong>Estimated Layout Positions (Approximate):</strong>
        <div class="artifact-position-summary-values">
          <span>Top: ${cand.position_summary.top_count} (${topPct}%)</span>
          <span>Body: ${cand.position_summary.middle_count} (${midPct}%)</span>
          <span>Bottom: ${cand.position_summary.bottom_count} (${botPct}%)</span>
        </div>
      `;
      dom.artifactDetailsContent.appendChild(posSummaryDiv);
    }

    const riskAdvisoryDiv = document.createElement("div");
    riskAdvisoryDiv.className = "artifact-risk-advisory";

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
      examplesTitle.className = "artifact-examples-title";
      if (cand.occurrence_count > cand.examples.length) {
        examplesTitle.textContent = `Sample Occurrences (showing ${cand.examples.length} of ${cand.occurrence_count})`;
      } else {
        examplesTitle.textContent = `Sample Occurrences (showing all ${cand.examples.length})`;
      }

      const examplesListDiv = document.createElement("div");
      examplesListDiv.className = "artifact-examples-list";

      cand.examples.forEach((ex, idx) => {
        const card = document.createElement("div");
        card.className = "artifact-example-card cw-panel cw-panel--compact";

        const cardHeader = document.createElement("div");
        cardHeader.className = "artifact-example-header";

        const docSpan = document.createElement("span");
        docSpan.className = "artifact-example-document";
        docSpan.textContent = `Instance ${idx + 1}: ${ex.file_name}`;
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
