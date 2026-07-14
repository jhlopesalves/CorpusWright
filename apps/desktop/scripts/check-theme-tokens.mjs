import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  appearanceThemeIds,
  lineColumn,
  parseCompleteThemes,
  relativeModulePath,
} from "./css-contracts.mjs";
import { loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const srcRoot = path.join(desktopRoot, "src");
const themesPath = path.join(srcRoot, "styles/foundations/themes.css");
const figurePalettesPath = path.join(srcRoot, "styles/foundations/figure-palettes.css");
const appearancePath = path.join(srcRoot, "app/appearance.ts");
const rawColourPattern = /(?<!&)#(?:[0-9a-fA-F]{3,8})\b|rgba?\([^)\n]+\)|hsla?\([^)\n]+\)/g;
const namedColourPattern = /\b(?:color|background(?:-color)?|border(?:-[\w-]+)?|outline(?:-color)?|fill|stroke)\s*[:=]\s*["']?[^;\n"'`]*\b(?:white|black|red|green|blue|yellow|orange|purple|gray|grey)\b/gi;

async function collectSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "assets" || entry.name === "generated" || entry.name === "styles") continue;
      files.push(...await collectSourceFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith(".ts")) {
      files.push(fullPath);
    }
  }
  return files;
}

function rawColourViolations(filePath, source) {
  const violations = [];
  for (const pattern of [rawColourPattern, namedColourPattern]) {
    pattern.lastIndex = 0;
    for (const match of source.matchAll(pattern)) {
      const location = lineColumn(source, match.index);
      violations.push(`${relativeModulePath(desktopRoot, filePath)}:${location.line}:${location.column} ${match[0]}`);
    }
  }
  return violations;
}

const graph = await loadStyleGraph();
const themesModule = graph.modules.find(module => module.path === themesPath);
if (!themesModule) throw new Error("The style graph does not import foundations/themes.css");
const figurePalettesModule = graph.modules.find(module => module.path === figurePalettesPath);
if (!figurePalettesModule) throw new Error("The style graph does not import foundations/figure-palettes.css");

const themes = parseCompleteThemes(themesModule.contents, relativeModulePath(desktopRoot, themesPath));
const appearanceSource = await readFile(appearancePath, "utf8");
const expectedThemeIds = appearanceThemeIds(appearanceSource);
const actualThemeIds = themes.map(theme => theme.id);
const violations = [];

if (themes.length !== 14) violations.push(`expected exactly 14 complete themes, found ${themes.length}`);
if (new Set(actualThemeIds).size !== actualThemeIds.length) violations.push("complete theme ids must be unique");
const missingThemes = expectedThemeIds.filter(id => !actualThemeIds.includes(id));
const extraThemes = actualThemeIds.filter(id => !expectedThemeIds.includes(id));
if (missingThemes.length) violations.push(`CSS is missing appearance themes: ${missingThemes.join(", ")}`);
if (extraThemes.length) violations.push(`CSS has themes absent from appearance.ts: ${extraThemes.join(", ")}`);

const contractKeys = themes[0] ? [...themes[0].declarations.keys()] : [];
for (const theme of themes) {
  const keys = [...theme.declarations.keys()];
  const missing = contractKeys.filter(token => !theme.declarations.has(token));
  const extra = keys.filter(token => !contractKeys.includes(token));
  if (missing.length) violations.push(`${theme.id}: missing tokens ${missing.join(", ")}`);
  if (extra.length) violations.push(`${theme.id}: extra tokens ${extra.join(", ")}`);
}

const requiredGroups = {
  "result-table": ["--cw-result-table-surface", "--cw-result-table-header-surface", "--cw-result-table-row-alt-surface", "--cw-result-table-border"],
  plot: [
    "--cw-plot-point",
    "--cw-plot-fit",
    "--cw-theme-plot-model-fit",
    "--cw-theme-plot-statistical-reference",
    "--cw-theme-plot-target",
    "--cw-theme-plot-coverage-trajectory",
    "--cw-plot-grid",
    "--cw-plot-ramp-0",
    "--cw-plot-ramp-5",
  ],
  "position-profile": ["--cw-position-profile-bar", "--cw-position-profile-bar-active"],
  native: ["--cw-native-titlebar-color"],
};
for (const theme of themes) {
  for (const [group, tokens] of Object.entries(requiredGroups)) {
    const missing = tokens.filter(token => !theme.declarations.has(token));
    if (missing.length) violations.push(`${theme.id}: missing ${group} tokens ${missing.join(", ")}`);
  }
}

const semanticPlotRoles = [
  "--cw-plot-observed",
  "--cw-plot-model-fit",
  "--cw-plot-statistical-reference",
  "--cw-plot-target",
  "--cw-plot-coverage-trajectory",
  "--cw-plot-selected",
  "--cw-plot-attention",
  "--cw-plot-density-0",
  "--cw-plot-density-5",
  "--cw-plot-composition-0",
  "--cw-plot-composition-5",
  "--cw-plot-rank-band-top10",
  "--cw-plot-rank-band-11-100",
  "--cw-plot-rank-band-101-1000",
  "--cw-plot-rank-band-1001-10000",
  "--cw-plot-rank-band-tail",
  ...Array.from({ length: 10 }, (_, index) => `--cw-plot-profile-${index}`),
  "--cw-plot-witness",
  "--cw-plot-callout-surface",
  "--cw-plot-callout-border",
];
for (const role of semanticPlotRoles) {
  if (!figurePalettesModule.contents.includes(`${role}:`)) {
    violations.push(`figure-palettes.css is missing semantic figure role ${role}`);
  }
}
for (const mode of ["theme", "colorBlind", "monochrome"]) {
  if (!figurePalettesModule.contents.includes(`[data-figure-palette="${mode}"]`)) {
    violations.push(`figure-palettes.css is missing figure palette mode ${mode}`);
  }
}

const scannedFiles = [
  ...graph.modules.filter(module => module.path !== themesPath).map(module => ({ path: module.path, contents: module.contents })),
  { path: path.join(desktopRoot, "index.html"), contents: await readFile(path.join(desktopRoot, "index.html"), "utf8") },
  ...await Promise.all((await collectSourceFiles(srcRoot)).map(async filePath => ({ path: filePath, contents: await readFile(filePath, "utf8") }))),
];
for (const file of scannedFiles) violations.push(...rawColourViolations(file.path, file.contents));

if (violations.length) {
  console.error("Theme token check failed:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log(`Theme token check passed for ${themes.length} complete themes, ${contractKeys.length} contract tokens, and ${scannedFiles.length} non-theme source files.`);
