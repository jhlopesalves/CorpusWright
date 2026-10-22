#!/usr/bin/env node
"use strict";

// Static contracts and witness provenance complement, rather than replace, browser inspection.
const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { Script } = require("node:vm");
const { pathToFileURL } = require("node:url");

const ROOT = path.resolve(__dirname, "..");
const DOCS = path.join(ROOT, "docs");
const FIGURES = path.join(DOCS, "assets/figures");
const redirectPages = new Set(["learn.html", "method.html", "method-and-measures.html"]);
const navigation = [
  ["Guide", "quick-start.html"],
  ["Methods", "concepts/overview.html"],
  ["About", "about.html"],
  ["GitHub", "https://github.com/jhlopesalves/CorpusWright"],
  ["Download", "https://github.com/jhlopesalves/CorpusWright/releases/latest"],
];

function htmlFiles(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const absolute = path.join(directory, entry.name);
    return entry.isDirectory() ? htmlFiles(absolute) : entry.name.endsWith(".html") ? [absolute] : [];
  });
}

function sha256(file) {
  return createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function checkCustomProperties(source) {
  // This stylesheet owns its tokens. A misspelt var() silently invalidates a
  // declaration in the browser, so asset/link validation alone cannot catch it.
  const css = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const definitions = new Set([...css.matchAll(/(--[\w-]+)\s*:/g)].map(match => match[1]));
  for (const reference of css.matchAll(/var\(\s*(--[\w-]+)\s*([,)])/g)) {
    assert.ok(definitions.has(reference[1]) || reference[2] === ",", `Undefined CSS custom property: ${reference[1]}`);
  }
}

function checkPage(file) {
  const html = fs.readFileSync(file, "utf8");
  const label = path.relative(ROOT, file);
  assert.doesNotMatch(html, /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/, label + ": unexpected control character");
  assert.match(html, /<!doctype html>/i, `${label}: missing doctype`);
  assert.match(html, /<html lang="en-GB">/, `${label}: missing document language`);
  for (const script of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
    if (/\bsrc=/.test(script[1])) continue;
    if (/\btype="module"/.test(script[1])) {
      execFileSync(process.execPath, ["--check", "--input-type=module"], { input: script[2] });
    } else {
      new Script(script[2], { filename: label });
    }
  }
  assert.match(html, /rel="icon" href="(?:\.\.\/)?assets\/icon-32\.png"/, `${label}: small favicon`);
  if (redirectPages.has(path.basename(file))) {
    assert.match(html, /name="robots" content="noindex, follow"/, `${label}: compatibility redirect indexing`);
    assert.match(html, /content="0; url=concepts\/overview.html"/, `${label}: changed legacy destination`);
    return;
  }

  const nav = html.match(/<nav\b[^>]*aria-label="Primary"[^>]*>([\s\S]*?)<\/nav>/)?.[1];
  assert.ok(nav, `${label}: missing primary navigation`);
  const actual = [...nav.matchAll(/<a\b[^>]*href="([^"]+)"[^>]*>([^<]+)<\/a>/g)].map(match => {
    const href = match[1].startsWith("https:") ? match[1] : path.relative(DOCS, path.resolve(path.dirname(file), match[1]));
    return [match[2].trim(), href];
  });
  assert.deepEqual(actual, navigation, `${label}: inconsistent navigation`);
  assert.match(nav, /<button[^>]*id="theme-toggle"[^>]*type="button"[^>]*aria-pressed=/, `${label}: missing theme control`);
  assert.equal([...html.matchAll(/<main\b/g)].length, 1, `${label}: expected one main landmark`);
  assert.match(html, /<main id="top" tabindex="-1">/, `${label}: missing skip destination`);
  assert.match(html, /<a class="skip-link" href="#top">Skip to content<\/a>/, `${label}: missing skip link`);
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(new Set(ids).size, ids.length, `${label}: duplicate IDs`);
  const levels = [...html.matchAll(/<h([1-6])\b/g)].map(match => Number(match[1]));
  assert.equal(levels.filter(level => level === 1).length, 1, `${label}: expected one h1`);
  for (let index = 1; index < levels.length; index += 1) {
    assert.ok(levels[index] <= levels[index - 1] + 1, `${label}: skipped heading level`);
  }
  for (const image of html.matchAll(/<img\b[^>]*>/g)) {
    assert.match(image[0], /\balt="[^"]*"/, `${label}: image lacks an alt attribute`);
    if (/\balt=""/.test(image[0])) assert.match(image[0], /class="mark"/, `${label}: only brand marks are decorative`);
  }
  const footer = html.match(/<footer\b[^>]*>([\s\S]*?)<\/footer>/)?.[1];
  assert.ok(footer, `${label}: missing footer`);
  assert.match(footer, /href="(?:\.\.\/)?about.html#citation">Citation<\/a>/, `${label}: missing human citation path`);
}

