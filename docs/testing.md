# CorpusAid Testing Policy

This document outlines the testing strategies and validation commands for the CorpusAid Rust/Tauri architecture.

## Testing Artifacts and Corpora Separation

In order to maintain a fast, reliable, and clean testing environment without risking the leak of private research data, the project strictly separates tests into three categories:

### 1. Tracked Synthetic Test Fixtures
- **Location:** `crates/corpusaid-core/tests/fixtures/`
- **Purpose:** Used in automated unit tests (`cargo test`).
- **Policy:** These files MUST remain tiny, deterministic, and entirely synthetic. They contain concentrated examples of artifacts (fake page numbers, HTML-like tags, Roman numerals) to exhaustively test string cleaning logic without bloating the repository.

### 2. Small Public Demo Corpus
- **Location:** `examples/corpora/public-domain-demo/`
- **Purpose:** A safe, public-domain corpus tracked in Git that contributors and users can manually load into the CorpusAid desktop application to experiment with parameters.
- **Policy:** This corpus is strictly composed of synthetic random text or completely verifiable public domain text. No copyrighted material or large files should ever be placed here.

### 3. Ignored Local Corpora
- **Location:** `.local-corpora/`, `local-corpora/`, `manual-corpora/`, `sample-corpora-local/`, `coragrarian/`
- **Purpose:** Used by developers to test the application locally against massive or private research data.
- **Policy:** **NEVER commit private or large corpora.** The directories listed above are strictly ignored by `.gitignore`. If you need to test local data, always place it in one of these directories.

---

## Validation Commands

Before proposing changes, ensure that all core tests pass and the desktop application compiles successfully. Run the following standard validation commands from the repository root:

1. **Test Rust Core Library:**
   ```powershell
   cargo test --manifest-path crates/corpusaid-core/Cargo.toml
   ```

2. **Check Tauri App Backend:**
   ```powershell
   cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
   ```

3. **Build Frontend Web Assets:**
   ```powershell
   cmd.exe /c "cd apps\desktop && npm run build"
   ```

### Manual Demo Validation
After building, you should run the desktop app and perform manual validation:
1. Open the app and click **File > Open Corpus Directory**.
2. Select the `examples/corpora/public-domain-demo/` folder.
3. Open the **Settings > Processing Parameters** modal and toggle the options.
4. Check the **Processed Text** preview tab to visually confirm that the synthetic artifacts in the `dirty/` subfolder are properly removed.
5. Save the processed corpus and ensure the manifest and texts are correctly exported to a temporary output folder.
