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
assert.equal(screenshotBlock.match(/\brecoveryShot\(/g)?.length, 12);
assert.equal(exportBlock.match(/\brecoveryExportCase\(/g)?.length, 3);
assert.equal(auditBlock.match(/\brecoveryAuditCase\(/g)?.length, 4);

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

assert.equal(
  packageJson.scripts?.["check:overview-rendering-recovery"],
  "node scripts/capture-overview-curation.mjs --rendering-recovery",
);
assert.match(capture, /\/tmp\/corpuswright-overview-rendering-recovery/);
assert.match(capture, /viewport: "1440x900"/);
assert.match(capture, /document\.getAnimations\(\)/);
assert.match(capture, /animation\.finish\(\)/);

for (const browserAssertion of [
  "zero-area painted geometry",
  "rank observed and fitted paint are identical",
  "growth observed legend paint does not match series",
  "coverage area gradient has unresolved stops",
  "dispersion profile ordered endpoints",
  "density legend does not expose the six-step scale",
  "density legend high step does not match the cell scale",
  "switched ${expectedLabel} control remains visible",
  "residual point outside fitted population",
  "lexical authored annotation outside SVG bounds",
  "dispersion witness label overlaps density legend",
  "automatic dispersion label overlaps threshold text",
]) {
  assert(fixture.includes(browserAssertion), `missing browser assertion: ${browserAssertion}`);
}
assert.match(fixture, /switchButton\.click\(\)/);
assert.match(fixture, /field\.hidden !== true/);
assert.match(fixture, /getComputedStyle\(field\)\.display !== "none"/);
assert.match(fixture, /element\.getBBox\(\)/);
