import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const shell = readFileSync(new URL("../src/workbench/shell.ts", import.meta.url), "utf8");
const figure = readFileSync(new URL("../src/plots/figure.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles/plots/figures.css", import.meta.url), "utf8");

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
assert.match(shell, /createOverviewFigureOptions\([\s\S]{0,160}binsControl/);
assert.match(shell, /manualControl\.hidden = state\.view !== "histogram" \|\| state\.histogramRule !== "manual"/);
assert.match(shell, /initialControls: \{ stopwords: "show", sort: "frequency", rows: 20 \}/);
assert.match(shell, /Show 30 more/);

assert.match(styles, /grid-template-columns: minmax\(min\(540px, 100%\), 1fr\) auto/);
assert.match(styles, /cw-overview-figure-control-row/);
assert.match(styles, /cw-overview-figure-controls[\s\S]*flex-wrap: wrap/);
assert.doesNotMatch(styles, /cw-overview-palette-toolbar/);
