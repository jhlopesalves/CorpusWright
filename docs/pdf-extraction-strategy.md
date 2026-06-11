# PDF Extraction Strategy

This document explains the technical challenges of born-digital PDF text extraction and details the architectural decisions implemented in CorpusAid.

---

## Technical Context & Challenges

### 1. The Glyphs/CMap Problem (Why `lopdf` Fails)
Low-level PDF extraction libraries like `lopdf` extract raw text streams directly from content objects. However, many PDFs do not use standard encodings. Instead, they rely on custom font Subsets with embedded `ToUnicode` CMaps mapping character indexes to actual Unicode points.
Without fully loading and rendering the PDF's fonts and decoding CMaps, low-level stream readers will extract glyph indexes or raw bytes, producing gibberish characters, boxes, or replacement characters like `\u{FFFD}`.

**Solution:** We use **PDFium**, which contains a complete, battle-tested PDF rendering engine. PDFium automatically processes embedded font CMaps, resolves glyph encoding, and returns fully decoded Unicode character strings.

### 2. Form XObjects (Why `page.objects()` is Unsafe)
A prior layout-aware parser iterated over top-level layout objects using `page.objects()`. However, PDFs often group text sections inside nested objects called **Form XObjects** (useful for reusable components like front-matter blocks, graphics, or complex sidebars).
Iterating only top-level page objects completely ignores characters grouped inside Form XObjects, creating significant text gaps (e.g. dropping front-matter pages entirely).

**Solution:** We query `text_page.chars()`, which extracts characters from all text contexts on the page, including text nested within Form XObjects.

### 3. Reading Order Complexity
Unlike formats with flowable layouts (HTML/DOCX), PDFs are purely drawing instructions. Text is placed at absolute (X, Y) coordinates in arbitrary drawing order. For example, a PDF generator might draw a page header first, then the footer, then the left column, then the right column, or even write out paragraphs out of order.
Naive stream readers simply dump characters in the order they are defined in the PDF file, which frequently scrambles multi-column layouts, code listings, sidebars, or headers/footers.

### 4. Midpoint Two-Column Scrambling (Why it is Experimental)
Heuristics that calculate page midpoints and partition characters into columns are extremely fragile. They fail on pages with wide margins, block quotes, Table of Contents (TOC) pages with dot leaders, or copyright pages. On these pages, a two-column parser will incorrectly split single-line elements into two fragments, scrambling the reading order.

**Solution:** We default to `PdfiumFlat` (the standard stream order) or `PdfiumVisualSingleColumn` (which groups lines by Y coordinates and sorts left-to-right). The two-column parser (`PdfiumVisualColumnsExperimental`) is kept strictly internal, experimental, and warning-heavy.

---

## Supported Extraction Strategies

### `PdfiumFlat` (Default)
Extracts characters in the raw stream order reported by PDFium. This matches the behavior of standard viewers and is the safest, most stable default choice.

### `PdfiumVisualSingleColumn` (Recommended for prose and code blocks)
Reconstructs lines visually:
1. Groups characters into lines using Y-midpoint coordinates (with a tolerance threshold of `(line_height * 0.4).max(4.0)`).
2. Sorts lines vertically from top to bottom.
3. Sorts characters within each line horizontally from left to right.
4. Programmatically reconstructs spaces using horizontal coordinate gaps to handle PDFs generated without explicit space characters (e.g., LaTeX/TeX documents).

### `PdfiumVisualColumnsExperimental` (Experimental)
An experimental strategy for two-column page layouts. Stubbed as unsupported in this pass to prevent reading order regression.

---

## Local Benchmarking

To benchmark extraction speed, character yields, and reading order correctness:
1. Create a directory named `.local-corpora/pdf-benchmarks/` at the workspace root.
2. Put test PDF documents into that folder.
3. Run the benchmark tool:
   ```bash
   cargo run --example pdf_bench
   ```
This example will run all strategies and print a markdown comparison table. It will gracefully skip execution if the benchmark folder is empty or absent.

---

## Thread Safety

PDFium is **not thread-safe**. Because Rayon processes files in parallel, all PDFium FFI calls are synchronized using a global static `PDFIUM_LOCK` mutex. 
To avoid thread contention, the lock is acquired **only** during FFI calls (loading document, retrieving pages, and collecting character arrays). Layout sorting, spacing reconstruction, diacritics cleaning, and quality evaluation are executed outside the lock.

---

## Optional PDF Cleanup Strategy

To clean up artifacts specific to PDF documents (such as running headers, footers, and page numbers/labels) without modifying the original source files, CorpusAid provides optional, deterministic, page-aware PDF post-extraction cleanup options.

### 1. Repeated Header/Footer Removal (`remove_repeated_pdf_headers_footers`)

