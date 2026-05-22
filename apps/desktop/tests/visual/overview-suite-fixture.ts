import katex from "katex";
import {
  overviewGlossary,
  type OverviewGlossaryEntry,
  type OverviewGlossaryId,
} from "../../src/overview-glossary";
import {
  createOverviewFigure,
  createOverviewRangeControl,
  createOverviewSegmentedControl,
  overviewFigureSvgString,
  replaceFigureSvg,
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

const params = new URLSearchParams(location.search);
const figureName = params.get("figure") ?? "document";
const view = params.get("view") ?? defaultView(figureName);
const switchTo = params.get("switchTo");
let auditedView = view;
const theme = params.get("theme") ?? "corpus-light";
const target = params.get("target") ?? "0.5";
const customBins = params.get("customBins") === "true";
const pinned = params.get("pinned") === "true";
const exportMode = params.get("export") === "true";
const methodDisclosure = params.get("methodDisclosure");
const hoverProfileBand = params.get("hoverProfileBand") === "true";
const palette = (params.get("palette") ?? "theme") as FigurePaletteMode;
document.documentElement.dataset.theme = theme;

const errors: string[] = [];
window.addEventListener("error", (event) => errors.push(event.message));
window.addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));

const root = document.querySelector<HTMLElement>("#suite-root")!;
const status = document.querySelector<HTMLOutputElement>("#suite-status")!;
const overviewFigureNames = ["document", "mattr", "rank", "growth", "lexical", "coverage", "profile", "plane"] as const;
const overviewMode = figureName === "overview";
const builtFigures = overviewMode
  ? overviewFigureNames.map((name) => buildFixtureFigure(name, defaultView(name)))
  : [buildFixtureFigure(figureName, view)];
const built = builtFigures[0]!;
if (overviewMode) {
  root.classList.add("is-overview-suite");
  const heading = document.createElement("h1");
  heading.className = "overview-suite-heading";
  heading.textContent = "Corpus Overview · eight analytical figures";
  root.appendChild(heading);
}
root.append(...builtFigures.map((entry) => entry.figure));

if (switchTo && !overviewMode) {
  const switchButton = built.figure.querySelector<HTMLButtonElement>(`.cw-overview-segmented-control[aria-label="View"] .cw-overview-segment[data-value="${switchTo}"]`);
  if (!switchButton) throw new Error(`Missing requested view switch: ${switchTo}`);
  switchButton.click();
}

await document.fonts.ready;
await animationFrame();
await animationFrame();
const timedFailures: string[] = [];
if ((figureName === "document" || figureName === "mattr") && auditedView === "ecdf") {
  auditEcdfState(built.figure, timedFailures, "initial");
}
for (const animation of document.getAnimations()) {
  try { animation.finish(); } catch { /* A non-finite fixture animation is not part of plot readiness. */ }
}
await animationFrame();
if ((figureName === "document" || figureName === "mattr") && (auditedView === "ecdf" || auditedView === "qq")) {
  await delay(1_000);
  await animationFrame();
}
if ((figureName === "document" || figureName === "mattr") && auditedView === "ecdf") {
  auditEcdfState(built.figure, timedFailures, "after 1,000 ms");
}

if (params.get("focusDensity") === "true" && !overviewMode) {
  built.figure.querySelector<SVGElement>(".cw-plot-density-cell[data-grid-row='6']:not(.is-zero)")?.focus();
}
if (params.get("exportDialog") === "true" && !overviewMode) {
  built.figure.querySelector<HTMLButtonElement>(".cw-overview-figure-export")?.click();
}
if (hoverProfileBand && figureName === "profile") {
  const band = built.figure.querySelector<SVGElement>(".cw-plot-composition-segment.cw-plot-profile-band-1.cw-plot-hover-target");
  band?.dispatchEvent(new PointerEvent("pointerover", { bubbles: true }));
  await animationFrame();
}
if (methodDisclosure) {
  built.figure.querySelector<HTMLElement>(".analysis-method-fixture")?.scrollIntoView({ block: "start" });
  await animationFrame();
}

