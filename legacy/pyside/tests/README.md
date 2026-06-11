# Test Layout

- `test_ingestion.py` contains pure Python ingestion tests. These must not
  require PySide6, spaCy, Rust, maturin, or an installed `corpus_preview`
  extension. Optional Tika fallback coverage uses fakes/mocks and must not
  require Java or a running Tika server.
- `test_gui_ingestion_integration.py` checks the GUI/ingestion boundary without
  launching the GUI. It is marked `integration` and skips when PySide6 or Qt
  WebEngine cannot be imported locally.
- `test_native_ingestion.py` is marked `native` and requires the compiled
  `corpus_preview` extension. It skips locally when the extension is unavailable,
  but CI installs the maturin-built wheel first so these tests exercise native
  HTML/DOCX/PDF functions and native-backed Python extractors.

Rust crate tests live under `rust_preview/` and are run with `cargo test`.
