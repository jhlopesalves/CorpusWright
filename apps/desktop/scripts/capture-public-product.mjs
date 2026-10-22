/** Reproduce three full application views without shipping fixture code in the application. */
import assert from "node:assert/strict";
import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { generateProductCorpus } from "../tests/visual/public-product-corpus.mjs";
import { browserFailures, createCapturePage, launchCaptureBrowser, waitFor } from "./browser-capture.mjs";

const desktop = fileURLToPath(new URL("..", import.meta.url));
const repository = resolve(desktop, "../..");
const output = resolve(process.argv[2] ?? join(tmpdir(), "corpuswright-public-product"));
const temporary = mkdtempSync(join(tmpdir(), "corpuswright-public-product-data-"));
const corpus = join(temporary, "corpus");
const helper = join(temporary, "helper");
const run = promisify(execFile);
const viewport = { width: 1440, height: 1000, deviceScaleFactor: 1 };
const theme = "corpus-light";
mkdirSync(corpus);
mkdirSync(helper);
mkdirSync(output, { recursive: true });
generateProductCorpus(corpus);
writeFileSync(join(helper, "Cargo.toml"), `
[package]
name = "corpuswright-public-product-data"
version = "0.0.0"
edition = "2024"
[dependencies]
corpuswright-core = { path = ${JSON.stringify(join(repository, "crates/corpuswright-core"))} }
serde_json = "1"
[[bin]]
name = "corpuswright-public-product-data"
path = ${JSON.stringify(join(desktop, "tests/visual/public-product-data.rs"))}
`);
// Reuse the repository lock and build cache; the temporary package never changes workspace metadata.
writeFileSync(join(helper, "Cargo.lock"), readFileSync(join(repository, "Cargo.lock")));
const target = join(repository, "target");
let server;
let browser;
let concordanceAudit;
let collocationAudit;
try {
  execFileSync("cargo", ["build", "--offline", "--manifest-path", join(helper, "Cargo.toml"), "--target-dir", target], {
    cwd: repository, stdio: "inherit",
  });
  const binary = join(target, "debug/corpuswright-public-product-data");
  const queryCore = async (command, args) => {
    const result = await run(binary, [corpus, command, JSON.stringify(args)], { maxBuffer: 16 * 1024 * 1024 });
    const value = JSON.parse(result.stdout);
    if (command === "search_concordance_command") {
      concordanceAudit = Object.fromEntries(["query", "caseSensitive", "windowTokens", "totalHits", "documentsWithHits", "selectedDocumentsTotal", "selectedTokensTotal", "dp"].map(key => [key, value[key]]));
    }
    if (command === "query_concordance_collocations_command") {
      collocationAudit = Object.fromEntries(["query", "caseSensitive", "span", "direction", "minCooccurrence", "minCollocateFrequency", "hideStopwords", "sortMeasure", "sortKey", "sortDirection", "limit", "r1", "n", "fNode"].map(key => [key, value[key]]));
      const row = value.rows.find(row => row.surface === "tray");
      collocationAudit.witness = row ? { surface: row.surface, events: row.o11, corpusFrequency: row.collocateFrequency, mi: row.mi } : null;
    }
    return value;
  };
  const data = await queryCore("bootstrap", {});
  server = await createServer({
    root: desktop, server: { host: "127.0.0.1", port: 4179, strictPort: true },
    plugins: [{
      name: "public-product-capture-only",
      configureServer(vite) {
        vite.middlewares.use(async (request, response, next) => {
          const pathname = new URL(request.url, "http://capture.local").pathname;
          if (!["/public-product", "/public-product-data", "/public-product-ipc"].includes(pathname)) return next();
          try {
            if (pathname === "/public-product") {
              const source = readFileSync(join(desktop, "index.html"), "utf8")
                .replace('src="/src/main.ts"', 'src="/tests/visual/public-product-fixture.ts"');
              response.setHeader("Content-Type", "text/html");
              response.end(await vite.transformIndexHtml("/index.html", source));
            } else {
              response.setHeader("Content-Type", "application/json");
              if (pathname === "/public-product-data") response.end(JSON.stringify(data));
              else {
                let body = "";
                for await (const chunk of request) body += chunk;
                const { command, args } = JSON.parse(body);
                response.end(JSON.stringify(await queryCore(command, args)));
              }
            }
          } catch (error) {
            response.statusCode = 500;
            response.end(JSON.stringify({ error: String(error) }));
          }
        });
      },
    }],
  });
  await server.listen();
  browser = await launchCaptureBrowser();
  const page = await createCapturePage(browser, viewport.width, viewport.height);
  try {
    await browser.send("Emulation.setEmulatedMedia", { features: [
      { name: "prefers-color-scheme", value: "light" }, { name: "prefers-reduced-motion", value: "reduce" },
    ] }, page.sessionId);
    await browser.send("Emulation.setTimezoneOverride", { timezoneId: "UTC" }, page.sessionId);
    await browser.send("Emulation.setLocaleOverride", { locale: "en-GB" }, page.sessionId);
    await page.navigate("http://127.0.0.1:4179/public-product");
    await waitFor(page, "document.querySelectorAll('#file-list input[type=checkbox]').length === 8");
    await page.evaluate("document.querySelector('#file-list input[type=checkbox]').click()");
    await waitFor(page, "document.querySelector('#preview-content')?.innerText.includes('The narrow bed')");
    const captures = [];
    async function capture(stage) {
      await waitFor(page, "window.publicProductPendingRequests === 0");
      await page.evaluate("(async () => { await document.fonts.ready; for (const animation of document.getAnimations()) { try { animation.finish(); } catch {} } await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); })()");
      const visibleText = await page.evaluate("document.body.innerText");
      assert.doesNotMatch(visibleText, /\/home\/|\/Users\/|\/tmp\/|Invalid Date|Preparing preview\.\.\.|Counting\.\.\./);
      assert.deepEqual(await page.evaluate("window.publicProductFailures"), []);
      assert.deepEqual(browserFailures(browser, page.sessionId), []);
      assert.equal(await page.evaluate("document.documentElement.dataset.theme"), theme);
      const filename = `corpuswright-${stage}.png`;
      const pixels = await page.screenshot();
      writeFileSync(join(output, filename), pixels);
      captures.push({ stage, asset: filename, sha256: createHash("sha256").update(pixels).digest("hex"),
        visibleTextSha256: createHash("sha256").update(visibleText).digest("hex") });
      process.stdout.write(`Captured ${filename}\n`);
    }
    await capture("prepare");
    const preparationTokenCount = await page.evaluate("Number(document.querySelector('#summary-total-words').textContent.replace(/[,\\s]/g, ''))");
    const coreWordCount = await queryCore("compute_word_count_window_command", { offset: 0, limit: 1000 });
    assert.equal(preparationTokenCount, coreWordCount.totalWords, "Prepare must display the shared core token count");
    await page.evaluate("document.querySelector('#workbench-analyse').click(); document.querySelector('#analyse-build-button').click()");
    await waitFor(page, "document.querySelector('#analyse-status-chip')?.innerText.includes('Built') && document.querySelectorAll('#analyse-overview svg').length >= 6");
    await waitFor(page, "window.publicProductPendingRequests === 0");
    const overviewTotals = await page.evaluate("window.publicProductOverview.totals");
    assert.equal(preparationTokenCount, overviewTotals.tokens, "Prepare and Overview must count the same default-prepared sample");
    await page.evaluate("document.querySelector('[data-plot-id=overview-lexical-spectrum]').closest('.analysis-panel').scrollIntoView({ block: 'start' })");
    assert.equal(await page.evaluate("[...document.querySelectorAll('#analyse-overview .analysis-panel > h3, #analyse-overview .cw-overview-figure-title-block h4')].filter(el => el.textContent.trim() === 'Lexical spectrum').length"), 1);
    await capture("overview");
    await page.evaluate("document.querySelector('[data-analysis-tool=concordance]').click(); document.querySelector('#concordance-pane').scrollIntoView({ block: 'start' }); const input = document.querySelector('#concordance-query-input'); input.value = 'seed'; input.dispatchEvent(new Event('input', { bubbles: true })); document.querySelector('#concordance-search-button').click()");
    await waitFor(page, "document.querySelectorAll('#concordance-lines-body tr').length > 5");
    await page.evaluate("const rows = document.querySelector('#concordance-page-size-input'); rows.value = '3'; rows.dispatchEvent(new Event('change', { bubbles: true }));");
    await waitFor(page, "document.querySelectorAll('#concordance-table-body tr').length === 3 && document.querySelector('#concordance-collocations-table-body')?.innerText.trim().length > 0");
    await waitFor(page, "!document.body.innerText.includes('Searching…') && !document.body.innerText.includes('Updating…')");
    assert.equal(await page.evaluate("document.querySelector('#concordance-collocations-table-body tr td')?.innerText.trim()"), "tray", "The authored corpus should demonstrate a substantive collocate under the default settings");
    const clippedCells = await page.evaluate("[...document.querySelectorAll('#concordance-table-body .concordance-file, #concordance-table-body .concordance-left, #concordance-table-body .concordance-right')].filter(el => el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1).map(el => el.textContent)");
    assert.deepEqual(clippedCells, [], "Concordance filenames and contexts must remain fully readable");
    await capture("concordance");
    await page.evaluate("document.querySelector('#concordance-table-body tr').focus()");
    await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 }, page.sessionId);
    await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 }, page.sessionId);
    await waitFor(page, "Boolean(document.querySelector('.concordance-expansion-context')) && window.publicProductPendingRequests === 0");
    assert.ok(await page.evaluate("document.querySelector('.concordance-expansion-context').textContent.length > document.querySelector('#concordance-table-body tr').textContent.length"), "Keyboard expansion must expose wider source context");
    await browser.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 }, page.sessionId);
    await browser.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 }, page.sessionId);
    await waitFor(page, "!document.querySelector('.concordance-expansion-context')");
    assert.deepEqual(await page.evaluate("window.publicProductFailures"), []);
    assert.deepEqual(browserFailures(browser, page.sessionId), []);
    const sources = Object.fromEntries(Object.keys(data.original.files).map(index => {
      const preview = data.original.files[index];
      return [preview.relative_path, createHash("sha256").update(readFileSync(join(corpus, preview.relative_path))).digest("hex")];
    }));
    const tree = path => {
      execFileSync("git", ["diff", "--quiet", "--", path], { cwd: repository });
      return execFileSync("git", ["write-tree", `--prefix=${path}/`], { cwd: repository, encoding: "utf8" }).trim();
    };
    writeFileSync(join(output, "provenance.json"), JSON.stringify({
      schemaVersion: 1, identity: "GUI demonstration corpus", purpose: "Eight synthetic documents illustrating the application workflow; no collected observations.", viewport, theme, browser: "Chrome headless", capture: "Unmodified application HTML and controllers, native IPC replaced by this isolated core adapter",
      command: "npm --prefix apps/desktop run capture:public-product", generator: "apps/desktop/tests/visual/public-product-corpus.mjs",
      renderedChecks: ["Prepare uses the shared core token count; Overview agrees", "Single Lexical spectrum heading", "Complete Concordance filenames and contexts", "Default MI collocate tray", "Enter opens wider context; Escape closes it"],
      totals: overviewTotals, concordanceAudit, collocationAudit,
      sourceChecksums: sources, coreTree: tree("crates/corpuswright-core"), frontendTree: tree("apps/desktop/src"),
      captureInputs: Object.fromEntries([
        "apps/desktop/scripts/capture-public-product.mjs", "apps/desktop/scripts/browser-capture.mjs",
        "apps/desktop/tests/visual/public-product-corpus.mjs", "apps/desktop/tests/visual/public-product-data.rs",
        "apps/desktop/tests/visual/public-product-fixture.ts", "apps/desktop/index.html",
      ].map(path => [path, createHash("sha256").update(readFileSync(join(repository, path))).digest("hex")])),
      captures,
    }, null, 2) + "\n");
  } finally {
    await page.close();
  }
} finally {
  await browser?.close();
  await server?.close();
  rmSync(temporary, { recursive: true, force: true });
}
