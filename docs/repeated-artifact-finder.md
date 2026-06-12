# Repeated Artefact Finder

The **Repeated Artefact Finder** is a corpus-wide diagnostic tool in CorpusWright designed to surface repetitive text lines and blocks (such as headers, footers, copyright boilerplate, extraction artefacts, and symbol-heavy noise) across your documents.

> **Diagnostic only.** This tool helps you *identify* potential artefacts so you can make informed decisions.
> - It does **not** modify any files.
> - There is no automatic deletion.
> - There is no automatic deletion button.
> - To remove a pattern, copy its text and add it to **Settings > Custom Removals** manually.

---

## Primary Workflow

1. **Open the Tool**: In the main menu, navigate to **Tools > Repeated Artefact Finder**.
2. **Configure the Scan**:
   - **Analyze Text Mode**: Choose whether to scan the **Original extracted text** (default, faster) or the **Processed text** (with your current Cleaning Config settings applied — slower).
   - **Candidate Types**: Toggle which patterns you want to discover (Exact lines, Normalised lines, 2-line blocks, or 3-line blocks).
   - **Thresholds**: Adjust the minimum occurrences, minimum distinct files, and maximum candidate limits.
3. **Run Scan**: Click **Run Scan**. The corpus will be parsed using a three-phase algorithm (parallel extraction → merge → rank). You can click **Cancel** to stop a running scan.
4. **Inspect Ranked Candidates**: Review the list of suspicious candidates in the left pane, sorted descending by their computed suspicion score.
5. **Inspect Examples & Context**: Click any candidate to view its estimated layout positions, advisory risk label, and sample occurrences with surrounding line context in the right pane.
6. **Take Action**:
   - Click **Copy Sequence** to copy the candidate's exact text to your clipboard.
   - Use the copied text in the **Custom Removals** section of the Settings modal to strip the sequence from future processed previews and exports.
   - Click **Export JSON** or **Export CSV** to save the full list of candidates for external analysis.

---

## Default Scan Settings (v1 — conservative)

The default scan is intentionally conservative to be fast and predictable:

| Setting | Default | Rationale |
|---|---|---|
| Analyse mode | Original extracted text | Avoids expensive PDF re-extraction and cleanup |
| Exact lines | Enabled | Fast, no extra allocation |
| Normalised lines | Enabled | Lightweight normalisation per valid line |
| 2-line blocks | **Disabled** | Opt-in — multiplies candidate space |
| 3-line blocks | **Disabled** | Opt-in — multiplies candidate space further |
| Min occurrences | 5 | Filters out rare noise |
| Min files | 2 | Requires corpus-wide presence (use ≥2 for cross-document artefacts) |
| Max candidates | 100 | Keeps results manageable |
| Max examples/candidate | 5 | Caps memory per candidate |
| Min line chars | 4 | Skips very short noise lines |
| Max line chars | 300 | Skips very long lines (paragraphs, code blocks) |

### Content-class filters

Each candidate is classified by its character composition into one of four content classes:

| Class | Default | Description | Examples |
|---|---|---|---|
| **Text** | Enabled | Predominantly alphabetic text | `Introduction`, `Methods`, prose lines |
| **Mixed text + numbers** | Enabled | Text with embedded numbers | `Page 12`, `Chapter 5`, `Figure 9.6` |
| **Numeric** | **Disabled** | Mostly digits and numeric formatting | `32.01 46.83`, `[0.386, 1.378]` |
| **Symbol / noise** | Enabled | Non-alphanumeric markers | `● ● ● ●`, `------`, `********` |

**Numeric-dominant candidates are disabled by default** because they can group unrelated statistical output, tables, formulas, axis ticks, or coefficients — making them appear as repeated artefacts when they are not.

Enabling numeric output will show these candidates with a caution banner:
> "Numeric-dominant candidate — review carefully. These may group unrelated tables, formulas, axis ticks, or statistical output."

### Normalised candidate safety

Normalised candidates are **grouping patterns**, not literal repeated strings. A normalised key like `page #` groups lines such as "Page 1", "Page 2", and "Page 3" that differ only in digit content.

**Numeric-dominant lines are not used for normalised grouping unless numeric output is explicitly enabled.** This prevents `#.# #.#` from being generated as a normalised pattern from unrelated decimal pairs.

Page/chapter/figure/table patterns remain useful examples of mixed text+number normalisation and work with default settings.

