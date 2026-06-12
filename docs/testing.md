# CorpusWright Testing Guide

This document covers test organisation, fixture policy, and how to run the test suite.

## Test Artefacts and Corpora

### Tracked Synthetic Test Fixtures

- **Location:** `crates/corpuswright-core/tests/fixtures/`
- **Purpose:** Used in automated unit tests (`cargo test`).
- **Policy:** These files should remain tiny, deterministic, and entirely synthetic. They contain concentrated examples of artefacts (fake page numbers, HTML-like tags, Roman numerals) to exhaustively test string cleaning logic without bloating the repository.

### Small Public Demo Corpus

- **Location:** `examples/corpora/public-domain-demo/`
- **Purpose:** A safe, public-domain corpus tracked in Git that contributors and users can manually load into the CorpusWright desktop application to experiment with parameters.
- **Policy:** This corpus should contain only synthetic random text or completely verifiable public-domain text. No copyrighted material or large files.

### Ignored Local/Private Corpora

- **Location:** `.local-corpora/`, `local-corpora/`, `manual-corpora/`, `sample-corpora-local/`, `coragrarian/`
- **Purpose:** Used by developers to test the application locally against large or private research data.
- **Policy:** Do not commit private or large corpora. The directories listed above are ignored by `.gitignore`. Place any local test data in one of these directories.

---

## Running Tests

### Rust Core Library

From the repository root:

```bash
cargo test --manifest-path crates/corpuswright-core/Cargo.toml
```

### Tauri App Backend Check

```bash
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

### Frontend Web Assets

```bash
cd apps/desktop
npm run build
```

### Manual Demo Validation

After building, run the desktop app and verify basic functionality:

1. Open the app and click **File > Open Corpus Directory**.
2. Select the `examples/corpora/public-domain-demo/` folder.
3. Open the **Settings > Processing Parameters** modal and toggle the options.
4. Check the **Processed Text** preview tab to confirm that the synthetic artefacts in the `dirty/` subfolder are properly removed.
5. Save the processed corpus and ensure the manifest and texts are correctly exported to a temporary output folder.
