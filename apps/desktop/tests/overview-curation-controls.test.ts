import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");

function functionSource(name: string): string {
  const start = shell.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing ${name}`);
  const bodyStart = shell.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < shell.length; index += 1) {
    if (shell[index] === "{") depth += 1;
    if (shell[index] === "}") depth -= 1;
    if (depth === 0) return shell.slice(start, index + 1);
  }
  throw new Error(`unterminated ${name}`);
}

assert.match(figure, /document\.createElement\("figure"\)/);
assert.match(figure, /document\.createElement\("figcaption"\)/);
assert.doesNotMatch(figure, /document\.createElement\("section"\)[\s\S]{0,100}cw-overview-figure/);
assert.match(figure, /figure\.appendChild\(header\)[\s\S]*controlRow[\s\S]*figure\.append\(body, caption\)/);
assert.match(figure, /field\.appendChild\(createControlLabel\(label\)\)[\s\S]*group\.className = "cw-overview-segmented-control"/);
assert.doesNotMatch(figure, /group\.appendChild\(createControlLabel/);

assert.doesNotMatch(shell, /createFigurePaletteToolbar|Figure palette/);
for (const option of ["Current theme colours", "Colour-blind-safe colours", "Monochrome / print"]) {
  assert(figure.includes(option), `missing export colour option: ${option}`);
}

assert.match(shell, /xScale: isLength \? state\.xScale : "linear"/);
assert.doesNotMatch(shell, /"X scale"/);
assert.doesNotMatch(shell, /"Q–Q reference"/);
assert.doesNotMatch(shell, /"Y",\s*\[\{ value: "share"/);
assert.doesNotMatch(shell, /"Labels",\s*\[/);
assert.doesNotMatch(shell, /"Class bands",\s*\[/);
assert.doesNotMatch(shell, /"Marginals",\s*\[/);
assert.doesNotMatch(shell, /"Thresholds",\s*\[/);
assert.doesNotMatch(shell, /"Frequency",\s*\[\s*\{ value: "rate"/);
assert.doesNotMatch(shell, /"Fit",\s*\[/);

assert.match(shell, /createOverviewSegmentedControl\(\s*"Coverage target"/);
assert.match(shell, /createOverviewSegmentedControl\(\s*"View"/);
assert.doesNotMatch(shell, /createOverviewFigureOptions|Figure options/);
assert.match(shell, /manualControl\.hidden = state\.view !== "histogram" \|\| state\.histogramRule !== "manual"/);
assert.doesNotMatch(shell, /Dispersion roster|stopwords: "show", sort: "frequency"|Show 30 more/);

const populationPanel = functionSource("renderAnalysisPopulation");
for (const unit of ["Tokens", "Types", "Documents"]) {
  assert.match(populationPanel, new RegExp(`label: "${unit}"`));
  assert.doesNotMatch(populationPanel, new RegExp(`label: "${unit}"[^\n]+glossaryId`));
}
const lexicalPanel = functionSource("renderLexicalSpectrum");
assert.doesNotMatch(lexicalPanel, /label: "Types"/);
assert.doesNotMatch(lexicalPanel, /label: "MATTR"[\s\S]{0,160}glossaryId: "mattr"/);
assert.match(lexicalPanel, /label: "MTLD"[\s\S]{0,120}glossaryId: "mtld"/);
const concentrationPanel = functionSource("renderFrequencyConcentration");
assert.doesNotMatch(concentrationPanel, /Top (?:100|1,000|10,000) types cover/);
for (const retained of ["Normalised entropy / evenness", "Shannon entropy", "Effective vocabulary (perplexity)"]) {
  assert(concentrationPanel.includes(retained), `missing retained concentration metric: ${retained}`);
}
assert.doesNotMatch(concentrationPanel, /glossaryId:/);
assert.match(concentrationPanel, /createGroupedMethodDisclosure\("Frequency concentration", \[\s*"shannonEntropy",\s*"normalisedEntropy",\s*"effectiveVocabulary",\s*\]\)/);
const balancePanel = functionSource("renderDocumentBalance");
assert.doesNotMatch(balancePanel, /glossaryId:/);
assert.match(balancePanel, /createGroupedMethodDisclosure\("Document balance", \[\s*"giniCoefficient",\s*"coefficientOfVariation",\s*"effectiveDocuments",\s*\]\)/);
assert.match(shell, /function createGroupedMethodDisclosure/);
for (const name of ["renderRankFrequency", "renderVocabularyGrowth"]) {
  assert.doesNotMatch(functionSource(name), /glossaryId:/, `${name} repeats a figure-owned method disclosure`);
}
assert.match(shell, /explanation: createFigureExplanation\("zipfFit"\)/);
assert.match(shell, /explanation: createFigureExplanation\("heapsFit"\)/);
const dispersionPanel = functionSource("renderDispersionProfile");
assert.doesNotMatch(dispersionPanel, /Types in (?:1 document|2–9 documents|10–99 documents|100\+ documents|at least|all )|Median documents per type/);
assert.doesNotMatch(dispersionPanel, /glossaryId: "griesDp"/);
for (const retained of ["Frequent types (10+ occurrences)", "Gries DP", "Universe: all analysis types", "renderDispersionFigure", "renderDispersionPlaneFigure"]) {
  assert(dispersionPanel.includes(retained), `missing retained dispersion content: ${retained}`);
}
for (const advanced of ["Gini coefficient", "Coefficient of variation", "Effective documents", "MATTR", "MTLD", "Shannon entropy", "Zipf exponent", "Heaps K", "Gries DP"]) {
  assert(shell.includes(advanced), `missing advanced Overview metric: ${advanced}`);
}

assert.match(styles, /grid-template-columns: minmax\(min\(540px, 100%\), 1fr\) auto/);
assert.match(styles, /cw-overview-figure-control-row/);
assert.match(styles, /cw-overview-figure-controls[\s\S]*flex-wrap: wrap/);
assert.match(styles, /\.cw-overview-control-field\[hidden\][\s\S]*display: none/);
assert.doesNotMatch(styles, /cw-overview-figure-options/);
assert.doesNotMatch(styles, /cw-overview-palette-toolbar/);
