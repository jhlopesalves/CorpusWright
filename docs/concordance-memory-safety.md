# Concordance memory safety

## Demand-driven Collocates

The Concordance Workbench deliberately treats Collocates as an explicit, expensive analysis.
On first use, Lines and Distribution are expanded and Collocates is collapsed. A saved empty
collapse preference is different from no preference and still restores all three cards expanded.

Shared Search updates the node query and requests the expanded Lines/Distribution views once. It
invalidates any previous Collocates aggregate, but it never starts a collocation aggregation.
Expanding Collocates also performs no query. The labelled **Compute collocates** action is the only
way to start a fresh aggregate. Presentation-only changes may query the cached aggregate, while
node, case, scope, span, or direction changes return the card to the explicit-compute state.

This separation keeps a resource refusal local to Collocates. Existing Lines and Distribution
results remain visible, and the card asks the user to narrow the file scope or use a less frequent
node before retrying.

## Candidate-proportional indexed aggregation

The indexed path has two phases:

1. Candidate discovery visits indexed node hits and keeps positional counts under the index's
   existing numeric symbol IDs. It does not copy the index vocabulary and does not allocate a
   vocabulary-sized frequency vector.
2. Finalisation sorts the candidate source IDs, assigns deterministic compact local IDs, resolves
   and copies each candidate string once, remaps the positional counts, and scans the selected
   token records once to count frequencies only for those candidates. Temporary source-to-local
   structures are dropped before the aggregate is returned.

The estimator accounts for the 40-`u64` positional payload, conservative hash-table bucket slack,
compact `String` headers and UTF-8 bytes, candidate frequencies, remapping state during
finalisation, and fixed aggregate fields. Instrumentation records peak candidate entries, peak and
final projected bytes, candidate string bytes, and final candidate count.

## Shared cache ownership

The desktop last-one cache stores `Arc<ConcordanceCollocationAggregate>`. A cache hit uses
`Arc::clone`, and a miss wraps the new aggregate once before sharing it with the cache and result
projection. Presentation queries borrow the aggregate. No production path deep-clones its owned
strings, maps, frequency vector, or positional counts.

## Combined admission estimate

`ConcordanceAnalysisMemoryEstimate` is intentionally an estimate of concordance-owned analysis
structures, not complete process RSS. Indexed aggregation is admitted against:

```text
estimated resident index bytes
+ peak projected collocation aggregate bytes
+ 268,435,456-byte safety margin
<= configured combined limit
```

The conservative default combined limit is 1,610,612,736 bytes (1.5 GiB). It can be overridden
with `CORPUSWRIGHT_CONCORDANCE_MAX_COMBINED_BYTES`. This does not raise the separate existing
surface-index limit. The remaining aggregate budget is calculated with checked subtraction before
aggregation and enforced again as candidates grow and before finalisation. Underflow or excess
returns a typed `AnalysisMemoryLimitExceeded` error containing the index estimate, aggregate
projection, safety margin, and configured combined limit. That resource error is fail-closed and
does not launch an automatic scan fallback. The scan route retains its own bounded allocation
contract.

## Synthetic scale evidence

The ignored `surface_collocation_scale_evidence` fixture is configurable through:

- `CORPUSWRIGHT_COLLOCATION_SCALE_TOKENS`
- `CORPUSWRIGHT_COLLOCATION_SCALE_TOKENS_PER_DOCUMENT`
- `CORPUSWRIGHT_COLLOCATION_SCALE_VOCABULARY`
- `CORPUSWRIGHT_COLLOCATION_SCALE_CANDIDATE_PERCENT`

It generates one bounded text buffer per synthetic document, builds the index from borrowed views,
then drops all source text before aggregation. The default 4,000,000-token document size lets the
fixture model 60M tokens without breaching the normal per-document staged-text guard or duplicating
the complete fixture text. It models a common node and a less frequent moderate node, configurable
vocabulary/candidate proportions, candidate-only aggregate storage, combined admission, and shared
`Arc` ownership. It never reads the maintainer's corpus.

On 2026-07-11, this command passed with 5,000,000 measured tokens, a configured vocabulary of
4,096, and a 1% common-node candidate proportion:

```sh
/usr/bin/time -v env CARGO_BUILD_JOBS=1 \
  CORPUSWRIGHT_COLLOCATION_SCALE_TOKENS=5000000 \
  CORPUSWRIGHT_COLLOCATION_SCALE_VOCABULARY=4096 \
  CORPUSWRIGHT_COLLOCATION_SCALE_CANDIDATE_PERCENT=1 \
  cargo test --release -p corpuswright-core surface_collocation_scale_evidence -- \
  --ignored --nocapture --test-threads=1
```

Measured fixture evidence:

- complete index vocabulary: 4,098 symbols;
- measured documents: 2, with at most 4,000,000 tokens per document;
- estimated resident index: 40,809,428 bytes;
- common node: 78,061 hits, 61 candidates, 213 candidate UTF-8 bytes, 46,071 final projected
  aggregate bytes, 93,881 peak projected bytes, and 43 returned rows;
- moderate node: 4,879 hits, 4,096 candidates, 19,370 candidate UTF-8 bytes, 3,067,270 final
  projected aggregate bytes, 6,246,242 peak projected bytes, and 500 returned rows;
- common-node combined projection: 309,338,765 bytes, including the 268,435,456-byte safety
  margin, against the 1,610,612,736-byte limit;
- cache sharing evidence: `Arc` strong count 2 with pointer identity;
- fixture timings: 24 ms text generation, 658 ms index build, 201 ms common-node aggregation, and
  77 ms moderate-node aggregation;
- full timed Cargo command: 4 minutes 16.63 seconds and 2,388,580 KiB maximum RSS. This maximum
  includes a fresh optimized compilation and linking pass, not only the 0.96-second fixture.

The fixture also calculates without allocating a 60,000,000-token compact token-array lower bound
of 480,000,000 bytes. The corresponding legacy structural floor is 4,800,000,000 bytes. These are
representation floors, not RSS forecasts: they exclude vocabulary offsets, symbols, maps,
documents, vector headers, allocator slack, and the rest of the process.

No 60,000,000-token test was executed, and no real-corpus validation is claimed. Maintainer checks
on the real approximately 60M-token corpus remain mandatory before merge.
