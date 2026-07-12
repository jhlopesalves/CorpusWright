import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { findTopLevelCssBlocks, lineColumn, relativeModulePath } from "./css-contracts.mjs";
import { DEFAULT_STYLE_ENTRY, loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const srcRoot = path.join(desktopRoot, "src");
const themesPath = path.join(srcRoot, "styles/foundations/themes.css");
const tablesPath = path.join(srcRoot, "styles/components/tables.css");
const axesPath = path.join(srcRoot, "styles/plots/axes-and-grids.css");
const annotationsPath = path.join(srcRoot, "styles/plots/legends-and-annotations.css");
const interactionsPath = path.join(srcRoot, "styles/plots/interactions.css");
const violations = [];
const importantAllowList = [
  {
    path: path.join(srcRoot, "styles/foundations/base.css"),
    selector: ".hidden",
    reason: "The explicit hidden state must override component and responsive display declarations.",
  },
];
const dynamicStyleAllowList = new Map([
  ["src/corpus.ts", new Set(["width"])], // PDF materialisation progress.
  ["src/file-list.ts", new Set(["height", "transform"])], // Virtual-scroll extent and offset.
  ["src/preview.ts", new Set(["height"])], // Data-sized preview virtualization spacers.
  ["src/workbench/build.ts", new Set(["width"])], // Analysis build progress.
  ["src/workbench/concordance/collocations.ts", new Set(["gridTemplateColumns"])], // Data-dependent position bins.
]);

async function collectTypeScript(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const filePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "assets" || entry.name === "generated") continue;
      files.push(...await collectTypeScript(filePath));
    } else if (entry.isFile() && entry.name.endsWith(".ts")) {
      files.push(filePath);
    }
  }
  return files;
}

function report(filePath, source, index, message) {
  const location = lineColumn(source, index);
  violations.push(`${relativeModulePath(desktopRoot, filePath)}:${location.line}:${location.column} ${message}`);
}

let graph;
try {
  graph = await loadStyleGraph(DEFAULT_STYLE_ENTRY);
} catch (error) {
  violations.push(error instanceof Error ? error.message : String(error));
  graph = { modules: [], css: "" };
}

