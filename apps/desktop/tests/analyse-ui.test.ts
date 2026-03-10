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
  workbench.includes("function handleConcordanceSortChange")
    && workbench.includes("void refreshConcordance(true, true);"),
  "Concordance sort changes should reset KWIC and distribution pagination",
);
assert(
  workbench.includes("Searching...") && workbench.includes("CONCORDANCE_NO_HITS_MESSAGE"),
  "Concordance renderer should include loading and no-hit states",
);
assert(
  workbench.includes("addEventListener(\"click\", () => {\n    void toggleConcordanceExpansion(row);")
    && workbench.includes("event.key !== \"Enter\" && event.key !== \" \"")
    && workbench.includes("aria-expanded"),
  "Concordance rows should support click, keyboard toggle, and aria-expanded",
);
assert(
  workbench.includes("concordanceExpansionViewModel")
    && workbench.includes("concordance-expansion-message is-")
    && workbench.includes("concordance-expansion-node"),
  "Concordance renderer should include ready, loading, and error expansion states",
);
assert(
  workbench.includes("collapseConcordanceExpansionFromControlChange")
    && workbench.includes("dom.concordanceQueryInput.addEventListener(\"input\", collapseConcordanceExpansionFromControlChange)")
    && workbench.includes("dom.concordanceWindowInput.addEventListener(\"change\", collapseConcordanceExpansionFromControlChange)")
    && workbench.includes("function handleConcordanceCaseChange"),
  "Query, window, and case changes should collapse expanded Concordance rows",
);
assert(
  workbench.includes("function showPreviousConcordancePage")
    && workbench.includes("void refreshConcordance(false, false);")
    && workbench.includes("function handleConcordancePageSizeChange")
    && workbench.includes("collapseConcordanceExpansion();\n  void refreshFrequencyPage();"),
  "Page, page-size, and file-scope changes should collapse expanded Concordance rows",
);

const concordanceNodeCssMatch = styles.match(/\.concordance-node \{([\s\S]*?)\n\}/);
assert(concordanceNodeCssMatch, "Concordance node CSS should exist");
const concordanceNodeCss = concordanceNodeCssMatch[1];
assert(
  !concordanceNodeCss.includes("text-overflow: ellipsis"),
  "Concordance node should not use ordinary ellipsis clipping",
);
assert(
  concordanceNodeCss.includes("overflow-x: auto") && concordanceNodeCss.includes("white-space: nowrap"),
  "Very long Concordance nodes should be horizontally scrollable",
);
assert(
  styles.includes(".concordance-table td:nth-child(3)")
    && styles.includes("min-width: 180px"),
  "Concordance node column should have a sensible minimum width",
);
