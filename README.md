# CorpusAid

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)
![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Tauri](https://img.shields.io/badge/Tauri-24C8DB?logo=tauri&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white)
![Status](https://img.shields.io/badge/status-active%20development-blue)

**CorpusAid** is a desktop workbench for preparing research corpora for analysis.

It helps researchers load PDF, DOCX, TXT, and HTML files; inspect original and processed text; configure reproducible cleaning rules; search across selected documents; detect repeated artefacts such as headers, footers, boilerplate, page labels, and layout noise; and export cleaned UTF-8 text with metadata for downstream corpus-linguistic work.

The current version is a **Rust/Tauri rewrite** of an earlier PySide6 prototype. The legacy PySide application is preserved in the repository for historical continuity, but active development now focuses on the Rust/Tauri desktop app.

## What CorpusAid does

CorpusAid is designed for the practical, often messy stage before corpus analysis: getting documents into a clean, inspectable, reproducible text form.

Core features include:

* loading individual files or whole folders of corpus documents;
* extracting text from TXT, HTML, DOCX, and PDF files;
* previewing original and processed text side by side;
* configuring cleaning rules for whitespace, line breaks, repeated artefacts, HTML, DOCX, and PDF-specific noise;
* searching selected documents with backend-powered hit navigation;
* detecting repeated artefacts such as running headers, footers, page labels, boilerplate, and layout fragments;
* expanding grouped artefact candidates into exact raw variants for Custom Removals;
* exporting cleaned text files with manifest, warnings, and configuration artefacts;
* preserving a reproducible cleaning configuration through JSON load/save.

## Current application

The active desktop application lives in:

```text
apps/desktop/
```

It uses:

* **Rust** for extraction, cleaning, search, export, repeated artefact detection, and cache-backed corpus operations;
* **Tauri v2** for the desktop shell and Rust/TypeScript bridge;
* **TypeScript + Vite** for the frontend;
* a Rust core crate, `corpusaid-core`, shared by the desktop app and tests.

## Repository layout

```text
crates/corpusaid-core/      Rust library crate: extraction, cleaning, search, export, repeated artefacts
apps/desktop/              Tauri v2 desktop application with TypeScript/Vite frontend
legacy/pyside/             Original PySide6 implementation, preserved for reference
docs/                      Design notes and reference documentation
examples/                  Example corpora and usage material, when available
```

## Why this project exists

Corpus linguistics often starts with a frustrating reality: texts are messy.

PDFs come with running headers, page numbers, broken line wraps, OCR errors, tables, and all sorts of layout artefacts that end up mixed into the extracted text. DOCX files can bring their own issues, such as headers, footers, comments, footnotes, and formatting structures that are not always relevant to the corpus itself.

CorpusAid was built to make that preparation stage easier. The goal is to give researchers a practical way to inspect documents, clean unwanted noise, identify repeated artefacts, and export texts in a form that is ready for further analysis.

## Build and run

### Prerequisites

* Rust
* Node.js 20+
* npm
* Tauri system dependencies for your platform

### Desktop app

```bash
cd apps/desktop
npm ci
npm run tauri dev
```

### Frontend build

```bash
cd apps/desktop
npm run build
```

### Rust tests

From the repository root:

```bash
cargo test -p corpusaid-core
```

### Tauri backend check

```bash
cargo check -p corpusaid-desktop --all-targets
```

## Development status

CorpusAid is under active development.

The current Rust/Tauri version includes the main corpus loading, preview, cleaning, search, export, repeated artefact detection, and extraction-cache architecture. Some areas are still evolving, especially user-facing polish, packaging, documentation, and corpus-linguistic analysis features.

Planned or likely future work includes:

* original-vs-processed diff view;
* richer export summaries;
* frequency lists and corpus-linguistic diagnostics;
* more generated TypeScript bindings from Rust types;
* frontend module clean-up and maintainability improvements;
* improved documentation and screenshots.

## Legacy PySide application

The original PySide6-based version is preserved in:

```text
legacy/pyside/
```

It represents the first working implementation of CorpusAid and is kept for historical reference. The Rust/Tauri application is now the active version.

## License

CorpusAid is released under the MIT License.
