# Name spelling

Issue #375 adds an **opt-in lens** for spellings near the author's own character and place names:
*Marra* beside a listed *Mara*, *Kathryn* beside *Katherine*, or *Zoe* beside *Zoë*. The report
names a roster entry and counts its forms across the document. It diagnoses a possible inconsistency;
it never constructs replacement prose. The idea comes from rule 1 of
[dictation-cleanup-rules](https://github.com/AbhishekBarali/dictation-cleanup-rules) (MIT), but its
dictation scorer is not ported: a probe on `bench/public` matched *the*, *to* and *they* to *Theo*
193 times in one roughly 2,000-word story.

The current implementation covers folded exact matches and tier 1 typed errors. Tier 2
phonetic matches and their option follow in N3.

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
is a common-word reading rather than `NNP`/`NNPS`. `includeDictionaryWords` relaxes the reading
test, but document-local lowercase uses still protect ordinary prose. Sentence-initial capitals
are allowed through only when this gate leaves them name-like. All-caps forms need their own case
check rather than being treated as an edit typo. Roster entries always win: a variant that is
itself listed is counted as that name, never flagged against another.

Apply these comparisons in order:

1. **Folded exact:** normalize to NFC inside this report, then compare case-folded and
   diacritic-folded keys; for adjacent name-like tokens, also compare keys without spaces and
   apostrophe punctuation. Report `reason: "diacritics"`, `"case"` or `"spacing"` according to
   the surface difference (including a Unicode normalization difference under `diacritics`). These
   comparisons work for names shorter than four letters. Keep the
   existing document skip rules unchanged: NFC handling here must not alter other reports' counts.
   Match up to three adjacent tokens, longest first, so *Van der Berg* and *Vanderberg* can meet
   without absorbing an unrelated neighbor. The full matched run supplies the span.
2. **Tier 1, typed errors:** use Damerau's optimal string alignment (OSA) distance on Unicode
   scalar values. Both candidate and roster name must have at least four letters. Allow distance
   at most 1 for a listed name under 8 letters and at most 2 otherwise. Require a shared first
   sound from the CMUdict-derived `initial_sound()` table already used by alliteration, with its
   spelling fallback for names absent from the table. Length and first-sound checks precede OSA.
3. **Tier 2, respellings:** compare primary and alternate Double Metaphone codes for English
   names, after the same candidate and first-sound gates. Require either OSA distance at most 3
   or Jaro-Winkler similarity at least 0.85 as a second bound. This catches variants such as
   *Kathryn*/*Katherine* and *Stephen*/*Steven* beyond tier 1. Write the algorithm in the style
   crate from its published description rather than depending on `rphonetic` or copying a port.
   Put the phonetic operation behind a per-language seam; English supplies Double Metaphone, and
   other languages need their own implementation later.

Exact folded matches take priority over edit, which takes priority over phonetic. Rank eligible
roster names by tier and then distance; a tied best rank keeps one deterministic `name` and puts
the other tied entries in `alternatives`. The UI must not present a tie as certain. Flag an
unlisted real name one edit from a listed name, even when it appears often: *Mary* near listed
*Mara* is an expected v1 flag. There is no frequency suppression. Multi-word fuzzy matching
checks a listed phrase when only one component differs and all others match; it does not infer
standalone roster names from the phrase.

Possessive clitics (`'s`, `’s`, trailing apostrophes) stay outside the flag span. Hyphenated
parts use the same UTF-8-safe subspan construction as regional spelling. Build multi-token spans
from the first and last token spans. Never slice text using unchecked byte offsets. Test curly
quotes and apostrophes, em dashes, ellipses, U+2028, NFC and NFD spellings, and UTF-16 offsets.

## Result and options

`NameSpellingReport` has `flags` and `names`. A `NameSpellingFlag` has `span`, global `sentence`
index, `form` (the flagged surface), `name` (the exact listed spelling), `alternatives` (other
equally ranked listed names), `reason` (`edit`, `diacritics`, `case` or `spacing`; `phonetic` joins in N3),
optional `distance` for edit matches, and `mode`. `NameSpellingName` has the listed `name`,
`counts: Split<usize>` for correct occurrences, and `variants`; each variant records `form`,
`reason`, `counts: Split<usize>` and `spans`. Return roster entries even at zero count so a caller
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
| `phonetic` (planned for N3; currently rejected) | `true` once added | Run tier 2 after folded exact and edit matching. |

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
| Unlisted real-name near misses, such as *Mary* with only *Mara* listed | Expected flags, excluded from the false-positive count |
| Natural-text hand review | At least 80% useful flags, with unlisted real-name flags recorded separately for later review |
| Native release time on `moby150k` with a 200-name roster | Under 10 ms |

Freeze test-split predictions on untouched public paragraphs, as the lexical suites do. The
Python prototype caught 98% of 706 planted typos with the tier 1 gate, with no flags in five
public stories and one in the public Moby-Dick excerpt. These are prototype measurements, not
Rust results or evidence for tier 2. Measure report time and WASM size when implemented. Run
`bench/tools/gates.sh` after any data rebuild; this design changes no grammar rules or data.

N2 measurement on this Apple Silicon Mac: the release report took 8.0–9.0 ms on
`bench/public/moby150k.txt` with 200 entries, using both synthetic `Name000`-style entries and
200 distinct capitalized forms from that public text (three runs each; analysis excluded).
The release WASM binary is 7,890,186 bytes raw and 2,043,283 bytes with gzip. These are
current build sizes, not a before/after delta.

## Out of scope for v1

- Roster-free detection and a list of likely proper-noun variants.
- Treating `Mara Vale` in the roster as implying separate `Mara` or `Vale` entries. That would
  change existing skip and word-count behavior across reports.
- A speller-dedupe option. Subplot should dedupe overlapping flags by span.
