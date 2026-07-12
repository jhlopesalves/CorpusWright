import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";
import { productionOutput, siteRoot } from "./paths.mjs";
import { manifestDifferences, treeManifest } from "./tree-manifest.mjs";

async function runBuild(output) {
  await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ["scripts/build.mjs", "--temporary", "--output", output], { cwd: siteRoot, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => code === 0 ? resolve() : reject(new Error(`temporary build exited with ${code}`)));
  });
}

const temporaryRoot = await fs.mkdtemp(path.join(os.tmpdir(), "corpuswright-site-check-"));
try {
  const first = path.join(temporaryRoot, "first");
  const second = path.join(temporaryRoot, "second");
  await runBuild(first);
  await runBuild(second);
  const committed = await treeManifest(productionOutput);
  const firstManifest = await treeManifest(first);
  const secondManifest = await treeManifest(second);
  const stale = manifestDifferences(committed, firstManifest);
  const nondeterministic = manifestDifferences(firstManifest, secondManifest);
  if (Object.values(stale).some((files) => files.length) || Object.values(nondeterministic).some((files) => files.length)) {
    console.error(JSON.stringify({ stale, nondeterministic }, null, 2));
    process.exitCode = 1;
  } else {
    console.log(`Generated output is current and reproducible (${committed.length} files).`);
  }
} finally {
  await fs.rm(temporaryRoot, { recursive: true, force: true });
}
