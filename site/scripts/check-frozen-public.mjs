import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

const contract = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "frozen-public.json"), "utf8"));
const errors = [];
for (const [file, expected] of Object.entries(contract.files)) {
  for (const [owner, root] of [["source", path.join(siteRoot, "src", "public")], ["output", productionOutput]]) {
    try {
      const bytes = await fs.readFile(path.join(root, file));
      const actual = crypto.createHash("sha256").update(bytes).digest("hex");
      if (actual !== expected) errors.push(`${owner}/${file}: expected ${expected}, received ${actual}`);
    } catch {
      errors.push(`${owner}/${file}: missing frozen public file`);
    }
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else console.log(`${Object.keys(contract.files).length} Markdown, asset, screenshot, figure, and figure-source files match the baseline.`);
