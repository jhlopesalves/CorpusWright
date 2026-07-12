import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";

async function walk(directory, root = directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name, "en"))) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await walk(absolute, root));
    else if (entry.isFile()) files.push({ absolute, relative: path.relative(root, absolute).split(path.sep).join("/") });
  }
  return files;
}

export async function treeManifest(directory) {
  const files = await walk(directory);
  return Promise.all(files.map(async ({ absolute, relative }) => {
    const bytes = await fs.readFile(absolute);
    return {
      path: relative,
      bytes: bytes.length,
      sha256: crypto.createHash("sha256").update(bytes).digest("hex")
    };
  }));
}

export function manifestDifferences(expected, actual) {
  const before = new Map(expected.map((entry) => [entry.path, entry]));
  const after = new Map(actual.map((entry) => [entry.path, entry]));
  return {
    added: [...after.keys()].filter((file) => !before.has(file)),
    removed: [...before.keys()].filter((file) => !after.has(file)),
    changed: [...after.keys()].filter((file) => before.has(file) && after.get(file).sha256 !== before.get(file).sha256)
  };
}
