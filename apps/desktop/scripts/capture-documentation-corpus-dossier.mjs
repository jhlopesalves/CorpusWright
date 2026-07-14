import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, spawnSync } from "node:child_process";

const dossierDirectory = process.env.CORPUSWRIGHT_DOCUMENTATION_DOSSIER
  ?? "/tmp/corpuswright-documentation-corpus-dossier";
const payloadPath = join(dossierDirectory, "dossier.json");
const baseUrl = "http://127.0.0.1:4179/tests/visual/documentation-corpus-fixture.html";
const payloadUrl = "http://127.0.0.1:4180/dossier.json";
const chromeBinary = process.env.CHROME_BIN ?? "google-chrome";
const ffmpegBinary = process.env.FFMPEG_BIN ?? "ffmpeg";
const payloadBytes = readFileSync(payloadPath);

const cases = [
  witness("document-length-histogram", "Document-length histogram", "document", "histogram", "Full-corpus document balance", "Binning and scale affect appearance; read with quartiles."),
  witness("document-length-ecdf", "Document-length ECDF", "document", "ecdf", "Full-corpus document percentiles", "Small document count produces visible steps."),
  witness("document-length-qq", "Document-length Q–Q", "document", "qq", "Selected/full-corpus shape diagnostic", "A reference comparison, not a normality test."),
  witness("rank-frequency", "Rank–frequency curve", "rank", "curve", "Full-corpus Zipf discussion", "The fit excludes documented head and tail ranks."),
  witness("rank-residuals", "Rank residuals", "rank", "residuals", "Full-corpus departures from fitted rank interval", "Residuals depend on the fitted interval."),
  witness("lexical-spectrum", "Lexical spectrum", "lexical", "histogram", "Full-corpus hapax/dislegomena evidence", "Log axes require careful frequency-class reading."),
  witness("cumulative-coverage", "Cumulative coverage", "coverage", "histogram", "Full-corpus Top-N token coverage", "Coverage is token mass, not semantic completeness."),
  witness("vocabulary-growth", "Vocabulary growth", "growth", "growth", "Full-corpus observed vocabulary growth", "The curve depends on deterministic document order."),
  witness("heaps-fit", "Heaps fit", "growth", "heaps", "Full-corpus fitted growth model", "The fit describes this snapshot only."),
  witness("mattr-distribution", "MATTR distribution", "mattr", "histogram", "Full-corpus lexical-diversity variation", "MATTR is window-dependent and higher is not automatically better."),
  witness("dispersion-profile", "Dispersion profile", "profile", "histogram", "Full-corpus document-frequency reach", "Exclusive reach bands differ from cumulative thresholds."),
  witness("dispersion-plane", "Dispersion plane", "plane", "histogram", "Full-corpus density plus selected real witnesses", "Thresholds are descriptive and DP is corpus-internal."),
];

const outputDirectories = {
  light: join(dossierDirectory, "figures/light"),
  dark: join(dossierDirectory, "figures/dark"),
  png: join(dossierDirectory, "figures/png"),
  logs: join(dossierDirectory, "logs"),
};
for (const directory of Object.values(outputDirectories)) mkdirSync(directory, { recursive: true });

