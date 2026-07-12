import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const source = fs.readFileSync(path.join(repositoryRoot, "CITATION.cff"), "utf8");
const value = (key) => source.match(new RegExp(`^${key}:\\s*["']?([^"'\\n]+)`, "m"))?.[1].trim() ?? "";
const familyName = source.match(/^\s*family-names:\s*([^\n]+)/m)?.[1].trim() ?? "";
const givenNames = source.match(/^\s*given-names:\s*([^\n]+)/m)?.[1].trim() ?? "";

export default {
  source,
  message: value("message"),
  author: `${givenNames} ${familyName}`.trim(),
  orcid: source.match(/^\s*orcid:\s*["']?([^"'\n]+)/m)?.[1].trim() ?? "",
  repositoryUrl: value("repository-code"),
  publicUrl: value("url"),
  licence: value("license")
};