function checkScreenshot(file) {
  const png = fs.readFileSync(file);
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "Invalid PNG signature");
  for (let offset = 8; offset < png.length;) {
    const length = png.readUInt32BE(offset);
    const chunk = png.subarray(offset + 4, offset + 8).toString("ascii");
    // Browser captures need pixels and colour data, never descriptive/EXIF metadata.
    assert.ok(["IHDR", "IDAT", "IEND", "sRGB", "gAMA", "cHRM", "iCCP"].includes(chunk), `${file}: unexpected PNG metadata ${chunk}`);
    assert.ok(offset + length + 12 <= png.length, `${file}: truncated PNG chunk`);
    offset += length + 12;
  }
}

function checkWitnesses() {
  const provenance = JSON.parse(fs.readFileSync(path.join(FIGURES, "provenance.json"), "utf8"));
  assert.equal(provenance.schemaVersion, 1);
  const assets = [...provenance.legacyFigures, ...provenance.canonicalFigures, ...provenance.screenshots];
  for (const witness of assets) {
    const file = path.resolve(FIGURES, witness.asset);
    assert.ok(file.startsWith(path.join(DOCS, "assets") + path.sep), "Witness must stay within public assets");
    assert.equal(sha256(file), witness.sha256, `${witness.asset}: witness changed without provenance`);
    if (file.endsWith(".png")) checkScreenshot(file);
  }
  assert.ok(provenance.canonicalFigures.every(figure => figure.type === "Product figure"));
  for (const removed of provenance.removedFigures) {
    assert.equal(fs.existsSync(path.join(FIGURES, removed.asset)), false, `${removed.asset}: retired figure must not remain public`);
  }
  const demo = provenance.demonstration;
  assert.equal(sha256(path.join(ROOT, demo.generator)), demo.generatorSha256, "Demonstration generator changed");
  for (const [directory, expected] of [["crates/corpuswright-core", demo.coreTree], ["apps/desktop/src/plots", demo.plotTree]]) {
    execFileSync("git", ["diff", "--quiet", "--", directory], { cwd: ROOT });
    const actual = execFileSync("git", ["write-tree", `--prefix=${directory}/`], { cwd: ROOT, encoding: "utf8" }).trim();
    assert.equal(actual, expected, `${directory}: canonical witnesses require review after source changes`);
  }
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "corpuswright-site-demo-check-"));
  try {
    execFileSync(process.execPath, [path.join(ROOT, demo.generator), temporary], { cwd: ROOT });
    assert.deepEqual(fs.readdirSync(temporary).sort(), Object.keys(demo.sourceChecksums).sort(), "Demonstration source set changed");
    for (const [name, expected] of Object.entries(demo.sourceChecksums)) {
      assert.equal(sha256(path.join(temporary, name)), expected, `${name}: demonstration is no longer reproducible`);
    }
    // Exercise the write boundary in a temporary checkout, so a failed guard cannot add corpus files here.
    const checkout = path.join(temporary, "checkout");
    fs.mkdirSync(path.join(checkout, "scripts"), { recursive: true });
    const generatorCopy = path.join(checkout, "scripts", path.basename(demo.generator));
    fs.copyFileSync(path.join(ROOT, demo.generator), generatorCopy);
    const forbiddenOutput = path.join(checkout, "corpus");
    assert.throws(
      () => execFileSync(process.execPath, [generatorCopy, forbiddenOutput], { stdio: "pipe" }),
      error => error.stderr?.toString().includes("outside the repository"),
      "Demonstration generator must reject corpus payloads inside its checkout"
    );
    assert.equal(fs.existsSync(forbiddenOutput), false, "Repository guard must run before creating the output");
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
  return assets.length;
}

