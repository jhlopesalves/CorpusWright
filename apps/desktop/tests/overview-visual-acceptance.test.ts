import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const capture = readFileSync(new URL("../scripts/capture-overview-curation.mjs", import.meta.url), "utf8");
const fixture = readFileSync(new URL("./visual/overview-suite-fixture.ts", import.meta.url), "utf8");
const html = readFileSync(new URL("./visual/overview-suite-fixture.html", import.meta.url), "utf8");
const interactionStyles = readFileSync(new URL("../src/styles/plots/interactions.css", import.meta.url), "utf8");
const packageJson = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")) as {
  scripts?: Record<string, string>;
};

const screenshotBlock = capture.match(/const screenshotCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const exportBlock = capture.match(/const exportCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
assert.equal(screenshotBlock.match(/\bshot\(/g)?.length, 32);
assert.equal(exportBlock.match(/\bexportCase\(/g)?.length, 13);

for (const figure of ["document", "mattr", "rank", "growth", "lexical", "coverage", "profile", "plane"]) {
  assert.match(exportBlock, new RegExp(`exportCase\\([^\\n]+, "${figure}"`), `missing default ${figure} export`);
  assert.match(fixture, new RegExp(`name === "${figure}"|name === "document" \\|\\| name === "mattr"`), `missing ${figure} fixture`);
}

for (const requiredCase of [
  "document-histogram-default",
  "document-custom-bin-options",
  "rank-residuals",
  "vocabulary-heaps-fit",
  "coverage-90",
  "lexical-spectrum-dark",
  "dispersion-plane-high-contrast",
  "pinned-plane-witness",
  "keyboard-focused-density-cell",
  "export-colour-dialog",
]) {
  assert.match(screenshotBlock, new RegExp(`"${requiredCase}"`));
}

assert.equal(packageJson.scripts?.["capture:overview-curation"], "node scripts/capture-overview-curation.mjs");
assert.match(html, /href="\/src\/styles\.css"/);
assert.match(fixture, /from "\.\.\/\.\.\/src\/plots\/figure"/);
assert.match(fixture, /from "\.\.\/\.\.\/src\/plots\/model"/);
assert.match(fixture, /from "\.\.\/\.\.\/src\/plots\/render"/);
assert.match(fixture, /overviewFigureSvgString\(built\.figure, palette\)/);
assert.match(fixture, /action outside figure bounds/);
assert.match(fixture, /clipped text/);
assert.match(fixture, /composition label overlap/);
assert.match(fixture, /legend overlaps pinned evidence/);
assert.match(fixture, /focused tooltip outside viewport/);
assert.match(fixture, /mass mark Tab stops/);
assert.doesNotMatch(interactionStyles, /\.cw-plot-tooltip-box\s*\{[^}]*filter:/s);
assert.match(capture, /\/tmp\/corpuswright-overview-curation/);
assert.match(capture, /Page\.captureScreenshot/);
assert.match(capture, /async function createCasePage/);
assert.match(capture, /Page\.bringToFront/);
assert.match(capture, /Target\.closeTarget/);
assert.match(capture, /result\?\.href === url && result\.status === "passed"/);
assert.match(capture, /async function settlePaint/);
assert.match(capture, /consoleErrors !== 0 \|\| report\.accessibilitySerious !== 0/);
assert.match(capture, /contains unresolved CSS variables/);
for (const requiredSvgContent of ["<title", "<desc", "cw-plot-figure-title", "cw-plot-figure-reading"]) {
  assert.match(capture, new RegExp(requiredSvgContent));
}
