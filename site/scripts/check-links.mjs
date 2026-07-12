import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { inspectLinks } from "./quality-rules.mjs";
import { productionOutput } from "./paths.mjs";

async function walk(directory, root = directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map((entry) => {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) return walk(absolute, root);
    return path.relative(root, absolute).split(path.sep).join("/");
  }))).flat();
}

const allFiles = new Set(await walk(productionOutput));
const htmlFiles = [...allFiles].filter((file) => file.endsWith(".html") && !file.startsWith("assets/"));
const documents = new Map(await Promise.all(htmlFiles.map(async (file) => [`/${file}`, await fs.readFile(path.join(productionOutput, file), "utf8")])));
const result = inspectLinks(documents, allFiles);
if (result.errors.length) {
  console.error(result.errors.join("\n"));
  process.exitCode = 1;
} else console.log(`${result.links} internal and external references resolve syntactically; ${result.fragments} internal fragments resolve.`);
