import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadStyleGraph } from "./style-graph.mjs";
import { validateVisualContracts } from "./visual-contracts.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const graph = await loadStyleGraph();
const themesPath = path.join(desktopRoot, "src/styles/foundations/themes.css");
const themeSource = graph.modules.find(module => module.path === themesPath)?.contents;
if (!themeSource) throw new Error("The style graph does not import foundations/themes.css");

const typescriptPaths = [
  "src/plots/figure.ts",
  "src/plots/render.ts",
  "src/plots/svg-export.ts",
];
const typescriptSources = await Promise.all(typescriptPaths.map(async relativePath => ({
  path: path.join(desktopRoot, relativePath),
  contents: await readFile(path.join(desktopRoot, relativePath), "utf8"),
})));
const violations = validateVisualContracts({
  modules: graph.modules,
  desktopRoot,
  themeSource,
  typescriptSources,
});

if (violations.length) {
  console.error("Visual contract check failed:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log(`Visual contract check passed for ${graph.modules.length} CSS modules, 14 themes, and ${typescriptSources.length} export sources.`);
