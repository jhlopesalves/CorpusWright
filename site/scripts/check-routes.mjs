import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

const contract = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-routes.json"), "utf8"));
const missing = [];
for (const file of contract.staticFiles) {
  try { await fs.access(path.join(productionOutput, file)); } catch { missing.push(file); }
}
if (missing.length) {
  console.error(`Missing baseline public files:\n${missing.join("\n")}`);
  process.exitCode = 1;
} else console.log(`All ${contract.staticFiles.length} baseline public files exist.`);
