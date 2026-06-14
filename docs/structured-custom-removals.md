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

The active structured cleaning path is `RemovalScope::WholeLine`. Whole-line
rules compare against the trimmed line content and remove the entire matching
line. They do not remove text embedded inside a sentence.

The structured schema also serialises `anywhere`, but current structured rule
application only removes enabled `whole_line` rules. Literal substring removals
that work anywhere in the text continue to use `remove_patterns`.

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

## Legacy candidate paths

Some repeated artefact candidates still use legacy literal removals:

- inline artefacts are added to `remove_patterns` because they occur inside
  lines rather than as standalone lines.
- two-line and three-line block candidates use the existing literal path.
- normalised candidates without an actionable normalised key fall back to
  tracked raw variants when available.

Page-zone scoped Custom Removal rules are not part of current cleaning
behaviour. The scanner reports estimated top and bottom positions for review,
but promoted repeated artefacts still become whole-line rules.

## Reviewability and reproducibility

CorpusWright cleaning remains config-based. The same source documents and the
same saved cleaning configuration produce the same cleaned text. The app does
not silently delete repeated artefacts just because the scanner found them; a
candidate affects output only after the user promotes it or adds an explicit
Custom Removal.

