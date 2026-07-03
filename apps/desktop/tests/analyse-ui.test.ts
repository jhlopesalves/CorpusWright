import { existsSync, readdirSync, readFileSync } from "node:fs";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const analysis = readFileSync(new URL("../src/analysis.ts", import.meta.url), "utf8");
const workbench = readWorkbenchSources();
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const generatedConcordanceQuery = readFileSync(new URL("../src/generated/ConcordanceQuery.ts", import.meta.url), "utf8");
const generatedConcordanceContextFilterDirection = readFileSync(
  new URL("../src/generated/ConcordanceContextFilterDirection.ts", import.meta.url),
  "utf8",
);

function readWorkbenchSources(): string {
  const files: URL[] = [];
  const legacyWorkbench = new URL("../src/workbench.ts", import.meta.url);
  if (existsSync(legacyWorkbench)) {
    files.push(legacyWorkbench);
  }

  const workbenchRoot = new URL("../src/workbench/", import.meta.url);
  if (existsSync(workbenchRoot)) {
    collectTypescriptFiles(workbenchRoot, files);
  }

  return files
    .sort((left, right) => left.pathname.localeCompare(right.pathname))
    .map((file) => readFileSync(file, "utf8"))
    .join("\n");
}

function collectTypescriptFiles(directory: URL, files: URL[]): void {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const child = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, directory);
    if (entry.isDirectory()) {
      collectTypescriptFiles(child, files);
    } else if (entry.isFile() && entry.name.endsWith(".ts")) {
      files.push(child);
    }
  }
}

const topAnalyseMatch = html.match(
  /<div class="analyse-status-row">([\s\S]*?)<div class="analyse-workspace-shell">/,
);
assert(topAnalyseMatch, "Top Analyse status row should exist");
const topAnalyse = topAnalyseMatch[1];

assert(
  topAnalyse.includes("Build surface analysis"),
  "Top Analyse action should build surface analysis",
);
assert(
  !/annotation/i.test(topAnalyse),
  "Top Analyse area should not contain annotation controls or copy",
);
assert(
  workbench.includes('"Rebuild surface analysis"'),
  "Runtime Analyse action should rebuild surface analysis",
);
assert(
  html.includes('id="frequency-annotation-layer"'),
  "Frequency List should contain the annotation-layer mount point",
);
assert(
  !html.includes("Build annotation cache"),
  "Primary UI copy should not say Build annotation cache",
);

const frequencyHeadMatch = html.match(/<thead id="frequency-table-head">([\s\S]*?)<\/thead>/);
assert(frequencyHeadMatch, "Frequency table head should exist");
const frequencyHead = frequencyHeadMatch[1];
for (const label of ["Token", "Frequency", "Range (docs)", "Dispersion (DP)"]) {
  assert(frequencyHead.includes(label), `Default surface table should render ${label}`);
}
for (const label of ["Per 10k", "Per 100k", "Per million", "Top-doc share"]) {
  assert(!frequencyHead.includes(label), `Default frequency table should not render ${label}`);
}
assert(
  html.includes('<option value="off" selected>Off</option>'),
  "Rate control should default to Off",
);
assert(
  html.includes("Normalised rates are optional and can be unstable in very short files; inspect raw counts and document tokens."),
  "Rate help text should use the compact methodological warning",
);
assert(
  !html.includes("frequency-language-display"),
  "Frequency controls should not include an editable-looking language field",
);
assert(
  !workbench.includes("Top-doc share"),
  "Frequency renderer should not render Top-doc share by default",
);

const concordanceHeadMatch = html.match(/<table class="concordance-table">([\s\S]*?)<\/table>/);
assert(concordanceHeadMatch, "Concordance table should exist");
const concordanceTable = concordanceHeadMatch[1];
for (const label of ["File", "Left context", "Node", "Right context"]) {
  assert(concordanceTable.includes(label), `Concordance table should render ${label}`);
}
for (const sort of ["filePosition", "node", "left1", "left2", "left3", "right1", "right2", "right3"]) {
  assert(html.includes(`value="${sort}"`), `Concordance sort should include ${sort}`);
}
assert(
  html.includes("Enter a word or phrase to see concordance lines."),
  "Concordance no-query copy should invite word or phrase search",
);
assert(
  !html.includes('id="concordance-result-count"'),
  "Concordance header should not render a duplicate standalone hit count",
);

