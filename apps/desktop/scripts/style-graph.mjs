import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_STYLE_ENTRY = path.resolve(scriptDir, "../src/styles.css");

const importPattern = /@import\s+(?:url\(\s*)?(?:(['"])(.*?)\1|([^'"\s);]+))(?:\s*\))?[^;]*;/g;

function asPath(input) {
  return input instanceof URL ? fileURLToPath(input) : path.resolve(input);
}

function isRemoteImport(specifier) {
  return /^(?:[a-z]+:)?\/\//i.test(specifier) || /^(?:data|https?):/i.test(specifier);
}

export async function loadStyleGraph(entry = DEFAULT_STYLE_ENTRY) {
  const entryPath = asPath(entry);
  const modules = [];

  async function visit(modulePath, ancestry) {
    const absolutePath = path.resolve(modulePath);
    if (ancestry.includes(absolutePath)) {
      const cycle = [...ancestry.slice(ancestry.indexOf(absolutePath)), absolutePath]
        .map(file => path.relative(path.dirname(entryPath), file) || path.basename(file));
      throw new Error(`CSS import cycle: ${cycle.join(" -> ")}`);
    }

    let contents;
    try {
      contents = await readFile(absolutePath, "utf8");
    } catch (error) {
      if (error?.code === "ENOENT") {
        throw new Error(`Missing CSS import: ${absolutePath}`, { cause: error });
      }
      throw error;
    }

    modules.push({ path: absolutePath, contents });
    const nextAncestry = [...ancestry, absolutePath];
    const chunks = [];
    let cursor = 0;

    importPattern.lastIndex = 0;
    for (const match of contents.matchAll(importPattern)) {
      chunks.push(contents.slice(cursor, match.index));
      const specifier = match[2] ?? match[3];
      if (!isRemoteImport(specifier)) {
        const importedPath = path.resolve(path.dirname(absolutePath), specifier);
        chunks.push(await visit(importedPath, nextAncestry));
      }
      cursor = match.index + match[0].length;
    }
    chunks.push(contents.slice(cursor));
    return chunks.join("");
  }

  const css = await visit(entryPath, []);
  return { css, modules, entryPath };
}
