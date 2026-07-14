# CorpusWright Documentation Books Corpus

Stable identifier: `corpuswright-documentation-books-v1`

This is a deliberately curated English literary corpus for transparent CorpusWright documentation examples, a manually loadable demonstration, and naturalistic acceptance testing. It is not representative of English, genre-balanced, historically balanced, author-population balanced, or suitable for inferential comparison.

## Editorial profile

Canonical adult literary writing, principally nineteenth- and early-twentieth-century fiction, drama, essays, and limited poetry, with Gothic, realist, decadent, modernist, philosophical, social, or political seriousness and enough linguistic substance for corpus diagnostics.

The retained seed profile covers psychologically, socially, politically, and aesthetically serious adult literature. `1984` and `Animal Farm` are included under Brazil's life-plus-70-year public-domain rule, based on George Orwell's death in 1950 and the maintainer's statement that this corpus is distributed in Brazil; no United States public-domain claim is made for those files. `Lady Windermere's Fan` is excluded to cap Oscar Wilde at three documents while retaining drama through Shaw's `Pygmalion`.

## Reproduction

From the repository root, run:

```bash
cargo run -p corpuswright-core --example documentation_corpus
node apps/desktop/scripts/capture-documentation-corpus-dossier.mjs
```

The Rust command caches the official Project Gutenberg catalogue and shortlisted ebook downloads under `/tmp/corpuswright-documentation-books-raw/`. It uses the approved high-speed Project Gutenberg mirror, a descriptive User-Agent, a minimum two-second interval between ebook requests, raw SHA-256 checksums, strict validated start/end markers, LF line endings, and exactly one final newline. Missing or ambiguous markers are refused. The second command renders the temporary dossier with CorpusWright's actual frontend figure geometry, layout, theme tokens, renderer, and standalone SVG export path.

Raw cache files and dossier images are intentionally not committed. Public source references use permanent landing pages; exact retrieved bytes are pinned by SHA-256 in the manifests.

## Corpus identity

- Repository baseline audited for this generation: `439205bd005937bf01d5298d8ff4499b4cea0d18`
- Documents: 20
- CorpusWright tokens: 1477104
- CorpusWright types: 40608
- Corpus checksum: `2f5a213218f94783c16258f0719fbae293021bb8426e44abfd9b884c1899d81d`
- Canonical tokenizer: `corpuswright-uax29` version 1, default configuration
- Document unit: one accepted book or coherent single-author collection per file

## Files

- `corpus.json`: corpus and document metadata
- `checksums.json`: ordered cleaned-file checksums and corpus identity
- `selection.csv`: seed dispositions, complete shortlist, editorial score inputs, selections, and rejections
- `cleaning-report.json`: marker evidence and removed byte counts
- `seed-manifest.json`: immutable original-seed audit
- `project-gutenberg-manifest.json`: catalogue, retrieval, and raw-byte evidence
- `texts/`: accepted cleaned UTF-8 documents only

Accepted cleaned bytes total 8280825. Byte-identical seed originals are retained in the external raw cache; the corpus directory contains no source subdirectory and no direct-child text files.
