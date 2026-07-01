# CorpusWright
[![CI](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jhlopesalves/CorpusWright?include_prereleases&label=release)](https://github.com/jhlopesalves/CorpusWright/releases/latest)
[![Docs](https://img.shields.io/badge/docs-website-006cd6)](https://jhlopesalves.github.io/CorpusWright/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Local-first](https://img.shields.io/badge/local--first-desktop-blue)
![Corpus linguistics](https://img.shields.io/badge/corpus-linguistics-purple)

A local-first desktop corpus workbench for cleaning, counting, concordancing, and exploring real-world text corpora — with every step visible and reproducible.

**Project website:** https://jhlopesalves.github.io/CorpusWright/ — the public GitHub Pages site for downloads, quick start, method and concepts, roadmap, citation guidance, and technical docs.

**Download the latest experimental alpha:** [CorpusWright releases](https://github.com/jhlopesalves/CorpusWright/releases/latest)

Latest release page: https://github.com/jhlopesalves/CorpusWright/releases/latest

> Experimental alpha. Release builds are on the Releases page. Windows installers may still trigger the usual unsigned-app warnings.

CorpusWright is a Rust/Tauri rewrite of an earlier PySide6 prototype. The old PySide app is still in the repo under `legacy/pyside/` for reference, but all active development now happens in the Rust/Tauri version.

## Documentation

The public GitHub Pages website is the best entry point for readers: **https://jhlopesalves.github.io/CorpusWright/**

Public website links:

- [Website home](https://jhlopesalves.github.io/CorpusWright/)
- [Quick start](https://jhlopesalves.github.io/CorpusWright/quick-start.html)
- [Learn / Method and concepts](https://jhlopesalves.github.io/CorpusWright/learn.html)
- [Roadmap](https://jhlopesalves.github.io/CorpusWright/roadmap.html)
- [Citation guidance](https://jhlopesalves.github.io/CorpusWright/citation.html)

Reference material lives in [`docs/`](docs/) and is linked in context throughout this README:

- [Corpus projects](docs/corpus-projects.md) — project folders and the `.corpuswright/` manifest
- [Supported formats](docs/supported-formats.md) — TXT, HTML, DOCX, and PDF extraction routes
- [PDF extraction strategy](docs/pdf-extraction-strategy.md) — embedded text, layout-aware text, and OCR
- [Structured custom removals](docs/structured-custom-removals.md)
- [Page-zone removal rules](docs/page-zone-removal-rules.md)
- [Repeated artefact review](docs/repeated-artifact-finder.md)

# Why it exists

Corpus work usually starts with a tedious problem: the text files are messy and chaotic. PDFs arrive with running headers, page numbers, broken line wraps, OCR slip-ups, and stray table fragments baked straight into the extracted text. DOCX files smuggle in footnotes, comments, and headers you never asked for. Before you can count or analyse anything, someone has to strip all of that out — reproducibly, across a few hundred files — and doing it by hand is miserable.

CorpusWright is my attempt to make that stage less miserable: load the documents, see exactly what was extracted, configure cleaning rules you can save and re-run, and export plain UTF-8 text that's actually ready for analysis.

## What it does

- Starts from a CorpusWright project folder, with a small manifest under `.corpuswright/project.json` and a recent-projects launcher.
- Keeps a project source inventory under `.corpuswright/sources.json`, shown as a Source Explorer with processing statuses, route badges, and reason badges.
- Reviews selected files or folders before import, with source-level extraction routes for TXT, HTML, DOCX, and PDF. PDF imports choose between standard embedded text, layout-aware embedded text, Full OCR all pages, or embedded text with no automatic OCR.
- Shows original and processed text side by side, so you can see what each rule actually changed rather than trusting it blindly.
- Lets you configure cleaning rules for whitespace, line breaks, format-specific noise (HTML, DOCX, conservative PDF cleanup), obvious extraction/OCR/markup noise lines, and your own custom removals.
- Reviews repeated headers, footers, page labels, boilerplate, layout fragments, and extraction artefacts in selected sources, or explicitly all loaded sources when no selection is active.
- Promotes selected exact and normalised repeated-line candidates into structured cleaning rules from Cleaning Settings.
- Uses page-aware repeated artefact evidence and page-zone scoped rules when reliable PDF/OCR page metadata is available; flat text paths do not invent page zones.
- Searches across selected documents with backend-powered hit navigation.
- Builds Corpus Overview / Frequency List tables for surface tokens, lemmas plus POS where annotation is available, and POS tags, with raw frequency, document range, DP/dispersion-aware defaults, and optional normalised rates.
- Provides alpha Concordance / KWIC workflows for surface and phrase search, KWIC lines, sorting by file/position, node, and left/right context positions, source preview, and document spread diagnostics.
- Provides alpha Collocates around the current query, including raw co-occurrence count, Near query/O11, corpus frequency/f_c, and association scores such as logDice.
- Keeps Collocates lazy and cached, so ordinary Search updates KWIC lines without immediately recomputing collocate tables.
- Uses canonical source decoding across preview, export, and word-count paths: strict UTF-8 first, then Windows-1252 fallback for legacy Western-European, Portuguese, and French-style text files.
- Records decode provenance in manifests, and includes decode policy in annotation/cache identity so stale text foundations can be detected.
- Exports cleaned text alongside a manifest, a warnings file, and the exact configuration used.

Cleaning configurations save and load as JSON, so a corpus you cleaned today can be cleaned identically six months from now.
CorpusWright project folders are described in [docs/corpus-projects.md](docs/corpus-projects.md).
Structured Custom Removals are described in [docs/structured-custom-removals.md](docs/structured-custom-removals.md).
Page-zone removal rules are described in [docs/page-zone-removal-rules.md](docs/page-zone-removal-rules.md).
Repeated artefact review signals are described in [docs/repeated-artifact-finder.md](docs/repeated-artifact-finder.md).
The obvious extraction/OCR noise option uses deterministic text/noise profiling and is disabled by default. It removes only full lines such as repeated symbol junk, `cid:` markers, replacement-character junk, and standalone markup fragments; headings, page labels, tables, formulae, code, and ordinary text are preserved.

The Source Explorer separates file type, extraction route, source text extraction, and cleaning state. `Ready to Extract` means the source is registered and has a route, but source text has not yet been materialised. `Needs OCR` marks PDFs that likely need OCR rather than treating them as failed. `Text Extracted` and `Text Extracted with Warnings` mean original source text is available for preview and search, but cleaned output is not implied. `Processed with Warnings` is reserved for processed text that has been generated with review signals, and `Failed` marks a hard blocker that needs user action or a code fix.

PDF handling and OCR quality are source import choices. Cleaning Settings keep advanced fallback defaults for older or unspecified PDF sources, plus conservative PDF cleanup for obvious page labels, symbol-noise lines, code-like blocks, and formula-like lines. Repeated headers and footers are best handled through Repeated artefact review, where accepted candidates become explicit cleaning rules.


## How it's built

The desktop app lives in `apps/desktop/`.

- **Rust** (`corpuswright-core`) does the real work: extraction, cleaning, search, export, repeated-artefact detection, and the extraction cache.
- **Tauri v2** provides the desktop shell and the Rust ↔ TypeScript bridge.
- **TypeScript + Vite** drive the frontend.

```
crates/corpuswright-core/   core library: extraction, cleaning, search, export, artefacts
apps/desktop/               Tauri v2 app + TypeScript/Vite frontend
legacy/pyside/              original PySide6 prototype, kept for reference
docs/                       design notes and reference documentation
examples/                   sample corpora and usage material
```

## Build and run

### Prerequisites

* Rust
* Node.js 20+
* npm
* Tauri system dependencies for your platform

Run the desktop app in development mode:

```bash
cd apps/desktop
npm ci
npm run tauri dev
```

Build the frontend:

```bash
cd apps/desktop
npm run build
```

Run the core tests and a backend check from the repository root:

```bash
cargo test -p corpuswright-core
cargo check -p corpuswright-desktop --all-targets
```

## Status

This is alpha, and it behaves like alpha. The core pipeline - loading, preview, cleaning, search, export, repeated-artefact detection, extraction caching - is in place and working. Alpha.8 also includes the first public analysis pass: Corpus Overview / Frequency List, surface and phrase KWIC, Collocates alpha, document spread diagnostics, and safer source decoding for UTF-8 and Windows-1252 text.

Known limits:

- Concordance and Collocates are functional alpha features, not final workflow design.
- Current analysis still uses scan and cached paths rather than the future positional index.
- CQL, N-grams, lemma/POS KWIC, and annotation-aware Concordance are not implemented yet.
- Collocates are useful alpha functionality, not the final indexed architecture.
- CorpusWright is local-first; it does not host corpora remotely.
- Windows installers may still be unsigned.

Next on my list:

- dogfood the current alpha on IMDB and Portuguese corpora;
- design and build the surface positional index;
- migrate KWIC and Collocates to the index behind parity gates;
- write the frontend/Analyse workspace state-model brief;
- shape the future one-workspace Concordance interaction model;
- add N-grams, CQL-lite, lemma/POS KWIC, richer Collocates, and export/copy workflows after the foundations are ready.

## Legacy PySide version

The first working version was built with PySide6 and lives in `legacy/pyside/`. It's kept for historical reference only — the Rust/Tauri app has replaced it.

## How to cite

If CorpusWright contributes to your research or teaching, please cite it. The repository contains a machine-readable [`CITATION.cff`](CITATION.cff) file.

To obtain the correct citation format for the version you are using:
- Use GitHub's **“Cite this repository”** widget on the right-hand sidebar of the repository page, which automatically exports APA and BibTeX formats.
- Alternatively, read the metadata directly from the [`CITATION.cff`](CITATION.cff) file.

**Current `CITATION.cff`:**
```yaml
cff-version: 1.2.0
message: "If you use CorpusWright in your research or teaching, please cite it as below."
title: CorpusWright
abstract: >-
  A desktop corpus-linguistics workbench with a Rust core for cleaning,
  inspecting, and analysing research corpora with statistical and
  methodological rigour. Local-first, auditable, and analysis-aware.
type: software
authors:
  - family-names: Lopes
    given-names: Jhonatan Henrique
    orcid: "https://orcid.org/0009-0008-4914-0262"
    email: jhlopesalves@gmail.com
repository-code: "https://github.com/jhlopesalves/CorpusWright"
url: "https://jhlopesalves.github.io/CorpusWright/"
version: 0.1.0-alpha.8
date-released: "2026-06-29"
license: MIT
keywords:
  - corpus linguistics
  - text processing
  - concordance
  - KWIC
  - reproducibility
  - Rust
```

For further guidance (including how to reference version-specific DOIs via Zenodo once minted), see the [Citation Guidance on our documentation site](https://jhlopesalves.github.io/CorpusWright/citation.html).

## Connect

- Documentation — https://jhlopesalves.github.io/CorpusWright/
- ORCID — https://orcid.org/0009-0008-4914-0262
- Substack — https://substack.com/@jhonatanlopes

## License

MIT. Bundled third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
