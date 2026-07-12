import esbuild from "esbuild";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { assertSafeOutput } from "./clean-output.mjs";
import { productionOutput, siteRoot } from "./paths.mjs";

const outputIndex = process.argv.indexOf("--output");
const output = path.resolve(outputIndex === -1 ? productionOutput : process.argv[outputIndex + 1]);
const temporary = output !== productionOutput;
await assertSafeOutput(output, { temporary });

const assetsOutput = path.join(output, "assets");
await fs.mkdir(assetsOutput, { recursive: true });

await esbuild.build({
  entryPoints: [path.join(siteRoot, "src", "styles", "site.css")],
  outfile: path.join(assetsOutput, "site.css"),
  bundle: true,
  external: ["fonts/*"],
  legalComments: "none",
  sourcemap: false
});

await esbuild.build({
  entryPoints: {
    "theme-init": path.join(siteRoot, "src", "scripts", "theme-init.ts"),
    "mathjax-config": path.join(siteRoot, "src", "scripts", "mathjax-config.ts"),
    redirects: path.join(siteRoot, "src", "scripts", "redirects.ts"),
    site: path.join(siteRoot, "src", "scripts", "site.ts"),
    mermaid: path.join(siteRoot, "src", "scripts", "mermaid.ts")
  },
  outdir: assetsOutput,
  entryNames: "[name]",
  bundle: true,
  format: "iife",
  platform: "browser",
  target: ["es2020"],
  minify: true,
  legalComments: "none",
  sourcemap: false
});

const mathjaxSource = path.join(siteRoot, "node_modules", "mathjax-full", "es5");
const mathjaxOutput = path.join(assetsOutput, "vendor", "mathjax");
await fs.mkdir(path.join(mathjaxOutput, "output", "chtml", "fonts"), { recursive: true });
await fs.copyFile(path.join(mathjaxSource, "tex-chtml.js"), path.join(mathjaxOutput, "tex-chtml.js"));
await fs.cp(
  path.join(mathjaxSource, "output", "chtml", "fonts", "woff-v2"),
  path.join(mathjaxOutput, "output", "chtml", "fonts", "woff-v2"),
  { recursive: true }
);
