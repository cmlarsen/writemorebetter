# Hidden verbs

Issue #307 (part of #296) adds an **opt-in lens** that finds nominalizations doing a verb's work:
*made a decision*, *gave an explanation*, *came to the realization*, *the destruction of*. It
diagnoses only. Each match names the related verb (`term`: *decide*, *explain*) so a writer can see
what is hidden, but the report never rewrites the sentence or suggests replacement text. Fiction uses
these on purpose (formal narrators, period dialogue), and they are rare: about 0.7 per 1,000
narration words in published books below.

| Surface | Call |
|---|---|
| Rust | `hidden_verbs::report(&Document, &HiddenVerbsOptions)` |
| Retained Rust | `document.hidden_verbs(options_json)` |
| WASM | `document.hiddenVerbs(optionsJSON?)` → `HiddenVerbsReport` (`wasm/style-results.d.ts`) |
| Swift | `document.hiddenVerbs(optionsJson:) -> HiddenVerbsReport` |

The report needs token readings (`StyleError::NeedsTokenDetails` on compact documents). Options are
camelCase JSON with snake_case aliases. Unknown keys are `invalidOptions`.

| Option | Default | Meaning |
|---|---|---|
| `ofPhrases` | `true` | Include `ofPhrase` matches (*the arrival of*). `false` keeps light verbs only. |

## Result

`{matches, terms, groups}`. Each match has:

- `span`: from the light verb to the noun (`made a quick decision`), or from the determiner to *of*
  (`the destruction of`).
- `noun` and `lightVerb`: the nominalization's span, and the light verb's (null for `ofPhrase`).
- `nominalization`: the noun's lemma (`decision`); `term`: the verb WordNet derives it from
  (`decide`).
- `pattern`: `lightVerb` or `ofPhrase`.
- `sentence`: the global sentence index, and `mode`.

`terms` has one row per nominalization (`term`, `nominalization`, `count`, `narration`,
`dialogue`), most frequent first. `groups: Split<HiddenVerbsGroup>` holds `matches`, `words` and
`perThousandWords` (null when there are no words). **Narration and dialogue are reported
separately**, by the document's speech mode; a match whose two ends fall in different modes is
dropped. Skipped names are never matched. Spans are UTF-16 `{start, end}` in the bindings.

## What counts

A noun from the bundled table, read as a common noun (lemma, so *decisions* counts), in one of two
frames inside one sentence:

