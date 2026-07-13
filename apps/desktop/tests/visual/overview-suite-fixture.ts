import {
  createOverviewFigure,
  createOverviewFigureOptions,
  createOverviewRangeControl,
  createOverviewSelectControl,
  createOverviewSegmentedControl,
  createOverviewToggleControl,
  overviewFigureSvgString,
} from "../../src/plots/figure";
import {
  FIGURE_INTERACTION_POLICIES,
  OverviewFigureController,
  type FigureControlState,
  type FigurePaletteMode,
  type OverviewFigureId,
} from "../../src/plots/figure-workbench";
import {
  buildCoverageGeometry,
  buildDispersionGeometry,
  buildDispersionPlaneGeometry,
  buildDispersionRosterGeometry,
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
  renderDispersionRosterSvg,
  renderDispersionSvg,
  renderDistributionDiagnosticSvg,
  renderLexicalSpectrumSvg,
  renderRankFrequencySvg,
  renderVocabularyGrowthSvg,
} from "../../src/plots/render";

const params = new URLSearchParams(location.search);
const figureName = params.get("figure") ?? "document";
const view = params.get("view") ?? defaultView(figureName);
const theme = params.get("theme") ?? "corpus-light";
const target = params.get("target") ?? "0.5";
const rows = Number(params.get("rows") ?? "20");
const customBins = params.get("customBins") === "true";
const pinned = params.get("pinned") === "true";
const exportMode = params.get("export") === "true";
const palette = (params.get("palette") ?? "theme") as FigurePaletteMode;
document.documentElement.dataset.theme = theme;

const errors: string[] = [];
window.addEventListener("error", (event) => errors.push(event.message));
window.addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

const root = document.querySelector<HTMLElement>("#suite-root")!;
const status = document.querySelector<HTMLOutputElement>("#suite-status")!;
const built = buildFixtureFigure(figureName, view);
root.appendChild(built.figure);

if (customBins) {
  const disclosure = built.figure.querySelector<HTMLDetailsElement>(".cw-overview-figure-options");
  if (disclosure) disclosure.open = true;
}

await document.fonts.ready;
await animationFrame();
await animationFrame();

if (params.get("focusDensity") === "true") {
  built.figure.querySelector<SVGElement>(".cw-plot-density-cell[data-grid-row='6']:not(.is-zero)")?.focus();
}
if (params.get("exportDialog") === "true") {
  built.figure.querySelector<HTMLButtonElement>(".cw-overview-figure-export")?.click();
}

const auditFailures = auditFigure(built.figure);
const failures = [...errors, ...auditFailures];
document.body.dataset.consoleErrors = String(errors.length);
document.body.dataset.accessibilitySerious = String(auditFailures.length);
document.body.dataset.fixtureStatus = failures.length === 0 ? "passed" : "failed";
status.textContent = failures.length === 0 ? "passed" : `failed: ${failures.join(" | ")}`;

if (exportMode && failures.length === 0) {
  const source = overviewFigureSvgString(built.figure, palette);
  if (!source) throw new Error("Unable to serialise fixture SVG");
  document.body.dataset.exportReady = "true";
  document.body.innerHTML = source;
}

interface BuiltFigure {
  figure: HTMLElement;
  controller: OverviewFigureController<FigureControlState>;
}

function buildFixtureFigure(name: string, selectedView: string): BuiltFigure {
  if (name === "document" || name === "mattr") return buildDistributionFigure(name, selectedView);
  if (name === "rank") return buildRankFigure(selectedView);
  if (name === "growth") return buildGrowthFigure(selectedView);
  if (name === "lexical") return buildLexicalFigure();
  if (name === "coverage") return buildCoverageFigure();
  if (name === "profile") return buildProfileFigure();
  if (name === "roster") return buildRosterFigure();
  if (name === "plane") return buildPlaneFigure();
  throw new Error(`Unknown fixture figure: ${name}`);
}

