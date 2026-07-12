import esbuild from "esbuild";
import path from "node:path";
import process from "node:process";
import { assertSafeOutput } from "./clean-output.mjs";
import { productionOutput, siteRoot } from "./paths.mjs";

const outputIndex = process.argv.indexOf("--output");
const output = path.resolve(outputIndex === -1 ? productionOutput : process.argv[outputIndex + 1]);
const temporary = output !== productionOutput;
await assertSafeOutput(output, { temporary });

await esbuild.build({
  entryPoints: [path.join(siteRoot, "src", "scripts", "redirects.ts")],
  outfile: path.join(output, "assets", "redirects.js"),
  bundle: true,
  format: "iife",
  platform: "browser",
  target: ["es2020"],
  legalComments: "none",
  sourcemap: false
});
