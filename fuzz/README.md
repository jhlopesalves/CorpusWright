# Parser fuzzing

Install nightly Rust and `cargo-fuzz`, then run a target from the repository root:

```sh
cargo fuzz run --codegen-units 16 source_parsers -- -max_total_time=600
cargo fuzz run --codegen-units 16 persistent_state -- -max_total_time=600
cargo fuzz run --codegen-units 16 tokenisation -- -max_total_time=600
```

Seeds must be synthetic bytes; do not add real documents or corpora. Crash inputs appear under
`fuzz/artifacts/` (ignored). Reproduce with
`cargo fuzz run --codegen-units 16 <target> <artifact>`, minimise when useful, and convert each
confirmed bug into a deterministic core regression test using generated fixtures before deleting
the local artifact. Never commit an artifact containing user data.
