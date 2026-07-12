import path from "node:path";
import { fileURLToPath } from "node:url";

export const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const repositoryRoot = path.resolve(siteRoot, "..");
export const productionOutput = path.join(repositoryRoot, "docs");
