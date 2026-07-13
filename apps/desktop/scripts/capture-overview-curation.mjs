import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";

const recoveryMode = process.argv.includes("--rendering-recovery");
const cleanupMode = process.argv.includes("--distribution-dispersion-cleanup");
const outputDirectory = cleanupMode
  ? "/tmp/corpuswright-overview-distribution-dispersion-cleanup"
  : recoveryMode
    ? "/tmp/corpuswright-overview-rendering-recovery"
    : "/tmp/corpuswright-overview-curation";
const baseUrl = "http://127.0.0.1:4179/tests/visual/overview-suite-fixture.html";
const chromeBinary = process.env.CHROME_BIN ?? "google-chrome";
rmSync(outputDirectory, { recursive: true, force: true });
mkdirSync(outputDirectory, { recursive: true });

const screenshotCases = [
  shot(1, "document-histogram-default", "document", { view: "histogram" }),
  shot(2, "document-ecdf", "document", { view: "ecdf" }),
  shot(3, "document-qq", "document", { view: "qq" }),
  shot(4, "document-custom-bin-options", "document", { view: "histogram", customBins: true }),
  shot(5, "mattr-histogram", "mattr", { view: "histogram" }),
  shot(6, "mattr-ecdf", "mattr", { view: "ecdf" }),
  shot(7, "mattr-qq", "mattr", { view: "qq" }),
  shot(8, "rank-curve", "rank", { view: "curve" }),
  shot(9, "rank-residuals", "rank", { view: "residuals" }),
  shot(10, "vocabulary-growth", "growth", { view: "growth" }),
  shot(11, "vocabulary-heaps-fit", "growth", { view: "heaps" }),
  shot(12, "lexical-spectrum", "lexical"),
  shot(13, "coverage-50", "coverage", { target: "0.5" }),
  shot(14, "coverage-90", "coverage", { target: "0.9" }),
  shot(15, "dispersion-profile", "profile"),
  shot(18, "dispersion-plane", "plane"),
  shot(19, "mattr-1024", "mattr", {}, 1024, 768),
  shot(20, "lexical-spectrum-1024", "lexical", {}, 1024, 768),
  shot(21, "dispersion-profile-1024", "profile", {}, 1024, 768),
  shot(22, "dispersion-plane-1024", "plane", {}, 1024, 768),
  shot(24, "lexical-spectrum-dark", "lexical", { theme: "corpus-dark" }),
  shot(25, "dispersion-profile-dark", "profile", { theme: "corpus-dark" }),
  shot(26, "dispersion-plane-dark", "plane", { theme: "corpus-dark" }),
  shot(28, "lexical-spectrum-high-contrast", "lexical", { theme: "high-contrast-light" }),
  shot(29, "dispersion-plane-high-contrast", "plane", { theme: "high-contrast-light" }),
  shot(30, "pinned-lexical-frequency", "lexical", { pinned: true }),
  shot(31, "pinned-coverage-checkpoint", "coverage", { pinned: true }),
  shot(32, "pinned-dispersion-band", "profile", { pinned: true }),
  shot(33, "pinned-plane-witness", "plane", { pinned: true }),
  shot(34, "custom-document-bins", "document", { customBins: true }),
  shot(35, "keyboard-focused-density-cell", "plane", { focusDensity: true }),
  shot(36, "export-colour-dialog", "lexical", { exportDialog: true }),
];

const exportCases = [
  exportCase("37-default-01-document", "document"),
  exportCase("37-default-02-mattr", "mattr"),
  exportCase("37-default-03-rank", "rank"),
  exportCase("37-default-04-growth", "growth"),
  exportCase("37-default-05-lexical", "lexical"),
  exportCase("37-default-06-coverage", "coverage"),
  exportCase("37-default-07-profile", "profile"),
  exportCase("37-default-09-plane", "plane"),
  exportCase("38-colour-blind-safe-lexical", "lexical", { palette: "colorBlind" }),
  exportCase("39-monochrome-plane", "plane", { palette: "monochrome" }),
  exportCase("40-pinned-evidence-lexical", "lexical", { pinned: true }),
  exportCase("41-custom-bin-document", "document", { customBins: true }),
  exportCase("42-coverage-90", "coverage", { target: "0.9" }),
];

