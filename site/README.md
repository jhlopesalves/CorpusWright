# CorpusWright documentation site

`site/` is the authoritative source for the public documentation website. `docs/` is committed generated output for GitHub Pages and must not be edited by hand.

Requirements: Node.js 20 and npm.

```sh
npm --prefix site ci
npm --prefix site run build
npm --prefix site run serve
npm --prefix site run check
```

The production build accepts only the repository's `docs/` directory as its clean target. Generated-output checks build in isolated temporary directories and compare deterministic SHA-256 tree manifests without rewriting `docs/`.
