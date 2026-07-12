import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { productionOutput, siteRoot } from "../scripts/paths.mjs";

const routes = JSON.parse(await fs.readFile(path.join(siteRoot, "contracts", "public-routes.json"), "utf8"));

test("every baseline public file is generated", async () => {
  for (const file of routes.staticFiles) await fs.access(path.join(productionOutput, file));
  assert(routes.staticFiles.includes(".nojekyll"));
});

test("generated text does not expose local absolute paths", async () => {
  for (const file of routes.staticFiles.filter((file) => /\.(?:html|css|js|md|txt|json|svg)$/i.test(file))) {
    const content = await fs.readFile(path.join(productionOutput, file), "utf8");
    assert.doesNotMatch(content, /\/(?:home|Users)\/[^/]+\//, file);
  }
});
