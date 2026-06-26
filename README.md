# CorpusWright
[![CI](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.1.0--alpha.7-blue)](https://github.com/jhlopesalves/CorpusWright/releases/tag/v0.1.0-alpha.7)
[![Docs](https://img.shields.io/badge/docs-website-006cd6)](https://jhlopesalves.github.io/CorpusWright/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Local-first](https://img.shields.io/badge/local--first-desktop-blue)
![Corpus linguistics](https://img.shields.io/badge/corpus-linguistics-purple)

A local-first desktop workbench for cleaning, inspecting, and analysing text corpora — with every step visible and reproducible.

**Download the experimental alpha:** [CorpusWright v0.1.0-alpha.7](https://github.com/jhlopesalves/CorpusWright/releases/tag/v0.1.0-alpha.7)

> Experimental alpha. Windows and macOS builds are on the Releases page, but they're unsigned, so your OS will probably warn you before letting them run.

CorpusWright is a Rust/Tauri rewrite of an earlier PySide6 prototype. The old PySide app is still in the repo under `legacy/pyside/` for reference, but all active development now happens in the Rust/Tauri version.

## Documentation

Full documentation: **https://jhlopesalves.github.io/CorpusWright/**

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

This is alpha, and it behaves like alpha. The core pipeline - loading, preview, cleaning, search, export, repeated-artefact detection, extraction caching - is in place and working. The rough edges are mostly in packaging, UI polish, documentation, and the higher-level corpus-linguistic features.

Next on my list:

- an original-vs-processed diff view;
- richer export summaries;
- frequency lists and other corpus diagnostics;
- more TypeScript bindings generated from the Rust types;
- a frontend tidy-up;
- proper screenshots and documentation.

## Legacy PySide version

The first working version was built with PySide6 and lives in `legacy/pyside/`. It's kept for historical reference only — the Rust/Tauri app has replaced it.

## How to cite

If CorpusWright contributes to your research or teaching, please cite it. The repository contains a machine-readable [`CITATION.cff`](CITATION.cff) file.

To obtain the correct citation format for the version you are using:
- Use GitHub's **“Cite this repository”** widget on the right-hand sidebar of the repository page, which automatically exports APA and BibTeX formats.
- Alternatively, read the metadata directly from the [`CITATION.cff`](CITATION.cff) file.

**`CITATION.cff` template:**
```yaml
cff-version: 1.2.0
title: CorpusWright
message: "If you use CorpusWright in your research or teaching, please cite it."
authors:
  - family-names: Lopes
    given-names: Jhonatan Henrique
    orcid: "https://orcid.org/0009-0008-4914-0262"
version: [version]
repository-code: "https://github.com/jhlopesalves/CorpusWright"
```

For further guidance (including how to reference version-specific DOIs via Zenodo once minted), see the [Citation Guidance on our documentation site](https://jhlopesalves.github.io/CorpusWright/citation.html).

## Connect

- Documentation — https://jhlopesalves.github.io/CorpusWright/
- ORCID — https://orcid.org/0009-0008-4914-0262
- Substack — https://substack.com/@jhonatanlopes

## License

MIT. Bundled third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