const recoveryScreenshotCases = [
  recoveryShot("document-histogram", "document", { view: "histogram" }),
  recoveryShot("document-ecdf", "document", { view: "ecdf" }),
  recoveryShot("mattr-histogram", "mattr", { view: "histogram" }),
  recoveryShot("lexical-spectrum", "lexical"),
  recoveryShot("cumulative-coverage", "coverage"),
  recoveryShot("rank-curve", "rank", { view: "curve" }),
  recoveryShot("rank-residuals", "rank", { view: "residuals" }),
  recoveryShot("vocabulary-growth", "growth", { view: "heaps" }),
  recoveryShot("dispersion-profile", "profile"),
  recoveryShot("dispersion-plane", "plane"),
  recoveryShot("dispersion-profile-dark", "profile", { theme: "corpus-dark" }),
  recoveryShot("dispersion-plane-dark", "plane", { theme: "corpus-dark" }),
];

const recoveryExportCases = [
  recoveryExportCase("rank-curve", "rank", { view: "curve" }),
  recoveryExportCase("cumulative-coverage", "coverage"),
  recoveryExportCase("dispersion-plane", "plane"),
];

const recoveryAuditCases = [
  recoveryAuditCase("document-histogram-to-ecdf", "document", "ecdf"),
  recoveryAuditCase("document-histogram-to-qq", "document", "qq"),
  recoveryAuditCase("mattr-histogram-to-ecdf", "mattr", "ecdf"),
  recoveryAuditCase("mattr-histogram-to-qq", "mattr", "qq"),
];

const cleanupScreenshotCases = [
  cleanupShot("document-histogram-controls", "document", { view: "histogram" }),
  cleanupShot("document-ecdf-final", "document", { view: "ecdf" }),
  cleanupShot("document-qq-final", "document", { view: "qq" }),
  cleanupShot("mattr-ecdf-final", "mattr", { view: "ecdf" }),
  cleanupShot("mattr-qq-final", "mattr", { view: "qq" }),
  cleanupShot("dispersion-profile", "profile"),
  cleanupShot("dispersion-plane", "plane", { pinned: true }),
  cleanupShot("overview-without-roster", "overview"),
];

const cleanupExportCases = [
  cleanupExportCase("document-ecdf-final", "document", { view: "ecdf" }),
  cleanupExportCase("dispersion-plane", "plane", { pinned: true }),
];

const activeScreenshotCases = cleanupMode ? cleanupScreenshotCases : recoveryMode ? recoveryScreenshotCases : screenshotCases;
const activeExportCases = cleanupMode ? cleanupExportCases : recoveryMode ? recoveryExportCases : exportCases;
const activeAuditCases = recoveryMode ? recoveryAuditCases : [];

class Cdp {
  constructor(url) {
    this.url = url;
    this.id = 0;
    this.pending = new Map();
  }

