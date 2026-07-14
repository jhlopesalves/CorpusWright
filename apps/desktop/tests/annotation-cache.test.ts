import {
  BUILD_ANNOTATIONS_HELP,
  annotationHeartbeatLine,
  annotationLayerControlViewModel,
  annotationTokenEtaLabel,
  appendAnnotationTokenProgressSample,
  estimateAnnotationTokenThroughput,
  formatAnnotationTokenRate,
  type AnnotationTokenProgressSample,
} from "../src/features/analyse/annotation/control.ts";
import type { AnnotationCacheAvailability } from "../src/generated/AnnotationCacheAvailability.ts";
import type { AnnotationCacheBuildProgress } from "../src/generated/AnnotationCacheBuildProgress.ts";
import type { AnnotationCacheState } from "../src/generated/AnnotationCacheState.ts";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function cache(state: AnnotationCacheState): AnnotationCacheAvailability {
  return { state, detail: null };
}

function model(
  state: AnnotationCacheState,
  transient: Parameters<typeof annotationLayerControlViewModel>[0]["transient"] = null,
) {
  return annotationLayerControlViewModel({
    cache: cache(state),
    transient,
    surfaceAnalysisBuilt: true,
    surfaceAnalysisRequired: false,
    languageLabel: "English",
    annotatedDocumentCount: 50000,
  });
}

function building(
  progress: AnnotationCacheBuildProgress | null = null,
  elapsedMs = 38000,
  annotatingElapsedMs: number | null = null,
  progressSamples: AnnotationTokenProgressSample[] = [],
  cancellationRequested = false,
): Parameters<typeof annotationLayerControlViewModel>[0]["transient"] {
  return {
    status: "building",
    elapsedMs,
    annotatingElapsedMs,
    cancellationRequested,
    progress,
    progressSamples,
  };
}

function progress(
  filesDone: number,
  filesTotal: number,
  tokensDone: bigint,
  tokensTotal: bigint,
  currentPhase: AnnotationCacheBuildProgress["current_phase"],
): AnnotationCacheBuildProgress {
  return {
    files_done: filesDone,
    files_total: filesTotal,
    tokens_done: tokensDone,
    tokens_total: tokensTotal,
    current_phase: currentPhase,
  };
}

assert(model("missing").statusLabel === "Missing", "Missing status should render");
assert(model("stale").statusLabel === "Stale", "Stale status should render");
assert(model("invalid").statusLabel === "Invalid", "Invalid status should render");
assert(model("ready").statusLabel === "Ready", "Ready status should render");
assert(
  model("missing").help === BUILD_ANNOTATIONS_HELP,
  "Missing copy should point to building annotations",
);
assert(
  model("ready").help === "English · 50,000 annotated documents",
  "Ready copy should show language and annotated document count",
);

assert(model("missing").actionLabel === "Build annotations", "Missing should build");
assert(model("invalid").actionLabel === "Build annotations", "Invalid should build");
assert(
  model("missing", { status: "failed", message: "UDPipe failed." }).actionLabel === "Build annotations",
  "Failed should build",
);
assert(
  model("missing", { status: "cancelled" }).actionLabel === "Build annotations",
  "Cancelled should build",
);
assert(
  model("missing", building()).actionLabel === "Cancel",
  "Building should cancel",
);
assert(
  model("missing", building()).action === "cancel",
  "Building should wire the action to cancel",
);
const cancelling = model("missing", building(null, 38000, null, [], true));
assert(cancelling.actionLabel === "Cancelling...", "Requested cancellation should be acknowledged");
assert(cancelling.actionDisabled, "Requested cancellation should disable a second cancel click");
assert(model("ready").actionLabel === "Rebuild annotations", "Ready should rebuild");
assert(model("stale").actionLabel === "Rebuild annotations", "Stale should rebuild");
assert(
  model("missing", building()).help === "38s elapsed · Preparing annotation build",
  "Building copy should show elapsed annotation work before backend progress arrives",
);

const loading = model("missing", building(progress(0, 10, 0n, 0n, { type: "LoadingModel" })));
assert(loading.progress?.kind === "indeterminate", "Loading model should render indeterminate progress");
assert(
  loading.progress.phaseLabel === "Loading model / preparing models",
  "Loading model phase should have a phase-aware label",
);
assert(loading.progress.etaLabel === "Estimating...", "Indeterminate phases should estimate ETA");

