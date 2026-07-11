import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const requiredTokens = [
  "--cw-kwic-sort-1",
  "--cw-kwic-sort-2",
  "--cw-kwic-sort-3",
  "--cw-kwic-pair",
] as const;
const expectedThemes = [
  "corpus-dark",
  "corpus-light",
  "workbench-light",
  "workbench-dark",
  "retro-light",
  "retro-dark",
  "graphite-light",
  "graphite-dark",
  "solar-light",
  "solar-dark",
  "sepia-light",
  "sepia-dark",
  "moss-light",
  "moss-dark",
  "high-contrast-light",
  "high-contrast-dark",
] as const;

const themeBlocks = new Map<string, string>();
const themePattern = /(?:^|\n)\[data-theme="([^"]+)"\]\s*\{([\s\S]*?)\n\}/g;
for (const match of styles.matchAll(themePattern)) {
  themeBlocks.set(match[1]!, match[2]!);
}

assert.deepEqual(
  [...themeBlocks.keys()].sort(),
  [...expectedThemes].sort(),
  "Focused theme coverage must enumerate every standalone data-theme block",
);
for (const theme of expectedThemes) {
  const body = themeBlocks.get(theme)!;
  for (const token of requiredTokens) {
    assert.match(body, new RegExp(`(?:^|\\n)\\s*${token}:`), `${theme} must define ${token}`);
  }
  const sortValues = [1, 2, 3].map((level) => {
    const match = body.match(new RegExp(`--cw-kwic-sort-${level}:\\s*([^;]+);`));
    assert(match, `${theme} must define sort level ${level}`);
    return match[1]!.trim();
  });
  assert.deepEqual(
    sortValues,
    ["var(--cw-collocate-text)", "var(--cw-success-text)", "var(--cw-category-numeric-text)"],
    `${theme} must map sort priority to magenta, green, and amber categorical roles`,
  );
  assert.equal(new Set(sortValues).size, 3, `${theme} sort roles must remain distinct`);
  assert(sortValues.every((value) => !value.includes("plot-ramp")), `${theme} sort roles must not use a sequential ramp`);
}

const componentRulePattern = /(?:^|\n)([^@{}][^{}]*)\{([^{}]*)\}/g;
for (const match of styles.matchAll(componentRulePattern)) {
  const selector = match[1]!;
  if (!selector.includes("concordance")) continue;
  assert.doesNotMatch(
    match[2]!,
    /#[0-9a-f]{3,8}\b/i,
    `Concordance component rule must use theme tokens: ${selector.trim()}`,
  );
}
