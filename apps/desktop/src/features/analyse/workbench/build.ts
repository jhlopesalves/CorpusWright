/**
 * Coordinates analysis-corpus and annotation-layer build lifecycles.
 *
 * Each asynchronous build and availability probe carries a generation. Progress, errors, and final
 * state reach the workbench only while that generation remains current.
 */
import {
  buildAnnotationCache,
  buildAnalysisCorpus,
  cancelAnnotationBuild,
  cancelAnalysisBuild,
  computeCorpusOverview,
  defaultAnalysisDocumentSelection,
  getAnalysisBuildStatus,
  queryAnnotatedFrequency,
  type AnnotationCacheAvailability,
  type AnnotationCacheBuildReport,
  type AnnotationCacheBuildProgress,
  type AnnotationCacheState,
  type AnalysisBuildProgress,
  type CorpusOverview as AnalysisCorpus,
  listen,
} from "../ipc";
import { GenerationCounter, type Generation } from "../../../app/generation";
import {
  appendAnnotationTokenProgressSample,
  annotationLayerControlViewModel,
  type AnnotationLayerTransientState,
} from "../annotation/control";
import { dom } from "../../../app/dom";
import { effectiveCleaningConfig } from "../../prepare/cleaning/effective-config";
import { state } from "../../../app/state";
import {
  activeProjectLanguageLabel,
  formatNumber,
  inputTextSourceLabel,
  pluralise,
  processedSourceCount,
  refreshStageNavigator,
  renderAnalyseWorkbench,
  setActiveAnalysisToolForBuild,
} from "./shell";
import { renderFrequencyList, resetFrequencyCorpusState } from "./frequency";

let analysisBuildInProgress = false;

let analysisBuildProgress: AnalysisBuildProgress | null = null;

let analysisBuildGeneration = new GenerationCounter<"analysis-build">();

let analysisProgressUnlisten: (() => void) | null = null;

let analysisNotice: { kind: "info" | "warning"; message: string } | null = null;

let annotationCacheAvailability: AnnotationCacheAvailability = { state: "missing", detail: null };

let annotationCacheProbeGeneration = new GenerationCounter<"annotation-cache-probe">();

let annotationCacheProbeKey: string | null = null;

let annotationCacheProbePending = false;

let annotationBuildTransient: AnnotationLayerTransientState | null = null;

let annotationBuildGeneration = new GenerationCounter<"annotation-build">();

let annotationBuildStartedAt: number | null = null;

let annotationBuildAnnotatingStartedAt: number | null = null;

let annotationBuildTimer: number | null = null;

let lastAnnotationBuildReport: AnnotationCacheBuildReport | null = null;

let annotationProgressUnlisten: (() => void) | null = null;

function analysisBuildSignature(): string {
  return JSON.stringify({
    projectRoot: state.activeProject?.rootPath ?? null,
    corpusVersion: state.currentCorpusVersion,
    sourceManifestVersion: state.sourceManifestVersion,
    cleaningConfig: effectiveCleaningConfig(),
  });
}

export function analysisIsStale(corpus: AnalysisCorpus | null = state.analysisCorpus): boolean {
  if (!corpus) {
    return false;
  }
  return corpus.buildSignature !== analysisBuildSignature();
}

export function analysisStatus(): "not-built" | "ready" | "stale" {
  if (!state.analysisCorpus) {
    return "not-built";
  }
  return analysisIsStale() ? "stale" : "ready";
}

function readyAnnotationCacheAvailability(): AnnotationCacheAvailability {
  return { state: "ready", detail: null };
}

export function resetAnnotationLayerState(): void {
  annotationCacheAvailability = { state: "missing", detail: null };
  annotationCacheProbeGeneration.invalidate();
  annotationCacheProbeKey = null;
  annotationCacheProbePending = false;
  annotationBuildTransient = null;
  annotationBuildGeneration.invalidate();
  annotationBuildStartedAt = null;
  lastAnnotationBuildReport = null;
  if (annotationBuildTimer !== null) {
    window.clearInterval(annotationBuildTimer);
    annotationBuildTimer = null;
  }
}

