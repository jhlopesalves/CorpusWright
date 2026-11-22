/** Capture the pinned walkthrough through the unmodified desktop UI and native core. */
import assert from "node:assert/strict";
import { execFile, execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { createServer } from "vite";
import { browserFailures, createCapturePage, launchCaptureBrowser, waitFor } from "./browser-capture.mjs";

const desktop = fileURLToPath(new URL("..", import.meta.url));
const repository = resolve(desktop, "../..");
const fixturePath = "apps/desktop/tests/visual/walkthrough-fixture.ts";
const adapterPath = "apps/desktop/tests/visual/walkthrough-data.rs";
const sourcesPath = "apps/desktop/tests/visual/walkthrough-sources.json";
const output = resolve(process.argv[2] ?? join(tmpdir(), "corpuswright-walkthrough-output"));
const viewport = { width: 1440, height: 1000, deviceScaleFactor: 1 };
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const execute = promisify(execFile);

async function completeAll(tasks) {
  const results = await Promise.allSettled(tasks);
  const failures = results.filter(result => result.status === "rejected");
  if (failures.length) throw new AggregateError(failures.map(result => result.reason));
  return results.map(result => result.value);
}

async function fetchCorpus(directory) {
  assert.ok(relative(repository, directory).split("/")[0] === "..", "Corpus must stay outside the repository");
  const pins = JSON.parse(await readFile(join(repository, sourcesPath), "utf8"));
  assert.equal(pins.length, 10);
  await completeAll(pins.map(async ({ repository: name, commit, path, sha256 }) => {
    const destination = resolve(directory, path);
    assert.ok(relative(directory, destination).split("/")[0] !== "..");
    const url = `https://raw.githubusercontent.com/GITenberg/${name}/${commit}/${path}`;
    const response = await fetch(url, { signal: AbortSignal.timeout(45_000) });
    assert.ok(response.ok, `${path}: HTTP ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    assert.equal(digest(bytes), sha256, `Checksum mismatch for ${path}`);
    await mkdir(dirname(destination), { recursive: true });
    await writeFile(destination, bytes);
  }));
  return Object.fromEntries(pins.map(({ path, sha256 }) => [path, sha256]));
}

async function buildAdapter(directory) {
  await mkdir(directory);
  await writeFile(join(directory, "Cargo.toml"), `
[package]
name = "corpuswright-walkthrough-data"
version = "0.0.0"
edition = "2024"
[dependencies]
corpuswright-core = { path = ${JSON.stringify(join(repository, "crates/corpuswright-core"))} }
serde_json = "1"
[[bin]]
name = "corpuswright-walkthrough-data"
path = ${JSON.stringify(join(repository, adapterPath))}
`);
  await copyFile(join(repository, "Cargo.lock"), join(directory, "Cargo.lock"));
  const target = join(repository, "target");
  execFileSync("cargo", ["build", "--offline", "--manifest-path", join(directory, "Cargo.toml"), "--target-dir", target], {
    cwd: repository, stdio: "inherit",
  });
  return join(target, "debug", "corpuswright-walkthrough-data");
}

function openAdapter(binary, args) {
  const child = spawn(binary, args, { stdio: ["pipe", "pipe", "inherit"] });
  const replies = createInterface({ input: child.stdout });
  const requests = new Map();
  let id = 0;
  let finished;
  const stopped = new Promise(resolve => { finished = resolve; });
  const fail = error => {
    for (const { reject } of requests.values()) reject(error);
    requests.clear();
  };
  child.once("error", fail);
  child.once("close", code => { fail(new Error(`Walkthrough adapter stopped (${code})`)); finished(); });
  child.stdin.on("error", fail);
  replies.on("line", line => {
    const { id: replyId, value, error } = JSON.parse(line);
    const request = requests.get(replyId);
    requests.delete(replyId);
    if (error) request?.reject(new Error(error));
    else request?.resolve(value);
  });
  return {
    call(command, args = {}) {
      if (child.exitCode !== null) throw new Error("Walkthrough adapter is closed");
      const requestId = id += 1;
      return new Promise((resolve, reject) => {
        requests.set(requestId, { resolve, reject });
        child.stdin.write(JSON.stringify({ id: requestId, command, args }) + "\n");
      });
    },
    async close() {
      child.stdin.end();
      await stopped;
    },
  };
}

const temporary = await mkdtemp(join(tmpdir(), "corpuswright-walkthrough-data-"));
const corpus = join(temporary, "corpus");
const ocr = join(temporary, "ocr");
const exports = join(temporary, "exports");
const scratchOutput = join(temporary, "captures");
const captures = [];
let artefacts;
let adapter;
let server;
let browser;

function reply(response, value) {
  response.setHeader("Content-Type", "application/json");
  response.end(JSON.stringify(value));
}

function nodeClick(selector) {
  return `document.querySelector(${JSON.stringify(selector)}).click()`;
}

function control(selector, value, event = "change") {
  return `(() => {
    const control = document.querySelector(${JSON.stringify(selector)});
    control.value = ${JSON.stringify(value)};
    control.dispatchEvent(new Event(${JSON.stringify(event)}, { bubbles: true }));
  })()`;
}

async function capture(page, stage, theme) {
  await waitFor(page, "window.walkthroughPendingRequests === 0");
  await page.evaluate("(async () => { await document.fonts.ready; await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); })()");
  const visible = await page.evaluate("document.body.innerText");
  assert.doesNotMatch(visible, /\/home\/|\/Users\/|\/tmp\/|Invalid Date|Preparing preview\.\.\.|Build failed/);
  assert.deepEqual(await page.evaluate("window.walkthroughFailures"), []);
  assert.deepEqual(browserFailures(browser, page.sessionId), []);
  assert.equal(await page.evaluate("document.documentElement.dataset.theme"), `wright-${theme}`);
  const asset = `${stage}-${theme}.png`;
  const png = await page.screenshot();
  assert.deepEqual([png.readUInt32BE(16), png.readUInt32BE(20)], [viewport.width, viewport.height]);
  await writeFile(join(scratchOutput, asset), png);
  captures.push({ stage, asset, theme: `wright-${theme}`, sha256: digest(png), visibleTextSha256: digest(visible) });
  process.stdout.write(`Captured ${asset}\n`);
}

function findFragment(report) {
  const flatlandHeader = report.candidates.filter(candidate => (
    ["exact_line", "normalized_line"].includes(candidate.kind)
    && /ebook is for the use of anyone/i.test(candidate.display_text)
    && candidate.examples.some(example => example.file_name === "97.txt" && example.line_number < 25)
  ));
  const fragment = flatlandHeader.find(candidate => (
    report.promotions.some(promotion => promotion.candidate_id === candidate.candidate_id && promotion.rule)
  ));
  assert.ok(fragment, "The Flatland licence header must be an actual promotable candidate");
  return fragment;
}

async function openCandidate(page, candidate) {
  const row = `#tbl-artifact-candidates tr[data-id="${candidate.candidate_id}"]`;
  const category = await page.evaluate(`(() => {
    for (const tab of document.querySelectorAll("#artifact-type-tabs button:not([disabled])")) {
      tab.click();
      if (document.querySelector(${JSON.stringify(row)})) return tab.dataset.category;
    }
    return null;
  })()`);
  assert.ok(category, "The licence candidate must be reviewable in the real UI");
  await page.evaluate(`document.querySelector(${JSON.stringify(row)}).scrollIntoView({block:"center"})`);
  await page.evaluate(nodeClick(row));
  await page.evaluate(nodeClick(`${row} input`));
  await waitFor(page, "document.querySelector('#repeated-artifact-decision-summary').textContent.includes('1 marked for removal')");
  await page.evaluate(nodeClick("#artifact-decision-filters [data-decision-filter=marked]"));
}

async function frameSection(page, selector, maxTop = 250) {
  const target = `document.querySelector(${JSON.stringify(selector)})`;
  await page.evaluate(`${target}.scrollIntoView({ block: "start", behavior: "instant" })`);
  await waitFor(page, `(() => {
    const bounds = ${target}.getBoundingClientRect();
    return bounds.top >= 135 && bounds.top < ${maxTop} && bounds.bottom <= 972;
  })()`);
}

async function walkthrough(url, theme) {
  const page = await createCapturePage(browser, viewport.width, viewport.height);
  try {
    await browser.send("Emulation.setEmulatedMedia", { features: [
      { name: "prefers-color-scheme", value: theme }, { name: "prefers-reduced-motion", value: "reduce" },
    ] }, page.sessionId);
    await browser.send("Emulation.setTimezoneOverride", { timezoneId: "UTC" }, page.sessionId);
    await browser.send("Emulation.setLocaleOverride", { locale: "en-GB" }, page.sessionId);
    await page.navigate(`${url}/walkthrough`);
    await waitFor(page, "document.querySelectorAll('#file-list input[type=checkbox]').length === 10 && Number(document.querySelector('#summary-total-words')?.textContent.replace(/[^0-9]/g,'')) > 0");
    const preparationCount = await page.evaluate("Number(document.querySelector('#summary-total-words').textContent.replace(/[^0-9]/g,''))");
    await page.evaluate("Array.from(document.querySelectorAll('#file-list li')).find(row => row.innerText.includes('97.txt')).querySelector('input').click()");
    await waitFor(page, "document.querySelector('#preview-content').innerText.includes('This eBook is for the use of anyone')");
    await capture(page, "01-sources", theme);
    await page.evaluate("Array.from(document.querySelectorAll('#file-list li')).find(row => row.innerText.includes('97.txt')).querySelector('input').click()");
    await page.evaluate(nodeClick("#prepare-repeated-artifact-finder"));
    await page.evaluate(control("#num-artifact-min-files", "3"));
    await page.evaluate(nodeClick("#btn-run-artifact-scan"));
    await waitFor(page, "document.querySelector('#artifact-scan-diagnostics').textContent.startsWith('Files: 10/10 scanned')");
    const fragment = findFragment(artefacts);
    await openCandidate(page, fragment);
    await capture(page, "02-artefacts", theme);
    await page.evaluate(nodeClick("#btn-apply-artifact-decisions"));
    await waitFor(page, "document.querySelector('#repeated-artifact-modal').classList.contains('hidden')");
    await page.evaluate("Array.from(document.querySelectorAll('#file-list li')).find(row => row.innerText.includes('97.txt')).querySelector('input').click()");
    await page.evaluate(nodeClick('[data-target="processed-text"]'));
    await waitFor(page, "document.querySelector('#processed-preview-content').innerText.includes('Flatland') && !document.querySelector('#processed-preview-content').innerText.includes('This eBook is for the use of anyone')");
    await capture(page, "03-cleaning", theme);
    await page.evaluate(nodeClick("#workbench-analyse"));
    await page.evaluate(nodeClick("#analyse-build-button"));
    await waitFor(page, "document.querySelector('#analyse-status-chip')?.innerText.includes('Built') && document.querySelector('[data-plot-id=overview-dispersion-plane]')");
    await page.evaluate(nodeClick('[data-analysis-tool="frequency"]'));
    await page.evaluate(control("#frequency-search-input", "gutenberg", "input"));
    await page.evaluate(control("#frequency-match-mode", "exact"));
    await page.evaluate(nodeClick("#frequency-case-fold"));
    await page.evaluate(control("#frequency-token-class", "all"));
    await waitFor(page, "document.querySelector('#frequency-table-body').innerText.toLowerCase().includes('gutenberg')");
    await capture(page, "04-counts", theme);
    await page.evaluate(nodeClick('[data-analysis-tool="overview"]'));
    await frameSection(page, ".cw-overview-figure:has([data-plot-id=overview-dispersion-plane])");
    await capture(page, "05-dispersion", theme);
    await page.evaluate(nodeClick('[data-analysis-tool="concordance"]'));
    await page.evaluate(control("#concordance-sort-1-select", "filePosition"));
    await page.evaluate(control("#concordance-page-size-input", "4"));
    await page.evaluate(control("#concordance-query-input", "probability", "input"));
    await page.evaluate(nodeClick("#concordance-search-button"));
    await waitFor(page, "document.querySelector('#concordance-table-body tr td') && document.querySelector('#concordance-distribution').children.length > 0");
    await waitFor(page, "window.walkthroughPendingRequests === 0");
    await browser.send("Page.bringToFront", {}, page.sessionId);
    await page.evaluate("document.querySelector('.analyse-workbench').scrollTo({ top: 0, behavior: 'instant' })");
    await waitFor(page, "(() => { const row = document.querySelector('#concordance-table-body tr').getBoundingClientRect(); return row.top >= 140 && row.bottom < 969; })()");
    await capture(page, "06-context", theme);
    // The native Concordance panels are stacked, with Collocates between them.
    // Capture Distribution independently at its normal size.
    await frameSection(page, "#concordance-distribution-panel", 440);
    await capture(page, "06-distribution", theme);
    assert.notEqual(captures.at(-1).sha256, captures.at(-2).sha256, "Distinct Concordance panels must yield distinct captures");
    await page.evaluate("window.walkthroughChooseExport()");
    await page.evaluate(nodeClick("#menu-save-corpus"));
    await waitFor(page, "Boolean(window.walkthroughExportExcerpt)");
    const exportExcerpt = await page.evaluate("window.walkthroughExportExcerpt");
    return { fragment, preparationCount, exportExcerpt };
  } catch (error) {
    const details = await page.evaluate("window.walkthroughFailures ?? []").catch(() => []);
    if (details.length) process.stderr.write(`${JSON.stringify(details)}\n`);
    throw error;
  } finally {
    await page.close();
  }
}

function tree(path) {
  const status = execFileSync("git", ["status", "--porcelain", "--untracked-files=all", "--", path], {
    cwd: repository, encoding: "utf8",
  });
  assert.equal(status, "", `Capture from a clean application tree: ${path}`);
  return execFileSync("git", ["rev-parse", `HEAD:${path}`], { cwd: repository, encoding: "utf8" }).trim();
}

try {
  const [sourceChecksums] = await completeAll([
    fetchCorpus(corpus),
    execute(process.execPath, [join(repository, "scripts/fetch-pdfium.cjs"), "linux-x64", "--output-dir", ocr]),
    mkdir(exports),
    mkdir(scratchOutput),
  ]);
  const binary = await buildAdapter(join(temporary, "helper"));
  server = await createServer({
    root: desktop,
    server: { host: "127.0.0.1", port: 0 },
    plugins: [{
      name: "walkthrough-capture-only",
      configureServer(vite) {
        vite.middlewares.use(async (request, response, next) => {
          if (request.url === "/walkthrough") {
            const html = (await readFile(join(desktop, "index.html"), "utf8"))
              .replace('src="/src/main.ts"', 'src="/tests/visual/walkthrough-fixture.ts"');
            response.setHeader("Content-Type", "text/html");
            response.end(await vite.transformIndexHtml("/index.html", html));
          } else if (request.url === "/walkthrough-ipc" && request.method === "POST") {
            try {
              const chunks = [];
              for await (const chunk of request) chunks.push(chunk);
              const { command, args } = JSON.parse(Buffer.concat(chunks).toString("utf8"));
              const value = await adapter.call(command, args);
              if (command === "scan_repeated_artifacts_command") artefacts = value;
              reply(response, { value });
            } catch (error) {
              reply(response, { error: String(error) });
            }
          } else {
            next();
          }
        });
      },
    }],
  });
  await server.listen();
  const url = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await launchCaptureBrowser();
  let witness;
  let audit;
  for (const theme of ["light", "dark"]) {
    const themeExport = join(exports, theme);
    await mkdir(themeExport);
    adapter = openAdapter(binary, [corpus, ocr, themeExport]);
    try {
      const current = await walkthrough(url, theme);
      if (witness) {
        assert.deepEqual(current, witness, "Both themes must witness the same state");
      } else {
        witness = current;
        audit = await adapter.call("audit");
      }
    } finally {
      await adapter.close();
    }
  }
  assert.equal(witness.preparationCount, audit.facts.originalTokens, "Prepare and Concordance must agree on the original count");
  assert.equal(audit.facts.rules, 1, "Exactly one reviewed rule must apply during analysis");
  assert.ok(audit.facts.tokensRemoved > 0, "The accepted fragment must affect the analytical population");
  const properties = [
    "apps/desktop/scripts/capture-walkthrough.mjs", "apps/desktop/scripts/browser-capture.mjs",
    fixturePath, adapterPath, sourcesPath, "apps/desktop/index.html", "apps/desktop/package-lock.json",
    "Cargo.lock", "rust-toolchain.toml", "scripts/fetch-pdfium.cjs", "third-party/pdfium.json",
  ];
  const provenance = {
    schemaVersion: 1,
    capture: "Unmodified application rendered in headless Chrome, with core-backed native IPC",
    viewport,
    coreTree: tree("crates/corpuswright-core"),
    frontendTree: tree("apps/desktop/src"),
    sourceChecksums,
    captureInputs: Object.fromEntries(await Promise.all(properties.map(async path => [path, digest(await readFile(join(repository, path)))]))),
    ...audit,
    facts: {
      ...audit.facts,
      artefactFilesScanned: artefacts.diagnostics.files_scanned,
      artefactCandidates: artefacts.diagnostics.final_candidates,
      artefactCandidateRange: witness.fragment.file_count,
      artefactCandidateOccurrences: witness.fragment.occurrence_count,
      exportWarnings: witness.exportExcerpt.warnings_count,
    },
    exportExcerpt: witness.exportExcerpt,
    captures,
  };
  await mkdir(output, { recursive: true });
  for (const { asset } of captures) await copyFile(join(scratchOutput, asset), join(output, asset));
  await writeFile(join(output, "provenance.json"), JSON.stringify(provenance, null, 2) + "\n");
  process.stdout.write(`${JSON.stringify(provenance.facts, null, 2)}\nCaptures written to ${output}\n`);
} finally {
  await browser?.close();
  await server?.close();
  await rm(temporary, { recursive: true, force: true });
}
