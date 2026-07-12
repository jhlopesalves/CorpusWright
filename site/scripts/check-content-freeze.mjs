import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput, repositoryRoot, siteRoot } from "./paths.mjs";
import { normalisePublicText, textHash } from "./content-text.mjs";

const contract = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-copy.json"), "utf8"));
const release = JSON.parse(await fs.readFile(path.join(repositoryRoot, "release-metadata.json"), "utf8"));
const citation = (await fs.readFile(path.join(repositoryRoot, "CITATION.cff"), "utf8")).replace(/\s+/g, " ").trim();
const dynamicValues = [
  [citation, "{{CITATION_CFF}}"],
  [release.version, "{{RELEASE_VERSION}}"],
  [release.dateReleased, "{{RELEASE_DATE}}"],
  [release.dateReleased.slice(0, 4), "{{RELEASE_YEAR}}"]
];
let failed = false;
for (const [url, expected] of Object.entries(contract.pages)) {
  const html = await fs.readFile(path.join(productionOutput, url.slice(1)), "utf8");
  const actualText = normalisePublicText(html, dynamicValues);
  const actualHash = textHash(actualText);
  if (actualHash !== expected.sha256) {
    failed = true;
    console.error(`${url}: public copy changed\nEXPECTED: ${expected.text.join(" ")}\nACTUAL:   ${actualText.join(" ")}`);
  }
}
if (failed) process.exitCode = 1;
else console.log(`Public copy is unchanged across ${Object.keys(contract.pages).length} routes.`);
