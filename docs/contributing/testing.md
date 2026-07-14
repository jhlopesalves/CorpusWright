# CorpusWright testing guide

The repository has one canonical local validation entry point:

```bash
CARGO_BUILD_JOBS=1 cargo xtask check --fresh-node
```

This runs Rust formatting, Clippy and tests for the core and desktop crates, regenerates the Rust-to-TypeScript contracts and checks them for drift, installs frontend dependencies with `npm ci`, runs the frontend build and test layers, and enforces release, provenance, reference, repository-data, and whitespace policy.

For focused work, use one of:

```bash
cargo xtask check --scope core
cargo xtask check --scope desktop
cargo xtask check --scope frontend
```

Without `--fresh-node`, the frontend scope reuses an existing `apps/desktop/node_modules` and reports a clear error when dependencies are missing.

## Fixture and corpus policy

CorpusWright's Git repository contains software, software resources, tests, provenance mechanisms, and generated non-content-bearing evidence. It does not distribute corpora, books, papers, or user/research source documents.

Automated document fixtures are generated at runtime by the unpublished `corpuswright-test-support` crate. Its deterministic synthetic contract covers plain text, deliberately dirty text, HTML, DOCX, born-digital PDF, malformed inputs, manifests, cache envelopes, repeated artefacts, and browser/native workflow input. The only file retained under `crates/corpuswright-core/tests/fixtures/` is a small synthetic JSON matcher contract.

Generate the maintained eight-document cross-format corpus anywhere outside the repository data boundary with:

```bash
cargo xtask generate-fixtures --output /tmp/corpuswright-fixtures
```

Local corpus roots are ignored at the repository root, including `examples/corpora/`, `.local-data/`, `.local-corpora/`, `local-corpora/`, `manual-corpora/`, `sample-corpora-local/`, `test-corpora/`, and `documentation-corpus-local/`. Do not add blanket extension ignores: stopwords, licences, authored HTML, configuration JSON, and other genuine software assets must remain visible to Git.

The policy is mechanical and Git-aware:

```bash
cargo xtask repository-policy
```

It rejects tracked corpus roots, committed source-document fixtures, and large likely document payloads outside explicit software/documentation resource roots.

## Local corpus dossier

The generic dossier accepts any local corpus supported by CorpusWright. It never downloads or selects documents and never writes source documents into the repository:

```bash
cargo xtask documentation-dossier \
  --corpus-dir /absolute/path/to/local-corpus \
  --output /tmp/corpuswright-local-dossier
```

It uses the real scan, extraction, cleaning, tokenisation, analysis, distribution, export, plot, and standalone-SVG paths. The final dossier contains checksums, configuration, non-content-bearing analytical evidence, and figure witnesses; temporary exported texts are removed. Source text is excluded unless the maintainer explicitly supplies the local-only `--include-source-text` diagnostic flag. `--skip-figures` is available for headless analytical smoke checks when Chrome or FFmpeg/librsvg is unavailable.

## Test layers

- Rust core and desktop tests prove analytical, cache, routing, cancellation, parsing, and IPC-command behaviour.
- Node contract tests protect generated ownership, architectural imports, CSS/theme contracts, static accessibility invariants, and visual geometry. Source inspection is contract evidence, not interaction proof.
- Pure frontend tests cover state transitions, request keys, generations, sorting, pagination, and formatting.
- DOM tests dispatch actual events and assert rendered state.
- Browser tests exercise a small renderer workflow through feature-owned Tauri mocks: `cargo xtask browser-test`.
- The Linux native smoke launches the built application against generated temporary input; it never depends on a committed corpus or native file dialogs.

## Security and Linux packaging

Run dependency, audit, repository-policy, and fuzz-smoke orchestration with:

```bash
cargo xtask security
```

On Ubuntu, build the mandatory Debian package with:

```bash
cargo xtask package-linux
```

`cargo xtask package-linux --install-smoke` asks before any `sudo` command, installs only after explicit confirmation, reports package/resource paths, and runs the maintained smoke workflow. It never publishes an artefact.

## Legacy implementation

`legacy/pyside/` is the original PySide implementation retained for historical and architectural reference. The modern Rust/Tauri gates do not rewrite it; its separate workflow runs only when legacy paths change.
