# corpus_preview Extension

This crate exposes native helpers for CorpusAid:

- `load_preview(path, limit)` streams a fixed-size preview of a document without blocking the GUI.
- `scan_directory(path)` walks the directory tree and returns supported text, HTML, experimental DOCX, and experimental PDF files quickly.
- `extract_html_text(path)` converts HTML documents to plain text.
- `extract_docx_text(path)` extracts plain text from DOCX `word/document.xml`, including accessible table text. DOCX support is experimental and limited to the main document body.
- `extract_pdf_text(path)` extracts embedded text from born-digital PDFs with `pdf-extract` and returns page-count/backend metadata. Rust PDF support is experimental and does not include OCR, Tika, or Pdfium.

## Building

```
python -m pip install maturin
maturin develop --release
```

The command builds the native extension and installs it into the current Python environment. On
Windows you may need to have the Rust toolchain (rustup) and a suitable MSVC build environment
installed.

If the extension is unavailable at runtime, CorpusAid automatically falls back to pure-Python
preview, HTML extraction, and experimental DOCX extraction loaders. PDF fallback orchestration
lives in the Python ingestion layer, not in this crate.
