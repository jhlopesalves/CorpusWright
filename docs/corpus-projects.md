# CorpusWright projects

A CorpusWright project is a folder with a small metadata directory. The project launcher opens this folder before the preprocessing workspace is shown.

New projects use this folder shape:

```text
Project Name/
    .corpuswright/
        project.json
        sources.json
        cache/
    sources/
    processed/
    exports/
```

Opening an existing folder as a project leaves the folder contents where they are and adds project metadata inside `.corpuswright/`:

```text
Existing Folder/
    .corpuswright/
        project.json
        sources.json
        cache/
        processed/
        exports/
```

The existing folder is recorded as a source root. CorpusWright scans it with the same folder-loading path used by the preprocessing workspace.

## Project manifest

The project manifest is `.corpuswright/project.json`. It is versioned and intentionally small:

```json
{
  "project_version": 1,
  "name": "Learner Essays B2",
  "root_path": "C:\\Corpora\\Learner Essays B2",
  "created_at": "2026-06-15T12:00:00Z",
  "updated_at": "2026-06-15T12:00:00Z",
  "source_roots": [],
  "notes": null
}
```

The manifest stores the project name, root path, creation and update timestamps, optional source roots, and optional notes. It does not store corpus metrics, OCR state, token counts, cleaning decisions, or analysis results.

## Source inventory

The project source inventory is `.corpuswright/sources.json`. It is versioned separately from the project manifest and stores one record for each source file registered with the project:

```json
{
  "version": 1,
  "sources": [
    {
      "id": "source_...",
      "path": "C:\\Corpora\\Learner Essays B2\\essay_001.docx",
      "file_name": "essay_001.docx",
      "extension": "docx",
      "kind": "docx",
      "status": "ready_to_process",
      "reason_codes": ["native_text_available"],
      "extraction_route": "docx_extract",
      "ocr_quality": null,
      "added_at": "2026-06-15T12:00:00Z",
      "last_checked_at": "2026-06-15T12:00:00Z",
      "last_processed_at": null
    }
  ]
}
```

Source IDs are stable hashes of the project-relative path when the file is inside the project folder, or the canonical absolute path otherwise. The ID is suitable for keeping later processing and review decisions attached to the same source file.

The desktop app shows this inventory in the Source Explorer. Add Files and Add Folder inspect selected sources before registration, show an Import Sources review dialog, and persist the chosen source-level extraction route only after confirmation. Duplicates are skipped by stable source ID.

`extraction_route` records how text should be extracted from the source before cleaning rules run. TXT uses `plain_text`, DOCX uses `docx_extract`, and HTML uses `html_extract`. PDF sources may use `embedded_text`, `ocr_if_needed`, `ocr_required`, `force_ocr`, or `undecided`. `ocr_quality` stores the OCR render quality for PDF routes that can run OCR.

Older source inventories without `extraction_route` or `ocr_quality` still load. Missing routes default from the source kind: PDFs use `ocr_if_needed`, TXT uses `plain_text`, DOCX uses `docx_extract`, HTML uses `html_extract`, and unknown files use `unsupported`.

## Import review

After a file or folder picker returns, CorpusWright preflights the selected sources without running OCR. The Import Sources dialog shows the number of candidate sources, grouped route recommendations, duplicate and ignored-file counts, and a compact source list.

The default policy is `Use recommended routes`. Preflight recommends embedded text for PDFs with usable text, OCR-required routes for PDFs with little or no text layer, and an undecided route when the audit cannot make a conservative recommendation. The batch PDF policy can instead route selected PDFs through OCR-if-needed, force OCR, or embedded text only. Cancel closes the dialog without writing `.corpuswright/sources.json`.

PDF route and OCR quality are source-ingestion settings. Cleaning Settings keep cleaning and fallback extraction defaults for older or unspecified sources; they do not replace routes already stored on project sources.

## Source statuses

`Unprocessed` means the file is registered but has no current preflight result.

`Ready to Process` means CorpusWright can attempt the normal preprocessing route. It does not mean the file is analysis-ready.

`Needs OCR` means a PDF appears to need an OCR route before usable text can be produced. This is not a failure.

`Processed` means the existing processed-preview path has produced text without extraction warnings.

`Processed with Warnings` means processed preview produced usable output, but extraction or processing warnings should be reviewed.

`Failed` means a blocker such as a missing file, unsupported type, unreadable file, encrypted PDF, PDF open failure, parser failure, OCR failure, or write failure prevents normal processing.

Reason codes are machine-readable strings such as `missing_file`, `unsupported_file_type`, `pdf_no_text_layer`, `ocr_required`, `native_text_available`, `processed_ok`, `processed_with_warnings`, `low_text_density`, and `parse_failed`. The frontend turns these codes into compact labels.

## Creating a project

`New Corpus Project` asks for a project folder, creates `.corpuswright/project.json`, creates the small project folders, records the project in the recent-projects list, and opens the preprocessing workspace.

If the selected folder already has `.corpuswright/project.json`, CorpusWright reports that the folder already contains a project.

## Opening a project

`Open Existing Project` asks for a project folder and validates `.corpuswright/project.json`. A valid project is recorded in recent projects and opened in the preprocessing workspace.

If the manifest is missing or invalid, CorpusWright shows a simple error and stays on the launcher.

## Opening a folder as a project

`Open Folder as Corpus Project` asks for an existing folder, creates `.corpuswright/project.json` inside it, records the folder as a source root, and scans the folder with the existing preprocessing loader. Source files are not moved.

If the folder already contains a CorpusWright project, CorpusWright opens that project instead of creating a second manifest.

## Recent projects

The desktop frontend keeps a small recent-projects list in localStorage. Each entry stores the project name, root path, and last-opened timestamp in a versioned schema. Selecting a recent project validates its manifest before opening it.

## Preprocessing workspace

After a project opens, CorpusWright shows the preprocessing workspace with the Source Explorer in the left panel. Processed previews, search, export, word counts, custom removals, and repeated-artefact review use the source-level PDF route when one is stored, with Cleaning Settings acting as fallback defaults for older or unspecified sources.

Repeated artefact review is part of cleaning-rule setup. The Repeated artefacts section in Cleaning Settings scans the current Source Explorer selection when sources are selected. When no source selection is active, the action label states that all loaded project sources will be scanned. Accepted candidates become the same structured Custom Removal rules used by normal cleaning and export.

## Source import inside a project

Inside a project, source import happens through the Source Explorer:

- **Add Files** — registers the selected files as project sources via the Import Sources review dialog. Use this when a project is already open and you want to add individual source files.
- **Add Folder** — scans and registers supported files from a folder as project sources via the Import Sources review dialog.

These actions are available from the Source Explorer header buttons and from the project empty state. When no project is active, these actions load the files into the preprocessing workspace directly.

**Open Folder as Corpus Project** (from the Project menu or Welcome screen) treats the selected folder itself as a project root, creating a new project if one does not exist. This is the correct entry point for starting a new project from an existing folder. It is not the same as Add Folder, which imports sources into an already-open project.

## Removing sources from a project

**Remove selected sources** (Source Explorer header) removes source entries from the project inventory (`sources.json`) without deleting the source files from disk. A confirmation dialog prevents accidental removal. After removal, the source list and preview are updated. Removal is blocked during active PDF materialisation operations.

## File menu when a project is active

When a project is active, the File menu contains only **Export Processed Corpus** (previously labelled Save Processed Corpus). The old `Open Files` and `Open Folder` actions that bypassed the project source inventory have been removed. Source import should be performed through the Source Explorer's Add Files / Add Folder buttons.