function checkProductGallery() {
  const home = fs.readFileSync(path.join(DOCS, "index.html"), "utf8");
  const gallery = home.match(/<section class="product-gallery"[\s\S]*?<\/section>/)?.[0];
  assert.ok(gallery, "Homepage needs a product gallery");
  const stages = ["prepare", "overview", "concordance"];
  assert.deepEqual([...gallery.matchAll(/data-gallery-tab="([^"]+)"/g)].map(match => match[1]), stages);
  assert.deepEqual([...gallery.matchAll(/data-gallery-tab="[^"]+">([^<]+)</g)].map(match => match[1]),
    ["Prepare", "Corpus Overview", "Concordance"]);
  for (const figure of gallery.matchAll(/<figure>([\s\S]*?)<\/figure>/g)) {
    assert.ok(figure[1].indexOf("<figcaption>") < figure[1].indexOf("<img "), "Gallery explanation must precede the screenshot");
  }
  assert.deepEqual([...gallery.matchAll(/<img src="([^"]+)"/g)].map(match => match[1]),
    stages.map(stage => `assets/screenshots/corpuswright-${stage}.png`));
  assert.doesNotMatch(home, /data-plot-|home-carousel|plot-carousel/);
  const script = fs.readFileSync(path.join(DOCS, "assets/product-gallery.js"), "utf8");
  new Script(script, { filename: "product-gallery.js" });
  assert.doesNotMatch(script, /setInterval|setTimeout/, "Product screenshots must remain under manual control");
  const provenance = JSON.parse(fs.readFileSync(path.join(DOCS, "assets/screenshots/provenance.json"), "utf8"));
  assert.equal(provenance.schemaVersion, 1);
  assert.deepEqual(provenance.viewport, { width: 1440, height: 1000, deviceScaleFactor: 1 });
  assert.equal(provenance.theme, "corpus-light");
  assert.equal(provenance.identity, "GUI demonstration corpus");
  assert.equal(provenance.collocationAudit.sortMeasure, "mi", "Public Concordance must show the active default score");
  const association = provenance.collocationAudit;
  const expectedMi = Math.log2(association.witness.events / (association.r1 * association.witness.corpusFrequency / association.n));
  assert.ok(Math.abs(association.witness.mi - expectedMi) < 1e-12, "Capture witness must match the documented MI calculation");
  assert.deepEqual(provenance.captures.map(capture => capture.stage), stages);
  for (const capture of provenance.captures) {
    assert.equal(capture.asset, `corpuswright-${capture.stage}.png`);
    const file = path.join(DOCS, "assets/screenshots", capture.asset);
    assert.equal(sha256(file), capture.sha256, `${capture.asset}: capture changed without provenance`);
    checkScreenshot(file);
    const png = fs.readFileSync(file);
    assert.equal(png.readUInt32BE(16), provenance.viewport.width);
    assert.equal(png.readUInt32BE(20), provenance.viewport.height);
  }
  for (const [file, expected] of Object.entries(provenance.captureInputs)) {
    assert.equal(sha256(path.join(ROOT, file)), expected, `${file}: recapture after capture input changes`);
  }
  for (const [directory, expected] of [["crates/corpuswright-core", provenance.coreTree], ["apps/desktop/src", provenance.frontendTree]]) {
    execFileSync("git", ["diff", "--quiet", "--", directory], { cwd: ROOT });
    assert.equal(execFileSync("git", ["write-tree", `--prefix=${directory}/`], { cwd: ROOT, encoding: "utf8" }).trim(), expected);
  }
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "corpuswright-product-check-"));
  try {
    const generator = pathToFileURL(path.join(ROOT, provenance.generator)).href;
    execFileSync(process.execPath, ["--input-type=module", "-e",
      `import { generateProductCorpus } from ${JSON.stringify(generator)}; generateProductCorpus(${JSON.stringify(temporary)});`]);
    assert.deepEqual(fs.readdirSync(temporary).sort(), Object.keys(provenance.sourceChecksums).sort());
    for (const [name, expected] of Object.entries(provenance.sourceChecksums)) {
      assert.equal(sha256(path.join(temporary, name)), expected, `${name}: product capture corpus changed`);
    }
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
  return provenance.captures.length;
}

function checkWorkedExamples() {
  const html = fs.readFileSync(path.join(DOCS, "concepts/overview.html"), "utf8");
  const provenance = JSON.parse(fs.readFileSync(path.join(FIGURES, "provenance.json"), "utf8"));
  const tables = new Map([...html.matchAll(/<figure class="worked-example" id="([^"]+)">([\s\S]*?)<\/figure>/g)]
    .map(match => [match[1], [...match[2].matchAll(/<tr>([\s\S]*?)<\/tr>/g)].slice(1)
      .map(row => [...row[1].matchAll(/<t[dh][^>]*>([^<]+)<\/t[dh]>/g)].map(cell => cell[1]))]));
  assert.equal(tables.size, 5, "All quantitative teaching examples must retain their inputs");
  const near = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-10, `${actual} != ${expected}`);
  for (const example of provenance.workedExamples) {
    assert.equal(example.type, "Concept schematic/worked example");
    const rows = tables.get(example.id);
    assert.ok(rows, `${example.id}: worked example missing`);
    const { inputs, results } = example;
    if (example.id === "example-frequency") {
      inputs.counts.forEach((count, index) => {
        const rate = count / inputs.tokens[index] * inputs.base;
        near(rate, results[index]);
        assert.deepEqual(rows[index], [count, inputs.tokens[index], rate].map(value => value.toLocaleString("en-GB")));
      });
    } else if (example.id === "example-dispersion") {
      inputs.counts.forEach((counts, index) => {
        const total = counts.reduce((sum, value) => sum + value, 0);
        near(counts.reduce((sum, value, document) => sum + Math.abs(value / total - inputs.tokenShares[document]), 0) / 2, results[index]);
      });
      assert.deepEqual(rows, inputs.tokenShares.map((share, index) => [String(index + 1), String(share), ...inputs.counts.map(counts => String(counts[index]))]));
    } else if (example.id === "example-entropy") {
      inputs.probabilities.forEach((probabilities, index) => {
        near(probabilities.reduce((sum, value) => sum + value, 0), 1);
        const entropy = -probabilities.reduce((sum, value) => sum + value * Math.log2(value), 0);
        const normalisedEntropy = entropy / Math.log2(probabilities.length);
        near(entropy, results.entropyBits[index]); near(normalisedEntropy, results.normalisedEntropy[index]);
        assert.deepEqual(rows[index], ["[" + probabilities.map(value => value.toFixed(2)).join(", ") + "]", entropy.toFixed(5), normalisedEntropy.toFixed(5)]);
      });
    } else if (example.id === "example-effective-documents") {
      inputs.tokenShares.forEach((shares, index) => {
        near(shares.reduce((sum, value) => sum + value, 0), 1);
        near(1 / shares.reduce((sum, value) => sum + value * value, 0), results[index]);
        assert.equal(Number(rows[index][2]), results[index]);
      });
      assert.deepEqual(rows.map(row => row.slice(0, 2)), [["[¼, ¼, ¼, ¼]", "¼"], ["[½, ⅙, ⅙, ⅙]", "⅓"]]);
    } else if (example.id === "example-collocation") {
      const { events, slots, corpusFrequency, nodeHits, tokens } = inputs;
      const score = Math.log2(events / (slots * corpusFrequency / tokens));
      near(score, results.score);
      assert.deepEqual(rows, [["Node hits", String(nodeHits)], ["Window positions W", String(slots)], ["blue events c", String(events)], ["blue corpus frequency F", String(corpusFrequency)], ["Corpus tokens N", String(tokens)], ["MI", score.toFixed(5)]]);
      assert.notEqual(slots, nodeHits, "The expectation uses window positions, not ordinary node frequency");
    } else assert.fail(`Unknown worked example: ${example.id}`);
  }
  assert.doesNotMatch(html, /ttr_curve|Exploration precedes confirmation|Preparation is analysis|saturation gauge|genuinely corpus-wide/);
  assert.match(html, /does not validate tokenisation/);
  assert.match(html, /does not diagnose an unbalanced design/);
  assert.match(html, /neither establishes saturation, representativeness or a vocabulary ceiling/);
  assert.match(html, /does not remove differences in corpus size, sampling/);
  assert.match(html, /No combination of rank-frequency trend, growth exponent, hapax share, entropy and dispersion establishes representativeness/);
  assert.match(html, /href="#ref-sinclair">Sinclair, 2005/);
  assert.match(html, /id="ref-sinclair"[\s\S]*?https:\/\/users\.ox\.ac\.uk\/~martinw\/dlc\/chapter1\.htm/);
  assert.doesNotMatch(html, /\b(?:Biber|Egbert|Sardinha)\b/i, "Methods must respect the excluded-source list");
  assert.match(html, /Document-size concentration/);
  assert.match(html, /Effective document count/);
  assert.match(html, /not an effective sample size for inference/);
  assert.match(html, /data-dependent choice/);
  assert.match(html, /metadata does not make it representative as an independent corpus/);
  assert.match(html, /Research design and analytical flexibility/);
  assert.match(html, /pp\. 1–16/);
  const glossary = fs.readFileSync(path.join(ROOT, "apps/desktop/src/features/analyse/overview/glossary.ts"), "utf8");
  assert.match(glossary, /label: "Effective document count"/);
  assert.match(glossary, /not an inferential effective sample size/);
  assert.match(glossary, /Ranks 1–3 and frequency-one types/);
  assert.doesNotMatch(glossary, /Rank 1 and|removes raw TTR|Raw H rises|far from saturation|so corpora with different vocabulary sizes can be compared/);
  for (const term of ["Collocation: association within a context window", "node-hit count", "Inactive compatibility calculation", "None of these outputs is a p-value", "machine epsilon", "Raw DP values from different partitions", "At low frequency"]) {
    assert.ok(html.toLowerCase().includes(term.toLowerCase()), `Missing methods boundary: ${term}`);
  }
  for (const measure of ["mi", "mi3", "t-score", "z-score", "g2", "log-ratio"]) {
    assert.match(html, new RegExp(`<details class="association-definition" id="association-${measure}">`), `${measure}: selectable score needs a definition`);
  }
  assert.match(html, /id="concordance-distribution"/);
  assert.match(html, /id="ref-rychly"/); assert.match(html, /id="ref-hill"/);
  assert.match(html, /id="document-size-concentration"/);
  assert.doesNotMatch(html, /Normalised entropy \/ evenness|Their distributions make the examples reproducible/);
  for (const file of ["404.html", "roadmap.html", "technical-docs.html"]) {
    assert.match(fs.readFileSync(path.join(DOCS, file), "utf8"), /name="robots" content="noindex, follow"/);
  }
  return tables.size;
}

