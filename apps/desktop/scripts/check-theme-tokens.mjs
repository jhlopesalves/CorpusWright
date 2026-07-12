import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadStyleGraph } from "./style-graph.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const desktopRoot = path.resolve(scriptDir, "..");
const srcRoot = path.join(desktopRoot, "src");
const stylesPath = path.join(srcRoot, "styles.css");
const rawColourPattern = /(?<!&)#(?:[0-9a-fA-F]{3,8})\b|rgba?\([^)\n]+\)|hsla?\([^)\n]+\)/g;
const namedColourPattern = /\b(?:color|background(?:-color)?|border(?:-[\w-]+)?|outline(?:-color)?|fill|stroke)\s*[:=]\s*["']?[^;\n"'`]*\b(?:white|black|red|green|blue|yellow|orange|purple|gray|grey)\b/gi;
const ignoredThemeTokens = new Set([
  "--cw-font-ui",
  "--cw-font-mono",
  "--cw-font-math",
  "--cw-font-size-base",
]);

function isThemeSelector(selector) {
  return selector.includes(":root") || selector.includes("[data-theme");
}

function lineColumn(source, index) {
  const before = source.slice(0, index);
  const lines = before.split(/\r?\n/);
  return {
    line: lines.length,
    column: lines[lines.length - 1].length + 1,
  };
}

function findClosingBrace(source, openIndex) {
  let depth = 0;
  for (let index = openIndex; index < source.length; index += 1) {
    if (source[index] === "{") {
      depth += 1;
    } else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return -1;
}

function findCssBlocks(source) {
  const blocks = [];
  let cursor = 0;

  while (cursor < source.length) {
    const openIndex = source.indexOf("{", cursor);
    if (openIndex === -1) {
      break;
    }

    const closeIndex = findClosingBrace(source, openIndex);
    if (closeIndex === -1) {
      throw new Error("Could not parse styles.css: unmatched opening brace.");
    }

    const selectorStart = Math.max(source.lastIndexOf("}", openIndex) + 1, 0);
    const selector = source.slice(selectorStart, openIndex).trim();
    blocks.push({
      selector,
      start: openIndex,
      end: closeIndex,
      body: source.slice(openIndex + 1, closeIndex),
    });
    cursor = closeIndex + 1;
  }

  return blocks;
}

function parseThemeBlocks(source) {
  return findCssBlocks(source)
    .filter(block => isThemeSelector(block.selector))
    .map(block => {
      const themeMatch = block.selector.match(/\[data-theme=["']?([^"'\]]+)["']?\]/);
      const themeId = themeMatch?.[1] ?? "root";
      const tokens = new Set(
        [...block.body.matchAll(/(--[\w-]+)\s*:/g)]
          .map(match => match[1])
          .filter(token => !ignoredThemeTokens.has(token)),
      );

      return { ...block, themeId, tokens };
    });
}

function isInsideRange(index, ranges) {
  return ranges.some(range => index >= range.start && index <= range.end);
}

function reportRawColours(filePath, source, tokenRanges) {
  const violations = [];

  for (const pattern of [rawColourPattern, namedColourPattern]) {
    pattern.lastIndex = 0;
    for (const match of source.matchAll(pattern)) {
      if (filePath === stylesPath && isInsideRange(match.index, tokenRanges)) {
        continue;
      }

      const location = lineColumn(source, match.index);
      violations.push(`${path.relative(desktopRoot, filePath)}:${location.line}:${location.column} ${match[0]}`);
    }
  }

  return violations;
}

async function collectSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "assets" || entry.name === "generated") {
        continue;
      }
      files.push(...await collectSourceFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith(".ts")) {
      files.push(fullPath);
    }
  }

  return files;
}

function reportThemeCoverage(themeBlocks) {
  const baseBlock = themeBlocks.find(block => block.themeId === "dark") ?? themeBlocks[0];
  if (!baseBlock) {
    return ["styles.css: no theme token blocks found"];
  }

  const violations = [];
  for (const block of themeBlocks) {
    const missing = [...baseBlock.tokens].filter(token => !block.tokens.has(token));
    const extra = [...block.tokens].filter(token => !baseBlock.tokens.has(token));

    if (missing.length > 0) {
      violations.push(`${block.themeId}: missing tokens ${missing.join(", ")}`);
    }
    if (extra.length > 0) {
      violations.push(`${block.themeId}: extra tokens ${extra.join(", ")}`);
    }
  }

  return violations;
}

const { css: stylesSource } = await loadStyleGraph(stylesPath);
const themeBlocks = parseThemeBlocks(stylesSource);
const tokenRanges = themeBlocks.map(block => ({ start: block.start, end: block.end }));
const files = [
  stylesPath,
  path.join(desktopRoot, "index.html"),
  ...await collectSourceFiles(srcRoot),
];

const rawColourViolations = [];
for (const filePath of files) {
  const source = await readFile(filePath, "utf8");
  rawColourViolations.push(...reportRawColours(filePath, source, tokenRanges));
}

const coverageViolations = reportThemeCoverage(themeBlocks);
const violations = [...rawColourViolations, ...coverageViolations];

if (violations.length > 0) {
  console.error("Theme token check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(`Theme token check passed for ${themeBlocks.length} theme token blocks and ${files.length} source files.`);
