# Echoes and repeated phrases

Part of #245 and #241. Both reports use the shared document pass, return original-text spans and
never suggest replacement prose. Token details are required; compact documents return
`StyleError::NeedsTokenDetails`.

## Echoes: an opt-in lens

Echoes are a way to inspect nearby repeats in a passage, not an issue list. They are omitted from
`StyleRequest::new()`, absent-request `run_json`, and the bindings' default `styleReports` call.
Select them explicitly with `{"echoes":{}}`, or call `echoes::report` directly.

`EchoOptions` retains `window` (50 counted words, inclusive) and `matchForms` (true). Every later
content word links to its nearest matching earlier word within the window. Windows cross paragraph,
sentence and dialogue boundaries. Counts and density remain available for existing consumers; density
is descriptive and must not be presented as a quality score.

Default suppressions include say/ask inflections, common honorifics (Mr., Mrs., Ms., Dr., M.,
Monsieur, Madame and related forms), and particles (up, down, out, back, off, over, away).
Caller-supplied skipped names and their preceding honorifics are excluded from document counts.
Recognized headings and scene-marker lines are removed in the document pass. Callers should pass
their character/place lists through `DocumentOptions::with_skip_names`; this crate cannot read
an application's character database.

### Matching and ambiguity

Spelling is lowercased with curly apostrophes folded to straight apostrophes. With `matchForms`,
two tokens match only through a directly shared single-word lemma or spelling. No transitive graph
connects different readings: a third token such as *better* cannot join *good* to *well*, and *saw*
cannot join *seen* to *saws*. An ambiguous token still yields at most one echo, against the nearest
matching token. Direct homograph matches can remain ambiguous; the report does not invent a POS
disambiguation. Exact repeated spellings always match. With `matchForms:false`, only spelling matches.

The echo's `key` is the directly shared matching key; `label` names the matched words, preferring
a shared lemma that is also a matched spelling. Keys are no longer transitive, document-wide word
families. Every echo includes `earlier`, `later` and `distance`.

### Content words

The owner-authored closed-class list excludes determiners, pronouns, auxiliaries, prepositions and
light adverbs. A spelling is also excluded if it has a number reading or most occurrences have no
noun, verb, adjective or adverb reading with an unlisted lemma. Unknown words count as content.
The new suppressions apply independently of POS ambiguity. Nothing comes from external linguistic
word lists.

## Repeated phrases

`repeated_phrases::report` remains selected by default. Its options are:

| JSON option | Default | Meaning |
|---|---:|---|
| `minimumLength` | 2 | Smallest candidate n-gram |
| `maximumLength` | 5 | Largest candidate n-gram before overlap merging |
| `minimumContentWords` | 2 | Unmasked content words required |
| `minimumOccurrences` | 3 | Uses required anywhere in the document |
| `nearbyWords` | 300 | Two uses also qualify when this close |

Options accept snake_case aliases and have Rust builders. Candidate lengths are clamped to the
longest sentence. Phrases do not cross sentences. Distance counts word tokens, including masked
names; punctuation does not count. Punctuation inside a sentence is ignored.

Skipped names become `<name>` positions and detected tag verbs become `<tag>` positions instead
of breaking the sentence or contributing content words. This preserves gestures such as *shrugged
her shoulders* next to an attribution. Display labels use the placeholders too. Other function
positions use spelling; unambiguous content lemmas inflect together, while ambiguous spellings keep
their own key instead of joining different readings transitively.

A candidate qualifies with at least three uses, or at least two within 300 words. Wholly contained
shorter repeats are suppressed when every occurrence is covered. Overlapping candidates whose
corresponding occurrences all overlap and whose keys agree are merged into one span per occurrence,
even beyond `maximumLength`. A repeated clause therefore yields one entry rather than several
overlapping five-grams. Different occurrence sets stay separate, preserving a shorter gesture that
also occurs on its own.

`RepeatedPhraseReport.phrases` returns `keys`, `labels`, `count` and `spans`, ordered by count,
length and first occurrence. Each span includes punctuation between its first and last word.

## Validation and overlap

The tier 2 injection suite is a regression check on public-domain passages, not a measure of
editorial usefulness. The usefulness revision intentionally removes distant twice-used phrases and
transitive lemma chains; its rebaseline and reasons are recorded in the benchmark README. Public
before/after flag counts are in the usefulness PR and `bench/style/usefulness-counts.json`.

The grammar engine's existing repeated-word rules remain unchanged. These reports diagnose prose
across the document, carry spans and provide no replacement suggestions.