function buildDistributionFigure(name: "document" | "mattr", selectedView: string): BuiltFigure {
  const isLength = name === "document";
  const id: OverviewFigureId = isLength ? "overview-length-histogram" : "overview-mattr-distribution";
  const state: FigureControlState = {
    view: selectedView,
    histogramRule: customBins ? "manual" : "auto",
    manualBins: 36,
    xScale: isLength ? "log" : "linear",
  };
  const controller = fixtureController(id, state);
  const result = distributionPayload(isLength);
  if (pinned) controller.togglePin(selectedView === "qq"
    ? "distribution-qq:10"
    : name === "mattr" ? "distribution-witness:1" : "distribution-bin:4");
  const controls = document.createElement("div");
  const viewControl = createOverviewSegmentedControl("View", [
    { value: "histogram", label: "Histogram" },
    { value: "ecdf", label: "ECDF" },
    { value: "qq", label: "Q–Q" },
  ], selectedView, () => undefined);
  const binning = createOverviewSegmentedControl("Binning", [
    { value: "auto", label: "Automatic" },
    { value: "manual", label: "Custom" },
  ], customBins ? "manual" : "auto", () => undefined);
  const binCount = createOverviewRangeControl("Bins", {
    minimum: 8,
    maximum: 100,
    step: 1,
    initialValue: 36,
    onChange: () => undefined,
  });
  binning.hidden = selectedView !== "histogram";
  binCount.hidden = selectedView !== "histogram" || !customBins;
  const horizontalScale = createOverviewSegmentedControl("Horizontal scale", [
    { value: "log", label: "Logarithmic" },
    { value: "linear", label: "Linear" },
  ], "log", () => undefined);
  horizontalScale.hidden = !isLength || selectedView === "qq";
  const options = createOverviewFigureOptions(binning, binCount, ...(isLength ? [horizontalScale] : []));
  options.hidden = selectedView === "qq" || !isLength && selectedView !== "histogram";
  controls.append(viewControl, options);
  const renderSvg = () => renderDistributionDiagnosticSvg(id, buildDistributionDiagnosticGeometry({
    result,
    metric: isLength ? "documentLength" : "mattr",
    view: selectedView as "histogram" | "ecdf" | "qq",
    yMode: "share",
    qqReference: isLength ? "lognormal" : "normal",
    labelsMode: "extremes",
    mattrWindow: 100,
    pinnedKeys: controller.pins,
  }));
  const stateText = selectedView === "histogram"
    ? `Histogram · ${customBins ? "custom 36 bins" : "automatic bins"} · ${isLength ? "logarithmic token scale" : "share of eligible documents"}`
    : selectedView === "ecdf" ? `ECDF · ${isLength ? "logarithmic token scale" : "linear MATTR scale"}`
      : `Q–Q · ${isLength ? "lognormal" : "normal"} reference`;
  const figure = createOverviewFigure({
    id,
    title: isLength ? "Document-length distribution" : "MATTR distribution",
    subtitle: isLength ? "How unevenly is the corpus divided among documents?" : "How much does lexical diversity vary between documents?",
    captionStats: isLength ? "100 documents · median 2,480 tokens · 3 zero-token documents" : "96 eligible documents · window 100 · median 0.612",
    captionReading: isLength
      ? "The histogram keeps zero-token documents explicit and shows positive lengths on the selected scale."
      : "MATTR compares lexical variety within equal windows; excluded short documents remain visible in the eligibility strip.",
    controls,
    controller,
    exportMetadata: () => [stateText],
    renderSvg,
  });
  return { figure, controller };
}

function buildRankFigure(selectedView: string): BuiltFigure {
  const id: OverviewFigureId = "overview-rank-frequency";
  const controller = fixtureController(id, { view: selectedView });
  if (pinned) controller.togglePin("rank:40:token-40");
  const points = Array.from({ length: 80 }, (_, index) => {
    const rank = index + 1;
    const frequency = Math.max(1, Math.round(220_000 * rank ** -1.08));
    return { rank, token: `token-${rank}`, frequency, ratePerMillion: frequency / 2_000_000 * 1_000_000 };
  });
  const controls = document.createElement("div");
  controls.appendChild(createOverviewSegmentedControl("View", [
    { value: "curve", label: "Curve" },
    { value: "residuals", label: "Residuals" },
  ], selectedView, () => undefined));
  const renderSvg = () => renderRankFrequencySvg(id, buildRankFrequencyGeometry({
    points,
    yMode: "rate",
    totalTokens: 2_000_000,
    zipfExponent: 1.08,
    zipfCoefficient: 220_000,
    fittedRankStart: 4,
    fittedRankEnd: 70,
    view: selectedView as "curve" | "residuals",
    fitVisible: true,
    labelsMode: "examples",
    pinnedKeys: controller.pins,
  }), selectedView === "residuals" ? "Log observed − log fitted frequency" : "Frequency per million (log scale)");
  const figure = createOverviewFigure({
    id,
    title: "Rank-frequency",
    subtitle: "How quickly does frequency fall as vocabulary rank increases?",
    captionStats: "alpha 1.080 · R-squared 0.984 · fitted ranks 4–70",
    captionReading: "Residuals use log observed minus log fitted raw frequency and retain the documented fitted range.",
    controls,
    controller,
    exportMetadata: () => [selectedView === "residuals" ? "Residuals · Zipf fit over ranks 4–70" : "Curve · frequency per million · Zipf fit over ranks 4–70"],
    renderSvg,
  });
  return { figure, controller };
}

