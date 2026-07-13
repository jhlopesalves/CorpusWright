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
    lowWitnesses: [{ documentId: 1, documentName: "short-document.txt", tokenCount: length ? 0 : 120, metricValue: length ? 0 : 0.31 }],
    highWitnesses: [{ documentId: 2, documentName: "long-document-with-a-descriptive-name.txt", tokenCount: 1_000_000, metricValue: length ? 1_000_000 : 0.83 }],
  };
}

function collectFixtureMetrics(entries: BuiltFigure[]): Record<string, unknown> {
  const metrics: Record<string, unknown> = { figureCount: entries.length };
  const svg = entries[0]?.figure.querySelector<SVGSVGElement>(".cw-plot-svg");
  if (!svg || entries.length !== 1) return metrics;
  const ecdf = svg.querySelector<SVGPathElement>(".cw-plot-ecdf-line");
  if (ecdf) {
    const style = getComputedStyle(ecdf);
    const end = ecdf.getPointAtLength(ecdf.getTotalLength());
    metrics.ecdf = {
      stroke: style.stroke,
      strokeOpacity: Number(style.strokeOpacity),
      opacity: Number(style.opacity),
      bounds: { width: ecdf.getBBox().width, height: ecdf.getBBox().height },
      endpoint: { x: end.x, y: end.y },
      visible: isVisiblePaint(style.stroke) && Number(style.strokeOpacity) > 0 && Number(style.opacity) > 0,
    };
    const targets = Array.from(svg.querySelectorAll<SVGCircleElement>(".cw-plot-ecdf-hit-target"));
    const targetStyle = targets[0] ? getComputedStyle(targets[0]) : null;
    metrics.ecdfTargets = {
      count: targets.length,
      fill: targetStyle?.fill ?? "missing",
      stroke: targetStyle?.stroke ?? "missing",
      opacity: Number(targetStyle?.opacity ?? -1),
      ordinaryPointCount: svg.querySelectorAll(".cw-plot-point.cw-plot-ecdf-hit-target").length,
    };
  }
  const qqPoints = Array.from(svg.querySelectorAll<SVGCircleElement>(".cw-plot-qq-point:not(.is-pinned)"));
  if (qqPoints.length > 0) {
    const visibleCount = qqPoints.filter((point) => {
      const style = getComputedStyle(point);
      return isVisiblePaint(style.fill) && Number(style.fillOpacity) > 0 && Number(style.opacity) > 0;
    }).length;
    const reference = svg.querySelector<SVGPathElement>(".cw-plot-qq-reference");
    metrics.qq = {
      ordinaryPointCount: qqPoints.length,
      visibleFillCount: visibleCount,
      visibleFillRatio: visibleCount / qqPoints.length,
      fill: getComputedStyle(qqPoints[0]!).fill,
      referenceStroke: reference ? getComputedStyle(reference).stroke : "none",
    };
  }
  const densityCells = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-density-cell:not(.is-zero)"));
  if (densityCells.length > 0) {
    const opacities = densityCells.map((cell) => Number(getComputedStyle(cell).opacity)).sort((left, right) => left - right);
    metrics.density = {
      positiveCellCount: densityCells.length,
      distinctComputedFills: new Set(densityCells.map((cell) => getComputedStyle(cell).fill)).size,
      medianPositiveOpacity: opacities[Math.floor(opacities.length / 2)] ?? 0,
    };
  }
  const tooltipBox = svg.querySelector<SVGRectElement>(".cw-plot-tooltip[aria-hidden='false'] .cw-plot-tooltip-box");
  if (tooltipBox) {
    metrics.tooltip = {
      width: Number(tooltipBox.getAttribute("width")),
      height: Number(tooltipBox.getAttribute("height")),
      visualLineCount: svg.querySelectorAll(".cw-plot-tooltip-text tspan").length,
      text: Array.from(svg.querySelectorAll(".cw-plot-tooltip-text tspan")).map((line) => line.textContent).join(" "),
    };
  }
  const method = entries[0]?.figure.querySelector<HTMLElement>(".analysis-method-disclosure");
  if (method) {
    const detail = method.querySelector<HTMLElement>(".analysis-method-disclosure-detail");
    const style = detail ? getComputedStyle(detail) : null;
    metrics.methodDisclosure = {
      label: method.querySelector(":scope > .analysis-glossary-term span")?.textContent ?? "",
      entryCount: method.querySelectorAll(".analysis-glossary-group-entry").length,
      metricValueCount: entries[0]?.figure.querySelectorAll(".analysis-method-fixture .analysis-metric-value").length ?? 0,
      borderLeft: style?.borderLeftStyle ?? "missing",
      background: style?.backgroundColor ?? "missing",
    };
  }
  return metrics;
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
  if (figure.querySelector(".cw-overview-figure-options") || figure.textContent?.includes("Figure options")) {
    failures.push("Figure options disclosure remains");
  }
  for (const role of ["observed", "reference", "selected", "attention", "density-0", "density-5", "composition-0", "composition-5"]) {
    if (!getComputedStyle(figure).getPropertyValue(`--cw-plot-${role}`).trim()) failures.push(`missing live semantic role: ${role}`);
  }
  for (const hidden of figure.querySelectorAll<HTMLElement>("[hidden]")) {
    const box = hidden.getBoundingClientRect();
    if (getComputedStyle(hidden).display !== "none" || box.width !== 0 || box.height !== 0) {
      failures.push(`hidden control occupies layout: ${hidden.className}`);
    }
  }
  if (switchTo && (figureName === "document" || figureName === "mattr")) {
    const irrelevantLabels = ["Binning", "Bins", ...(switchTo === "qq" && figureName === "document" ? ["Scale"] : [])];
    for (const expectedLabel of irrelevantLabels) {
      const label = Array.from(figure.querySelectorAll<HTMLElement>(".cw-overview-control-label"))
        .find((candidate) => candidate.textContent === expectedLabel);
      const field = label?.closest<HTMLElement>(".cw-overview-control-field");
      const box = field?.getBoundingClientRect();
      if (!field || field.hidden !== true || getComputedStyle(field).display !== "none" || box?.width !== 0 || box.height !== 0) {
        failures.push(`switched ${expectedLabel} control remains visible`);
      }
    }
  }
  if (figureName === "document" || figureName === "mattr") auditDistributionControlsAndGeometry(figure, svg, failures);
  auditComputedPaint(svg, failures);
  auditRecoveryGeometry(svg, failures);
  auditInteractionCapability(svg, failures);
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
    const tooltipBox = visibleTooltip.querySelector<SVGRectElement>(".cw-plot-tooltip-box");
    const tooltipWidth = Number(tooltipBox?.getAttribute("width"));
    const tooltipHeight = Number(tooltipBox?.getAttribute("height"));
    if (!(tooltipWidth > 0) || tooltipWidth > 300) failures.push("tooltip exceeds 300px maximum width");
    if (!(tooltipHeight > 0) || tooltipHeight > 180) failures.push("tooltip creates a giant rectangle");
    for (const line of visibleTooltip.querySelectorAll<SVGTSpanElement>("tspan")) {
      if (line.getComputedTextLength() > tooltipWidth - 16 + 1) {
        failures.push("tooltip text line is clipped");
        break;
      }
    }
  }
  if (hoverProfileBand && figureName === "profile") {
    const tooltipText = Array.from(svg.querySelectorAll<SVGTSpanElement>(".cw-plot-tooltip-text tspan"))
      .map((line) => line.textContent ?? "").join(" ");
    if (!tooltipText.includes("Examples: the, and, of +179,224 more")) failures.push("profile tooltip example budget or remainder is incorrect");
    if (tooltipText.includes(" in ") || tooltipText.includes("long-example-type")) failures.push("profile tooltip exposes more than three examples");
    if (svg.querySelector(".cw-plot-crosshair-x, .cw-plot-crosshair-y")) failures.push("profile hover creates a crosshair");
  }
  if (methodDisclosure) auditMethodDisclosure(figure, failures);
  return failures;
}

