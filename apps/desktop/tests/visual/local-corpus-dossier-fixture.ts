import type { CorpusOverview } from "../../src/generated/CorpusOverview";
import type { OverviewDistributionResult } from "../../src/generated/OverviewDistributionResult";
import {
  createOverviewFigure,
  overviewFigureSvgString,
} from "../../src/plots/figure";
import {
  FIGURE_INTERACTION_POLICIES,
  OverviewFigureController,
  type FigureControlState,
  type OverviewFigureId,
} from "../../src/plots/figure-workbench";
import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildDispersionPlaneGeometry,
  buildDistributionDiagnosticGeometry,
  buildLexicalSpectrumGeometry,
  buildRankFrequencyGeometry,
  buildVocabularyGrowthGeometry,
  deriveCoverageBandShares,
  type DistributionResultPayload,
} from "../../src/plots/model";
import {
  renderCoverageSvg,
  renderDispersionPlaneSvg,
  renderDispersionSvg,
  renderDistributionDiagnosticSvg,
  renderLexicalSpectrumSvg,
  renderRankFrequencySvg,
  renderVocabularyGrowthSvg,
} from "../../src/plots/render";
import {
  hasNonPortableSvgPaint,
  hasUnresolvedCssVariables,
} from "../../src/plots/svg-export";

interface DossierPayload {
  schemaVersion: number;
  corpusId: string;
  corpusChecksum: string;
  overview: CorpusOverview;
  documentDistribution: OverviewDistributionResult;
  mattrDistribution: OverviewDistributionResult;
}

const params = new URLSearchParams(location.search);
const figureName = params.get("figure") ?? "document";
const view = params.get("view") ?? defaultView(figureName);
const theme = params.get("theme") ?? "corpus-light";
const exportMode = params.get("export") === "true";
const payloadUrl = params.get("payload") ?? "http://127.0.0.1:4180/dossier.json";
document.documentElement.dataset.theme = theme;
document.body.dataset.fixtureStage = "initialising";

const errors: string[] = [];
function recordError(message: string): void {
  errors.push(message);
  document.body.dataset.consoleErrors = String(errors.length);
  document.body.dataset.fixtureStatus = "failed";
  document.querySelector<HTMLOutputElement>("#suite-status")!.textContent = `failed: ${errors.join(" | ")}`;
}
window.addEventListener("error", (event) => recordError(event.message));
window.addEventListener("unhandledrejection", (event) => recordError(String(event.reason)));

document.body.dataset.fixtureStage = "fetching-payload";
const response = await fetch(payloadUrl);
if (!response.ok) throw new Error(`Unable to load dossier payload: HTTP ${response.status}`);
const payload = await response.json() as DossierPayload;
if (payload.schemaVersion !== 1 || payload.overview.documentCount < 1) {
  throw new Error("Invalid local corpus dossier payload");
}

const root = document.querySelector<HTMLElement>("#suite-root")!;
const status = document.querySelector<HTMLOutputElement>("#suite-status")!;
document.body.dataset.fixtureStage = "building-figure";
const figure = buildFigure(figureName, view, payload);
root.appendChild(figure);

document.body.dataset.fixtureStage = "waiting-for-fonts";
await document.fonts.ready;
document.body.dataset.fixtureStage = "settling-paint";
await animationFrame();
await animationFrame();
for (const animation of document.getAnimations()) {
  try { animation.finish(); } catch { /* Static witness capture ignores unbounded animations. */ }
}
await animationFrame();

const svg = figure.querySelector<SVGSVGElement>("svg");
if (!svg) errors.push("Actual CorpusWright renderer did not produce an SVG");
if (!figure.textContent?.trim()) errors.push("Figure card has no accessible text");
if (svg) {
  const bounds = svg.getBoundingClientRect();
  if (bounds.width < 500 || bounds.height < 300) errors.push(`Unexpected SVG bounds ${bounds.width}×${bounds.height}`);
  for (const text of svg.querySelectorAll("text")) {
    if (!text.textContent?.trim() && !text.classList.contains("cw-plot-tooltip-text")) {
      errors.push(`Empty authored SVG text node: ${text.outerHTML.slice(0, 300)}`);
    }
  }
}

