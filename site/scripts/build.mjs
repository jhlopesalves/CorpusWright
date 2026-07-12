import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { cleanOutput } from "./clean-output.mjs";
import { productionOutput, siteRoot } from "./paths.mjs";

function parseArguments(arguments_) {
  const outputIndex = arguments_.indexOf("--output");
  const output = outputIndex === -1 ? productionOutput : arguments_[outputIndex + 1];
  if (outputIndex !== -1 && !output) throw new Error("--output requires a path");
  return { output: path.resolve(output), temporary: arguments_.includes("--temporary") };
}

function run(command, arguments_) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { cwd: siteRoot, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => code === 0 ? resolve() : reject(new Error(`${command} exited with ${code}`)));
  });
}

export async function build(arguments_ = process.argv.slice(2)) {
  const options = parseArguments(arguments_);
  const output = await cleanOutput(options.output, { temporary: options.temporary });
  await run(process.execPath, [
    path.join(siteRoot, "node_modules", "@11ty", "eleventy", "cmd.cjs"),
    "--config=eleventy.config.mjs",
    `--output=${output}`
  ]);
  await run(process.execPath, ["scripts/build-assets.mjs", "--output", output]);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await build();
}
