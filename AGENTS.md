# AGENTS.md

Instructions for coding agents working on the CorpusWright repository. Read this before making changes.

## Validation

After making changes, and before committing, run these three commands from the repository root. All three must pass.

```bash
# Rust core library tests
cargo test --manifest-path crates/corpuswright-core/Cargo.toml

# Tauri backend check
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml

# Frontend build
npm --prefix apps/desktop run build
```

These run identically on Windows, macOS, and Linux. Don't wrap them in `powershell` or `cmd.exe` invocations — agents often run in a Linux sandbox, and shell-specific commands will fail there.

## Performance architecture contract

CorpusWright is a Rust application with a TypeScript interface. The Rust code is the corpus engine. The TypeScript code is the GUI layer.

Anything that scales with corpus size belongs in Rust.

This includes work that scales with:

* files or project sources
* file contents
* lines
* tokens
* types/terms
* repeated-artefact candidates
* concordance/KWIC hits
* n-grams
* search results
* export rows
* analysis tables

TypeScript may orchestrate the interface, hold small UI state, and render bounded visible data. TypeScript must not become the corpus engine.

### Rust owns corpus-scale work

Implement corpus-scale operations in Rust, exposed through Tauri commands or core-library APIs. This includes:

* source discovery and registration
* source inventory queries
* source counting and summaries
* source preview reads
* extraction and OCR orchestration
* cleaning and structural cleanup
* tokenisation
* analysis corpus construction
* frequency counting
* dispersion and per-document distributions
* repeated-artefact scanning
* KWIC/concordance search
* n-gram generation
* keyness and statistical summaries
* exports

When a frontend screen needs corpus-scale data, expose a Rust command that returns a bounded result: a page, a visible window, a summary, a cursor, or a progress update. Do not return the whole corpus to TypeScript so the frontend can filter, sort, count, or scan it.

### TypeScript renders bounded UI data

TypeScript may loop over and render:

* visible rows
* small option lists
* current-page results
* bounded preview text
* summary objects
* chart-ready aggregates
* user-interface controls

TypeScript must not loop over corpus-sized collections such as all sources, all tokens, all lines, all candidates, all terms, all concordance hits, or all n-grams.

A TypeScript implementation is not acceptable if it depends on rendering or repeatedly iterating over tens of thousands of sources, hundreds of thousands of rows, or millions of tokens.

### Source lists and tables must be windowed

Large lists must be queried and rendered as bounded windows.

Preferred response shape:

```text
{
  total_count,
  offset_or_cursor,
  limit,
  items,
  summary
}
```

The frontend should render the visible window plus a small overscan buffer. It must not create one DOM node per source, token, candidate, or result row.

Virtualisation is a rendering technique, not a substitute for Rust ownership. A virtualised frontend over a corpus-sized TypeScript array is only a temporary compromise. The target architecture is Rust-backed paging/windowing.

Windowing and virtualisation are internal implementation details. They must not become user-facing caps. A supported large corpus must remain fully selectable, navigable, previewable, processable, and analysable. TypeScript renders only the current viewport, but the logical dataset remains complete.

### Selection must stay compact

Large selections must be represented compactly.

Prefer selection descriptors such as:

```text
none
single(source_id)
explicit(ids)
all_matching(query_or_filter)
all_sources
```

Do not materialise tens of thousands of selected IDs in TypeScript if the operation can be represented as `all_sources` or `all_matching`.

When applying an operation to a large selection, pass the descriptor to Rust and let Rust resolve the affected sources.

### Preview is bounded inspection

Preview is for inspection, not bulk loading.

Selected-source previews expose the complete logical selection through Rust-backed viewports. The frontend renders only the visible source cards, and Rust returns bounded text for those cards without reading every selected file at once.

### Cleaning and processed previews

Processed Text previews current cleaning settings. The Process action materialises processed output. TypeScript must not apply cleaning rules.

### Long-running work reports progress

Long-running work must run in Rust and report progress to the frontend.

Examples:

```text
registered 12,000 / 50,000 sources
counted 9,000 / 50,000 files
processed 4,500 / 50,000 files
indexed 600,000 / 1,400,000 tokens
```

The UI must remain responsive while this work runs.

