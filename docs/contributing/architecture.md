# Maintained architecture boundaries

This note describes the boundaries that contributors should preserve after the final hardening lane. It is a map of ownership, not a request for further module movement. New structural changes should be driven by a concrete bug, test, profile, package failure, or repeated maintenance cost.

## Product and repository boundaries

`corpuswright-core` owns UI-independent scanning, extraction, cleaning, tokenisation, analysis, caches, and export. It must not import Tauri. The desktop crate owns native composition, IPC commands, runtime state, and resource lookup. The TypeScript frontend owns interaction and rendering. Repository orchestration, fixture generation, checks, dossiers, and packaging live in `corpuswright-xtask` or its scripts and do not enter release binaries.

`corpuswright-test-support` is unpublished and may be a development dependency or an xtask dependency. Product runtime dependency graphs must not contain it. The older `legacy/pyside/` implementation remains as historical and architectural reference and is outside the Rust/Tauri migration.

## Runtime configuration

The desktop process constructs `DesktopRuntimeConfig` once at startup and installs its validated limits before registering commands. Invalid values fall back to conservative defaults and produce one diagnostic. Product code should receive this typed state instead of reading environment variables in query or worker hot paths.

The maintained product settings are:

| Setting | Ownership |
| --- | --- |
| `CORPUSWRIGHT_CONCORDANCE_BACKEND` | Concordance route mode: default index, scan, strict index, or audit |
| `CORPUSWRIGHT_SURFACE_INDEX_MAX_RESIDENT_BYTES` | Projected resident index budget |
| `CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TEXT_BYTES` | Staged prepared-text budget |
| `CORPUSWRIGHT_SURFACE_INDEX_MAX_STAGED_TOKEN_RECORDS` | Staged token-record budget |
| `CORPUSWRIGHT_SURFACE_INDEX_MAX_SORT_RECORDS` | Retained query-sort budget |
| `CORPUSWRIGHT_CONCORDANCE_MAX_COMBINED_BYTES` | Combined Concordance/collocation memory admission |
| `CORPUSWRIGHT_OCR_WORKERS` | Bounded OCR worker count |
| `CORPUSWRIGHT_ANNOTATION_TIMING` | Annotation timing diagnostics |
| `CORPUSWRIGHT_OCR_DIR` | Deliberate OCR/PDFium resource override |
| `CORPUSWRIGHT_UDPIPE_MODEL_DIR` | Deliberate UDPipe model override |
| `CORPUSWRIGHT_VERIFY_CACHE_WRITE` | Cache write-readback verification |

Benchmark and test-only controls remain close to their harnesses and must not become implicit product configuration.

## Identities, generations, cancellation, and caches

IPC and persisted files retain primitive numbers and strings for compatibility. Inside the desktop process, `CorpusVersion`, `AnalysisGeneration`, `CleaningConfigSignature`, `ProjectRootIdentity`, and `IndexIdentity` prevent unrelated lifecycle and cache domains from being mixed. The frontend uses domain-labelled generation counters; a completion is accepted only by the owner whose current generation matches the request.

The core `CancellationToken` defines cooperative cancellation and job identity. Desktop `ActiveCancellation` owns at most one active job, cancels the job it replaces, and ignores a stale job's attempt to finish a newer one. Domain algorithms keep their existing checkpoint frequency and deterministic partial-state rules.

Cache keys must name every semantic input. Examples include extraction source/config identity, PDF page range and extraction settings, annotation corpus/language/model identity, and Concordance index/collocation identities. Presentation-only sorting or display settings stay out of expensive aggregate keys when they do not change the aggregate. Tests must prove both invalidation when a semantic component changes and reuse when only presentation changes.

## Error boundary

This release deliberately retains string errors for every registered Tauri command so the IPC protocol is consistent and visible wording remains compatible. The desktop classifies errors internally, and `app/system-ipc.ts` plus `app/ipc-error.ts` provide the single frontend rejection and normalisation boundary. A future structured error-envelope migration must update every command and every adapter as one vertical change; mixed protocols are not allowed.

## Concordance ownership

The desktop Concordance feature is organised by policy:

- `service/backend.rs` owns validated modes, route labels, and route policy.
- `service/indexed.rs` owns warm persisted-index execution and rebuild context.
- `service/scan.rs` owns bounded scan execution and its adapters.
- `service/parity.rs` owns audit comparison and difference evidence.
- `service/recovery.rs` owns retry-pending/disabled transitions and permitted fallback.
- `service/telemetry.rs` owns stable timing and log-field formatting.
- `service/mod.rs` orchestrates those policies without redefining them.
- `state.rs` owns index, failure, cancellation, and collocation-cache identities.

The core remains the owner of analytical semantics. Route choice, fallback eligibility, parity comparison, deterministic ordering, and telemetry field names are behavioural contracts.

## Frontend IPC and tests

Raw Tauri APIs enter through `app/system-ipc.ts`. Feature adapters in project, prepare, analyse, and plot ownership construct requests, subscribe and clean up listeners, invoke cancellation, and normalise failures. Controllers own state transitions, rendering, and interaction; they do not import Tauri directly.

The test hierarchy is intentionally mixed:

- Rust tests cover core behaviour, desktop policy, parsing, cancellation, and IPC commands.
- Node tests cover pure behaviour and static architecture/style/generated contracts.
- Vitest with happy-dom dispatches real DOM events and checks rendered state.
- the small WebdriverIO browser workflow uses the real Vite frontend with feature-boundary mocks;
- the Linux-only `native-smoke` build feature launches the compiled app, resolves bundled resources, processes generated data, and exits with machine-readable evidence.

The native-smoke feature is absent from normal release builds. It does not register unrestricted production commands or automate native file dialogs.

## Security and Linux resources

The public reporting and untrusted-document threat model is in [`SECURITY.md`](../../SECURITY.md). `cargo xtask security` enforces dependency policy, npm audit policy, parser tests, repository data policy, and architecture boundaries; scheduled CI adds bounded fuzz smoke jobs. Parser budgets fail with explicit errors rather than truncating input.

Tauri packages an `ocr/` resource directory and a `udpipe/` resource directory. PDFium lookup derives the platform library name from that directory:

- Windows: `pdfium.dll`
- Linux: `libpdfium.so`
- macOS: `libpdfium.dylib`

Installed Linux resources live under `/usr/lib/CorpusWright/`; the executable and desktop entry live under `/usr/bin/` and `/usr/share/applications/`. The maintained native smoke proves PDFium can be loaded from the resolved resource directory before scan, preview, analysis, Frequency, Concordance, and export run. Environment paths are deliberate overrides, not development-tree requirements.

## Mechanical enforcement

`cargo xtask repository-policy` runs both the repository-data and architecture checks. They protect core/Tauri separation, product dependency direction, frontend IPC ownership, generated-file ownership, removed compatibility aliases, ignored corpus roots, the legacy tree, and explicit command registration. Update an allowlist only when the exception is a durable architectural fact.
