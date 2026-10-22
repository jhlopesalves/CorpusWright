#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const REPOSITORY_URL = "https://github.com/jhlopesalves/CorpusWright";
const WEBSITE_URL = "https://jhlopesalves.github.io/CorpusWright/";

function rootPath(relativePath) {
  return path.join(ROOT, relativePath);
}

function readText(relativePath) {
  return fs.readFileSync(rootPath(relativePath), "utf-8");
}

function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

function detectNewline(text) {
  return text.includes("\r\n") ? "\r\n" : "\n";
}

function preserveNewline(original, next) {
  return next.replace(/\r?\n/g, detectNewline(original));
}

function stringifyJsonLike(original, value) {
  return preserveNewline(original, `${JSON.stringify(value, null, 2)}\n`);
}

function htmlEscape(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function versionPattern() {
  return /^\d+\.\d+\.\d+-alpha\.\d+$/;
}

function validateMetadata(metadata) {
  if (!versionPattern().test(metadata.version || "")) {
    throw new Error(
      `release-metadata.json version "${metadata.version}" must match X.Y.Z-alpha.N`
    );
  }

  if (!/^\d{4}-\d{2}-\d{2}$/.test(metadata.dateReleased || "")) {
    throw new Error(
      `release-metadata.json dateReleased "${metadata.dateReleased}" must match YYYY-MM-DD`
    );
  }

}

function getReleaseMetadata() {
  const metadata = readJson("release-metadata.json");
  validateMetadata(metadata);
  return metadata;
}

function releaseTag(metadata) {
  return `v${metadata.version}`;
}

function alphaLabel(metadata) {
  const match = metadata.version.match(/-alpha\.(\d+)$/);
  return `alpha.${match[1]}`;
}

function releaseYear(metadata) {
  return metadata.dateReleased.slice(0, 4);
}

function buildCitationCff(metadata) {
  return `cff-version: 1.2.0
message: "If you use CorpusWright in your research or teaching, please cite it as below."
title: CorpusWright
abstract: >-
  A desktop corpus-linguistics workbench with a Rust core for preparing,
  inspecting, and analysing text corpora. Corpus data is processed locally,
  and cleaned-corpus exports include preparation settings and file checksums.
type: software
authors:
  - family-names: Lopes
    given-names: Jhonatan H.
    orcid: "https://orcid.org/0009-0008-4914-0262"
repository-code: "${REPOSITORY_URL}"
url: "${WEBSITE_URL}"
version: ${metadata.version}
date-released: "${metadata.dateReleased}"
license: MIT
keywords:
  - corpus linguistics
  - text processing
  - concordance
  - KWIC
  - reproducibility
  - Rust
`;
}

function buildBibtex(metadata) {
  const year = releaseYear(metadata);
  return `@software{lopes_corpuswright_${year},
  author  = {Lopes, Jhonatan H.},
  title   = {CorpusWright},
  year    = {${year}},
  version = {${metadata.version}},
  url     = {${REPOSITORY_URL}},
  note    = {ORCID: 0009-0008-4914-0262}
}`;
}

function replaceOrThrow(content, pattern, replacement, relativePath, label) {
  if (!pattern.test(content)) {
    throw new Error(`Could not find ${label} in ${relativePath}`);
  }
  return content.replace(pattern, () => replacement);
}

function createWriter(options) {
  const changed = [];
  const check = Boolean(options.check);
  const quiet = Boolean(options.quiet);

  return {
    write(relativePath, next) {
      const absolutePath = rootPath(relativePath);
      const current = fs.existsSync(absolutePath)
        ? fs.readFileSync(absolutePath, "utf-8")
        : null;

      if (current === next) {
        return;
      }

      changed.push(relativePath);
      if (!check) {
        fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
        fs.writeFileSync(absolutePath, next, "utf-8");
      }
    },

    finish() {
      if (!quiet) {
        if (changed.length === 0) {
          console.log("Release metadata is already in sync.");
        } else {
          const verb = check ? "Would update" : "Updated";
          for (const relativePath of changed) {
            console.log(`${verb} ${relativePath}`);
          }
        }
      }
      return changed;
    },
  };
}

function syncJson(writer, relativePath, update) {
  const original = readText(relativePath);
  const value = JSON.parse(original);
  update(value);
  writer.write(relativePath, stringifyJsonLike(original, value));
}

function syncText(writer, relativePath, update) {
  const original = readText(relativePath);
  const next = preserveNewline(original, update(original));
  writer.write(relativePath, next);
}

function syncCargoLock(writer, metadata) {
  syncText(writer, "Cargo.lock", (content) => {
    let found = false;
    const blocks = content.split(/(?=^\[\[package\]\]$)/m);
    const next = blocks
      .map((block) => {
        if (!/^name = "corpuswright-desktop"$/m.test(block)) {
          return block;
        }
        found = true;
        return replaceOrThrow(
          block,
          /^version = "[^"]+"$/m,
          `version = "${metadata.version}"`,
          "Cargo.lock",
          "corpuswright-desktop version"
        );
      })
      .join("");

    if (!found) {
      throw new Error("Could not find corpuswright-desktop package in Cargo.lock");
    }
    return next;
  });
}