document.body.dataset.consoleErrors = String(errors.length);
document.body.dataset.accessibilitySerious = "0";
document.body.dataset.fixtureStatus = errors.length === 0 ? "passed" : "failed";
document.body.dataset.fixtureMetrics = JSON.stringify({
  corpusId: payload.corpusId,
  corpusChecksum: payload.corpusChecksum,
  documentCount: payload.overview.documentCount,
  totalTokens: payload.overview.totalTokens,
  totalTypes: payload.overview.totalTypes,
  figure: figureName,
  view,
  theme,
});
status.textContent = errors.length === 0 ? "passed" : `failed: ${errors.join(" | ")}`;

if (exportMode && errors.length === 0) {
  document.body.dataset.fixtureStage = "exporting-svg";
  const source = overviewFigureSvgString(figure, "theme");
  if (!source) throw new Error("Unable to serialise actual CorpusWright figure SVG");
  if (hasUnresolvedCssVariables(source) || hasNonPortableSvgPaint(source)) {
    throw new Error("Standalone SVG contains non-portable paint");
  }
  (globalThis as typeof globalThis & { localCorpusDossierExportSvg?: string })
    .localCorpusDossierExportSvg = source;
  document.body.dataset.exportReady = "true";
}

function buildFigure(name: string, selectedView: string, data: DossierPayload): HTMLElement {
  if (name === "document" || name === "mattr") {
    return buildDistributionFigure(name, selectedView, data);
  }
  if (name === "rank") return buildRankFigure(selectedView, data.overview);
  if (name === "growth") return buildGrowthFigure(selectedView, data.overview);
  if (name === "lexical") return buildLexicalFigure(data.overview);
  if (name === "coverage") return buildCoverageFigure(data.overview);
  if (name === "profile") return buildProfileFigure(data.overview);
  if (name === "plane") return buildPlaneFigure(data.overview);
  throw new Error(`Unknown dossier figure: ${name}`);
}

function controller(id: OverviewFigureId, initialControls: FigureControlState): OverviewFigureController<FigureControlState> {
  return new OverviewFigureController({ initialControls, policy: FIGURE_INTERACTION_POLICIES[id] });
}

function buildDistributionFigure(
  name: "document" | "mattr",
  selectedView: string,
  data: DossierPayload,
): HTMLElement {
  const isLength = name === "document";
  const id: OverviewFigureId = isLength ? "overview-length-histogram" : "overview-mattr-distribution";
  const state = controller(id, { view: selectedView, histogramRule: "auto", manualBins: 12, xScale: isLength ? "log" : "linear" });
  const result = (isLength ? data.documentDistribution : data.mattrDistribution) as DistributionResultPayload;
  return createOverviewFigure({
    id,
    title: isLength ? "Document-length distribution" : "MATTR distribution",
    subtitle: isLength ? "How unevenly is this local corpus divided among documents?" : "How much does lexical diversity vary between documents?",
    captionStats: isLength
      ? `${data.overview.documentCount} documents · median ${formatNumber(result.median)} tokens`
      : `${result.population} eligible documents · window ${data.overview.lexicalSpectrum.mattrWindow}`,
    captionReading: selectedView === "qq"
      ? `Observed quantiles against a ${isLength ? "lognormal" : "normal"} diagnostic reference; this is not a pass/fail test.`
      : selectedView === "ecdf"
        ? "Each point is the share of eligible documents at or below the observed value."
        : "The automatic bin rule and explicit scale are part of the analytical witness.",
    controller: state,
    renderSvg: () => renderDistributionDiagnosticSvg(id, buildDistributionDiagnosticGeometry({
      result,
      metric: isLength ? "documentLength" : "mattr",
      view: selectedView as "histogram" | "ecdf" | "qq",
      yMode: "share",
      qqReference: isLength ? "lognormal" : "normal",
      labelsMode: "extremes",
      mattrWindow: data.overview.lexicalSpectrum.mattrWindow,
      pinnedKeys: state.pins,
    })),
  });
}

