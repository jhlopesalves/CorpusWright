import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const html = readFileSync(resolve("apps/desktop/index.html"), "utf8");

assert(!html.includes("analyse-language-override"), "Analyse must not expose a language override dropdown");
assert(!html.includes("Auto detect"), "Analyse must not offer auto-detect as a project-language alternative");
assert(!html.includes("analyse-project-language"), "Analyse toolbar no longer exposes a language override/pill");
assert(html.includes("project-context-language"), "Project metadata/header must show project language");
assert(html.includes("project-language-upgrade-modal"), "Legacy projects must have a language upgrade dialog");
