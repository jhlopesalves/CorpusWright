import {
  BUILD_ANNOTATIONS_HELP,
  annotationLayerControlViewModel,
} from "../src/annotation-layer-control.ts";
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
): Parameters<typeof annotationLayerControlViewModel>[0]["transient"] {
  return {
    status: "building",
    elapsedMs,
    annotatingElapsedMs,
    progress,
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
assert(model("ready").actionLabel === "Rebuild annotations", "Ready should rebuild");
assert(model("stale").actionLabel === "Rebuild annotations", "Stale should rebuild");
assert(
  model("missing", building()).help === "38s elapsed · Preparing annotation build",
  "Building copy should show elapsed annotation work before backend progress arrives",
);

const loading = model("missing", building({
  files_done: 0,
  files_total: 10,
  current_phase: { type: "LoadingModel" },
}));
assert(loading.progress?.kind === "indeterminate", "Loading model should render indeterminate progress");
assert(
  loading.progress.phaseLabel === "Loading model / preparing models",
  "Loading model phase should have a phase-aware label",
);
assert(loading.progress.etaLabel === "Estimating...", "Indeterminate phases should estimate ETA");

const annotating = model("missing", building({
  files_done: 25,
  files_total: 100,
  current_phase: { type: "Annotating" },
}, 90000, 50000));
assert(annotating.progress?.kind === "determinate", "Annotating should render determinate progress");
assert(annotating.progress.percent === 25, "Percentage should use files_done / files_total");
assert(annotating.progress.percentLabel === "25%", "Percentage label should be formatted");
assert(
  annotating.progress.filesLine === "25 / 100 files",
  "Annotating progress should show the file count line",
);
assert(
  annotating.progress.etaLabel === "2m 30s remaining",
  "ETA should use annotation-phase elapsed time",
);

const earlyAnnotating = model("missing", building({
  files_done: 1,
  files_total: 100,
  current_phase: { type: "Annotating" },
}, 90000, 50000));
assert(
  earlyAnnotating.progress?.etaLabel === "Estimating...",
  "ETA should wait until enough annotating progress is available",
);

const overdoneAnnotating = model("missing", building({
  files_done: 105,
  files_total: 100,
  current_phase: { type: "Annotating" },
}, 90000, 50000));
assert(overdoneAnnotating.progress?.percent === 100, "Percentage should clamp at 100");
assert(
  overdoneAnnotating.progress?.etaLabel === "0s remaining",
  "ETA should never be negative",
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
