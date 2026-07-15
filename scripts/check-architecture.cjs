#!/usr/bin/env node

const { execFileSync, spawnSync } = require("node:child_process");
const { readFileSync } = require("node:fs");
const { extname } = require("node:path");

const root = execFileSync("git", ["rev-parse", "--show-toplevel"], {
  encoding: "utf8",
}).trim();
const tracked = execFileSync("git", ["ls-files"], {
  cwd: root,
  encoding: "utf8",
})
  .trim()
  .split("\n")
  .filter(Boolean);
const failures = [];

function text(path) {
  return readFileSync(`${root}/${path}`, "utf8");
}

function manifestSection(manifest, name) {
  const match = manifest.match(new RegExp(`^\\[${name}\\]\\n([\\s\\S]*?)(?=^\\[|$)`, "m"));
  return match?.[1] ?? "";
}

for (const path of tracked) {
  if (path.startsWith("crates/corpuswright-core/") && extname(path) === ".rs") {
    if (/\btauri(?:::|\s*=)/.test(text(path))) {
      failures.push(`${path}: product core must not import or depend on Tauri`);
    }
  }

  if (path.startsWith("docs/") || path === "README.md") {
    if (/\.maintainer\//.test(text(path))) {
      failures.push(`${path}: public documentation must not reference .maintainer/`);
    }
  }

  if (path.endsWith("Cargo.toml") && !path.includes("corpuswright-test-support")) {
    const manifest = text(path);
    const runtimeDependencies = manifestSection(manifest, "dependencies");
    const xtaskMayGenerateFixtures = path === "crates/corpuswright-xtask/Cargo.toml";
    if (/corpuswright-test-support\s*=/.test(runtimeDependencies) && !xtaskMayGenerateFixtures) {
      failures.push(`${path}: test-support may only be a development dependency`);
    }
    if (/corpuswright-xtask\s*=/.test(manifest)) {
      failures.push(`${path}: product crates must not depend on xtask`);
    }
  }

  if (path.startsWith("apps/desktop/src/") && path.endsWith(".ts")) {
    const source = text(path);
    const importsTauri = /from\s+["']@tauri-apps\//.test(source);
    const approved =
      path.endsWith("/ipc.ts") ||
      path.endsWith("-ipc.ts") ||
      path === "apps/desktop/src/app/system-ipc.ts";
    if (importsTauri && !approved) {
      failures.push(`${path}: direct Tauri imports belong in an approved IPC adapter`);
    }
  }

  if (path.endsWith(".rs") && path !== "crates/corpuswright-core/src/lib.rs") {
    const source = text(path);
    const aliases = [
      "annotation_cache", "scan", "clean", "repeated_artifact_promotion",
      "project_sources", "pdf_audit", "pdf_ocr", "pdf_quality", "repeated_artifacts",
      "tokenization",
    ];
    for (const alias of aliases) {
      if (source.includes(`corpuswright_core::${alias}::`)) {
        failures.push(`${path}: internal use of removed compatibility alias ${alias}`);
      }
    }
  }
}

const desktopManifest = text("apps/desktop/src-tauri/Cargo.toml");
if (/corpuswright-test-support\s*=/.test(manifestSection(desktopManifest, "dependencies"))) {
  failures.push("desktop runtime must not depend on corpuswright-test-support");
}

const comparisonBase = ["main", "origin/main"].find((candidate) =>
  spawnSync("git", ["rev-parse", "--verify", "--quiet", candidate], {
    cwd: root,
    stdio: "ignore",
  }).status === 0,
);
if (!comparisonBase) {
  failures.push(
    "legacy/pyside policy requires a complete checkout containing main or origin/main",
  );
} else {
  const allowedLegacyRemovals = new Set([
    "legacy/pyside/rust_preview/README.md",
    "legacy/pyside/src/assets/printscreen.png",
    "legacy/pyside/tests/README.md",
  ]);
  const legacyChanges = execFileSync(
    "git",
    ["diff", "--name-status", `${comparisonBase}...HEAD`, "--", "legacy/pyside"],
    { cwd: root, encoding: "utf8" },
  )
    .trim()
    .split("\n")
    .filter(Boolean);
  const unexpectedLegacyChanges = legacyChanges.filter((line) => {
    const [status, changedPath] = line.split("\t");
    return status !== "D" || !allowedLegacyRemovals.has(changedPath);
  });
  if (unexpectedLegacyChanges.length > 0) {
    failures.push(
      `legacy/pyside source must remain untouched; only the approved private screenshot and Markdown removals are allowed:\n${unexpectedLegacyChanges.join("\n")}`,
    );
  }
}

if (failures.length > 0) {
  console.error("Architecture policy failed:\n" + failures.map((item) => `- ${item}`).join("\n"));
  process.exit(1);
}

console.log(`Architecture policy passed across ${tracked.length} tracked paths.`);
