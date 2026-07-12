# Phase 4B: PDF OCR Roadmap

This document outlines the feasibility, architecture options, and recommendations for implementing Optical Character Recognition (OCR) for scanned/image-only PDFs in CorpusWright Phase 4B.

## Objective
To allow researchers to seamlessly extract text from scanned, non-embedded PDF files without relying on external cloud APIs, ensuring local-first privacy.

---

## Option 1: Tesseract through Rust bindings

Using crates like `leptess` or direct `tesseract-sys` bindings.

**Pros:**
- Pure programmatic integration directly within the Rust process.
- No need to manage external process execution.

**Cons/Implications:**
- **Dependencies required**: Requires Leptonica and Tesseract C/C++ libraries.
- **Windows packaging**: EXTREMELY difficult to cross-compile or bundle statically on Windows. Requires users to install Tesseract manually or forces us to ship heavy `.dll` sidecars alongside the Tauri binary.
- **Language data**: Requires `tessdata` (language packs) to be downloaded and placed in specific directories.

## Option 2: Tesseract as a Tauri sidecar

Tauri provides first-class support for "sidecars"—bundling external executable binaries alongside the app.

**Pros:**
- Complete isolation. We don't need to fight C/C++ compilation toolchains in Rust.
- If Tesseract crashes on a malformed image, the main app survives.

**Cons/Implications:**
- **Bundling**: We must source pre-compiled Tesseract binaries for every target OS (Windows `.exe`, macOS, Linux) and configure Tauri's `tauri.conf.json` to embed them.
- **Data**: Still requires language models to be bundled or downloaded on-the-fly.
- **Communication**: We must parse `stdout`/`stderr` from the Tesseract child process to report extraction progress to the UI.

## Option 3: Pure Rust OCR

Using an emerging crate like `ocrs`.

**Pros:**
- Solves the C/C++ compilation nightmare.
- Cross-compiles beautifully to Windows, Mac, and Linux without native dependencies.

**Cons/Implications:**
- **Maturity**: Still experimental. Lacks the decades of heuristics and layout analysis present in Tesseract.
- **Language Support**: Currently limited primarily to English, lacking the massive global language support of Tesseract (critical for diverse linguistic corpora).
- **Models**: Requires downloading ONNX models at runtime.

## Option 4: PDF rendering for OCR

Tesseract operates on *images*, not PDFs. To OCR a PDF, we must first rasterize its pages into images.

**Pros/Cons:**
- **pdfium-render**: A high-quality Rust binding to Google's Pdfium engine. It can reliably render PDF pages to images. However, it requires shipping `pdfium.dll` (another native dependency).
- **External Poppler (pdftoppm)**: Could be shipped as a sidecar, but adds more bulk.

*Why OCR requires rendering:* PDFs are vector instruction sets. When a PDF is just a wrapper around a JPEG, we could theoretically extract the JPEG natively. But many scanned PDFs have multiple tiled images, masks, or vector elements layered over them. Rasterizing the entire page to a flat image guarantees we "see" exactly what a human sees before feeding it to the OCR engine.

---

## Implementation Decision (Completed)

Phase 4B successfully implemented OCR using a hybrid of **Option 3 and Option 4** (`pdfium-render` for rasterisation and `ocrs` for pure-Rust OCR).

**Why this was chosen:**
1. **Dependency Management**: We avoided the C/C++ compilation nightmares of Tesseract by using `ocrs`, which relies on `rten` (Rust Tensor engine).
2. **Bundle Size vs Complexity**: While `pdfium` still requires a `.dll`/`.so` binary sidecar, it is much smaller and easier to manage than a full Tesseract sidecar with hundreds of MBs of `tessdata`.
3. **UX Alignment**: OCR is strictly opt-in via a UI checkbox ("Use OCR (Experimental)"). If enabled, `extract_pdf` attempts normal embedded text extraction first, and only falls back to rendering and OCRing pages if the embedded text is empty.

This implementation keeps the main `corpuswright-core` crate pure Rust (mostly) while seamlessly supporting legacy, scanned corpora natively without cloud APIs.