function syncReadme(writer, metadata) {
  const cff = buildCitationCff(metadata).trimEnd();
  syncText(writer, "README.md", (content) => {
    let next = content;
    next = replaceOrThrow(
      next,
      /^\[!\[Release\]\([^)]+\)\]\([^)]+\)$/m,
      "[![Release](https://img.shields.io/github/v/release/jhlopesalves/CorpusWright?include_prereleases&label=release)](https://github.com/jhlopesalves/CorpusWright/releases/latest)",
      "README.md",
      "release badge"
    );
    const legacyDownload = /^\*\*Download (?:the experimental alpha|the latest experimental alpha):\*\* \[[^\]]+\]\([^)]+\)$/m;
    if (legacyDownload.test(next)) {
      next = next.replace(
        legacyDownload,
        "**Download the latest experimental alpha:** [CorpusWright releases](https://github.com/jhlopesalves/CorpusWright/releases/latest)"
      );
    } else {
      next = replaceOrThrow(
        next,
        /^\[Download CorpusWright\]\([^)]+\) ·$/m,
        "[Download CorpusWright](https://github.com/jhlopesalves/CorpusWright/releases/latest) ·",
        "README.md",
        "download link"
      );
    }
    next = next.replace(/\bdogfood alpha\.\d+ on\b/, "dogfood the current alpha on");
    const citationSnippet = /\*\*(?:`CITATION\.cff` template|Current `CITATION\.cff`):\*\*\r?\n```yaml\r?\n[\s\S]*?\r?\n```/;
    if (citationSnippet.test(next)) {
      next = next.replace(
        citationSnippet,
        `**Current \`CITATION.cff\`:**\n\`\`\`yaml\n${cff}\n\`\`\``
      );
    } else if (!/^\[Citation\]\(CITATION\.cff\)$/m.test(next)) {
      throw new Error("Could not find CITATION.cff link in README.md");
    }
    return next;
  });
}

function stripFooterVersion(writer, relativePath) {
  syncText(writer, relativePath, (content) =>
    content.replace(
      /\s*\(v\d+\.\d+\.\d+-alpha\.\d+\)(?=\.<\/span>)/g,
      ""
    )
  );
}

function syncIndex(writer) {
  syncText(writer, "docs/index.html", (content) => {
    let next = content;
    next = replaceOrThrow(
      next,
      /href="https:\/\/github\.com\/jhlopesalves\/CorpusWright\/releases\/(?:tag\/v\d+\.\d+\.\d+-alpha\.\d+|latest)">Download (?:alpha\.\d+|latest alpha)<\/a>/,
      'href="https://github.com/jhlopesalves/CorpusWright/releases/latest">Download latest alpha</a>',
      "docs/index.html",
      "download nav link"
    );
    // Citation examples are synchronised on their compatibility page; Home owns its prose.
    return next;
  });
}

function syncQuickStart(writer) {
  syncText(writer, "docs/quick-start.html", (content) => {
    let next = content;
    next = next.replace(
      /\bDownload alpha\.\d+ from the release page\./,
      "Download the latest alpha from the release page."
    );
    next = replaceOrThrow(
      next,
      /href="https:\/\/github\.com\/jhlopesalves\/CorpusWright\/releases\/(?:tag\/v\d+\.\d+\.\d+-alpha\.\d+|latest)">Download (?:alpha\.\d+|latest alpha)<\/a>/,
      'href="https://github.com/jhlopesalves/CorpusWright/releases/latest">Download latest alpha</a>',
      "docs/quick-start.html",
      "download button"
    );
    return next;
  });
}

