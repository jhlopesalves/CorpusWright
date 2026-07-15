#!/usr/bin/env node
"use strict";

const { execFileSync } = require("node:child_process");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");

const MARKDOWN_ALLOWLIST = new Set([
  "README.md",
  "SECURITY.md",
  "THIRD_PARTY_NOTICES.md",
  "apps/desktop/src-tauri/resources/ocr/README.md",
  "apps/desktop/src-tauri/resources/udpipe/README.md",
  "apps/desktop/src/assets/fonts/README.md",
]);

const AGENT_INSTRUCTION_NAMES = new Set([
  "agent.md",
  "agents.md",
  "claude.md",
  "codex.md",
  "copilot-instructions.md",
  "gemini.md",
  "prompt.md",
  "prompts.md",
]);

const PROHIBITED_PREFIXES = [
  "examples/corpora/",
  "docs/assets/figure-src/",
  "site/src/public/assets/figure-src/",
  "crates/corpusaid-core/tests/fixtures/docx/",
  "crates/corpusaid-core/tests/fixtures/html/",
  "crates/corpusaid-core/tests/fixtures/pdfs/",
  "crates/corpuswright-core/tests/fixtures/docx/",
  "crates/corpuswright-core/tests/fixtures/html/",
  "crates/corpuswright-core/tests/fixtures/pdfs/",
];

const PROHIBITED_PATHS = new Set([
  "CorpusWright",
  "apps/desktop/public/ico.svg",
  "apps/desktop/public/icon.ico.png",
  "apps/desktop/src/assets/ico.svg",
  "crates/corpusaid-core/tests/fixtures/clean.txt",
  "crates/corpusaid-core/tests/fixtures/dirty.txt",
  "crates/corpuswright-core/tests/fixtures/clean.txt",
  "crates/corpuswright-core/tests/fixtures/dirty.txt",
  "docs/assets/screenshots/analyse-workspace-light.png",
  "docs/assets/screenshots/prepare-cleaning-settings-dark.png",
  "docs/assets/screenshots/prepare-cleaning-settings-light.png",
  "docs/assets/screenshots/prepare-dark.png",
  "docs/assets/screenshots/prepare-light.png",
  "docs/assets/screenshots/prepare-repeated-artefacts-dark.png",
  "docs/assets/screenshots/prepare-repeated-artefacts-light.png",
  "force-ocr-stage1-wip.patch",
  "legacy/pyside/src/assets/printscreen.png",
  "icon.ico.png",
]);

const PROHIBITED_ASSET_SHA256 = new Set([
  "061473350b17112ef76088f366820c37888b9cf5c378fda0c7b4e6849d9fb151",
  "64152e75bd418e388e2d20b325cd4926a8614180d795fa8bdfa4d842f556cd9a",
  "653214681b8f61112ea2e3846fec86af7fee18df9ec08c38c6ba1e777a26138e",
]);

const IDE_WORKSPACE_MARKER = [
  [".", "gemini"].join(""),
  ["anti", "gravity-ide"].join(""),
].join("/");
const PLACEHOLDER_EMAIL_TOKEN = ["YOUR", "GITHUB", "EMAIL"].join("_");
const PERSONAL_POSIX_ROOT = ["/home", "jhonatan"].join("/");
const EXACT_PRIVATE_IDE_PATH = [
  "C:/Users",
  "jhonm",
  IDE_WORKSPACE_MARKER,
  "brain",
  "04c58a98-8503-4862-b308-7375810882a5",
].join("/");

function normalisePath(value) {
  return value.split(path.sep).join("/");
}

function isAgentInstructionPath(relativePath) {
  const lower = relativePath.toLowerCase();
  const base = path.posix.basename(lower);
  return (
    AGENT_INSTRUCTION_NAMES.has(base) ||
    /(?:^|\/)(?:agent|codex|claude|gemini)[-_]instructions?\.md$/.test(lower) ||
    /(?:^|\/)(?:dispatch|checkpoint)[-_][^/]*\.md$/.test(lower)
  );
}

function isPublicPortraitPath(relativePath) {
  return (
    /^(?:docs|site\/src\/public)\//.test(relativePath) &&
    /(?:^|\/)(?:maintainer|portrait)[^/]*\.(?:gif|jpe?g|png|svg|webp)$/i.test(relativePath)
  );
}

function isProbablyText(buffer) {
  const sample = buffer.subarray(0, Math.min(buffer.length, 8192));
  return !sample.includes(0);
}

function isProhibitedAssetHash(value) {
  return PROHIBITED_ASSET_SHA256.has(value);
}

