import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { parseCompleteThemes } from "../scripts/css-contracts.mjs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const desktopRoot = path.resolve("apps/desktop");
const graph = await loadStyleGraph(new URL("../src/styles.css", import.meta.url));
const moduleSource = (relativePath: string): string => {
  const absolutePath = path.join(desktopRoot, "src/styles", relativePath);
  const module = graph.modules.find(candidate => candidate.path === absolutePath);
  assert(module, `${relativePath} must be imported by the style graph`);
  return module.contents;
};

const allCss = graph.modules.map(module => module.contents).join("\n");
const themes = parseCompleteThemes(moduleSource("foundations/themes.css"));
const marks = moduleSource("plots/marks.css");
const interactions = moduleSource("plots/interactions.css");
const feedback = moduleSource("components/feedback.css");
const analyse = moduleSource("features/analyse.css");
const concordance = moduleSource("features/concordance.css");

assert.equal(themes.length, 16, "focus contracts must cover all sixteen themes");
for (const theme of themes) {
  assert.match(theme.declarations.get("--cw-interaction-focus-ring") ?? "", /^0 0 0 [23]px var\(--focus-ring\)$/);
  assert.match(theme.declarations.get("--cw-interaction-focus-outline") ?? "", /^[23]px solid var\(--focus-ring\)$/);
  const expectedThickness = theme.id.startsWith("high-contrast-") ? "3px" : "2px";
  assert.equal(theme.declarations.get("--cw-interaction-focus-outline")?.split(" ")[0], expectedThickness);
}

const focusRingConsumers = [...allCss.matchAll(/([\w-]+)\s*:\s*[^;{}]*var\(--cw-interaction-focus-ring\)[^;{}]*;/g)];
assert(focusRingConsumers.length > 0, "focus shadow contract must retain consumers");
for (const match of focusRingConsumers) {
  assert.equal(match[1], "box-shadow", "focus-ring is a box-shadow-only contract");
}

const focusOutlineConsumers = [...allCss.matchAll(/([\w-]+)\s*:\s*[^;{}]*var\(--cw-interaction-focus-outline\)[^;{}]*;/g)];
assert(focusOutlineConsumers.length >= 3, "the three repaired outline consumers must retain the dedicated contract");
for (const match of focusOutlineConsumers) {
  assert.equal(match[1], "outline", "focus-outline is an outline-only contract");
}

assert.doesNotMatch(allCss, /\bstroke-color\s*:/, "invalid stroke-color declarations are forbidden");
assert.equal(
  [...marks.matchAll(/\.cw-plot-point\.is-excluded\s*\{/g)].length,
  1,
  "excluded points must have one authoritative rule",
);
for (let index = 0; index < 6; index += 1) {
  const rampRule = new RegExp(`\\.cw-plot-ramp-${index}\\s*\\{([^}]*)\\}`).exec(marks)?.[1] ?? "";
  assert.match(rampRule, new RegExp(`color:\\s*var\\(--cw-plot-ramp-${index}\\)`));
  assert.match(rampRule, new RegExp(`fill:\\s*var\\(--cw-plot-ramp-${index}\\)`));
}
assert.doesNotMatch(marks, /\.cw-plot-lollipop-stem\.cw-plot-ramp-/, "generic ramp rules must supply currentColor");
assert.match(marks, /\.cw-plot-lollipop-stem\s*\{[^}]*stroke:\s*currentColor/s);
assert.match(marks, /\.cw-plot-point\s*\{[^}]*stroke:\s*var\(--panel-bg\)/s, "ramp fill must not replace point outlines");

const indefiniteAnimations = [...allCss.matchAll(/animation:\s*([\w-]+)[^;{}]*\binfinite\b/g)].map(match => match[1]).sort();
assert.deepEqual(indefiniteAnimations, [
  "annotation-progress-indeterminate",
  "concordance-collocation-progress",
  "spin",
]);
for (const [source, selector] of [
  [feedback, "\\.spinner"],
  [analyse, "\\.annotation-layer-progress-bar\\.is-indeterminate \\.annotation-layer-progress-fill"],
  [concordance, "\\.concordance-collocation-progress-indicator"],
] as const) {
  assert.match(
    source,
    new RegExp(`@media \\(prefers-reduced-motion: reduce\\)[\\s\\S]*?${selector}\\s*\\{[^}]*animation:\\s*none`),
    `${selector} must stop under reduced motion`,
  );
}
assert.match(analyse, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.annotation-layer-progress-bar\.is-indeterminate \.annotation-layer-progress-fill\s*\{[^}]*width:\s*100%[^}]*transform:\s*none/s);
assert.match(concordance, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.concordance-collocation-progress-indicator\s*\{[^}]*width:\s*100%[^}]*transform:\s*none/s);
for (const selector of ["cw-plot-point", "cw-plot-plane-witness"]) {
  assert.match(interactions, new RegExp(`@media \\(prefers-reduced-motion: reduce\\)[\\s\\S]*?\\.${selector}`));
}
assert.match(interactions, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?opacity:\s*1[^}]*transform:\s*none/s);

const modelSource = readFileSync(new URL("../src/plots/model.ts", import.meta.url), "utf8");
const renderSource = readFileSync(new URL("../src/plots/render.ts", import.meta.url), "utf8");
assert.match(modelSource, /className: `cw-plot-lollipop-stem cw-plot-ramp-\$\{point\.rampIndex\} \$\{point\.className/);
assert.match(modelSource, /function densityRampFill\(t: number\): string/);
assert.match(renderSource, /`cw-plot-ramp-\$\{point\.rampIndex\}`/);
assert.match(renderSource, /baseClass\.includes\("is-lollipop"\) \? 0\.8 : 0/);
assert.match(renderSource, /r: 4\.3 \+ sizeAdjustment/);
