import { readFileSync } from "node:fs";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const workbench = readFileSync(new URL("../src/workbench.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

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
assert(
  html.includes('id="concordance-view-tabs"')
    && html.includes('data-concordance-view="lines"')
    && html.includes('data-concordance-view="collocates"')
    && html.includes('data-concordance-view="distribution"')
    && html.includes(">Evidence lines</button>")
    && html.includes(">Collocate profile</button>")
    && html.includes(">Distribution</button>"),
  "Concordance should expose Evidence lines, Collocate profile, and Distribution as query view tabs",
);
const concordanceTabsStart = html.indexOf('id="concordance-view-tabs"', concordanceStatusStart);
const concordanceViewsStart = html.indexOf('<div class="concordance-views">', concordanceTabsStart);
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
  "Concordance view panels should render as Evidence, Collocate Profile, and Distribution siblings",
);
const concordanceLinesPanel = html.slice(concordanceLinesStart, concordanceCollocationsStart);
const concordanceCollocationsPanel = html.slice(concordanceCollocationsStart, concordanceDistributionStart);
assert(
  concordanceLinesPanel.includes('class="concordance-lines-controls"')
    && concordanceLinesPanel.includes('id="concordance-window-input"')
    && concordanceLinesPanel.includes('id="concordance-sort-select"')
    && concordanceLinesPanel.includes('id="concordance-page-size-input"')
    && !concordanceLinesPanel.includes('id="concordance-distribution-panel"'),
  "Evidence Lines controls should live inside the Lines view panel without nesting Distribution",
);
assert(
  concordanceCollocationsPanel.includes('class="concordance-collocation-controls"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-span"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-direction"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-measure"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-min-o11"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-min-freq"')
    && concordanceCollocationsPanel.includes('id="concordance-collocation-hide-stopwords"'),
  "Collocate Profile controls should remain inside the Collocate Profile panel",
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
  "Search should delegate to the active Concordance view workflow without forcing Evidence lines",
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
    && runConcordanceSearchForActiveViewBody.includes("await refreshConcordance();"),
  "Active-view Concordance search should capture the current view/query and refresh the query result first",
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
  "Concordance collocations should query Rust for true top-N sorting, split aggregation/presentation controls, auto-compute fresh keys once, and support click-through",
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
    && workbench.includes("concordanceSummaryMetricsText(result)")
    && workbench.includes("Distribution —")
    && !workbench.includes('const panel = document.createElement("details");\n  panel.className = "concordance-document-spread-panel";'),
  "Concordance Distribution should render directly rather than as a collapsed details widget",
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
assert(
  workbench.includes('setActiveConcordanceView("lines")')
    && workbench.includes('label.textContent = `Showing lines where "${result.query}" occurs near "${filter}".`;')
    && workbench.includes('activeConcordanceView !== "lines" || !concordanceContextFilter')
    && workbench.includes("clearConcordanceContextFilter"),
  "Collocate filtering should switch to Lines and expose a clearable active-filter chip",
);
assert(
  workbench.includes("function renderContextTokens(")
    && workbench.includes("concordanceContextTokenSegments(tokens, activeFilter, caseSensitive)")
    && workbench.includes("document.createTextNode(\" \")")
    && workbench.includes('const token = document.createElement("span");')
    && workbench.includes('token.className = "concordance-collocate";')
    && workbench.includes("token.textContent = segment.text;")
    && workbench.includes("const activeContextFilter = concordanceResult?.contextFilter ?? concordanceContextFilter;")
    && workbench.includes("const contextFilterCaseSensitive = concordanceResult?.caseSensitive ?? false;")
    && workbench.includes("const activeFilter = concordanceResult?.contextFilter ?? concordanceContextFilter;")
    && workbench.includes("const caseSensitive = concordanceResult?.caseSensitive ?? false;")
    && workbench.includes("left.appendChild(renderContextTokens(concordanceExpansionResult.leftContext, activeFilter, caseSensitive));")
    && workbench.includes("right.appendChild(renderContextTokens(concordanceExpansionResult.rightContext, activeFilter, caseSensitive));")
    && !workbench.includes("innerHTML"),
  "Concordance context rendering should use text nodes and span-based collocate highlights in KWIC and expanded context",
);
assert(
  styles.includes(".concordance-collocate {\n  background: var(--cw-collocate-bg);\n  color: var(--cw-collocate-text);")
    && !styles.includes("mark.concordance-collocate"),
  "Concordance collocate tokens should use the quiet collocate colour role, not mark styling",
);
assert(
  workbench.includes('item.className = "concordance-collocation-row"')
    && workbench.includes('item.classList.toggle("is-active", row.surface === concordanceContextFilter);')
    && workbench.includes('item.setAttribute("role", "button")')
    && workbench.includes('item.setAttribute("aria-label", actionLabel)')
    && workbench.includes('event.key !== "Enter" && event.key !== " "'),
  "Collocate rows should be clickable, keyboard accessible, and mark the active evidence row without an inline Show lines cue",
);
assert(
  styles.includes(".concordance-collocation-row.is-active td {\n  background: var(--cw-collocate-bg);")
    && styles.includes(".concordance-collocation-row.is-active .concordance-collocate-text {\n  color: var(--cw-collocate-text);"),
  "Active collocate rows should use the quiet collocate colour role",
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
  styles.includes(".concordance-table td:nth-child(3)")
    && styles.includes("min-width: 140px"),
  "Concordance node column should have a sensible minimum width",
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
assert(
  styles.includes(".concordance-collocations-table th:nth-child(4)")
    && styles.includes("width: 24%")
    && styles.includes("min-width: 760px"),
  "Concordance collocation measure-score column should have visible full-width table space",
);
assert(
  !styles.includes("max-height: min(62vh, 720px)")
    && styles.includes(".analyse-workbench.is-concordance-active .analyse-content")
    && styles.includes(".concordance-table-wrap {\n  overflow-x: clip;")
    && styles.includes("@media (max-width: 760px)")
    && styles.includes(".concordance-table {\n    min-width: 760px;")
    && styles.includes(".concordance-collocations-table-wrap {\n  overflow-x: auto;")
    && styles.includes(".concordance-distribution-table-wrap {\n  overflow-x: auto;"),
  "Concordance should use a scoped wide workspace and reserve KWIC horizontal overflow for narrow viewports",
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
  "Changing the demoted measure select should update active measure state and rerank presentation",
);
assert(
  html.includes('class="concordance-collocation-measure-field" for="concordance-collocation-measure"')
    && styles.includes("#concordance-collocation-measure {\n  width: 122px;")
    && styles.includes("color: var(--text-muted);")
    && styles.includes(".concordance-collocation-measure-field {\n  opacity: 0.86;"),
  "The retained collocation measure select should be visually demoted but still accessible",
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