function auditMethodDisclosure(figure: HTMLElement, failures: string[]): void {
  const fixture = figure.querySelector<HTMLElement>(".analysis-method-fixture");
  const disclosure = fixture?.querySelector<HTMLElement>(".analysis-method-disclosure");
  const detail = disclosure?.querySelector<HTMLElement>(".analysis-method-disclosure-detail");
  const buttons = disclosure?.querySelectorAll(":scope > .analysis-glossary-term") ?? [];
  if (!fixture || !disclosure || !detail || buttons.length !== 1) {
    failures.push("grouped method disclosure does not appear exactly once");
    return;
  }
  if (fixture.querySelectorAll(".analysis-metric-value").length !== 3) failures.push("grouped method metric values are missing");
  if (detail.querySelectorAll(".analysis-glossary-group-entry").length !== 3) failures.push("grouped method entries are missing");
  if (detail.querySelectorAll(".analysis-glossary-formula").length !== 3) failures.push("grouped method formulas are missing");
  if (detail.querySelectorAll(".analysis-glossary-limitations").length < 3) failures.push("grouped method limitations are missing");
  const detailStyle = getComputedStyle(detail);
  if (detailStyle.borderLeftStyle !== "none" || detailStyle.backgroundColor !== "rgba(0, 0, 0, 0)") {
    failures.push("grouped method disclosure retains the bright metric panel treatment");
  }
  if (getComputedStyle(disclosure).borderTopStyle === "none") failures.push("grouped method disclosure lacks its subtle divider");
}