function annotationCacheProbeSignature(corpus: AnalysisCorpus): string {
  return [
    state.currentCorpusVersion,
    corpus.buildSignature,
    corpus.processingConfigSignature,
  ].join(":");
}

export function renderAnalyseStatus(): void {
  const status = analysisStatus();
  const corpus = state.analysisCorpus;
  const processedCount = processedSourceCount();

  dom.analyseStatusChip.dataset.status = status;
  dom.analyseBuildButton.textContent = corpus ? "Rebuild surface analysis" : "Build surface analysis";

  if (analysisBuildInProgress) {
    dom.analyseStatusChip.textContent = "Building surface analysis";
    dom.analyseStatusDetail.textContent = analysisBuildProgressText(analysisBuildProgress);
    dom.analyseBuildButton.textContent = "Cancel build";
    dom.analyseBuildButton.disabled = false;
    dom.analyseBuildNote.textContent = analysisBuildProgressText(analysisBuildProgress);
    return;
  }

  if (status === "ready" && corpus) {
    dom.analyseStatusChip.textContent =
      `Built ${new Date(corpus.builtAt).toLocaleString()} from ${inputTextSourceLabel(corpus.inputTextSource)}`;
    dom.analyseStatusDetail.textContent =
      `${pluralise(corpus.documentCount, "document")} - ${formatNumber(corpus.totalTokens)} tokens`;
    dom.analyseBuildNote.textContent = "Rebuilds surface analysis from the current project text.";
    return;
  }

  if (status === "stale") {
    dom.analyseStatusChip.textContent = "Surface analysis stale";
    dom.analyseStatusDetail.textContent =
      "Prepare-side inputs changed after the surface analysis was built. Rebuild to refresh the counts.";
    dom.analyseBuildNote.textContent = "Rebuild keeps the surface analysis data in memory for this session.";
    return;
  }

  dom.analyseStatusChip.textContent = "Surface analysis not built";
  dom.analyseStatusDetail.textContent = processedCount > 0
    ? "Processed text is available. Build surface analysis to inspect descriptive counts."
    : "Surface analysis not built yet.";
  dom.analyseBuildNote.textContent = "Builds surface analysis from the current project text.";
}

function analysisPhaseLabel(progress: AnalysisBuildProgress): string {
  switch (progress.currentPhase) {
    case "loadingSources": return "loading sources";
    case "readingFiles": return "reading files";
    case "tokenising": return "tokenising";
    case "computingDispersion": return "computing dispersion";
    case "finalisingSummaries": return "finalising summaries";
  }
}

function analysisBuildProgressText(progress: AnalysisBuildProgress | null): string {
  if (!progress) {
    return "Building surface analysis...";
  }
  return `${formatNumber(progress.filesDone)} / ${formatNumber(progress.filesTotal)} sources · ${formatNumber(progress.tokensSeen)} tokens · ${analysisPhaseLabel(progress)}`;
}

export function renderAnalysisNotice(): void {
  dom.analyseInputBanner.classList.add("hidden");
  dom.analyseInputBanner.classList.remove("warning");
  dom.analyseInputBanner.textContent = "";

  const corpus = state.analysisCorpus;
  if (!corpus) {
    if (analysisNotice) {
      dom.analyseInputBanner.textContent = analysisNotice.message;
      dom.analyseInputBanner.classList.toggle("warning", analysisNotice.kind === "warning");
      dom.analyseInputBanner.classList.remove("hidden");
    }
    return;
  }

  if (analysisIsStale(corpus)) {
    dom.analyseInputBanner.textContent =
      "The current surface analysis is stale because Prepare-side data or cleaning settings changed.";
    dom.analyseInputBanner.classList.add("warning");
    dom.analyseInputBanner.classList.remove("hidden");
    return;
  }

  if (analysisNotice) {
    dom.analyseInputBanner.textContent = analysisNotice.message;
    dom.analyseInputBanner.classList.toggle("warning", analysisNotice.kind === "warning");
    dom.analyseInputBanner.classList.remove("hidden");
    return;
  }

  if (corpus.population.usingSourceTextFallback > 0) {
    dom.analyseInputBanner.textContent =
      `${pluralise(corpus.population.usingSourceTextFallback, "source")} used native/extracted text because processed text could not be produced.`;
    dom.analyseInputBanner.classList.add("warning");
    dom.analyseInputBanner.classList.remove("hidden");
  }
}

