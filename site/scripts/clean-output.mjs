import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { productionOutput, repositoryRoot, siteRoot } from "./paths.mjs";

const forbiddenRepositoryTargets = [
  repositoryRoot,
  siteRoot,
  path.join(repositoryRoot, "apps"),
  path.join(repositoryRoot, "crates")
];

async function assertRepositoryRoot() {
  for (const marker of ["Cargo.toml", "release-metadata.json", ".git"]) {
    await fs.access(path.join(repositoryRoot, marker));
  }
}

export async function assertSafeOutput(output, { temporary = false } = {}) {
  await assertRepositoryRoot();
  const resolved = path.resolve(output);

  if (resolved === path.parse(resolved).root || forbiddenRepositoryTargets.includes(resolved)) {
    throw new Error(`Refusing unsafe documentation clean target: ${resolved}`);
  }

  if (temporary) {
    const relativeToTmp = path.relative(path.resolve(os.tmpdir()), resolved);
    if (relativeToTmp.startsWith("..") || path.isAbsolute(relativeToTmp)) {
      throw new Error(`Temporary documentation output must be below ${os.tmpdir()}: ${resolved}`);
    }
    return resolved;
  }

  if (resolved !== productionOutput) {
    throw new Error(`Documentation output must resolve exactly to ${productionOutput}: ${resolved}`);
  }
  return resolved;
}

export async function cleanOutput(output, options) {
  const safeOutput = await assertSafeOutput(output, options);
  await fs.rm(safeOutput, { recursive: true, force: true });
  await fs.mkdir(safeOutput, { recursive: true });
  return safeOutput;
}