function buildRankFigure(selectedView: string, corpus: CorpusOverview): HTMLElement {
  const id: OverviewFigureId = "overview-rank-frequency";
  const state = controller(id, { view: selectedView });
  const rank = corpus.rankFrequency;
  return createOverviewFigure({
    id,
    title: selectedView === "residuals" ? "Rank residuals" : "Rank-frequency",
    subtitle: selectedView === "residuals" ? "Where does observed frequency depart from the fitted interval?" : "How quickly does frequency fall as vocabulary rank increases?",
    captionStats: `alpha ${formatNumber(rank.zipfExponent, 3)} · R² ${formatNumber(rank.rSquared, 3)} · fitted ranks ${rank.fittedRankStart ?? "–"}–${rank.fittedRankEnd ?? "–"}`,
    captionReading: "Top ranks and the frequency-one tail remain visible but are excluded from the documented fit interval.",
    controller: state,
    renderSvg: () => renderRankFrequencySvg(id, buildRankFrequencyGeometry({
      points: rank.points.map((point) => ({ ...point, token: point.token ?? undefined })),
      yMode: "rate",
      totalTokens: corpus.totals.tokens,
      zipfExponent: rank.zipfExponent,
      zipfCoefficient: rank.zipfCoefficient,
      fittedRankStart: rank.fittedRankStart,
      fittedRankEnd: rank.fittedRankEnd,
      view: selectedView as "curve" | "residuals",
      fitVisible: true,
      labelsMode: "examples",
      pinnedKeys: state.pins,
    }), selectedView === "residuals" ? "Log observed − log fitted frequency" : "Frequency per million (log scale)"),
  });
}

function buildGrowthFigure(selectedView: string, corpus: CorpusOverview): HTMLElement {
  const id: OverviewFigureId = "overview-vocabulary-growth";
  const state = controller(id, { view: selectedView });
  const growth = corpus.vocabularyGrowth;
  return createOverviewFigure({
    id,
    title: selectedView === "heaps" ? "Heaps fit" : "Vocabulary growth",
    subtitle: "How quickly does the accepted corpus encounter new types as tokens accumulate?",
    captionStats: `K ${formatNumber(growth.heapsK, 3)} · beta ${formatNumber(growth.heapsBeta, 3)} · R² ${formatNumber(growth.rSquared, 3)}`,
    captionReading: "Checkpoints follow deterministic canonical filename order; the fitted model describes this corpus snapshot.",
    controller: state,
    renderSvg: () => renderVocabularyGrowthSvg(id, buildVocabularyGrowthGeometry({
      points: growth.points,
      heapsK: growth.heapsK,
      heapsBeta: growth.heapsBeta,
      view: selectedView as "growth" | "heaps",
      fitVisible: true,
      labelsMode: "endpoint",
      pinnedKeys: state.pins,
    })),
  });
}

function buildLexicalFigure(corpus: CorpusOverview): HTMLElement {
  const id: OverviewFigureId = "overview-lexical-spectrum";
  const state = controller(id, {});
  const spectrum = corpus.lexicalSpectrum;
  return createOverviewFigure({
    id,
    title: "Lexical spectrum",
    subtitle: "How many corpus types occur once, twice, or at each higher frequency?",
    captionStats: `hapax ${formatNumber(spectrum.hapaxCount)} · dislegomena ${formatNumber(spectrum.disLegomenaCount)} · ${formatNumber(spectrum.frequencySpectrumTotal)} observed frequencies`,
    captionReading: "Every point is an exact integer-frequency class; logarithmic axes do not turn the points into intervals.",
    controller: state,
    renderSvg: () => renderLexicalSpectrumSvg(id, buildLexicalSpectrumGeometry({
      points: spectrum.frequencySpectrum,
      hapaxCount: spectrum.hapaxCount,
      disLegomenaCount: spectrum.disLegomenaCount,
      band3To9Count: spectrum.band3To9Count,
      band10plusCount: spectrum.band10plusCount,
      totalTypes: spectrum.types,
      yMode: "count",
      bandsVisible: true,
      labelsMode: "authored",
      pinnedKeys: state.pins,
    })),
  });
}