function buildGrowthFigure(selectedView: string): BuiltFigure {
  const id: OverviewFigureId = "overview-vocabulary-growth";
  const controller = fixtureController(id, { view: selectedView });
  if (pinned) controller.togglePin("growth:100000");
  const points = Array.from({ length: 18 }, (_, index) => {
    const tokens = Math.round(1_000 * 1.55 ** index);
    return { tokens, types: Math.round(5.2 * tokens ** 0.67) };
  });
  const controls = document.createElement("div");
  controls.appendChild(createOverviewSegmentedControl("View", [
    { value: "growth", label: "Growth" },
    { value: "heaps", label: "Heaps fit" },
  ], selectedView, () => undefined));
  const renderSvg = () => renderVocabularyGrowthSvg(id, buildVocabularyGrowthGeometry({
    points,
    heapsK: 5.2,
    heapsBeta: 0.67,
    view: selectedView as "growth" | "heaps",
    fitVisible: true,
    labelsMode: "endpoint",
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Vocabulary growth",
    subtitle: "How quickly does the corpus encounter new types as tokens accumulate?",
    captionStats: "K 5.200 · beta 0.670 · R-squared 0.991",
    captionReading: "Observed checkpoints and the fitted Heaps model share the visible domain in both authored views.",
    controls,
    controller,
    exportMetadata: () => [selectedView === "heaps" ? "Heaps fit · observed and fitted model" : "Growth · observed and Heaps fit"],
    renderSvg,
  });
  return { figure, controller };
}

function buildLexicalFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-lexical-spectrum";
  const controller = fixtureController(id, {});
  if (pinned) controller.togglePin("spectrum:10000");
  const renderSvg = () => renderLexicalSpectrumSvg(id, buildLexicalSpectrumGeometry({
    points: [
      { frequency: 1, typeCount: 211_271 }, { frequency: 2, typeCount: 41_999 },
      { frequency: 3, typeCount: 18_220 }, { frequency: 4, typeCount: 10_300 },
      { frequency: 9, typeCount: 2_101 }, { frequency: 10, typeCount: 1_900 },
      { frequency: 100, typeCount: 220 }, { frequency: 1_000, typeCount: 18 },
      { frequency: 10_000, typeCount: 1 },
    ],
    hapaxCount: 211_271,
    disLegomenaCount: 41_999,
    band3To9Count: 37_000,
    band10plusCount: 21_500,
    totalTypes: 311_770,
    yMode: "count",
    bandsVisible: true,
    labelsMode: "authored",
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Lexical spectrum",
    subtitle: "How many types occur once, twice, or at each higher frequency?",
    captionStats: "hapax 211,271 · dis 41,999 · 9 observed frequencies",
    captionReading: "Each point is the exact type count at one observed integer frequency; class headings and exact annotations are both retained.",
    controller,
    exportMetadata: () => ["Exact frequency spectrum · logarithmic axes"],
    renderSvg,
  });
  return { figure, controller };
}

function buildCoverageFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-cumulative-coverage";
  const controller = fixtureController(id, { target });
  if (pinned) controller.togglePin("coverage:1000");
  const checkpoints = [
    { k: 1, share: 0.08 }, { k: 10, share: 0.31 }, { k: 100, share: 0.55 },
    { k: 1_000, share: 0.76 }, { k: 10_000, share: 0.91 }, { k: 100_000, share: 0.985 },
    { k: 311_770, share: 1 },
  ];
  const crossingByTarget: Record<string, { k: number; share: number }> = {
    "0.5": { k: 67, share: 0.503 }, "0.8": { k: 1_840, share: 0.801 },
    "0.9": { k: 8_940, share: 0.9004 }, "0.95": { k: 29_400, share: 0.9501 },
  };
  const controls = document.createElement("div");
  controls.appendChild(createOverviewSegmentedControl("Coverage target", [
    { value: "0.5", label: "50%" }, { value: "0.8", label: "80%" },
    { value: "0.9", label: "90%" }, { value: "0.95", label: "95%" },
  ], target, () => undefined));
  const renderSvg = () => renderCoverageSvg(id, buildCoverageGeometry({
    points: checkpoints,
    milestones: checkpoints.slice(1, 5),
    totalTypes: 311_770,
    xMode: "topK",
    targetShare: Number(target),
    targetCrossing: crossingByTarget[target] ?? crossingByTarget["0.5"]!,
    labelsMode: "milestones",
    bandShares: deriveCoverageBandShares({
      totalTypes: 311_770,
      cumulative: checkpoints.slice(1, 5),
    }),
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Cumulative coverage",
    subtitle: "How much token mass is carried by the most frequent vocabulary?",
    captionStats: "top 10 31.0% · top 100 55.0% · top 1,000 76.0% · top 10,000 91.0%",
    captionReading: "The display path includes the exact selected first crossing while stored checkpoints remain unchanged.",
    controls,
    controller,
    exportMetadata: () => [`Cumulative token coverage · ${Math.round(Number(target) * 100)}% target`],
    renderSvg,
  });
  return { figure, controller };
}

function buildProfileFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-dispersion-profile";
  const controller = fixtureController(id, {});
  if (pinned) controller.togglePin("dispersion-reach-band-8");
  const counts = [211_271, 14_610, 12_420, 10_210, 8_500, 7_200, 6_100, 5_000, 3_800, 0];
  const total = counts.reduce((sum, count) => sum + count, 0);
  const renderSvg = () => renderDispersionSvg(id, buildDispersionGeometry({
    buckets: counts.map((count, index) => ({
      key: `band-${index}`,
      label: index === 0 ? "1 document" : index === 9 ? "All documents" : `${index + 1}–${index + 2} documents`,
      lowerBound: index + 1,
      upperBound: index + 1,
      count,
      share: count / total,
      sampleTypes: [`example-${index}`],
    })),
    totalTypes: total,
    totalDocuments: 100,
    xMode: "count",
    reachLabels: "documents",
    labelsMode: "countsShares",
    cumulativeCallout: { count: 655, examples: ["the", "and", "of"] },
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Dispersion profile",
    subtitle: "How widely across documents does the vocabulary reach?",
    captionStats: `${total.toLocaleString("en-US")} types · 10 exclusive reach bands · 655 reach at least 90%`,
    captionReading: "Profile bands are mutually exclusive; the separate ≥90% callout is cumulative and follows the measured strip-label rail.",
    controller,
    layoutFamily: "profile",
    exportMetadata: () => ["Exclusive document-reach bands · type-count scale"],
    renderSvg,
  });
  return { figure, controller };
}

function buildRosterFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-dispersion-roster";
  const controller = fixtureController(id, { sort: "frequency", stopwords: "show", rows });
  if (pinned) controller.togglePin("roster:token-7");
  const entries = Array.from({ length: 100 }, (_, index) => ({
    token: index === 7 ? "a-very-long-pinned-token-label" : `token-${index + 1}`,
    frequency: 80_000 - index * 701,
    documentFrequency: 100 - index % 94,
    dp: ((index * 37) % 100) / 100,
  }));
  const controls = document.createElement("div");
  controls.append(
    createOverviewSelectControl("Sort", [
      { value: "frequency", label: "Frequency" }, { value: "dp", label: "DP" },
      { value: "documentFrequency", label: "Document frequency" },
    ], "frequency", () => undefined),
    createOverviewToggleControl("Stopwords", true, () => undefined),
  );
  const progressive = document.createElement("div");
  progressive.className = "cw-overview-result-controls";
  for (const label of rows > 20 ? ["Show fewer"] : ["Show 30 more", "Show all up to 100"]) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    progressive.appendChild(button);
  }
  controls.appendChild(progressive);
  const renderSvg = () => renderDispersionRosterSvg(id, buildDispersionRosterGeometry({
    entries: entries.slice(0, Math.min(100, rows)),
    highDispersionThreshold: 0.5,
    totalTokens: 2_000_000,
    totalDocuments: 100,
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Dispersion roster",
    subtitle: "Which frequent types are corpus-wide, and which are locally concentrated?",
    captionStats: `${Math.min(rows, 100)} of 100 frequent types · DP threshold 0.50`,
    captionReading: "Frequency does not imply wide distribution; the threshold is descriptive rather than inferential.",
    controls,
    controller,
    layoutFamily: "roster",
    exportMetadata: () => [`Top ${Math.min(rows, 100)} frequent types · sorted by frequency · stopwords shown`],
    renderSvg,
  });
  return { figure, controller };
}

function buildPlaneFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-dispersion-plane";
  const controller = fixtureController(id, {});
  if (pinned) controller.togglePin("plane:loudButLocal:specialised-local-witness");
  const renderSvg = () => renderDispersionPlaneSvg(id, buildDispersionPlaneGeometry({
    plane: {
      xBins: 12,
      yBins: 8,
      freqLog10Max: 6,
      counts: Array.from({ length: 96 }, (_, index) => index % 7 === 0 ? 0 : (index * 17) % 113),
      maxCellCount: 112,
      witnesses: [
        { token: "internationalisation", frequency: 420_000, documentFrequency: 98, dp: 0.08, kind: "core" },
        { token: "specialised-local-witness", frequency: 75_000, documentFrequency: 2, dp: 0.86, kind: "loudButLocal" },
        { token: "quiet-even", frequency: 43, documentFrequency: 80, dp: 0.14, kind: "quietAndEven" },
        { token: "rare-local", frequency: 2, documentFrequency: 1, dp: 0.93, kind: "rareAndLocal" },
      ],
      loudButLocalCount: 1,
      loudButLocalMinFrequency: 1_000,
      highDispersionThreshold: 0.5,
      reading: "fixture",
    },
    totalTypes: 5_000,
    totalTokens: 2_000_000,
    frequencyMode: "raw",
    densityMode: "log",
    examplesMode: "labelled",
    marginalsVisible: true,
    thresholdsVisible: true,
    pinnedKeys: controller.pins,
  }));
  const figure = createOverviewFigure({
    id,
    title: "Dispersion plane",
    subtitle: "Which types are frequent, and which are evenly distributed across documents?",
    captionStats: "5,000 types · grid 12×8 · raw frequency threshold 1,000 · DP threshold 0.50",
    captionReading: "Logarithmic density changes contrast rather than counts; authored witnesses, marginals, thresholds and legends remain visible.",
    controller,
    layoutFamily: "plane",
    exportMetadata: () => ["Raw frequency against Gries's DP · logarithmic density"],
    renderSvg,
  });
  return { figure, controller };
}