function syncTechnicalDocs(writer) {
  syncText(writer, "docs/technical-docs.html", (content) =>
    replaceOrThrow(
      content,
      /<a href="https:\/\/github\.com\/jhlopesalves\/CorpusWright\/releases\/(?:tag\/v\d+\.\d+\.\d+-alpha\.\d+|latest)">(?:v\d+\.\d+\.\d+-alpha\.\d+|Latest alpha release)<\/a>/,
      '<a href="https://github.com/jhlopesalves/CorpusWright/releases/latest">Latest alpha release</a>',
      "docs/technical-docs.html",
      "latest release link"
    )
  );
}

function syncRoadmap(writer) {
  syncText(writer, "docs/roadmap.html", (content) => {
    let next = content;
    next = next.replace(
      /CorpusWright alpha\.\d+ covers/,
      "The current public alpha covers"
    );
    next = next.replace(/\bDogfood alpha\.\d+ on\b/, "Dogfood the current alpha on");
    next = next.replace(
      /\bpublic alpha\.\d+ builds\b/,
      "current public alpha builds"
    );
    return next;
  });
}

function syncCitationPage(writer, metadata) {
  const year = releaseYear(metadata);
  syncText(writer, "docs/citation.html", (content) => {
    let next = content;
    next = replaceOrThrow(
      next,
      /<p><strong>Lopes, J\. H\.[\s\S]*?<\/strong><\/p>/,
      `<p><strong>Lopes, J. H. (${year}). <em>CorpusWright</em> (Version ${metadata.version}) [Computer software]. <a href="${REPOSITORY_URL}">${REPOSITORY_URL}</a></strong></p>`,
      "docs/citation.html",
      "APA citation"
    );
    next = replaceOrThrow(
      next,
      /<pre class="code"(?: id="citation-bibtex")?><code class="mono">@software[\s\S]*?<\/code><\/pre>/,
      `<pre class="code" id="citation-bibtex"><code class="mono">${htmlEscape(buildBibtex(metadata))}</code></pre>`,
      "docs/citation.html",
      "BibTeX citation"
    );
    return next;
  });
}

function syncReleaseMetadata(options = {}) {
  const metadata = getReleaseMetadata();
  const writer = createWriter(options);

  syncJson(writer, "apps/desktop/src-tauri/tauri.conf.json", (value) => {
    value.version = metadata.version;
  });
  syncText(writer, "apps/desktop/src-tauri/Cargo.toml", (content) =>
    replaceOrThrow(
      content,
      /^version\s*=\s*"[^"]+"/m,
      `version = "${metadata.version}"`,
      "apps/desktop/src-tauri/Cargo.toml",
      "package version"
    )
  );
  syncJson(writer, "apps/desktop/package.json", (value) => {
    value.version = metadata.version;
  });
  syncJson(writer, "apps/desktop/package-lock.json", (value) => {
    value.version = metadata.version;
    value.packages[""].version = metadata.version;
  });
  syncCargoLock(writer, metadata);
  syncText(writer, "apps/desktop/index.html", (content) =>
    replaceOrThrow(
      content,
      /<p><strong>Version:<\/strong>\s*\d+\.\d+\.\d+-alpha\.\d+<\/p>/,
      `<p><strong>Version:</strong> ${metadata.version}</p>`,
      "apps/desktop/index.html",
      "About dialog version"
    )
  );

  const cffOriginal = fs.existsSync(rootPath("CITATION.cff"))
    ? readText("CITATION.cff")
    : "\n";
  writer.write(
    "CITATION.cff",
    preserveNewline(cffOriginal, buildCitationCff(metadata))
  );

  syncReadme(writer, metadata);
  syncIndex(writer);
  syncQuickStart(writer);
  syncTechnicalDocs(writer);
  syncRoadmap(writer);
  syncCitationPage(writer, metadata);

  for (const relativePath of [
    "docs/404.html",
    "docs/citation.html",
    "docs/index.html",
    "docs/method-and-measures.html",
    "docs/quick-start.html",
    "docs/roadmap.html",
    "docs/technical-docs.html",
  ]) {
    stripFooterVersion(writer, relativePath);
  }

  return writer.finish();
}

if (require.main === module) {
  syncReleaseMetadata();
}

module.exports = {
  ROOT,
  alphaLabel,
  buildBibtex,
  buildCitationCff,
  getReleaseMetadata,
  releaseTag,
  syncReleaseMetadata,
  versionPattern,
};
