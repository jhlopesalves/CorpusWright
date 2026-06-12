# Release Cleanup: Accidental `0.3` Tag/Release

> **Do not execute destructive remote commands without first verifying
> the current state of releases and tags.  Read each step carefully.**

## Background

The repository currently has an inconsistent tag state where both `0.3`
and `v0.1.0-alpha.3` point to the alpha.3 commit.  Build assets may have
been uploaded under the short `0.3` tag instead of the canonical
`v0.1.0-alpha.3` tag.

This note describes how to inspect the current state and, if safe,
remove the accidental release/tag **without losing any build assets**.

---

## Step A — Inspect current state

Run these commands from a local clone of the repo (or a GitHub Codespace
with `gh` authenticated):

```bash
# 1. View the accidental release (if any)
gh release view 0.3

# 2. View the canonical release
gh release view v0.1.0-alpha.3

# 3. List all remote tags to confirm what exists
git ls-remote --tags origin
```

Take note of:
- Which tag has build assets attached (Windows NSIS setup, macOS DMGs).
- Whether the canonical `v0.1.0-alpha.3` release exists and has the
  expected assets.
- Whether `0.3` is a lightweight tag or an annotated tag.

---

## Step B — Ensure canonical release has assets

If build assets exist **only** on the `0.3` release/tag, do **not**
delete anything yet.  First make sure the canonical release has them.

**Option 1 — Re-run the fixed workflow**

Push a commit to `main` that includes the hardened release workflow,
then go to:

```
https://github.com/jhlopesalves/CorpusWright/actions/workflows/release.yml
```

Click **Run workflow** → select the `main` branch.  The fixed workflow
will create the release under the canonical tag `v0.1.0-alpha.3` with
the derived release metadata.

**Option 2 — Manually move or reupload assets**

If re-running the workflow is not feasible, download the assets from the
`0.3` release and upload them to the `v0.1.0-alpha.3` release via the
GitHub web UI:

1. Go to `https://github.com/jhlopesalves/CorpusWright/releases/tag/0.3`
2. Download each asset.
3. Go to `https://github.com/jhlopesalves/CorpusWright/releases/tag/v0.1.0-alpha.3`
4. Click **Edit** → drag-and-drop the assets into the binaries section → **Save**.

---

## Step C — Delete accidental release/tag

Only proceed after the canonical `v0.1.0-alpha.3` release has the
expected assets:

| Platform | Expected asset pattern |
|----------|-----------------------|
| Windows NSIS | `CorpusWright_0.1.0-alpha.3_x64-setup.exe` or similar |
| macOS Intel DMG | `CorpusWright_0.1.0-alpha.3_x64.dmg` or similar |
| macOS Apple Silicon DMG | `CorpusWright_0.1.0-alpha.3_aarch64.dmg` or similar |

**If `0.3` is a full release** (has associated release page):

```bash
gh release delete 0.3 --cleanup-tag --yes
```

**If only a tag exists** (no release page — rare):

```bash
git push origin :refs/tags/0.3
```

---

## Step D — Never delete the canonical tag

**Do not** delete `v0.1.0-alpha.3`.  This is the canonical, permanent
release tag for alpha.3.

---

## Prevention

The hardened release workflow (`.github/workflows/release.yml`) now:

1. Reads the app version from `tauri.conf.json`.
2. Computes the canonical tag as `v${APP_VERSION}`.
3. Validates both the version and the computed tag against strict
   alpha-semver patterns.
4. Uses the derived tag and release name in the Tauri action.
5. Refuses to run if the version is malformed.

This prevents accidental short tags like `0.3`, `alpha.3`, or a missing
`v` prefix from being created through the release workflow.

Additionally, the release consistency check script
(`scripts/check-release-consistency.cjs`) verifies that the version
string is aligned across all relevant files.  Add it to your CI or run
it locally before tagging a release.
</write_to_file>