function fixtureController(id: OverviewFigureId, state: FigureControlState): OverviewFigureController<FigureControlState> {
  return new OverviewFigureController({ initialControls: state, policy: FIGURE_INTERACTION_POLICIES[id] });
}

function distributionPayload(length: boolean): DistributionResultPayload {
  const counts = length ? [3, 4, 6, 9, 12, 15, 14, 11, 9, 7, 5, 3, 2] : [2, 4, 7, 11, 16, 19, 17, 11, 6, 3];
  const total = counts.reduce((sum, count) => sum + count, 0);
  let cumulative = 0;
  const bins = counts.map((count, index) => {
    const lowerBound = length ? index === 0 ? 0 : 10 ** (index / 2) : 0.25 + index * 0.06;
    const upperBound = length ? 10 ** ((index + 1) / 2) : lowerBound + 0.06;
    cumulative += count;
    return { lowerBound, upperBound, count, share: count / total, cumulativeShare: cumulative / total };
  });
  const ecdfPoints = length
    ? [{ value: 0, cumulativeCount: 3, cumulativeShare: 3 / total }, ...Array.from({ length: 18 }, (_, index) => ({
        value: Math.round(10 ** (0.4 + index * 0.28)),
        cumulativeCount: Math.min(total, 3 + Math.round((index + 1) / 18 * (total - 3))),
        cumulativeShare: Math.min(1, (3 + Math.round((index + 1) / 18 * (total - 3))) / total),
      }))]
    : Array.from({ length: 18 }, (_, index) => ({ value: 0.28 + index * 0.03, cumulativeCount: Math.round((index + 1) / 18 * total), cumulativeShare: (index + 1) / 18 }));
  const qqPoints = Array.from({ length: 24 }, (_, index) => ({
    expectedQuantile: -2.1 + index * 4.2 / 23,
    observedValue: length ? 2.7 + index * 0.075 + Math.sin(index) * 0.025 : 0.36 + index * 0.018 + Math.sin(index) * 0.006,
    observationRank: index + 1,
    plottingPosition: (index + 0.5) / 24,
  }));
  return {
    xScale: length ? "log" : "linear",
    population: total,
    excludedCount: length ? 0 : 4,
    minimum: length ? 0 : 0.25,
    firstQuartile: length ? 320 : 0.49,
    median: length ? 2_480 : 0.612,
    thirdQuartile: length ? 14_200 : 0.71,
    maximum: length ? 1_000_000 : 0.85,
    histogramBins: bins,
    ecdfPoints,
    qqPoints,
    qqLineIntercept: length ? 3.55 : 0.56,
    qqLineSlope: length ? 0.43 : 0.1,
    lowWitnesses: [{ documentId: 1, documentName: "short-document.txt", tokenCount: length ? 0 : 120, metricValue: length ? 0 : 0.31 }],
    highWitnesses: [{ documentId: 2, documentName: "long-document-with-a-descriptive-name.txt", tokenCount: 1_000_000, metricValue: length ? 1_000_000 : 0.83 }],
  };
}

