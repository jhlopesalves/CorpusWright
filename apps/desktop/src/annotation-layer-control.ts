import type { AnnotationCacheAvailability } from "./generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheState } from "./generated/AnnotationCacheState.js";

import type { AnnotationCacheBuildProgress } from "./generated/AnnotationCacheBuildProgress.js";

export type AnnotationLayerTransientState =
  | {
      status: "building";
      elapsedMs: number | null;
      annotatingElapsedMs: number | null;
      progress: AnnotationCacheBuildProgress | null;
    }
  | { status: "failed"; message: string }
  | { status: "cancelled" };

export type AnnotationLayerStatus = AnnotationCacheState | AnnotationLayerTransientState["status"];
export type AnnotationLayerAction = "start" | "cancel";

export interface AnnotationLayerControlInput {
  cache: AnnotationCacheAvailability;
  transient: AnnotationLayerTransientState | null;
  surfaceAnalysisBuilt: boolean;
  surfaceAnalysisRequired: boolean;
  languageLabel: string | null;
  annotatedDocumentCount: number | null;
}

export interface AnnotationLayerControlViewModel {
  title: string;
  status: AnnotationLayerStatus;
  statusLabel: string;
  help: string;
  progress: AnnotationLayerProgressViewModel | null;
  action: AnnotationLayerAction;
  actionLabel: "Build annotations" | "Cancel" | "Rebuild annotations";
  actionDisabled: boolean;
  actionDisabledReason: string | null;
}

export interface AnnotationLayerProgressViewModel {
  kind: "indeterminate" | "determinate";
  phaseLabel: string;
  percent: number | null;
  percentLabel: string | null;
  filesLine: string | null;
  etaLabel: string;
}

export const BUILD_ANNOTATIONS_HELP =
  "Build annotations to enable lemma and POS frequency.";

function statusLabel(status: AnnotationLayerStatus): string {
  switch (status) {
    case "ready": return "Ready";
    case "stale": return "Stale";
    case "missing": return "Missing";
    case "invalid": return "Invalid";
    case "building": return "Building";
    case "failed": return "Failed";
    case "cancelled": return "Cancelled";
  }
}

function formatNumber(value: number): string {
  return value.toLocaleString();
}

function formatCoarseDuration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = `${seconds % 60}`.padStart(2, "0");
  return `${minutes}m ${remainingSeconds}s`;
}

export function formatAnnotationElapsed(elapsedMs: number | null): string | null {
  if (elapsedMs === null) {
    return null;
  }
  return `${formatCoarseDuration(elapsedMs)} elapsed`;
}

function annotationPhaseLabel(progress: AnnotationCacheBuildProgress): string {
  switch (progress.current_phase.type) {
    case "BuildingSurface":
      return "Building surface analysis";
    case "LoadingModel":
      return "Loading model / preparing models";
    case "Annotating":
      return "Annotating";
    case "WritingCache":
      return "Writing cache";
    case "Completed":
      return "Completed";
    case "Failed":
      return "Failed";
    case "Cancelled":
      return "Cancelled";
  }
}

function annotationProgressViewModel(
  progress: AnnotationCacheBuildProgress,
  annotatingElapsedMs: number | null,
): AnnotationLayerProgressViewModel {
  const phaseLabel = annotationPhaseLabel(progress);
  if (progress.current_phase.type !== "Annotating") {
    return {
      kind: "indeterminate",
      phaseLabel,
      percent: null,
      percentLabel: null,
      filesLine: null,
      etaLabel: "Estimating...",
    };
  }

  const total = Math.max(0, progress.files_total);
  const done = Math.min(Math.max(0, progress.files_done), total);
  const fraction = total > 0 ? done / total : 0;
  const percent = total > 0
    ? Math.min(100, Math.max(0, Math.floor(fraction * 100)))
    : 0;

  let etaLabel = "Estimating...";
  if (annotatingElapsedMs !== null && done > 0 && fraction >= 0.02) {
    const remainingFiles = Math.max(0, total - done);
    const msPerFile = Math.max(0, annotatingElapsedMs) / done;
    etaLabel = `${formatCoarseDuration(remainingFiles * msPerFile)} remaining`;
  }

  return {
    kind: "determinate",
    phaseLabel,
    percent,
    percentLabel: `${percent}%`,
    filesLine: `${formatNumber(done)} / ${formatNumber(total)} files`,
    etaLabel,
  };
}

function readyHelp(input: AnnotationLayerControlInput): string {
  const parts: string[] = [];
  if (input.languageLabel && input.languageLabel !== "Not available") {
    parts.push(input.languageLabel);
  }
  if (input.annotatedDocumentCount !== null) {
    parts.push(`${formatNumber(input.annotatedDocumentCount)} annotated documents`);
  }
  return parts.length > 0 ? parts.join(" · ") : "Ready for lemma and POS frequency.";
}

function helpText(input: AnnotationLayerControlInput, status: AnnotationLayerStatus): string {
  if (input.surfaceAnalysisRequired && !input.surfaceAnalysisBuilt && status !== "building") {
    return "Build surface analysis first.";
  }

  switch (status) {
    case "ready":
      return readyHelp(input);
    case "stale":
      return "Rebuild annotations to refresh lemma and POS frequency.";
    case "missing":
      return BUILD_ANNOTATIONS_HELP;
    case "invalid":
      return "Build annotations to replace the invalid annotation layer.";
    case "building": {
      const trans = input.transient;
      const isBuilding = trans?.status === "building";
      const elapsed = isBuilding ? formatAnnotationElapsed(trans.elapsedMs) : null;
      const progress = isBuilding ? trans.progress : null;
      const statusMsg = progress ? annotationPhaseLabel(progress) : "Preparing annotation build";

      return elapsed ? `${elapsed} · ${statusMsg}` : statusMsg;
    }
    case "failed":
      return input.transient?.status === "failed"
        ? `Annotation build failed: ${input.transient.message}`
        : "Annotation build failed.";
    case "cancelled":
      return "Annotation build cancelled. Build annotations to enable lemma and POS frequency.";
  }
}

export function annotationLayerControlViewModel(
  input: AnnotationLayerControlInput,
): AnnotationLayerControlViewModel {
  const status = input.transient?.status ?? input.cache.state;
  const action: AnnotationLayerAction = status === "building" ? "cancel" : "start";
  const actionLabel = status === "building"
    ? "Cancel"
    : status === "ready" || status === "stale"
      ? "Rebuild annotations"
      : "Build annotations";
  const actionDisabled =
    action === "start" && input.surfaceAnalysisRequired && !input.surfaceAnalysisBuilt;
  const label = statusLabel(status);
  const progress = status === "building" && input.transient?.status === "building" && input.transient.progress
    ? annotationProgressViewModel(
        input.transient.progress,
        input.transient.annotatingElapsedMs,
      )
    : null;

  return {
    title: `Linguistic annotations · ${label}`,
    status,
    statusLabel: label,
    help: helpText(input, status),
    progress,
    action,
    actionLabel,
    actionDisabled,
    actionDisabledReason: actionDisabled ? "Build surface analysis first." : null,
  };
}
