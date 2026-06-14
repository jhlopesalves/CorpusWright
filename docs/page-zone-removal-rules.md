# Page-Zone Removal Rules

Page-zone scoped Custom Removals would allow a structured rule to remove a
matching whole-line artefact only when it appears in a page header or footer
zone. This is useful for repeated running titles, page labels, and publisher
boilerplate that can also appear legitimately in body text.

CorpusWright does not currently expose page-zone scoped `RemovalRule` behaviour.
The core has a shared page-aware document model for internal line metadata, and
the implementation has enough page awareness for PDF-specific cleanup and
repeated artefact review, but not for generic structured Custom Removal
application.

## Current page-aware document model

The Rust core represents page-aware text with `StructuredDocument`,
`DocumentPage`, and `DocumentLine`. A structured document contains pages in
order. Each line stores its text, zero-based page index, zero-based line index
within the page, total line count for that page, and separate `is_page_top` and
`is_page_bottom` flags.

The page edge width is 3 lines, matching the repeated artefact scanner's
existing first 3 and last 3 line convention. The flags are intentionally
separate because short pages can contain lines that are both in the top zone
and in the bottom zone.

Flat text can be represented as one page for internal workflows that need a
line container without pretending that paragraph breaks are page breaks. The
model can also flatten itself deterministically by joining page lines with
single newlines and pages with blank lines.

## Current page information in extraction

PDF embedded-text extraction builds page-local line lists before joining the
document into text. The PDF cleanup layer receives a
`Vec<(usize, Vec<String>)>` representation and uses the first 3 and last 3
lines of each page for built-in repeated header/footer and page-label cleanup.
The extraction cache stores the resulting flat text and, when native PDF
extraction exposes page chunks, the corresponding page text list.

OCR extraction also produces page text page by page and joins non-empty pages
with blank lines. Cached OCR extraction results preserve the page text list
when OCR completes through the page-by-page path.

For cached or already extracted PDF text, `clean_extracted_pdf_text` reconstructs
page chunks by splitting on `\n\n`. That convention is intentionally used by the
PDF extraction path, but it is still a text convention rather than durable line
metadata attached to each line.

## Current page information in the scanner

The Repeated Artefact Finder builds a `StructuredDocument` for each scanned
file. When extraction or the cache provides page texts, the scanner builds the
structured document from those pages and can report page number, line index
within the page, and total lines on the page. When only flat text is available,
the scanner uses the structured document as a flat line container and falls back
to a file-level position estimate rather than treating blank lines as reliable
page metadata.

That metadata is used for diagnostics and scoring. It is not saved into the
cleaning configuration and is not passed to `clean_text` when Custom Removals
are applied.

## Current page information in the cleaner

General cleaning is performed by:

```rust
clean_text(text: &str, config: &CleaningConfig) -> String
```

At this point the cleaner receives a flat string. `remove_structured_removal_rules`
collects enabled `RemovalScope::WholeLine` rules and removes matching lines. It
does not know the source document type, page number, line index within a page,
or whether a line is in the top, middle, or bottom of a page.

Export, preview, search, word count, and processed repeated-artefact scans all
route text through the same broad shape: extract text, apply PDF-specific
cleanup for PDFs when configured, then call `clean_text` for general cleaning.
After the PDF cleanup step returns a string, structured Custom Removal rules no
longer have page-line metadata available.

Frontend configuration stores `remove_patterns` and `removal_rules`, but it does
not carry per-document page spans or line context.

## Why naive page zones are unsafe

A page-zone rule cannot safely be implemented inside `clean_text` by treating
blank lines as page breaks for all inputs. Plain text, HTML, DOCX, OCR output,
and already processed text can contain blank lines for paragraph structure or
formatting. Removing header/footer-scoped rules based on those blank lines would
make ordinary paragraphs look like pages.

It is also unsafe to use the first or last 10 percent of a flat file and call
that page-aware. The repeated artefact scanner uses a file-level estimate only
as an advisory diagnostic for non-PDF files. That estimate is not precise enough
to drive deletion.

Reusing PDF blank-line page chunks for Custom Removals without a stronger
contract would also create uneven behaviour across cache, preview, export,
search, and word count paths. Built-in PDF cleanup already uses the convention,
but serialised user rules need a clearer guarantee because they may be saved and
reused across corpora and formats.

## Required architecture

Page-zone Custom Removals need a page-aware rule application layer. The shared
document model provides this shape:

```text
document
  page
    line text
    page index
    line index within page
    total lines on page
    top and bottom page-zone flags
```

The extraction cache preserves compact page text for native PDF extraction and
OCR extraction when that page text is available, instead of storing only a flat
text string for downstream processing. A companion to `clean_text`, or a
refactored cleaner, can then apply structured whole-line rules with reliable
line context before flattening the result for display, search, word count, and
export.

Generic text formats should receive page-zone behaviour only when their page
boundaries are explicit and intentionally represented. Otherwise page-zone
rules should be ignored or rejected for that input rather than guessed from
paragraph spacing.

## Future implementation plan

1. Route preview, export, search, word count, and processed scans through a
   common page-aware processing path for documents that have page metadata.
2. Pass structured page-aware text into Custom Removal rule application before
   flattening the cleaned output.
3. Add `PageTop`, `PageBottom`, and `PageTopOrBottom` structured scopes, using
   the same first 3 and last 3 line definition as current PDF cleanup.
4. Apply page-zone scopes only to whole-line `Literal` and `NormalizedLine`
   matchers.
5. Keep `remove_patterns` unchanged and keep `WholeLine` behaviour unchanged.
6. Generate TypeScript bindings for the new scope variants and update frontend
   config validation.
7. Add focused tests for top, bottom, top-or-bottom, middle-line preservation,
   literal matching, normalised matching, legacy config loading, and
   serialisation.

Promotion from repeated artefact candidates can remain conservative after the
scopes exist. Exact and normalised candidates can continue to promote to
`WholeLine` until the UI can show the page-zone decision clearly and tests cover
the full review path.
