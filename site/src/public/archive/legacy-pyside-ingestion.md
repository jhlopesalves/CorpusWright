> **Archived document.** This file describes the legacy PySide prototype, not the current Rust/Tauri application.

# Document Ingestion

The archived PySide prototype has an ingestion layer in
`legacy/pyside/src/corpusaid/ingestion/`. The GUI should stay format-agnostic:
it asks ingestion which paths are
supported, loads previews through ingestion, and receives extracted plain text
plus provenance.

## Supported Formats

The default ingestion service currently registers:

- TXT: `.txt` - stable
- HTML: `.html`, `.htm` - stable plain-text extraction
- DOCX: `.docx` - experimental plain-text extraction for main document body
- PDF: `.pdf` - experimental embedded-text extraction for born-digital PDFs

Extension matching is case-insensitive. Legacy `.doc` files are intentionally
unsupported and should fail with `UnsupportedDocumentTypeError` at the ingestion
service boundary.

EPUB, OCR, Pdfium, and Tika jar lifecycle management are not implemented in this
branch. Apache Tika can be used only as an optional experimental server fallback
when explicitly configured.

## GUI Exposure

The GUI file picker, drag-and-drop filtering, and directory scan paths derive
supported extensions from the registered ingestion extractors. No GUI code
branches on DOCX or HTML specifically.

DOCX remains registered because the Python fallback is covered by local tests and
the native path is covered by CI, but its support level is marked
`experimental`. PDF is registered as experimental even when the native backend is
unavailable so the GUI can surface explicit backend-unavailable warnings.
Structured sources such as HTML, DOCX, and PDF are never overwritten with
processed plain text; processed output is exported as `.txt`.

## Extraction Methods

Each extractor returns an `ExtractedDocument` with source path, text, document
type, extraction method, structured warnings, and metadata.

Rust-native helpers are preferred when the optional `corpus_preview` extension
is installed:

- TXT preview/full text: `corpus_preview.load_preview` and
  `corpus_preview.load_full_text`
- HTML: `corpus_preview.extract_html_text`
- DOCX: `corpus_preview.extract_docx_text`
- PDF: `corpus_preview.extract_pdf_text`

When the native hook is unavailable or fails, Python fallbacks remain available
for TXT/HTML/DOCX. PDF uses the backend orchestration layer: Rust extraction is
attempted first, then an optional fallback is considered when the primary result
fails, is empty, is near-empty, or has suspiciously low text yield. Fallback
tests must not require the native extension.

PDF uses the pure-Rust `pdf-extract` crate because it can extract page-level
embedded text without Java/Tika, OCR engines, Pdfium binaries, or system library
packaging. Pdfium was deferred because shipping and validating its native
runtime would add packaging and CI complexity beyond the current lightweight
maturin extension path.

## Backend Orchestration And Quality

Backend attempts are recorded with backend name, success/failure, elapsed time,
support status, warnings, metadata, error text when applicable, and extracted
character count. Extracted documents include common quality metadata:

- `extracted_character_count`
- `non_whitespace_character_count`
- `line_count`
- `primary_backend`
- `chosen_backend`
- `fallback_attempted`
- `fallback_reason`
- `fallback_available`
- `backend_attempts`

For PDFs, empty extraction, near-empty extraction, suspected scanned/image-only
input, and low text yield emit structured warnings. Processed-corpus manifests
include this metadata through the existing export path.

## Optional Tika Server Fallback

Tika is experimental and disabled by default in the archived PySide ingestion
layer. It only calls a user-provided Tika server; it does not vendor binaries,
start a jar, require Java, or add Tika to `pyproject.toml`.

Environment variables:

- `CORPUSAID_TIKA_SERVER_URL`: enables fallback, for example
  `http://localhost:9998`
- `CORPUSAID_TIKA_ENDPOINT`: optional, defaults to `/tika`
- `CORPUSAID_TIKA_TIMEOUT_SECONDS`: optional, defaults to `10`

The fallback sends the source bytes with `PUT`, requests `text/plain`, and uses
the fallback text only when Tika succeeds and returns non-empty text. If Tika is
configured but unavailable or fails, the archived ingestion layer records
structured warnings and continues with the native result.

## DOCX Scope And Limits

DOCX extraction reads the main `word/document.xml` part only. The current Python
and Rust implementations extract paragraphs and simple table cell text.

Current limitations:

