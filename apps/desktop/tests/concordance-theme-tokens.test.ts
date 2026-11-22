import { parseCompleteThemes } from "../scripts/css-contracts.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { loadStyleGraph } from "../scripts/style-graph.mjs";

const styles = (await loadStyleGraph(new URL("../src/styles.css", import.meta.url))).css;
const requiredTokens = [
  "--cw-kwic-sort-1",
  "--cw-kwic-sort-2",
  "--cw-kwic-sort-3",
  "--cw-kwic-pair",
] as const;
const expectedThemes = ["wright-light", "wright-dark", "high-contrast-light", "high-contrast-dark"];
const themeSource = readFileSync(new URL("../src/styles/foundations/themes.css", import.meta.url), "utf8");
const themeBlocks = new Map(parseCompleteThemes(themeSource).map(({ id, body }) => [id, body]));

assert.deepEqual([...themeBlocks.keys()].sort(), [...expectedThemes].sort());

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
    theme.startsWith("wright-")
      ? ["var(--visual-range-ink)", "var(--visual-success-ink)", "var(--visual-source-ink)"]
      : ["var(--cw-collocate-text)", "var(--cw-success-text)", "var(--cw-category-numeric-text)"],
    `${theme} must map sort priority to distinct semantic inks`,
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
