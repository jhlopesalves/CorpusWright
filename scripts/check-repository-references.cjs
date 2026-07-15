#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const DOCS_ROOT = path.join(ROOT, "docs");
const errors = [];

function relativeToRoot(absolutePath) {
  return path.relative(ROOT, absolutePath).split(path.sep).join("/");
}

function lineNumber(text, index) {
  return text.slice(0, index).split(/\r?\n/).length;
}

function report(sourcePath, text, index, message) {
  errors.push(`${relativeToRoot(sourcePath)}:${lineNumber(text, index)} ${message}`);
}

function isExternalReference(reference) {
  return (
    /^[a-z][a-z\d+.-]*:/i.test(reference) ||
    reference.startsWith("//")
  );
}

function splitReference(reference) {
  const withoutQuery = reference.split("?", 1)[0];
  const hashIndex = withoutQuery.indexOf("#");
  if (hashIndex === -1) {
    return { target: withoutQuery, anchor: "" };
  }
  return {
    target: withoutQuery.slice(0, hashIndex),
    anchor: withoutQuery.slice(hashIndex + 1),
  };
}

function decodeReference(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function resolveReference(sourcePath, target) {
  const decoded = decodeReference(target);
  if (!decoded) {
    return sourcePath;
  }
  if (decoded.startsWith("/CorpusWright/")) {
    return path.join(DOCS_ROOT, decoded.slice("/CorpusWright/".length));
  }
  if (decoded.startsWith("/")) {
    return path.join(ROOT, decoded.slice(1));
  }
  return path.resolve(path.dirname(sourcePath), decoded);
}

function markdownSlug(heading) {
  return heading
    .replace(/<[^>]*>/g, "")
    .replace(/[`*_~]/g, "")
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .replace(/\s+/g, "-");
}

function anchorsFor(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  const anchors = new Set();
  if (/\.md$/i.test(filePath)) {
    const counts = new Map();
    for (const match of text.matchAll(/^#{1,6}\s+(.+?)\s*#*$/gm)) {
      const base = markdownSlug(match[1]);
      const count = counts.get(base) || 0;
      counts.set(base, count + 1);
      anchors.add(count === 0 ? base : `${base}-${count}`);
    }
  } else if (/\.html?$/i.test(filePath)) {
    for (const match of text.matchAll(/\b(?:id|name)\s*=\s*["']([^"']+)["']/gi)) {
      anchors.add(match[1]);
    }
  }
  return anchors;
}

function validateReference(sourcePath, sourceText, index, rawReference) {
  const reference = rawReference.trim().replace(/^<|>$/g, "");
  if (!reference || isExternalReference(reference)) {
    return;
  }

  const { target, anchor } = splitReference(reference);
  const targetPath = resolveReference(sourcePath, target);
  if (!fs.existsSync(targetPath)) {
    report(sourcePath, sourceText, index, `references missing local target "${reference}"`);
    return;
  }

  if (anchor && fs.statSync(targetPath).isFile() && /\.(?:md|html?)$/i.test(targetPath)) {
    const decodedAnchor = decodeReference(anchor);
    if (!anchorsFor(targetPath).has(decodedAnchor)) {
      report(
        sourcePath,
        sourceText,
        index,
        `references missing anchor "#${decodedAnchor}" in ${relativeToRoot(targetPath)}`,
      );
    }
  }
}

function liveDocumentationFiles() {
  const files = [path.join(ROOT, "README.md")];

  function visit(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const absolutePath = path.join(directory, entry.name);
      const relativePath = relativeToRoot(absolutePath);
      if (entry.isDirectory()) {
        if (relativePath === "docs/archive" || relativePath === "docs/releases") {
          continue;
        }
        visit(absolutePath);
      } else if (entry.isFile() && /\.(?:md|html)$/i.test(entry.name)) {
        files.push(absolutePath);
      }
    }
  }

  visit(DOCS_ROOT);
  return files.sort();
}

function validateMarkdown(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  for (const match of text.matchAll(/!?\[[^\]]*\]\(([^)]+)\)/g)) {
    let destination = match[1].trim();
    if (destination.startsWith("<")) {
      destination = destination.slice(1, destination.indexOf(">"));
    } else {
      destination = destination.split(/\s+["']/)[0];
    }
    validateReference(filePath, text, match.index, destination);
  }
  for (const match of text.matchAll(/^\s*\[[^\]]+\]:\s*(\S+)/gm)) {
    validateReference(filePath, text, match.index, match[1]);
  }
}

function validateHtml(filePath) {
  const text = fs.readFileSync(filePath, "utf8");
  for (const match of text.matchAll(/\b(?:href|src)\s*=\s*(["'])(.*?)\1/gi)) {
    validateReference(filePath, text, match.index, match[2]);
  }
}

function main() {
  for (const filePath of liveDocumentationFiles()) {
    if (/\.md$/i.test(filePath)) {
      validateMarkdown(filePath);
    } else {
      validateHtml(filePath);
    }
  }
  if (errors.length > 0) {
    console.error("Repository reference check failed:");
    for (const error of errors) {
      console.error(`  - ${error}`);
    }
    process.exit(1);
  }

  console.log("Repository-local documentation links, anchors, and paths are valid.");
}

if (require.main === module) {
  main();
}

module.exports = {
  isExternalReference,
  markdownSlug,
  splitReference,
};
