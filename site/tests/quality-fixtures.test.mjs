import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";
import { assertSafeOutput } from "../scripts/clean-output.mjs";
import { normalisePublicText, textHash } from "../scripts/content-text.mjs";
import { productionOutput, repositoryRoot } from "../scripts/paths.mjs";
import { inspectCss, inspectHtml, inspectLinks, missingManifestFiles, releaseRenderingErrors } from "../scripts/quality-rules.mjs";
import { manifestDifferences } from "../scripts/tree-manifest.mjs";

const valid = '<!doctype html><html lang="en-GB"><head><title>Fixture</title><link rel="canonical" href="https://example.test/fixture.html"></head><body><h1>Fixture</h1></body></html>';

test("fixture detects a broken internal link", () => {
  const result = inspectLinks(new Map([["/index.html", `${valid}<a href="missing.html">Missing</a>`]]), new Set(["index.html"]));
  assert(result.errors.some((error) => error.includes("missing target")));
});

test("fixture detects a broken fragment", () => {
  const result = inspectLinks(new Map([["/index.html", `${valid}<a href="#missing">Missing</a>`]]), new Set(["index.html"]));
  assert(result.errors.some((error) => error.includes("missing fragment")));
});

test("fixture detects duplicate IDs", () => {
  assert(inspectHtml(valid.replace("</body>", '<p id="same"></p><p id="same"></p></body>')).some((error) => error.includes("duplicate ID")));
});

test("fixture detects a missing title", () => {
  assert(inspectHtml(valid.replace("<title>Fixture</title>", "")).includes("missing title"));
});

test("fixture detects a missing canonical URL", () => {
  assert(inspectHtml(valid.replace(/<link rel="canonical"[^>]+>/, "")).includes("missing canonical URL"));
});

test("fixture detects a skipped heading level", () => {
  assert(inspectHtml(valid.replace("</body>", "<h3>Skipped</h3></body>")).some((error) => error.includes("skipped heading")));
});

test("fixture detects an inline style attribute", () => {
  assert(inspectHtml(valid.replace("<h1", '<h1 style="color: red"')).includes("inline style attribute"));
});

test("fixture detects an inline style block", () => {
  assert(inspectHtml(valid.replace("</head>", "<style>h1 { color: red; }</style></head>")).includes("inline style block"));
});

test("fixture detects inline behaviour", () => {
  assert(inspectHtml(valid.replace("</body>", "<script>alert(1)</script></body>")).includes("inline behaviour script"));
});

test("fixture detects raw CSS colour outside theme ownership", () => {
  assert(inspectCss(".card { color: #123456; }").includes("raw CSS colour outside theme ownership"));
});

test("fixture detects an unsupported font weight", () => {
  assert(inspectCss(".card { font-weight: 700; }").includes("unsupported font weight"));
});

test("fixture detects a remote executable script", () => {
  assert(inspectHtml(valid.replace("</body>", '<script src="https://example.test/app.js"></script></body>')).some((error) => error.includes("remote executable")));
});

test("fixture detects stale generated output", () => {
  const differences = manifestDifferences([{ path: "index.html", bytes: 1, sha256: "a" }], [{ path: "index.html", bytes: 1, sha256: "b" }]);
  assert.deepEqual(differences.changed, ["index.html"]);
});

test("fixture detects changed public copy", () => {
  const expected = textHash(normalisePublicText("<p>Original</p>", []));
  const actual = textHash(normalisePublicText("<p>Changed</p>", []));
  assert.notEqual(actual, expected);
});

test("fixture detects release metadata drift", () => {
  assert(releaseRenderingErrors("Version 0.0.0 (2025)", { version: "1.2.3-alpha.4", dateReleased: "2026-01-01" }).length >= 1);
});

test("fixture detects a missing asset", () => {
  assert.deepEqual(missingManifestFiles(["assets/icon.png"], new Set()), ["assets/icon.png"]);
});

test("fixture detects an unsafe clean target", async () => {
  await assert.rejects(assertSafeOutput(path.join(repositoryRoot, "site")), /unsafe|must resolve/);
  assert.equal(await assertSafeOutput(productionOutput), productionOutput);
});

test("fixture detects nondeterministic output", () => {
  const differences = manifestDifferences([{ path: "site.js", bytes: 1, sha256: "first" }], [{ path: "site.js", bytes: 2, sha256: "second" }]);
  assert.deepEqual(differences.changed, ["site.js"]);
});
