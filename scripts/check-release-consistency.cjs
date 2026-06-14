#!/usr/bin/env node
/**
 * check-release-consistency.cjs
 *
 * Verifies that the app version is consistent across all files that
 * reference it.  Exits with code 0 if everything is aligned, or code 1
 * with a clear list of mismatches.
 *
 * Usage:
 *   node scripts/check-release-consistency.cjs
 *
 * Can also be invoked from a GitHub Actions step with:
 *   node ${{ github.workspace }}/scripts/check-release-consistency.cjs
 */

"use strict";

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function readJson(...parts) {
  const p = path.join(ROOT, ...parts);
  return JSON.parse(fs.readFileSync(p, "utf-8"));
}

function readFile(...parts) {
  return fs.readFileSync(path.join(ROOT, ...parts), "utf-8");
}

function semverAlphaPattern() {
  // Strict alpha semver: X.Y.Z-alpha.N
  return /^\d+\.\d+\.\d+-alpha\.\d+$/;
}

/* ------------------------------------------------------------------ */
/*  Collect version from each source                                   */
/* ------------------------------------------------------------------ */

const sources = [];

// 1. tauri.conf.json  (canonical source)
const tauriConf = readJson("apps", "desktop", "src-tauri", "tauri.conf.json");
sources.push({
  file: "apps/desktop/src-tauri/tauri.conf.json",
  version: tauriConf.version,
  canonical: true,
});

// 2. Cargo.toml
const cargoToml = readFile("apps", "desktop", "src-tauri", "Cargo.toml");
const cargoVersion = cargoToml.match(/^version\s*=\s*"(.*?)"/m);
sources.push({
  file: "apps/desktop/src-tauri/Cargo.toml",
  version: cargoVersion ? cargoVersion[1] : null,
});

// 3. package.json
const pkgJson = readJson("apps", "desktop", "package.json");
sources.push({
  file: "apps/desktop/package.json",
  version: pkgJson.version,
});

// 4. package-lock.json
const pkgLock = readJson("apps", "desktop", "package-lock.json");
sources.push({
  file: "apps/desktop/package-lock.json",
  version: pkgLock.version,
});

// 5. Cargo.lock entry for corpuswright-desktop
const cargoLock = readFile("Cargo.lock");
const lockMatch = cargoLock.match(
  /\[\[package\]\]\s+name\s*=\s*"corpuswright-desktop"\s+version\s*=\s*"([^"]+)"/m
);
sources.push({
  file: "Cargo.lock",
  version: lockMatch ? lockMatch[1] : null,
});

// 6. README.md alpha download link
const readme = readFile("README.md");
const readmeMatch = readme.match(
  /CorpusWright v(\d+\.\d+\.\d+-alpha\.\d+)/
);
sources.push({
  file: "README.md",
  version: readmeMatch ? readmeMatch[1] : null,
});

// 7. index.html About dialog version
const indexHtml = readFile("apps", "desktop", "index.html");
const htmlMatch = indexHtml.match(
  /<strong>Version:<\/strong>\s*(\d+\.\d+\.\d+-alpha\.\d+)/
);
sources.push({
  file: "apps/desktop/index.html",
  version: htmlMatch ? htmlMatch[1] : null,
});

/* ------------------------------------------------------------------ */
/*  Validate                                                           */
/* ------------------------------------------------------------------ */

const canonical = sources.find((s) => s.canonical);
const expected = canonical.version;

if (!expected) {
  console.error(
    "FATAL: Could not read version from tauri.conf.json (canonical source)."
  );
  process.exit(1);
}

if (!semverAlphaPattern().test(expected)) {
  console.error(
    `FATAL: Canonical version "${expected}" does not match strict alpha semver ` +
      `(required: X.Y.Z-alpha.N).`
  );
  process.exit(1);
}

let allOk = true;

for (const source of sources) {
  const label = source.file;
  const actual = source.version;

  if (actual === null) {
    console.error(`MISMATCH: ${label} — could not extract version`);
    allOk = false;
    continue;
  }

  if (actual !== expected) {
    console.error(
      `MISMATCH: ${label} has version "${actual}", expected "${expected}"`
    );
    allOk = false;
  } else {
    console.log(`  OK  ${label}  =>  "${actual}"`);
  }
}

if (!allOk) {
  console.error("\nVersion consistency check FAILED.");
  process.exit(1);
}

console.log("\nAll version references are consistent.");