const overviewShellSource = overviewMode ? await fetch("/src/workbench/shell.ts").then((response) => response.text()) : "";
const auditFailures = overviewMode ? auditOverviewSuite(builtFigures, overviewShellSource) : auditFigure(built.figure);
const failures = [...errors, ...timedFailures, ...auditFailures];
document.body.dataset.consoleErrors = String(errors.length);
document.body.dataset.accessibilitySerious = String(auditFailures.length);
document.body.dataset.fixtureStatus = failures.length === 0 ? "passed" : "failed";
document.body.dataset.fixtureMetrics = JSON.stringify(collectFixtureMetrics(builtFigures));
status.textContent = failures.length === 0 ? "passed" : `failed: ${failures.join(" | ")}`;

if (exportMode && !overviewMode && failures.length === 0) {
  const source = overviewFigureSvgString(built.figure, palette);
  if (!source) throw new Error("Unable to serialise fixture SVG");
  document.body.dataset.exportReady = "true";
  document.body.innerHTML = source;
}

interface BuiltFigure {
  figure: HTMLElement;
  controller: OverviewFigureController<FigureControlState>;
}

function auditOverviewSuite(entries: BuiltFigure[], shellSource: string): string[] {
  const failures: string[] = [];
  if (entries.length !== 8 || root.querySelectorAll(":scope > figure").length !== 8) failures.push("Overview fixture does not contain eight figures");
  if (root.querySelectorAll(":scope > figure .cw-plot-svg").length !== 8) failures.push("Overview fixture does not contain eight real-renderer SVGs");
  if (Array.from(root.querySelectorAll("h1, h2, h3, h4")).some((heading) => heading.textContent?.trim() === "Dispersion roster")) {
    failures.push("Dispersion roster heading remains in Overview");
  }
  for (const duplicate of [
    'label: "Types", value: formatNumber(spectrum.types)',
    'label: "Top 100 types cover"',
    'label: "Top 1,000 types cover"',
    'label: "Top 10,000 types cover"',
    'label: "Types in 1 document only"',
    'label: "Median documents per type"',
  ]) {
    if (shellSource.includes(duplicate)) failures.push(`duplicated Overview structure remains: ${duplicate}`);
  }
  if (shellSource.includes("Dispersion roster")) failures.push("Dispersion roster source remains in Overview");
  return failures;
}

