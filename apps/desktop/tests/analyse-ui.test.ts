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
assert(
  html.includes('id="concordance-view-tabs"')
    && html.includes('data-concordance-view="lines"')
    && html.includes('data-concordance-view="collocates"')
    && !html.includes('data-concordance-view="distribution"'),
  "Concordance should expose Lines and Collocates as the work-mode tabs",
);
assert(
  html.includes('id="concordance-lines-panel" class="concordance-view-panel concordance-kwic-panel"')
    && html.includes('id="concordance-source-preview" class="concordance-source-preview hidden"')
    && html.includes('id="concordance-collocations-panel" class="concordance-view-panel concordance-collocations-panel hidden"')
    && html.includes('id="concordance-distribution-panel" class="concordance-document-spread"'),
  "Concordance should render source preview and document spread inside the Lines workspace",
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
  "CONCORDANCE_COLLOCATION_STALE_DETAIL",
  "Refresh collocates",
]) {
  assert(workbench.includes(label), `Collocates should render ${label}`);
}
assert(
  !workbench.includes("Compute collocates") && !workbench.includes("Show lines"),
  "Collocates should avoid resting compute buttons and glued Show lines row text",
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
  runConcordanceSearchMatch[1].includes("void refreshConcordance();")
    && !runConcordanceSearchMatch[1].includes("refreshConcordanceCollocations"),
  "Search should refresh KWIC lines only",
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
  workbench.includes('type ConcordanceViewId = "lines" | "collocates";')
    && workbench.includes('type ConcordanceCollocationState = "idle_not_computed" | "loading" | "ready" | "stale" | "error";')
    && workbench.includes("function syncConcordanceViewTabs")
    && workbench.includes("function maybeAutoComputeConcordanceCollocations")
    && workbench.includes('dom.concordanceViewTabs.addEventListener("click", handleConcordanceViewTabClick)')
    && !workbench.includes("function handleConcordanceViewTabClick(event: MouseEvent): void {\n  void refreshConcordance"),
  "Concordance tab switching should use the lazy collocates trigger rather than refreshing KWIC",
);
assert(
  workbench.includes('table.className = "concordance-distribution-table"')
    && workbench.includes('for (const label of ["File", "Hits", "Tokens", "Bar"])')
    && workbench.includes('panel.className = "concordance-document-spread-panel"')
    && workbench.includes("Document spread"),
  "Concordance document spread should render as a collapsible table",
);
assert(
  workbench.includes("addEventListener(\"click\", () => {\n    void toggleConcordanceExpansion(row);")
    && workbench.includes("event.key !== \"Enter\" && event.key !== \" \"")
    && workbench.includes("aria-expanded"),
  "Concordance rows should support click, keyboard toggle, and aria-expanded",
);
assert(
  workbench.includes("concordanceExpansionViewModel")
    && workbench.includes("function renderConcordanceSourcePreview")
    && workbench.includes("dom.concordanceSourcePreview.replaceChildren(createConcordanceSourcePreviewPanel(row))")
    && workbench.includes("concordance-expansion-message is-")
    && workbench.includes("concordance-expansion-node")
    && workbench.includes("handleConcordanceExpansionEscape")
    && !workbench.includes("createConcordanceExpansionRow")
    && !workbench.includes("concordanceTableBody.appendChild(createConcordanceExpansionRow"),
  "Concordance renderer should include ready, loading, error, and Escape-close preview states outside the table body",
);
assert(
  styles.includes(".concordance-source-preview")
    && !styles.includes("concordance-expansion-cell"),
  "Concordance source preview should have its own mount outside the KWIC table overflow context",
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
  workbench.includes('item.className = "concordance-collocation-row"')
    && workbench.includes('item.setAttribute("role", "button")')
    && workbench.includes('item.setAttribute("aria-label", actionLabel)')
    && workbench.includes('event.key !== "Enter" && event.key !== " "'),
  "Collocate rows should be clickable and keyboard accessible without an inline Show lines cue",
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
assert(
  styles.includes(".concordance-collocation-score-track")
    && styles.includes(".concordance-collocation-score-fill")
    && styles.includes("--concordance-score-bar")
    && styles.includes("background: var(--concordance-score-bar);")
    && styles.includes(".concordance-collocation-row:hover td")
    && styles.includes(".concordance-document-spread-toggle::before"),
  "Collocates should render muted association bars, row hover, and Document spread should expose a collapsible toggle",
);