  async open() {
    this.socket = new WebSocket(this.url);
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(`${pending.method}: ${message.error.message}`));
      else pending.resolve(message.result ?? {});
    });
    await new Promise((resolve, reject) => {
      this.socket.addEventListener("open", resolve, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
  }

  send(method, params = {}, sessionId) {
    const id = ++this.id;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Timed out waiting for CDP method ${method}`));
      }, 15_000);
      this.pending.set(id, {
        method,
        resolve: (value) => { clearTimeout(timeout); resolve(value); },
        reject: (error) => { clearTimeout(timeout); reject(error); },
      });
      this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
}

const server = spawn("npm", ["run", "dev", "--", "--host", "127.0.0.1", "--port", "4179"], {
  cwd: new URL("..", import.meta.url),
  stdio: "ignore",
  detached: true,
});
const chromeProfile = mkdtempSync(join(tmpdir(), "corpuswright-chrome-"));
let chrome;
try {
  await waitForServer();
  const browser = await launchChrome(chromeProfile);
  chrome = browser.process;
  const cdp = new Cdp(browser.webSocketUrl);
  await cdp.open();

  const manifest = [];
  for (const item of activeAuditCases) {
    const page = await createCasePage(cdp);
    try {
      await setViewport(cdp, page.sessionId, 1440, 900);
      const result = await navigateAndWait(cdp, page.sessionId, item.url, false);
      await settlePaint(cdp, page.sessionId);
      manifest.push({ ...item, ...result, browser: chromeBinary });
      process.stdout.write(`audited ${item.name}\n`);
    } finally {
      await cdp.send("Target.closeTarget", { targetId: page.targetId });
    }
  }

  for (const item of activeScreenshotCases) {
    const page = await createCasePage(cdp);
    try {
      await setViewport(cdp, page.sessionId, item.width, item.height);
      const result = await navigateAndWait(cdp, page.sessionId, item.url, false);
      await settlePaint(cdp, page.sessionId);
      const capture = await cdp.send("Page.captureScreenshot", { format: "png", fromSurface: true }, page.sessionId);
      const path = join(outputDirectory, item.filename);
      writeFileSync(path, Buffer.from(capture.data, "base64"));
      manifest.push({ ...item, path, ...result, browser: chromeBinary });
      process.stdout.write(`captured ${item.filename}\n`);
    } finally {
      await cdp.send("Target.closeTarget", { targetId: page.targetId });
    }
  }

  for (const item of activeExportCases) {
    const page = await createCasePage(cdp);
    try {
      await setViewport(cdp, page.sessionId, 1440, 900);
      const result = await navigateAndWait(cdp, page.sessionId, item.url, true);
      await settlePaint(cdp, page.sessionId);
      const evaluated = await cdp.send("Runtime.evaluate", {
        expression: "document.querySelector('svg')?.outerHTML ?? ''",
        returnByValue: true,
      }, page.sessionId);
      const source = evaluated.result.value;
      assertStandaloneSvg(source, item.filename);
      const path = join(outputDirectory, item.filename);
      writeFileSync(path, source);
      manifest.push({ ...item, path, ...result, browser: chromeBinary });
      process.stdout.write(`exported ${item.filename}\n`);
    } finally {
      await cdp.send("Target.closeTarget", { targetId: page.targetId });
    }
  }

  const report = {
    generatedAt: new Date().toISOString(),
    mode: cleanupMode ? "distribution-dispersion-cleanup" : recoveryMode ? "rendering-recovery" : "overview-curation",
    fixture: baseUrl,
    viewport: "1440x900",
    timing: cleanupMode ? { ecdfAndQqMinimumDelayMs: 1_000, animationsFinishedBeforeCapture: true } : undefined,
    auditCount: activeAuditCases.length,
    screenshotCount: activeScreenshotCases.length,
    standaloneSvgCount: activeExportCases.length,
    consoleErrors: manifest.reduce((sum, item) => sum + item.consoleErrors, 0),
    accessibilitySerious: manifest.reduce((sum, item) => sum + item.accessibilitySerious, 0),
    cases: manifest,
  };
  writeFileSync(join(outputDirectory, "manifest.json"), `${JSON.stringify(report, null, 2)}\n`);
  await cdp.send("Browser.close");
  if (report.consoleErrors !== 0 || report.accessibilitySerious !== 0) {
    throw new Error(`Visual harness found ${report.consoleErrors} console errors and ${report.accessibilitySerious} serious accessibility failures`);
  }
  process.stdout.write(`visual harness passed: ${report.screenshotCount} screenshots, ${report.standaloneSvgCount} standalone SVGs\n`);
} finally {
  try { process.kill(-server.pid, "SIGTERM"); } catch { /* The server already stopped. */ }
  chrome?.kill("SIGTERM");
  try { rmSync(chromeProfile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }); } catch { /* Chrome may finish releasing cache files asynchronously. */ }
}

function shot(index, slug, figure, options = {}, width = 1440, height = 900) {
  const filename = `${String(index).padStart(2, "0")}-${slug}.png`;
  return { kind: "screenshot", index, filename, figure, width, height, theme: options.theme ?? "corpus-light", url: fixtureUrl(figure, options) };
}

function exportCase(slug, figure, options = {}) {
  return { kind: "svg", filename: `${slug}.svg`, figure, width: 1440, height: 900, theme: options.theme ?? "corpus-light", url: fixtureUrl(figure, { ...options, export: true }) };
}

function recoveryShot(slug, figure, options = {}) {
  return { kind: "screenshot", filename: `${slug}.png`, figure, width: 1440, height: 900, theme: options.theme ?? "corpus-light", url: fixtureUrl(figure, options) };
}

function recoveryExportCase(slug, figure, options = {}) {
  return { kind: "svg", filename: `${slug}.svg`, figure, width: 1440, height: 900, theme: options.theme ?? "corpus-light", url: fixtureUrl(figure, { ...options, export: true }) };
}

function cleanupShot(slug, figure, options = {}) {
  const postRenderDelayMs = options.view === "ecdf" || options.view === "qq" ? 1_000 : 0;
  return { kind: "screenshot", filename: `${slug}.png`, figure, width: 1440, height: 900, theme: "corpus-light", postRenderDelayMs, url: fixtureUrl(figure, { ...options, theme: "corpus-light" }) };
}

function cleanupExportCase(slug, figure, options = {}) {
  return { kind: "svg", filename: `${slug}.svg`, figure, width: 1440, height: 900, theme: "corpus-light", url: fixtureUrl(figure, { ...options, theme: "corpus-light", export: true }) };
}

function recoveryAuditCase(name, figure, switchTo) {
  return { kind: "audit", name, figure, width: 1440, height: 900, theme: "corpus-light", url: fixtureUrl(figure, { view: "histogram", switchTo }) };
}

function fixtureUrl(figure, options) {
  const url = new URL(baseUrl);
  url.searchParams.set("figure", figure);
  for (const [key, value] of Object.entries(options)) url.searchParams.set(key, String(value));
  return url.href;
}

async function waitForServer() {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(baseUrl);
      if (response.ok) return;
    } catch {
      // Vite is still starting.
    }
    await delay(100);
  }
  throw new Error("Timed out waiting for the visual fixture server");
}

async function launchChrome(profile) {
  const process = spawn(chromeBinary, [
    "--headless=new",
    "--no-sandbox",
    "--disable-gpu",
    "--hide-scrollbars",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  return await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`Timed out launching Chrome: ${stderr}`)), 10_000);
    process.stderr.on("data", (chunk) => {
      stderr += String(chunk);
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (!match) return;
      clearTimeout(timeout);
      resolve({ process, webSocketUrl: match[1] });
    });
    process.once("exit", (code) => reject(new Error(`Chrome exited before DevTools was ready (${code}): ${stderr}`)));
  });
}

async function createCasePage(cdp) {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  await cdp.send("Target.activateTarget", { targetId });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send("Page.bringToFront", {}, sessionId);
  return { targetId, sessionId };
}

async function setViewport(cdp, sessionId, width, height) {
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
    screenWidth: width,
    screenHeight: height,
  }, sessionId);
}

async function navigateAndWait(cdp, sessionId, url, exporting) {
  await cdp.send("Page.navigate", { url }, sessionId);
  const fixtureMetrics = "(() => { try { return JSON.parse(document.body?.dataset.fixtureMetrics ?? '{}'); } catch { return {}; } })()";
  const expression = exporting
    ? `({ href: location.href, status: document.body?.dataset.fixtureStatus, ready: document.body?.dataset.exportReady, consoleErrors: Number(document.body?.dataset.consoleErrors ?? 0), accessibilitySerious: Number(document.body?.dataset.accessibilitySerious ?? 0), metrics: ${fixtureMetrics}, message: document.querySelector('#suite-status')?.textContent ?? '' })`
    : `({ href: location.href, status: document.body?.dataset.fixtureStatus, ready: 'true', consoleErrors: Number(document.body?.dataset.consoleErrors ?? 0), accessibilitySerious: Number(document.body?.dataset.accessibilitySerious ?? 0), metrics: ${fixtureMetrics}, message: document.querySelector('#suite-status')?.textContent ?? '' })`;
  for (let attempt = 0; attempt < 120; attempt += 1) {
    const evaluated = await cdp.send("Runtime.evaluate", { expression, returnByValue: true }, sessionId);
    const result = evaluated.result.value;
    if (result?.href === url && result.status === "failed") throw new Error(`Fixture failed for ${url}: ${result.message}`);
    if (result?.href === url && result.status === "passed" && result.ready === "true") {
      if (result.consoleErrors !== 0 || result.accessibilitySerious !== 0) {
        throw new Error(`Fixture audit failed for ${url}: ${result.message}`);
      }
      return result;
    }
    await delay(50);
  }
  throw new Error(`Timed out waiting for fixture: ${url}`);
}

async function settlePaint(cdp, sessionId) {
  await cdp.send("Runtime.evaluate", {
    expression: "(async () => { for (const animation of document.getAnimations()) { try { animation.finish(); } catch {} } await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); })()",
    awaitPromise: true,
  }, sessionId);
}

function assertStandaloneSvg(source, filename) {
  if (!source.startsWith("<svg") || !source.includes("</svg>")) throw new Error(`${filename} is not standalone SVG`);
  if (/var\(--/.test(source)) throw new Error(`${filename} contains unresolved CSS variables`);
  for (const required of ["<title", "<desc", "cw-plot-figure-title", "cw-plot-figure-reading"]) {
    if (!source.includes(required)) throw new Error(`${filename} is missing ${required}`);
  }
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