function auditFigure(figure: HTMLElement): string[] {
  const failures: string[] = [];
  const figureBounds = figure.getBoundingClientRect();
  if (figure.tagName !== "FIGURE") failures.push("shell is not a figure");
  if (!figure.querySelector(":scope > figcaption")) failures.push("missing direct figcaption");
  const svg = figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  if (!svg) return [...failures, "missing SVG"];
  if (svg.getAttribute("tabindex") !== "0") failures.push("plot owner is not the Tab entry");
  if (svg.querySelectorAll(".cw-plot-hover-target[tabindex='0']").length > 0) failures.push("mass mark Tab stops");
  if (svg.querySelector(".cw-plot-composition-segment.is-zero[tabindex]")) failures.push("zero segment is focusable");
  if (figure.hasAttribute("data-figure-palette")) failures.push("live figure depends on an export palette attribute");
  for (const role of ["observed", "reference", "selected", "attention", "density-0", "density-5", "composition-0", "composition-5"]) {
    if (!getComputedStyle(figure).getPropertyValue(`--cw-plot-${role}`).trim()) failures.push(`missing live semantic role: ${role}`);
  }
  for (const hidden of figure.querySelectorAll<HTMLElement>("[hidden]")) {
    const box = hidden.getBoundingClientRect();
    if (getComputedStyle(hidden).display !== "none" || box.width !== 0 || box.height !== 0) {
      failures.push(`hidden control occupies layout: ${hidden.className}`);
    }
  }
  auditComputedPaint(svg, failures);
  auditRecoveryGeometry(svg, failures);
  for (const button of figure.querySelectorAll<HTMLButtonElement>("button")) {
    if (!(button.textContent?.trim() || button.getAttribute("aria-label"))) failures.push("unnamed button");
    const box = button.getBoundingClientRect();
    if (box.width > 0 && (box.left < figureBounds.left - 1 || box.right > figureBounds.right + 1)) failures.push("action outside figure bounds");
  }
  const ids = Array.from(figure.querySelectorAll<HTMLElement>("[id]")).map((element) => element.id);
  if (new Set(ids).size !== ids.length) failures.push("duplicate IDs");
  const svgBounds = svg.getBoundingClientRect();
  for (const text of svg.querySelectorAll<SVGTextElement>("text")) {
    const style = getComputedStyle(text);
    if (style.display === "none" || style.visibility === "hidden" || !text.textContent?.trim()) continue;
    const box = text.getBoundingClientRect();
    if (box.left < svgBounds.left - 1 || box.top < svgBounds.top - 1 || box.right > svgBounds.right + 1 || box.bottom > svgBounds.bottom + 1) {
      failures.push(`clipped text: ${text.textContent}`);
      break;
    }
  }
  if (svg.querySelector(".cw-plot-spectrum-band-label")) failures.push("detached lexical class-heading rail");
  const compositionLabels = Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-composition-label"))
    .filter((text) => getComputedStyle(text).display !== "none");
  for (let left = 0; left < compositionLabels.length; left += 1) {
    for (let right = left + 1; right < compositionLabels.length; right += 1) {
      if (compositionLabels[left]!.closest(".cw-plot-composition-entry") === compositionLabels[right]!.closest(".cw-plot-composition-entry")) continue;
      if (rectanglesOverlap(compositionLabels[left]!.getBoundingClientRect(), compositionLabels[right]!.getBoundingClientRect(), 1)) {
        failures.push("composition label overlap");
      }
    }
  }
  const callout = svg.querySelector<SVGGElement>(".cw-plot-dispersion-cumulative");
  if (callout) {
    for (const text of compositionLabels) {
      if (rectanglesOverlap(callout.getBoundingClientRect(), text.getBoundingClientRect(), 2)) failures.push("callout overlaps composition rail");
    }
  }
  const rosterHeaders = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-roster-header"));
  const rosterLegendLabels = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-roster-legend .cw-plot-legend-label"));
  for (const header of rosterHeaders) {
    for (const legend of rosterLegendLabels) {
      if (rectanglesOverlap(header.getBoundingClientRect(), legend.getBoundingClientRect(), 1)) failures.push("roster header overlaps legend");
    }
  }
  const legends = Array.from(svg.querySelectorAll<SVGGElement>(".cw-plot-legend, .cw-plot-density-legend, .cw-plot-witness-legend"));
  const priorityMarks = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-hover-target.is-pinned"));
  for (const legend of legends) {
    for (const mark of priorityMarks) {
      if (rectanglesOverlap(legend.getBoundingClientRect(), mark.getBoundingClientRect(), 2)) failures.push("legend overlaps pinned evidence");
    }
  }
  const visibleTooltip = svg.querySelector<SVGGElement>(".cw-plot-tooltip[aria-hidden='false']");
  if (visibleTooltip) {
    const box = visibleTooltip.getBoundingClientRect();
    if (box.left < 0 || box.top < 0 || box.right > window.innerWidth || box.bottom > window.innerHeight) {
      failures.push("focused tooltip outside viewport");
    }
  }
  return failures;
}

