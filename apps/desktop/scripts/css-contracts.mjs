import path from "node:path";

export const THEME_VARIANTS = ["light", "dark"];

function closingBrace(source, openIndex) {
  let depth = 0;
  let quote = null;
  let comment = false;
  for (let index = openIndex; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (comment) {
      if (char === "*" && next === "/") {
        comment = false;
        index += 1;
      }
      continue;
    }
    if (quote) {
      if (char === "\\") index += 1;
      else if (char === quote) quote = null;
      continue;
    }
    if (char === "/" && next === "*") {
      comment = true;
      index += 1;
    } else if (char === '"' || char === "'") {
      quote = char;
    } else if (char === "{") {
      depth += 1;
    } else if (char === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error("Could not parse CSS: unmatched opening brace");
}

export function findTopLevelCssBlocks(source) {
  const blocks = [];
  let cursor = 0;
  while (cursor < source.length) {
    const openIndex = source.indexOf("{", cursor);
    if (openIndex === -1) break;
    const closeIndex = closingBrace(source, openIndex);
    const selector = source.slice(cursor, openIndex).replace(/\/\*[\s\S]*?\*\//g, "").trim();
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

export function parseDeclarations(body) {
  const declarations = new Map();
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    declarations.set(match[1], match[2].trim());
  }
  return declarations;
}

export function parseCompleteThemes(source, modulePath = "themes.css") {
  return findTopLevelCssBlocks(source).map(block => {
    const themeMatch = block.selector.match(/\[data-theme=["']([^"']+)["']\]/);
    if (!themeMatch || (!block.selector.startsWith(":root") && block.selector !== themeMatch[0])) {
      throw new Error(`${modulePath}: non-contract selector in complete theme module: ${block.selector}`);
    }
    return {
      ...block,
      id: themeMatch[1],
      declarations: parseDeclarations(block.body),
      modulePath,
    };
  });
}

export function appearanceThemeIds(source) {
  const familyBlock = source.match(/THEME_FAMILIES[^=]*=\s*\[([\s\S]*?)\n\];/);
  if (!familyBlock) throw new Error("Could not find THEME_FAMILIES in appearance.ts");
  const families = [...familyBlock[1].matchAll(/\bid:\s*["']([^"']+)["']/g)].map(match => match[1]);
  return families.flatMap(family => THEME_VARIANTS.map(variant => `${family}-${variant}`));
}

export function resolveToken(token, declarations, seen = new Set()) {
  if (seen.has(token)) return null;
  const value = declarations.get(token);
  if (!value) return null;
  const alias = value.match(/^var\((--[\w-]+)\)$/);
  if (!alias) return value;
  seen.add(token);
  return resolveToken(alias[1], declarations, seen);
}

export function lineColumn(source, index) {
  const before = source.slice(0, index);
  const lines = before.split(/\r?\n/);
  return { line: lines.length, column: lines.at(-1).length + 1 };
}

export function relativeModulePath(root, modulePath) {
  return path.relative(root, modulePath).split(path.sep).join("/");
}
