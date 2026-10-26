# Name spelling

Issue #375 adds an **opt-in lens** for spellings near the author's own character and place names:
*Marra* beside a listed *Mara*, *Kathryn* beside *Katherine*, or *Zoe* beside *Zoë*. The report
names a roster entry and counts its forms across the document. It diagnoses a possible inconsistency;
it never constructs replacement prose. The idea comes from rule 1 of
[dictation-cleanup-rules](https://github.com/AbhishekBarali/dictation-cleanup-rules) (MIT), but its
dictation scorer is not ported: a probe on `bench/public` matched *the*, *to* and *they* to *Theo*
193 times in one roughly 2,000-word story.

The implementation covers folded exact matches, typed errors, and English phonetic respellings.

| Surface | Call |
|---|---|
| Rust | `name_spelling::report(&Document, &NameSpellingOptions)` |
| Retained Rust | `document.name_spelling(options_json)` |
| WASM | `document.nameSpelling(optionsJSON?)` → `NameSpellingReport` |
| Swift | `document.nameSpelling(optionsJson:) -> NameSpellingReport` |
| Python | `document.name_spelling(options_json=None)` → report dictionary |

The speller already searches its user-dictionary entries when suggesting corrections
(`nlprule/src/spell/mod.rs`, `UserMatcher::suggestion_fst`). Subplot can feed its roster to both
`addWords` and `skipNames`. The speller can then suggest *Mara* for an unknown *Marra*. This report
adds a whole-document view, always identifies the listed name, and can flag a near spelling that
the speller accepts as a word or name. Whether particular names are present in the built English
speller data still needs checking. Subplot should dedupe overlapping speller and report flags by span.

## Roster and document

`DocumentOptions.skip_names` (`skipNames` in JSON) is the sole v1 roster input. Add
`Document.skip_names: Vec<String>` with `#[serde(default)]` and populate it in `analyze_document`;
`Document` currently retains only `skip_names_supplied`. Keeping the full list also covers a name
that never appears correctly in the text. This adds a field to serialized `analyzeDocument` output.
It does not change skip matching or any other report's counts. An empty roster returns an empty
report. A multi-word roster entry is one entry: list `Mara` separately if lone *Mara* should be
recognized or skipped.

The report normalizes each roster entry before matching: it trims the entry, collapses each inner
run of whitespace to one space, and drops entries left empty. Duplicates are removed by the exact
normalized string, keeping the first. The report's `names` hold the normalized strings, so
`"Van  der Berg"` is reported, and matched, as `Van der Berg`. This normalization is the report's
own; `skip_names_supplied` and the document's skip matching are unchanged. A document form with
the same inner whitespace runs, such as *Van  der Berg* for that entry, is normalized the same way
before the exact lookup, so it counts as the listed name, as the skip matcher already treats it.

Use `Sentence.tokens` for word text and spans, `is_skipped` for listed occurrences, and
`Document::mode_at` for narration/dialogue. Exact listed forms are never flags; count them under
their roster entry without counting skipped honorifics or possessive clitics as names. Count every
flagged occurrence under its chosen entry too. Work on distinct
candidate forms once per call, then map the result back to token spans. Run on compact documents:
token readings improve the ordinary-word gate, and document-local lowercase use supplies its
fallback. No report-specific data or name list is bundled.

## Matching

For each word token, require an initial capital and `!is_skipped`. Reject it as an ordinary word
when its lowercase form occurs lowercase elsewhere in the document or any available token reading
is a common-word reading rather than `NNP`/`NNPS`. The tagger reads most capitalized
mid-sentence words as `NNP` (*Part*, *Port*, *Lady*), and one chapter may never use the word
lowercase, so a single-token candidate is also rejected when it is a lemma in the bundled
concreteness table (14,572 fiction lemmas without proper nouns), an honorific, or a month, weekday
or *Christmas*. The lemma test reuses the concreteness table the diction report already bundles, so
the report ships no word list of its own; the first call in a process parses that table once, about
3.9 ms, and later calls reuse it. A multi-word run is exempt from that lemma test, since its other components must
already match the listed phrase (*Van der Breg*). `includeDictionaryWords` relaxes the reading and
lemma tests, but document-local lowercase uses still protect ordinary prose. A form that is a listed
name word plus an English derivational ending (`s`, `es`, `er`, `ers`, `ian`, `ians`, `ish`: *the
Stirlings*, *Nantucketers*, *Mingottian*) is a family plural, demonym or adjective, not a
misspelling; it gets no fuzzy match. Sentence-initial capitals
are allowed through only when this gate leaves them name-like. An all-caps form of a listed name
counts as that name, as does capitalizing only the first letter of a listed lowercase-initial
name at a sentence start. Roster entries always win: a variant that is itself listed is counted
as that name, never flagged against another.

Apply these comparisons in order:

1. **Folded exact:** normalize to NFC inside this report, then compare case-folded and
   diacritic-folded keys; for adjacent name-like tokens, also compare keys without spaces and
   apostrophe punctuation. Report `reason: "diacritics"`, `"case"` or `"spacing"` according to
   the visible surface difference. Canonically equivalent spellings, such as NFD and NFC *Zoë*,
   count as the listed name without a flag. These
   comparisons work for names shorter than four letters. Keep the
   existing document skip rules unchanged: NFC handling here must not alter other reports' counts.
   Match runs of adjacent words, longest first, so *Van der Berg* and *Vanderberg* can meet
   without absorbing an unrelated neighbor. The longest run tried is the longest of three
   bounds. The first is the longest roster entry, counted in units: words split at whitespace
   and at hyphens (`-`, U+2010 and U+2011), so *Anna Maria de Sousa* allows four and *Mary-Ann*
   two. The second is a whole hyphenated word, since syllabified dialogue can split a listed word
   into more parts (*Val-an-cy*). The third extends the run while its words joined are the start
   of a listed entry's joined key, so a one-word *Vanderberg* still meets *Van der Berg* and
   *Maryann* meets *Mary Ann*. A run crosses only whitespace, apostrophe punctuation, or one
   hyphen between two parts. The tokenizer splits U+2011 into its own token; a lone hyphen token
   joins the words on both sides as one hyphenated word, so a non-breaking *Mary‑Ann* counts as a
   listed *Mary-Ann* and *Val‑an‑cy* is a spacing variant of *Valancy*. Hyphen-joined listed
   words are spacing variants with either hyphen: *Jane-Hart* and *Jane‑Hart* (U+2011) for
   *Jane Hart*. The full matched run supplies the span.
2. **Tier 1, typed errors:** use Damerau's optimal string alignment (OSA) distance on Unicode
   scalar values. Both candidate and roster name must have at least four letters. Allow distance
   at most 1 for a listed name under 8 letters and at most 2 otherwise. Require a shared first
   sound from the CMUdict-derived `initial_sound()` table already used by alliteration, with its
   spelling fallback for names absent from the table. Length and first-sound checks precede OSA.
3. **Tier 2, respellings:** compare primary and alternate Double Metaphone codes for English
   names, after the same candidate and first-sound gates. As a second bound, require
   Jaro-Winkler similarity of at least 0.85 between the two names' *transcripts*: the whole
   primary code, without the four-symbol limit, with each vowel letter kept in place
   (*Catherine* `KA0ERINE`, *Kathryn* `KA0RYN`). The codes drop vowels, and the bound limits
   those vowel and syllable differences relative to length. It does not count spellings the
   code already treats as one sound: *C*/*K*, *PH*/*F*, doubled letters, silent *GH*. This
   catches variants such as *Kathryn*/*Katherine*, *Catherine*/*Kathryn* and *Stephen*/*Steven*
   beyond tier 1. The 0.85 Jaro-Winkler bound was chosen after looking at the books' flags, so
   the books evaluation below is not held out for that choice. The transcript is not part of Double Metaphone. The style crate implements the
   contextual rules described in [Lawrence Philips' original article](https://jacobfilipp.com/DrDobbs/articles/CUJ/2000/0006/philips/philips.htm),
   without depending on `rphonetic` or copying a port.
   Put the phonetic operation behind a per-language seam; English supplies Double Metaphone, and
   other languages need their own implementation later.

Exact folded matches take priority over edit, which takes priority over phonetic. Rank eligible
roster names by tier and then distance; a tied best rank keeps one deterministic `name` and puts
the other tied entries in `alternatives`. The UI must not present a tie as certain.

**Frequent near misses are listed, not flagged** (owner decision, 2026-10-26, replacing
"flag regardless of frequency"). Count each distinct edit or phonetic form, by its exact
surface and matched entry, across the whole document; possessive occurrences count toward the
form. A form used at least `otherCharacterMinimum` times (default 3) is not flagged. It is listed
under its roster entry in `possibleCharacters` as possibly another character, and the author can add it
to the roster. Forms used fewer times are flagged as before. A one-off *Mary* near listed *Mara* is still an
expected flag. Case, diacritic and spacing variants are exempt, so they are flagged however often
they occur: they spell the listed name itself (*Zoe* for *Zoë*, *Vanderberg* for *Van der Berg*),
and no other character can be written that way. The count is per form, not per entry: *Marra* ×2
and *Marro* ×1 near *Mara* are three flags. A form tied between entries is listed only under the
entry its flag would have named. Because the count is per document, a caller that checks one
chapter at a time sees only that chapter's uses.

Multi-word fuzzy matching
checks a listed phrase when only one component differs and all others match; it does not infer
standalone roster names from the phrase. Every compared word of a fuzzy multi-word candidate needs
an initial capital, and a capitalized later word must not occur lowercase in the document. The one
exception is a lowercase particle that is exactly the listed entry's own lowercase word at that
place: *de* in *Anna Maria de Sousa*, *van* or *der*. So *Jane hurt* is not a near miss of
*Jane Hart*, and *da Sousa* is not one of *de Sousa*. A later part of a hyphenated word is not a
separate word, so *Mary-ann Smyth* is still a near miss of *Mary-Ann Smith*.

Possessive clitics (`'s`, `’s`, trailing apostrophes) stay outside the flag span. Hyphenated
parts use the same UTF-8-safe subspan construction as regional spelling. Build multi-token spans
from the first and last token spans. Never slice text using unchecked byte offsets. Test curly
quotes and apostrophes, em dashes, ellipses, U+2028, NFC and NFD spellings, and UTF-16 offsets.

## Result and options

`NameSpellingReport` has `flags` and `names`. A `NameSpellingFlag` has `span`, global `sentence`
index, `form` (the flagged surface), `name` (the exact listed spelling), `alternatives` (other
equally ranked listed names), `reason` (`edit`, `diacritics`, `case`, `spacing` or `phonetic`),
optional `distance` for edit matches, and `mode`. `NameSpellingName` has the listed `name`,
`counts: Split<usize>` for correct occurrences, `variants` for flagged forms, and
`possibleCharacters` for frequent near misses that are not flagged. Both lists hold the same
record: `form`, `reason`, `counts: Split<usize>` and `spans`. They are kept separate so `variants`
always matches `flags`, and a caller can show the list without filtering. Return roster entries even at zero count so a caller
can see which names were checked. `Split` gives all, narration and dialogue counts. The listed
name is the author's own string, not generated prose.

Results and options use camelCase JSON, with snake_case option aliases and
`#[serde(deny_unknown_fields)]`; unknown options return `invalidOptions`. Binding spans are
UTF-16 `{start, end}` in WASM and Swift and character offsets in Python, following regional
spelling. Add the typed Swift result and `wasm/style-results.d.ts` declarations alongside the
bindings.

| Option | Default | Meaning |
|---|---|---|
| `includeDictionaryWords` | `false` | Allow capitalized forms with common-word token readings into fuzzy matching. Lowercase document uses still exclude ordinary words. |
| `phonetic` | `true` | Run tier 2 after folded exact and edit matching. |
| `otherCharacterMinimum` | `3` | An edit or phonetic form used at least this many times in the document moves from `flags` to its entry's `possibleCharacters`. `0` never moves one. |

The four-letter minimum and tier 1 distances are fixed in v1. Short names still receive folded
exact checks. A caller wanting *Mary* and *Mara* treated as separate characters lists both.

## Evaluation plan

Use the tier 2 JSONL framework in `bench/style` with exact byte and UTF-16 spans, seeded
injections into `bench/public` and pinned public-domain chapters, and untouched paragraphs as
controls. Plant deletions, insertions, substitutions, transpositions, phonetic respellings,
diacritic changes, case changes, and joined or split names. Include possessives and Unicode
punctuation. A hand-built suite covers every reason, ranking ties, compact documents, and
nearby roster pairs. Hand-judge natural-text flags with real rosters; an optional owner-run tier 3
pass on private drafts exports aggregate counts only.

| Measure | Pass gate |
|---|---|
| Hand-built cases | 100% |
| Planted single-edit errors on names of at least four letters | Recall at least 95% |
| Planted phonetic respellings with tier 2 | Recall at least 80% |
| Planted diacritic, case and spacing variants | Recall 100% |
| Correct roster suggestion | Intended name is `name` or appears in `alternatives` on a tie, 100% |
| Untouched controls and ordinary-word controls | At most 1 unexpected flag per 10,000 words |
| Nearby names both listed | Zero flags for their correct spellings |
| Unlisted real-name near misses, such as *Mary* with only *Mara* listed | Expected flags below `otherCharacterMinimum` uses, listed at or above it; excluded from the false-positive count |
| Natural-text hand review | At least 80% useful flags, with unlisted real-name flags recorded separately for later review |
| Native release time on `moby150k` with a 200-name roster | Under 10 ms |

Freeze test-split predictions on untouched public paragraphs, as the lexical suites do. The
Python prototype caught 98% of 706 planted typos with the tier 1 gate, with no flags in five
public stories and one in the public Moby-Dick excerpt. These are prototype measurements, not
Rust results or evidence for tier 2. Measure report time and WASM size when implemented. Run
`bench/tools/gates.sh` after any data rebuild; this design changes no grammar rules or data.

The folded and edit matching baseline on this Apple Silicon Mac: the release report took 8.0–9.0 ms on
`bench/public/moby150k.txt` with 200 entries, using both synthetic `Name000`-style entries and
200 distinct capitalized forms from that public text (three runs each; analysis excluded).
The folded/edit release WASM baseline is 7,890,186 bytes raw and 2,043,283 bytes with gzip.

### Evaluation with other characters listed (2026-10-26)

This replaces the first phonetic evaluation (also 2026-10-26), whose figures are kept below for
comparison. Since then, frequent near misses are listed instead of flagged, and the phonetic
second bound uses transcripts.

`python3 bench/style/name_spelling.py --gate` uses seed 375 and a held-out split of 12 public-story
chunks. Each planted item follows untouched public prose containing an emoji, curly quotes, an em
dash, an ellipsis and U+2028. Exact byte and UTF-16 spans are scored. The development split has
the same size and recall. Every planted form occurs once in its document, so the
`otherCharacterMinimum` default never applies to it. The 18 hand-built integration cases and the
published phonetic-code fixtures also pass.

| Measure | Pass gate | Held-out result |
|---|---:|---:|
| Hand-built cases | 100% | 18/18 pass |
| Single-edit errors, names ≥4 letters | ≥95% recall | 48/48 (100%) |
| Phonetic respellings, tier 2 enabled | ≥80% recall | 444/456 (97.4%, Wilson 95.5–98.5%), 37 of 38 pairs; was 408/456 (89.5%) |
| Diacritic, case and spacing variants | 100% recall | 36/36 (100%) |
| Correct roster suggestion | 100% of caught items | 552/552 (100%) |
| Untouched appended controls | ≤1 unexpected flag / 10,000 words | 0/2,232 words; at scale, 1 not-useful flag in 853,767 words (0.01) |
| Nearby names both listed | Zero flags | 0/12 flags |
| Unlisted real-name near misses | Expected flags | 12/12 one-off *Mary* → *Mara*; in the books, 29 flagged and 4 forms listed, below |
| Natural-text hand review with real rosters | ≥80% useful | 5/6 (83.3%) on all 35 book flags; Wilson 95% interval 44–97%, so n is too small to show the gate is met |
| Native release time, `moby150k`, 200-name roster | <10 ms | Median of 19 warm calls: 5.2 ms synthetic, 7.0 ms real capitalized forms, after computing each sentence's units once and deferring key allocation past the cheap rejections (7.9 and 8.8 ms before, same machine). Bounding runs by joined roster prefixes, so one-word entries meet spaced variants, measured 5.4 ms synthetic and 6.5 ms real against 5.4 and 5.9 ms just before it in one session; first call in a process about 13 ms, of which 3.9 ms parses the bundled concreteness table (analysis excluded) |

The correct-suggestion gate was stated against all planted items while recall was 100%. Missed
phonetic pairs count against recall instead, and the gate checks every caught item.

**Phonetic recall.** The suite plants 38 distinct respelling pairs of common English names, written
for this suite rather than taken from a dataset, 12 times each. Both tiers are on.

| Tier that caught the pair | Pairs | Pairs |
|---|---:|---|
| Edit (one or two edits) | 15 | Christine/Kristine, Christopher/Kristopher, Cynthia/Synthia, Frederick/Fredrik, Gerald/Jerald, Jillian/Gillian, Jonathan/Johnathon, Josephine/Josefine, Kathleen/Cathleen, Madeline/Madelyn, Marcus/Markus, Nicholas/Nikolas, Philippa/Filippa, Shaun/Shawn, Zachary/Zackary |
| Phonetic | 22 of the other 23 (95.7%) | Allison/Alyson, Ashley/Ashleigh, Caitlin/Kaitlyn, Catherine/Kathryn, Cecily/Cecilie, Deborah/Debra, Eleanor/Elinor, Elliott/Eliot, Hailey/Haleigh, Isaac/Izaak, Jasmine/Jazmyn, Jeffrey/Geoffrey, Joseph/Josef, Katherine/Kathryn, Michael/Mikael, Phillip/Filip, Raphael/Rafael, Rebecca/Rebekah, Sophia/Sofia, Stephen/Steven, Stuart/Stewart, Tiffany/Tiphanie |
| Missed | 1 | Sean/Shawn |

Why the four pairs missed, and what changed:

- **Catherine/Kathryn, Hailey/Haleigh, Tiffany/Tiphanie: the second bound.** The codes agree
  (`K0RN`, `HL`, `TFN`), and so do the first sounds. The bound was "OSA ≤ 3 or Jaro-Winkler ≥ 0.85"
  on the spellings: OSA 4 and Jaro-Winkler 0.76, 0.80 and 0.69. On spellings, the bound counted
  *C*/*K*, *PH*/*F*, *Y*/*I*, doubled letters and silent *GH* as differences, though the code had
  already judged them one sound. It now uses Jaro-Winkler ≥ 0.85 on the transcripts. The three
  pairs score 0.87, 0.88 and 0.91. All 19 pairs caught before score at least 0.87.
  `respellings_beyond_the_spelled_distance_match` and `transcripts_keep_vowels` are the tests.
- **The absolute OSA ≤ 3 alternative was dropped.** Three edits are most of a short transcript.
  On the books, keeping it alongside transcripts newly matched *Louisa* and *Lewis* to *Lizzy*
  (`LOUISA`/`LISY`, OSA 3). Without it, *Madeira*/*Medora* (0.80) and *Santa*/*Snaith* (0.62), both
  flagged before and not useful, fall outside the bound. *Lewis*/*Lizzy* scores exactly 0.85 and
  is still matched. This bound was chosen after seeing the book flags, so the book precision
  below is not an independent test of it.
- **Sean/Shawn: the encoder, and not fixable in principle here.** Philips' rules code an initial
  *S* before a vowel as `S` and *SH* as `X`, so the keys are `SN` and `XN`. The first-sound gate
  agrees (CMUdict gives *Sean* an *sh* onset), and this repo's encoder is correct to the published
  rules. Comparing the keys after the onset would only help if a short pair could then pass the
  bound. *Sean*/*Shawn* (`SEAN`/`XAN`, OSA 2, Jaro-Winkler 0.72) is no closer than *Lewis*/*Lizzy*
  (OSA 2), so any bound that admits it also admits that class of different names. The pair stays
  missed, and `short_respellings_outside_the_bound_are_not_matched` records the limit. A caller
  can list both spellings.

Stuart/Stewart was fixed in the previous round: Philips codes W only at a word start, in WR, in a
final W after a vowel (alternate F) and in Polish -EWSKI and -WICZ (`w_rules`,
`medial_w_is_silent_in_phonetic_codes`). The encoder now follows Philips for two more initial
rules (`initial_rules`): only *GN*, *KN*, *PN*, *WR* and *PS* skip their first letter, so *Aerin*
keeps its initial vowel key and meets *Erin* (`ARN`); and *CH* codes `K` at the start or after
*A*, *O*, *U* or *E* when a consonant or the word end follows, and before *T* or *S*
(*Christopher* and *Chris* `KRST` and `KRS`, *Bach* `PK`). One known deviation remains: an initial
*CH* otherwise has the alternate `K`, where Philips gives `X` alone. The held-out predictions and
recall did not change.

**Precision at scale.** `python3 bench/style/name_spelling_books.py --check` runs the release
report over bench/public and the 13 public-domain books and chapters in `sources.json`, one
document per chapter, with each text's main characters and places from
`bench/style/name-spelling-rosters.json` (4 to 26 entries per text). One document per chapter is the
stricter case, since lowercase uses elsewhere in a book cannot protect a word. `moby150k`
overlaps the opening of *Moby-Dick*. Every flag is judged in
`bench/style/labels/name-spelling-judgments.jsonl`, since there are fewer than the 150-flag sample
size. A flag is **useful** when it is a variant of the listed entity's own name. It is an
**unlisted name** when the form is itself another real personal name (the *Mary*/*Mara* class,
outside the rate). Anything else is **not useful**. The script also reports the listed
possible characters, and runs a second pass with each book as one document.