function auditComputedPaint(svg: SVGSVGElement, failures: string[]): void {
  const rejectBrowserBlack = theme === "corpus-light" || theme === "corpus-dark";
  const assertPaint = (selector: string, property: "fill" | "stroke", label: string, nonBlack = false): void => {
    const element = svg.querySelector<SVGGraphicsElement>(selector);
    if (!element) {
      failures.push(`missing painted geometry: ${label}`);
      return;
    }
    const style = getComputedStyle(element);
    const paint = style[property];
    const opacity = Number(property === "fill" ? style.fillOpacity : style.strokeOpacity);
    if (!isVisiblePaint(paint) || !(opacity > 0)) failures.push(`invisible computed ${property}: ${label}`);
    if (nonBlack && isPureBlack(paint)) failures.push(`browser-black computed ${property}: ${label}`);
  };
  const assertDistinctFills = (selector: string, minimum: number, label: string): void => {
    const fills = new Set(Array.from(svg.querySelectorAll<SVGGraphicsElement>(selector))
      .map((element) => getComputedStyle(element).fill)
      .filter(isVisiblePaint));
    if (fills.size < minimum) failures.push(`insufficient computed fill variation: ${label} (${fills.size})`);
  };

  if (figureName === "document" || figureName === "mattr") {
    if (view === "histogram") assertPaint(".cw-plot-bar", "fill", `${figureName} histogram bar`, rejectBrowserBlack);
    if (view === "ecdf") assertPaint(".cw-plot-ecdf-line", "stroke", `${figureName} ECDF`);
    if (view === "qq") {
      assertPaint(".cw-plot-point", "fill", `${figureName} Q–Q point`, rejectBrowserBlack);
      assertPaint(".cw-plot-qq-reference", "stroke", `${figureName} Q–Q reference`);
    }
    if (figureName === "mattr") assertDistinctFills(".cw-plot-composition-segment", 2, "MATTR eligibility strip");
  }
  if (figureName === "rank") {
    assertPaint(".cw-plot-point:not(.is-excluded)", "fill", "rank observations", rejectBrowserBlack);
    if (view === "curve") assertPaint(".cw-plot-fit-line", "stroke", "rank fitted line");
  }
  if (figureName === "growth") {
    assertPaint(".cw-plot-line", "stroke", "vocabulary growth observed line");
    assertPaint(".cw-plot-fit-line", "stroke", "vocabulary growth fit");
  }
  if (figureName === "lexical") {
    assertPaint(".cw-plot-point", "fill", "lexical exact-frequency point", rejectBrowserBlack);
    assertDistinctFills(".cw-plot-composition-segment", 3, "lexical composition strip");
  }
  if (figureName === "coverage") {
    assertPaint(".cw-plot-line", "stroke", "coverage cumulative line");
    assertPaint(".cw-plot-coverage-crossing", "fill", "coverage target point");
    assertDistinctFills(".cw-plot-composition-segment", 3, "coverage composition strip");
  }
  if (figureName === "profile") {
    assertDistinctFills(".cw-plot-composition-segment", 3, "dispersion profile composition strip");
    assertPaint(".cw-plot-point", "fill", "dispersion profile endpoint", rejectBrowserBlack);
  }
  if (figureName === "plane") {
    assertDistinctFills(".cw-plot-density-cell:not(.is-zero)", 4, "dispersion plane density");
    if (rejectBrowserBlack) assertDistinctFills(".cw-plot-plane-witness", 4, "dispersion plane witnesses");
  }
}

