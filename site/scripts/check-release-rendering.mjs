import fs from "node:fs/promises";
import path from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";
import { productionOutput, repositoryRoot } from "./paths.mjs";

const release = JSON.parse(await fs.readFile(path.join(repositoryRoot, "release-metadata.json"), "utf8"));
const cff = await fs.readFile(path.join(repositoryRoot, "CITATION.cff"), "utf8");
const citationHtml = await fs.readFile(path.join(productionOutput, "citation.html"), "utf8");
const homeHtml = await fs.readFile(path.join(productionOutput, "index.html"), "utf8");
const decode = (text) => text.replace(/&quot;/g, "\"").replace(/&#39;/g, "'").replace(/&gt;/g, ">").replace(/&lt;/g, "<").replace(/&amp;/g, "&");
const displayedCff = decode(citationHtml.match(/id="citation-cff"><code class="mono">([^]*?)<\/code>/)?.[1] ?? "");

if (!citationHtml.includes(release.version) || !citationHtml.includes(release.dateReleased.slice(0, 4)) || displayedCff !== cff.trimEnd()) {
  throw new Error("Generated release or CITATION.cff rendering does not match canonical metadata");
}
if (!homeHtml.includes(release.version)) throw new Error("Homepage citation does not render the canonical version");

await new Promise((resolve, reject) => {
  const child = spawn(process.execPath, ["scripts/check-release-consistency.cjs"], { cwd: repositoryRoot, stdio: "inherit" });
  child.on("error", reject);
  child.on("exit", (code) => code === 0 ? resolve() : reject(new Error(`release consistency check exited with ${code}`)));
});
console.log("Website release and citation rendering matches repository authorities.");
