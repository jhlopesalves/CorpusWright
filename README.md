# CorpusAid

[![MIT License](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)

CorpusAid is a desktop corpus preprocessing and cleaning workbench for corpus
linguistics. It lets researchers load thousands of documents (plain text, HTML,
DOCX, PDF), configure a reproducible cleaning pipeline, preview results, and
export processed text.

## Features

- Recursive folder scanning with metadata collection
- Configurable text cleaning (normalization, regex substitutions, HTML stripping, and more)
- Plain-text, HTML, DOCX, and PDF extraction (embedded text and OCR)
- PDF page quality assessment
- Repeated-artifact (boilerplate) detection
- Interactive preview workspace with bounded text snippets
- Full-text search across the corpus
- Word-count statistics
- Export of cleaned UTF-8 `.txt` files with manifest, warnings, and configuration artifacts

## Repository Layout

- `crates/corpusaid-core/` — Rust library crate with all extraction, cleaning, and analysis logic.
- `apps/desktop/` — Vite + TypeScript frontend and Tauri v2 desktop shell.
- `legacy/` — the original PySide6 application (see [Legacy PySide Application](#legacy-pyside-application)).
- `docs/` — design and reference documentation.
- `examples/` — sample corpora and usage examples.

## Prerequisites

- Rust >= 1.85
- Node.js >= 20

## Build & Run

```bash
cd apps/desktop
npm ci
npm run tauri dev
```

## Running Tests

```bash
cargo test -p corpusaid-core
```

## Legacy PySide Application

The original PySide6-based application lives in [`legacy/pyside/`](legacy/pyside). It
is preserved for reference and not required by the current Tauri-based build.
