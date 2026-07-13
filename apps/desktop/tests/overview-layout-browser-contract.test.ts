import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const fixture = readFileSync(new URL("./visual/overview-layout-fixture.ts", import.meta.url), "utf8");
const html = readFileSync(new URL("./visual/overview-layout-fixture.html", import.meta.url), "utf8");

assert.match(html, /href="\/src\/styles\.css"/);
assert.match(fixture, /from "\.\.\/\.\.\/src\/plots\/model"/);
assert.match(fixture, /from "\.\.\/\.\.\/src\/plots\/render"/);
assert.match(fixture, /renderDispersionSvg/);
assert.match(fixture, /renderLexicalSpectrumSvg/);
assert.match(fixture, /renderDispersionPlaneSvg/);
assert.match(fixture, /getBoundingClientRect\(\)/);
assert.match(fixture, /profile callout overlaps strip-label rail/);
assert.match(fixture, /mandatory text overlap/);
assert.match(fixture, /text outside viewBox/);
assert.match(fixture, /211_271/);
assert.match(fixture, /intentionally long category/);
assert.match(fixture, /profileCounts = \[[^\]]*, 0\]/s);
assert.match(fixture, /dataset\.visualStatus = "passed"/);
assert.doesNotMatch(fixture, /mock|snapshot|substitute/i);
