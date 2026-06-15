# CorpusWright projects

A CorpusWright project is a folder with a small metadata directory. The project launcher opens this folder before the preprocessing workspace is shown.

New projects use this folder shape:

```text
Project Name/
    .corpuswright/
        project.json
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

After a project opens, CorpusWright shows the existing preprocessing screen. File loading, processed previews, export, custom removals, and repeated-artefact review continue to use the same preprocessing commands and cleaning pipeline.
