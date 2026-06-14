# Repeated Artefact Finder

The Repeated Artefact Finder scans extracted corpus text for recurring lines,
normalised line families, selected inline conversion artefacts, and optional
multi-line blocks. It returns candidates for review. A candidate does not alter
cleaned output unless the user promotes it to a Custom Removal rule.

## Candidate evidence

Each candidate carries several independent pieces of evidence:

- `CandidateContentClass` describes broad character composition: text, mixed
  text and numbers, numeric-dominant, or symbol/noise-dominant.
- `ArtifactRiskLabel` describes repeated-artefact risk, including common
  section headings, probable boilerplate, symbol/noise candidates, and strong
  header/footer candidates.
- `PositionSummary` describes where occurrences appear in the scanned text.
  Page-based summaries come only from reliable page-aware extraction metadata;
  fallback flat-text summaries are advisory and do not represent real page
  structure.
- `CandidateTextProfile` describes the candidate's local text shape with
  deterministic ratios and flags.
- `CandidateTextSignalLabel` gives a compact advisory reading of the text
  profile for review.

These signals complement each other. Page-aware evidence and text/noise
evidence are separate: a page label can be recognised from its text pattern, and
a running header can be recognised from page-edge repetition, but neither signal
silently removes text.

## Text profile

The text profile is deterministic and local. It uses character and token
inspection only; it does not use machine-learning models, external services, or
network calls.

The profile records:

- character and token counts;
- alphabetic, digit, symbol, whitespace, and uppercase ratios;
- average token length;
- the longest repeated non-whitespace character run;
- the ratio of suspicious-looking tokens;
- whether sentence punctuation is present;
- whether the text resembles a common section heading, page label,
  table/statistical row, formula/code-like line, or markup/extraction artefact.

Unicode alphabetic characters and decimal digits are counted with Unicode-aware
character checks where the scanner already needs them. The profile is intended
for review, not for deletion.

The same deterministic text/noise profiling layer is also available to the
cleaner. The cleaner's `remove_obvious_extraction_noise` flag uses a stricter
automatic-removal predicate than the advisory repeated artefact label, and only
for full lines with overwhelming extraction/OCR/markup noise signals.

## Text signal labels

The candidate text signal is conservative. It uses the profile, existing
content class, existing risk label, and position summary to choose one advisory
label:

- likely natural text;
- likely section heading;
- likely page label;
- likely table/statistical row;
- likely formula/code-like;
- likely extraction noise;
- ambiguous.

Common English, Portuguese, and French academic section headings are labelled as
section headings rather than noise. Page labels such as `Page 12 of 42`,
`p. 5`, and `Página 3 de 10` are labelled as page labels. Obvious numeric rows,
statistical rows, formula/code-like lines, HTML tags/entities, `cid:` markers,
and repeated punctuation runs receive more specific advisory labels when the
signals are strong enough. Short or weakly signalled text remains ambiguous.

Reason codes explain the label with stable, short strings such as
`page_label_pattern`, `high_symbol_ratio`, `formula_or_code_symbols`,
`markup_entity_or_tag`, `cid_marker`, `long_repeated_character_run`,
`page_edge_repetition`, `mostly_alphabetic`, and `multi_token_text`. The
frontend maps these codes to short review phrases.

## Review and removal

The text signal does not change candidate filtering, candidate promotion,
cleaning output, or page-zone behaviour. It is shown in the Repeated Artefact
Finder so the user can decide whether a candidate is likely boilerplate,
structure, data, code, markup residue, or ordinary text.

Cleaning remains config-based and explicit. Exact and normalised repeated line
candidates affect output only after promotion to structured Custom Removal
rules. Inline artefacts and block candidates continue to use their existing
removal paths. Page-zone promotion continues to depend on reliable page
metadata and the candidate's page-position summary, not on the text/noise
signal.

The processing parameter labelled "Remove obvious extraction/OCR noise" is a
separate opt-in cleaner setting. It can remove lines such as repeated symbol
junk, `cid:` markers, replacement-character junk, and standalone markup
fragments without running a repeated artefact scan. It does not broadly remove
"non-text", and it protects section headings, page labels, table/statistical
rows, formulae, code-like lines, mixed text with numbers, and ordinary prose.
