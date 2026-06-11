# corpusaid-core

`corpusaid-core` is the Rust backend spike for CorpusAid 2. It provides core functionality for corpus compilation and text processing in a UI-agnostic manner, ready to be integrated with Tauri.

## Supported Formats

Currently, the following file formats are supported for scanning and processing:

- `.txt`
- `.html`
- `.htm`

## Unsupported Formats

The following formats are not supported in this core spike:

- PDF
- DOCX
- OCR
- Apache Tika integration
- NLP capabilities

## Main Capabilities

- **Recursive directory scanning**: Discovers supported files in a directory tree.
- **Bounded preview generation**: Extracts text for preview, ensuring fast reads and memory safety with bounded character limits.
- **Processed preview generation**: Demonstrates how cleaned text will look before exporting.
- **Deterministic cleaning rules**: Applies standard text cleaning based on simple, reproducible configurations (e.g. line ending normalization, lowercasing, find/replace).
- **Safe export**: Writes processed text corpus along with a `manifest.json`, `config.json`, and `warnings.json`.

## Main Public API

- `scan_directory`: Recursively scans a path for supported documents.
- `preview_files`: Generates a bounded raw text preview of files.
- `preview_processed_files`: Generates a bounded preview with text cleaning rules applied.
- `clean_text`: Processes a string according to cleaning rules.
- `export_corpus`: Safely exports the corpus to an output directory.

## Examples

You can find minimal code examples under the `examples/` directory.

To test scanning and preview:

```bash
cargo run --example scan_preview -- <directory>
```

To test exporting a corpus:

```bash
cargo run --example export_corpus -- <input_dir> <output_dir>
```

## Validation Commands

Run the test suite:

```bash
cargo test
```

Run the scan and preview example:

```bash
cargo run --example scan_preview -- <directory>
```

Run the export example:

```bash
cargo run --example export_corpus -- <input_dir> <output_dir>
```

## Safety Principles

1. **Source files are never modified**: All processing occurs in-memory or by writing new files to an export directory.
2. **Preview is bounded**: The maximum characters read and displayed per file is limited, preventing out-of-memory errors on massive files.
3. **Export writes UTF-8 `.txt` files**: Cleaned documents are guaranteed to be valid UTF-8.
4. **Export writes metadata**: A complete manifest, config log, and warnings log are always generated alongside the exported files.
5. **Unsupported formats are ignored at scan stage**: Unrecognized extensions are cleanly ignored and logged.

## Known Limitations

- Raw HTML preview for now (tags are not parsed out or styled).
- No PDF/DOCX support.
- No OCR.
- No Tika backend.
- No regex cleaning yet.
- No NLP features.