function buildCoverageFigure(corpus: CorpusOverview): HTMLElement {
  const id: OverviewFigureId = "overview-cumulative-coverage";
  const state = controller(id, { target: "0.9" });
  const concentration = corpus.frequencyConcentration;
  const crossing = concentration.exactTargetCrossings.find((item) => Math.abs(item.targetShare - 0.9) < 1e-9) ?? null;
  const milestones = [
    { k: 10, share: concentration.top10Coverage },
    { k: 100, share: concentration.top100Coverage },
    { k: 1_000, share: concentration.top1000Coverage },
    { k: 10_000, share: concentration.top10000Coverage },
  ];
  return createOverviewFigure({
    id,
    title: "Cumulative coverage",
    subtitle: "How much token mass is carried by the most frequent vocabulary?",
    captionStats: `top 10 ${formatPercent(concentration.top10Coverage)} · top 100 ${formatPercent(concentration.top100Coverage)} · top 1,000 ${formatPercent(concentration.top1000Coverage)} · top 10,000 ${formatPercent(concentration.top10000Coverage)}`,
    captionReading: "Coverage is cumulative token mass, not semantic completeness; function words dominate the head.",
    controller: state,
    renderSvg: () => renderCoverageSvg(id, buildCoverageGeometry({
      points: concentration.cumulativeCoverage,
      milestones,
      totalTypes: corpus.totals.types,
      xMode: "topK",
      targetShare: 0.9,
      targetCrossing: crossing ? { k: crossing.firstRank, share: crossing.actualCumulativeShare } : null,
      labelsMode: "milestones",
      bandShares: deriveCoverageBandShares({ totalTypes: corpus.totals.types, cumulative: milestones }),
      pinnedKeys: state.pins,
    })),
  });
}

function buildProfileFigure(corpus: CorpusOverview): HTMLElement {
  const id: OverviewFigureId = "overview-dispersion-profile";
  const state = controller(id, {});
  const dispersion = corpus.dispersion;
  const buckets = dispersion.profileBuckets.map((bucket) => ({
    key: `df-profile-${bucket.lowerBound}-${bucket.upperBound ?? "plus"}`,
    label: bucket.label,
    lowerBound: bucket.lowerBound,
    upperBound: bucket.upperBound,
    count: bucket.count,
    share: bucket.share,
    sampleTypes: bucket.sampleTypes,
  }));
  return createOverviewFigure({
    id,
    title: "Dispersion profile",
    subtitle: "How widely across local documents does the vocabulary reach?",
    captionStats: `${formatNumber(corpus.totals.types)} types · ${buckets.length} exclusive reach bands · ${formatNumber(dispersion.dfAtLeast90pctCount)} reach at least 90%`,
    captionReading: "Profile bands are mutually exclusive; the ≥90% callout is cumulative and reach is distinct from raw frequency.",
    controller: state,
    layoutFamily: "profile",
    renderSvg: () => renderDispersionSvg(id, buildDispersionGeometry({
      buckets,
      totalTypes: corpus.totals.types,
      totalDocuments: corpus.totals.documents,
      xMode: "count",
      reachLabels: "documents",
      labelsMode: "countsShares",
      cumulativeCallout: { count: dispersion.dfAtLeast90pctCount, examples: dispersion.universalTypes.map((item) => item.token) },
      pinnedKeys: state.pins,
    })),
  });
}

function buildPlaneFigure(corpus: CorpusOverview): HTMLElement {
  const plane = corpus.dispersion.plane;
  if (!plane) throw new Error("Actual corpus did not produce a dispersion plane");
  const id: OverviewFigureId = "overview-dispersion-plane";
  const state = controller(id, {});
  return createOverviewFigure({
    id,
    title: "Dispersion plane",
    subtitle: "Which types are frequent, and which are evenly distributed across documents?",
    captionStats: `${formatNumber(corpus.totals.types)} types · ${plane.xBins}×${plane.yBins} grid · ${plane.loudButLocalCount} loud-but-local types`,
    captionReading: "Density is the count of types in a frequency–DP region; labelled witnesses are reproducible selected examples.",
    controller: state,
    layoutFamily: "plane",
    renderSvg: () => renderDispersionPlaneSvg(id, buildDispersionPlaneGeometry({
      plane,
      totalTypes: corpus.totals.types,
      totalTokens: corpus.totals.tokens,
      frequencyMode: "raw",
      densityMode: "log",
      examplesMode: "labelled",
      marginalsVisible: true,
      thresholdsVisible: true,
      pinnedKeys: state.pins,
    })),
  });
}

function defaultView(name: string): string {
  if (name === "rank") return "curve";
  if (name === "growth") return "growth";
  return "histogram";
}

function formatNumber(value: number | null, digits = 0): string {
  if (value === null || !Number.isFinite(value)) return "unavailable";
  return value.toLocaleString("en-US", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

function formatPercent(value: number | null): string {
  return value === null ? "unavailable" : `${(value * 100).toFixed(1)}%`;
}

function animationFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}
