# Manner adverbs

Issue #301 adds an opt-in `adverbs` report over a retained `StyleDocument`. It lists individual manner adverbs in narration and quoted speech, their lemmas, counts, and rates per 1,000 words. It diagnoses a pattern and does not suggest replacement prose.

The inventory follows the existing dialogue-tag manner rule: a word must end in *-ly*, have an `RB` reading, and be absent from `style/data/dialogue/adverb-exclusions.txt`. A report-local exclusion also removes time/frequency words (*daily, weekly, monthly, yearly, hourly, nightly, early, only*) and lexical adjectives (*friendly, holy*). A word with a `JJ` reading directly before a noun is excluded. Words without token readings need token details and are not guessed from spelling alone. Lemmas come from the adverb reading. The report counts each word once even if the tagger has multiple readings. Skipped names are absent from counts and denominators.

`matches` contains `span`, `lemma`, `mode`, and `tagAdverb`. `tagAdverb` is true when the same span is already returned in `dialogueTags.tags[].adverb`, so an editor can avoid highlighting it twice. Quoted speech is dialogue; quoted material classified as non-speech is narration. `groups` splits all, narration, and dialogue. Each group has `words`, `count`, `perThousandWords`, and alphabetically ordered `lemmas` with their counts and rates. Empty groups have rate zero. Every denominator counts the document's eligible word tokens in that mode.

Rust uses `adverbs::report(&document, &AdverbsOptions::new())` or `StyleDocument::adverbs(options_json)`. WASM uses `document.adverbs(optionsJSON?)`, and Swift uses `document.adverbs(optionsJson:) -> AdverbsReport`. Bindings serialize spans as UTF-16 `{start,end}`. Options currently have no fields; an unknown key is an error. Compact documents return `needsTokenDetails`.

This report is an opt-in frequency lens. It does not set an editorial threshold or flag an adverb as wrong. The lexical heuristic can miss manner adverbs that do not end in *-ly* and can include an ambiguous *-ly* word when an `RB` reading remains.

## Public counts

On the six `bench/public` texts, using default document options except hard-wrapped mode for Moby:

| Text | Matches | Narration | Dialogue | Tag adverbs | Per 1,000 words |
|---|---:|---:|---:|---:|---:|
| story1 | 3 | 3 | 0 | 0 | 1.36 |
| story2 | 8 | 7 | 1 | 0 | 3.93 |
| story3 | 6 | 6 | 0 | 0 | 2.99 |
| story4 | 3 | 3 | 0 | 0 | 1.38 |
| story5 | 3 | 2 | 1 | 0 | 1.62 |
| moby150k | 241 | 193 | 48 | 1 | 9.64 |

These are observations, not editorial flags. The only tag-adverb overlap in these samples is one Moby occurrence.
