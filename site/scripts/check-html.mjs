import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";

const contract = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));
let failed = false;
for (const url of Object.keys(contract.pages)) {
  const html = await fs.readFile(path.join(productionOutput, url.slice(1)), "utf8");
  if (!/<html\b[^>]*\blang=/i.test(html) || !/<title>[^<]+<\/title>/i.test(html)) {
    failed = true;
    console.error(`${url}: missing language or title`);
  }
}
if (failed) process.exitCode = 1;
else console.log(`Basic HTML contract passes for ${Object.keys(contract.pages).length} routes.`);
