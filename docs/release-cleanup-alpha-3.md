# Release Cleanup — Alpha 3

## Known Issues

### MSI bundling does not accept alpha prerelease version strings

The MSI packaging target requires numeric-only prerelease identifiers (e.g., `1.0.0.1`) and rejects
semver prerelease tags like `1.0.0-alpha.3`. The build fails with:

```
optional pre-release identifier in app version must be numeric-only and cannot be greater than 65535 for msi target
```

**Workaround:** Use `cargo run` (dev mode) for local testing, or build non-MSI alpha artifacts
(e.g., portable `.exe` or NSIS installer) until the packaging version is adjusted.

**Planned fix:** Strip or replace the prerelease tag in the MSI build pipeline so that alpha
releases can produce MSI installers without manual version patching.

---

## Changes in this release

- Unified PDF handling review UI for import and re-extract modes
- Removed generic `review recommended` badge from source rows and preview headers
- Added per-source PDF handling update command for re-extract row-level overrides
- Concrete warning/error badges preserved: `extraction_cancelled`, `text_extraction_failed`,
  `render_size_clamped`, `ocr_generated`, `layout_heavy_profile`
- Processed Text tab shows placeholder when processed text has not been generated