function auditComputedPaint(svg: SVGSVGElement, failures: string[]): void {
  const rejectBrowserBlack = theme === "corpus-light" || theme === "corpus-dark";
  const assertPaint = (selector: string, property: "fill" | "stroke", label: string, nonBlack = false, requireArea = false): SVGGraphicsElement | null => {
    const element = svg.querySelector<SVGGraphicsElement>(selector);
    if (!element) {
      failures.push(`missing painted geometry: ${label}`);
      return null;
    }
    const style = getComputedStyle(element);
    const paint = style[property];
    const opacity = Number(property === "fill" ? style.fillOpacity : style.strokeOpacity);
    if (!isVisiblePaint(paint) || !(opacity > 0) || !(Number(style.opacity) > 0)) failures.push(`invisible computed ${property}: ${label}`);
    if (nonBlack && isPureBlack(paint)) failures.push(`browser-black computed ${property}: ${label}`);
    if (requireArea) {
      const box = element.getBBox();
      if (!(box.width > 0) || !(box.height > 0)) failures.push(`zero-area painted geometry: ${label}`);
    }
    return element;
  };
  const assertDistinctFills = (selector: string, minimum: number, label: string): void => {
    const fills = new Set(Array.from(svg.querySelectorAll<SVGGraphicsElement>(selector))
      .map((element) => getComputedStyle(element).fill)
      .filter(isVisiblePaint));
    if (fills.size < minimum) failures.push(`insufficient computed fill variation: ${label} (${fills.size})`);
  };

  if (figureName === "document" || figureName === "mattr") {
    if (auditedView === "histogram") assertPaint(".cw-plot-bar", "fill", `${figureName} histogram bar`, rejectBrowserBlack);
    if (auditedView === "ecdf") assertPaint(".cw-plot-ecdf-line", "stroke", `${figureName} ECDF`, false, true);
    if (auditedView === "qq") {
      assertPaint(".cw-plot-point", "fill", `${figureName} Q–Q point`, rejectBrowserBlack);
      assertPaint(".cw-plot-qq-reference", "stroke", `${figureName} Q–Q reference`);
      const ordinaryPoints = Array.from(svg.querySelectorAll<SVGCircleElement>(".cw-plot-qq-point:not(.is-pinned)"));
      const visiblyFilled = ordinaryPoints.filter((point) => {
        const style = getComputedStyle(point);
        return isVisiblePaint(style.fill) && Number(style.fillOpacity) > 0 && Number(style.opacity) > 0;
      });
      if (ordinaryPoints.length === 0 || visiblyFilled.length / ordinaryPoints.length < 0.9) failures.push("fewer than 90% of Q–Q points have visible fill");
      if (ordinaryPoints.some((point) => getComputedStyle(point).animationName !== "none")) failures.push("Q–Q point retains entrance animation");
      if (ordinaryPoints.some((point) => getComputedStyle(point).stroke !== "none")) failures.push("ordinary Q–Q point retains panel-coloured outline");
      if (ordinaryPoints.some((point) => Number(point.getAttribute("r")) < 2.4 || Number(point.getAttribute("r")) > 2.8)) failures.push("ordinary Q–Q point radius is outside the stable diagnostic range");
      const legend = svg.querySelector<SVGGElement>(".cw-plot-legend");
      auditOrdinaryLegendBounds(svg, legend, failures, "Q–Q");
      if (legend?.dataset.layoutPosition !== "upper-left") failures.push("Q–Q legend is not in the expected upper-left axes corner");
    }
    if (figureName === "mattr") assertDistinctFills(".cw-plot-composition-segment", 2, "MATTR eligibility strip");
  }
  if (figureName === "rank") {
    const observed = assertPaint(".cw-plot-point:not(.is-excluded)", "fill", "rank observations", rejectBrowserBlack);
    if (auditedView === "curve") {
      const fitted = assertPaint(".cw-plot-fit-line", "stroke", "rank fitted line");
      if (observed && fitted && getComputedStyle(observed).fill === getComputedStyle(fitted).stroke) failures.push("rank observed and fitted paint are identical");
    }
  }
  if (figureName === "growth") {
    const observed = assertPaint(".cw-plot-line:not(.cw-plot-legend-swatch)", "stroke", "vocabulary growth observed line");
    const fitted = assertPaint(".cw-plot-fit-line:not(.cw-plot-legend-swatch)", "stroke", "vocabulary growth fit");
    const observedLegend = svg.querySelector<SVGGraphicsElement>(".cw-plot-legend-swatch.cw-plot-line");
    const fittedLegend = svg.querySelector<SVGGraphicsElement>(".cw-plot-legend-swatch.cw-plot-fit-line");
    if (!observed || !observedLegend || getComputedStyle(observed).stroke !== getComputedStyle(observedLegend).stroke) failures.push("growth observed legend paint does not match series");
    if (!fitted || !fittedLegend || getComputedStyle(fitted).stroke !== getComputedStyle(fittedLegend).stroke) failures.push("growth fitted legend paint does not match series");
    auditOrdinaryLegendBounds(svg, svg.querySelector<SVGGElement>(".cw-plot-legend"), failures, "Vocabulary-growth");
  }
  if (figureName === "lexical") {
    assertPaint(".cw-plot-point", "fill", "lexical exact-frequency point", rejectBrowserBlack);
    assertDistinctFills(".cw-plot-composition-segment", 3, "lexical composition strip");
  }
  if (figureName === "coverage") {
    assertPaint(".cw-plot-line", "stroke", "coverage cumulative line");
    assertPaint(".cw-plot-area", "fill", "coverage cumulative area", false, true);
    assertPaint(".cw-plot-coverage-crossing", "fill", "coverage target point");
    assertDistinctFills(".cw-plot-composition-segment", 3, "coverage composition strip");
    const gradientStops = Array.from(svg.querySelectorAll<SVGStopElement>(".cw-plot-area-stop"));
    if (gradientStops.length !== 3 || gradientStops.some((stop) => !isVisiblePaint(getComputedStyle(stop).stopColor))) {
      failures.push("coverage area gradient has unresolved stops");
    }
  }
  if (figureName === "profile") {
    assertDistinctFills(".cw-plot-composition-segment", 8, "dispersion profile ten-band composition strip");
    assertPaint(".cw-plot-point", "fill", "dispersion profile endpoint", rejectBrowserBlack);
    assertDistinctFills(".cw-plot-point:not(.is-excluded)", 8, "dispersion profile ordered endpoints");
    const internalEntries = Array.from(svg.querySelectorAll<SVGGElement>(".cw-plot-composition-entry.is-wide"))
      .filter((entry) => Array.from(entry.querySelectorAll<SVGTextElement>(".cw-plot-composition-label"))
        .every((label) => getComputedStyle(label).display !== "none" && Boolean(label.textContent?.trim())));
    if (internalEntries.length < 2) failures.push("two widest profile segments do not retain category and value internally");
    const figureSurface = getComputedStyle(svg.closest<HTMLElement>(".cw-overview-figure")!).backgroundColor;
    for (const entry of svg.querySelectorAll<SVGGElement>(".cw-plot-composition-entry:is(.is-medium, .is-narrow, .is-zero)")) {
      const label = entry.classList.contains("is-medium")
        ? entry.querySelector<SVGTextElement>(".cw-plot-composition-label.is-detail")
        : entry.querySelector<SVGTextElement>(".cw-plot-composition-label.is-category");
      const style = label ? getComputedStyle(label) : null;
      if (!label || !label.textContent?.trim() || style?.display === "none" || label.getBBox().width <= 0 || style.fill === figureSurface) {
        failures.push("external profile band is not visibly named");
        break;
      }
    }
    const callout = svg.querySelector<SVGRectElement>(".cw-plot-cumulative-callout");
    if (!callout || isWarmPaint(getComputedStyle(callout).fill) || isWarmPaint(getComputedStyle(callout).stroke)) failures.push("dispersion callout retains warning paint");
  }
  if (figureName === "plane") {
    assertDistinctFills(".cw-plot-density-cell:not(.is-zero)", 5, "dispersion plane density");
    const positiveCells = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-density-cell:not(.is-zero)"));
    const positiveOpacities = positiveCells.map((cell) => Number(getComputedStyle(cell).opacity)).sort((left, right) => left - right);
    if ((positiveOpacities[Math.floor(positiveOpacities.length / 2)] ?? 0) < 0.8) failures.push("dispersion density median opacity is below 0.8");
    const witnessFills = new Set(Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-plane-witness")).map((witness) => getComputedStyle(witness).fill));
    if (witnessFills.size !== 1) failures.push("dispersion witness classes do not share one base colour");
    const witnessShapes = new Set(Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-plane-witness")).map((witness) => witness.dataset.pointShape));
    if (witnessShapes.size !== 4) failures.push("dispersion witness classes do not retain four shapes");
    const pinnedWitness = svg.querySelector<SVGGraphicsElement>(".cw-plot-plane-witness.is-pinned");
    if (pinnedWitness && Number(getComputedStyle(pinnedWitness).strokeWidth.replace("px", "")) < 2.5) failures.push("pinned witness lacks a distinct outer selection treatment");
    const legendSteps = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-density-legend-step"));
    const legendFills = legendSteps.map((step) => getComputedStyle(step).fill);
    if (legendSteps.length !== 6 || new Set(legendFills).size !== 6) failures.push("density legend does not expose the six-step scale");
    const cellFills = new Set(Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-density-cell:not(.is-zero)"))
      .map((cell) => getComputedStyle(cell).fill));
    if (!legendFills.at(-1) || !cellFills.has(legendFills.at(-1)!)) failures.push("density legend high step does not match the cell scale");
    if (!svg.querySelector(".cw-plot-density-legend .cw-plot-legend-label")?.textContent?.includes("Log scale")) failures.push("density legend omits Log scale");
  }
  if (figureName === "coverage") {
    const disclosure = document.querySelector<HTMLElement>(".cw-overview-figure-teaching .analysis-glossary-term");
    const detail = document.querySelector<HTMLElement>(".cw-overview-figure-teaching .analysis-glossary-detail");
    if (!disclosure || getComputedStyle(disclosure).borderBottomStyle !== "none") failures.push("figure disclosure retains link styling");
    if (!detail || getComputedStyle(detail).backgroundColor !== "rgba(0, 0, 0, 0)") failures.push("figure disclosure detail is not footer-owned");
  }
}

function auditEcdfState(figure: HTMLElement, failures: string[], timing: string): void {
  const path = figure.querySelector<SVGPathElement>(".cw-plot-ecdf-line");
  if (!path) {
    failures.push(`missing ECDF path ${timing}`);
    return;
  }
  const style = getComputedStyle(path);
  const box = path.getBBox();
  if (!isVisiblePaint(style.stroke) || !(Number(style.strokeOpacity) > 0) || !(Number(style.opacity) > 0)) {
    failures.push(`invisible ECDF stroke ${timing}`);
  }
  if (!(box.width > 0) || !(box.height > 0)) failures.push(`zero-area ECDF path ${timing}`);
  const svg = path.ownerSVGElement;
  const targets = Array.from(svg?.querySelectorAll<SVGCircleElement>(".cw-plot-ecdf-hit-target") ?? []);
  if (targets.length === 0) failures.push(`missing ECDF interaction targets ${timing}`);
  for (const target of targets) {
    const targetStyle = getComputedStyle(target);
    if (target.classList.contains("cw-plot-point")) failures.push(`ECDF target uses ordinary point renderer ${timing}`);
    if (targetStyle.stroke !== "none" || Number(targetStyle.opacity) !== 0 || targetStyle.pointerEvents !== "all") {
      failures.push(`visible ECDF interaction target ${timing}`);
      break;
    }
    if (!target.dataset.plotX || !target.dataset.plotY || !target.dataset.tooltipTitle || !target.dataset.tooltipRows || target.getAttribute("tabindex") !== "-1") {
      failures.push(`incomplete ECDF interaction target ${timing}`);
      break;
    }
  }
  const median = svg?.querySelector<SVGLineElement>(".cw-plot-median-line");
  if (!median || getComputedStyle(median).stroke === "none") failures.push(`interrupted median line contract ${timing}`);
  const crosshairX = svg?.querySelector<SVGLineElement>(".cw-plot-crosshair-x");
  const crosshairY = svg?.querySelector<SVGLineElement>(".cw-plot-crosshair-y");
  const end = path.getPointAtLength(path.getTotalLength());
  const plotLeft = Number(crosshairY?.getAttribute("x1"));
  const plotRight = Number(crosshairY?.getAttribute("x2"));
  const plotTop = Number(crosshairX?.getAttribute("y1"));
  if (!Number.isFinite(end.x) || !Number.isFinite(end.y) || end.x < plotLeft || end.x > plotRight || Math.abs(end.y - plotTop) > 1) {
    failures.push(`ECDF final cumulative endpoint is not visible ${timing}`);
  }
}

function auditOrdinaryLegendBounds(
  svg: SVGSVGElement,
  legend: SVGGElement | null,
  failures: string[],
  label: string,
): void {
  const left = Number(svg.dataset.plotInnerLeft);
  const top = Number(svg.dataset.plotInnerTop);
  const right = Number(svg.dataset.plotInnerRight);
  const bottom = Number(svg.dataset.plotInnerBottom);
  const matrix = svg.getScreenCTM();
  if (!legend || !matrix || ![left, top, right, bottom].every(Number.isFinite)) {
    failures.push(`${label} legend has no measurable inner-plot bounds`);
    return;
  }
  const innerStart = new DOMPoint(left, top).matrixTransform(matrix);
  const innerEnd = new DOMPoint(right, bottom).matrixTransform(matrix);
  const inner = {
    left: Math.min(innerStart.x, innerEnd.x),
    top: Math.min(innerStart.y, innerEnd.y),
    right: Math.max(innerStart.x, innerEnd.x),
    bottom: Math.max(innerStart.y, innerEnd.y),
  };
  const bounds = legend.getBoundingClientRect();
  if (bounds.left < inner.left - 1 || bounds.top < inner.top - 1 || bounds.right > inner.right + 1 || bounds.bottom > inner.bottom + 1) {
    failures.push(`${label} legend leaves inner plot rectangle`);
  }
  if (legend.dataset.layoutPosition === "external-rail") failures.push(`${label} legend was placed in an external rail`);
}

function auditDistributionControlsAndGeometry(figure: HTMLElement, svg: SVGSVGElement, failures: string[]): void {
  const pressedView = figure.querySelector<HTMLButtonElement>(`.cw-overview-segmented-control[aria-label="View"] .cw-overview-segment[aria-pressed="true"]`)?.dataset.value;
  if (pressedView !== auditedView) failures.push("selected View and controller state disagree");
  const geometryMatches = auditedView === "histogram"
    ? svg.querySelector(".cw-plot-bar") !== null && svg.querySelector(".cw-plot-ecdf-line, .cw-plot-qq-point") === null
    : auditedView === "ecdf"
      ? svg.querySelector(".cw-plot-ecdf-line") !== null && svg.querySelector(".cw-plot-bar, .cw-plot-qq-point") === null
      : svg.querySelector(".cw-plot-qq-point") !== null && svg.querySelector(".cw-plot-bar, .cw-plot-ecdf-line") === null;
  if (!geometryMatches) failures.push("selected View and rendered geometry disagree");

  const visibleLabels = Array.from(figure.querySelectorAll<HTMLElement>(".cw-overview-control-field:not([hidden]) > .cw-overview-control-label"))
    .map((label) => label.textContent ?? "");
  const expectedLabels = auditedView === "histogram"
    ? ["View", "Binning", ...(customBins ? ["Bins"] : []), ...(figureName === "document" ? ["Scale"] : [])]
    : auditedView === "ecdf" && figureName === "document" ? ["View", "Scale"] : ["View"];
  if (visibleLabels.join("|") !== expectedLabels.join("|")) failures.push(`incorrect control inventory: ${visibleLabels.join(", ")}`);
}

function auditRecoveryGeometry(svg: SVGSVGElement, failures: string[]): void {
  const plotTop = Number(svg.querySelector<SVGLineElement>(".cw-plot-crosshair-x")?.getAttribute("y1"));
  const plotBottom = Number(svg.querySelector<SVGLineElement>(".cw-plot-crosshair-x")?.getAttribute("y2"));
  if (figureName === "rank" && auditedView === "residuals") {
    if (svg.querySelector(".cw-plot-point.is-excluded")) failures.push("excluded point rendered in rank residuals");
    for (const point of svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-point")) {
      const rank = Number(point.dataset.pinKey?.split(":")[1]);
      const rows = point.dataset.tooltipRows ?? "";
      const tokens = Number(rows.match(/(?:^|\n)([\d,]+) tokens(?:\n|$)/)?.[1].replaceAll(",", ""));
      if (!(rank >= 4 && rank <= 70) || !(tokens > 1) || !rows.includes("included in fit")) {
        failures.push("residual point outside fitted population");
        break;
      }
    }
    if (Array.from(svg.querySelectorAll(".cw-plot-legend-label")).some((label) => label.textContent?.includes("Excluded"))) {
      failures.push("residual legend describes excluded geometry");
    }
    const zeroY = Number(svg.querySelector<SVGLineElement>(".cw-plot-residual-zero")?.getAttribute("y1"));
    if (!Number.isFinite(zeroY) || Math.abs(zeroY - ((plotTop + plotBottom) / 2)) > 0.5) failures.push("residual zero is not centred");
  }
  if (figureName === "rank" && auditedView === "curve") {
    const band = svg.querySelector<SVGRectElement>(".cw-plot-fitted-range-band");
    const y = Number(band?.getAttribute("y"));
    const height = Number(band?.getAttribute("height"));
    const labelY = Number(svg.querySelector<SVGTextElement>(".cw-plot-fitted-range-label")?.getAttribute("y"));
    if (!band || y < plotTop || y + height > plotBottom || labelY < plotTop || labelY > plotBottom) {
      failures.push("fitted range leaves rank plot bounds");
    }
  }
  if (figureName === "mattr" && auditedView === "histogram") {
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
    for (const category of ["Dis legomena (2)", "3–9 occurrences", "10+ occurrences"]) {
      if (!Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-composition-label")).some((label) => label.textContent?.startsWith(category))) {
        failures.push(`lexical composition category missing: ${category}`);
      }
    }
    const svgBounds = svg.getBoundingClientRect();
    for (const label of Array.from(svg.querySelectorAll<SVGTextElement>(".cw-plot-label"))
      .filter((candidate) => /hapax · f = 1|dis legomena · f = 2/.test(candidate.textContent ?? ""))) {
      const bounds = label.getBoundingClientRect();
      if (bounds.left < svgBounds.left || bounds.top < svgBounds.top || bounds.right > svgBounds.right || bounds.bottom > svgBounds.bottom) {
        failures.push("lexical authored annotation outside SVG bounds");
      }
    }
  }
  if (figureName === "plane") {
    const legend = svg.querySelector<SVGGElement>(".cw-plot-density-legend");
    const witnessLabels = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-label-mono[class*='is-witness-']"));
    if (legend && witnessLabels.some((label) => rectanglesOverlap(legend.getBoundingClientRect(), label.getBoundingClientRect(), 2))) {
      failures.push("dispersion witness label overlaps density legend");
    }
    const automaticWitnessLabels = witnessLabels.filter((label) => !label.classList.contains("is-pinned"));
    const thresholdLabels = Array.from(svg.querySelectorAll<SVGGraphicsElement>(".cw-plot-label-mono.cw-plot-label-muted"));
    if (automaticWitnessLabels.some((label) => thresholdLabels.some((threshold) => rectanglesOverlap(label.getBoundingClientRect(), threshold.getBoundingClientRect(), 2)))) {
      failures.push("automatic dispersion label overlaps threshold text");
    }
  }
}

function auditInteractionCapability(svg: SVGSVGElement, failures: string[]): void {
  const crosshairExpected = figureName === "rank"
    || figureName === "growth"
    || figureName === "coverage"
    || figureName === "plane"
    || (figureName === "document" || figureName === "mattr") && auditedView !== "histogram";
  const crosshairLines = svg.querySelectorAll(".cw-plot-crosshair-x, .cw-plot-crosshair-y");
  if (svg.dataset.crosshairEnabled !== String(crosshairExpected)) failures.push("incorrect crosshair capability metadata");
  if (crosshairExpected && crosshairLines.length !== 2) failures.push("approved figure lacks crosshair geometry");
  if (!crosshairExpected && crosshairLines.length !== 0) failures.push("unapproved figure has crosshair geometry");
}

function isVisiblePaint(value: string): boolean {
  if (!value || value === "none" || value === "transparent") return false;
  const alpha = value.match(/^rgba?\([^/]+(?:\/\s*([\d.]+))?\)$/)?.[1];
  return alpha === undefined || Number(alpha) > 0;
}

function isPureBlack(value: string): boolean {
  return /^(?:rgb\(0,\s*0,\s*0\)|rgba\(0,\s*0,\s*0,\s*1\)|#000(?:000)?)$/i.test(value.trim());
}

function isWarmPaint(value: string): boolean {
  const channels = value.match(/[\d.]+/g)?.slice(0, 3).map(Number);
  return Boolean(channels && channels[0]! > channels[1]! * 1.08 && channels[0]! > channels[2]! * 1.08);
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

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}