function annotationCacheAvailabilityForUi(corpus: AnalysisCorpus | null = state.analysisCorpus): AnnotationCacheAvailability {
  if (corpus?.annotationAvailable) {
    return readyAnnotationCacheAvailability();
  }
  if (corpus && annotationCacheAvailability.state === "ready") {
    return { state: "missing", detail: annotationCacheAvailability.detail };
  }
  return annotationCacheAvailability;
}

export function annotationCacheStateForUi(corpus: AnalysisCorpus | null = state.analysisCorpus): AnnotationCacheState {
  return annotationCacheAvailabilityForUi(corpus).state;
}

async function refreshAnnotationCacheAvailability(
  probeKey: string,
  generation: Generation<"annotation-cache-probe">,
): Promise<void> {
  try {
    const page = await queryAnnotatedFrequency(
      {
        selection: defaultAnalysisDocumentSelection(),
        mode: "lemmaPos",
        query: "",
        matchMode: "contains",
        minimumFrequency: 1,
        hideStopwords: false,
        caseFold: false,
        language: null,
        posFilter: null,
        sortKey: "frequency",
        sortDirection: "desc",
        offset: 0,
        limit: 1,
      },
      state.currentCorpusVersion,
      effectiveCleaningConfig(),
    );
    // The probe can outlive a corpus or cleaning change. Generation ownership is checked before its
    // cache state reaches Frequency controls.
    if (!annotationCacheProbeGeneration.isCurrent(generation)) {
      return;
    }
    annotationCacheAvailability = page.cache;
    annotationCacheProbeKey = probeKey;
  } catch {
    if (annotationCacheProbeGeneration.isCurrent(generation)) {
      annotationCacheProbeKey = probeKey;
    }
  } finally {
    if (annotationCacheProbeGeneration.isCurrent(generation)) {
      annotationCacheProbePending = false;
      if (state.activeWorkbench === "analyse") {
        renderFrequencyList();
      }
    }
  }
}

export function ensureAnnotationCacheAvailability(corpus: AnalysisCorpus | null): void {
  if (!corpus) {
    annotationCacheProbeKey = null;
    return;
  }
  if (analysisIsStale(corpus)) {
    return;
  }
  const probeKey = annotationCacheProbeSignature(corpus);
  if (corpus.annotationAvailable) {
    annotationCacheAvailability = readyAnnotationCacheAvailability();
    annotationCacheProbeKey = probeKey;
    return;
  }
  if (annotationBuildTransient?.status === "building") {
    return;
  }
  if (annotationCacheProbePending || annotationCacheProbeKey === probeKey) {
    return;
  }
  annotationCacheProbePending = true;
  const generation = annotationCacheProbeGeneration.next();
  void refreshAnnotationCacheAvailability(probeKey, generation);
}

function annotationLayerControlInput() {
  const corpus = state.analysisCorpus;
  const cache = annotationCacheAvailabilityForUi(corpus);
  const annotatedDocumentCount = lastAnnotationBuildReport?.documentsAnnotated
    ?? (corpus?.annotationAvailable ? corpus.documentCount : null);
  return {
    cache,
    transient: annotationBuildTransient,
    surfaceAnalysisBuilt: corpus !== null,
    surfaceAnalysisRequired: false,
    languageLabel: activeProjectLanguageLabel(),
    annotatedDocumentCount,
  };
}

