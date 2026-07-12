import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const metadata = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "release-metadata.json"), "utf8"));

export default {
  version: metadata.version,
  date: metadata.dateReleased,
  year: metadata.dateReleased.slice(0, 4)
};