function auditRecoveryGeometry(svg: SVGSVGElement, failures: string[]): void {
  const plotTop = Number(svg.querySelector<SVGLineElement>(".cw-plot-crosshair-x")?.getAttribute("y1"));
  const plotBottom = Number(svg.querySelector<SVGLineElement>(".cw-plot-crosshair-x")?.getAttribute("y2"));
  if (figureName === "rank" && view === "residuals") {
    if (svg.querySelector(".cw-plot-point.is-excluded")) failures.push("excluded point rendered in rank residuals");
    if (Array.from(svg.querySelectorAll(".cw-plot-legend-label")).some((label) => label.textContent?.includes("Excluded"))) {
      failures.push("residual legend describes excluded geometry");
    }
    const zeroY = Number(svg.querySelector<SVGLineElement>(".cw-plot-residual-zero")?.getAttribute("y1"));
    if (!Number.isFinite(zeroY) || Math.abs(zeroY - ((plotTop + plotBottom) / 2)) > 0.5) failures.push("residual zero is not centred");
  }
  if (figureName === "rank" && view === "curve") {
    const band = svg.querySelector<SVGRectElement>(".cw-plot-fitted-range-band");
    const y = Number(band?.getAttribute("y"));
    const height = Number(band?.getAttribute("height"));
    const labelY = Number(svg.querySelector<SVGTextElement>(".cw-plot-fitted-range-label")?.getAttribute("y"));
    if (!band || y < plotTop || y + height > plotBottom || labelY < plotTop || labelY > plotBottom) {
      failures.push("fitted range leaves rank plot bounds");
    }
  }
  if (figureName === "mattr" && view === "histogram") {
    const automaticNames = Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-label"))
      .map((label) => label.textContent ?? "")
      .filter((text) => text.includes("document") || text.includes("short-document") || text.includes("long-document"));
    if (!pinned && automaticNames.length > 0) failures.push("automatic MATTR filename label");
    if (pinned && !automaticNames.some((text) => text.includes("short-document"))) failures.push("pinned MATTR filename missing");
  }
  if (figureName === "lexical") {
    const texts = Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-label")).map((label) => label.textContent ?? "");
    if (!texts.some((text) => text.includes("hapax · f = 1"))) failures.push("hapax point annotation missing");
    if (!texts.some((text) => text.includes("dis legomena · f = 2"))) failures.push("dis point annotation missing");
    const hapaxEntry = Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-composition-label.is-category"))
      .some((label) => label.textContent === "Hapax (1)");
    if (!hapaxEntry) failures.push("Hapax composition entry missing");
  }
}

function isVisiblePaint(value: string): boolean {
  if (!value || value === "none" || value === "transparent") return false;
  const alpha = value.match(/^rgba?\([^/]+(?:\/\s*([\d.]+))?\)$/)?.[1];
  return alpha === undefined || Number(alpha) > 0;
}

function isPureBlack(value: string): boolean {
  return /^(?:rgb\(0,\s*0,\s*0\)|rgba\(0,\s*0,\s*0,\s*1\)|#000(?:000)?)$/i.test(value.trim());
}

function rectanglesOverlap(left: DOMRect, right: DOMRect, gap = 0): boolean {
  return left.left < right.right + gap
    && left.right + gap > right.left
    && left.top < right.bottom + gap
    && left.bottom + gap > right.top;
}

function defaultView(name: string): string {
  if (name === "rank") return "curve";
  if (name === "growth") return "growth";
  return "histogram";
}

function animationFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}