function buildFixtureFigure(name: string, selectedView: string): BuiltFigure {
  if (name === "document" || name === "mattr") return buildDistributionFigure(name, selectedView);
  if (name === "rank") return buildRankFigure(selectedView);
  if (name === "growth") return buildGrowthFigure(selectedView);
  if (name === "lexical") return buildLexicalFigure();
  if (name === "coverage") return buildCoverageFigure();
  if (name === "profile") return buildProfileFigure();
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
  let activeView = selectedView as "histogram" | "ecdf" | "qq";
  let figure!: HTMLElement;
  const result = distributionPayload(isLength);
  if (pinned) controller.togglePin(selectedView === "qq"
    ? "distribution-qq:10"
    : name === "mattr" ? "distribution-witness:1" : "distribution-bin:4");
  const controls = document.createElement("div");
  const viewControl = createOverviewSegmentedControl("View", [
    { value: "histogram", label: "Histogram" },
    { value: "ecdf", label: "ECDF" },
    { value: "qq", label: "Q–Q" },
  ], activeView, (nextView) => {
    activeView = nextView;
    auditedView = nextView;
    controller.updateControls({ view: nextView });
    syncDistributionControls();
    replaceFigureSvg(figure, renderSvg());
  });
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
  const horizontalScale = createOverviewSegmentedControl("Scale", [
    { value: "log", label: "Logarithmic" },
    { value: "linear", label: "Linear" },
  ], "log", () => undefined);
  controls.append(viewControl, binning, binCount, ...(isLength ? [horizontalScale] : []));
  const renderSvg = () => renderDistributionDiagnosticSvg(id, buildDistributionDiagnosticGeometry({
    result,
    metric: isLength ? "documentLength" : "mattr",
    view: activeView,
    yMode: "share",
    qqReference: isLength ? "lognormal" : "normal",
    labelsMode: "extremes",
    mattrWindow: 100,
    pinnedKeys: controller.pins,
  }));
  const stateText = () => activeView === "histogram"
    ? `Histogram · ${customBins ? "custom 36 bins" : "automatic bins"} · ${isLength ? "logarithmic token scale" : "share of eligible documents"}`
    : activeView === "ecdf" ? `ECDF · ${isLength ? "logarithmic token scale" : "linear MATTR scale"}`
      : `Q–Q · ${isLength ? "lognormal" : "normal"} reference`;
  const syncDistributionControls = (): void => {
    binning.hidden = activeView !== "histogram";
    binCount.hidden = activeView !== "histogram" || !customBins;
    horizontalScale.hidden = !isLength || activeView === "qq";
  };
  syncDistributionControls();
  const explanation = methodDisclosure
    ? buildGroupedMethodFixture(methodDisclosure)
    : undefined;
  figure = createOverviewFigure({
    id,
    title: isLength ? "Document-length distribution" : "MATTR distribution",
    subtitle: isLength ? "How unevenly is the corpus divided among documents?" : "How much does lexical diversity vary between documents?",
    captionStats: isLength ? "100 documents · median 2,480 tokens · 3 zero-token documents" : "96 eligible documents · window 100 · median 0.612",
    captionReading: isLength
      ? "The histogram keeps zero-token documents explicit and shows positive lengths on the selected scale."
      : "MATTR compares lexical variety within equal windows; excluded short documents remain visible in the eligibility strip.",
    controls,
    explanation,
    controller,
    exportMetadata: () => [stateText()],
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
    explanation: buildFixtureExplanation(),
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
  const counts = [165_000, 179_227, 12_000, 8_500, 6_500, 5_000, 4_000, 3_000, 1_500, 0];
  const total = counts.reduce((sum, count) => sum + count, 0);
  const renderSvg = () => renderDispersionSvg(id, buildDispersionGeometry({
    buckets: counts.map((count, index) => ({
      key: `band-${index}`,
      label: index === 0 ? "1 document" : index === 9 ? "All documents" : `${index + 1} documents`,
      lowerBound: index + 1,
      upperBound: index + 1,
      count,
      share: count / total,
      sampleTypes: index === 1
        ? ["the", "and", "of", "in", "to", "a", "long-example-type-that-must-not-appear"]
        : [`example-${index}`],
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

function buildPlaneFigure(): BuiltFigure {
  const id: OverviewFigureId = "overview-dispersion-plane";
  const controller = fixtureController(id, {});
  if (pinned) controller.togglePin("plane:loudButLocal:specialised-local-witness");
  const renderSvg = () => renderDispersionPlaneSvg(id, buildDispersionPlaneGeometry({
    plane: {
      xBins: 12,
      yBins: 8,
      freqLog10Max: 6,
      counts: [
        1, 1, 2, 2, 4, 8, 18, 42, 78, 112, 64, 24,
        0, 1, 1, 2, 5, 11, 30, 68, 96, 84, 38, 12,
        0, 0, 1, 1, 3, 8, 20, 48, 62, 44, 20, 7,
        0, 0, 0, 1, 2, 5, 10, 18, 24, 16, 8, 3,
        0, 0, 0, 0, 1, 2, 4, 8, 12, 7, 3, 1,
        0, 0, 0, 0, 0, 1, 2, 4, 5, 3, 1, 0,
        0, 0, 0, 0, 0, 0, 1, 2, 3, 1, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0,
      ],
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

function buildFixtureExplanation(): HTMLElement {
  const explanation = document.createElement("div");
  explanation.className = "cw-overview-figure-teaching";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "analysis-glossary-term";
  button.setAttribute("aria-expanded", "false");
  const label = document.createElement("span");
  label.textContent = "Explain Top-N coverage";
  const caret = document.createElement("span");
  caret.className = "analysis-glossary-caret";
  caret.textContent = "›";
  button.append(label, caret);
  const detail = document.createElement("div");
  detail.className = "analysis-glossary-detail";
  detail.hidden = true;
  detail.textContent = "Fixture explanation detail.";
  explanation.append(button, detail);
  return explanation;
}

function buildGroupedMethodFixture(kind: string): HTMLElement {
  const documentBalance = kind === "documentBalance";
  const label = documentBalance ? "Document balance" : "Frequency concentration";
  const glossaryIds: OverviewGlossaryId[] = documentBalance
    ? ["giniCoefficient", "coefficientOfVariation", "effectiveDocuments"]
    : ["shannonEntropy", "normalisedEntropy", "effectiveVocabulary"];
  const values = documentBalance
    ? ["0.418", "0.731", "42.6 of 100"]
    : ["11.240", "0.823", "≈ 2,410 types"];
  const fixture = document.createElement("div");
  fixture.className = "analysis-method-fixture";
  const table = document.createElement("table");
  table.className = "analysis-metric-table has-no-share-column";
  const body = document.createElement("tbody");
  glossaryIds.forEach((glossaryId, index) => {
    const row = document.createElement("tr");
    const heading = document.createElement("th");
    heading.scope = "row";
    heading.textContent = overviewGlossary[glossaryId].label;
    const value = document.createElement("td");
    value.className = "numeric analysis-metric-value";
    value.textContent = values[index]!;
    row.append(heading, value);
    body.appendChild(row);
  });
  table.appendChild(body);

  const disclosure = document.createElement("div");
  disclosure.className = "analysis-method-disclosure";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "analysis-glossary-term";
  button.setAttribute("aria-expanded", "true");
  button.setAttribute("aria-controls", `fixture-${kind}`);
  const buttonLabel = document.createElement("span");
  buttonLabel.textContent = label;
  const caret = document.createElement("span");
  caret.className = "analysis-glossary-caret";
  caret.textContent = "›";
  button.append(buttonLabel, caret);
  const panel = document.createElement("div");
  panel.id = `fixture-${kind}`;
  panel.className = "analysis-glossary-detail analysis-method-disclosure-detail";
  for (const glossaryId of glossaryIds) {
    const entry = overviewGlossary[glossaryId];
    const section = document.createElement("section");
    section.className = "analysis-glossary-group-entry";
    const heading = document.createElement("h4");
    heading.className = "analysis-glossary-entry-title";
    heading.textContent = entry.label;
    section.appendChild(heading);
    appendFixtureGlossaryEntry(section, entry);
    panel.appendChild(section);
  }
  disclosure.append(button, panel);
  fixture.append(table, disclosure);
  return fixture;
}

function appendFixtureGlossaryEntry(panel: HTMLElement, entry: OverviewGlossaryEntry): void {
  for (const text of [entry.explanation, entry.detail].filter((value): value is string => Boolean(value))) {
    const paragraph = document.createElement("p");
    paragraph.textContent = text;
    panel.appendChild(paragraph);
  }
  const formula = document.createElement("div");
  formula.className = "analysis-glossary-formula";
  katex.render(entry.latex, formula, { displayMode: true, output: "htmlAndMathml", throwOnError: false });
  panel.appendChild(formula);
  if (entry.symbols?.length) appendFixtureGlossarySection(panel, "Where", entry.symbols.map((symbol) => `${symbol.sym}: ${symbol.def}`));
  if (entry.interpretation) appendFixtureGlossarySection(panel, "How to read it", [entry.interpretation]);
  if (entry.limitations?.length) appendFixtureGlossarySection(panel, "Limitations", entry.limitations);
}

function appendFixtureGlossarySection(panel: HTMLElement, label: string, rows: string[]): void {
  const section = document.createElement("section");
  section.className = "analysis-glossary-section";
  const heading = document.createElement("h4");
  heading.textContent = label;
  const list = document.createElement("ul");
  list.className = "analysis-glossary-limitations";
  for (const row of rows) {
    const item = document.createElement("li");
    item.textContent = row;
    list.appendChild(item);
  }
  section.append(heading, list);
  panel.appendChild(section);
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
