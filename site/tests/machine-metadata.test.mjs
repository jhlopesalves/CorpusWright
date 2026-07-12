import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { productionOutput } from "../scripts/paths.mjs";

test("sitemap contains only preferred principal routes", async () => {
  const sitemap = await fs.readFile(path.join(productionOutput, "sitemap.xml"), "utf8");
  const locations = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
  assert.deepEqual(locations, [
    "https://jhlopesalves.github.io/CorpusWright/",
    "https://jhlopesalves.github.io/CorpusWright/quick-start.html",
    "https://jhlopesalves.github.io/CorpusWright/concepts/overview.html",
    "https://jhlopesalves.github.io/CorpusWright/roadmap.html",
    "https://jhlopesalves.github.io/CorpusWright/technical-docs.html",
    "https://jhlopesalves.github.io/CorpusWright/citation.html",
    "https://jhlopesalves.github.io/CorpusWright/about.html"
  ]);
});

test("robots points to the generated sitemap", async () => {
  const robots = await fs.readFile(path.join(productionOutput, "robots.txt"), "utf8");
  assert.match(robots, /^User-agent: \*$/m);
  assert.match(robots, /^Sitemap: https:\/\/jhlopesalves\.github\.io\/CorpusWright\/sitemap\.xml$/m);
});
