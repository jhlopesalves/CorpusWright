import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { productionOutput } from "./paths.mjs";

const css = await fs.readFile(path.join(productionOutput, "assets", "site.css"), "utf8");
if (!css.trim()) {
  console.error("Generated site stylesheet is empty.");
  process.exitCode = 1;
} else console.log("Generated site stylesheet exists.");