async function capture() {
const payloadServer = createServer((request, response) => {
  if (request.url !== "/dossier.json") {
    response.writeHead(404).end();
    return;
  }
  response.writeHead(200, {
    "Content-Type": "application/json; charset=utf-8",
    "Access-Control-Allow-Origin": "http://127.0.0.1:4179",
    "Cache-Control": "no-store",
  });
  response.end(payloadBytes);
});
await new Promise((resolve, reject) => {
  payloadServer.once("error", reject);
  payloadServer.listen(4180, "127.0.0.1", resolve);
});

const vite = spawn("npm", ["run", "dev", "--", "--host", "127.0.0.1", "--port", "4179"], {
  cwd: new URL("..", import.meta.url),
  stdio: "ignore",
  detached: true,
});
const chromeProfile = mkdtempSync(join(tmpdir(), "corpuswright-documentation-chrome-"));
let chrome;
try {
  await waitForServer(baseUrl);
  const browser = await launchChrome(chromeProfile);
  chrome = browser.process;
  const cdp = new Cdp(browser.webSocketUrl);
  await cdp.open();
  const manifest = [];
  for (const theme of ["light", "dark"]) {
    const themeName = theme === "light" ? "corpus-light" : "corpus-dark";
    for (const item of cases) {
      const page = await createCasePage(cdp);
      try {
        await setViewport(cdp, page.sessionId, 1440, 1400);
        const url = fixtureUrl(item, themeName);
        const result = await navigateAndWait(cdp, page.sessionId, url);
        await settlePaint(cdp, page.sessionId);
        const evaluated = await cdp.send("Runtime.evaluate", {
          expression: "globalThis.documentationCorpusExportSvg ?? ''",
          returnByValue: true,
        }, page.sessionId);
        const source = evaluated.result.value;
        assertStandaloneSvg(source, item.slug);
        const svgPath = join(outputDirectories[theme], `${item.slug}.svg`);
        const pngPath = join(outputDirectories.png, `${theme}-${item.slug}.png`);
        writeFileSync(svgPath, source);
        const rasterSourceResult = await cdp.send("Runtime.evaluate", {
          expression: `(() => {
            const source = globalThis.documentationCorpusExportSvg;
            if (!source) throw new Error("Missing documentation corpus standalone SVG");
            const parsed = new DOMParser().parseFromString(source, "image/svg+xml");
            const svg = parsed.documentElement;
            const background = svg.querySelector(".cw-plot-export-background");
            const fill = background instanceof SVGElement && background.style.fill
              ? background.style.fill
              : "rgb(255, 255, 255)";
            background?.remove();
            svg.querySelectorAll("filter, title, desc, .cw-plot-line-halo").forEach((element) => element.remove());
            for (const definition of [...svg.querySelectorAll("defs > *")]) {
              const id = definition.getAttribute("id");
              if (!id) continue;
              const reference = "url(#" + id + ")";
              const used = [...svg.querySelectorAll("*")].some((element) =>
                element !== definition
                && [...element.attributes].some((attribute) => attribute.value.includes(reference))
              );
              if (!used) definition.remove();
            }
            svg.querySelectorAll("defs:empty").forEach((definitions) => definitions.remove());
            const viewBox = svg.getAttribute("viewBox")?.split(/\\s+/).map(Number);
            if (!viewBox || viewBox.length !== 4 || viewBox.some((value) => !Number.isFinite(value))) {
              throw new Error("Standalone export has no valid viewBox");
            }
            const content = parsed.createElementNS("http://www.w3.org/2000/svg", "g");
            content.setAttribute("transform", "translate(" + (-viewBox[0]) + " " + (-viewBox[1]) + ")");
            for (const child of [...svg.children]) {
              if (child.localName !== "defs") content.appendChild(child);
            }
            svg.setAttribute("viewBox", "0 0 " + viewBox[2] + " " + viewBox[3]);
            const opaqueBackground = parsed.createElementNS("http://www.w3.org/2000/svg", "rect");
            opaqueBackground.setAttribute("x", "0");
            opaqueBackground.setAttribute("y", "0");
            opaqueBackground.setAttribute("width", String(viewBox[2]));
            opaqueBackground.setAttribute("height", String(viewBox[3]));
            opaqueBackground.setAttribute("fill", fill);
            svg.insertBefore(opaqueBackground, svg.firstChild);
            svg.appendChild(content);
            return new XMLSerializer().serializeToString(svg);
          })()`,
          returnByValue: true,
        }, page.sessionId);
        const rasterSource = rasterSourceResult.result.value;
        if (typeof rasterSource !== "string" || !rasterSource.startsWith("<svg")) {
          throw new Error(`Unable to prepare ${item.slug} for PNG rasterisation: ${JSON.stringify(rasterSourceResult)}`);
        }
        const { width, height } = svgRasterDimensions(source, item.slug);
        const rasterInputPath = join(chromeProfile, `${theme}-${item.slug}.svg`);
        writeFileSync(rasterInputPath, rasterSource);
        const raster = spawnSync(ffmpegBinary, [
          "-hide_banner", "-loglevel", "error", "-y",
          "-width", String(width), "-height", String(height),
          "-i", rasterInputPath, "-frames:v", "1", pngPath,
        ], { encoding: "utf8", maxBuffer: 20 * 1024 * 1024 });
        rmSync(rasterInputPath, { force: true });
        if (raster.error || raster.status !== 0) {
          throw new Error(`Unable to rasterise ${item.slug} with FFmpeg/librsvg: ${raster.error?.message ?? raster.stderr}`);
        }
        manifest.push({
          ...item,
          theme: themeName,
          svgPath: `figures/${theme}/${item.slug}.svg`,
          pngPath: `figures/png/${theme}-${item.slug}.png`,
          renderedInspection: "PASS",
          consoleErrors: result.consoleErrors,
          accessibilitySerious: result.accessibilitySerious,
          metrics: result.metrics,
        });
        process.stdout.write(`rendered ${theme}/${item.slug}\n`);
      } finally {
        await cdp.send("Target.closeTarget", { targetId: page.targetId });
      }
    }
  }
  const report = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    renderer: "CorpusWright plot model, measured layout, SVG renderer, theme tokens, standalone exporter",
    pngRasterizer: "FFmpeg with librsvg at the native standalone-SVG viewBox size",
    corpusPayload: payloadPath,
    corpusLightCount: manifest.filter((item) => item.theme === "corpus-light").length,
    corpusDarkCount: manifest.filter((item) => item.theme === "corpus-dark").length,
    forbiddenPaintCount: 0,
    figures: manifest,
  };
  writeFileSync(join(dossierDirectory, "figures/manifest.json"), `${JSON.stringify(report, null, 2)}\n`);
  writeFileSync(join(outputDirectories.logs, "figure-capture.log"), `PASS ${manifest.length} witnesses\n`);
  await cdp.send("Browser.close");
  process.stdout.write(`documentation corpus figure dossier passed: ${manifest.length} real witnesses\n`);
} finally {
  try { process.kill(-vite.pid, "SIGTERM"); } catch { /* Vite already stopped. */ }
  chrome?.kill("SIGTERM");
  await new Promise((resolve) => payloadServer.close(resolve));
  try { rmSync(chromeProfile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }); } catch { /* Chrome can hold cache files briefly. */ }
}
}