- **Algorithm:**
  1. For each page in the PDF (or the subset of pages extracted under character limits), candidate lines are collected from the top $N = 3$ lines and the bottom $N = 3$ lines.
  2. Candidate lines are normalized for matching by:
     - Trimming whitespace;
     - Stripping surrounding punctuation (hyphens, dashes, brackets, parentheses, dots, commas, asterisks, slashes, etc.) at the start/end;
     - Collapsing internal spaces;
     - Converting to lowercase;
     - Replacing all digit runs with a placeholder (`#`).
  3. Unique normalized lines per page are counted across pages.
  4. If a normalized candidate pattern appears on **at least 3 pages** and in **at least 50%** of all pages, it is marked as a repeated header/footer pattern.
  5. The matching lines are removed **only** from the top/bottom zones (top 3 and bottom 3 lines) of the pages. Body text remains untouched.
  6. A descriptive warning is logged detailing how many pages had headers/footers removed and the specific patterns matched.

- **Limitations:**
  - Legitimate repeated lines (e.g. standard short sections or list separators) might rarely be removed if they look like headers/footers and appear within the top 3 or bottom 3 lines of pages.
  - Varying headers (e.g., changing chapter titles on alternate pages) may not be removed if they do not meet the 50% occurrence threshold across the document.
  - Extremely short documents (fewer than 3 pages) are skipped to remain conservative.

### 2. Page Label Removal (`remove_pdf_page_labels`)

- **Algorithm:**
  - Standard page label patterns are matched inside the top $N = 3$ and bottom $N = 3$ lines of pages.
  - Page labels include:
    - Pure Arabic numbers (e.g., `12`, `104`);
    - Standard prefixes (e.g., `Page 12`, `p. 12`);
    - Bracketed or dashed forms (e.g., `- 12 -`, `— 12 —`);
    - Slashes and "of" ranges (e.g., `12 / 40`, `Page 12 of 40`);
    - Roman numeral page labels (e.g., `i`, `iv`, `xii`, `Page iv`) verified against a standard Roman numeral grammar.
  - Matches are removed **only** from the top/bottom zones.
  - Numbers embedded inside normal prose (e.g., "Chapter 12. Introduction") are safely preserved.

### 3. Symbol-Heavy Graphical Noise Removal (`remove_pdf_symbol_heavy_artifacts`)

- **Algorithm:**
  1. For each page, every extracted line is analyzed and scored.
  2. A line is marked as a removable symbol-heavy graphical artefact if it consists entirely of graphical markers and whitespace (regardless of length), OR if **all** of the following basic thresholds are met:
     - **Trimmed Length:** The trimmed character length of the line is $\ge 7$. This avoids removing short legitimate text labels.
     - **Symbol/Punctuation Ratio:** The ratio of non-alphanumeric characters (symbols/punctuation) to total non-whitespace characters in the line is $\ge 0.70$.
     - **Alphabetic Ratio:** The ratio of alphabetic characters to total non-whitespace characters is $\le 0.20$.
     - **Low Word-Like Token Count:** The count of "word-like" tokens (tokens containing at least one letter or digit) is $\le 1$.
  3. In addition to the basic thresholds, the line must satisfy **at least one** of the following repeated-marker conditions:
     - Contains **at least 5** graphical marker glyphs (such as `●`, `•`, `·`, `○`, `■`, `□`, `▲`, `△`, `◆`, `◇`).
     - Contains **one** specific symbol/punctuation character dominating $\ge 70\%$ of the non-whitespace content.
     - Contains an **obvious repeated symbol run** of $\ge 4$ identical non-alphanumeric characters (whitespace-collapsed, e.g. `● ● ● ●`, `---------`, `********`).
  4. Consecutive removed lines are grouped/folded into a single "artefact block" per page to keep logs and statistics concise.
  5. Descriptive warnings are generated:
     - If removals occurred on only one page: `"Removed X symbol-heavy PDF graphical artefact lines (folded into Y blocks) from page Z."`
     - If removals occurred across multiple pages, they are aggregated: `"Removed X symbol-heavy PDF graphical artefact lines across Y pages. Example pages: A, B, C."`

- **Limitations:**
  - This is a line-level heuristic designed to catch dense plot markers or line separators; it does not perform semantic figure or image detection.
  - Figure captions, mathematical/statistical equations (e.g. `y = a + bx`, `x + y / z`), file paths, and normal prose with bullet lists are preserved by design.
  - Some sparse axis labels or legends might not be removed if they do not meet the symbol ratio/dominance thresholds.

---

## Pipeline Integration

- **Original Text tab:** Shows default PDF extraction without optional cleanup.
- **Processed Text tab & Export:** Applies PDF cleanup options (if enabled) first on the page/line representation, followed by general text-cleaning operations (lowercase, unicode normalization, diacritics replacement, etc.).
- **Search:** Uses the active extraction and cleanup settings configured in `CleaningConfig` for consistency.
- **Word Count Command:** Does not have config context in the Tauri command API and defaults to the safe fallback strategy (`PdfiumFlat` with cleanup disabled).

> [!NOTE]
> **Extraction Backend vs. Cleanup Defaults:**
> Required extraction choices (such as the default PDF extraction strategy `PdfiumFlat`) are active by default to ensure the format is readable. Optional post-extraction cleanup and layout-scrambling options remain strictly disabled by default.