export function renderFrequencyAnnotationLayer(): void {
  const view = annotationLayerControlViewModel(annotationLayerControlInput());
  dom.frequencyAnnotationLayer.dataset.status = view.status;
  dom.frequencyAnnotationLayer.classList.remove("is-hidden");
  dom.frequencyAnnotationLayer.replaceChildren();

  const copy = document.createElement("div");
  copy.className = "annotation-layer-copy";

  const title = document.createElement("p");
  title.className = "annotation-layer-title";
  title.textContent = view.title;

  const help = document.createElement("p");
  help.className = "annotation-layer-help";
  help.textContent = view.help;

  copy.append(title, help);

  if (view.progress) {
    const progress = document.createElement("div");
    progress.className = "annotation-layer-progress";

    const progressLabel = document.createElement("p");
    progressLabel.className = "annotation-layer-progress-label";
    progressLabel.textContent = view.progress.phaseLabel;

    const bar = document.createElement("div");
    bar.className = `annotation-layer-progress-bar is-${view.progress.kind}`;
    bar.setAttribute("role", "progressbar");
    bar.setAttribute("aria-label", view.progress.phaseLabel);

    const fill = document.createElement("span");
    fill.className = "annotation-layer-progress-fill";
    if (view.progress.kind === "determinate") {
      const percent = view.progress.percent ?? 0;
      bar.setAttribute("aria-valuemin", "0");
      bar.setAttribute("aria-valuemax", "100");
      bar.setAttribute("aria-valuenow", `${percent}`);
      fill.style.width = `${percent}%`;
    }
    bar.append(fill);

    const meta = document.createElement("p");
    meta.className = "annotation-layer-progress-meta";
    meta.textContent = view.progress.kind === "determinate"
      ? `${view.progress.percentLabel} · ${view.progress.filesLine} · ETA ${view.progress.etaLabel}`
      : view.progress.etaLabel;

    progress.append(progressLabel, bar, meta);
    if (view.progress.heartbeatLine) {
      const heartbeat = document.createElement("p");
      heartbeat.className = "annotation-layer-progress-meta";
      heartbeat.textContent = view.progress.heartbeatLine;
      progress.append(heartbeat);
    }
    copy.append(progress);
  }

  const action = document.createElement("button");
  action.type = "button";
  action.className = `${view.action === "cancel" ? "secondary-btn" : "primary-btn"} annotation-layer-action`;
  action.textContent = view.actionLabel;
  action.disabled = view.actionDisabled;
  action.dataset.annotationLayerAction = view.action;
  action.title = view.actionDisabledReason ?? "";

  dom.frequencyAnnotationLayer.append(copy, action);
}

function stopAnnotationBuildTimer(): void {
  if (annotationBuildTimer !== null) {
    window.clearInterval(annotationBuildTimer);
    annotationBuildTimer = null;
  }
}

function refreshAnnotationBuildElapsed(): void {
  if (annotationBuildTransient?.status !== "building" || annotationBuildStartedAt === null) {
    return;
  }
  const now = Date.now();
  const annotatingElapsedMs =
    annotationBuildTransient.progress?.current_phase.type === "Annotating"
      && annotationBuildAnnotatingStartedAt !== null
      ? now - annotationBuildAnnotatingStartedAt
      : null;
  annotationBuildTransient = {
    status: "building",
    elapsedMs: now - annotationBuildStartedAt,
    annotatingElapsedMs,
    cancellationRequested: annotationBuildTransient.cancellationRequested,
    progress: annotationBuildTransient?.status === "building" ? annotationBuildTransient.progress : null,
    progressSamples: annotationBuildTransient?.status === "building"
      && annotationBuildTransient.progress?.current_phase.type === "Annotating"
      ? appendAnnotationTokenProgressSample(
          annotationBuildTransient.progressSamples,
          {
            atMs: now,
            tokensDone: annotationBuildTransient.progress.tokens_done,
          },
        )
      : [],
  };
  renderFrequencyAnnotationLayer();
}

function startAnnotationBuildTimer(): void {
  stopAnnotationBuildTimer();
  annotationBuildTimer = window.setInterval(refreshAnnotationBuildElapsed, 1000);
}

function annotationBuildRequestIndices(): number[] | null {
  return state.activeProject ? null : state.allFiles.map((_, index) => index);
}

