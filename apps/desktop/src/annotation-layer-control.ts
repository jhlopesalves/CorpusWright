import type { AnnotationCacheAvailability } from "./generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheState } from "./generated/AnnotationCacheState.js";

import type { AnnotationCacheBuildProgress } from "./generated/AnnotationCacheBuildProgress.js";

export type AnnotationLayerTransientState =
  | { status: "building"; elapsedMs: number | null; progress: AnnotationCacheBuildProgress | null }
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
  action: AnnotationLayerAction;
  actionLabel: "Build annotations" | "Cancel" | "Rebuild annotations";
  actionDisabled: boolean;
  actionDisabledReason: string | null;
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

export function formatAnnotationElapsed(elapsedMs: number | null): string | null {
  if (elapsedMs === null) {
    return null;
  }
  const seconds = Math.max(0, Math.floor(elapsedMs / 1000));
  if (seconds < 60) {
    return `${seconds}s elapsed`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = `${seconds % 60}`.padStart(2, "0");
  return `${minutes}m ${remainingSeconds}s elapsed`;
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

      if (progress && progress.current_phase.type === "Annotating") {
        const percent =
          progress.files_total > 0
            ? Math.floor((progress.files_done / progress.files_total) * 100)
            : 0;
        
        let eta = "Estimating...";
        if (isBuilding && trans.elapsedMs !== null && percent >= 2 && progress.files_done > 0) {
          const msPerFile = trans.elapsedMs / progress.files_done;
          const remainingFiles = progress.files_total - progress.files_done;
          const remainingMs = remainingFiles * msPerFile;
          const remainingSeconds = Math.max(0, Math.floor(remainingMs / 1000));
          if (remainingSeconds < 60) {
            eta = `${remainingSeconds}s remaining`;
          } else {
            const minutes = Math.floor(remainingSeconds / 60);
            const rSec = `${remainingSeconds % 60}`.padStart(2, "0");
            eta = `${minutes}m ${rSec}s remaining`;
          }
        }
        
        return `${formatNumber(progress.files_done)} / ${formatNumber(progress.files_total)} files · ${percent}% · ${eta}`;
      }

      const statusMsg = progress
        ? (progress.current_phase.type === "BuildingSurface" ? "Preparing text" :
           progress.current_phase.type === "LoadingModel" ? "Loading models" :
           progress.current_phase.type === "WritingCache" ? "Saving annotations" : "Annotating")
        : "Annotating current project text.";

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

  return {
    title: `Linguistic annotations · ${label}`,
    status,
    statusLabel: label,
    help: helpText(input, status),
    action,
    actionLabel,
    actionDisabled,
    actionDisabledReason: actionDisabled ? "Build surface analysis first." : null,
  };
}