const annotatingSamples = [
  { atMs: 0, tokensDone: 0n },
  { atMs: 50000, tokensDone: 40000n },
];
const annotating = model("missing", building(
  progress(25, 100, 40000n, 200000n, { type: "Annotating" }),
  90000,
  50000,
  annotatingSamples,
));
assert(annotating.progress?.kind === "determinate", "Annotating should render determinate progress");
assert(annotating.progress.percent === 20, "Percentage should use tokens_done / tokens_total");
assert(annotating.progress.percentLabel === "20%", "Percentage label should be formatted");
assert(
  annotating.progress.filesLine === "25 / 100 files",
  "Annotating progress should show the file count line",
);
assert(
  annotating.progress.etaLabel === "3m 20s remaining",
  "ETA should use rolling token throughput",
);
assert(
  annotating.progress.heartbeatLine === "annotating - 800 tokens/s",
  "Annotating progress should show rounded token throughput",
);

const earlyAnnotating = model("missing", building(
  progress(1, 100, 10000n, 200000n, { type: "Annotating" }),
  90000,
  50000,
  [
    { atMs: 0, tokensDone: 0n },
    { atMs: 20000, tokensDone: 10000n },
  ],
));
assert(
  earlyAnnotating.progress?.etaLabel === "Estimating...",
  "ETA should wait until enough token-window data is available",
);
assert(
  earlyAnnotating.progress?.heartbeatLine === "annotating - Estimating...",
  "Heartbeat should show active annotation before throughput is ready",
);

const overdoneAnnotating = model("missing", building(
  progress(105, 100, 250n, 100n, { type: "Annotating" }),
  90000,
  50000,
));
assert(overdoneAnnotating.progress?.percent === 100, "Percentage should clamp at 100");
assert(
  overdoneAnnotating.progress?.etaLabel === "0s remaining",
  "ETA should never be negative",
);

const rollingSamples = [
  { atMs: 0, tokensDone: 0n },
  { atMs: 40000, tokensDone: 40000n },
  { atMs: 100000, tokensDone: 220000n },
];
assert(
  estimateAnnotationTokenThroughput(rollingSamples) === 3000,
  "Throughput should use the trailing 60s token window",
);
assert(
  estimateAnnotationTokenThroughput([
    { atMs: 0, tokensDone: 0n },
    { atMs: 20000, tokensDone: 20000n },
  ]) === null,
  "Throughput should wait for at least a 30s window",
);
assert(
  annotationTokenEtaLabel(
    progress(5, 10, 10000n, 70000n, { type: "Annotating" }),
    [
      { atMs: 0, tokensDone: 0n },
      { atMs: 30000, tokensDone: 10000n },
    ],
  ) === "3m 00s remaining",
  "ETA should project remaining tokens from token throughput",
);
assert(
  annotationTokenEtaLabel(
    progress(5, 10, 69500n, 70000n, { type: "Annotating" }),
    [
      { atMs: 0, tokensDone: 0n },
      { atMs: 30000, tokensDone: 69500n },
    ],
  ) === "Almost done",
  "ETA should not show a stale one-second remaining label",
);
assert(
  annotationHeartbeatLine(
    progress(5, 10, 10000n, 70000n, { type: "Annotating" }),
    [
      { atMs: 0, tokensDone: 0n },
      { atMs: 30000, tokensDone: 37500n },
    ],
  ) === "annotating - 1,300 tokens/s",
  "Heartbeat throughput should round coarsely to hundreds",
);
assert(
  formatAnnotationTokenRate(1249) === "1,200 tokens/s",
  "Token rate formatting should round to hundreds",
);
assert(
  formatAnnotationTokenRate(99) === "under 100 tokens/s",
  "Token rate formatting should not floor slow throughput to 100",
);
assert(
  appendAnnotationTokenProgressSample([{ atMs: 0, tokensDone: 0n }], { atMs: 61000, tokensDone: 61000n }).length === 1,
  "Token sample history should keep only the trailing throughput window",
);

const blocked = annotationLayerControlViewModel({
  cache: cache("missing"),
  transient: null,
  surfaceAnalysisBuilt: false,
  surfaceAnalysisRequired: true,
  languageLabel: null,
  annotatedDocumentCount: null,
});
assert(blocked.actionDisabled, "Surface dependency should disable the build action when required");
assert(
  blocked.actionDisabledReason === "Build surface analysis first.",
  "Surface dependency copy should name the required first step",
);