- **Light verb.** Any form of *make, take, give, have, conduct, perform, reach, offer, provide,
  undertake*, *carry out* or *come to*, then up to four determiners, possessives, indirect objects
  (*her*, *him*, *them*), adjectives, adverbs or numbers, then the noun: *made a decision*, *gave her
  an explanation*, *had a long discussion*, *came to the realization*, *carried out an
  investigation*. Not counted:
  - a noun that modifies another noun (*took the examination room*);
  - causative *make* and *have*, with an adjective, participle or *more*/*less* after the noun
    (*made his isolation more complete*, *have the inquiry hampered*);
  - *have* without an article or negation, or with a possessive (*has expectations*, *had your
    statement*); with one it counts (*had no intention*, *have an argument*);
  - *make decision(s) about* with no determiner, which LanguageTool's `MAKE_DECISIONS_ABOUT` flags.
- **Of-phrase.** A determiner or possessive (*the, a, this, his, their, any, no, …*), up to two
  adjectives, the noun and *of*: *the arrival of the train*, *a full discussion of*, *her
  expectations of*. A noun already claimed by a light verb isn't counted again (*made the
  decision of a lifetime* is one match).

## Table

`style/data/hidden-verbs/nominalizations.tsv` has 316 `noun<TAB>verb` rows, 7,164 bytes (2,597
with `gzip -9`). It is `include_str!`'d into the style crate and parsed once on first use.

It is derived from **Princeton WordNet 3.0** under the WordNet License, so it is a separate data
file. It sits beside the other WordNet-derived table (`style/data/emotions/emotions.tsv`, #304) under
the same terms: its header carries the copyright notice, the full notice and disclaimer are in
`style/data/licenses/WordNet-LICENSE` (verbatim from the release, unchanged), and the NOTICE entry
now names both files and describes both derivations. It is not MIT/Apache. CatVar (OSL-1.1) is not
used. The derivation was vetted in
[STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#princeton-wordnet-30--31-304-307-hidden-verbs).

`scripts/build-hidden-verbs.py WordNet-3.0.tar.gz` rebuilds it byte for byte. It reads the tarball
in place, checks its sha256 (`640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52`)
and keeps a noun→verb pair when:

1. WordNet has a derivational pointer (`+`) from the noun to the verb, both single words;
2. the noun ends in *-tion, -sion, -ment, -ance, -ence, -al* or *-ure* and is the verb's stem plus
   that suffix (they share all but at most two of the verb's letters, and the verb is shorter), so
   back-formations such as *commentate* or *gesticulate* drop out;
3. the pointer starts from a synset in `noun.act`, `noun.cognition`, `noun.communication`,
   `noun.event`, `noun.process` or `noun.state`, and one of the noun's two most frequent senses is
   in those files, so concrete things and groups (*collection*, *government*) drop out;
4. the SemCor tag counts in `index.sense` are at least 1 for the noun and 5 for the verb, so rare
   variants (*botheration*, *surprisal*) drop out and the verb is a common word;
5. the noun is not in the emotion lexicon (feelings are the emotion-tells lens's), nor in a
   reviewed list of 86 nouns whose fiction sense isn't their verb's action: lexicalized senses
   (*information*, *reservation*), things and places (*signature*, *prescription*), fixed phrases
   (*in addition*, *pay attention*), feelings (*fascination*) and wrong links (*destination* →
   *destine*).

One verb per noun: the one with the highest tag count. The script fails if an excluded noun is no
longer selected, so the list can't go stale.

### Not duplicated

LanguageTool's `PLAIN_ENGLISH` category (off by default; presets can turn it on) has a few
nominalization rules: `MAKE_DECISIONS_ABOUT` (*make decision(s) about*, no determiner),
`MAKE_AN_ATTEMPT` (*make an attempt / effort / try*), `TAKE_ACTION_TO`, `HAVE_A_TENDENCY`,
`HAVE_THE_ABILITY_TO` and `ASSIST_ASSISTANCE`. Only the first overlaps the table, and the report
skips that exact shape. *attempt*, *effort*, *try*, *tendency* and *ability* aren't suffix
nominalizations and aren't in the table; *action* and *assistance* are excluded. Those rules match
fixed phrases and suggest a replacement; this report matches any of 316 nouns in two frames, counts
per mode and per term, and only names the related verb.

## Measurements (2026-10-21)

`python3 bench/style/vague_hidden_verbs.py` on bench/public:

| Text | Narration matches / words | per 1,000 | Dialogue matches / words | per 1,000 |
|---|---|---|---|---|
| story1–5 | 0 / 9,070 | 0.00 | 0 / 1,216 | 0.00 |
| moby150k | 8 / 24,602 | 0.33 | 0 / 400 | 0.00 |

The five contemporary CC0 stories have none: their narrators use plain verbs. In moby150k the eight
are *the consideration of* (twice), *a description of* (twice), *come to the conclusion*, *make a
full confession*, *an eruption of* (bears) and *giving utterance*. With `--books` (13
public-domain books and chapters plus bench/public), 386 matches in 581,826 narration words (0.66
per 1,000) and 117 in 289,952 dialogue words (0.40 per 1,000); the commonest verbs are *intend* 23
(*had no intention of*), *explain* 18, *object* 17, *arrive* 17, *describe* 15 and *move* 15.

**Precision, hand-judged** (is the span a nominalization doing its verb's work?): 54 of 60 on a
development sample from bench/public and the books (`--sample 60 --report hiddenVerbs`, seed 307)
before the last guards; the six misses (*has expectations*, *had only your statement*, *made his
isolation more complete*, *have employment*, *the easy assurance of* (confidence), *have the
official investigation hampered*) led to the causative and *have* guards. 36 of 40 on a held-out
sample afterwards (`--seed 3071`): the misses are other senses (*the searching illumination of*
Fifth Avenue, *a suggestion of deference*) and causative *make* with the adjective after an
*of*-phrase (*make her former assurance of …ruin still more certain*, *make assurance doubly sure*).
No recall measure.

## Size

Measured together with [vague words](STYLE-VAGUE-WORDS.md), against `main` at `2b90758`:

| | Bytes | `gzip -9` |
|---|---|---|
| `nominalizations.tsv` | 7,164 | 2,597 |
| `vague-words.tsv` + `fillers.txt` | 759 + 1,137 | 403 + 669 |
| `licenses/fillers.txt` (not compiled in) | 2,629 | |
| `writemorebetter_wasm_bg.wasm`, before → after | 7,053,229 → 7,174,085 (+120,856, +1.7%) | 1,771,694 → 1,792,470 (+20,776, +1.2%) |

Release build, wasm-bindgen 0.2.129, macOS, Rust's wasm32 target, no `wasm-opt`. Most of the WASM
growth is code (the two reports, their serde records and bindings), not data: the three tables are
9,060 bytes. The tables are parsed once, on first use, into a `HashMap` and a sorted entry list.