function annotationBuildSourceCount(indices: number[] | null): number {
  return indices?.length ?? state.projectCorpusFileCount;
}

function annotationBuildErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "type" in error) {
    const typed = error as { type: string; error?: string; languages?: string[] };
    switch (typed.type) {
      case "cancelled":
        return "Annotation build cancelled.";
      case "missingCacheRoot":
        return "Missing annotation storage for this project.";
      case "surfaceBuildFailed":
        return typed.error ?? "Surface build failed.";
      case "unsupportedLanguages":
        return `No annotation model is available for language(s): ${(typed.languages ?? []).join(", ")}.`;
      case "annotationFailed":
      case "manifestError":
      case "cacheWriteError":
        return typed.error ?? "Annotation build failed.";
    }
  }
  return `${error}`;
}

function annotationBuildWasCancelled(error: unknown): boolean {
  return typeof error === "object" && error !== null && "type" in error
    ? (error as { type: string }).type === "cancelled"
    : `${error}`.toLowerCase().includes("cancelled");
}

async function refreshSurfaceAnalysisAfterAnnotationBuild(
  generation: Generation<"annotation-build">,
): Promise<void> {
  if (!state.analysisCorpus) {
    return;
  }
  const indices = annotationBuildRequestIndices();
  const builtOverview = await buildAnalysisCorpus(
    indices,
    state.currentCorpusVersion,
    analysisBuildSignature(),
    effectiveCleaningConfig(),
  );
  if (!annotationBuildGeneration.isCurrent(generation)) {
    return;
  }
  const overview = await computeCorpusOverview(effectiveCleaningConfig()).catch(() => builtOverview);
  if (!annotationBuildGeneration.isCurrent(generation)) {
    return;
  }
  state.analysisCorpus = overview;
  resetFrequencyCorpusState(false);
  annotationCacheAvailability = overview.annotationAvailable
    ? readyAnnotationCacheAvailability()
    : annotationCacheAvailability;
  annotationCacheProbeKey = null;
}

export async function handleAnnotationLayerAction(): Promise<void> {
  const view = annotationLayerControlViewModel(annotationLayerControlInput());
  if (view.actionDisabled) {
    return;
  }

  if (view.action === "cancel") {
    if (annotationBuildTransient?.status === "building") {
      annotationBuildTransient = {
        ...annotationBuildTransient,
        cancellationRequested: true,
      };
    }
    dom.statusBar.textContent = "Cancelling annotation build...";
    renderFrequencyList();
    await cancelAnnotationBuild().catch(() => undefined);
    renderFrequencyList();
    return;
  }

  const indices = annotationBuildRequestIndices();
  if (annotationBuildSourceCount(indices) === 0) {
    annotationBuildTransient = {
      status: "failed",
      message: "No text is available for annotations.",
    };
    dom.statusBar.textContent = "No text is available for annotations.";
    renderFrequencyList();
    return;
  }

  const generation = annotationBuildGeneration.next();
  annotationCacheProbeGeneration.invalidate();
  annotationCacheProbePending = false;
  annotationBuildStartedAt = Date.now();
  annotationBuildAnnotatingStartedAt = null;
  annotationBuildTransient = {
    status: "building",
    elapsedMs: 0,
    annotatingElapsedMs: null,
    cancellationRequested: false,
    progress: null,
    progressSamples: [],
  };
  lastAnnotationBuildReport = null;
  startAnnotationBuildTimer();
  renderFrequencyList();
  dom.statusBar.textContent = "Building annotations...";

  try {
    const report = await buildAnnotationCache(
      indices,
      state.currentCorpusVersion,
      analysisBuildSignature(),
      effectiveCleaningConfig(),
    );
    if (!annotationBuildGeneration.isCurrent(generation)) {
      return;
    }

    lastAnnotationBuildReport = report;
    annotationCacheAvailability = readyAnnotationCacheAvailability();
    annotationCacheProbeKey = null;

    if (state.analysisCorpus) {
      dom.statusBar.textContent = "Refreshing surface analysis for annotations...";
      await refreshSurfaceAnalysisAfterAnnotationBuild(generation);
      if (!annotationBuildGeneration.isCurrent(generation)) {
        return;
      }
    }

    annotationBuildTransient = null;
    const reportSummary =
      `${formatNumber(report.documentsAnnotated)} annotated documents · ${formatNumber(report.totalAnnotatedTermCountRows)} rows`;
    dom.statusBar.textContent = state.analysisCorpus
      ? `Annotations ready. ${reportSummary}.`
      : `Annotations built. ${reportSummary}. Build surface analysis to inspect lemma and POS frequency.`;
  } catch (error) {
    if (!annotationBuildGeneration.isCurrent(generation)) {
      return;
    }
    const message = annotationBuildErrorMessage(error);
    annotationBuildTransient = annotationBuildWasCancelled(error)
      ? { status: "cancelled" }
      : { status: "failed", message };
    dom.statusBar.textContent = message;
  } finally {
    if (annotationBuildGeneration.isCurrent(generation)) {
      stopAnnotationBuildTimer();
      annotationBuildStartedAt = null;
      annotationBuildAnnotatingStartedAt = null;
      renderFrequencyList();
      refreshStageNavigator();
    }
  }
}