| Text | Words | Flags | per 10,000 | Useful | Not useful | Unlisted name | Listed (uses) |
|---|---:|---:|---:|---:|---:|---:|---|
| story1–5 | 10,179 | 0 | 0.00 | 0 | 0 | 0 | |
| moby150k | 25,264 | 2 | 0.79 | 1 | 1 | 0 | |
| moby | 205,102 | 1 | 0.05 | 0 | 0 | 1 | |
| pride | 121,450 | 3 | 0.25 | 0 | 0 | 3 | |
| blue_castle | 68,011 | 6 | 0.88 | 1 | 0 | 5 | *Stalling* in two chapters (11, 23) |
| ackroyd | 69,294 | 2 | 0.29 | 2 | 0 | 0 | *Porrott* (6) |
| age_of_innocence | 101,176 | 21 | 2.08 | 1 | 0 | 20 | *Olenski* in one chapter (4) |
| thirty_nine_steps, hound, silas_marner, treasure_island, 4 chapters | 253,291 | 0 | 0.00 | 0 | 0 | 0 | |
| **all** | 853,767 | 35 | 0.41 | 5 | 1 | 29 | 4 entries, 44 uses |

Useful (5): *Fernley* for *Fernly* and *Ferrar’s* for *Ferrars*, inconsistencies in the source
text; *MOBY-DICK* for *Moby Dick*; syllabified *Val-an-cy* and *O-len-ska*. Not useful (1): the
French demonym *Nantuckois*. Unlisted names (29): *Olenski* for *Olenska* ×17 (her husband; one
to three uses per chapter), *Lewis* for *Lizzy* ×3 (Sir Lewis de Bourgh), *Stalling* for
*Stirling* ×3 (chapters with one or two uses), *Dossie* and *Jane* ×2 each, *Ahaz* and *Manzoni*.
The useful rate is 83.3% (5/6), Wilson 95% interval 44–97%, against 68.8% (11/16) before.

