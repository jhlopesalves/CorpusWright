# Structured Custom Removals

CorpusWright Custom Removals are deterministic cleaning rules saved in the
cleaning configuration. They are local, reviewable, and applied only after the
user adds or promotes a removal item. The cleaner does not infer removals from
models, embeddings, or external services.

## Legacy literal removals

The `remove_patterns` list remains active. Each entry is a literal substring,
not a regular expression. During cleaning, every non-empty pattern is removed
from the processed text. If lowercase cleaning is enabled, patterns follow the
same lowercasing step as the text.

Legacy literal removals are still used for saved configurations that predate
structured rules. They also remain the path for Custom Removals that are meant
to remove text anywhere inside a line, including repeated inline artefacts such
as conversion leftovers.

## Structured rules

Structured removals live in `removal_rules` as `RemovalRule` values. A rule has:

- `id`: a stable identifier for duplicate checks and saved configurations.
- `label`: display text for review in the UI.
- `source`: the origin of the rule, such as `manual`,
  `promoted_repeated_artifact`, or `generated_pdf_cleanup`.
- `matcher`: the matching strategy.
- `scope`: the area of text affected by the rule.
- `enabled`: whether the rule participates in cleaning.

Two matcher forms are supported in the current model:

- `RemovalMatcher::Literal { text }` matches the configured text literally.
- `RemovalMatcher::NormalizedLine { normalized_key }` matches the same
  normalised whole-line key used by the repeated artefact scanner.

The supported structured rule scopes include:

- `RemovalScope::WholeLine`: removes the matching line anywhere in the document.
- `RemovalScope::PageTop`: removes the matching line only when it appears in the top zone of a page (first 3 lines).
- `RemovalScope::PageBottom`: removes the matching line only when it appears in the bottom zone of a page (last 3 lines).
- `RemovalScope::PageTopOrBottom`: removes the matching line when it appears in either the top or bottom zone.

These page-zone scopes require page-aware cleaning metadata. When page-zone rules are present, `clean_structured_document` cleans the document page-by-page and returns a flat output derived from the cleaned page representation. Flat `clean_text` ignores page-zone rules because it lacks page metadata.

The structured schema also serialises `anywhere`, but literal substring removals that work anywhere in the text continue to use `remove_patterns`.

## Exact repeated artefact promotion

An `exact_line` candidate promoted from the Repeated Artefact Finder becomes a
structured literal whole-line rule:

```text
exact_line candidate
-> RemovalMatcher::Literal
-> RemovalScope::WholeLine
```

For example, a repeated line such as:

```text
Journal of Corpus Linguistics
```

becomes a rule that removes only lines whose trimmed content is exactly
`Journal of Corpus Linguistics`. A sentence that merely mentions that text is
preserved.

## Normalised repeated artefact promotion

A `normalized_line` candidate with a grouping key becomes one structured
normalised whole-line rule:

```text
normalized_line candidate
-> RemovalMatcher::NormalizedLine
-> RemovalScope::WholeLine
```

The normalisation used for these rules trims the line, strips selected boundary
punctuation, lowercases text, collapses whitespace, and replaces digit runs
with `#`.

For example:

```text
Page 1
Page 2
Page 44
```

normalises to one key:

```text
page #
```

A promoted normalised rule for `page #` removes whole lines whose normalised
form is `page #`. It does not remove a prose sentence such as `This mentions
page 3 inside a sentence.` because that sentence has a different normalised
whole-line form.

## Candidate text signals

Repeated artefact candidates include deterministic text/noise signals for
review. The signal describes the candidate's local text shape, such as likely
natural text, section heading, page label, table/statistical row,
formula/code-like line, extraction noise, or ambiguous text. The profile also
records compact ratios for letters, digits, symbols, whitespace, token count,
and repeated character runs.

These signals are advisory. They do not suppress candidates, change candidate
promotion, or remove text. They sit beside `CandidateContentClass`,
`ArtifactRiskLabel`, and page-position summaries so the user can distinguish
text/noise evidence from repeated-page-position evidence before adding a Custom
Removal.

## Legacy candidate paths

Some repeated artefact candidates still use legacy literal removals:

- inline artefacts are added to `remove_patterns` because they occur inside
  lines rather than as standalone lines.
- two-line and three-line block candidates use the existing literal path.
- normalised candidates without an actionable normalised key fall back to
  tracked raw variants when available.

## Page-zone candidate promotion

Page-zone scoped Custom Removal rules are supported and automatically generated by the scanner interface. When promoting exact or normalised line candidates, the scope is selected conservatively:
- If a candidate's occurrences are entirely derived from documents scanned with real, page-aware text metadata, and there are no middle occurrences:
  - If occurrences are only in the top zone, the promoted scope is `PageTop`.
  - If occurrences are only in the bottom zone, the promoted scope is `PageBottom`.
  - If occurrences are in both top and bottom zones (and zero in the middle), the promoted scope is `PageTopOrBottom`.
- In all other cases (e.g. occurrences in the middle zone, missing page metadata, or fallback flat/file estimates), the promoted scope remains `WholeLine`.

This prevents aggressive automatic promotion to page zones on documents where structural page metadata is absent or where the candidates also exist within main body text.

## Reviewability and reproducibility

CorpusWright cleaning remains config-based. The same source documents and the
same saved cleaning configuration produce the same cleaned text. The app does
not silently delete repeated artefacts just because the scanner found them; a
candidate affects output only after the user promotes it or adds an explicit
Custom Removal.