function prohibitedTextMatches(text) {
  const matches = [];
  if (text.includes(EXACT_PRIVATE_IDE_PATH)) matches.push("exact private IDE workspace path");
  if (text.includes(IDE_WORKSPACE_MARKER)) matches.push("private IDE workspace marker");
  if (text.includes(PLACEHOLDER_EMAIL_TOKEN)) matches.push("placeholder commit email token");
  if (text.includes(PERSONAL_POSIX_ROOT)) matches.push("personal POSIX home path");
  if (/[A-Za-z]:[\\/]Users[\\/](?:jhonm|jhonn)(?:[\\/]|\b)/i.test(text)) {
    matches.push("personal Windows user path");
  }
  return matches;
}

function markdownTargets(text) {
  const targets = [];
  for (const match of text.matchAll(/!?\[[^\]]*\]\(([^)]+)\)/g)) {
    let target = match[1].trim();
    if (target.startsWith("<")) {
      target = target.slice(1, target.indexOf(">"));
    } else {
      target = target.split(/\s+["']/)[0];
    }
    targets.push(target);
  }
  for (const match of text.matchAll(/^\s*\[[^\]]+\]:\s*(\S+)/gm)) targets.push(match[1]);
  return targets;
}

function htmlTargets(text) {
  return [...text.matchAll(/\b(?:href|src)\s*=\s*(["'])(.*?)\1/gi)].map(
    (match) => match[2],
  );
}

function projectMarkdownPath(sourcePath, rawTarget) {
  const target = rawTarget.trim().replace(/^<|>$/g, "").split(/[?#]/, 1)[0];
  if (!/\.md$/i.test(target)) return null;

  const projectBlobPrefix =
    "https://github.com/jhlopesalves/CorpusWright/blob/main/";
  if (target.startsWith(projectBlobPrefix)) return target.slice(projectBlobPrefix.length);
  if (/^[a-z][a-z\d+.-]*:/i.test(target) || target.startsWith("//")) return null;

  const decoded = (() => {
    try {
      return decodeURIComponent(target);
    } catch {
      return target;
    }
  })();
  if (decoded.startsWith("/CorpusWright/")) return decoded.slice("/CorpusWright/".length);
  if (decoded.startsWith("/")) return decoded.slice(1);
  return normalisePath(path.posix.normalize(path.posix.join(path.posix.dirname(sourcePath), decoded)));
}

function inspectTrackedEntry(relativePath, buffer, trackedPaths) {
  const violations = [];
  const lower = relativePath.toLowerCase();

  if (lower.endsWith(".md") && !MARKDOWN_ALLOWLIST.has(relativePath)) {
    violations.push("tracked Markdown is outside the explicit public allowlist");
  }
  if (isAgentInstructionPath(relativePath)) {
    violations.push("public agent, prompt, dispatch, or checkpoint instruction file is prohibited");
  }
  if (
    PROHIBITED_PATHS.has(relativePath) ||
    PROHIBITED_PREFIXES.some((prefix) => relativePath.startsWith(prefix))
  ) {
    violations.push("path is a prohibited corpus, document, private, or historical artefact");
  }
  if (isPublicPortraitPath(relativePath)) {
    violations.push("maintainer portrait is prohibited in public documentation/site paths");
  }
  const digest = createHash("sha256").update(buffer).digest("hex");
  if (isProhibitedAssetHash(digest)) {
    violations.push("file matches a prohibited metadata-heavy or oversized icon blob");
  }

  if (!isProbablyText(buffer)) return violations;
  const text = buffer.toString("utf8");
  for (const match of prohibitedTextMatches(text)) {
    violations.push(`tracked text contains ${match}`);
  }

  if (/\.(?:html?|md)$/i.test(relativePath)) {
    const targets = lower.endsWith(".md") ? markdownTargets(text) : htmlTargets(text);
    for (const target of targets) {
      const markdownPath = projectMarkdownPath(relativePath, target);
      if (markdownPath && !trackedPaths.has(markdownPath)) {
        violations.push(`documentation link points to removed Markdown: ${target}`);
      }
    }
  }
  return violations;
}

function sha256(filePath) {
  return createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function validateFontManifest(violations, trackedPaths) {
  const manifestPath = path.join(ROOT, "third-party/fonts.json");
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch (error) {
    violations.push(`third-party/fonts.json: unreadable manifest: ${error.message}`);
    return;
  }

  const entries = new Map();
  for (const entry of manifest.files || []) {
    if (!entry || typeof entry.path !== "string" || entries.has(entry.path)) {
      violations.push("third-party/fonts.json: invalid or duplicate file entry");
      continue;
    }
    entries.set(entry.path, entry);
    const absolutePath = path.join(ROOT, entry.path);
    if (!trackedPaths.has(entry.path) || !fs.existsSync(absolutePath)) {
      violations.push(`third-party/fonts.json: missing tracked font ${entry.path}`);
      continue;
    }
    if (sha256(absolutePath) !== entry.sha256) {
      violations.push(`third-party/fonts.json: checksum mismatch for ${entry.path}`);
    }
    if (!manifest.sources?.[entry.source]) {
      violations.push(`third-party/fonts.json: unknown source ${entry.source} for ${entry.path}`);
    }
    if (!trackedPaths.has(entry.licenseFile) || !fs.existsSync(path.join(ROOT, entry.licenseFile))) {
      violations.push(`third-party/fonts.json: missing licence ${entry.licenseFile}`);
    }
  }

  const trackedFonts = [...trackedPaths].filter(
    (relativePath) =>
      /^(?:apps\/desktop\/src\/assets\/fonts|docs\/assets\/fonts)\//.test(relativePath) &&
      /\.(?:ttf|woff2)$/i.test(relativePath),
  );
  for (const relativePath of trackedFonts) {
    if (!entries.has(relativePath)) {
      violations.push(`third-party/fonts.json: unrecorded bundled font ${relativePath}`);
    }
  }
}

function validateOcrManifest(violations, trackedPaths) {
  const manifestPath = path.join(ROOT, "third-party/ocr-models.json");
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch (error) {
    violations.push(`third-party/ocr-models.json: unreadable manifest: ${error.message}`);
    return;
  }

  for (const entry of manifest.files || []) {
    const absolutePath = path.join(ROOT, entry.path || "");
    if (!entry.path || !trackedPaths.has(entry.path) || !fs.existsSync(absolutePath)) {
      violations.push(`third-party/ocr-models.json: missing tracked model ${entry.path}`);
      continue;
    }
    const stat = fs.statSync(absolutePath);
    if (stat.size !== entry.bytes) {
      violations.push(`third-party/ocr-models.json: size mismatch for ${entry.path}`);
    }
    if (sha256(absolutePath) !== entry.sha256) {
      violations.push(`third-party/ocr-models.json: checksum mismatch for ${entry.path}`);
    }
  }
}

function inspectRepositorySnapshot(entries) {
  const trackedPaths = new Set(entries.map(([relativePath]) => relativePath));
  const violations = [];
  for (const [relativePath, value] of entries) {
    const buffer = Buffer.isBuffer(value) ? value : Buffer.from(value);
    for (const message of inspectTrackedEntry(relativePath, buffer, trackedPaths)) {
      violations.push(`${relativePath}: ${message}`);
    }
  }
  return violations;
}

function main() {
  const tracked = execFileSync("git", ["ls-files", "-z"], {
    cwd: ROOT,
    encoding: "utf8",
  })
    .split("\0")
    .filter(Boolean);
  const trackedPaths = new Set(tracked);
  const violations = [];

  for (const relativePath of tracked) {
    const absolutePath = path.join(ROOT, relativePath);
    if (!fs.existsSync(absolutePath)) {
      violations.push(`${relativePath}: tracked path is missing from the working tree`);
      continue;
    }
    for (const message of inspectTrackedEntry(
      relativePath,
      fs.readFileSync(absolutePath),
      trackedPaths,
    )) {
      violations.push(`${relativePath}: ${message}`);
    }
  }

  validateFontManifest(violations, trackedPaths);
  validateOcrManifest(violations, trackedPaths);

  if (violations.length > 0) {
    console.error("Public repository policy failed:");
    for (const violation of violations) console.error(`  - ${violation}`);
    process.exit(1);
  }

  console.log(
    `Public repository policy passed for ${tracked.length} tracked paths and ${MARKDOWN_ALLOWLIST.size} explicitly allowed Markdown files.`,
  );
}

if (require.main === module) main();

module.exports = {
  EXACT_PRIVATE_IDE_PATH,
  IDE_WORKSPACE_MARKER,
  MARKDOWN_ALLOWLIST,
  PERSONAL_POSIX_ROOT,
  PLACEHOLDER_EMAIL_TOKEN,
  PROHIBITED_ASSET_SHA256,
  inspectRepositorySnapshot,
  inspectTrackedEntry,
  isAgentInstructionPath,
  isProhibitedAssetHash,
  isPublicPortraitPath,
  projectMarkdownPath,
  prohibitedTextMatches,
};
