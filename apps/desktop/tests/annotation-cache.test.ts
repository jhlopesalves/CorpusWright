import {
  BUILD_ANNOTATIONS_HELP,
  annotationLayerControlViewModel,
} from "../src/annotation-layer-control.ts";
import type { AnnotationCacheAvailability } from "../src/generated/AnnotationCacheAvailability.ts";
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
  model("missing", { status: "building", elapsedMs: 38000 }).actionLabel === "Cancel",
  "Building should cancel",
);
assert(model("ready").actionLabel === "Rebuild annotations", "Ready should rebuild");
assert(model("stale").actionLabel === "Rebuild annotations", "Stale should rebuild");
assert(
  model("missing", { status: "building", elapsedMs: 38000 }).help === "38s elapsed · annotating",
  "Building copy should show elapsed annotation work",
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
