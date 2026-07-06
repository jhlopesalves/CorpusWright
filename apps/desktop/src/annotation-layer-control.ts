import type { AnnotationCacheAvailability } from "./generated/AnnotationCacheAvailability.js";
import type { AnnotationCacheState } from "./generated/AnnotationCacheState.js";

import type { AnnotationCacheBuildProgress } from "./generated/AnnotationCacheBuildProgress.js";

export type AnnotationLayerTransientState =
  | {
      status: "building";
      elapsedMs: number;
      annotatingElapsedMs: number | null;
      progress: AnnotationCacheBuildProgress | null;
      progressSamples: AnnotationTokenProgressSample[];
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
  heartbeatLine: string | null;
}

export const BUILD_ANNOTATIONS_HELP =
  "Build annotations to enable lemma and POS frequency.";
export const ANNOTATION_THROUGHPUT_WINDOW_MS = 60000;
export const ANNOTATION_THROUGHPUT_MIN_SPAN_MS = 30000;

export interface AnnotationTokenProgressSample {
  atMs: number;
  tokensDone: bigint;
}

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

export function formatCoarseDuration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = `${seconds % 60}`.padStart(2, "0");
  return `${minutes}m ${remainingSeconds}s`;
}

export function formatAnnotationElapsed(elapsedMs: number): string {
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

function clampTokenProgress(progress: AnnotationCacheBuildProgress): {
  done: bigint;
  total: bigint;
} {
  const total = progress.tokens_total > 0n ? progress.tokens_total : 0n;
  const rawDone = progress.tokens_done > 0n ? progress.tokens_done : 0n;
  const done = rawDone > total ? total : rawDone;
  return { done, total };
}

export function annotationTokenProgressPercent(progress: AnnotationCacheBuildProgress): number {
  const { done, total } = clampTokenProgress(progress);
  if (total === 0n) {
    return 0;
  }
  return Number((done * 100n) / total);
}

export function appendAnnotationTokenProgressSample(
  samples: readonly AnnotationTokenProgressSample[],
  sample: AnnotationTokenProgressSample,
  windowMs = ANNOTATION_THROUGHPUT_WINDOW_MS,
): AnnotationTokenProgressSample[] {
  const cutoff = sample.atMs - windowMs;
  const retained = samples.filter((entry) =>
    Number.isFinite(entry.atMs) && entry.atMs >= cutoff && entry.atMs <= sample.atMs
  );
  const last = retained[retained.length - 1];
  if (last?.atMs === sample.atMs && last.tokensDone === sample.tokensDone) {
    return retained;
  }
  return [...retained, sample];
}

export function estimateAnnotationTokenThroughput(
  samples: readonly AnnotationTokenProgressSample[],
  windowMs = ANNOTATION_THROUGHPUT_WINDOW_MS,
  minSpanMs = ANNOTATION_THROUGHPUT_MIN_SPAN_MS,
): number | null {
  const latest = samples[samples.length - 1];
  if (!latest || !Number.isFinite(latest.atMs)) {
    return null;
  }

  const cutoff = latest.atMs - windowMs;
  let earliest: AnnotationTokenProgressSample | null = null;
  for (const sample of samples) {
    if (!Number.isFinite(sample.atMs) || sample.atMs < cutoff || sample.atMs > latest.atMs) {
      continue;
    }
    if (latest.atMs - sample.atMs >= minSpanMs) {
      earliest = sample;
      break;
    }
  }

  if (!earliest) {
    return null;
  }

  const tokenDelta = latest.tokensDone - earliest.tokensDone;
  const elapsedMs = latest.atMs - earliest.atMs;
  if (tokenDelta <= 0n || elapsedMs <= 0) {
    return null;
  }

  return Number(tokenDelta) / (elapsedMs / 1000);
}

export function formatAnnotationTokenRate(tokensPerSecond: number): string {
  const rounded = Math.max(100, Math.round(tokensPerSecond / 100) * 100);
  return `${formatNumber(rounded)} tokens/s`;
}

export function annotationTokenEtaLabel(
  progress: AnnotationCacheBuildProgress,
  samples: readonly AnnotationTokenProgressSample[],
): string {
  const { done, total } = clampTokenProgress(progress);
  const remainingTokens = total - done;
  if (remainingTokens <= 0n) {
    return "0s remaining";
  }

  const tokensPerSecond = estimateAnnotationTokenThroughput(samples);
  if (tokensPerSecond === null) {
    return "Estimating...";
  }

  const remainingMs = Number(remainingTokens) / tokensPerSecond * 1000;
  const duration = formatCoarseDuration(remainingMs);
  return duration === "1s" ? "Almost done" : `${duration} remaining`;
}

export function annotationHeartbeatLine(
  progress: AnnotationCacheBuildProgress,
  samples: readonly AnnotationTokenProgressSample[],
): string | null {
  if (progress.current_phase.type !== "Annotating") {
    return null;
  }

  const tokensPerSecond = estimateAnnotationTokenThroughput(samples);
  return tokensPerSecond === null
    ? "annotating - Estimating..."
    : `annotating - ${formatAnnotationTokenRate(tokensPerSecond)}`;
}

function annotationProgressViewModel(
  progress: AnnotationCacheBuildProgress,
  annotatingElapsedMs: number | null,
  samples: readonly AnnotationTokenProgressSample[],
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
      heartbeatLine: null,
    };
  }

  const totalFiles = Math.max(0, progress.files_total);
  const doneFiles = Math.min(Math.max(0, progress.files_done), totalFiles);
  const percent = annotationTokenProgressPercent(progress);
  const progressSamples = annotatingElapsedMs === null ? [] : samples;

  return {
    kind: "determinate",
    phaseLabel,
    percent,
    percentLabel: `${percent}%`,
    filesLine: `${formatNumber(doneFiles)} / ${formatNumber(totalFiles)} files`,
    etaLabel: annotationTokenEtaLabel(progress, progressSamples),
    heartbeatLine: annotationHeartbeatLine(progress, progressSamples),
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
        input.transient.progressSamples,
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
