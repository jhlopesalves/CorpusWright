# Supported Formats

CorpusWright currently supports the extraction of plain text from the following document formats:

- `txt` (Plain Text)
- `html` / `htm` (Hypertext Markup Language)
- `docx` (Microsoft Word Document)
- `pdf` (Portable Document Format)

## Extraction Methods

CorpusWright does not modify original source files. Extraction is a read-only process, decoupled from text cleaning. The exact method used depends on the format.

By default, CorpusWright uses the required format readers needed to extract text. Optional cleanup transformations such as HTML text extraction, line cleanup, Unicode normalisation, conservative PDF cleanup, and custom removals are controlled by settings. PDF/OCR routes are normally stored during source import, with fallback settings used only for older or unspecified PDF sources.

The exact method used depends on the format:

- **Plain Text (`txt`)**: Extracted as plain text (`plain_text`). If invalid UTF-8 sequences are encountered, they are replaced with the Unicode replacement character ().
- **HTML (`html`, `htm`)**: Read as raw HTML text by default. If HTML text extraction is explicitly enabled, it is parsed via an optional HTML transformation that strips HTML tags and attempts to preserve basic block-level structure.
- **DOCX (`docx`)**: Read and extracted directly from the file structure using a required DOCX parser. Optional transformations like stripping headers, footers, footnotes, or endnotes are disabled by default.
- **PDF (`pdf`)**: Extracted using a required PDF character-level reader. Optional post-extraction cleanup transformations, such as conservative page-label cleanup, obvious symbol-noise cleanup, code-like block cleanup, and formula-like line cleanup, are disabled by default. Repeated headers and footers are normally handled through Repeated Artefact review so accepted candidates become explicit cleaning rules. OCR routes run according to stored source handling, and repeated artefact scans disable OCR.

## Limitations and Warnings

### DOCX
- **Experimental**: Support for DOCX is experimental.
- **Unsupported Elements**: Text boxes, drawings, and tracked changes (insertions/deletions) are currently unsupported and will result in an extraction warning.
- **Tables**: Table extraction strategies are configurable, but complex merged cells may not be perfectly preserved.

### PDF
- **Experimental**: Support for PDF is experimental.
- **Reading Order**: The reading order of extracted text is not guaranteed, as PDF layouts define elements by coordinates rather than a structured text flow. Formatting may be lost.
- **Scanned Documents**: CorpusWright can rasterise pages and run experimental OCR locally for PDF sources stored with OCR-based handling. This is significantly slower and probabilistic, but it prevents empty extractions for image-only PDFs. The Repeated Artefact Finder disables OCR during scans.
- **Encrypted PDFs**: Password-protected or encrypted PDFs are not supported and may result in an extraction error or garbage text.

### General
- If a document is completely empty or fails to produce text during extraction, a warning will be registered in the export report.
- Warnings can be viewed in the `warnings.json` file generated during export.