### Candidate deduplication

To avoid duplicate-looking rows, a `NormalizedLine` candidate with only one raw text variant (`raw_variant_count <= 1`) is suppressed when an equivalent `ExactLine` candidate exists with the same display text, occurrence count, and file count. Normalised candidates that genuinely group multiple distinct raw variants (e.g., "Page 1", "Page 2", "Page 3") remain visible.

### Why 2-line/3-line blocks are disabled by default

2-line and 3-line block detection can massively increase the candidate space. For a file with N valid lines, 2-line blocks produce up to N-1 additional candidates and 3-line blocks produce up to N-2. Across a corpus of thousands of files, this can cause pathological growth. Enable blocks only when you need to find multi-line artefacts.

---

## Candidate Types

- **Exact Repeated Lines**: Identifies lines that match character-for-character across the corpus.
- **Normalised Repeated Lines**: Group lines after applying a deterministic normalization filter:
  - Collapses multiple spaces.
  - Strips leading and trailing punctuation.
  - Replaces digit runs with a `#` symbol.
  - Converts text to lowercase.
  This is useful for grouping headers/footers with variable page numbers (e.g., `"--- Page 12 ---"` and `"--- Page 13 ---"` both normalize to `"page #"`).
- **Repeated 2-line Blocks**: Groups contiguous pairs of lines that recur in the same order. **Opt-in only.**
- **Repeated 3-line Blocks**: Groups contiguous triples of lines that recur in the same order. **Opt-in only.**
- **Repeated Inline Artefacts** (enabled by default): Detects repeated inline markup/conversion tokens such as `<br/>`, `&nbsp;`, `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;` that appear embedded within lines rather than as standalone lines. This catches common conversion leftovers like `<br/>` that the line-based candidate types miss.

### Why inline artefacts matter

HTML to plain-text conversion often leaves markup fragments embedded inside prose lines:

```
some text.<br/>more text
awful movie.<br/>the acting was great
```

These are not standalone lines, so line/block detection misses them. The inline artefact detector uses a bounded set of known patterns to find these without arbitrary substring mining.

---

## Advisory Risk Labels

The finder labels each candidate based on heuristics to guide your review. These labels are advisory review aids, not absolute truths:

- **Header/Footer (`StrongHeaderFooterCandidate`)**: High concentration of occurrences at the top or bottom of pages/documents.
- **Boilerplate (`PossibleBoilerplate`)**: High document coverage (occurs across multiple distinct files).
- **Heading (`CommonSectionHeadingReviewCarefully`)**: Matches common academic section names (e.g., "Introduction", "References"). Removing these might disrupt structural analysis, so review them carefully.
- **Noise (`SymbolOrNoiseCandidate`)**: Contains 50% or more non-alphanumeric/non-whitespace characters (such as dividers or OCR noise).
- **Review (`Ambiguous`)**: Pattern does not strongly fit the other categories and requires manual inspection.

---

## Layout & Position Estimation

To categorize headers and footers, the tool estimates layout positions:

- **PDF Files**: The tool splits text on `\n\n` (double newlines) as a fallback heuristic to infer page boundaries. Page numbers and page-level top/middle/bottom estimates are approximate/inferred and should not be treated as guaranteed.
- **Non-PDF Files**: Positions are estimated based on overall line percentage within the file (top 10% is "Top", bottom 10% is "Bottom", remainder is "Body"). This layout summary is documented as approximate.

---

## Algorithm & Performance

### v1 Algorithm (three-phase)

1. **Phase 1 (Parallel)**: Each file is processed independently by a Rayon worker. Text is extracted, valid lines are identified via early filtering (`should_skip_line`), and per-file candidate stats are **aggregated** in a local `HashMap` — one entry per candidate key per file, not one per occurrence. Normalisation is computed once per line and reused for both exact and normalised candidates.
2. **Phase 2 (Sequential merge)**: Local per-file stats maps are merged into a global `HashMap`. File presence is tracked without per-occurrence `Vec::contains()` — since each local map belongs to exactly one file, the merge increments per-key counts at file granularity.
3. **Phase 3 (Filter + score + rank + dedup)**: Candidates are filtered by `min_occurrences` and `min_files`, then duplicate-looking normalised candidates are removed (see dedup below). Survivors are scored, sorted, and truncated to `max_candidates`. Examples are collected only for surviving candidates.

