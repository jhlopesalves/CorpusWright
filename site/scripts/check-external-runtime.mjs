import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

const contract = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));
const errors = [];
for (const route of Object.keys(contract.pages)) {
  const html = await fs.readFile(path.join(productionOutput, route.slice(1)), "utf8");
  const withoutComments = html.replace(/<!--[^]*?-->/g, "");
  for (const match of withoutComments.matchAll(/<script\b([^>]*)>/gi)) {
    const source = match[1].match(/\bsrc=["']([^"']+)["']/i)?.[1];
    if (!source) errors.push(`${route}: inline behaviour script`);
    else if (/^(?:https?:)?\/\//i.test(source)) errors.push(`${route}: remote executable script ${source}`);
  }
  if (/<style\b/i.test(withoutComments)) errors.push(`${route}: inline style block`);
  if (/\sstyle=["']/i.test(withoutComments)) errors.push(`${route}: inline style attribute`);
}

for (const asset of [
  "assets/theme-init.js",
  "assets/site.js",
  "assets/mermaid.js",
  "assets/mathjax-config.js",
  "assets/vendor/mathjax/tex-chtml.js"
]) {
  try { await fs.access(path.join(productionOutput, asset)); } catch { errors.push(`missing local runtime ${asset}`); }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else console.log("All executable runtime is owned and served locally; generated pages contain no inline styles or behaviour.");