- no legacy `.doc` support
- no formatting preservation
- no EPUB, OCR, or Tika jar integration
- comments, footnotes, endnotes, headers, and footers are detected but not
  extracted into the body text
- tracked changes are detected and warned about, but not represented separately
- malformed ZIP packages, missing `word/document.xml`, or malformed document XML
  raise clear extraction errors

Warnings are structured `ExtractionWarning` values so callers can log or surface
them without parsing message text.

## PDF Scope And Limits

PDF extraction is experimental and unreliable by nature. The extractor is
intended for born-digital PDFs with embedded text. It preserves page boundaries
with form-feed markers when the native backend can provide page text, and records
metadata such as backend, native-backend usage, page count when available,
source type, support status, and extracted character count.

Known limitations:

- no OCR for scanned/image-only PDFs
- Tika fallback requires a user-configured server URL
- no Pdfium runtime
- reading order, columns, headers, footers, ligatures, and embedded text may be
  wrong or incomplete
- encrypted/password-protected PDFs are not supported
- empty extraction emits explicit warnings and may indicate scanned/image-only
  input

## Backend Consistency

Extractor outputs use stable document type names: `txt`, `html`, `docx`, and
`pdf`.
Python fallback methods are named `python:utf-8`, `python:utf-8-replace`,
`python:html.parser`, and `python:zip+xml`; Rust-backed methods are named after
their `corpus_preview` entry points. PDF uses
`rust:corpus_preview.extract_pdf_text` when native extraction is available,
`unavailable:corpus_preview.extract_pdf_text` when it is not, and `tika:server`
when optional Tika fallback produces the final text.

Metadata is still produced in Python even when native text extraction succeeds.
Common metadata keys include `extension` and `size_bytes`; HTML may add `title`;
DOCX adds `has_tables` and may add `unextracted_parts`; PDF adds
`source_type`, `support_status`, `backend`, `native_backend`,
`extracted_character_count`, fallback orchestration metadata, and native keys
such as `page_count`, `image_count`, and `page_separator` when available.

Text extraction returns normalised plain text. TXT preserves file text apart from
invalid UTF-8 replacement when needed. HTML removes script/style/noscript content
and normalises block boundaries. DOCX normalises paragraph/table cell whitespace
and uses ` | ` between visible table cells.
PDF joins page text with form-feed page separators when native extraction
returns page-level text.

## Manifest Foundation

`ExtractionManifest` is a small pure-Python provenance model for extraction-only
provenance today. It serialises to a JSON-compatible dictionary containing
optional app/project versions, source path, document type, extraction method,
warnings, metadata, extracted character count, and an optional SHA-256
source-file hash.

The planned integration path is:

- now: create manifests from `ExtractedDocument` when an extraction/export caller
  needs provenance
- later: add processed-corpus manifests that also record cleaning parameters,
  processed-output paths, and project-level export metadata

## Save Safety

Only extractors that declare in-place plain-text overwrite safety should be
saved back to their original source paths. Structured sources such as HTML,
DOCX, and PDF should be exported as processed `.txt` output instead of
overwriting the original package/markup/PDF file.

## Local Validation

From `legacy/pyside`:

```bash
python -m pytest -q
python -m compileall src
```

When Rust is available:

```bash
cd rust_preview
cargo fmt --check
cargo check
cargo test
cd ..
```

When Rust and maturin are available:

```bash
python -m pip install maturin
maturin build --manifest-path rust_preview/Cargo.toml --release --out dist
python -m pip install dist/*.whl
python -m pytest -q -m native tests/test_native_ingestion.py
```

On Windows PowerShell, install the built wheel with a small Python glob helper
instead of relying on shell wildcard expansion:

```powershell
python -c "import glob, subprocess, sys; wheels = glob.glob('dist/*.whl'); subprocess.check_call([sys.executable, '-m', 'pip', 'install', wheels[0]])"
```

## CI Expectations

GitHub Actions should validate:

- Python tests
- `python -m compileall src`
- `cargo fmt --check`
- `cargo check`
- `cargo test`
- maturin wheel build and install
- native extension import
- direct native `extract_html_text`
- direct native `extract_docx_text`
- direct native `extract_pdf_text`
- `HtmlExtractor` using the installed native backend
- `DocxExtractor` using the installed native backend
- `PdfExtractor` using the installed native backend

The native pytest module skips cleanly when `corpus_preview` is unavailable
locally, but should run after the maturin install step in CI. Native extension
validation runs on Ubuntu and Windows.