function checkSocialPreview() {
  const asset = path.join(DOCS, "assets/social-preview.png");
  const provenance = JSON.parse(fs.readFileSync(path.join(DOCS, "assets/social-preview.json"), "utf8"));
  checkScreenshot(asset);
  const pixels = fs.readFileSync(asset);
  assert.deepEqual([pixels.readUInt32BE(16), pixels.readUInt32BE(20)], [1200, 630]);
  assert.equal(sha256(asset), provenance.sha256);
  assert.equal(sha256(path.join(ROOT, provenance.source)), provenance.sourceSha256);
  assert.equal(sha256(path.join(ROOT, provenance.generator)), provenance.generatorSha256);
  const crop = provenance.sourceCrop;
  assert.ok(crop.x >= 0 && crop.y >= 0 && crop.width > 0 && crop.height > 0);
  assert.ok(crop.x + crop.width <= 1440 && crop.y + crop.height <= 1000, "Social crop must remain within the real GUI capture");
  assert.deepEqual(crop, { x: 0, y: 0, width: 1440, height: 1000 }, "Social preview must retain actual application navigation, tool rail and Overview");
  assert.ok(pixels.length < 200_000, "Social preview must stay compact");
  for (const file of ["index.html", "quick-start.html", "concepts/overview.html", "about.html", "citation.html", "technical-docs.html", "roadmap.html", "404.html"]) {
    const html = fs.readFileSync(path.join(DOCS, file), "utf8");
    assert.match(html, /og:image" content="https:\/\/jhlopesalves.github.io\/CorpusWright\/assets\/social-preview.png/);
    assert.match(html, /twitter:card" content="summary_large_image/);
    assert.match(html, /og:image:alt/); assert.match(html, /twitter:image:alt/);
  }
}

const pages = htmlFiles(DOCS);
for (const file of pages) checkPage(file);
const cssPath = path.join(DOCS, "assets/site.css");
const css = fs.readFileSync(cssPath, "utf8");
checkCustomProperties(css);
assert.doesNotMatch(css, /IBM Plex Serif|--serif|hero-split|hero-downloads/);
for (const reference of css.matchAll(/url\(['"]?([^'"\)]+)['"]?\)/g)) {
  assert.ok(fs.existsSync(path.resolve(path.dirname(cssPath), reference[1])), `Missing CSS asset: ${reference[1]}`);
}
const witnesses = checkWitnesses();
const screenshots = checkProductGallery();
const workedExamples = checkWorkedExamples();
checkSocialPreview();
execFileSync(process.execPath, [path.join(ROOT, "scripts/check-repository-references.cjs")], { stdio: "inherit" });
console.log(`Public site contracts passed: ${pages.length} HTML pages, inline JavaScript, CSS asset paths, ${witnesses} audited product figures, ${workedExamples} worked calculations, ${screenshots} product screenshots, social preview and reproducible demonstration bytes.`);
