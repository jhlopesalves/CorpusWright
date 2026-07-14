#!/usr/bin/env node

const { createHash } = require("node:crypto");
const { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync, copyFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join, resolve } = require("node:path");
const { spawnSync } = require("node:child_process");

const repositoryRoot = resolve(__dirname, "..");
const manifestPath = join(repositoryRoot, "third-party", "pdfium.json");
const trackedWindowsDll = join(
  repositoryRoot,
  "apps",
  "desktop",
  "src-tauri",
  "resources",
  "ocr",
  "pdfium.dll",
);

function fail(message) {
  throw new Error(message);
}

function loadManifest() {
  try {
    return JSON.parse(readFileSync(manifestPath, "utf8"));
  } catch (error) {
    fail(`Cannot read ${manifestPath}: ${error.message}`);
  }
}

function sha256(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

function verifyFile(filePath, expectedHash, expectedSize, label) {
  if (!existsSync(filePath)) {
    fail(`${label} is missing: ${filePath}`);
  }
  const actualSize = statSync(filePath).size;
  if (expectedSize !== undefined && actualSize !== expectedSize) {
    fail(`${label} size mismatch: expected ${expectedSize}, received ${actualSize}`);
  }
  const actualHash = sha256(filePath);
  if (actualHash !== expectedHash) {
    fail(`${label} SHA-256 mismatch: expected ${expectedHash}, received ${actualHash}`);
  }
  console.log(`Verified ${label}: ${actualHash}`);
}

function parseArguments(argv) {
  if (argv.length === 1 && argv[0] === "--verify-tracked-windows") {
    return { verifyTrackedWindows: true };
  }

  const platform = argv[0];
  let outputDirectory;
  for (let index = 1; index < argv.length; index += 1) {
    if (argv[index] === "--output-dir" && argv[index + 1]) {
      outputDirectory = resolve(argv[index + 1]);
      index += 1;
      continue;
    }
    fail(`Unknown or incomplete argument: ${argv[index]}`);
  }
  if (!platform || !outputDirectory) {
    fail(
      "Usage: node scripts/fetch-pdfium.cjs <platform> --output-dir <directory>\n" +
        "   or: node scripts/fetch-pdfium.cjs --verify-tracked-windows",
    );
  }
  return { platform, outputDirectory, verifyTrackedWindows: false };
}

async function download(url, destination) {
  console.log(`Downloading ${url}`);
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) {
    fail(`Download failed with HTTP ${response.status} ${response.statusText}`);
  }
  writeFileSync(destination, Buffer.from(await response.arrayBuffer()));
}

function extractTarGz(archivePath, extractionDirectory) {
  const result = spawnSync(
    "tar",
    ["-xzf", archivePath, "-C", extractionDirectory],
    { encoding: "utf8" },
  );
  if (result.error) {
    fail(`Could not run tar: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`tar extraction failed: ${(result.stderr || result.stdout).trim()}`);
  }
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const manifest = loadManifest();

  if (options.verifyTrackedWindows) {
    const windows = manifest.platforms?.["windows-x64"];
    if (!windows) {
      fail("Manifest does not define windows-x64");
    }
    verifyFile(
      trackedWindowsDll,
      windows.librarySha256,
      windows.librarySize,
      "tracked Windows PDFium DLL",
    );
    return;
  }

  const platform = manifest.platforms?.[options.platform];
  if (!platform) {
    fail(
      `Unknown PDFium platform '${options.platform}'. Available platforms: ${Object.keys(manifest.platforms || {}).join(", ")}`,
    );
  }

  const temporaryDirectory = mkdtempSync(join(tmpdir(), "corpuswright-pdfium-"));
  try {
    const archivePath = join(temporaryDirectory, platform.asset);
    const extractionDirectory = join(temporaryDirectory, "extracted");
    mkdirSync(extractionDirectory);

    await download(platform.url, archivePath);
    verifyFile(
      archivePath,
      platform.archiveSha256,
      undefined,
      `${options.platform} PDFium archive`,
    );

    extractTarGz(archivePath, extractionDirectory);
    const extractedLibrary = join(extractionDirectory, ...platform.libraryPath.split("/"));
    verifyFile(
      extractedLibrary,
      platform.librarySha256,
      platform.librarySize,
      `${options.platform} PDFium library`,
    );

    mkdirSync(options.outputDirectory, { recursive: true });
    const destination = join(options.outputDirectory, platform.resourceFile);
    copyFileSync(extractedLibrary, destination);
    verifyFile(
      destination,
      platform.librarySha256,
      platform.librarySize,
      `installed ${options.platform} PDFium library`,
    );
    console.log(`Installed PDFium ${manifest.pdfiumVersion} at ${destination}`);
  } finally {
    rmSync(temporaryDirectory, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`PDFium retrieval failed: ${error.message}`);
  process.exitCode = 1;
});
