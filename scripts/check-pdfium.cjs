#!/usr/bin/env node

const { createHash } = require("node:crypto");
const { readFileSync, statSync } = require("node:fs");
const { join, resolve } = require("node:path");

const repositoryRoot = resolve(__dirname, "..");
const manifestPath = join(repositoryRoot, "third-party", "pdfium.json");
const requiredPlatforms = [
  "windows-x64",
  "linux-x64",
  "macos-x64",
  "macos-arm64",
];
const workflowPaths = [
  join(repositoryRoot, ".github", "workflows", "ci.yml"),
  join(repositoryRoot, ".github", "workflows", "release.yml"),
];
const hashPattern = /^[0-9a-f]{64}$/;

function fail(message) {
  throw new Error(message);
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be a non-empty string`);
  }
}

function main() {
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch (error) {
    fail(`PDFium manifest is malformed or unreadable: ${error.message}`);
  }

  requireString(manifest.upstreamRepository, "upstreamRepository");
  requireString(manifest.release, "release");
  requireString(manifest.releaseUrl, "releaseUrl");
  requireString(manifest.tagCommit, "tagCommit");
  requireString(manifest.license, "license");
  if (!/^[0-9a-f]{40}$/.test(manifest.tagCommit)) {
    fail("tagCommit must be a full 40-character Git commit ID");
  }
  if (!manifest.platforms || typeof manifest.platforms !== "object") {
    fail("platforms must be an object");
  }

  for (const platformKey of requiredPlatforms) {
    const platform = manifest.platforms[platformKey];
    if (!platform || typeof platform !== "object") {
      fail(`Required PDFium platform is absent: ${platformKey}`);
    }
    for (const field of ["asset", "url", "libraryPath", "resourceFile"]) {
      requireString(platform[field], `${platformKey}.${field}`);
    }
    for (const field of ["archiveSha256", "librarySha256"]) {
      if (!hashPattern.test(platform[field] || "")) {
        fail(`${platformKey}.${field} must be a lowercase SHA-256 digest`);
      }
    }
    if (!Number.isSafeInteger(platform.librarySize) || platform.librarySize <= 0) {
      fail(`${platformKey}.librarySize must be a positive safe integer`);
    }
    const exactReleasePath = `/releases/download/${manifest.release}/`;
    if (!platform.url.includes(exactReleasePath) || !platform.url.endsWith(`/${platform.asset}`)) {
      fail(`${platformKey}.url must identify its asset under exact release ${manifest.release}`);
    }
    if (/releases\/latest\/download/i.test(platform.url)) {
      fail(`${platformKey}.url uses the mutable latest/download endpoint`);
    }
  }

  const workflows = workflowPaths
    .map((workflowPath) => readFileSync(workflowPath, "utf8"))
    .join("\n");
  if (/releases\/latest\/download/i.test(workflows)) {
    fail("A workflow uses the mutable releases/latest/download endpoint");
  }
  if (/bblanchon\/pdfium-binaries|pdfium-(?:linux|mac|win)-[^\s\"']+\.tgz/i.test(workflows)) {
    fail("A workflow bypasses scripts/fetch-pdfium.cjs with a direct PDFium asset reference");
  }
  for (const workflowPath of workflowPaths) {
    const workflow = readFileSync(workflowPath, "utf8");
    if (!workflow.includes("scripts/fetch-pdfium.cjs")) {
      fail(`${workflowPath} does not use scripts/fetch-pdfium.cjs`);
    }
  }
  for (const platformKey of requiredPlatforms) {
    if (!workflows.includes(platformKey)) {
      fail(`Workflow coverage does not mention required platform ${platformKey}`);
    }
  }

  const trackedDll = join(
    repositoryRoot,
    "apps",
    "desktop",
    "src-tauri",
    "resources",
    "ocr",
    "pdfium.dll",
  );
  const trackedHash = createHash("sha256")
    .update(readFileSync(trackedDll))
    .digest("hex");
  const windows = manifest.platforms["windows-x64"];
  if (trackedHash !== windows.librarySha256) {
    fail(
      `Tracked Windows DLL checksum mismatch: expected ${windows.librarySha256}, received ${trackedHash}`,
    );
  }
  const trackedSize = statSync(trackedDll).size;
  if (trackedSize !== windows.librarySize) {
    fail(
      `Tracked Windows DLL size mismatch: expected ${windows.librarySize}, received ${trackedSize}`,
    );
  }

  console.log(
    `PDFium manifest, workflows, and tracked DLL are consistent at ${manifest.release} (${trackedHash}).`,
  );
}

try {
  main();
} catch (error) {
  console.error(`PDFium consistency check failed: ${error.message}`);
  process.exitCode = 1;
}
