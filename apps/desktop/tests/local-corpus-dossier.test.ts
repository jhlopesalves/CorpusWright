import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const root = process.cwd();
const capture = readFileSync(
  join(root, "apps/desktop/scripts/capture-local-corpus-dossier.mjs"),
  "utf8",
);
const fixture = readFileSync(
  join(root, "apps/desktop/tests/visual/local-corpus-dossier-fixture.ts"),
  "utf8",
);

test("local dossier captures every required witness in light and dark", () => {
  const witnesses = [...capture.matchAll(/witness\("([^"]+)"/g)].map((match) => match[1]);
  assert.equal(witnesses.length, 12);
  assert.equal(new Set(witnesses).size, 12);
  for (const required of [
    "document-length-histogram",
    "document-length-ecdf",
    "document-length-qq",
    "rank-frequency",
    "rank-residuals",
    "lexical-spectrum",
    "cumulative-coverage",
    "vocabulary-growth",
    "heaps-fit",
    "mattr-distribution",
    "dispersion-profile",
    "dispersion-plane",
  ]) {
    assert(witnesses.includes(required), required);
  }
  assert.match(capture, /for \(const theme of \["light", "dark"\]\)/);
  assert.match(capture, /theme === "light" \? "corpus-light" : "corpus-dark"/);
});

test("local dossier uses real plotting and standalone export paths", () => {
  for (const symbol of [
    "buildDistributionDiagnosticGeometry",
    "buildRankFrequencyGeometry",
    "buildVocabularyGrowthGeometry",
    "renderDistributionDiagnosticSvg",
    "renderRankFrequencySvg",
    "renderVocabularyGrowthSvg",
    "createOverviewFigure",
    "overviewFigureSvgString",
  ]) {
    assert(fixture.includes(symbol), symbol);
  }
  assert.match(fixture, /hasUnresolvedCssVariables\(source\)/);
  assert.match(fixture, /hasNonPortableSvgPaint\(source\)/);
});

test("captured standalone SVG rejects unresolved and non-portable paint", () => {
  assert.match(capture, /var\\s\*\\\(/);
  assert.match(capture, /color-mix\\s\*\\\(/);
  assert.match(capture, /color\\\(\\s\*srgb\\b/);
  assert.match(capture, /forbiddenPaintCount: 0/);
});
