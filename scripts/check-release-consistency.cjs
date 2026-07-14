#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const {
  ROOT,
  alphaLabel,
  getReleaseMetadata,
  syncReleaseMetadata,
  versionPattern,
} = require("./sync-release-metadata.cjs");

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(ROOT, relativePath), "utf-8"));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(ROOT, relativePath), "utf-8");
}

function extractCargoVersion(relativePath) {
  const match = readText(relativePath).match(/^version\s*=\s*"([^"]+)"/m);
  return match ? match[1] : null;
}

function extractDesktopLockVersion() {
  const cargoLock = readText("Cargo.lock");
  const packageBlocks = cargoLock.split(/\[\[package\]\]/);
  for (const block of packageBlocks) {
    const nameMatch = block.match(/^\s*name\s*=\s*"([^"]+)"/m);
    if (nameMatch && nameMatch[1] === "corpuswright-desktop") {
      const versionMatch = block.match(/^\s*version\s*=\s*"([^"]+)"/m);
      return versionMatch ? versionMatch[1] : null;
    }
  }
  return null;
}

function extractAboutVersion() {
  const match = readText("apps/desktop/index.html").match(
    /<strong>Version:<\/strong>\s*(\d+\.\d+\.\d+-alpha\.\d+)/
  );
  return match ? match[1] : null;
}

function reportSource(label, actual, expected) {
  if (actual === expected) {
    console.log(`  OK  ${label}  =>  "${actual}"`);
    return true;
  }

  if (actual === null || actual === undefined) {
    console.error(`MISMATCH: ${label} - could not extract version`);
  } else {
    console.error(
      `MISMATCH: ${label} has version "${actual}", expected "${expected}"`
    );
  }
  return false;
}

function checkNoLiveVersionCopy(metadata) {
  const scannedFiles = listLiveDocumentation();
  const liveVersionPattern = /\bv?\d+\.\d+\.\d+-alpha\.\d+\b|\balpha\.\d+\b/gi;
  let ok = true;

  for (const relativePath of scannedFiles) {
    const text = stripGeneratedCitationCopy(relativePath, readText(relativePath));
    const matches = [...text.matchAll(liveVersionPattern)];
    if (matches.length === 0) {
      console.log(`  OK  ${relativePath} has no hard-coded live alpha version`);
      continue;
    }

    ok = false;
    for (const match of matches) {
      const before = text.slice(0, match.index);
      const line = before.split(/\r?\n/).length;
      console.error(
        `MISMATCH: ${relativePath}:${line} hard-codes "${match[0]}"; use release-metadata sync or agnostic copy`
      );
    }
  }

  if (!ok) {
    console.error(
      `\nLive docs should not hard-code "${metadata.version}" or "${alphaLabel(
        metadata
      )}" outside release-history pages.`
    );
  }

  return ok;
}

function listLiveDocumentation() {
  const files = ["README.md", ".github/workflows/release.yml"];

  function visit(relativeDirectory) {
    const absoluteDirectory = path.join(ROOT, relativeDirectory);
    for (const entry of fs.readdirSync(absoluteDirectory, { withFileTypes: true })) {
      const relativePath = path.posix.join(relativeDirectory, entry.name);
      if (entry.isDirectory()) {
        if (relativePath === "docs/releases" || relativePath === "docs/archive") {
          continue;
        }
        visit(relativePath);
        continue;
      }
      if (!entry.isFile() || !/\.(?:html|md)$/i.test(entry.name)) {
        continue;
      }
      files.push(relativePath);
    }
  }

  visit("docs");
  return files.sort();
}

function checkReleaseNotes(metadata) {
  const relativePath = metadata.releaseNotes;
  const absolutePath = path.join(ROOT, relativePath);
  if (!fs.existsSync(absolutePath) || !fs.statSync(absolutePath).isFile()) {
    console.error(`MISMATCH: configured release notes do not exist: ${relativePath}`);
    return false;
  }

  const content = readText(relativePath);
  const heading = content.match(/^#\s+.*$/m)?.[0] || null;
  const expectedHeading = `# CorpusWright v${metadata.version}`;
  if (!heading || heading.toLowerCase() !== expectedHeading.toLowerCase()) {
    console.error(
      `MISMATCH: ${relativePath} heading is ${JSON.stringify(heading)}, expected "${expectedHeading}"`
    );
    return false;
  }

  console.log(`  OK  release notes  =>  ${relativePath}`);
  return true;
}

function stripGeneratedCitationCopy(relativePath, text) {
  let next = text;
  next = next.replace(
    /\*\*Current `CITATION\.cff`:\*\*\r?\n```yaml\r?\n[\s\S]*?\r?\n```/g,
    ""
  );
  next = next.replace(
    /<pre class="code" id="home-citation-cff"><code class="mono">[\s\S]*?<\/code><\/pre>/g,
    ""
  );
  next = next.replace(
    /<pre class="code" id="citation-(?:bibtex|cff)"><code class="mono">[\s\S]*?<\/code><\/pre>/g,
    ""
  );

  if (relativePath === "docs/citation.html") {
    next = next.replace(/<p><strong>Lopes, J\. H\.[\s\S]*?<\/strong><\/p>/g, "");
  }

  return next;
}

let metadata;
try {
  metadata = getReleaseMetadata();
} catch (error) {
  console.error(`FATAL: invalid release-metadata.json: ${error.message}`);
  process.exit(1);
}
const expected = metadata.version;

if (!versionPattern().test(expected)) {
  console.error(
    `FATAL: release-metadata.json version "${expected}" does not match X.Y.Z-alpha.N.`
  );
  process.exit(1);
}

let allOk = true;

const tauriConf = readJson("apps/desktop/src-tauri/tauri.conf.json");
const packageJson = readJson("apps/desktop/package.json");
const packageLock = readJson("apps/desktop/package-lock.json");

allOk =
  reportSource("release-metadata.json", expected, expected) && allOk;
allOk =
  reportSource("apps/desktop/src-tauri/tauri.conf.json", tauriConf.version, expected) &&
  allOk;
allOk =
  reportSource(
    "apps/desktop/src-tauri/Cargo.toml",
    extractCargoVersion("apps/desktop/src-tauri/Cargo.toml"),
    expected
  ) && allOk;
allOk =
  reportSource("apps/desktop/package.json", packageJson.version, expected) &&
  allOk;
allOk =
  reportSource("apps/desktop/package-lock.json", packageLock.version, expected) &&
  allOk;
allOk =
  reportSource(
    'apps/desktop/package-lock.json packages[""]',
    packageLock.packages && packageLock.packages[""]
      ? packageLock.packages[""].version
      : null,
    expected
  ) && allOk;
allOk = reportSource("Cargo.lock", extractDesktopLockVersion(), expected) && allOk;
allOk =
  reportSource("apps/desktop/index.html", extractAboutVersion(), expected) &&
  allOk;

const staleFiles = syncReleaseMetadata({ check: true, quiet: true });
if (staleFiles.length > 0) {
  allOk = false;
  console.error("\nGenerated release metadata outputs are stale:");
  for (const relativePath of staleFiles) {
    console.error(`  - ${relativePath}`);
  }
  console.error("\nRun: node scripts/sync-release-metadata.cjs");
}

allOk = checkNoLiveVersionCopy(metadata) && allOk;
allOk = checkReleaseNotes(metadata) && allOk;

if (!allOk) {
  console.error("\nRelease consistency check FAILED.");
  process.exit(1);
}

console.log("\nAll release metadata references are consistent.");