From the previous 107 flags: 44 moved to the list (*Stalling* ×34, *Porrott* ×6, *Olenski* ×4).
The transcript bound removed 31 (*Bernie* for *Barney* ×25, *Madeira* ×3, *Elaine* ×2, *Santa*)
and added *Lewis* ×3. *Porrott*, *Poirot* as another character mispronounces it, was useful as
six flags and is now one listed entry. The author still sees it under *Poirot*, but not as a
flag. *Bernie*, an unlisted name, is now neither flagged nor listed (`PERNIE`/`PARNEY`,
Jaro-Winkler below 0.85).

Run with each book as one document, the count sees every chapter: 12 flags (0.14 per 10,000
words), 5 useful, 1 not useful and 6 unlisted names (*Dossie*, *Jane* ×2 each, *Ahaz*,
*Manzoni*). The 4 listed forms are *Stalling* ×37, *Olenski* ×21, *Porrott* ×6 and *Lewis* ×3, 67 uses. The
useful rate is the same 5/6. Unlisted names in one-chapter documents dominate the remaining
flags, so a caller that checks a whole manuscript gets the fuller benefit of the list.

The first phonetic evaluation (2026-10-26) had 107 flags, 1.25 per 10,000 words: 11 useful, 5 not
useful and 91 unlisted names. The useful rate was 68.8% (11/16, Wilson 44–86%). Phonetic recall
was 408/456, 34 of 38 pairs. Before the derived-form and lemma fixes in that round, the same run
produced 373 flags (4.37 per 10,000 words), of which 14 were useful. Derived forms of listed names
(*Stirlings* ×37, *Luydens* ×33, *Mingotts* ×30, *Nantucketer* ×22 and 22 other forms) accounted
for 252. Capitalized common or calendar words accounted for 14: *June* ×4, *Lady* ×2, *John* ×2,
*Part*, *Port*, *Harrow*, and *Sterling* ×3. *Sterling* was useful: a letter misaddresses *Miss
Stirling*. It is the one useful flag the lemma gate loses; a misspelling that is itself a common
word is out of reach without `includeDictionaryWords`.

The held-out natural predictions are frozen by count and exact-span digest in
`bench/style/name-spelling-baseline.json`; the test fails if those predictions change. Neither
change altered them (5 flags, same digest; none is a near miss used three times), so the baseline
was not refreshed.

The release WASM binary is now 7,922,724 bytes raw and 2,052,475 bytes with gzip, an increase of
10,013 and 1,829 bytes over the first phonetic build (7,912,711 and 2,050,646). That build was
12,525 and 7,363 bytes over the folded/edit baseline above.

## Out of scope for v1

- Roster-free detection and a list of likely proper-noun variants.
- Treating `Mara Vale` in the roster as implying separate `Mara` or `Vale` entries. That would
  change existing skip and word-count behavior across reports.
- A speller-dedupe option. Subplot should dedupe overlapping flags by span.