const htmlPath = path.join(desktopRoot, "index.html");
const html = await readFile(htmlPath, "utf8");
for (const match of html.matchAll(/\sstyle\s*=\s*["']/gi)) report(htmlPath, html, match.index, "static style attribute");

const entrySource = await readFile(DEFAULT_STYLE_ENTRY, "utf8");
const entryWithoutImports = entrySource
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/@import\s+(?:url\([^;]+\)|["'][^"']+["'])[^;]*;/g, "")
  .trim();
if (entryWithoutImports) violations.push("src/styles.css: ordinary rule bodies are prohibited in the entrypoint");

for (const module of graph.modules) {
  const relativePath = relativeModulePath(desktopRoot, module.path);
  for (const match of module.contents.matchAll(/@import\s+(?:url\(\s*)?["']?(?:https?:)?\/\//gi)) {
    report(module.path, module.contents, match.index, "remote CSS import");
  }
  if (/migration-compatibility|(?:^|\/)legacy\.css$|temporary\.css$/.test(relativePath)) {
    violations.push(`${relativePath}: migration or legacy stylesheet must not survive`);
  }

  const ownershipRules = [
    { pattern: /\.cw-result-table(?:[\w-]*)?\b/, owner: tablesPath, label: "generic result-table selector" },
    { pattern: /\.cw-plot-(?:axis|grid|tick|minor-tick|reference-line|threshold-line)[\w-]*\b/, owner: axesPath, label: "generic axis/grid selector" },
    { pattern: /\.cw-plot-(?:legend|label|halo|bracket|quadrant-pill)[\w-]*\b/, owner: annotationsPath, label: "generic legend/annotation selector" },
    { pattern: /\.cw-plot-(?:tooltip|crosshair|hover-target|animated-path)[\w-]*\b/, owner: interactionsPath, label: "generic interaction selector" },
  ];
  for (const rule of ownershipRules) {
    if (module.path === rule.owner) continue;
    for (const match of module.contents.matchAll(new RegExp(rule.pattern.source, "g"))) {
      report(module.path, module.contents, match.index, `${rule.label} belongs in ${relativeModulePath(desktopRoot, rule.owner)}`);
    }
  }

  if (module.path.includes(`${path.sep}styles${path.sep}features${path.sep}`)) {
    for (const match of module.contents.matchAll(/font-family\s*:\s*([^;]+);/g)) {
      const value = match[1].trim();
      if (value !== "inherit" && !/^var\(--cw-font-(?:ui|mono|math)\)$/.test(value)) {
        report(module.path, module.contents, match.index, `feature font-family must use a semantic font token, found ${value}`);
      }
    }
    for (const match of module.contents.matchAll(/font-weight\s*:\s*([1-9]00)\s*;/g)) {
      report(module.path, module.contents, match.index, `feature font-weight must use a semantic weight token, found ${match[1]}`);
    }
  }

  for (const block of findTopLevelCssBlocks(module.contents)) {
    if (/(?:analysis|analyse).*(?:ready|building|build-complete|result-state)/i.test(block.selector)
        && /font-(?:family|weight|size|stretch)|letter-spacing/.test(block.body)) {
      report(module.path, module.contents, block.start, `analysis state selector changes inherited typography: ${block.selector}`);
    }
    if (block.body.includes("!important")) {
      const allowed = importantAllowList.some(entry => entry.path === module.path && entry.selector === block.selector);
      if (!allowed) report(module.path, module.contents, block.start, `unapproved !important in ${block.selector}`);
    }
  }
}

const rawColour = /(?<!&)#(?:[0-9a-fA-F]{3,8})\b|rgba?\([^)\n]+\)|hsla?\([^)\n]+\)/g;
const typescriptFiles = await collectTypeScript(srcRoot);
const nonThemeFiles = [
  ...graph.modules.filter(module => module.path !== themesPath).map(module => ({ path: module.path, source: module.contents })),
  { path: htmlPath, source: html },
  ...await Promise.all(typescriptFiles.map(async filePath => ({ path: filePath, source: await readFile(filePath, "utf8") }))),
];
for (const file of nonThemeFiles) {
  for (const match of file.source.matchAll(rawColour)) report(file.path, file.source, match.index, `raw colour ${match[0]} outside complete theme contracts`);
}

for (const filePath of typescriptFiles) {
  const source = await readFile(filePath, "utf8");
  const relativePath = relativeModulePath(desktopRoot, filePath);
  for (const match of source.matchAll(/\.style\.([A-Za-z]+)\s*=/g)) {
    if (!dynamicStyleAllowList.get(relativePath)?.has(match[1])) {
      report(filePath, source, match.index, `unapproved TypeScript style assignment for ${match[1]}`);
    }
  }
  for (const match of source.matchAll(/\.style\.removeProperty\(\s*["']([^"']+)["']/g)) {
    if (!dynamicStyleAllowList.get(relativePath)?.has(match[1])) {
      report(filePath, source, match.index, `unapproved TypeScript style removal for ${match[1]}`);
    }
  }
  for (const match of source.matchAll(/style\s*=\s*["'`]/g)) report(filePath, source, match.index, "inline style markup in TypeScript");
  for (const match of source.matchAll(/\.style\.setProperty\(\s*["']([^"']+)["']/g)) {
    if (!match[1].startsWith("--")) report(filePath, source, match.index, `setProperty must target a runtime custom property, found ${match[1]}`);
  }
  for (const match of source.matchAll(/\.style\.setProperty\(\s*[A-Za-z_$]/g)) {
    if (relativePath !== "src/plots/render.ts") report(filePath, source, match.index, "dynamic style property names are restricted to SVG export token resolution");
  }
}

if (violations.length) {
  console.error("Style ownership check failed:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log(`Style ownership check passed for ${graph.modules.length} CSS modules, ${typescriptFiles.length} TypeScript files, and zero HTML style attributes.`);
