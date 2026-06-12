# AGENTS.md — Coding Agent Instructions

This file contains instructions for coding agents (LLM-based assistants) working on the CorpusWright repository.

## Validation Commands

Before proposing any changes, run the following validation commands from the repository root and confirm they pass.

### 1. Test Rust Core Library

```powershell
cargo test --manifest-path crates/corpuswright-core/Cargo.toml
```

### 2. Check Tauri App Backend

```powershell
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

### 3. Build Frontend Web Assets

```powershell
cmd.exe /c "cd apps\desktop && npm run build"
```

All three commands MUST succeed before committing.

## Testing Policy

- Test fixtures in `crates/corpuswright-core/tests/fixtures/` MUST remain tiny, deterministic, and entirely synthetic. Do NOT add real research data, copyrighted material, or large files.
- The public demo corpus in `examples/corpora/public-domain-demo/` MUST contain only synthetic or verifiable public-domain text.
- NEVER commit files to `.local-corpora/`, `local-corpora/`, `manual-corpora/`, `sample-corpora-local/`, or `coragrarian/`. These directories are `.gitignore`-d and reserved for local private testing.

## Documentation Conventions

- Use British spelling "artefact" in prose (not in code identifiers, filenames, or API names).
- Do not use session-relative language such as "in this pass", "new in this pass", or "has been removed for v1". Describe current behaviour as plain statements.
- The legacy PySide prototype documentation lives in `docs/archive/`. Do not present archived content as describing current application behaviour.

## Repository Structure

```text
crates/corpuswright-core/   Rust library crate: extraction, cleaning, search, export, repeated artefacts
apps/desktop/              Tauri v2 desktop application with TypeScript/Vite frontend
legacy/pyside/             Original PySide6 implementation, preserved for reference
docs/                      Design notes and reference documentation
docs/archive/              Archived documentation from the legacy PySide prototype
examples/                  Example corpora and usage material
```