const concordanceControlsStart = html.indexOf('<div class="concordance-controls">');
const concordanceStatusStart = html.indexOf('<div id="concordance-status"', concordanceControlsStart);
assert(concordanceControlsStart >= 0 && concordanceStatusStart > concordanceControlsStart, "Concordance shared controls should exist before the status line");
const concordanceSharedControls = html.slice(concordanceControlsStart, concordanceStatusStart);
assert(
  concordanceSharedControls.includes('id="concordance-query-input"')
    && concordanceSharedControls.includes('id="concordance-case-sensitive"')
    && concordanceSharedControls.includes('id="concordance-files-button"')
    && concordanceSharedControls.includes('id="concordance-search-button"')
    && !concordanceSharedControls.includes('id="concordance-window-input"')
    && !concordanceSharedControls.includes('id="concordance-sort-select"')
    && !concordanceSharedControls.includes('id="concordance-page-size-input"'),
  "Concordance shared controls should contain only query, case, file scope, and search controls",
);
const concordanceTabsStart = html.indexOf('id="concordance-view-tabs"', concordanceStatusStart);
const concordanceViewsStart = html.indexOf('<div class="concordance-views">', concordanceTabsStart);
const concordanceTabs = html.slice(concordanceTabsStart, concordanceViewsStart);
assert(
  concordanceTabs.includes('id="concordance-lines-tab"')
    && concordanceTabs.includes('data-concordance-view="lines"')
    && concordanceTabs.includes(">Concordance lines</button>")
    && concordanceTabs.includes('id="concordance-collocates-tab"')
    && concordanceTabs.includes('data-concordance-view="collocates"')
    && concordanceTabs.includes(">Collocates</button>")
    && concordanceTabs.includes('id="concordance-distribution-tab"')
    && concordanceTabs.includes('data-concordance-view="distribution"')
    && concordanceTabs.includes(">Distribution</button>"),
  "Concordance should expose Concordance lines, Collocates, and Distribution as query view tabs",
);
assert(
  !concordanceTabs.includes(">Evidence lines</button>")
    && !concordanceTabs.includes(">Collocate profile</button>"),
  "Concordance view tabs should not use the old Evidence lines or Collocate profile labels",
);
assert(
  concordanceStatusStart < concordanceTabsStart && concordanceTabsStart < concordanceViewsStart,
  "Concordance status should sit above the query view selector and panels",
);
const concordanceLinesStart = html.indexOf('id="concordance-lines-panel"', concordanceViewsStart);
const concordanceCollocationsStart = html.indexOf('id="concordance-collocations-panel"', concordanceLinesStart);
const concordanceDistributionStart = html.indexOf('id="concordance-distribution-panel"', concordanceCollocationsStart);
assert(
  concordanceLinesStart > concordanceViewsStart
    && concordanceCollocationsStart > concordanceLinesStart
    && concordanceDistributionStart > concordanceCollocationsStart,
  "Concordance view panels should render as Concordance lines, Collocates, and Distribution siblings",
);
const concordanceLinesPanel = html.slice(concordanceLinesStart, concordanceCollocationsStart);
const concordanceCollocationsPanel = html.slice(concordanceCollocationsStart, concordanceDistributionStart);
assert(
  concordanceLinesPanel.includes('class="concordance-lines-controls"')
    && concordanceLinesPanel.includes('id="concordance-window-input"')
    && concordanceLinesPanel.includes('id="concordance-sort-select"')
    && concordanceLinesPanel.includes('id="concordance-page-size-input"')
    && !concordanceLinesPanel.includes('id="concordance-distribution-panel"'),
  "Concordance Lines controls should live inside the Lines view panel without nesting Distribution",
);
assert(
  concordanceCollocationsPanel.includes('class="concordance-collocation-controls"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-span"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-direction"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-measure"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-min-o11"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-min-freq"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-hide-stopwords"'),
  "Collocates controls should remain inside the Collocates panel",
);
assert(
  html.includes('id="concordance-distribution-panel" class="concordance-view-panel concordance-distribution-panel hidden"')
    && html.includes('data-concordance-view-panel="distribution"')
    && html.includes('aria-labelledby="concordance-distribution-tab"')
    && html.includes('aria-hidden="true"'),
  "Distribution should render as a hidden sibling view panel",
);
assert(
  !html.includes('id="concordance-source-preview"'),
  "Concordance source preview should not use a global bottom-of-table mount",
);
assert(
  !html.includes("concordance-results-grid") && !styles.includes("concordance-results-grid"),
  "Concordance should not use the old side-by-side results grid",
);
assert(
  html.includes('id="concordance-collocations-summary"')
    && html.includes('id="concordance-collocation-span"')
    && html.includes('id="concordance-collocation-direction"')
    && html.includes('id="concordance-collocation-measure"')
    && html.includes('id="concordance-collocation-min-o11"')
    && html.includes('id="concordance-collocation-min-freq"')
    && html.includes('id="concordance-collocation-hide-stopwords"'),
  "Concordance should expose collocation controls",
);
for (const label of ["Collocate", "Near query", "O11", "Corpus frequency", "f_c", "Association: logDice"]) {
  assert(html.includes(label), `Collocation table should render ${label}`);
}
for (const label of [
  "CONCORDANCE_COLLOCATION_NOT_COMPUTED_MESSAGE",
  "CONCORDANCE_COLLOCATION_STALE_MESSAGE",
  "Update profile",
]) {
  assert(workbench.includes(label), `Collocate Profile should render ${label}`);
}
assert(
  !workbench.includes("CONCORDANCE_COLLOCATION_STALE_DETAIL")
    && !workbench.includes("Update collocate profile"),
  "Collocate Profile stale copy should not concatenate repeated implementation-state text",
);
assert(
  !workbench.includes("Compute collocates")
    && !workbench.includes("Show lines")
    && !workbench.includes("Show KWIC lines"),
  "Collocate Profile should avoid resting compute buttons and glued Show lines row text",
);
for (const measure of ["logDice", "mi", "mi3", "tScore", "zScore", "g2", "logRatio"]) {
  assert(html.includes(`value="${measure}"`), `Collocation measure should include ${measure}`);
}
assert(
  generatedConcordanceContextFilterDirection.includes('export type ConcordanceContextFilterDirection = "left" | "right" | "both";')
    && generatedConcordanceQuery.includes('import type { ConcordanceContextFilterDirection } from "./ConcordanceContextFilterDirection.js";')
    && generatedConcordanceQuery.includes("contextFilterDirection: ConcordanceContextFilterDirection")
    && analysis.includes("GeneratedConcordanceContextFilterDirection")
    && analysis.includes("export type ConcordanceContextFilterDirection = GeneratedConcordanceContextFilterDirection;"),
  "Generated bindings and the analysis type barrel should expose ConcordanceContextFilterDirection",
);
assert(
  workbench.includes("function handleConcordanceSortChange")
    && workbench.includes("void refreshConcordance(true, true);"),
  "Concordance sort changes should reset KWIC and distribution pagination",
);
assert(
  workbench.includes("concordanceSummaryMetricsText(result)")
    && workbench.includes('query.className = "concordance-summary-query"')
    && workbench.includes('metrics.className = "concordance-summary-scope"')
    && !workbench.includes("createInlineDispersionBar")
    && !workbench.includes("concordance-dp-track"),
  "Concordance summary should emphasise only the query and keep DP plain",
);
assert(
  workbench.includes("Searching...") && workbench.includes("CONCORDANCE_NO_HITS_MESSAGE"),
  "Concordance renderer should include loading and no-hit states",
);
const runConcordanceSearchMatch = workbench.match(
  /function runConcordanceSearch\(\): void \{([\s\S]*?)\n\}/,
);
assert(runConcordanceSearchMatch, "Concordance search handler should exist");
assert(
  runConcordanceSearchMatch[1].includes("void runConcordanceSearchForActiveView();")
    && !runConcordanceSearchMatch[1].includes("setActiveConcordanceView"),
  "Search should delegate to the active Concordance view workflow without forcing Concordance lines",
);
const runConcordanceSearchForActiveViewMatch = workbench.match(
  /async function runConcordanceSearchForActiveView\(\): Promise<void> \{([\s\S]*?)\n\}/,
);
assert(runConcordanceSearchForActiveViewMatch, "Active-view Concordance search helper should exist");
const runConcordanceSearchForActiveViewBody = runConcordanceSearchForActiveViewMatch[1];
const collocationRefreshesFromSearch = (
  runConcordanceSearchForActiveViewBody.match(/refreshConcordanceCollocations\(/g) ?? []
).length;
assert(
  runConcordanceSearchForActiveViewBody.includes("const viewAtSearch = activeConcordanceView;")
    && runConcordanceSearchForActiveViewBody.includes("const queryAtSearch = normaliseConcordanceQuery(dom.concordanceQueryInput.value);")
    && runConcordanceSearchForActiveViewBody.includes("clearCollocateEvidence();")
    && runConcordanceSearchForActiveViewBody.includes("await refreshConcordance();"),
  "Active-view Concordance search should capture the current view/query, clear collocate evidence, and refresh the query result first",
);
assert(
  runConcordanceSearchForActiveViewBody.includes('viewAtSearch === "collocates"')
    && runConcordanceSearchForActiveViewBody.includes('activeConcordanceView === "collocates"')
    && runConcordanceSearchForActiveViewBody.includes("normaliseConcordanceQuery(dom.concordanceQueryInput.value) === queryAtSearch")
    && runConcordanceSearchForActiveViewBody.includes("concordanceResult !== null")
    && runConcordanceSearchForActiveViewBody.includes("concordanceFailure === null")
    && runConcordanceSearchForActiveViewBody.includes("await refreshConcordanceCollocations();")
    && collocationRefreshesFromSearch === 1,
  "Search should update Collocate Profile directly only when the Collocate Profile view is still active for the same successful query",
);
assert(
  !runConcordanceSearchForActiveViewBody.includes('viewAtSearch === "distribution"')
    && !runConcordanceSearchForActiveViewBody.includes("maybeAutoComputeConcordanceCollocations")
    && !runConcordanceSearchForActiveViewBody.includes("setActiveConcordanceView"),
  "Distribution search should rely on the refreshed Concordance result and avoid collocation computation or forced view changes",
);
assert(
  workbench.includes("queryConcordanceCollocations")
    && workbench.includes("sortKey: concordanceCollocationSortKey")
    && workbench.includes("sortDirection: currentConcordanceCollocationSortDirection()")
    && workbench.includes("function handleConcordanceCollocationSortClick")
    && workbench.includes("function handleConcordanceCollocationAggregationControlChange")
    && workbench.includes("function handleConcordanceCollocationPresentationControlChange")
    && workbench.includes("refreshConcordanceCollocationsForPresentationChange")
    && workbench.includes("markConcordanceCollocationsForAggregationChange")
    && workbench.includes("concordanceCollocationAutoComputedAggregationKeys")
    && !workbench.includes("sortConcordanceCollocates(result.rows, measure)")
    && workbench.includes("handleConcordanceCollocateClick"),
  "Concordance collocations should query Rust for true top-N sorting, split aggregation/presentation controls, auto-compute fresh keys once, and support evidence drawers",
);
assert(
  workbench.includes('type ConcordanceViewId = "lines" | "collocates" | "distribution";')
    && workbench.includes('return value === "lines" || value === "collocates" || value === "distribution";')
    && workbench.includes('{ view: "distribution", element: dom.concordanceDistributionPanel }')
    && workbench.includes('type ConcordanceCollocationState = "idle_not_computed" | "loading" | "ready" | "stale" | "error";')
    && workbench.includes("function syncConcordanceViewTabs")
    && workbench.includes("function maybeAutoComputeConcordanceCollocations")
    && workbench.includes('dom.concordanceViewTabs.addEventListener("click", handleConcordanceViewTabClick)')
    && workbench.includes('if (view === "collocates") {\n    maybeAutoComputeConcordanceCollocations();\n  }')
    && !workbench.includes("function handleConcordanceViewTabClick(event: MouseEvent): void {\n  void refreshConcordance"),
  "Concordance tab switching should recognise Distribution and only lazy-compute Collocate Profile",
);
assert(
  workbench.includes('table.className = "concordance-distribution-table"')
    && workbench.includes('panel.className = "concordance-document-spread-panel"')
    && workbench.includes('panelHeader.className = "concordance-document-spread-header"')
    && workbench.includes('metrics.className = "concordance-distribution-metrics"')
    && workbench.includes('title.textContent = "Distribution"')
    && !workbench.includes('title.textContent = `Distribution — ${concordanceSummaryMetricsText(result)}`')
    && !workbench.includes('const panel = document.createElement("details");\n  panel.className = "concordance-document-spread-panel";'),
  "Concordance Distribution should render directly with a compact title and metric strip rather than as a collapsed details widget",
);
const renderConcordanceDistributionMatch = workbench.match(
  /function renderConcordanceDistribution\(\n  result: ConcordanceResult \| null,\n  emptyMessage = CONCORDANCE_NO_QUERY_MESSAGE,\n\): void \{([\s\S]*?)\n\}/,
);
assert(renderConcordanceDistributionMatch, "Concordance Distribution renderer should exist");
const renderConcordanceDistributionBody = renderConcordanceDistributionMatch[1];
assert(
  renderConcordanceDistributionBody.includes('for (const label of ["File", "Hits", "Tokens", "Hits per 10k", "Share of hits"])')
    && !renderConcordanceDistributionBody.includes('"Bar"')
    && renderConcordanceDistributionBody.includes("cell.colSpan = 5;"),
  "Concordance Distribution should expose metric-first columns and a five-column empty state",
);
assert(
  renderConcordanceDistributionBody.includes('metrics.className = "concordance-distribution-metrics"')
    && renderConcordanceDistributionBody.includes('hitMetric.className = "concordance-distribution-metric"')
    && renderConcordanceDistributionBody.includes('hitLabel.className = "concordance-distribution-metric-label"')
    && renderConcordanceDistributionBody.includes("formatNumber(result.totalHits)")
    && renderConcordanceDistributionBody.includes("result.documentsWithHits")
    && renderConcordanceDistributionBody.includes("result.selectedDocumentsTotal")
    && renderConcordanceDistributionBody.includes("concordanceRangePercent(result)")
    && renderConcordanceDistributionBody.includes("result.topDocumentShare * 100")
    && renderConcordanceDistributionBody.includes("result.dp")
    && renderConcordanceDistributionBody.includes("result.dispersionLabel"),
  "Concordance Distribution should render a metric strip from existing distribution result fields",
);
assert(
  styles.includes(".concordance-distribution-metrics")
    && styles.includes(".concordance-distribution-metric")
    && styles.includes(".concordance-distribution-metric-label")
    && styles.includes("grid-template-columns: repeat(auto-fit, minmax(8rem, max-content));"),
  "Concordance Distribution metric strip should have compact wrapping CSS",
);
assert(
  renderConcordanceDistributionBody.includes("item.ratePer10k")
    && renderConcordanceDistributionBody.includes("item.count / result.totalHits * 100")
    && renderConcordanceDistributionBody.includes("formatFixedNumber(item.ratePer10k, 2)")
    && renderConcordanceDistributionBody.includes("formatFixedNumber(shareOfHits, 1)"),
  "Concordance Distribution should render Hits per 10k and Share of hits from existing result data",
);
assert(
  !renderConcordanceDistributionBody.includes('track.className = "frequency-distribution-track"')
    && !renderConcordanceDistributionBody.includes('fill.className = "frequency-distribution-fill"')
    && !renderConcordanceDistributionBody.includes("--bar-width"),
  "Concordance Distribution should not create bar tracks or fills",
);
assert(
  workbench.includes("addEventListener(\"click\", () => {\n    void toggleConcordanceExpansion(row);")
    && workbench.includes("event.key !== \"Enter\" && event.key !== \" \"")
    && workbench.includes("aria-expanded"),
  "Concordance rows should support click, keyboard toggle, and aria-expanded",
);
assert(
  workbench.includes('item.title = "Show wider context";')
    && !workbench.includes('item.title = "Show source preview";'),
  "Concordance row expansion affordance should describe wider context rather than source preview",
);
assert(
  workbench.includes("concordanceExpansionViewModel")
    && workbench.includes("function createConcordanceSourcePreviewRow")
    && workbench.includes('item.className = "concordance-preview-row"')
    && workbench.includes('cell.className = "concordance-preview-cell"')
    && workbench.includes("cell.appendChild(createConcordanceSourcePreviewPanel(row))")
    && workbench.includes("dom.concordanceTableBody.appendChild(createConcordanceSourcePreviewRow(row))")
    && workbench.includes("concordance-expansion-message is-")
    && workbench.includes("concordance-expansion-node")
    && workbench.includes("handleConcordanceExpansionEscape")
    && !workbench.includes("function renderConcordanceSourcePreview")
    && !workbench.includes("dom.concordanceSourcePreview"),
  "Concordance renderer should include ready, loading, error, and Escape-close preview states immediately after the selected row",
);
const concordanceRowsLoopMatch = workbench.match(
  /for \(const row of result\.rows\) \{([\s\S]*?)\n    \}/,
);
assert(concordanceRowsLoopMatch, "Concordance renderer should append rows in a single result loop");
assert(
  concordanceRowsLoopMatch[1].indexOf("dom.concordanceTableBody.appendChild(rowElement);")
    < concordanceRowsLoopMatch[1].indexOf("dom.concordanceTableBody.appendChild(createConcordanceSourcePreviewRow(row));"),
  "Concordance source preview row should be appended immediately after the selected KWIC row",
);
assert(
  styles.includes(".concordance-preview-row .concordance-expansion-panel")
    && styles.includes(".concordance-table .concordance-preview-cell")
    && !styles.includes("concordance-expansion-cell")
    && !styles.includes(".concordance-source-preview"),
  "Concordance source preview should render in a local preview row without the old nowrap expansion cell or bottom mount",
);
const concordanceExpansionCssMatch = styles.match(/\.concordance-expansion-context \{([\s\S]*?)\n\}/);
assert(concordanceExpansionCssMatch, "Concordance source preview CSS should exist");
const concordanceExpansionCss = concordanceExpansionCssMatch[1];
assert(
  concordanceExpansionCss.includes("white-space: normal")
    && concordanceExpansionCss.includes("overflow-wrap: anywhere")
    && !concordanceExpansionCss.includes("white-space: nowrap"),
  "Concordance source preview should wrap instead of expanding the table sideways",
);
assert(
  workbench.includes("collapseConcordanceExpansionFromControlChange")
    && workbench.includes("function handleConcordanceQueryInput")
    && workbench.includes("dom.concordanceQueryInput.addEventListener(\"input\", handleConcordanceQueryInput)")
    && workbench.includes("dom.concordanceWindowInput.addEventListener(\"change\", collapseConcordanceExpansionFromControlChange)")
    && workbench.includes("function handleConcordanceCaseChange"),
  "Query, window, and case changes should collapse expanded Concordance rows, with query changes dirtying Collocates",
);
assert(
  workbench.includes("function showPreviousConcordancePage")
    && workbench.includes("void refreshConcordance(false, false);")
    && workbench.includes("function handleConcordancePageSizeChange")
    && workbench.includes("collapseConcordanceExpansion();\n  void refreshFrequencyPage();"),
  "Page, page-size, and file-scope changes should collapse expanded Concordance rows",
);
assert(
  workbench.includes("renderFrequencyScopeControl(state.analysisCorpus);")
    && workbench.includes("dom.concordanceFilesButton.textContent = dom.frequencyFilesButton.textContent"),
  "Concordance file-scope button should refresh from the shared Frequency scope",
);
const handleConcordanceCollocateStart = workbench.indexOf("function handleConcordanceCollocateClick");
const handleConcordanceCollocateEnd = workbench.indexOf("function createConcordanceCollocationRow", handleConcordanceCollocateStart);
assert(handleConcordanceCollocateStart >= 0 && handleConcordanceCollocateEnd > handleConcordanceCollocateStart, "Collocate click handler should exist");
const handleConcordanceCollocateBody = workbench.slice(handleConcordanceCollocateStart, handleConcordanceCollocateEnd);
assert(
  handleConcordanceCollocateBody.includes("collocateEvidenceSelection = {")
    && handleConcordanceCollocateBody.includes("direction: currentConcordanceCollocationDirection() as ConcordanceContextFilterDirection")
    && handleConcordanceCollocateBody.includes("collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT")
    && handleConcordanceCollocateBody.includes("void refreshCollocateEvidence();")
    && !handleConcordanceCollocateBody.includes("preserveResult")
    && !handleConcordanceCollocateBody.includes("setActiveConcordanceView")
    && !workbench.includes("concordanceContextFilter"),
  "Collocate clicks should open dedicated inline evidence without switching to Concordance lines or mutating the base context filter",
);
const refreshConcordanceStart = workbench.indexOf("async function refreshConcordance(");
const refreshConcordanceEnd = workbench.indexOf("async function refreshCollocateEvidence", refreshConcordanceStart);
const refreshCollocateEvidenceEnd = workbench.indexOf("async function refreshConcordanceCollocations", refreshConcordanceEnd);
assert(
  refreshConcordanceStart >= 0
    && refreshConcordanceEnd > refreshConcordanceStart
    && refreshCollocateEvidenceEnd > refreshConcordanceEnd,
  "Base and collocate evidence refresh helpers should exist in sequence",
);
const refreshConcordanceBody = workbench.slice(refreshConcordanceStart, refreshConcordanceEnd);
const refreshCollocateEvidenceBody = workbench.slice(refreshConcordanceEnd, refreshCollocateEvidenceEnd);
assert(
  refreshConcordanceBody.includes("contextFilter: null")
    && refreshConcordanceBody.includes('contextFilterDirection: "both"')
    && refreshCollocateEvidenceBody.includes("const result = await searchConcordance(")
    && refreshCollocateEvidenceBody.includes("contextFilter: selection.surface")
    && refreshCollocateEvidenceBody.includes("contextFilterDirection: selection.direction")
    && refreshCollocateEvidenceBody.includes("offset: 0")
    && refreshCollocateEvidenceBody.includes("limit: collocateEvidenceLimit")
    && refreshCollocateEvidenceBody.includes("options: { preserveResult?: boolean } = {}")
    && refreshCollocateEvidenceBody.includes("if (!options.preserveResult) {\n    collocateEvidenceResult = null;\n  }")
    && !refreshCollocateEvidenceBody.includes("collocateEvidenceFailure = null;\n  collocateEvidenceResult = null;\n  renderConcordanceCollocations();")
    && !refreshCollocateEvidenceBody.includes("collocateEvidenceOffset"),
  "Base Concordance refresh should stay unfiltered while the collocate drawer performs a separate direction-aware cumulative evidence search with optional preserved results",
);
const collocateEvidenceLimitMatch = workbench.match(/const CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT = (\d+);/);
assert(
  collocateEvidenceLimitMatch?.[1] === "8",
  "Collocate evidence drawer should expand in compact 8-line increments",
);
const clearCollocateEvidenceMatch = workbench.match(/function clearCollocateEvidence\(\): void \{([\s\S]*?)\n\}/);
assert(clearCollocateEvidenceMatch, "Collocate evidence clear helper should exist");
assert(
  clearCollocateEvidenceMatch[1].includes("collocateEvidenceLimit = CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT")
    && clearCollocateEvidenceMatch[1].includes("collocateEvidenceExpansionKey = null")
    && clearCollocateEvidenceMatch[1].includes("collocateEvidenceExpansionPending = false")
    && clearCollocateEvidenceMatch[1].includes("collocateEvidenceExpansionFailure = null")
    && clearCollocateEvidenceMatch[1].includes("collocateEvidenceExpansionResult = null")
    && clearCollocateEvidenceMatch[1].includes("collocateEvidenceExpansionGeneration += 1")
    && !clearCollocateEvidenceMatch[1].includes("collocateEvidenceOffset"),
  "Collocate evidence clearing should reset the expanding limit and drawer expansion without retaining offset state",
);
assert(
  workbench.includes("let collocateEvidenceExpansionKey: string | null = null;")
    && workbench.includes("let collocateEvidenceExpansionPending = false;")
    && workbench.includes("let collocateEvidenceExpansionFailure: string | null = null;")
    && workbench.includes("let collocateEvidenceExpansionResult: ConcordanceExpansionResult | null = null;")
    && workbench.includes("let collocateEvidenceExpansionGeneration = 0;"),
  "Collocate evidence expansion should keep drawer-local state variables",
);
const refreshExpansionResetIndex = refreshCollocateEvidenceBody.indexOf("collocateEvidenceExpansionKey = null;");
const refreshSelectionIndex = refreshCollocateEvidenceBody.indexOf("const selection = collocateEvidenceSelection;");
assert(
  refreshExpansionResetIndex >= 0
    && refreshExpansionResetIndex < refreshSelectionIndex
    && refreshCollocateEvidenceBody.includes("collocateEvidenceExpansionPending = false;")
    && refreshCollocateEvidenceBody.includes("collocateEvidenceExpansionFailure = null;")
    && refreshCollocateEvidenceBody.includes("collocateEvidenceExpansionResult = null;")
    && refreshCollocateEvidenceBody.includes("collocateEvidenceExpansionGeneration += 1;"),
  "Collocate evidence refresh should clear drawer expansion before preserve/non-preserve loading branches",
);
const collocateEvidenceSpanTextStart = workbench.indexOf("function collocateEvidenceSpanText");
const collocateEvidenceSpanTextEnd = workbench.indexOf("function isActiveCollocateEvidenceRow", collocateEvidenceSpanTextStart);
assert(
  collocateEvidenceSpanTextStart >= 0
    && collocateEvidenceSpanTextEnd > collocateEvidenceSpanTextStart,
  "Collocate evidence span helper should exist before active-row checks",
);
const collocateEvidenceSpanTextBody = workbench.slice(collocateEvidenceSpanTextStart, collocateEvidenceSpanTextEnd);
assert(
  collocateEvidenceSpanTextBody.includes('return `span ${formatNumber(span)}L`;')
    && collocateEvidenceSpanTextBody.includes('return `span ${formatNumber(span)}R`;')
    && collocateEvidenceSpanTextBody.includes('return `span ${formatNumber(span)}L–${formatNumber(span)}R`;')
    && !workbench.includes("collocateEvidenceRelationText")
    && !workbench.includes("collocateEvidenceSideText"),
  "Collocate evidence span copy should use L/R span notation and remove relation/side wording helpers",
);
const toggleCollocateEvidenceExpansionStart = workbench.indexOf("async function toggleCollocateEvidenceExpansion");
const toggleCollocateEvidenceExpansionEnd = workbench.indexOf(
  "function renderConcordanceTableState",
  toggleCollocateEvidenceExpansionStart,
);
assert(
  toggleCollocateEvidenceExpansionStart >= 0
    && toggleCollocateEvidenceExpansionEnd > toggleCollocateEvidenceExpansionStart,
  "Collocate evidence expansion toggle should exist before Concordance table rendering",
);
const toggleCollocateEvidenceExpansionBody = workbench.slice(
  toggleCollocateEvidenceExpansionStart,
  toggleCollocateEvidenceExpansionEnd,
);
assert(
  toggleCollocateEvidenceExpansionBody.includes("const rowKey = concordanceRowExpansionKey(row);")
    && toggleCollocateEvidenceExpansionBody.includes("toggleConcordanceExpansionKey(collocateEvidenceExpansionKey, rowKey)")
    && toggleCollocateEvidenceExpansionBody.includes("const result = await expandConcordanceContext(")
    && toggleCollocateEvidenceExpansionBody.includes("windowTokens: CONCORDANCE_EXPANSION_WINDOW")
    && toggleCollocateEvidenceExpansionBody.includes("state.currentCorpusVersion")
    && toggleCollocateEvidenceExpansionBody.includes("effectiveCleaningConfig()")
    && toggleCollocateEvidenceExpansionBody.includes("renderConcordanceCollocations();")
    && !toggleCollocateEvidenceExpansionBody.includes("concordanceExpansionKey"),
  "Collocate evidence expansion should reuse the expansion command with drawer-local state only",
);
const collocateEvidenceSnippetMatch = workbench.match(
  /function createCollocateEvidenceSnippet\(row: ConcordanceRow\): HTMLSpanElement \{([\s\S]*?)\n\}/,
);
assert(collocateEvidenceSnippetMatch, "Collocate evidence snippet helper should exist");
const collocateEvidenceSnippetBody = collocateEvidenceSnippetMatch[1];
const collocateEvidenceSnippetSpaceCount = (
  collocateEvidenceSnippetBody.match(/snippet\.appendChild\(document\.createTextNode\(" "\)\);/g) ?? []
).length;
const collocateEvidenceRendererStart = workbench.indexOf("function createCollocateEvidenceSnippet");
const collocateEvidenceRendererEnd = workbench.indexOf(
  "function appendCollocateEvidenceShowMore",
  collocateEvidenceRendererStart,
);
assert(
  collocateEvidenceRendererStart >= 0 && collocateEvidenceRendererEnd > collocateEvidenceRendererStart,
  "Collocate evidence renderer helpers should be grouped before show-more controls",
);
const collocateEvidenceRendererBody = workbench.slice(collocateEvidenceRendererStart, collocateEvidenceRendererEnd);
const collocateEvidenceShowMoreStart = collocateEvidenceRendererEnd;
const collocateEvidenceShowMoreEnd = workbench.indexOf(
  "function createCollocateEvidenceDrawerRow",
  collocateEvidenceShowMoreStart,
);
assert(
  collocateEvidenceShowMoreStart >= 0 && collocateEvidenceShowMoreEnd > collocateEvidenceShowMoreStart,
  "Collocate evidence show-more helper should exist before the drawer row renderer",
);
const collocateEvidenceShowMoreBody = workbench.slice(collocateEvidenceShowMoreStart, collocateEvidenceShowMoreEnd);
const collocateEvidenceDrawerStart = collocateEvidenceShowMoreEnd;
const collocateEvidenceDrawerEnd = workbench.indexOf(
  "function handleConcordanceCollocateClick",
  collocateEvidenceDrawerStart,
);
assert(
  collocateEvidenceDrawerStart >= 0 && collocateEvidenceDrawerEnd > collocateEvidenceDrawerStart,
  "Collocate evidence drawer renderer should exist before the click handler",
);
const collocateEvidenceDrawerBody = workbench.slice(collocateEvidenceDrawerStart, collocateEvidenceDrawerEnd);
const legacyCollocateEvidenceMoreCopy = "Show more " + "evidence";
assert(
  workbench.includes("function renderContextTokens(")
    && workbench.includes("concordanceContextTokenSegments(tokens, activeFilter, caseSensitive)")
    && workbench.includes("document.createTextNode(\" \")")
    && workbench.includes('const token = document.createElement("span");')
    && workbench.includes('token.className = "concordance-collocate";')
    && workbench.includes("token.textContent = segment.text;")
    && workbench.includes("function appendConcordanceKwicCells(")
    && workbench.includes("const activeContextFilter = concordanceResult?.contextFilter ?? null;")
    && workbench.includes("const contextFilterCaseSensitive = concordanceResult?.caseSensitive ?? false;")
    && workbench.includes("const activeFilter = concordanceResult?.contextFilter ?? null;")
    && workbench.includes("const caseSensitive = concordanceResult?.caseSensitive ?? false;")
    && workbench.includes("left.appendChild(renderContextTokens(concordanceExpansionResult.leftContext, activeFilter, caseSensitive));")
    && workbench.includes("right.appendChild(renderContextTokens(concordanceExpansionResult.rightContext, activeFilter, caseSensitive));")
    && workbench.includes("function createCollocateEvidenceSnippet(row: ConcordanceRow): HTMLSpanElement")
    && workbench.includes("function createCollocateEvidenceLine(row: ConcordanceRow): HTMLDivElement")
    && workbench.includes("function createCollocateEvidenceList(result: ConcordanceResult): HTMLDivElement")
    && !workbench.includes("function createCollocateEvidenceKwicRow")
    && !workbench.includes("function createCollocateEvidenceTable")
    && !workbench.includes("concordance-collocate-evidence-table")
    && !workbench.includes("innerHTML"),
  "Concordance context rendering should use text nodes and shared span-based collocate highlights in KWIC, expanded context, and inline drawer evidence",
);
assert(
  collocateEvidenceRendererBody.includes("renderContextTokens(row.leftContext, activeFilter, caseSensitive)")
    && collocateEvidenceRendererBody.includes("renderContextTokens(row.rightContext, activeFilter, caseSensitive)")
    && collocateEvidenceRendererBody.includes('const node = document.createElement("mark");')
    && collocateEvidenceRendererBody.includes('node.className = "concordance-node";')
    && collocateEvidenceRendererBody.includes("node.textContent = row.node;")
    && collocateEvidenceRendererBody.includes('for (const label of ["File", "Context"])')
    && !collocateEvidenceRendererBody.includes("appendConcordanceKwicCells(")
    && !collocateEvidenceRendererBody.includes("CONCORDANCE_COLUMNS")
    && !collocateEvidenceRendererBody.includes('document.createElement("table")')
    && !collocateEvidenceRendererBody.includes("Left context")
    && !collocateEvidenceRendererBody.includes("Right context"),
  "Collocate evidence drawer should render File plus inline Context snippets instead of a nested KWIC table",
);
assert(
  collocateEvidenceSnippetBody.includes("if (row.leftContext.length > 0)")
    && collocateEvidenceSnippetBody.includes("if (row.rightContext.length > 0)")
    && collocateEvidenceSnippetSpaceCount === 2
    && collocateEvidenceSnippetBody.indexOf("renderContextTokens(row.leftContext") < collocateEvidenceSnippetBody.indexOf("node.textContent = row.node;")
    && collocateEvidenceSnippetBody.indexOf("node.textContent = row.node;") < collocateEvidenceSnippetBody.indexOf("renderContextTokens(row.rightContext"),
  "Collocate evidence snippets should use ordinary conditional text spaces in left-context, node, right-context order",
);
assert(
  styles.includes(".concordance-collocate {\n  background: var(--cw-collocate-bg);\n  color: var(--cw-collocate-text);")
    && !styles.includes("mark.concordance-collocate"),
  "Concordance collocate tokens should use the quiet collocate colour role, not mark styling",
);
assert(
  workbench.includes('item.className = "concordance-collocation-row"')
    && workbench.includes('item.classList.toggle("is-active", isActiveCollocateEvidenceRow(row));')
    && workbench.includes('item.setAttribute("role", "button")')
    && workbench.includes('item.setAttribute("aria-label", actionLabel)')
    && workbench.includes('event.key !== "Enter" && event.key !== " "')
    && workbench.includes('item.className = "concordance-collocate-evidence-row"')
    && workbench.includes("panel.appendChild(createCollocateEvidenceList(collocateEvidenceResult));")
    && workbench.includes("dom.concordanceCollocationsTableBody.appendChild(createCollocateEvidenceDrawerRow(row));"),
  "Collocate rows should be clickable, keyboard accessible, mark the active evidence row, and render the drawer inline",
);
assert(
  collocateEvidenceRendererBody.includes('item.className = "concordance-collocate-evidence-line"')
    && collocateEvidenceRendererBody.includes('item.classList.toggle("is-expanded", expanded);')
    && collocateEvidenceRendererBody.includes("item.tabIndex = 0;")
    && collocateEvidenceRendererBody.includes('item.setAttribute("aria-expanded", expanded ? "true" : "false");')
    && collocateEvidenceRendererBody.includes('item.setAttribute("aria-controls", collocateEvidenceExpansionDomId(rowKey));')
    && collocateEvidenceRendererBody.includes("void toggleCollocateEvidenceExpansion(row);")
    && collocateEvidenceRendererBody.includes('if (event.key !== "Enter" && event.key !== " ")')
    && collocateEvidenceRendererBody.includes('item.className = "concordance-collocate-evidence-expansion-row";')
    && collocateEvidenceRendererBody.includes('item.setAttribute("role", "row");')
    && collocateEvidenceRendererBody.includes('panel.className = "concordance-collocate-evidence-expansion";')
    && collocateEvidenceRendererBody.includes("collocateEvidenceExpansionResult.leftContext")
    && collocateEvidenceRendererBody.includes("collocateEvidenceExpansionResult.rightContext")
    && collocateEvidenceRendererBody.includes("renderContextTokens(")
    && collocateEvidenceRendererBody.includes("collocateEvidenceExpansionResult.nodeTokens.join(\" \")")
    && collocateEvidenceRendererBody.includes("list.appendChild(createCollocateEvidenceExpansionRow(evidenceRow));")
    && !collocateEvidenceRendererBody.includes('document.createElement("tr")')
    && !collocateEvidenceRendererBody.includes("colSpan")
    && !collocateEvidenceRendererBody.includes("createConcordanceSourcePreviewRow"),
  "Co-occurrence lines should expand in place with drawer-local div rows and keyboard activation",
);
assert(
  workbench.includes('title.textContent = `Co-occurrence lines for "${row.surface}" with "${query}"`;')
    && workbench.includes('const actionLabel = `Show co-occurrence lines for "${row.surface}" with "${query}"`;')
    && !workbench.includes('title.textContent = `Evidence for')
    && !workbench.includes('const actionLabel = `Show evidence')
    && !workbench.includes('"Loading evidence..."')
    && !workbench.includes("No evidence lines found"),
  "Collocate drawer user-visible terminology should use co-occurrence lines rather than Evidence copy",
);
assert(
  collocateEvidenceDrawerBody.includes("collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0")
    && collocateEvidenceDrawerBody.includes("const hitCount = collocateEvidenceResult.totalHits;")
    && collocateEvidenceDrawerBody.includes("const spanText = collocateEvidenceSpanText(span, direction);")
    && collocateEvidenceDrawerBody.includes("meta.textContent = `${lineText} · ${spanText}`;")
    && collocateEvidenceDrawerBody.includes("meta.textContent = spanText;")
    && !collocateEvidenceDrawerBody.includes("collocateEvidenceResult?.totalHits ?? 0"),
  "Collocate evidence drawer meta should show line counts only when a non-empty result exists and otherwise show span notation",
);
assert(
  collocateEvidenceDrawerBody.indexOf("if (collocateEvidenceResult !== null && collocateEvidenceResult.totalHits > 0)") >= 0
    && collocateEvidenceDrawerBody.indexOf("panel.appendChild(createCollocateEvidenceList(collocateEvidenceResult));")
      < collocateEvidenceDrawerBody.indexOf("appendCollocateEvidenceMessage(panel, \"Loading co-occurrence lines...\");")
    && collocateEvidenceDrawerBody.includes("appendCollocateEvidenceMessage(panel, `Could not load more co-occurrence lines: ${collocateEvidenceFailure}`, true);"),
  "Collocate evidence drawer should keep existing rows visible while loading or reporting a failed show-more request",
);
assert(
  collocateEvidenceShowMoreBody.includes("function appendCollocateEvidenceShowMore")
    && collocateEvidenceShowMoreBody.includes('controls.className = "concordance-collocate-evidence-more"')
    && collocateEvidenceShowMoreBody.includes("const remaining = Math.max(0, result.totalHits - shown);")
    && collocateEvidenceShowMoreBody.includes("const nextCount = Math.min(CONCORDANCE_COLLOCATE_EVIDENCE_LIMIT, remaining);")
    && collocateEvidenceShowMoreBody.includes("remaining <= 0")
    && collocateEvidenceShowMoreBody.includes("Showing ${formatNumber(shown)} of ${formatNumber(result.totalHits)}")
    && collocateEvidenceShowMoreBody.includes('? "Loading..."')
    && collocateEvidenceShowMoreBody.includes("`Show remaining ${formatNumber(remaining)}`")
    && collocateEvidenceShowMoreBody.includes("`Show ${formatNumber(nextCount)} more`")
    && collocateEvidenceShowMoreBody.includes("collocateEvidenceLimit += nextCount")
    && collocateEvidenceShowMoreBody.includes("void refreshCollocateEvidence({ preserveResult: true });")
    && !workbench.includes(legacyCollocateEvidenceMoreCopy)
    && !collocateEvidenceShowMoreBody.includes('previous.textContent = "Previous"')
    && !collocateEvidenceShowMoreBody.includes('next.textContent = "Next"'),
  "Collocate evidence drawer should use cumulative count-based show-more controls instead of Previous/Next paging",
);
assert(
  styles.includes(".concordance-collocation-row.is-active td {\n  background: var(--cw-collocate-bg);")
    && styles.includes(".concordance-collocation-row.is-active .concordance-collocate-text {\n  color: var(--cw-collocate-text);"),
  "Active collocate rows should use the quiet collocate colour role",
);
const collocateEvidenceCellCssMatch = styles.match(
  /\.concordance-collocate-evidence-row > \.concordance-collocate-evidence-cell \{([\s\S]*?)\n\}/,
);
assert(collocateEvidenceCellCssMatch, "Collocate evidence drawer cell CSS should exist");
const collocateEvidenceCellCss = collocateEvidenceCellCssMatch[1];
const collocateEvidenceListGridMatch = styles.match(
  /\.concordance-collocate-evidence-list-head,\n\.concordance-collocate-evidence-line \{([\s\S]*?)\n\}/,
);
assert(collocateEvidenceListGridMatch, "Collocate evidence list grid CSS should exist");
const collocateEvidenceListGridCss = collocateEvidenceListGridMatch[1];
assert(
  styles.includes(".concordance-collocations-table > thead > tr > th,\n.concordance-collocations-table > tbody > tr.concordance-collocation-row > td")
    && !styles.includes(".concordance-collocations-table th,\n.concordance-collocations-table td")
    && collocateEvidenceCellCss.includes("overflow: visible")
    && collocateEvidenceCellCss.includes("text-overflow: clip")
    && collocateEvidenceCellCss.includes("white-space: normal")
    && styles.includes(".concordance-collocate-evidence {\n  display: grid;")
    && styles.includes("border-left: 1px solid var(--cw-collocate-border);")
    && styles.includes(".concordance-collocate-evidence-list")
    && styles.includes(".concordance-collocate-evidence-line")
    && styles.includes(".concordance-collocate-evidence-snippet")
    && styles.includes(".concordance-collocate-evidence-expansion-row")
    && styles.includes(".concordance-collocate-evidence-expansion {\n  display: grid;")
    && styles.includes("border-left: 1px solid var(--border-color);")
    && styles.includes(".concordance-collocate-evidence-expansion-context")
    && collocateEvidenceListGridCss.includes("grid-template-columns: minmax(8rem, 11rem) minmax(0, 1fr);")
    && styles.includes(".concordance-collocate-evidence-more")
    && !styles.includes(".concordance-collocate-evidence-table")
    && !styles.includes(".concordance-collocate-evidence-table .concordance-node-cell")
    && !styles.includes(".concordance-collocate-evidence-left")
    && !styles.includes(".concordance-collocate-evidence-right")
    && !styles.includes(".concordance-collocate-evidence-modal"),
  "Collocate evidence should be styled as a quiet attached drawer with File plus inline Context snippets and div-based expansion",
);

const concordanceNodeCssMatch = styles.match(/\.concordance-node \{([\s\S]*?)\n\}/);
assert(concordanceNodeCssMatch, "Concordance node CSS should exist");
const concordanceNodeCss = concordanceNodeCssMatch[1];
assert(
  !concordanceNodeCss.includes("text-overflow: ellipsis"),
  "Concordance node should not use ordinary ellipsis clipping",
);
assert(
  !concordanceNodeCss.includes("overflow-x") && concordanceNodeCss.includes("white-space: nowrap"),
  "Very long Concordance nodes should expand into the table scroll region rather than adding a nested scrollbar",
);
assert(
  styles.includes(".concordance-table td:nth-child(3)"),
  "Concordance node column selector should remain present",
);
const concordanceContextColumnCssMatch = styles.match(
  /\.concordance-table th:nth-child\(2\),\n\.concordance-table td:nth-child\(2\),\n\.concordance-table th:nth-child\(4\),\n\.concordance-table td:nth-child\(4\) \{([\s\S]*?)\n\}/,
);
assert(concordanceContextColumnCssMatch, "Concordance context column CSS should exist");
const concordanceContextColumnCss = concordanceContextColumnCssMatch[1];
const concordanceNodeColumnCssMatch = styles.match(
  /\.concordance-table th:nth-child\(3\),\n\.concordance-table td:nth-child\(3\) \{([\s\S]*?)\n\}/,
);
assert(concordanceNodeColumnCssMatch, "Concordance node column CSS should exist");
const concordanceNodeColumnCss = concordanceNodeColumnCssMatch[1];
assert(
  concordanceNodeColumnCss.includes("width: 9rem;")
    && concordanceNodeColumnCss.includes("min-width: 7rem;")
    && concordanceContextColumnCss.includes("width: calc((100% - 150px - 9rem) / 2);")
    && !concordanceNodeColumnCss.includes("width: 180px;")
    && !concordanceNodeColumnCss.includes("min-width: 140px;")
    && !concordanceContextColumnCss.includes("330px"),
  "Concordance node column should be materially narrower while context columns share the remaining space",
);

const concordanceRightCssMatch = styles.match(/\.concordance-right \{\n  color: var\(--text-muted\);([\s\S]*?)\n\}/);
assert(concordanceRightCssMatch, "Concordance right-context CSS should exist");
const concordanceRightCss = concordanceRightCssMatch[1];
assert(
  concordanceRightCss.includes("white-space: nowrap")
    && concordanceRightCss.includes("text-overflow: ellipsis")
    && concordanceRightCss.includes("overflow: hidden"),
  "Concordance right context should clip inside its own cell instead of forcing table scroll",
);
const concordanceCollocationScoreColumnCssMatch = styles.match(
  /\.concordance-collocations-table > thead > tr > th:nth-child\(4\),\n\.concordance-collocations-table > tbody > tr\.concordance-collocation-row > td:nth-child\(4\) \{([\s\S]*?)\n\}/,
);
assert(concordanceCollocationScoreColumnCssMatch, "Concordance collocation measure-score column CSS should exist");
const concordanceCollocationScoreColumnCss = concordanceCollocationScoreColumnCssMatch[1];
assert(
  concordanceCollocationScoreColumnCss.includes("width: 24%;")
    && styles.includes(".concordance-collocations-table-wrap {\n  max-width: 980px;")
    && styles.includes(".concordance-collocations-table {\n  width: 100%;\n  min-width: 760px;"),
  "Concordance collocation measure-score column should remain proportioned inside the capped collocations table",
);
const concordanceAnalyseShellOverrides = styles.match(/\.analyse-workbench\.is-concordance-active \.analyse-content \{([\s\S]*?)\n\}/g) ?? [];
assert(
  styles.includes(".analyse-content {\n  width: min(1180px, calc(100% - 48px));")
    && concordanceAnalyseShellOverrides.every(
      (block) =>
        !block.includes("max-width: none")
        && !block.includes("width: calc(100% - 48px)")
        && !block.includes("width: calc(100% - 28px)"),
    )
    && !styles.includes("width: calc(100% - 48px)")
    && !styles.includes("width: calc(100% - 28px)"),
  "Concordance should share the normal Analyse content shell instead of using a Concordance-only full-width override",
);
const concordanceTableWrapCssMatch = styles.match(/\.concordance-table-wrap \{([\s\S]*?)\n\}/);
const concordanceCollocationsTableWrapCssMatch = styles.match(/\.concordance-collocations-table-wrap \{([\s\S]*?)\n\}/);
const concordanceDistributionTableWrapCssMatch = styles.match(
  /\.concordance-distribution-table-wrap \{\n  max-width: 1000px;([\s\S]*?)\n\}/,
);
const frequencyTableWrapCssMatch = styles.match(/\.frequency-table-wrap \{([\s\S]*?)\n\}/);
assert(concordanceTableWrapCssMatch, "Concordance table wrap CSS should exist");
assert(concordanceCollocationsTableWrapCssMatch, "Concordance collocations table wrap CSS should exist");
assert(concordanceDistributionTableWrapCssMatch, "Concordance distribution table wrap CSS should exist");
assert(frequencyTableWrapCssMatch, "Frequency table wrap CSS should exist");
const concordanceTableWrapCss = concordanceTableWrapCssMatch[1];
const concordanceCollocationsTableWrapCss = concordanceCollocationsTableWrapCssMatch[1];
const concordanceDistributionTableWrapCss = concordanceDistributionTableWrapCssMatch[0];
const frequencyTableWrapCss = frequencyTableWrapCssMatch[1];
assert(
  concordanceTableWrapCss.includes("max-height: min(62vh, 720px);")
    && concordanceTableWrapCss.includes("overflow-x: clip;")
    && concordanceTableWrapCss.includes("overflow-y: auto;")
    && styles.includes("@media (max-width: 760px)")
    && styles.includes(".concordance-table-wrap {\n    overflow-x: auto;")
    && styles.includes(".concordance-table {\n    min-width: 760px;"),
  "Concordance lines should keep vertical scrolling inside the table wrap and reserve horizontal overflow for narrow viewports",
);
assert(
  concordanceCollocationsTableWrapCss.includes("max-width: 980px;")
    && concordanceCollocationsTableWrapCss.includes("max-height: min(58vh, 680px);")
    && concordanceCollocationsTableWrapCss.includes("overflow-x: auto;")
    && concordanceCollocationsTableWrapCss.includes("overflow-y: auto;"),
  "Concordance collocations table wrap should cap width and height while retaining horizontal overflow",
);
assert(
  concordanceDistributionTableWrapCss.includes("max-width: 1000px;")
    && concordanceDistributionTableWrapCss.includes("max-height: min(58vh, 680px);")
    && concordanceDistributionTableWrapCss.includes("overflow-x: auto;")
    && concordanceDistributionTableWrapCss.includes("overflow-y: auto;"),
  "Concordance distribution table wrap should cap width and height while retaining horizontal overflow",
);
assert(
  frequencyTableWrapCss.includes("max-height: min(58vh, 680px);")
    && frequencyTableWrapCss.includes("overflow: auto;")
    && frequencyTableWrapCss.includes("border: 1px solid var(--border-color);")
    && frequencyTableWrapCss.includes("border-radius: 4px;")
    && frequencyTableWrapCss.includes("background: var(--bg-color);"),
  "Frequency List table wrap should keep the existing internal scroll discipline",
);
const collocateTextCssMatch = styles.match(/\.concordance-collocate-text \{([\s\S]*?)\n\}/);
assert(collocateTextCssMatch, "Collocate text CSS should exist");
const collocateTextCss = collocateTextCssMatch[1];
assert(
  collocateTextCss.includes("font-weight: 500")
    && !styles.includes(".concordance-collocate-button")
    && !styles.includes(".concordance-collocate-action")
    && !collocateTextCss.includes("text-decoration"),
  "Collocate identifiers should render as normal row text, not as glued link-style actions",
);
const infoTokenCount = styles.match(/^  --cw-info-text:/gm)?.length ?? 0;
assert(infoTokenCount > 0, "Theme info colour roles should exist");
assert(
  (styles.match(/^  --cw-collocate-bg:/gm)?.length ?? 0) === infoTokenCount
    && (styles.match(/^  --cw-collocate-border:/gm)?.length ?? 0) === infoTokenCount
    && (styles.match(/^  --cw-collocate-text:/gm)?.length ?? 0) === infoTokenCount,
  "Each theme block with info colours should define the collocate colour role",
);
const currentMeasureMatch = workbench.match(
  /function currentConcordanceCollocationMeasure\(\): ConcordanceCollocationMeasure \{([\s\S]*?)\n\}/,
);
assert(currentMeasureMatch, "Current collocation measure state helper should exist");
assert(
  currentMeasureMatch[1].includes("return concordanceCollocationMeasure;")
    && !currentMeasureMatch[1].includes("dom.concordanceCollocationMeasureSelect.value"),
  "Collocation measure state should come from the module-level active measure",
);
const measureChangeMatch = workbench.match(
  /function handleConcordanceCollocationMeasureChange\(\): void \{([\s\S]*?)\n\}/,
);
assert(measureChangeMatch, "Collocation measure change handler should exist");
assert(
  measureChangeMatch[1].includes("concordanceCollocationMeasure = dom.concordanceCollocationMeasureSelect.value as ConcordanceCollocationMeasure;")
    && measureChangeMatch[1].includes("refreshConcordanceCollocationsForPresentationChange();"),
  "Changing the measure select should update active measure state and rerank presentation",
);
const measureFieldCssMatch = styles.match(
  /\.concordance-collocation-measure-field \{([\s\S]*?)\n\}/,
);
const measureSelectCssMatch = styles.match(
  /#concordance-collocation-measure \{([\s\S]*?)\n\}/,
);
assert(measureFieldCssMatch, "Collocation measure field CSS should exist");
assert(measureSelectCssMatch, "Collocation measure select CSS should exist");
assert(
  html.includes('class="concordance-collocation-measure-field" for="concordance-collocation-measure"')
    && styles.includes("#concordance-collocation-measure {\n  width: 122px;")
    && measureFieldCssMatch[1].includes("opacity: 1;")
    && !measureFieldCssMatch[1].includes("opacity: 0.86;")
    && measureSelectCssMatch[1].includes("border-color: var(--border-color);")
    && measureSelectCssMatch[1].includes("background: var(--bg-color);")
    && measureSelectCssMatch[1].includes("color: var(--text-color);")
    && !measureSelectCssMatch[1].includes("background: var(--cw-surface-subtle);")
    && !measureSelectCssMatch[1].includes("color: var(--text-muted);"),
  "The retained collocation measure select should stay compact without disabled-looking muted styling",
);
assert(
  styles.includes(".concordance-collocation-score-track")
    && styles.includes(".concordance-collocation-score-fill")
    && styles.includes("--concordance-score-bar")
    && styles.includes("background: var(--concordance-score-bar);")
    && styles.includes(".concordance-collocation-row:hover td")
    && styles.includes(".concordance-document-spread-header")
    && !styles.includes(".concordance-document-spread-toggle"),
  "Collocate Profile should render muted association bars, and Distribution should render directly without a collapsible toggle",
);
