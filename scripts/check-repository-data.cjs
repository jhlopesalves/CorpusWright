#!/usr/bin/env node

const { execFileSync } = require("node:child_process");
const { statSync } = require("node:fs");
const { extname } = require("node:path");

const root = execFileSync("git", ["rev-parse", "--show-toplevel"], {
  encoding: "utf8",
}).trim();
const tracked = execFileSync("git", ["ls-files", "-z"], {
  cwd: root,
  encoding: "utf8",
})
  .split("\0")
  .filter(Boolean);

const prohibitedRoots = [
  "examples/corpora/",
  ".local-data/",
  ".local-corpora/",
  "local-corpora/",
  "manual-corpora/",
  "sample-corpora-local/",
  "test-corpora/",
  "documentation-corpus-local/",
];

const testDocumentExtensions = new Set([".txt", ".html", ".htm", ".docx", ".pdf"]);
const likelyDocumentExtensions = new Set([".txt", ".html", ".htm", ".docx", ".pdf"]);
const largeDocumentThreshold = 256 * 1024;

const approvedTestFixturePrefixes = ["apps/desktop/tests/visual/"];
const approvedLargeResourcePrefixes = [
  "apps/desktop/src-tauri/resources/",
  "apps/desktop/src/assets/",
  "apps/desktop/public/",
  "crates/corpuswright-core/resources/",
  "docs/",
  "legacy/",
];
const approvedLargeResourceFiles = new Set([
  "LICENSE",
  "THIRD_PARTY_NOTICES.md",
]);

const violations = [];
for (const path of tracked) {
  const extension = extname(path).toLowerCase();
  const prohibitedRoot = prohibitedRoots.find(
    (prefix) => path === prefix.slice(0, -1) || path.startsWith(prefix),
  );
  if (prohibitedRoot) {
    violations.push(`${path}: tracked file under prohibited local corpus root ${prohibitedRoot}`);
    continue;
  }

  const underTests = path.includes("/tests/") || path.startsWith("tests/");
  const approvedTestFixture = approvedTestFixturePrefixes.some((prefix) => path.startsWith(prefix));
  if (underTests && testDocumentExtensions.has(extension) && !approvedTestFixture) {
    violations.push(`${path}: committed source-document fixture; generate it at runtime`);
    continue;
  }

  if (!likelyDocumentExtensions.has(extension)) continue;
  const size = statSync(`${root}/${path}`).size;
  const approvedResource =
    approvedLargeResourceFiles.has(path) ||
    approvedLargeResourcePrefixes.some((prefix) => path.startsWith(prefix));
  if (size >= largeDocumentThreshold && !approvedResource) {
    violations.push(
      `${path}: ${size} bytes resembles a large source-document payload outside approved software/documentation roots`,
    );
  }
}

if (violations.length > 0) {
  console.error("Repository data policy failed:");
  for (const violation of violations) console.error(`  - ${violation}`);
  process.exit(1);
}

console.log(
  `Repository data policy passed for ${tracked.length} tracked files; no corpus roots, committed document fixtures, or unapproved large document payloads found.`,
);
