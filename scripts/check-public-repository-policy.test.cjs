#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  EXACT_PRIVATE_IDE_PATH,
  PERSONAL_POSIX_ROOT,
  PLACEHOLDER_EMAIL_TOKEN,
  PROHIBITED_ASSET_SHA256,
  inspectRepositorySnapshot,
  isAgentInstructionPath,
  isProhibitedAssetHash,
  isPublicPortraitPath,
  projectMarkdownPath,
  prohibitedTextMatches,
} = require("./check-public-repository-policy.cjs");

test("Markdown requires an explicit approved or provenance path", () => {
  assert.deepEqual(inspectRepositorySnapshot([["docs/new-guide.md", "# Draft"]]), [
    "docs/new-guide.md: tracked Markdown is outside the explicit public allowlist",
  ]);
  assert.deepEqual(
    inspectRepositorySnapshot([["apps/desktop/src-tauri/resources/ocr/README.md", "# Facts"]]),
    [],
  );
});

test("agent and prompt instruction filenames are rejected", () => {
  assert.equal(isAgentInstructionPath("AGENTS.md"), true);
  assert.equal(isAgentInstructionPath(".github/copilot-instructions.md"), true);
  assert.equal(isAgentInstructionPath("notes/dispatch-cleanup.md"), true);
});

test("personal workspace markers are rejected from tracked text", () => {
  assert.deepEqual(prohibitedTextMatches(EXACT_PRIVATE_IDE_PATH), [
    "exact private IDE workspace path",
    "private IDE workspace marker",
    "personal Windows user path",
  ]);
  assert.deepEqual(prohibitedTextMatches(`${PERSONAL_POSIX_ROOT}/private`), [
    "personal POSIX home path",
  ]);
  assert.deepEqual(prohibitedTextMatches(PLACEHOLDER_EMAIL_TOKEN), [
    "placeholder commit email token",
  ]);
});

test("public maintainer portraits are rejected without banning future synthetic screenshots", () => {
  assert.equal(isPublicPortraitPath("docs/assets/maintainer.jpg"), true);
  assert.equal(isPublicPortraitPath("site/src/public/assets/portrait.webp"), true);
  assert.equal(isPublicPortraitPath("docs/assets/screenshots/synthetic-project.png"), false);
});

test("project links cannot target removed Markdown", () => {
  assert.equal(projectMarkdownPath("README.md", "docs/removed.md"), "docs/removed.md");
  assert.equal(
    projectMarkdownPath(
      "docs/technical-docs.html",
      "https://github.com/jhlopesalves/CorpusWright/blob/main/THIRD_PARTY_NOTICES.md",
    ),
    "THIRD_PARTY_NOTICES.md",
  );
  assert.deepEqual(
    inspectRepositorySnapshot([
      ["README.md", "[removed](docs/removed.md)"],
      ["THIRD_PARTY_NOTICES.md", "# Notices"],
    ]),
    ["README.md: documentation link points to removed Markdown: docs/removed.md"],
  );
});

test("known real-document and metadata-heavy asset paths cannot return", () => {
  const violations = inspectRepositorySnapshot([
    ["docs/assets/screenshots/prepare-dark.png", Buffer.from([0])],
    ["apps/desktop/public/ico.svg", "<svg/>"],
    ["apps/desktop/public/icon.ico.png", Buffer.from([0])],
  ]);
  assert.equal(violations.length, 3);
  assert.match(violations[0], /prohibited corpus, document, private, or historical artefact/);
  assert.match(violations[1], /prohibited corpus, document, private, or historical artefact/);
  assert.match(violations[2], /prohibited corpus, document, private, or historical artefact/);
  assert.equal(PROHIBITED_ASSET_SHA256.size, 3);
  assert.equal(
    isProhibitedAssetHash(
      "64152e75bd418e388e2d20b325cd4926a8614180d795fa8bdfa4d842f556cd9a",
    ),
    true,
  );
  assert.equal(isProhibitedAssetHash("0".repeat(64)), false);
});