### Cancellation

Scans are cooperatively cancellable. The scanner checks an `AtomicBool`:
- Before extracting each file
- After extracting each file
- After counting each file
- Between phases

Clicking **Cancel** or **Close** during a scan signals cancellation. The Rust thread returns `Err("Scan cancelled.")` which the UI displays as a clean "Scan was cancelled." message — not a crash or error.

### Deduplication of normalised candidates

A normalised candidate that has only one raw text variant (`raw_variant_count <= 1`) is redundant when an equivalent exact candidate exists (same display text, same occurrence and file counts). The dedup phase removes these redundant normalised rows from results, preventing duplicate-looking table entries.

Normalised candidates that genuinely group multiple raw variants (e.g., "Page 1", "Page 2", "Page 3" → `page #`) are preserved because `raw_variant_count > 1`.

### Content classification

Each candidate is classified by a deterministic `classify_content()` function that reads character composition:

- **NumericDominant**: ≥60% digit characters, or digit count > alpha count — includes decimal patterns, statistical output, axis ticks
- **MixedTextNumbers**: both alphabetic and digit characters present, alpha ratio ≥ digit ratio — includes "Page 12", "Chapter 5"
- **SymbolNoiseDominant**: ≥50% non-alphanumeric symbols — includes dividers, bullet markers
- **TextDominant**: everything else — prose, headings, boilerplate

This classification gates both exact and normalised candidates (see content-class filters above).

### PDF and OCR

**The Repeated Artefact Finder does not trigger OCR by default.** The PDF extraction call passes `use_ocr = false` regardless of analysis mode. This prevents silent OCR of 20+ PDFs during a scan.

PDF extraction is inherently slower than text file scanning, and the existing `PDFIUM_LOCK` global mutex serialises all PDF extraction calls for safety. Scans with many PDFs will be noticeably slower than text-only scans. This is documented behaviour.

### Memory safeguards

- Early filtering (`should_skip_line`) skips empty, whitespace-only, too-short, and too-long lines **before** any candidate key creation or HashMap insertion.
- Block candidates (2-line, 3-line) are **never created** when disabled — no wasted allocation.
- Examples are capped at `max_examples_per_candidate` (default 5) using compact reference tuples, not cloned strings.
- File tracking uses `u32` IDs rather than cloning full `PathBuf` into a `HashSet` per candidate.
- Processed-text mode is **off by default** to avoid expensive PDF re-extraction (pdfium + OCR).

### Parallel strategy

Each Rayon worker builds a **local** `HashMap` without any global locks. Maps are merged sequentially after all workers finish. This avoids the contention that a shared `Mutex<HashMap>` would create.

### Remaining limitations

- PDF extraction (even in original-text mode) still requires pdfium rendering, which is serialized by a global `PDFIUM_LOCK`.
- No true progress events from Rust to JS — the UI uses timer-based status stages.
- Cancellation is soft (Close discards stale results by generation counter) rather than truly interrupting the Rust thread.
- Content classification is a simple character-composition heuristic. Some edge cases (e.g., very short lines, mixed punctuation and digits) may be misclassified.
- Normalised candidate dedup uses exact match on occurrence/file counts; rare cases where counts differ by 1 may still show duplicates.

## Scan Modes and Recommended Workflow

### Original vs Processed Scans

- **Original extracted text** finds artefacts in raw extracted text before any processing.
- **Processed text with current settings** finds artefacts that remain after current Processing Parameters and Custom Removals are applied.

### Recommended Detection Workflow

1. Scan **Original extracted text** to discover all raw artefacts (e.g., `<br />`).
2. Select the artefact candidates and click **Add Selected to Custom Removals**.
3. Scan **Processed text with current settings** to verify the artefacts have been removed.
4. Confirm `<br />` no longer appears in the processed scan results.

If processed scan returns zero candidates and Custom Removals are active, this is expected — the artefacts have already been removed.

### Why processed scans may return fewer results

When you run a processed-mode scan, the scanner applies your current `CleaningConfig` (including `remove_patterns`) before analysing the text. If you have already added `<br />` to Custom Removals, the processed scan will correctly scan text where `<br />` has already been removed. This is expected behaviour. The UI displays contextual messages:

- A warning banner appears when processed mode is selected and Custom Removals are active.
- If processed mode returns zero candidates with active removals, a friendly note explains why.
