/** Compose the public card from the real GUI capture; keep its source and renderer reproducible. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { browserFailures, createCapturePage, launchCaptureBrowser, waitFor } from "../apps/desktop/scripts/browser-capture.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const output = resolve(process.argv[2] ?? join(root, "docs/assets/social-preview.png"));
const screenshot = "docs/assets/walkthrough/05-dispersion-light.png";
// Keep the application navigation and Analyse tool rail with the complete Dispersion plane.
// Coordinates select original capture pixels without altering the application view.
const crop = { x: 0, y: 0, width: 1440, height: 1000 };
const previewWidth = 820;
const scale = previewWidth / crop.width;
const temporary = mkdtempSync(join(tmpdir(), "corpuswright-social-preview-"));
const url = path => pathToFileURL(join(root, path)).href;
const sha256 = path => createHash("sha256").update(readFileSync(path)).digest("hex");
let browser;
try {
  const source = `<!doctype html><html lang="en-GB"><meta charset="utf-8"><title>CorpusWright social preview</title>
<style>
@font-face { font-family: Reading; src: url("${url("docs/assets/fonts/wright/WrightReading.woff2")}"); font-weight: 200 900; }
@font-face { font-family: Manrope; src: url("${url("docs/assets/fonts/wright/Manrope.woff2")}"); font-weight: 200 800; }
* { box-sizing: border-box; }
body { margin: 0; width: 1200px; height: 630px; background: #222b36; color: #f1f5fa; font: 20px/1.45 Reading, sans-serif; }
main { display: grid; grid-template-columns: 280px 820px; gap: 24px; height: 100%; padding: 30px 38px; align-items: center; }
.copy { align-self: stretch; display: flex; flex-direction: column; }
h1 { margin: 0 0 36px; font: 600 34px/1.2 Manrope, sans-serif; letter-spacing: -0.02em; }
.descriptor { font-size: 25px; line-height: 1.3; margin: 0 0 30px; }
.status { margin: auto 0 0; color: #bdc9d7; font-size: 17px; }
.gui-crop { position: relative; width: 820px; height: ${crop.height * scale}px; overflow: hidden; border: 1px solid #3b495b; border-radius: 10px; }
.gui-crop img { position: absolute; width: ${1440 * scale}px; max-width: none; left: ${-crop.x * scale}px; top: ${-crop.y * scale}px; display: block; }
</style>
<main><div class="copy"><h1>CorpusWright</h1><p class="descriptor">Desktop software for corpus preparation and descriptive corpus analysis.</p><p class="status">Open-source software · public alpha</p></div><div class="gui-crop"><img src="${url(screenshot)}" alt="CorpusWright desktop application displaying the Dispersion plane for the walkthrough corpus."></div></main></html>`;
  const html = join(temporary, "preview.html");
  writeFileSync(html, source);
  browser = await launchCaptureBrowser();
  const page = await createCapturePage(browser, 1200, 630);
  try {
    await page.navigate(pathToFileURL(html).href);
    await waitFor(page, "document.images[0].complete && document.images[0].naturalWidth === 1440");
    await page.evaluate("document.fonts.ready");
    assert.deepEqual(browserFailures(browser, page.sessionId), []);
    assert.deepEqual(await page.evaluate("[document.documentElement.scrollWidth, document.documentElement.scrollHeight]"), [1200, 630]);
    const pixels = await page.screenshot();
    writeFileSync(output, pixels);
    writeFileSync(output.replace(/\.png$/, ".json"), JSON.stringify({
      schemaVersion: 1, width: 1200, height: 630, source: screenshot, sourceSha256: sha256(join(root, screenshot)),
      sourceCrop: crop,
      generator: "scripts/generate-social-preview.mjs", generatorSha256: sha256(fileURLToPath(import.meta.url)),
      sha256: createHash("sha256").update(pixels).digest("hex"),
      provenance: "Public card composed from an unmodified crop of the walkthrough screenshot and locally bundled Wright fonts. The full-view source retains the application navigation, Analyse tools and Dispersion plane.",
    }, null, 2) + "\n");
    process.stdout.write(`Generated ${output} (1200 × 630, ${pixels.length} bytes)\n`);
  } finally { await page.close(); }
} finally {
  await browser?.close();
  rmSync(temporary, { recursive: true, force: true });
}