function witness(slug, title, figure, view, conceptsRole, limitations) {
  return {
    slug,
    title,
    figure,
    view,
    pedagogicalSuitability: "SUITABLE",
    limitations,
    proposedConceptsPageRole: conceptsRole,
  };
}

function fixtureUrl(item, theme) {
  const url = new URL(baseUrl);
  url.searchParams.set("figure", item.figure);
  url.searchParams.set("view", item.view);
  url.searchParams.set("theme", theme);
  url.searchParams.set("payload", payloadUrl);
  url.searchParams.set("export", "true");
  return url.href;
}

class Cdp {
  constructor(url) {
    this.url = url;
    this.id = 0;
    this.pending = new Map();
    this.events = [];
  }

  async open() {
    this.socket = new WebSocket(this.url);
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) {
        if (message.method === "Runtime.exceptionThrown" || message.method === "Runtime.consoleAPICalled" || message.method === "Log.entryAdded") {
          this.events.push(message);
        }
        return;
      }
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

async function waitForServer(url) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // Vite is still starting.
    }
    await delay(100);
  }
  throw new Error("Timed out waiting for documentation corpus visual fixture");
}

async function launchChrome(profile) {
  const process = spawn(chromeBinary, [
    "--headless=new",
    "--no-sandbox",
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

async function navigateAndWait(cdp, sessionId, url) {
  await cdp.send("Page.navigate", { url }, sessionId);
  const expression = `({ href: location.href, status: document.body?.dataset.fixtureStatus, ready: document.body?.dataset.exportReady, consoleErrors: Number(document.body?.dataset.consoleErrors ?? 0), accessibilitySerious: Number(document.body?.dataset.accessibilitySerious ?? 0), metrics: (() => { try { return JSON.parse(document.body?.dataset.fixtureMetrics ?? '{}'); } catch { return {}; } })(), message: document.querySelector('#suite-status')?.textContent ?? '' })`;
  for (let attempt = 0; attempt < 600; attempt += 1) {
    const evaluated = await cdp.send("Runtime.evaluate", { expression, returnByValue: true }, sessionId);
    const result = evaluated.result.value;
    if (result?.href === url && result.status === "failed") {
      throw new Error(`Fixture failed for ${url}: ${result.message}\nCDP events: ${JSON.stringify(cdp.events.slice(-10))}`);
    }
    if (result?.href === url && result.status === "passed" && result.ready === "true") {
      if (result.consoleErrors !== 0 || result.accessibilitySerious !== 0) {
        throw new Error(`Fixture audit failed for ${url}: ${result.message}`);
      }
      return result;
    }
    await delay(50);
  }
  const diagnostic = await cdp.send("Runtime.evaluate", {
    expression: "({ readyState: document.readyState, href: location.href, stage: document.body?.dataset.fixtureStage, status: document.body?.dataset.fixtureStatus, errors: document.body?.dataset.consoleErrors, body: document.body?.innerText?.slice(0, 1000) ?? '', html: document.documentElement?.outerHTML?.slice(0, 2000) ?? '' })",
    returnByValue: true,
  }, sessionId);
  throw new Error(`Timed out waiting for fixture: ${url}\n${JSON.stringify(diagnostic.result.value)}\nCDP events: ${JSON.stringify(cdp.events.slice(-10))}`);
}

async function settlePaint(cdp, sessionId) {
  await cdp.send("Runtime.evaluate", {
    expression: "(async () => { for (const animation of document.getAnimations()) { try { animation.finish(); } catch {} } await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); })()",
    awaitPromise: true,
  }, sessionId);
}

function assertStandaloneSvg(source, slug) {
  if (!source.startsWith("<svg") || !source.includes("</svg>")) throw new Error(`${slug} is not standalone SVG`);
  for (const [pattern, label] of [
    [/var\s*\(/i, "unresolved CSS variables"],
    [/color-mix\s*\(/i, "unresolved color-mix paint"],
    [/color\(\s*srgb\b/i, "non-portable color(srgb) paint"],
  ]) {
    if (pattern.test(source)) throw new Error(`${slug} contains ${label}`);
  }
  for (const required of ["<title", "<desc", "cw-plot-figure-title", "cw-plot-figure-reading"]) {
    if (!source.includes(required)) throw new Error(`${slug} is missing ${required}`);
  }
  if (/cw-plot-tooltip|cw-plot-crosshair-[xy]|cw-plot-hover-target|\bis-hovering\b|\btabindex=|(?:^|;)\s*(?:--cw-plot-stagger|animation|animation-delay|transition)\s*:/.test(source)) {
    throw new Error(`${slug} retains transient interaction or animation state`);
  }
}

function svgRasterDimensions(source, slug) {
  const match = source.match(/\bviewBox="([^\"]+)"/);
  const values = match?.[1].trim().split(/\s+/).map(Number);
  if (!values || values.length !== 4 || values.some((value) => !Number.isFinite(value))) {
    throw new Error(`${slug} has no valid SVG viewBox`);
  }
  return { width: Math.ceil(values[2]), height: Math.ceil(values[3]) };
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

await capture();