### Performance-sensitive changes require an architecture note in the final response

When a change touches source loading, preview, cleaning, tokenisation, repeated artefacts, analysis, search, or export, the final response must state:

* what work is done in Rust
* what work remains in TypeScript
* whether any corpus-sized data is held in frontend state
* whether the frontend renders only bounded visible data
* what large-corpus manual test was performed

If a temporary TypeScript-side workaround remains, call it out explicitly as transitional technical debt. Do not present it as the final architecture.

### Local large-corpus testing

Large-corpus tests must use local synthetic data and must not be committed.

Use local ignored directories such as `.local-corpora/`, `local-corpora/`, or another gitignored location for manual performance testing.

Never commit large generated corpora, private data, copyrighted corpora, or performance-test outputs.


## Testing policy

These are hard constraints. They exist to keep private and copyrighted material out of the repository.

- Test fixtures in `crates/corpuswright-core/tests/fixtures/` must stay tiny, deterministic, and entirely synthetic. Never add real research data, copyrighted material, or large files.
- The public demo corpus in `examples/corpora/public-domain-demo/` must contain only synthetic or verifiable public-domain text.
- Never commit anything to `.local-corpora/`, `local-corpora/`, `manual-corpora/`, `sample-corpora-local/`, or `coragrarian/`. These are gitignored and reserved for local private testing.

## Writing documentation

Write every document as a plain description of the software's current, permanent behaviour, for a reader who knows nothing about how the code came to be. If a sentence sounds like a status report, a changelog entry, or a note to yourself, rewrite it as a description.

That principle covers most cases. Concretely, it rules out:

- **Development-process references.** No "in this pass", "new in this version", "recently changed", or "has been removed for v1". State current behaviour as fact, not as a diff from some earlier state.
- **Explanatory scaffolding.** No "Problem: … Solution: we use …" structure, no parenthetical justifications in headings such as "(Why X Fails)" or "(Honesty)". Make the claim directly.
- **Maths notation in prose.** Write `at least 0.70` and `the top 3 lines`, not `$\ge 0.70$` or `$N = 3$`.
- **Mechanically symmetrical lists.** Vary sentence and bullet length. A list where every item is the same shape and length reads as generated.

Two conventions that aren't about voice:

- Use British spelling — "artefact", "normalise", "behaviour" — in prose. Code identifiers, filenames, and API names stay exactly as written, even when that means American spelling.
- The legacy PySide prototype documentation lives in `docs/archive/`. Don't present archived content as describing the current application.

## Writing code comments

Comment the *why*, not the *what*. The code already states what it does; a comment earns its place by explaining something the code can't — a non-obvious decision, an edge case, a constraint, or the reason a simpler approach doesn't work. A comment that paraphrases the line above it is noise.

Specifically:

- Don't narrate obvious operations (`// increment the counter`, `// loop over the documents`), and don't restate a function's signature in its doc comment.
- Don't leave scaffolding: banner dividers (`// ===== Helpers =====`), step-numbered walkthroughs (`// Step 1: …`), "example usage" blocks in source files, or `// TODO: implement` left in finished code.
- Don't write changelog comments in code (`// added to fix …`, `// new in this version`). Same rule as the prose docs — describe what the code does now, not how it got here. History lives in git.

For public API, do write real doc comments:

- **Rust:** use `///` on public items. Describe the behaviour, and add `# Panics`, `# Errors`, and `# Safety` sections where they apply. Prefer a runnable example (doctest) over prose describing how to call it. Don't restate the types — they're already in the signature.
- **TypeScript:** use TSDoc on exported functions and types. Document intent, units, and invariants, not types — TypeScript already has them, so `@param {string} name` is redundant. Call out anything a caller could easily get wrong.

When a comment is genuinely needed, write it as a plain note a maintainer would leave, not as a lesson addressed to a reader.


## Repository structure

```
crates/corpuswright-core/   Rust library: extraction, cleaning, search, export, repeated artefacts
apps/desktop/               Tauri v2 desktop app with TypeScript/Vite frontend
legacy/pyside/              original PySide6 implementation, kept for reference
docs/                       design notes and reference documentation
docs/archive/               archived documentation from the legacy PySide prototype
examples/                   example corpora and usage material
```
