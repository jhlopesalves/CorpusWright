import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const capture = readFileSync(new URL("../scripts/capture-overview-curation.mjs", import.meta.url), "utf8");
const fixture = readFileSync(new URL("./visual/overview-suite-fixture.ts", import.meta.url), "utf8");
const packageJson = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")) as {
  scripts?: Record<string, string>;
};

const screenshotBlock = capture.match(/const recoveryScreenshotCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const exportBlock = capture.match(/const recoveryExportCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const auditBlock = capture.match(/const recoveryAuditCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const cleanupScreenshotBlock = capture.match(/const cleanupScreenshotCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const cleanupExportBlock = capture.match(/const cleanupExportCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const legibilityScreenshotBlock = capture.match(/const legibilityScreenshotCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
const legibilityExportBlock = capture.match(/const legibilityExportCases = \[([\s\S]*?)\n\];/)?.[1] ?? "";
assert.equal(screenshotBlock.match(/\brecoveryShot\(/g)?.length, 12);
assert.equal(exportBlock.match(/\brecoveryExportCase\(/g)?.length, 3);
assert.equal(auditBlock.match(/\brecoveryAuditCase\(/g)?.length, 4);
assert.equal(cleanupScreenshotBlock.match(/\bcleanupShot\(/g)?.length, 8);
assert.equal(cleanupExportBlock.match(/\bcleanupExportCase\(/g)?.length, 2);
assert.equal(legibilityScreenshotBlock.match(/\blegibilityShot\(/g)?.length, 7);
assert.equal(legibilityExportBlock.match(/\blegibilityExportCase\(/g)?.length, 2);

for (const filename of [
  "document-histogram.png",
  "document-ecdf.png",
  "mattr-histogram.png",
  "lexical-spectrum.png",
  "cumulative-coverage.png",
  "rank-curve.png",
  "rank-residuals.png",
  "vocabulary-growth.png",
  "dispersion-profile.png",
  "dispersion-plane.png",
  "dispersion-profile-dark.png",
  "dispersion-plane-dark.png",
]) {
  assert.match(screenshotBlock, new RegExp(`"${filename.replace(/\.png$/, "")}"`));
}
for (const filename of ["rank-curve.svg", "cumulative-coverage.svg", "dispersion-plane.svg"]) {
  assert.match(exportBlock, new RegExp(`"${filename.replace(/\.svg$/, "")}"`));
}
for (const switchName of [
  "document-histogram-to-ecdf",
  "document-histogram-to-qq",
  "mattr-histogram-to-ecdf",
  "mattr-histogram-to-qq",
]) {
  assert.match(auditBlock, new RegExp(`"${switchName}"`));
}
for (const filename of [
  "document-histogram-controls",
  "document-ecdf-final",
  "document-qq-final",
  "mattr-ecdf-final",
  "mattr-qq-final",
  "dispersion-profile",
  "dispersion-plane",
  "overview-without-roster",
]) {
  assert.match(cleanupScreenshotBlock, new RegExp(`"${filename}"`));
}
for (const filename of ["document-ecdf-final", "dispersion-plane"]) {
  assert.match(cleanupExportBlock, new RegExp(`"${filename}"`));
}
for (const filename of [
  "document-ecdf-final",
  "mattr-ecdf-final",
  "document-qq-legend",
  "vocabulary-growth-legend",
  "document-concentration-disclosure",
  "dispersion-profile-tooltip",
  "dispersion-plane",
]) {
  assert.match(legibilityScreenshotBlock, new RegExp(`"${filename}"`));
}
assert.match(legibilityExportBlock, /"document-ecdf"/);
assert.match(legibilityExportBlock, /"dispersion-plane"/);

assert.equal(
  packageJson.scripts?.["check:overview-rendering-recovery"],
  "node scripts/capture-overview-curation.mjs --rendering-recovery",
);
assert.equal(
  packageJson.scripts?.["check:overview-distribution-dispersion-cleanup"],
  "node scripts/capture-overview-curation.mjs --distribution-dispersion-cleanup",
);
assert.equal(
  packageJson.scripts?.["check:overview-legibility-and-dispersion-plane"],
  "node scripts/capture-overview-curation.mjs --overview-legibility-and-dispersion-plane",
);
assert.match(capture, /\/tmp\/corpuswright-overview-rendering-recovery/);
assert.match(capture, /\/tmp\/corpuswright-overview-distribution-dispersion-cleanup/);
assert.match(capture, /\/tmp\/corpuswright-overview-legibility-and-dispersion-plane/);
assert.match(capture, /viewport: "1440x900"/);
assert.match(capture, /document\.getAnimations\(\)/);
assert.match(capture, /animation\.finish\(\)/);
assert.match(capture, /ecdfAndQqMinimumDelayMs: 1_000/);
assert.match(capture, /postRenderDelayMs = options\.view === "ecdf" \|\| options\.view === "qq" \? 1_000 : 0/);

for (const browserAssertion of [
  "zero-area painted geometry",
  "rank observed and fitted paint are identical",
  "growth observed legend paint does not match series",
  "coverage area gradient has unresolved stops",
  "dispersion profile ordered endpoints",
  "density legend does not expose the six-step scale",
  "density legend high step does not match the cell scale",
  "density legend colour strength is not monotonic",
  "adjacent dispersion density cells contain an intentional gap",
  "witness-category legend remains",
  "Fixed references must not create qualitative vocabulary categories",
  "dispersion threshold lines are too visually prominent",
  "dispersion density grid keyboard contract changed",
  "switched ${expectedLabel} control remains visible",
  "residual point outside fitted population",
  "lexical authored annotation outside SVG bounds",
  "dispersion witness label overlaps density legend",
  "automatic dispersion label overlaps threshold text",
  "dispersion witness label overlaps a marginal histogram",
  "automatic dispersion witness labels overlap each other",
  "external profile band is not visibly named",
  "ECDF final cumulative endpoint is not visible",
  "ECDF target uses ordinary point renderer",
  "legend leaves inner plot rectangle",
  "tooltip exceeds 300px maximum width",
  "profile tooltip example budget or remainder is incorrect",
  "grouped method disclosure retains the bright metric panel treatment",
  "Dispersion roster heading remains in Overview",
  "duplicated Overview structure remains",
]) {
  assert(fixture.includes(browserAssertion), `missing browser assertion: ${browserAssertion}`);
}
assert.match(fixture, /switchButton\.click\(\)/);
assert.match(fixture, /field\.hidden !== true/);
assert.match(fixture, /getComputedStyle\(field\)\.display !== "none"/);
assert.match(fixture, /element\.getBBox\(\)/);
assert.match(fixture, /document\.body\.dataset\.fixtureMetrics = JSON\.stringify/);
assert.match(capture, /metrics: \$\{fixtureMetrics\}/);
