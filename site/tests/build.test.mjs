import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { assertSafeOutput } from "../scripts/clean-output.mjs";
import { productionOutput, repositoryRoot, siteRoot } from "../scripts/paths.mjs";

test("only docs is accepted as the production output", async () => {
  assert.equal(await assertSafeOutput(productionOutput), productionOutput);
  for (const target of [path.parse(repositoryRoot).root, repositoryRoot, siteRoot, path.join(repositoryRoot, "apps"), path.join(repositoryRoot, "crates"), path.join(repositoryRoot, "other")]) {
    await assert.rejects(assertSafeOutput(target), /Refusing unsafe|must resolve exactly/);
  }
});

test("temporary outputs must remain below the system temporary directory", async () => {
  const safe = path.join(os.tmpdir(), "corpuswright-site-test-output");
  assert.equal(await assertSafeOutput(safe, { temporary: true }), safe);
  await assert.rejects(assertSafeOutput(path.join(repositoryRoot, "temporary"), { temporary: true }), /must be below/);
});

test("repository has no root Node project", async () => {
  await assert.rejects(fs.access(path.join(repositoryRoot, "package.json")));
  await assert.rejects(fs.access(path.join(repositoryRoot, "package-lock.json")));
});
