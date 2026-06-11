# Technical Debt

## Processed corpus export

The Save action now avoids overwriting structured source files with plain text:
plain-text sources can still be backed up and overwritten in place, while
structured sources are exported as processed `.txt` files.

Remaining work is to design a proper processed-corpus export flow that can write
all processed documents, extraction manifests, and project metadata together.
That broader export UI was intentionally not built during this ingestion
stabilisation task.

## DOCX extraction completeness

DOCX is registered but experimental. Current extraction covers the main
`word/document.xml` body, paragraphs, and simple tables. It detects but does not
extract comments, footnotes, endnotes, headers, footers, or separate tracked
change state. Promote DOCX to stable only after those limitations are either
implemented or accepted as documented product scope with broader fixtures.

## PDF extraction hardening

PDF is registered but experimental. The current prototype uses the pure-Rust
`pdf-extract` backend for embedded text and records structured warnings when the
native backend is unavailable, extraction fails, output is empty, encryption is
detected, or the source appears scanned/image-only. It can optionally call a
user-configured Tika server as an experimental fallback, but it does not perform
OCR, start or manage Java, vendor a Tika jar, or include Pdfium fallback.

Future hardening should add broader fixtures for reading order, columns,
headers, footers, ligatures, encrypted files, scanned pages, and malformed PDFs.
OCR, Pdfium, jar-based Tika lifecycle management, and broader Tika format
registration should remain separate decisions with packaging and CI coverage
before being enabled.

## Legacy cleanup wording

Some preprocessing option labels still mention PDF-to-text artefacts. These are
text-cleaning descriptions, not ingestion support, and should be revisited when
the cleaning workflow copy is refreshed.
