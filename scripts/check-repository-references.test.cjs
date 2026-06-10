#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  isExternalReference,
  markdownSlug,
  splitReference,
} = require("./check-repository-references.cjs");

test("external documentation references are not resolved as repository paths", () => {
  assert.equal(isExternalReference("https://example.com/guide.md"), true);
  assert.equal(isExternalReference("mailto:security@example.com"), true);
  assert.equal(isExternalReference("../README.md"), false);
});

test("references split local targets from anchors and queries", () => {
  assert.deepEqual(splitReference("guide.html#details"), {
    target: "guide.html",
    anchor: "details",
  });
  assert.deepEqual(splitReference("guide.html?mode=print"), {
    target: "guide.html",
    anchor: "",
  });
});

test("Markdown headings use deterministic repository anchors", () => {
  assert.equal(markdownSlug("Public Markdown policy"), "public-markdown-policy");
  assert.equal(markdownSlug("OCR & PDFium"), "ocr-pdfium");
});
