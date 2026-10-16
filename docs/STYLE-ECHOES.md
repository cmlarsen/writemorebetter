# Echoes and repeated phrases

Part of #245 and #241. Both reports use the shared document pass, return original-text spans and
never suggest replacement prose. Token details are required; compact documents return
`StyleError::NeedsTokenDetails`.

## Echoes: an opt-in lens

Echoes are a way to inspect nearby repeats in a passage, not an issue list. They are omitted from
`StyleRequest::new()`, absent-request `run_json`, and the bindings' default `styleReports` call.
Select them explicitly with `{"echoes":{}}`, or call `echoes::report` directly.

`EchoOptions` retains `window` (30 counted words, inclusive; larger requests are capped at 30) and `matchForms` (true). Every later
content word links to its nearest matching earlier word within the window. Windows cross paragraph,
sentence boundaries. Same-sentence pairs within ten words are suppressed, as are quoted pairs in different paragraphs (a conservative dialogue-turn proxy; speaker identity is not inferred). Counts and density remain available for existing consumers; density
is descriptive and must not be presented as a quality score.

Default suppressions include say/ask inflections, honorifics before capitalized words (Mr., Mrs., Ms., Dr., M.,
Monsieur, Madame and related forms; ordinary *miss* and *ladies* remain content), and particles (up, down, out, back, off, over, away).
Caller-supplied skipped names and their preceding honorifics are excluded from document counts.
Recognized headings and scene-marker lines are removed in the document pass. Callers should pass
their character/place lists through `DocumentOptions::with_skip_names`; this crate cannot read
an application's character database.

### Matching and ambiguity

Spelling is lowercased with curly apostrophes folded to straight apostrophes. With `matchForms`,
two tokens match through identical spelling, or a single unambiguous content lemma with the same noun/verb/adjective/adverb POS family. Noun/verb chunk context resolves mixed POS families before comparing lemmas, so plural nouns such as bucket/buckets and verb forms such as swung/swinging can join without crossing unrelated adjective/verb readings. Unchunked gerunds use their VBG reading. Remaining NN/VB ambiguity uses a noun key; other unresolved mixed families remain spelling-only. Comparative and superlative readings do not supply lemma keys. No transitive graph
connects different readings: a third token such as *better* cannot join *good* to *well*, and *saw*
cannot join *seen* to *saws*. An ambiguous token still yields at most one echo, against the nearest
matching token. Multiple distinct lemmas keep only their spelling key; a lemma never matches an unrelated surface key. Exact repeated spellings always match. With `matchForms:false`, only spelling matches.

The echo's `key` is the directly shared matching key; `label` names the matched words, preferring
a shared lemma that is also a matched spelling. Keys are no longer transitive, document-wide word
families. Every echo includes `earlier`, `later` and `distance`.

### Content words

The owner-authored closed-class list excludes determiners, pronouns, auxiliaries, prepositions and
light adverbs. A spelling is also excluded if it has a number reading or most occurrences have no
noun, verb, adjective or adverb reading with an unlisted lemma. Unknown words count as content.
Echo-only suppressions do not apply to repeated phrases. Nothing comes from external linguistic
word lists.

## Repeated phrases

Repeated phrases are an opt-in lens, omitted from `StyleRequest::new()` and absent-request
`run_json`/`styleReports`. Select `{"repeatedPhrases":{}}` or call `repeated_phrases::report` directly.
The 2026-10-16 re-review measured 97% noise or wrong flags (146 of 150 reviewed); this is a way to
inspect repetition, rather than a default issue list. Its options are:

| JSON option | Default | Meaning |
|---|---:|---|
| `minimumLength` | 2 | Smallest candidate n-gram |
| `maximumLength` | 5 | Largest candidate n-gram before overlap merging |
| `minimumContentWords` | 2 | Unmasked content words required |
| `minimumOccurrences` | 3 | Uses required anywhere in the document |
| `nearbyWords` | 300 | Two uses also qualify when this close |
| `automaticNames` | false | Infer recurring mid-sentence capitalized names when no roster was supplied |

Options accept snake_case aliases and have Rust builders. Candidate lengths are clamped to the
longest sentence. Phrases do not cross sentences. Distance counts word tokens, including masked
names; punctuation does not count. Periods in abbreviations such as Mrs. and Dr. do not break a phrase; honorifics before capitalized names do not count as content. Ellipses still break phrases. Commas, semicolons, dashes, ellipses and closing quotes end n-grams.

Skipped names become `<name>` positions instead of contributing content words. Attribution verbs
remain content, so *Piglet said that* and phrasal verbs such as *I got up* remain eligible.
With `automaticNames:true`, capitalized tokens with proper-noun readings recurring mid-sentence are
masked only at capitalized occurrences when `skip_names` is empty; a detected Rose does not mask a lowercase rose. This optional heuristic is a fallback for Subplot's character
roster; any nonempty caller roster disables it, even if its names do not occur in the passage. Other function
positions use spelling; unambiguous content lemmas inflect together, while ambiguous spellings keep
their own key instead of joining different readings transitively.

A candidate qualifies with at least three uses, or at least two within 300 words. Wholly contained
shorter repeats are suppressed when they overlap a longer retained repeat, except when the shorter phrase occurs at least twice as often as that longer phrase. Overlapping candidates whose
corresponding occurrences all overlap and whose keys agree are merged into one span per occurrence,
even beyond `maximumLength`. A repeated clause therefore yields one entry rather than several
overlapping five-grams. Longest retained phrases take priority over nested or overlapping shorter entries with comparable counts; frequent shorter phrases retain their full occurrence lists.

`RepeatedPhraseReport.phrases` returns `keys`, `labels`, `count` and `spans`, ordered by count,
length and first occurrence. Each span includes punctuation between its first and last word.

## Validation and overlap

The tier 2 injection suite is a regression check on public-domain passages, not a measure of
editorial usefulness. The usefulness revision intentionally removes distant twice-used phrases and
transitive lemma chains; its rebaseline and reasons are recorded in the benchmark README. Public
before/after flag counts are in the usefulness PR and `bench/style/usefulness-counts.json`.

The grammar engine's existing repeated-word rules remain unchanged. These reports diagnose prose
across the document, carry spans and provide no replacement suggestions.