export async function handleBuildAnalysisCorpus(): Promise<void> {
  if (analysisBuildInProgress) {
    dom.analyseBuildButton.disabled = true;
    dom.analyseBuildNote.textContent = "Cancelling analysis build...";
    dom.statusBar.textContent = "Cancelling analysis build...";
    try {
      await cancelAnalysisBuild();
    } finally {
      dom.analyseBuildButton.disabled = false;
    }
    return;
  }

  const indices = state.activeProject ? null : state.allFiles.map((_, index) => index);
  const buildSignature = analysisBuildSignature();

  analysisNotice = null;

  if ((indices?.length ?? state.projectCorpusFileCount) === 0) {
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    analysisNotice = {
      kind: "warning",
      message: "No text available for analysis. Go to Prepare, extract/process sources, then return to Analyse.",
    };
    dom.statusBar.textContent = "No text available for analysis.";
    renderAnalyseWorkbench();
    refreshStageNavigator();
    return;
  }

  analysisBuildInProgress = true;
  analysisBuildProgress = null;
  const buildGeneration = analysisBuildGeneration.next();
  state.analysisCorpus = null;
  resetFrequencyCorpusState();
  renderAnalyseWorkbench();
  dom.statusBar.textContent = "Building surface analysis...";

  try {
    const builtOverview = await buildAnalysisCorpus(
      indices,
      state.currentCorpusVersion,
      buildSignature,
      effectiveCleaningConfig(),
    );
    if (!analysisBuildGeneration.isCurrent(buildGeneration)) {
      return;
    }
    const overview = await computeCorpusOverview(effectiveCleaningConfig()).catch(() => builtOverview);
    state.analysisCorpus = overview;
    resetFrequencyCorpusState();
    setActiveAnalysisToolForBuild("overview");
    const sourceMessage = overview.population.usingSourceTextFallback > 0
      ? `Built from processed text with ${pluralise(overview.population.usingSourceTextFallback, "source")} using source-text fallback.`
      : "Built from processed text.";
    const skipped = overview.skippedDocuments > 0
      ? ` ${pluralise(overview.skippedDocuments, "document")} without usable text skipped.`
      : "";
    const skippedPdfs = overview.skippedPdfDocuments > 0
      ? ` ${pluralise(overview.skippedPdfDocuments, "PDF")} had no materialised text.`
      : "";
    analysisNotice = skipped || skippedPdfs
      ? { kind: "warning", message: `${sourceMessage}${skipped}${skippedPdfs}`.trim() }
      : null;
    dom.statusBar.textContent =
      `Analysis corpus built from ${inputTextSourceLabel(overview.inputTextSource)}. ${formatNumber(overview.documentCount)} documents · ${formatNumber(overview.totalTokens)} tokens · ${formatNumber(overview.totalTypes)} types.`;
  } catch (error) {
    if (!analysisBuildGeneration.isCurrent(buildGeneration)) {
      return;
    }
    state.analysisCorpus = null;
    resetFrequencyCorpusState();
    const message = `${error}`;
    analysisNotice = {
      kind: "warning",
      message: message.toLowerCase().includes("cancelled")
        ? "Analysis corpus build cancelled."
        : `Analysis build failed: ${message}`,
    };
    dom.statusBar.textContent = analysisNotice.message;
  } finally {
    if (!analysisBuildGeneration.isCurrent(buildGeneration)) {
      return;
    }
    analysisBuildInProgress = false;
    analysisBuildProgress = null;
    dom.analyseBuildButton.disabled = false;
    renderAnalyseWorkbench();
    refreshStageNavigator();
  }
}

