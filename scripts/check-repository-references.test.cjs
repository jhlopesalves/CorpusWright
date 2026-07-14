#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  isOptionalTestingGuidePath,
  normaliseRepositoryPath,
} = require("./check-repository-references.cjs");

test("clean checkout may omit generated frontend dependencies", () => {
  assert.equal(isOptionalTestingGuidePath("apps/desktop/node_modules"), true);
  assert.equal(isOptionalTestingGuidePath("apps/desktop/node_modules/"), true);
});

test("testing-guide path normalisation removes only trailing separators", () => {
  assert.equal(normaliseRepositoryPath("apps/desktop/node_modules/"), "apps/desktop/node_modules");
  assert.equal(normaliseRepositoryPath("apps/desktop/src"), "apps/desktop/src");
});

test("genuine repository paths remain required", () => {
  assert.equal(isOptionalTestingGuidePath("apps/desktop/src"), false);
  assert.equal(isOptionalTestingGuidePath("crates/corpuswright-core"), false);
  assert.equal(isOptionalTestingGuidePath("arbitrary/missing/path"), false);
});
