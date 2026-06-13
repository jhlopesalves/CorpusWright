# CorpusWright
[![CI](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jhlopesalves/CorpusWright/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.1.0--alpha.3-blue)](https://github.com/jhlopesalves/CorpusWright/releases/tag/v0.1.0-alpha.3)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)


![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Local-first](https://img.shields.io/badge/local--first-desktop-blue)
![Corpus linguistics](https://img.shields.io/badge/corpus-linguistics-purple)


**Download the experimental alpha:** [CorpusWright v0.1.0-alpha.3](https://github.com/jhlopesalves/CorpusWright/releases/tag/v0.1.0-alpha.3)

> Experimental alpha. Windows and macOS builds are on the Releases page, but they're unsigned and your OS will probably warn you before letting them run.


CorpusWright is a Rust/Tauri rewrite of an earlier PySide6 prototype. The old PySide app is still in the repo under `legacy/pyside/` for reference, but all active development now happens in the Rust/Tauri version.


## Why it exists

Corpus work usually starts with a tedious problem: text files are _mess_ and _chaos_. PDFs arrive with running headers, page numbers, broken line wraps, OCR slip-ups, and stray table fragments baked straight into the extracted text. DOCX files smuggle in footnotes, comments, and headers you never asked for. Before you can count or analyse anything, someone has to strip all of that out - reproducibly, across a few hundred files - and doing it by hand is miserable.

CorpusWright is my attempt to make that stage less miserable: load the documents, see exactly what was extracted, configure cleaning rules you can save and re-run, and export plain UTF-8 text that's actually ready for analysis.

## What it does

- Loads single files or whole folders — TXT, HTML, DOCX, and PDF.
- Shows original and processed text side by side, so you can see what each rule actually changed rather than trusting it blindly.
- Lets you configure cleaning rules for whitespace, line breaks, format-specific noise (HTML, DOCX, PDF), and your own custom removals.
- Finds repeated artefacts — running headers, footers, page labels, boilerplate, layout fragments — across the whole corpus, then lets you promote the ones you choose into removal rules.
- Searches across selected documents with backend-powered hit navigation.
- Exports cleaned text alongside a manifest, a warnings file, and the exact configuration used.

Cleaning configurations save and load as JSON, so a corpus you cleaned today can be cleaned identically six months from now.

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

This is alpha, and it behaves like alpha. The core pipeline — loading, preview, cleaning, search, export, repeated-artefact detection, extraction caching — is in place and working. The rough edges are mostly in packaging, UI polish, documentation, and the higher-level corpus-linguistic features.

Next on my list:

- an original-vs-processed diff view;
- richer export summaries;
- frequency lists and other corpus diagnostics;
- more TypeScript bindings generated from the Rust types;
- a frontend tidy-up;
- proper screenshots and documentation.

## Legacy PySide version

The first working version was built with PySide6 and lives in `legacy/pyside/`. It's kept for historical reference only — the Rust/Tauri app has replaced it.

## License

MIT. Bundled third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