export async function initialiseAnalysisBuildState(): Promise<void> {
  if (analysisProgressUnlisten) {
    analysisProgressUnlisten();
  }
  if (annotationProgressUnlisten) {
    annotationProgressUnlisten();
  }

  annotationProgressUnlisten = await listen<AnnotationCacheBuildProgress>("annotation-build-progress", (event) => {
    if (annotationBuildTransient?.status !== "building") {
      return;
    }
    const now = Date.now();
    const previousPhase = annotationBuildTransient.progress?.current_phase.type ?? null;
    const nextPhase = event.payload.current_phase.type;
    if (nextPhase === "Annotating" && (previousPhase !== "Annotating" || annotationBuildAnnotatingStartedAt === null)) {
      annotationBuildAnnotatingStartedAt = now;
    }
    const annotatingElapsedMs =
      nextPhase === "Annotating" && annotationBuildAnnotatingStartedAt !== null
        ? now - annotationBuildAnnotatingStartedAt
        : null;
    const progressSamples = nextPhase === "Annotating"
      ? appendAnnotationTokenProgressSample(
          previousPhase === "Annotating" ? annotationBuildTransient.progressSamples : [],
          {
            atMs: now,
            tokensDone: event.payload.tokens_done,
          },
        )
      : [];
    annotationBuildTransient = {
      ...annotationBuildTransient,
      annotatingElapsedMs,
      progress: event.payload,
      progressSamples,
    };
    renderFrequencyAnnotationLayer();
  });

  analysisProgressUnlisten = await listen<AnalysisBuildProgress>("analysis-build-progress", (event) => {
    if (!analysisBuildInProgress) {
      return;
    }
    analysisBuildProgress = event.payload;
    const progressText = analysisBuildProgressText(event.payload);
    dom.statusBar.textContent = `Building surface analysis... ${progressText}`;
    if (state.activeWorkbench === "analyse") {
      renderAnalyseStatus();
    }
  });

  try {
    const status = await getAnalysisBuildStatus(effectiveCleaningConfig());
    analysisBuildInProgress = status.state === "building";
    analysisBuildProgress = status.progress;
    if (status.state === "ready" && status.overview && !state.analysisCorpus) {
      state.analysisCorpus = await computeCorpusOverview(effectiveCleaningConfig()).catch(() => status.overview);
    }
  } catch {
    // The Analyse screen remains usable even if startup status recovery is unavailable.
  }
}

export function currentAnalysisNotice(): { kind: "info" | "warning"; message: string } | null {
  return analysisNotice;
}

export function setAnalysisNotice(notice: { kind: "info" | "warning"; message: string } | null): void {
  analysisNotice = notice;
}

export function updateAnnotationCacheAvailabilityFromFrequencyPage(
  cache: AnnotationCacheAvailability,
): void {
  annotationCacheAvailability = cache;
  annotationCacheProbeKey = null;
}

export function resetAnalysisBuildStateForWorkbench(): void {
  analysisBuildGeneration.invalidate();
  if (analysisBuildInProgress) {
    void cancelAnalysisBuild();
  }
  analysisBuildInProgress = false;
  analysisBuildProgress = null;
  analysisNotice = null;
}
