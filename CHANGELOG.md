# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions after the
inherited 0.6.4 track this project's own release line. Each version is a top-level
`# <version>` heading; the Release workflow copies that section verbatim into the GitHub
release notes (see [`docs/RELEASING.md`](docs/RELEASING.md)).

# Unreleased

### Added

- **PyPI publishing.** A real release now also uploads the `writemorebetter` wheels and sdist to
  PyPI through Trusted Publishing; the Python workflow builds and tests the sdist too.
- **The `TOO_OFTEN_USED_*` minimum percentage can be set.** `Rules::set_too_often_used_percent`
  (Rust), `Checker.setTooOftenUsedPercent(percent:)` (Swift) and `checker.setTooOftenUsedPercent`
  (WASM) set the share, 1 to 100 as in LanguageTool's rule option, from which the three
  word-frequency rules flag a word; the default stays 5. Rust callers can set one rule through
  `TooOftenUsed::set_min_percent`. `Error` gains `OptionOutOfRange` and Swift's `CheckError` gains
  `InvalidOption`, so exhaustive matches on either need a new arm ([#235](https://github.com/cmlarsen/writemorebetter/issues/235)).
- **Python bindings.** The `python/` crate is rewritten as the `writemorebetter` package (PyO3,
  maturin, abi3 for CPython 3.9+), replacing upstream nlprule's unmaintained binding. It mirrors
  the WASM and Swift API: `Checker(tokenizer_path, rules_path, spell_variant=, profile=)` and
  `Checker.from_dir`, `check`, the user dictionary (`add_words`, `remove_words`, `set_words`,
  `clear_words`, `words`), `enable_rules`, `set_mother_tongue`, `apply_preset`, `analyze`,
  `analyze_document` and every style report through `style_document`. Offsets are Python string
  indices (code points). The wheel holds engine code only; the LGPL data still ships separately
  in the release data zip. A new Python workflow builds and smoke-tests wheels on Linux, macOS
  and Windows ([#93](https://github.com/cmlarsen/writemorebetter/issues/93)).
- **`reports::to_char_json`** in the style crate serializes a report with code-point spans, beside
  `to_utf16_json`.
- **Whole-text rules through paragraph digests.** Checking a paragraph can now also return a
  `ParagraphDigest`, a small serializable record of offsets and flags (no text) that the rules
  needing the whole text use. `Rules::check_paragraph(text, ends_paragraph, tokenizer)` returns a
  paragraph's suggestions and its digest (`ends_paragraph` is for editors that store paragraphs
  without their separators), `Rules::resolve_document` runs the whole-text rules over the digests
  of all paragraphs and returns `DocumentMatch`es tagged with their paragraph,
  `DocumentMatch::to_suggestion` turns one into a suggestion (or `None` when the paragraph text
  no longer fits the digest), and `merge_document_suggestions` combines them with a paragraph's
  suggestions. Digests carry a version (`DIGEST_VERSION`); `ParagraphDigest::to_bytes` /
  `from_bytes` store them and reject other versions. `Rules::suggest` uses the same digests and
  resolver internally and builds none while no whole-text rule is enabled
  ([#329](https://github.com/cmlarsen/writemorebetter/issues/329)).
- **`EN_CONSISTENT_APOS`**, LanguageTool's check for a text that mixes typewriter (`'`) and
  typographic (`’`) apostrophes, is ported as the first whole-text builtin. Once a text has both,
  every apostrophe token is flagged with the other style as the suggestion, also where another
  rule flags the same word, as in LanguageTool. It is off by default,
  as in LanguageTool 6.6; enable it with `BUILTIN/EN_CONSISTENT_APOS`
  ([#228](https://github.com/cmlarsen/writemorebetter/issues/228)).

- **Incremental document checking in Swift, WASM and Python.** `checkParagraph` (Swift
  `checker.checkParagraph(text:endsParagraph:)`, WASM `checker.checkParagraph(text,
  endsParagraph)`, Python `checker.check_paragraph(text, ends_paragraph=True)`) returns a
  paragraph's suggestions and its digest bytes (`Data`, `Uint8Array`, `bytes`) to store with the
  paragraph; `resolveDocument` / `resolve_document` takes the digests of every paragraph and
  returns a `DocumentResolution` whose `suggestions(paragraph, text, checked)` merges that
  paragraph's whole-document matches into its own suggestions, and whose `stale` lists digests to
  recompute. Offsets are in each binding's units (UTF-16 in Swift, chars and UTF-16 in WASM, code
  points in Python), relative to the paragraph, and equal those of `check` on the joined text for
  the whole-document rules. Python also gains `set_too_often_used_percent`
  ([#234](https://github.com/cmlarsen/writemorebetter/issues/234),
  [#329](https://github.com/cmlarsen/writemorebetter/issues/329)).
- **`DocumentMatches::suggestions(paragraph, text, checked)`** merges a paragraph's whole-text
  matches into the suggestions `check_paragraph` returned for it, `DocumentMatches::in_paragraph`
  lists one paragraph's matches, and `Rules::resolve_document_bytes` resolves stored digest bytes,
  listing the ones it cannot read (another `DIGEST_VERSION`, or not a digest) as stale.
  `Suggestion::new` is public so bindings can rebuild suggestions from their own records
  ([#234](https://github.com/cmlarsen/writemorebetter/issues/234)).

### Changed

- **The word-statistics builtins count the whole document.** `EN_WORD_COHERENCY`,
  `EN_REPEATEDWORDS` and `TOO_OFTEN_USED_*` are now whole-text rules: their counts come from
  the paragraph digests, so `check_paragraph` + `resolve_document` give the same matches as
  `Rules::suggest` on the joined text. `EN_WORD_COHERENCY` (on by default) now remembers a
  spelling across paragraphs, as LanguageTool 6.6 does ("adviser", a blank line, then "advisor"
  is flagged); it used to forget at each paragraph. `EN_REPEATEDWORDS` looks back 150 words across
  paragraphs, and `TOO_OFTEN_USED_*` count every paragraph (direct speech left open at a
  paragraph's end goes on into the next, as in LanguageTool). The digests store the lemmas as
  strings, so they stay valid when the tagger data's word ids change: on `moby150k`, checked
  paragraph by paragraph, the digests take 3,512 bytes per 1,000 words (784 before), and
  resolving all of them from bytes takes about 5 ms. `DIGEST_VERSION` is 2, and
  `ParagraphDigest::to_bytes` writes variable-length integers and relative offsets; digests of
  version 1 are stale. A pure function of the paragraph's text, a digest does not depend on
  which rules are on or their options (the minimum percentage, `DIRECT_SPEECH`)
  ([#234](https://github.com/cmlarsen/writemorebetter/issues/234)).
- **Overlapping matches of the word-statistics rules are kept.** A `TOO_OFTEN_USED_*` match that
  overlaps a match of another rule group is no longer dropped, and an `EN_REPEATEDWORDS` or
  `EN_WORD_COHERENCY` match no longer gives way to (or hides) one, as LanguageTool 6.6 reports
  both (`FREE_GIFT` and `TOO_OFTEN_USED_NOUN_EN` on "free gift", `ENGLISH_WORD_REPEAT_RULE` and
  `EN_REPEATEDWORDS_EXCELLENT` on "excellent excellent"). This is the overlap policy of
  [#178](https://github.com/cmlarsen/writemorebetter/issues/178) and
  [#216](https://github.com/cmlarsen/writemorebetter/issues/216) for these rules;
  `apply_suggestions` skips a fix that overlaps one it already applied.
- **`Rules::apply` and `Builtin::check_text` run the whole-text rules** on their sentences as
  the whole text, so `EN_CONSISTENT_APOS` now also flags in `apply`, and `check_text` of a
  whole-text rule returns its matches; `Builtin::check_paragraph` still returns none for them.
- **`RULES_FORMAT_VERSION` is 37.** Version 33 added `SpellLangOptions::latin_script`, 34 the
  `ConsistentApostrophes` builtin, 35 stores the multitoken phrase list once instead of twice
  ([#212](https://github.com/cmlarsen/writemorebetter/issues/212)), 36 adds the speller
  dictionaries' replacement pairs (#194), and 37 the curated spelling suggestions, so 0.12.0's
  `en_rules.bin` no longer loads; use the next release's data zip. The tokenizer format stays 18.
- **Common misspellings get LanguageTool's hand-written suggestions first.** "wierd" now gets
  weird, "freind" friend and "cemetary" cemetery as the only suggestion; "tonite" gets tonight,
  "alot" a lot and "ur" your before the speller's candidates; a word ending in "ys" gets its
  "ies" form first when that is a word ("babys" gets babies). The lists come from LanguageTool's
  `AbstractEnglishSpellerRule` and `MorfologikAmericanSpellerRule` (675 words, 77 patterns, 4
  American-only words, with LanguageTool's later additions such as "forseeable"), extracted by
  `build/extract_speller_suggestions.py` into a builtin data file (`speller_suggestions.txt`,
  LGPL) compiled into `en_rules.bin`; the engine keeps only a generic table. The patterns are
  case-sensitive as in LanguageTool, so "WIERD" still gets the speller's candidates. On 728
  test words LanguageTool 6.6 flags, top-1 agreement goes from 199 to 708 and top-5 from 126
  to 684.
- **Suggestions for a sentence's first word are capitalized.** As in LanguageTool, the
  lowercase suggestions for the first word of a sentence (after any opening punctuation such
  as a quote or a dash) start with a capital ("“wierd thing…" gets Weird, "alot of"
  gets A lot, Lot), unless the word is the sentence's last token.
- **The `fiction` profile turns on `EN_CONSISTENT_APOS`**, so a manuscript that mixes straight
  and curly apostrophes is flagged by default in the Swift, WASM and Python checkers. In this
  profile the rule flags only the less common style and suggests the more common one (on a tie,
  the straight ones), so one stray `'` in a curly manuscript is one flag rather than one per
  apostrophe. It is the rule's new `MAJORITY` case (`BUILTIN/EN_CONSISTENT_APOS/MAJORITY`, which
  the profile disables; `ConsistentApostrophes::flag_minority_only` in Rust); elsewhere the rule
  still flags both styles as in LanguageTool. The counts cover the whole document, so
  `Rules::suggest` and `check_paragraph` + `resolve_document` agree. The option is set at
  runtime and not stored in `en_rules.bin`, whose format does not change. Curly single
  quotation marks are not apostrophes to the rule, so curly-quoted dialogue does not trigger
  it; straight single quotation marks in a curly-apostrophe text count, as in LanguageTool. Profiles
  can now enable rules: `Profile` gains a public `enable` list, applied after `disable`, whose
  unmatched selectors `Rules::apply_profile` reports too. Code that builds a `Profile` with a
  struct literal needs the new field; `Profile::new` is unchanged.
- **`Builtin::check` and `Builtin::check_text` take the speller.** Both take an
  `Option<&Speller>` as a new last argument, as `Builtin::check_paragraph` does. Pass
  `rules.spell().speller().as_ref()` to get the `EN_COMPOUNDS` suggestions `Rules::suggest`
  reports: before, they always ran without a speller and kept suggestions LanguageTool and
  `Rules::suggest` drop, such as `Yojong` for `Kim Yo jong`
  ([#233](https://github.com/cmlarsen/writemorebetter/issues/233)).
- **The Release workflow checks `python/Cargo.toml`'s version** with the other crates, so a
  wheel's version always names the data release it pairs with.
- **The chunker is faster.** Model lookups are hoisted out of the beam search and the pmap gets a
  bucket index built at load, recovering the cost of OpenNLP beam parity: about 44% fewer
  instructions per chunker pass and 2.5% per whole check on moby150k, with byte-identical output
  and data binaries ([#174](https://github.com/cmlarsen/writemorebetter/issues/174)).
- **About 12 MiB less memory once the English binaries are loaded.** The multitoken phrase list
  keeps each spelling and key once (5.6 MiB to 1.7 MiB live heap), and the rules, part pool,
  speller, tagger FSTs and multiword phrase list drop the spare capacity they carried after
  deserialization. Loaded live heap goes from 62.9 MiB to 51.0 MiB, and loaded RSS drops by
  about 4 to 5 MiB ([#212](https://github.com/cmlarsen/writemorebetter/issues/212),
  [#225](https://github.com/cmlarsen/writemorebetter/issues/225)).
- **`Tokenizer::pipe` no longer keeps the raw tokens.** Its sentences' `Sentence::raw()` is now
  `None`, which saves a copy of every sentence's tokens for callers that never check rules
  (Python's `Tokenizer.pipe`, `dump_sentences`, `dump_tags`, `wmb-chunkbench`). To pass piped
  sentences to `Rules::apply` with `raw_pos` rules matching as `Rules::suggest` matches them, use
  the now public `Tokenizer::pipe_with_raw(text, rules.needs_raw())`; `Rules::needs_raw` is new
  ([#169](https://github.com/cmlarsen/writemorebetter/issues/169)).
- **Sentence spans keep whitespace with the sentence before it.** `pipe()`, `sentencize()` and so
  `analyze()` and the Swift, WASM and Python sentence spans now give `"A.  \n\n" | "B."` where
  they gave `"A. " | " \n\nB."`, and trailing whitespace at the end of the text belongs to the
  last sentence instead of being dropped
  ([#211](https://github.com/cmlarsen/writemorebetter/issues/211)).

### Fixed

- **The number check names centuries.** An ordinal before *century* (`19th century`,
  `19th-century ruins`) is still flagged, since Chicago spells centuries out, but the message now
  says so ("Spell out centuries in prose, as in “the nineteenth century”.") instead of naming the
  zero-through-100 limit, which read as wrong on a number like 19th.
- **`apply_suggestions` (and so `Rules::correct`) no longer panics on overlapping suggestions.**
  It shifted each span by the length change of the replacements before it, which underflowed
  once two suggestions overlapped, as `EN_CONSISTENT_APOS` and another rule do on the same word.
  It now applies suggestions in order and skips one that overlaps or precedes an applied one or
  reaches past the end of the text. The stress harness's input is in the fuzz crash corpus.
- **The multitoken run-on check uses the en_US speller in every variant, as in LanguageTool
  6.6.** It used the selected variant's speller, so with en_GB "Lincol nCenter" was flagged as a
  misspelled phrase and "Roger sCentre" (→ "Rogers Centre") was not. Both now match
  LanguageTool 6.6 for en-GB, en-US, en-AU and en-CA
  ([#215](https://github.com/cmlarsen/writemorebetter/issues/215)).
- **The earliest subrule of a rule group wins a span, as in LanguageTool 6.6.** When two subrules
  of one group matched at the same start, the later one was reported: "The break- up was
  painful." got `EN_SPLIT_WORDS_HYPHEN/3` ("breakup") instead of subrule 0 ("break-up",
  "break - up", "breakup"), and "It it" got `IT_IT/5` instead of `IT_IT/3` ("If it"). Rules of
  different groups keep the old order ([#190](https://github.com/cmlarsen/writemorebetter/issues/190)).
- **`SENTENCE_WHITESPACE` no longer flags the sentence after a U+2029.** A paragraph or line
  separator (U+2028, U+2029, NEL) now counts as the space between sentences, following
  LanguageTool's fix after 6.6 (commit `86c5f3a9`) in every profile
  ([#193](https://github.com/cmlarsen/writemorebetter/issues/193)).
- **Words with no Latin letter are no longer misspelled.** A Greek, Cyrillic or CJK word quoted in
  English prose ("Привет", "ΟΔΟΣ", "東京") was flagged by the speller. As in LanguageTool, a
  Latin-script language (the new per-language `latin_script` option, on for en, de and es) now
  accepts a token with no `Script=Latin` character. The test applies to the whole token, so a
  mixed-script token ("Τhe" with a Greek tau, "Привет-friend") is still flagged (#226).
- **Mixed-case words get capitalized, deduplicated suggestions.** A query starting with a capital
  that is neither all caps nor title case ("LeBRON", "XerXes") got lowercase candidates and the
  same word twice ("xeroxes", "Xeroxes"). Candidates are now capitalized as in LanguageTool 6.6:
  "LeBRON" gets LeBron, Hebron, Lebrun, Learn, Lesson (#196).
- **Suggestions are capitalized like LanguageTool.** A suggestion's first letter is uppercased only
  when the matched text starts uppercase, not merely because the match is at sentence start
  (`a compete` → `a competed`), and a rule whose suggestions convert case keeps the converted case.
  The first letter or digit is uppercased, past leading punctuation: `(The) latest`. Rule examples
  in `en_rules.bin` now keep their `correction` exactly as written, so the conformance harness
  needs this release's rebuilt data; this change alone does not alter the binary format
  ([#158](https://github.com/cmlarsen/writemorebetter/issues/158),
  [#191](https://github.com/cmlarsen/writemorebetter/issues/191)).
- **Accepting an `EN_COMPOUNDS` fix keeps paragraph indentation.** A compound at the start of
  a sentence that begins with whitespace (`He left.\n  Part time work.`) was flagged from the
  whitespace on, while the suggestion (`Part-time`) has none, so accepting it deleted the
  indentation. The match now starts at the first word, so `Rules::suggest` also stops
  dropping it where it overlapped a `WHITESPACE_RULE` flag. LanguageTool 6.6 includes the
  whitespace, so the offsets differ from it for such sentences only
  ([#232](https://github.com/cmlarsen/writemorebetter/issues/232)).
- **Quoted names ending in "in" are no longer read as dropped-g verbs.** `His name was 'Corin'.`
  tagged `Corin'` as *coring* and `'Rin'` as *ring*, so the name never reached the speller. The
  g-dropped fallback now needs a vowel before the `in`, and a straight quote before a g-dropped
  word is paired with a later closer when there is one, as LanguageTool 6.6 tokenizes it. As a
  result `She said 'goin'.` now splits like LanguageTool (`'` `goin` `'`). Curly-quoted names
  and multi-word quotations ending in a name still behave like LanguageTool 6.6
  (part of [#203](https://github.com/cmlarsen/writemorebetter/issues/203)).
- **`EN_UNPAIRED_QUOTES` reads clipped words as apostrophes.** In `‘Will o’ the wisp,’ she said.`
  or `‘He was jus’ there—tho’ he left…’` the mark of `o’`, `jus’` or `tho’` closed the opening
  quote, so the real closing quote was flagged. As in LanguageTool 6.6, a mark the disambiguator
  tags `_apostrophe_contraction_`, `POS` or `NNP` now counts as an apostrophe. Plural possessives
  inside single quotes are unchanged and still match 6.6
  (part of [#187](https://github.com/cmlarsen/writemorebetter/issues/187)).
- **Disambiguation keeps the readings LanguageTool 6.6 keeps.** A `filter` whose tag matched none
  of a token's readings emptied it, so both "as" tokens in "as soon as possible" lost their
  multiword `RB` reading; such a filter now leaves the token alone. A `unify` whose only
  surviving reading on the sentence-final token is SENT_END now drops that token's other
  readings ("!" in "He saw the city!" loses `.` and `PCT`) instead of restoring them. Tagger
  output on bench/public now matches LanguageTool 6.6 `--taggeronly` on every aligned token's
  reading set ([#153](https://github.com/cmlarsen/writemorebetter/issues/153)).
- **SENT_END sits where LanguageTool 6.6 puts it among a final token's readings.** It came last;
  it now follows the tagger's readings, comes first on an untagged final token, and goes after
  the readings a disambiguation rule rebuilds. A `scope="next"` exception that checks the first
  reading now sees it, so `TYPOS/A3FT` flags "It was a3ft!" as LanguageTool does
  ([#152](https://github.com/cmlarsen/writemorebetter/issues/152)).
- **Inflected tokens match lemmas only.** `inflected="yes"` also matched the surface form through
  the empty and SENT_END readings, so false friends such as pt RETIRED, VEGETABLES and
  SPECTACLES, fr GRAPES, and grammar rules such as APART_A_PART fired where LanguageTool 6.6
  doesn't. The SENT_END reading now takes the first reading's lemma, as in LanguageTool
  ([#157](https://github.com/cmlarsen/writemorebetter/issues/157)).
- **`<disambig>` with `<wd>` children compiles as in LanguageTool 6.6, or is rejected.** The
  default action and `action="replace"` replace the token's readings with the `<wd>` reading
  (lemma included), `action="remove"` matches the `<wd>`, and shapes LanguageTool would apply
  differently fail to compile with `Unimplemented` instead of silently filtering. No English
  rule uses these shapes, so the English binaries are unchanged
  ([#230](https://github.com/cmlarsen/writemorebetter/issues/230)).
- **Antipatterns apply per sentence, as in LanguageTool 6.6.** A rule's antipatterns now immunize
  tokens only once the rule matches somewhere in the sentence, and then for a rerun over the whole
  sentence. Before, the decision was made at each start position, so "It soon then was eaten and
  it was gone." lost a match at "It soon then was" that LanguageTool reports
  ([#223](https://github.com/cmlarsen/writemorebetter/issues/223)). An antipattern whose
  `<marker>` covers only an absent optional token now immunizes its whole match.
- **A disambiguation antipattern with a `<marker>` blocks only with its marked tokens**, or with
  its whole match when they are absent, as in LanguageTool 6.6
  ([#217](https://github.com/cmlarsen/writemorebetter/issues/217), first half).
- **`<token min="N">` with N > 1 and a `max` other than 1 is rejected at compile time** instead of
  matching N..M tokens where LanguageTool can match up to N·M. No English rule uses it
  ([#224](https://github.com/cmlarsen/writemorebetter/issues/224)).
- **Spelling suggestions apply the dictionary's replacement pairs.** The speller now reads
  `fsa.dict.speller.replacement-pairs` from each variant's `.info` into the data binary and, as
  Morfologik does, treats each pair (`ei`/`a`, `ey`/`a`, `ph`/`f`, `kw`/`qu`, `uff`/`ough`, …) as a
  free edit: "Feyre" gets Fare, Care, Face; "fone" gets phone; "enuff" gets enough. Top-1
  agreement with LanguageTool 6.6 on a 308-word probe rose from 80.8% to 86.7% (#194).
- **Suggestions may change the first two letters twice.** `SpellOptions::prefix_length` now
  defaults to 0, no limit, as in LanguageTool: "Tyerman" gets German, "Davenant" gets Covenant,
  "IPHONE" gets PRONE. The distance-2 search now runs only when distance 1 finds nothing, which
  pays for most of the wider search (#195).
- **Suggestions ignore accents as LanguageTool does.** The English dictionaries set
  `fsa.dict.speller.ignore-diacritics`, now read into the data binary: a letter matches the same
  letter with any accent at no cost ("Citroen" gets Citroën, "Nescafee" Nescafé, "Zoee" Zoé and
  Zoë). Ties now keep dictionary order with the word's own case forms first, as in Morfologik
  ("renée" gets Renée, then Renee). Probe top-1 agreement 88.0% → 91.9% (#219).
- **Long words with nothing within two edits get distance-3 suggestions.** As in LanguageTool,
  a word of five or more letters with no candidate at distance 2 is searched again at distance
  3: "Queequeg" gets Queue, "Thornfeild" Shortfall, "Havisham" Lavish, "Yossarain" Constrain.
  `SpellOptions::max_distance` now defaults to 3; set it to 2 for the old behaviour. Suggestions
  are now cached per word (emptied when the options or the user dictionary change), so a
  repeated name costs one search. With the defaults of #194, #195 and #182 together, the first
  search for an invented name takes about 15 ms instead of about 3 ms; `prefix_length` 2 and
  `max_distance` 2 bring it to about 5 ms (#182).
- **Run-together words get split suggestions.** As in LanguageTool, a misspelling that is two
  dictionary words run together gets the split after the other candidates ("Aduke" gets Duke,
  A duke; "Rhysand" gets Rhys and; "oilpainting" gets oil painting). Splits into a prefix,
  suffix or stray letter ("re view", "use full") are dropped using LanguageTool's English list,
  now a builtin data file (`speller_split_filter.txt`, LGPL) compiled into `en_rules.bin`.
  Probe top-1 agreement 94.2% → 99.4% (#183).
- **`NUMBERS_IN_WORDS` flags invented names again.** It drops a match when the speller has no
  suggestions for the word without its digits; with the distance-3 retry, "Mirr0wen" now gets
  LanguageTool 6.6's Married, Warren, Mirror, … (#199).
- **Parallel disambiguation of very long sentences is no longer several times slower than
  serial.** The parallel search for the next disambiguation rule that applies evaluated rules
  past it and threw their results away, and on a long sentence the next rule is usually only a
  few rules further. After each change, the next 16 rules are now tried serially before the rest
  are searched in parallel. On LanguageTool’s rule examples glued into one 108,000-token
  sentence, parallel disambiguation uses 9.9 s of CPU instead of 30.4 s, about the same as
  serial. The output is unchanged ([#149](https://github.com/cmlarsen/writemorebetter/issues/149)).
- **Whitespace-only sentence splits are checked as LanguageTool checks them.** The blank line
  after a sentence ending in two or more spaces, and whitespace at the start or end of the text,
  are now sentences of their own for `Rules::suggest`, as in LanguageTool 6.6's splitter. So
  `TOO_LONG_PARAGRAPH` and `ENGLISH_WORD_REPEAT_BEGINNING_RULE` see the right paragraphs
  ([#211](https://github.com/cmlarsen/writemorebetter/issues/211)), `WHITESPACE_RULE` flags
  trailing spaces and `EMPTY_LINE` leading blank lines
  ([#184](https://github.com/cmlarsen/writemorebetter/issues/184)), and repetition distances
  are no longer off by one after a blank line
  ([#210](https://github.com/cmlarsen/writemorebetter/issues/210)).

### Removed

- **Upstream's `scripts/maturin.sh`** and the `BUILD_BINDINGS_*` markers it rewrote in
  `Cargo.toml`, `nlprule/Cargo.toml` and `build/Cargo.toml`. It renamed `nlprule` to
  `nlprule-core` and would break the new binding's path dependency; build wheels with
  `maturin build` in `python/` instead.

# 0.12.0 (2026-10-22)

### Added

- **Rules for errors LanguageTool misses or needs n-grams for.** The forked `grammar.xml` adds
  `PEAKED_THROUGH` (peaked → peeked), `HEAL_OF` and `HEAL_POCKET` (heal → heel),
  `EACH_OF_NNS_VBP` ("…each of the dogs are" → "is" mid-sentence), `THERE_ARE_WHAT` ("There are
  what I thought was" → "is"), and new sub-rules of `YOUR_YOU_RE` ("Your welcome here anytime",
  "your welcome to stay") and `HE_THE` ("Popping he SD card" → "the"). Recall is now 50 / 50 on
  the private corpus and 60 / 60 on the public one; the CI floors are raised to match ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).

### Changed

- **The data binaries are rebuilt from forked LanguageTool files.** `en_rules.bin` is compiled from
  `build/lt-rules/en/grammar.xml` and `en_tokenizer.bin` from `build/lt-rules/core/segment.srx`,
  each a fork of LanguageTool 6.6's file with its edits listed in the README beside it. The format
  versions stay 32 (rules) and 18 (tokenizer), so 0.11.0's binaries still load, but they lack the
  fixes below; use this release's data zip ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).
- **Double spaces get one message under the fiction profile.** The profile disables
  `TYPOGRAPHY/CONSECUTIVE_SPACES`, LanguageTool's rule for two spaces between words, so the
  `WHITESPACE_RULE` builtin reports every repeated space. Its message is now "Possible typo:
  repeated spaces" (was "Possible typo: you repeated a whitespace"), in every profile. The default
  profile still runs `CONSECUTIVE_SPACES` ([#327](https://github.com/cmlarsen/writemorebetter/pull/327)).

### Fixed

- **"One way or other" and "something or other" are no longer flagged.** `OTHER_OTHERS`
  suggested "others" when the idiom "X or other" came before a verb ("one way or other
  served"). The English `grammar.xml` is now a fork of LanguageTool 6.6's, committed at
  `build/lt-rules/en/grammar.xml` with its local edits listed in the README next to it;
  `scripts/build-data.sh` builds from it, so the data binaries need rebuilding ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).
- **Spaced ellipses (". . .") are no longer flagged.** Each run produced a "space before the full
  stop" flag from `COMMA_PARENTHESIS_WHITESPACE`, and the sentence split after it flagged the next
  lowercase word under `UPPERCASE_SENTENCE_START` (about 15,000 flags on 7.5M words of published
  fiction). `segment.srx` is now a fork of LanguageTool 6.6's (`build/lt-rules/core/`) with the two
  no-break rules LanguageTool added after 6.6, and the builtin skips full stops in a run of three or
  more separated by whitespace, as LanguageTool's `IMMUNIZE_ELLIPSIS` does. The tokenizer binary
  needs rebuilding ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).
- **Spacing that publishers use around ellipses and nested quotes is no longer flagged.** A space
  between a spaced ellipsis and a quote mark or comma (`. . . ”`, `. . . ,`) and a space between two
  adjacent quote marks (`‘trouble’ ”`, `' "`) were reported as stray whitespace; LanguageTool 6.9
  reports them too. A space before a quote at a line end is still flagged ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).
- **Words with digits are no longer spell checked**, matching LanguageTool's Morfologik default
  (`1860s`, `75mm`, `3:21pm`, `21st`) ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).
- **The fiction profile disables `COMMA_COMPOUND_SENTENCE_2`** as well as
  `COMMA_COMPOUND_SENTENCE`; it fired for 61 of 70 authors in published fiction ([#278](https://github.com/cmlarsen/writemorebetter/pull/278)).

# 0.11.0 (2026-10-21)

### Added

- **Style presets** ([#305](https://github.com/cmlarsen/writemorebetter/issues/305)). A preset is one
  named, versioned JSON file that drives the grammar checker's profile, rule selectors and speller
  variant, the consistency report's preferred forms, Vale-format house-style rules and new built-in
  preset checks. `chicago-fiction` (the default), `ap` and `uk` are built in; a user preset can
  `extends` one and override any part as a JSON merge patch (`null` removes an inherited setting).
  Unknown keys and values are `invalidPreset` errors, and a user preset must set its own `name`
  (`name`, `version` and `description` are never inherited). The preset checks flag numerals that prose
  spells out (zero to one hundred, or one to nine for AP, with exceptions for dates, times,
  addresses, years, percentages, decimals and labels), periods and commas outside closing quotes (or,
  for UK, inside a quoted non-speech phrase), *James’* versus *James’s*, *AM*/*a.m.* after a time,
  and *my Mom* versus *Mom* as a name. Rust: `StylePreset::{builtin, parse, load}`,
  `apply_to_rules`, `report`, `StyleDocument::preset` and `preset_checks`. WASM: `stylePresetNames`,
  `loadStylePreset`, `parseStylePreset`, `checker.applyPreset`, `document.preset` and
  `document.presetChecks`, with TypeScript types. Swift: `StylePresets`, `checker.applyPreset(preset:)`,
  `document.preset(_:)` and `document.presetChecks(optionsJson:)`. The consistency report gains a
  `preferred` option and `preferred`/`nonPreferred` on each check, and `nlprule` gains
  `Rules::unmatched_selectors`. Recall is unchanged for every preset (46/50 private, 55/60 public).
  See [`docs/STYLE-PRESETS.md`](docs/STYLE-PRESETS.md).
- **House-style rules.** `StyleDocument.houseStyle` (Rust `house_style`, Swift
  `houseStyle(rules:)`) runs caller-supplied rules in a subset of [Vale](https://vale.sh)'s YAML
  rule format: `existence`, `substitution`, `consistency` and `occurrence`, with `ignorecase`,
  `nonword`, `scope`, `exceptions`, `limit` and `link`
  ([#305](https://github.com/cmlarsen/writemorebetter/issues/305)). Rules come as YAML or JSON
  text: one rule per YAML document, a list of rules, or `{rules: …}`, so an existing Vale style can be
  imported. Each match carries its span, rule name, level, message and narration or dialogue mode,
  and the `skipDialogue` option leaves quoted speech out. Curly apostrophes, em dashes, ellipses and
  U+2028 are handled, and invalid rules return an `invalidRules` error that names the rule. A rule
  that needs regex features this engine lacks (lookaround, backreferences) is listed in the
  report's `unsupported` and skipped, while the rest of the bundle runs. Style
  presets and the Chicago rules from #305 will follow as rule bundles. See
  [`docs/STYLE-HOUSE-STYLE.md`](docs/STYLE-HOUSE-STYLE.md).
- **Title case for headings** ([#307](https://github.com/cmlarsen/writemorebetter/issues/307),
  part). Rust `title_case::report(&headings, &options)` and `StylePreset::title_case`, WASM
  `titleCase(headingsJSON, optionsJSON)` and Swift `TitleCase.check(_:optionsJson:)` check chapter
  titles against Chicago-style or AP-style headline case or sentence case, with each word's rule
  and its recased form; hyphenated compounds, words after colons and words that are prepositions
  only sometimes are handled. Presets gain a `titleCase` key (`chicago-fiction`: Chicago, `ap`: AP,
  `uk`: sentence case), and the built-in presets are now version 1.1.0. See
  [`docs/STYLE-TITLE-CASE.md`](docs/STYLE-TITLE-CASE.md).
- **Enable off-by-default rules.** `Rules::enable_rules(&selectors)` turns on LanguageTool's
  `default="off"` and `picky` rules, such as `PLAIN_ENGLISH`, `REDUNDANCY`, `STYLE/PASSIVE_VOICE` and
  `STYLE/SERIAL_COMMA_ON`, by the selector syntax profiles use (`CATEGORY`, `CATEGORY/RULE_ID`,
  `BUILTIN/ID`), applied after the profile; unknown selectors are an error and enable nothing.
  Swift gets `Checker.enableRules(selectors:)` and WASM `checker.enableRules([...])`. With no
  selectors the rule set is unchanged. See [`docs/PROFILE.md`](docs/PROFILE.md#enabling-rules)
  ([#297](https://github.com/cmlarsen/writemorebetter/issues/297)).
- **Four confusion rules adapted from Harper**
  ([#306](https://github.com/cmlarsen/writemorebetter/issues/306)). New default-on English builtins
  `HARPER_WAIST_WASTE`, `HARPER_BELLOW_BELOW`, `HARPER_FEEL_FELL` and `HARPER_QUITE_QUIET`, adapted
  from Harper's Apache-2.0 lints (commit `37d344cd`, credited in NOTICE), flag *a waist of time*,
  *right bellow the deck*, *to fell free* and *very quite*. Each pattern is narrower than Harper's so
  ordinary narration passes (*As night fell*, *had to fell it*, *won’t you quiet down*), and a line
  break ends any match. Harper's then/than, though/thought, were/where and thing/think lints are not
  ported because LanguageTool 6.6 already covers them. Grammar conformance and public recall are
  unchanged ([#311](https://github.com/cmlarsen/writemorebetter/pull/311)).
- **Diction has a bundled concreteness table**
  ([#256](https://github.com/cmlarsen/writemorebetter/issues/256)).
  `style/data/norms/concreteness.tsv` is writemorebetter's own table (14,572 words, MIT OR
  Apache-2.0): AI-generated ratings from Claude Opus 5.5, Sonnet 5.5 and Fable 5.1 over the fiction
  lemma list, calibrated to a 1–5 scale on a 30% split of the Glasgow Norms. No Glasgow or
  Brysbaert ratings are in it. On the held-out 70% of Glasgow it scores Spearman 0.926, Pearson
  0.925, abstract F1 0.755 and mean absolute error 0.289, and it rates 94.6% of the narration words
  diction looks up on `bench/public` (97.8–98.4% on each modern story). The ratings, prompt, build
  script and validation gate are committed under `bench/style/concreteness/`, and the build is
  byte-reproducible from them. See [`docs/STYLE-NORMS.md`](docs/STYLE-NORMS.md#bundled-concreteness-table-256)
  and NOTICE.
- **Readability report** ([#300](https://github.com/cmlarsen/writemorebetter/issues/300), phase 1).
  An opt-in `style` lens that reports Flesch Reading Ease, Flesch–Kincaid grade, Coleman–Liau, the
  Automated Readability Index and estimated reading time for the whole text, for narration and
  dialogue separately, and for each paragraph. Dialogue-only paragraphs can be excluded. It returns
  statistics, never flags, so it doesn't duplicate LanguageTool's readability rules (#207).
  Syllables come from an original heuristic counter (96.5% on a 484-word development list of hard
  cases, 99.5% on a 190-word held-out sample, 99.2% of `bench/public` tokens) that a dictionary-backed counter can replace later. Available as
  `StyleDocument::readability` in Rust, `document.readability()` in WASM and Swift. See
  [`docs/STYLE-READABILITY.md`](docs/STYLE-READABILITY.md).
- **Readability counts syllables with CMUdict**
  ([#300](https://github.com/cmlarsen/writemorebetter/issues/300), phase 2). The readability
  report's default counter, `DictionarySyllables`, looks words up in a table derived from the CMU
  Pronouncing Dictionary (commit `74790861`, BSD-2-Clause-style) and falls back to the phase 1
  heuristic, which stays available as `HeuristicSyllables`. The table holds only the 8,437
  CMUdict words the heuristic gets wrong (92,749 bytes; +34.9 KB to the gzipped WASM module), so
  every CMUdict headword gets CMUdict's first-pronunciation count; no phonemes are bundled.
  Accuracy on the development word list rises from 467 to 482 of 484 and on the held-out list
  from 189 to 190 of 190; whole-text indices on `bench/public` move by at most 0.5. Built by
  `scripts/build-cmudict.py` from a pinned sha256. Apps that ship the style reports must
  reproduce CMU's notice in their documentation; see NOTICE and
  [`docs/STYLE-READABILITY.md`](docs/STYLE-READABILITY.md#dictionary-phase-2). The XCFramework
  and data release zips now carry `NOTICE` and the third-party license texts, and `NOTICE` is
  a release asset. CI regenerates the table from the pinned `cmudict.dict` and fails if it is
  stale relative to the heuristic.
- **Consistency report.** A new opt-in style lens, `consistency`, counts and locates mixed usage
  within one document: curly and straight double and single quotes, apostrophes (counted apart
  from quotes, with elisions such as `’em` and `rock ’n’ roll`), `…`/`...`/`. . .` ellipses,
  closed and spaced em dashes, spaced en dashes, `--` and spaced hyphens, the same word
  capitalized and lowercase mid-sentence, and the same compound written hyphenated, open and
  closed ([#298](https://github.com/cmlarsen/writemorebetter/issues/298)). Each check returns
  per-form counts split by narration and dialogue, spans, and whether the document is mixed; it
  never says which form is right. Rust `StyleDocument::consistency`, WASM
  `document.consistency()` and Swift `document.consistency()` (typed `ConsistencyReport`). It
  needs no data and runs on compact documents. See
  [`docs/STYLE-CONSISTENCY.md`](docs/STYLE-CONSISTENCY.md).
- **Passive-voice rate** ([#303](https://github.com/cmlarsen/writemorebetter/issues/303)): passive
  clauses (*be*/*get* + past participle, with guards for adjectival participles such as *was
  tired* and *was married*) per 100 sentences, with narration and dialogue reported separately.
  Each passive has a span, and the rate is compared with a p5–p95 band through a replaceable
  baseline argument. Available as `StyleDocument::passive` in Rust, `passive()` in WASM
  (`PassiveReport`) and `passive(optionsJson:baseline:)` in Swift. There is **no default band**
  yet (no comparison) because the published-fiction export was not available. A **provisional
  public stand-in** band built from `bench/public` and the pinned *Pride and Prejudice* is opt-in
  with the `"publicStandIn"` baseline (Swift `.passivePublicStandIn`). The flag-rate probe and
  `published_bands.py --passive` can build the real band from that export. See
  [`docs/STYLE-PASSIVE.md`](docs/STYLE-PASSIVE.md).
- **Clarity lens** (opt-in): `clarity` flags sticky sentences, where glue words (determiners,
  prepositions, conjunctions, pronouns, *to*, modals, forms of *be*/*have*/*do*) are at least 65% of a
  sentence of 15 or more words, and long subordinate clauses of 20 or more words, opened by a
  subordinating conjunction or relative/WH-word and bounded by punctuation, mode changes and
  unpunctuated (including coordinated) main clauses ([#302](https://github.com/cmlarsen/writemorebetter/issues/302)). It
  uses the existing tagger readings and chunker output, needs no new data, and measures neither
  sentence length nor `TOO_LONG_SENTENCE`. On published public-domain text the defaults flag 2.0% of
  sentences as sticky and 0.9% for long clauses. Rust `StyleDocument::clarity`, Swift
  `document.clarity()` with typed `ClarityReport`, and WASM `document.clarity()` with TypeScript
  types. See [`docs/STYLE-CLARITY.md`](docs/STYLE-CLARITY.md).
- **Regional spelling lens** ([#299](https://github.com/cmlarsen/writemorebetter/issues/299)).
  Rust `StyleDocument::regional_spelling(options)`, WASM `document.regionalSpelling(optionsJSON)`
  and Swift `document.regionalSpelling()` report mixed American and British spelling: counts for
  American, British (-ise) and Oxford (-ize) spelling, the dominant system, the variant words seen,
  and flags for tokens no dominant system accepts or minority forms of one word. Oxford -ize is
  British, words valid in both are never markers, mid-sentence capitals and quoted titles in
  narration are skipped as names, and each flag carries the narration/dialogue mode at the marker.
  Options `minimumMarkers` (default 3) and `system` (fix the expected system). The bundled
  `style/data/spelling/regional.txt` (153,303 bytes) is a modified table derived from VarCon
  2020.12.07 by `scripts/build-regional-spelling.py`; its Atkinson, Titze and Ispell notices are in
  NOTICE and `style/data/licenses/varcon.txt`. See
  [`docs/STYLE-REGIONAL-SPELLING.md`](docs/STYLE-REGIONAL-SPELLING.md).
- **Emotion tells lens** ([#304](https://github.com/cmlarsen/writemorebetter/issues/304)). Rust
  `StyleDocument::emotion_tells`, WASM `document.emotionTells(optionsJSON?)` and Swift
  `document.emotionTells()` flag narration that names a feeling: *she was afraid*, *he felt angry*,
  *she felt a surge of relief*, *fear washed over her*. Each tell has its span, the emotion and verb
  spans, the lexicon word and its pattern (`predicate`, `object`, `subject`); coordinated feelings
  on one verb (*was angry and afraid*) are one tell. Per-mode counts and
  rates are in `groups`. Dialogue, quoted text included, is skipped unless `includeDialogue` is
  set, and hedges such as *I'm afraid so* and *I'm sorry* are never flagged. The lexicon
  (`style/data/emotions/emotions.tsv`, 760 words, 10 KB) is derived from Princeton WordNet 3.0,
  weighted by the bundled Lancaster norms, and ships as a separate data file under the WordNet
  License (`style/data/licenses/WordNet-LICENSE`, NOTICE). `scripts/build-emotion-lexicon.py`
  rebuilds it. Hand-judged precision on public-domain published fiction is 0.91 on a held-out
  sample of 100 (0.94 on the 200-row development sample). The WASM module grows by 46 KB
  (5,678,182 → 5,724,344 bytes; 1,461,541 → 1,474,140 with `gzip -9`). See
  [`docs/STYLE-EMOTION-TELLS.md`](docs/STYLE-EMOTION-TELLS.md).
- **Alliteration lens** ([#307](https://github.com/cmlarsen/writemorebetter/issues/307), part).
  Rust `StyleDocument::alliteration(options)`, WASM `document.alliteration(optionsJSON)` and Swift
  `document.alliteration()` report runs of three or more different stressed words in one sentence
  that share a first consonant sound, separately for narration and dialogue, with per-1,000-word
  counts. Informational only: fiction alliterates on purpose. Function words are skipped without
  breaking a run; options `minimumWords` (3), `maximumGap` (0) and `skipDialogue`. First sounds
  come from a new CMUdict-derived table, `style/data/pronunciation/cmudict-onsets.tsv` (each
  word's first phoneme, only for the 1,943 words the spelling guess gets wrong; 20,888 bytes),
  built by `scripts/build-cmudict.py` next to the syllable table and covered by CI's staleness
  check. See [`docs/STYLE-ALLITERATION.md`](docs/STYLE-ALLITERATION.md).
- **Vague words and hidden verbs lenses** ([#307](https://github.com/cmlarsen/writemorebetter/issues/307),
  first half). Rust `StyleDocument::vague_words` / `hidden_verbs`, WASM
  `document.vagueWords(optionsJSON?)` / `document.hiddenVerbs(optionsJSON?)` and Swift
  `document.vagueWords()` / `document.hiddenVerbs()` count, per narration and dialogue, words that
  blur a sentence (fillers, hedges such as *sort of* and *a bit*, *a lot*, *thing*, *nice*) and
  nominalizations doing a verb's work (*made a decision*, *came to the realization*, *the arrival
  of*). Each match has its span, term and category or pattern; hidden verbs name the related verb
  (`term`: *decide*) but never rewrite. Options `fillers` and `ofPhrases` (both default true). The
  vague list is project-authored plus the 80 words/fillers entries backed by mikpanko/expresso
  (both MIT; `style/data/licenses/fillers.txt`, NOTICE); words already flagged by LanguageTool's
  `TIRED_INTENSIFIERS` or the overused-words fillers are left out. The 313-row noun→verb table
  (`style/data/hidden-verbs/nominalizations.tsv`, 7 KB) is derived from Princeton WordNet 3.0 by
  `scripts/build-hidden-verbs.py` and ships under the WordNet License beside the emotion lexicon;
  `scripts/build-filler-words.py` rebuilds the filler list. Hand-judged precision on public-domain
  fiction: 48/50 (vague words) and 36/40 (hidden verbs) on held-out samples. The WASM module grows
  by 121 KB (7,053,229 → 7,174,085 bytes; 1,771,694 → 1,792,470 with `gzip -9`). See
  [`docs/STYLE-VAGUE-WORDS.md`](docs/STYLE-VAGUE-WORDS.md) and
  [`docs/STYLE-HIDDEN-VERBS.md`](docs/STYLE-HIDDEN-VERBS.md).
- Opt-in manner-adverb counts by lemma and mode, with dialogue-tag adverbs marked for deduplication; personal and possessive pronoun shares of words; and a condition transition category (`if`, `unless`, `provided that`, `as long as`, `in case`) ([#301](https://github.com/cmlarsen/writemorebetter/issues/301)).

### Changed

- **The rules binary format is now version 32** (was 31) for the Harper builtins, so 0.10.0's
  `en_rules.bin` does not load; use this release's data zip. The tokenizer format stays 18
  ([#311](https://github.com/cmlarsen/writemorebetter/pull/311)).
- **`diction` no longer needs a table.** Rust `StyleDocument::diction(options, None)`, WASM
  `document.diction(optionsJSON)` and Swift `document.diction()` use the bundled table; passing a
  table (JSON object or TSV text) still replaces it for that call. `missingTable` is no longer
  returned; the error kind is kept so existing matches compile. Swift's `diction(table:)` argument
  is now optional (`String? = nil`). `ConcretenessTable::bundled()` returns the parsed table, and
  TSV tables may carry `#` comment lines.
- **Diction's default `abstractBelow` is 2.85** (was 2.5), tuned for the bundled table: at 2.5 its
  sentence means almost never formed an abstract run. On held-out Crane, Wharton and Gatsby chapters
  the new default flags 11 runs, all judged useful. Callers that pass their own table also get
  2.85 unless they set `abstractBelow`: a 1–5 human table such as Brysbaert-style norms, which the
  2.5 default was tuned for, now flags far more sentences, so set `abstractBelow: 2.5` to keep the
  previous behaviour. A table on another scale should set its own threshold.
- **Intensifiers before an adjective keep their own rating.** When the tagger reads a word such as
  *fucking* or *freaking* as both an adverb and a present participle and an adjective follows
  (*was fucking furious*), diction and sensory balance look up the word itself instead of the verb
  lemma.
- **The WASM module grows by 184 KB** (`writemorebetter_wasm_bg.wasm`, release, wasm-bindgen
  0.2.129: 5,495,225 → 5,678,797 bytes; 1,401,961 → 1,462,831 with `gzip -9`). The bundled table
  loads once, in about 4.5 ms natively.

### Fixed

- **`EN_PLAIN_ENGLISH_REPLACE` no longer suggests the literal text `[OMIT]`.** The word list
  marks "delete this phrase" with `[OMIT]`, which LanguageTool 6.6 passes through as a
  suggestion. The builtin now offers a deletion that also removes the surrounding space, or
  paired commas or dashes, and capitalizes the next word at a sentence start; the message says
  "omit it". See [`docs/PROFILE.md`](docs/PROFILE.md#enabling-rules).
- `wmb-probe` exits with an error message instead of panicking on an unknown `ENABLE` selector.

# 0.10.0 (2026-10-20)

### Added

- **Prose style reports.** A new crate, `writemorebetter-style` (MIT OR Apache-2.0), reports
  patterns across a whole chapter or manuscript to a fiction writer: sentence length and monotony,
  sentence starts, clichés and redundancies, dialogue tags, echoes, repeated phrases, overused and
  filler words, transitions, pronoun starts, sensory balance, diction (concreteness) and pacing
  ([#241](https://github.com/cmlarsen/writemorebetter/issues/241)). The reports diagnose and never
  generate replacement prose: each returns spans, counts and, where it has a baseline, the
  difference from published fiction. They run on device with no network, over a document built on
  save or idle rather than on each keystroke. The crate holds no LGPL data and does not duplicate
  the LanguageTool rules that `nlprule` ports; where a report overlaps one (echoes and
  `EN_REPEATEDWORDS`, long sentences and `TOO_LONG_SENTENCE`), its doc says how they differ. Swift
  and WASM reach the reports through the existing `Checker`, so an app still links one library and
  loads the tokenizer once. The design and decision log are in
  [`docs/STYLE-REPORTS.md`](docs/STYLE-REPORTS.md), and each report has its own `docs/STYLE-*.md`.
- **A recommended default set, and opt-in lenses.** The library runs only the reports a caller asks
  for. The recommended default set for an editor is **clichés, dialogue tags, sentence length
  statistics and monotony runs, and sentence starts**; on the six `bench/public` texts it gives 27
  flags, down from 196 before the reports were tuned for novelists
  ([#273](https://github.com/cmlarsen/writemorebetter/pull/273)). **Echoes, repeated phrases,
  long-sentence flags, overused and filler words, transitions, pronoun starts, sensory balance,
  diction and pacing** are opt-in lenses: their flags are accurate but too frequent to show by
  default, or have not yet been judged on real manuscripts. Long sentences are a sentence-length
  option (`{"includeLongSentences": true}`).
- **`StyleDocument` analyzes a chapter once and runs each report on its own**
  ([#275](https://github.com/cmlarsen/writemorebetter/issues/275)). Swift's
  `checker.styleDocument(text:options:)` and WASM's `checker.styleDocument(text, optionsJSON)`
  return a document with one method per report: `sentenceLength`, `sentenceStarts`, `cliches`,
  `dialogueTags`, `echoes`, `repeatedPhrases`, `overusedWords`, `transitions`, `pronounStarts`,
  `sensoryBalance`, `diction`, `pacing`, plus `stats` for the shared word and sentence counts.
  Swift methods return typed `Codable`, `Equatable`, `Sendable` records (`SentenceLengthReport`,
  `ClicheReport`, …) whose `Span.nsRange` is ready for attributed text; the raw `*Json` methods
  return the same JSON. WASM methods return JSON strings, and TypeScript declarations for every
  result ship in the generated package (`wasm/style-results.d.ts`) for `import type`. Binding spans
  are UTF-16 `{start, end}`. Options are JSON in camelCase (snake_case is accepted, unknown keys are
  rejected). The baseline-aware methods (`sentenceLength`, `sentenceStarts`, `dialogueTags`,
  `overusedWords`) use the bundled baseline when the argument is omitted, `"none"` to turn
  comparisons off, or a JSON override; Swift spells these `StyleBaseline.bundled`, `.none` and
  `.custom(json)`. A document owns its analysis and can outlive its checker; WASM callers must call
  `free()` on it. In Rust, `analyze_document(&tokenizer, text, &DocumentOptions)` builds the
  `Document`, each report is `reports::<name>::report(&Document, &Options, …)` returning
  `Result<_, StyleError>`, and `StyleDocument` wraps the same calls with typed results.
- **Document options.** `skipNames` lists character and place names to leave out of counts and
  denominators, `hardWrapped` makes paragraphs end at blank lines for hard-wrapped imports (the
  default is one paragraph per line), and `includeTokenDetails: false` builds a compact document
  without readings, which the reports that need readings reject with `needsTokenDetails`.
- **Diction ships without a concreteness table.** The diction report flags abstract narration
  sentences and runs of them ([#250](https://github.com/cmlarsen/writemorebetter/issues/250)), and
  it needs a word-to-concreteness table that the caller supplies as JSON or TSV
  (`document.diction(table:)` in Swift, `document.diction(optionsJSON, tableText)` in WASM,
  `ConcretenessTable::parse` in Rust). Without one it fails with `missingTable` instead of returning
  an empty report. No table is bundled: the standard ratings (Brysbaert et al., 2014) are
  CC BY-NC-ND, so they cannot ship here, and no replacement has been chosen. The owner decided on
  2026-10-20 to ship the style reports now with diction as it is; a bundled concreteness table will
  come in a later release.
- **Style report errors.** `StyleError` has five kinds: `invalidOptions`, `invalidBaseline`,
  `needsTokenDetails`, `missingTable` and `invalidTable`, and all but the first name the report.
  WASM throws an `Error` named `StyleError` with `kind` and `report` properties. Swift throws the new
  `CheckError.ReportInput` with the same message.
- **Sentence length and sentence starts**
  ([#244](https://github.com/cmlarsen/writemorebetter/issues/244)). Sentence length gives the mean,
  percentiles and histogram for narration and dialogue separately, monotony runs (four or more
  sentences whose lengths are within two words of each other) and, opted in, the three longest
  sentences of at least 40 words per chapter. Sentence starts classifies each sentence's opening
  (subject, adverb, introductory phrase, coordinating or subordinating conjunction, gerund,
  infinitive, past participle, inversion, other) and flags runs of three or more sentences opening
  with the same word. Both compare against the bundled published-fiction bands, highlight only
  values outside them, withhold percentages below 50 sentences and mark results under 200 as low
  confidence. Chapter headings such as `CHAPTER 1. Loomings.` start a new chapter and are left out
  of the statistics ([#273](https://github.com/cmlarsen/writemorebetter/pull/273)).
- **Dialogue tags** ([#249](https://github.com/cmlarsen/writemorebetter/issues/249)). The report
  finds the tag of each quotation (before, after or split around it), counts action beats
  separately, and flags showy speech verbs ("cried", "demanded" and other bookisms), non-speech
  tags (`“Fine,” she smiled.`), `-ly` manner adverbs and runs of more than six untagged exchanges.
  "Said", "asked" and other neutral verbs are counted, not flagged. Scare quotes, titles and quoted
  labels are not speech. Dialogue introduced by a dash is not detected yet.
- **Clichés and redundancies** ([#246](https://github.com/cmlarsen/writemorebetter/issues/246)).
  The report matches 698 clichés from no-cliches (MIT) and redundancy lists from
  retext-redundant-acronyms (MIT) and proselint (BSD-3-Clause), through inflections ("kicked the
  bucket"), possessive slots ("aired the captain’s dirty laundry") and hard line wraps, and counts
  narration and dialogue separately. Entries that a rule in `nlprule` already flags ("ATM machine",
  "PIN number") are left out. Redundancy coverage is thin: only acronym redundancies and 20
  proselint entries cleared licensing. Callers can compile their own list with `ClicheList::new`.
- **Echoes and repeated phrases** ([#245](https://github.com/cmlarsen/writemorebetter/issues/245)),
  opt-in. Echoes flags a content word whose lemma family (`thought` … `thoughts`, `wound` …
  `wounds`) occurred shortly before it. Repeated phrases lists lemma n-grams of two to five words
  with at least two content words that recur, stopping at commas, dashes and quotes.
- **Overused and filler words** ([#248](https://github.com/cmlarsen/writemorebetter/issues/248)),
  opt-in. Content lemmas used at least four times as often as in published fiction, and at least
  five times (more in long documents), are flagged against the bundled Ngram table; nouns are
  opt-in. The fillers "just", "really", "very", "quite", "actually", "simply", "suddenly",
  "somehow" and complementizer "that" are reported with their rates. British spellings count under
  the American lemma (`grey` with `gray`).
- **Transitions and pronoun starts** ([#247](https://github.com/cmlarsen/writemorebetter/issues/247)),
  opt-in. Transitions counts transition words of five kinds (contrast, cause, time, addition,
  example) and flags the same one repeated within five sentences. Pronoun starts gives each
  pronoun's share of sentence openings and flags runs. Neither has a baseline yet.
- **Sensory balance** ([#250](https://github.com/cmlarsen/writemorebetter/issues/250),
  [#282](https://github.com/cmlarsen/writemorebetter/issues/282)), opt-in. Sense words are
  classified by their strongest modality in the Lancaster Sensorimotor Norms (sight, sound, touch,
  taste, smell, interoception) and shared out per chapter and scene, narration and dialogue
  separately. Figurative perception ("felt that", "looked as if") and speech tags do not count.
- **Pacing** ([#251](https://github.com/cmlarsen/writemorebetter/issues/251)), opt-in. Each
  paragraph gets three signals (mean sentence length, past-perfect share, cognition-verb share),
  and `slowRuns` marks three or more paragraphs, or more than 400 words, without speech in which
  two signals pass their thresholds. The defaults (18 words, 15%, 15%) were chosen from published
  flag rates and a five-model label set
  ([#291](https://github.com/cmlarsen/writemorebetter/pull/291)). A pacing strip should highlight
  only `slowRuns`; single paragraphs that pass are mostly noise.
- **Bundled baselines and data, aggregate numbers only.** Sentence length and sentence starts
  compare against author-weighted p5/p95 bands measured by the owner over 1,559 chapter windows of
  210 published novels by 71 authors; the repository holds only those aggregate numbers, never
  text. Overused words compares against a 14,382-lemma table built from Google Books Ngram English
  Fiction 1-grams (version 20200217, editions 1990–2019, CC BY 3.0). Sensory balance uses an
  11,456-word table derived from the Lancaster Sensorimotor Norms (CC BY 4.0), whose source CSV was
  checked against OSF's own download on 2026-10-20
  ([#289](https://github.com/cmlarsen/writemorebetter/issues/289)). The cliché lists keep their MIT
  and BSD licenses, and the other word lists were written for this crate. Every source is credited
  in `NOTICE` and [`docs/STYLE-DATA-LICENSES.md`](docs/STYLE-DATA-LICENSES.md). There is no bundled
  dialogue-tag baseline yet.
- **`analyze()` returns the tokenizer's sentences and tagged tokens**
  ([#242](https://github.com/cmlarsen/writemorebetter/issues/242)). `Tokenizer::analyze` in Rust,
  `Checker.analyze(text:)` in Swift and `checker.analyze(text)` in WASM return owned sentences and
  tokens with their lemma and POS readings after disambiguation, chunk tags and whitespace and
  punctuation flags. Rust spans carry UTF-8 byte, Unicode scalar and UTF-16 ranges; the bindings
  return UTF-16 offsets. The readings match LanguageTool 6.6's `--taggeronly` output on the test
  transcripts, with one documented chunk difference on emoji. See [`docs/ANALYSIS.md`](docs/ANALYSIS.md).
- **`analyzeDocument()` returns the whole-document pass the reports use**
  ([#243](https://github.com/cmlarsen/writemorebetter/issues/243)). Swift's
  `Checker.analyzeDocument(text:options:)` and WASM's `checker.analyzeDocument(text, options)`
  return paragraphs, sentences, tokens, quoted-dialogue spans with speech flags, a lemma index and
  word counts. Its JSON keys are snake_case, unlike the reports' camelCase. Quotation spans score
  88.6% F1 on LitBank and 98.6% on QuoteLi3's *Pride and Prejudice*.
- **WASM `check()` returns UTF-16 offsets too**
  ([#257](https://github.com/cmlarsen/writemorebetter/issues/257)). Each suggestion has
  `start16`/`end16` in UTF-16 code units, the convention of `analyze()` and the style reports, so
  `text.slice(start16, end16)` is the flagged text even after emoji. `start`/`end` still count
  Unicode scalars and will be deprecated later.
- **Benchmarks.** `bench/style` measures the analysis and every report: tier 1 scores sentence
  boundaries, tags and lemmas on UD English-EWT and quotation spans on LitBank and QuoteLi3, tier 2
  injects errors into pinned Standard Ebooks prose and scores each report, and tier 3 computes
  published-fiction flag rates and bands for the owner to run locally, printing aggregate numbers
  only. CI gates tiers 1 and 2 and runs tier 3's self-tests
  ([#259](https://github.com/cmlarsen/writemorebetter/pull/259),
  [#270](https://github.com/cmlarsen/writemorebetter/pull/270),
  [#283](https://github.com/cmlarsen/writemorebetter/pull/283),
  [#290](https://github.com/cmlarsen/writemorebetter/pull/290),
  [#293](https://github.com/cmlarsen/writemorebetter/pull/293)).

### Changed

- **No breaking change to checking; the data binaries are unchanged.** `check()`, `suggest()` and
  the bindings' `Checker` keep their signatures, and the tokenizer and rules binary format versions
  stay 18 and 31, so 0.9.0's `en_tokenizer.bin` and `en_rules.bin` still load. One source change in
  Swift: `CheckError` has a new case, `ReportInput`, so a `switch` over `CheckError` without a
  `default` needs it.
- **The WASM module is about 60% larger.** Built the same way as CI (release, wasm-bindgen
  0.2.129), `writemorebetter_wasm_bg.wasm` grows from 3,340,768 bytes in 0.9.0 to 5,507,374
  (867 KB to 1.40 MB with `gzip -9`); after `wasm-opt -Oz` it grows from 1,979,906 to 3,255,089
  bytes (779 KB to 1.27 MB gzipped). Most of the growth is the style crate with its bundled
  tables (the Ngram lemma table is 233 KB and the Lancaster table 193 KB before compression); the
  fancy-regex upgrade adds about 4%.
- **Style documents cost memory and time while retained.** A `StyleDocument` for the 149 KB
  `moby150k` excerpt holds about 11–12 MB of WASM linear memory, and WASM's linear memory does not
  shrink back after a document is freed, so retain the chapter on screen, `free()` it on
  navigation and do not cache every chapter. Building that document takes about 1 s natively and
  1.2 s in WASM on an Apple M5 Pro, and each report then takes 2–45 ms; build documents off the UI
  thread. `analyzeDocument()` JSON is large (46 MB for a 582 KB novel with token details, 33 MB
  compact), so prefer `StyleDocument` when only the reports are needed.
- **fancy-regex is upgraded from 0.5 to 0.19** in `nlprule`'s `regex-fancy` backend (the one WASM
  uses) and in `srx` ([#288](https://github.com/cmlarsen/writemorebetter/pull/288)). Loading the
  English rules takes 66 ms instead of 47 ms; check time and peak memory are unchanged (3.0 s and
  about 101 MB on the 150 KB Moby-Dick excerpt).

### Fixed

- **Sentence segmentation no longer takes an hour on some long texts.** One English SRX rule needs
  look-around, so it ran on fancy-regex, which gives up after a million backtracks counted over the
  whole unanchored search; a text with no match in its first ~900K characters failed, and the
  segmenter retried from every following character, taking 70–100 minutes on some ~1 MB novels
  ([#279](https://github.com/cmlarsen/writemorebetter/issues/279)). Look-around rules now find
  candidates with regex-automata and confirm each with an anchored fancy-regex match, so the scan is
  linear and the backtrack limit stays in place; a full check of those novels now takes 16–21 s
  ([#287](https://github.com/cmlarsen/writemorebetter/pull/287),
  [#288](https://github.com/cmlarsen/writemorebetter/pull/288)).
- **Look-around rules no longer miss matches in very long sentences with the `regex-fancy`
  backend.** The backend treated fancy-regex's backtrack-limit error as no match, which began at
  about 250K characters without a sentence break. It now retries that regex with a limit scaled to
  the input and returns exactly the unlimited result
  ([#288](https://github.com/cmlarsen/writemorebetter/pull/288)).
- **`scripts/build-xcframework.sh` builds in one pass with Xcode 27.** The script exported
  `MACOSX_DEPLOYMENT_TARGET=12.0` for the whole run, which also linked the host's proc-macro
  libraries at 12.0, and Xcode 27 rejected them (`mis-aligned LINKEDIT string pool`, or `E0463`
  on a fresh `target/`). The macOS and iOS minimums are now set on the Apple slice builds only;
  run `cargo clean` once if an older checkout hit this
  ([#258](https://github.com/cmlarsen/writemorebetter/issues/258)).
- **`nlprule-build` builds again after every zip 0.5 and 0.6 release was yanked.** It now uses
  zip 9 ([#272](https://github.com/cmlarsen/writemorebetter/pull/272)).

# 0.9.0 (2026-10-15)

### Added

- **Compound, proper-noun, dash, wordiness, redundancy and profanity checks.** Six more of
  LanguageTool's Java-coded English rules run as builtins
  ([#81](https://github.com/cmlarsen/writemorebetter/issues/81)): `EN_COMPOUNDS` flags compounds
  written as separate words (`part time` → `part-time`, `back fire` → `backfire`) and reports
  LanguageTool's sub-rule ids such as `EN_COMPOUNDS_PART_TIME`; `EN_SPECIFIC_CASE` flags proper
  nouns with the wrong capitalization (`Harry potter` → `Harry Potter`); `EN_DASH_RULE` flags a
  compound written with a dash (`T—shirt` → `T-shirt`); `EN_PLAIN_ENGLISH_REPLACE` and
  `EN_REDUNDANCY_REPLACE` suggest plainer wording (`in close proximity to` → `near`) and drop
  redundant words (`tuna fish` → `tuna`); `PROFANITY` flags offensive terms without a suggestion.
  As in LanguageTool 6.6, only `EN_COMPOUNDS` and `EN_SPECIFIC_CASE` are on by default; the
  other four are off (LanguageTool marks them default-off or picky) and can be enabled through
  `Rules::builtins_mut`. Their word lists are copied from LanguageTool by
  `build/make_build_dir.py`, so a build directory needs regenerating. `Builtin::check_paragraph`
  takes the speller as a new last argument. The rules binary format version is now 29.
- **Misspelled multi-word names and phrases are caught.** The 14 `MULTITOKEN_SPELLING` rules
  (`EN_MULTITOKEN_SPELLING_TWO`, `_THREE` and `_FOUR`) now compile: `MultitokenSpellerFilter` is
  ported from LanguageTool 6.6, so "Yuval Noha Harari" suggests "Yuval Noah Harari" and "Rocki
  Mountains National Park" suggests "Rocky Mountain National Park"
  ([#72](https://github.com/cmlarsen/writemorebetter/issues/72)). `build/make_build_dir.py`
  writes LanguageTool's phrase lists (`en/multiwords.txt` and `spelling_global.txt`, nouns and
  adjectives only, as LanguageTool reads them) to `spell/multitoken.txt`, and the compiler stores
  them in the rules binary; a build directory without that file leaves the rules uncompiled.
  The rules stay on in the fiction profile: they flagged nothing in `bench/public`. The rules
  binary format version is now 30.
- **Word-repetition statistics.** Four of LanguageTool's Java-coded style rules run as builtins,
  all off by default as in LanguageTool 6.6 and enabled through `Rules::builtins_mut`
  ([#83](https://github.com/cmlarsen/writemorebetter/issues/83)). `EN_REPEATEDWORDS` (`picky`)
  flags a word from LanguageTool's synonym list (e.g. "nice", "definitely", "choose") that was
  used in an earlier sentence within 150 words, reports it as `EN_REPEATEDWORDS_<WORD>` and
  suggests inflected synonyms; it checks only sentences that end in `.`, `!` or `?`.
  `TOO_OFTEN_USED_ADJECTIVE_EN`, `TOO_OFTEN_USED_NOUN_EN` and `TOO_OFTEN_USED_VERB_EN` flag every
  use of a word that makes up 5% or more of the adjectives, nouns or verbs of a text with at
  least 100 of them; they report once the whole text has been read, and give way to any
  suggestion they overlap. Both kinds count within one `Rules::suggest` call and start from
  zero on every call, so an app that checks text incrementally sees only the text it passes.
  Disabling the new `DIRECT_SPEECH` case of a `TOO_OFTEN_USED_*` rule (e.g. the profile selector
  `BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH`) leaves dialogue out of its counts and matches;
  the fiction profile does this for all three. The synonym list is LanguageTool's `en/synonyms.txt`,
  which `build/make_build_dir.py` now copies into the build directory. The rules binary format
  version is now 31.

### Fixed

- **A match a rule filter vetoes no longer hides a later overlapping match of the same rule.**
  The matcher claimed a match's characters before the filter ran, so when the filter vetoed it,
  the next candidate starting inside it was skipped. LanguageTool 6.6 drops a vetoed match
  before it removes overlaps, so `“yuval Noha Harari,” she said.` now gets "Noah Harari" from
  `EN_MULTITOKEN_SPELLING_TWO` after the filter rejects "yuval Noha".
- **Names from LanguageTool's global spelling list are accepted by `Speller::is_correct` and
  suggested.** Single-word entries of `spelling_global.txt` (`Josquin`, `fit4work`, `Nestlé`) are
  now speller words for every variant, as in LanguageTool 6.6, so `Josquinn` suggests `Josquin`
  and the speller-backed rule filters stop treating them as misspelled
  ([#167](https://github.com/cmlarsen/writemorebetter/issues/167)). Recompile rules binaries to
  pick up the words; the format is unchanged.
- **Spelling suggestions ignore the case of accented and non-Latin letters.** The suggestion
  search compares characters instead of UTF-8 bytes, so a capitalized or all-caps word matches
  its dictionary entry whatever its letters (`ÉCLAIRR` → `ÉCLAIR`, `SØRENN` → `SØREN`, Greek and
  Cyrillic names too), and an accented letter counts as one edit, not two
  ([#144](https://github.com/cmlarsen/writemorebetter/issues/144)). Lowercase words are searched
  case-insensitively too, as LanguageTool 6.6's English spellers compare letters, so a lowercased
  name suggests the name first (`søren` → `Søren`, `zoë` → `Zoë`, `paris` → `Paris`).
- **Speller-backed rule filters use the speller LanguageTool 6.6 uses.** `NUMBERS_IN_WORDS` asks
  the default variant's (`en_US`) speller whatever variant is selected, so with `en_GB` it
  suggests `color` for `c0lor` as LanguageTool does, and `FindSuggestionsFilter` (`PRP_MD_NN`,
  `A_INFINITIVE`) searches the tagger dictionary instead of the speller
  ([#135](https://github.com/cmlarsen/writemorebetter/issues/135)). While a variant other than the
  default one is selected, the default variant's checker is kept loaded as well.
- **Disambiguation antipatterns block a match the way LanguageTool 6.6 does.** A
  disambiguation rule is skipped when one of its antipatterns touches or overlaps any part of
  the match from its first token to its last, including tokens a `skip` passed over, so an
  antipattern on the word right before or after the match (no space between) also blocks it.
  Disambiguation rules keep dropping blocked matches rather than immunizing tokens as grammar
  rules do, which is what 6.6's `DisambiguationPatternRuleReplacer` does
  ([#170](https://github.com/cmlarsen/writemorebetter/issues/170)).
- **A case-sensitive antipattern makes its disambiguation rule's pattern case-sensitive, as in
  LanguageTool 6.6.** When the last antipattern before a disambiguation `<pattern>` has
  `case_sensitive="yes"`, the pattern is compiled case-sensitive too, so `STATUS_QUO` tags only
  lowercase `status quo`
  ([#177](https://github.com/cmlarsen/writemorebetter/issues/177)).

# 0.8.0 (2026-10-14)

### Added

- **Repetition style rules (`min_prev_matches` / `distance_tokens`).** Rules from LanguageTool
  6.6 that fire only when a phrase recurs nearby now compile and run: the seven
  `REPETITIONS_STYLE/REP_PASSIVE_VOICE` rules (a fifth passive within 80 tokens of four earlier
  ones; `picky`, so off by default) and `REPETITIONS_STYLE/REP_THANK_YOU_FOR/0` (a second
  "Thank you for" within 60 tokens; on by default)
  ([#73](https://github.com/cmlarsen/writemorebetter/issues/73)). As in LanguageTool, every match
  counts as a previous one whether or not it was reported, distance is measured in tokens across
  sentence and paragraph boundaries, and the count starts from zero on every `Rules::suggest`
  call, so an app that checks text incrementally sees repetitions only within the text it passes
  (`Rules::apply` sees only its one sentence). The fiction profile turns `REP_THANK_YOU_FOR` off,
  since dialogue repeats it on purpose. The rules binary format version is now 26.
- **The Swift binding exposes the mother tongue.** `Checker.setMotherTongue(motherTongue:)` and
  `Checker.motherTongues()` mirror the wasm `setMotherTongue` / `motherTongues`, so Swift apps
  can opt in to the false-friend rules ([#138](https://github.com/cmlarsen/writemorebetter/issues/138)).
- **Whitespace and double-punctuation checks.** Four more of LanguageTool's Java-coded rules run
  as builtins ([#80](https://github.com/cmlarsen/writemorebetter/issues/80)):
  `DOUBLE_PUNCTUATION` flags `..` and `,,` (ellipses, `?..`, `!..` and `…` next to a dot are
  left alone), `WHITESPACE_RULE` flags repeated whitespace such as two spaces between words,
  `SENTENCE_WHITESPACE` flags a sentence that follows the previous one without a space
  (`world.This`), and `EMPTY_LINE` flags more than one empty line between paragraphs. As in
  LanguageTool 6.6, `EMPTY_LINE` is off by default; enable it through `Rules::builtins_mut`. The
  fiction profile keeps all four but stops `WHITESPACE_RULE` from flagging the spaces that indent
  a paragraph or line, which LanguageTool flags whenever the previous paragraph ends a sentence.
  A profile selector can now name a builtin case, as in `BUILTIN/WHITESPACE_RULE/INDENTATION`. The
  rules binary format version is now 23.
- **Sentence- and paragraph-level style rules from LanguageTool 6.6** run as builtins
  ([#82](https://github.com/cmlarsen/writemorebetter/issues/82)), with LanguageTool's IDs,
  messages, thresholds and spans:
  - `ENGLISH_WORD_REPEAT_BEGINNING_RULE` flags the third of three sentences that begin with the
    same word, or the second of two that begin with the same adverb (`Moreover`, `Indeed`, ...),
    across paragraphs. It is on by default, as in LanguageTool, and the `fiction` profile
    disables it.
  - `TOO_LONG_SENTENCE` (over 40 words) and `TOO_LONG_PARAGRAPH` (over 225 words) are off by
    default, because LanguageTool tags them picky; `Rules::builtins_mut` enables them. Like
    LanguageTool, they give way to any other match that overlaps them.

  Builtins now see the last two sentences before a paragraph and whether the paragraph ends the
  text (`Builtin::check_paragraph`, which replaces `Builtin::check_text_with_previous`). The rules
  binary format version is now 27.
- **The speller-backed rule filters are ported, so nine more rules run.** `FindSuggestionsFilter`
  (`PRP_MD_NN` [5] and [6], `A_INFINITIVE` [0] and [1]), `EnglishSuppressMisspelledSuggestionsFilter`
  (`EN_SPLIT_WORDS_HYPHEN` [0]–[3]) and `EnglishNumberInWordFilter` (`NUMBERS_IN_WORDS`) now
  compile and query the speller while a rule matches
  ([#71](https://github.com/cmlarsen/writemorebetter/issues/71)), so "Go0d morning" suggests
  "Good" and "The break- up" suggests "break-up". `FindSuggestionsFilter` searches the speller's
  word list where LanguageTool searches its tagger dictionary, so some candidates differ
  ([#135](https://github.com/cmlarsen/writemorebetter/issues/135)). The rules binary format
  version is now 22.

- **Rule filters can query the speller while a rule matches.** `Spell::speller()` returns a
  read-only `Speller` for the selected variant with `is_correct(word)` and `suggest(word, n)`,
  and the grammar-rule filters receive it alongside the rule match, ready for the speller-backed
  filter ports ([#70](https://github.com/cmlarsen/writemorebetter/issues/70)). A curly
  apostrophe counts as a straight one and surrounding whitespace (including U+2028) is ignored.
  Checking output is unchanged.
- **False-friend rules for writers whose first language is not English.** `Rules::set_mother_tongue`
  (`setMotherTongue` in the wasm binding) selects a mother tongue, and `Rules::mother_tongues`
  lists the 13 the English binary supports (ca, de, es, fr, ga, gl, it, ja, nl, pl, pt, ru, sv).
  The rules are compiled from LanguageTool 6.6's `false-friends.xml`, with its localized
  messages, into the rules binary as `FALSE_FRIENDS/<ID>/<n>`, and only the selected mother
  tongue's rules run. Selecting a mother tongue is the opt-in: LanguageTool 6.6 tags these rules
  `picky` and runs them only at its picky level, and for German, French, Spanish and Dutch
  speakers it runs n-gram confusion rules instead of these XML rules (n-gram rules are out of
  scope here; [#137](https://github.com/cmlarsen/writemorebetter/issues/137)). No mother tongue is
  selected by default, so checking output is unchanged. The rules binary format version is now 17.
- **Speller-backed rule filters work with spellchecking off.** While no spell variant is
  selected, `Spell::speller()` answers for the language's default variant (`en_US` for English,
  as LanguageTool uses for plain English) instead of returning `None`; spelling suggestions stay
  off. The rule test harness (`--bin test`, `Rules::test_rule`) therefore runs rule examples with
  an `en_US` speller ([#119](https://github.com/cmlarsen/writemorebetter/issues/119)). Rules
  binaries move to format version 15 (`SpellLangOptions::default_variant`); recompile them.
- **`raw_pos` rules that read POS tags or chunks compile and match.** A rule whose
  `<pattern raw_pos="yes">` tests tags or chunks now matches, as in LanguageTool, against the
  sentence as tagged and chunked, before multiword tagging and disambiguation, which
  `Sentence::raw()` exposes. That adds `TYPOS/LOOK_SLIKE/0` (`look sfor` → `looks for`), which
  is off by default ([#130](https://github.com/cmlarsen/writemorebetter/issues/130)).
  `Rules::suggest` keeps the raw tokens only while such a rule is enabled (a text-only
  `raw_pos` rule such as `TYPOS/CONTRACTION_CASE/0` does not count); `Tokenizer::pipe` always
  keeps them. Rules binaries move to format version 18 (`Rule::raw_pos`); recompile them.
- **Grammar-rule suggestions can spell out numbers.** `<match postag="_spell_number_"/>` now
  writes the matched number in English words as LanguageTool 6.6 does (`12` → `twelve`,
  `1001` → `one thousand and one`, `100200` → `one hundred thousand, two hundred`, up to 45
  digits, leading zeros ignored); anything other than plain ASCII digits is not spelled and the
  match is dropped as before. The off-by-default `STYLE/SENT_START_NUM/0` now suggests
  `Twelve` for `12 soldiers were killed!`
  ([#163](https://github.com/cmlarsen/writemorebetter/issues/163)). Rules binaries move to
  format version 24 (`PosReplacer::spell_number`); recompile them.

### Changed

- **The Swift package ships a dynamic framework and supports macOS 12.** `WriteMoreBetterFFI.xcframework`
  now holds a `WriteMoreBetterFFI.framework` per slice instead of a static library, so an app can link it
  next to another Rust/UniFFI XCFramework: only the UniFFI C functions are exported, so Rust's `std` and
  shared crates no longer collide, and the module map sits inside the framework instead of a top-level
  `Headers/module.modulemap` that Xcode copies to `include/`. `Package.swift` declares macOS 12 (was 13) and
  every object, including the Oniguruma C code, targets macOS 12.0 and iOS 16.0. The generated Swift API is
  unchanged. The release consumer check now links a second Rust static library and rejects a framework that
  exports Rust symbols ([#239](https://github.com/cmlarsen/writemorebetter/issues/239)).

### Fixed

- **The Swift package no longer segfaults when built with Xcode 27.** `build-xcframework.sh` ran Xcode's
  `strip -S` over the static library, and Xcode 27's `strip` drops the `.llvm.<hash>` suffixes that keep
  LLVM's promoted local functions apart. Two different `<char as Pattern>::is_contained_in` copies (a general
  one and one specialized for U+2019 with a different calling convention) ended up with one name, so calls
  landed in the wrong function and crashed in the tokenizer and speller. The framework is now linked as a
  dylib with the linker dropping local symbols and debug info, and `build-xcframework.sh` never runs `strip`
  ([#238](https://github.com/cmlarsen/writemorebetter/issues/238)).

- **Token patterns match like LanguageTool 6.6, without backtracking.** An optional token
  (`min="0"`) is skipped whenever the pattern token after it matches the same word, a `skip`
  window comes from the last token that matched, and a repeated token takes as many words as it
  can ([#160](https://github.com/cmlarsen/writemorebetter/issues/160)). `COMMA_COMPOUND_SENTENCE`
  now flags "They've acted irresponsibly and immorally and it has to stop.", and
  `DO_XX_MD_VB` no longer flags "Would that I could".

- **An absent optional token no longer applies its own `skip`.** The words in a `skip` window
  are checked against the `scope="next"` exceptions of the pattern token before them, whether
  or not that token matched, as in LanguageTool 6.6
  ([#161](https://github.com/cmlarsen/writemorebetter/issues/161)). The rules binary format
  version is now 28 and the tokenizer binary format version is now 17.

- **A `scope="next"` exception after an optional token depends on whether the token matched.**
  The exception checks the next word's first reading only when the last token that matched has
  no `skip`, decided per match instead of once when the rule is compiled
  ([#151](https://github.com/cmlarsen/writemorebetter/issues/151)).

- **Repeated tokens follow LanguageTool 6.6's `min` and `max`.** `min="2"` without `max`
  matches exactly two words instead of up to 20, `max="-1"` has no limit instead of 20, and a
  word an antipattern immunized no longer trips a `scope="next"` exception inside a `skip`
  window ([#214](https://github.com/cmlarsen/writemorebetter/pull/214)).

- **"best" before a preposition is tagged as a superlative adjective, as in LanguageTool 6.6.**
  The English tokenizer config no longer skips the `BEST_JJS` disambiguation rule, a leftover
  from upstream nlprule, so `PRP_MD_NN` now flags "It may best to add this" and suggests "may be
  best" ([#221](https://github.com/cmlarsen/writemorebetter/issues/221)). Two disambiguation
  actions now behave as in LanguageTool 6.6: `<disambig postag="…">` without an action leaves one
  reading (the last one with that tag, so "best" becomes `well/JJS` rather than `good/JJS` and
  `well/JJS`), and `<disambig action="remove">` with a `<wd>` that has only a lemma removes every
  reading with that lemma, so "installed" no longer keeps `instal` readings.
- **The apostrophe-style rules see a curly apostrophe, as in LanguageTool 6.6.** The opt-in
  `TYPEWRITER_APOSTROPHE` rule now flags `’s` in `actress’s` and suggests `'s` (a straight
  apostrophe in a suggestion is no longer made curly when that would repeat the flagged text),
  and the opt-in `TYPOGRAPHICAL_APOSTROPHE` rule no longer flags a `’s` that is already curly:
  their filter read the token after its curly apostrophe was normalized to a straight one. The
  rules compiler also keeps a whitespace-only `<marker>` in rule examples, so `CONSECUTIVE_SPACES`
  and `CONSECUTIVE_SPACES_SINGLE_CHAR` pass their examples. Checking output with the default rules
  is unchanged on the golden texts ([#76](https://github.com/cmlarsen/writemorebetter/issues/76)).
- **A `<token>` combining a `<match>` reference with literal text matches both, as in LanguageTool
  6.6.** `<token><match no="0"/>s</token>` now matches the referenced token's text followed by
  `s` instead of the referenced text alone, so `TYPO_OR_OF` (off by default) no longer flags
  "the active couple or actives" ([#164](https://github.com/cmlarsen/writemorebetter/issues/164)).
  Literal text around a reference that also carries `postag` or `regexp="yes"` is rejected at
  compile time. `TOKENIZER_FORMAT_VERSION` is now 16 and `RULES_FORMAT_VERSION` 25, so binaries
  built by earlier versions must be rebuilt.
- **Spelling suggestions rank by edit distance, then frequency, as in LanguageTool 6.6.** A
  common word two edits away no longer outranks a rarer word one edit away, and only the
  candidates at the smallest edit distance found are suggested, so `Hecla` suggests `Hekla`
  instead of `Held`; after a match that differs only in case, the closer candidates follow it
  (`McCARTHY` → `McCarthy`, `McCarty`, `McCartney`, `McCarry`). Dropping a word's first letter
  now counts as one edit (`Aduke` → `Duke`, `Wat` → `At`, `IPHONE` → `PHONE`), a deletion later
  in the word is no longer pruned (`thick` reaches `tick`,
  [#189](https://github.com/cmlarsen/writemorebetter/issues/189)), and
  `SpellOptions::prefix_length` is honoured instead of a fixed prefix of 2. `SpellOptions::freq_weight` (and the Python `freq_weight` property) is
  removed; `RULES_FORMAT_VERSION` is now 21, so rules binaries must be recompiled
  ([#147](https://github.com/cmlarsen/writemorebetter/issues/147)).
- **G-dropped words like `doin'` stay one token tagged as the `-ing` form, as in LanguageTool
  6.6.** An unknown word ending in `in'` (or `in’`) is tagged like its `-ing` form (`doin'` as
  `do/VBG`, `nothin'` as `nothing`), so the tokenizer no longer splits off the apostrophe, the
  spellchecker no longer flags `goin`, and `HOW_YOU_DOING` marks the whole `how you doin'`
  ([#162](https://github.com/cmlarsen/writemorebetter/issues/162)). This deliberately differs
  from LanguageTool 6.6 when the word without its apostrophe is itself a word: LanguageTool tags
  `sin’` as `sing` and `robin’` as `robe/VBG`, which swallows the closing quote of `‘a sin’` and
  raises a false unpaired-quote error; here those quotes stay tokens of their own. The
  unpaired-quote rule no longer treats the apostrophe of such a word as a closing quote, so
  `‘Doin’ fine,’ she said.` is not flagged. The tokenizer binary format version is now 15.
- **A g-dropped word right after an opening straight single quote stays whole.** In
  `'goin' home,' she said.`, `'Nothin',' she said.` and `'Comin' through!'` the opening `'` is
  now its own token and `goin'`, `Nothin'` and `Comin'` stay one word tagged as their `-ing`
  form, as they already were after a curly `‘`
  ([#188](https://github.com/cmlarsen/writemorebetter/issues/188)). This deliberately differs
  from LanguageTool 6.6, which splits `'goin'` at both apostrophes, flags `goin` as a misspelling
  and the closing quote as unpaired. A word whose stem is itself a word (`'sin'`, `'Robin'`) and
  an ordinary quoted word (`'hello'`) are still split as before.
- **Grammar-rule antipatterns immunize tokens, as in LanguageTool 6.6.** A pattern element can no
  longer match a token covered by one of the rule's antipatterns, and a `skip` continues past such
  a token to a later one, instead of the whole match being dropped when it overlaps an
  antipattern. `GERMAN_QUOTES` now flags the last `“` in `He said “a,“ b c“ d.`
  ([#134](https://github.com/cmlarsen/writemorebetter/issues/134)). An antipattern with a
  `<marker>` immunizes only the marked tokens, as LanguageTool does, instead of its whole match.
  Disambiguation rules keep the drop-on-overlap behaviour, and check time is unchanged. The
  tokenizer and rules binary format versions are now 14 and 20; recompile them.
- **U+2028 and other non-ASCII spaces after a period no longer end a sentence on their own.**
  LanguageTool's SRX rules are Java regexes, where `\s` and `\S` match only ASCII whitespace;
  they also matched U+2028, U+00A0, U+2003 and other Unicode spaces here, so `Good.` + U+2028 +
  `bad start` was two sentences instead of LanguageTool 6.6's one. U+00A0 still ends a sentence
  where LanguageTool's rules list it explicitly, as in `The end.` + U+00A0 + `The`. Sentence-level rules such as
  `UPPERCASE_SENTENCE_START` and `EN_A_VS_AN` now see the same sentences as LanguageTool across
  a soft line break ([#171](https://github.com/cmlarsen/writemorebetter/issues/171)). Rebuild the
  tokenizer binary to pick up the change; its format is unchanged.
- **Sentences are split as in LanguageTool 6.6.** The SRX sentencizer dropped or misplaced
  rules: trailing spaces in a pattern were trimmed (so `… and` started a new sentence), a capture
  group in a `beforebreak` moved the split (`Mr. Bush` and `Dr. Doe` were split after the
  title), overlapping matches were missed (`et. al. 2001`), a `break="no"` rule held at only one
  split per match (`“Wait!”  she cried` with two spaces was split after the first), and the one
  English rule with a look-ahead (`8 a.m. on`, `i. e.,`) failed to compile and was dropped. The
  splits now match LanguageTool's on all 23,595 English rule examples, and rule examples are checked one
  sentence at a time like LanguageTool's test harness
  ([#124](https://github.com/cmlarsen/writemorebetter/issues/124)). Rebuild the tokenizer
  binary to pick up the new rules.
- **A suggestion starting with a comma widens the match as in LanguageTool 6.6.** When any of a
  match's suggestions starts with a comma, the match now starts at the end of the previous token,
  not only when all of them do. `Tim make` offers ` makes`, ` made` and `, make` over ` make`
  instead of applying ` makes` to `make` and doubling the space. Grammar conformance rises from
  6017 to 6021 of 6038 rules ([#78](https://github.com/cmlarsen/writemorebetter/issues/78)).
- **The chunker's beam search follows OpenNLP 1.9.4, as LanguageTool 6.6 runs it.** The POS
  tagger and chunker no longer drop candidate tags below probability 0.1, the "advance every
  valid tag" fallback extends the best sequence first instead of an arbitrary one, a chunk can no
  longer continue (`I-`) after an `O` tag, tags tied with the last tag a beam keeps also advance,
  and the chunker's beam is 10 wide instead of 7. On the public texts this removes 36 of 43
  POS-tag and 42 of 58 chunk-tag differences from LanguageTool 6.6's chunker; the rest are
  near-ties that half-precision model weights cannot break. Six more rules pass their own
  examples, among them `TYPOS/VERB_APOSTROPHE_S/5` (`that just go's`)
  ([#123](https://github.com/cmlarsen/writemorebetter/issues/123)). The fuller beam costs about
  4% more instructions per check (see `docs/PERF.md`).
- **The chunker breaks near-ties like LanguageTool 6.6.** The chunker's tokenizer, POS tagger and
  chunk models now keep their parameters as `f32` instead of `bf16`, which was too coarse to
  separate sequences whose scores differ by less than 0.01 log-probability. On the public texts
  the chunker now matches LanguageTool 6.6's `EnglishChunker` on every internal token, POS tag
  and chunk tag (previously 1 sentence's tokens, 7 POS tags and 16 chunk tags differed), and
  `PUNCTUATION/OXFORD_NEW_CLAUSE/1` (`nginx, php, and mysql error logs and`) and
  `STYLE/SERIAL_COMMA_ON/4` (`special, fast and nice`) pass. Grammar conformance rises from 6046
  to 6048 rules ([#165](https://github.com/cmlarsen/writemorebetter/issues/165),
  [#154](https://github.com/cmlarsen/writemorebetter/issues/154)). The tokenizer binary grows
  by 1.9 MB (2.0 MB gzipped), peak RSS by 1.3–1.9 MiB and the tokenizer's loaded heap by
  1.0 MiB. The loaded parameter tables no longer keep the up-to-2× spare capacity that
  deserialization left them with. The tokenizer binary format version is now 18.
- **`scope="next"` exceptions see the readings LanguageTool 6.6 sees.** A token's next-token
  exception now checks only the next token's first reading (`UNKNOWN` for an untagged word), and
  is ignored (text exceptions included) when the previous pattern token has a `skip`; on a
  token with its own `skip` it still checks every reading, unless that token is the last
  pattern token. `This vehicle get’s used too much.` is now flagged
  (`TYPOS/VERB_APOSTROPHE_S/3`), `„Wait…“and` is flagged by `TYPOGRAPHY/GERMAN_QUOTES/3`, and
  `GRAMMAR/ONE_PLURAL/0` passes too
  ([#122](https://github.com/cmlarsen/writemorebetter/issues/122)). Tokenizer binaries move to
  format version 12 and rules binaries to 16 (`WordDataAtom::first_reading`); recompile them.
- **All-caps names with apostrophes get their proper-noun reading.** The multiword tagger now
  works like LanguageTool 6.6's English multiword chunker: it also matches each phrase in all
  caps and with its first letter capitalized, matches phrases the tokenizer split into adjacent
  tokens (`O'BRIEN` is `O` `'` `BRIEN`), and gives every token of a multi-token phrase only the
  phrase's reading. `O'BRIEN`, `O’BRIEN` and `L'ORÉAL` are now tagged `O'Brien/NNP`,
  `O’Brien/NNP` and `L'Oréal/NNP`, and `STANLEY KUBRICK` matches like `Stanley Kubrick`.
  Multiword readings are no longer protected from disambiguation rules, and `DM'D` stays one
  token like `CC'D` ([#116](https://github.com/cmlarsen/writemorebetter/issues/116)). Tokenizer
  binaries move to format version 13; recompile them.
- **The speller accepts all-caps words and restores case in suggestions.** An all-caps token
  (`THE END`, shouted `HELLO`) is no longer flagged when its lowercase or capitalized form is
  known, and a capitalized or all-caps token is searched case-insensitively with the candidates
  re-cased to match (`Teh` suggests `The`, `TEDD` suggests `TED`; mixed-case words such as
  `iPhone` keep their case), matching LanguageTool 6.6's case conversion
  ([#121](https://github.com/cmlarsen/writemorebetter/issues/121)). Applies to spelling
  suggestions and to `Speller::is_correct`/`suggest`.
- **User-dictionary words and LanguageTool's global name list are accepted in all caps.** A
  user-dictionary entry now also accepts its all-caps form and that form's possessive
  (`Zorvath` accepts `ZORVATH` and `ZORVATH’S`), so a shouted character name is not flagged
  ([#143](https://github.com/cmlarsen/writemorebetter/issues/143)). The English build data now
  includes LanguageTool's `spelling_global.txt`, whose entries are accepted as written, with the
  first letter capitalized and in all caps unless camel case, as LanguageTool 6.6's multiword
  chunker accepts them: `MCCARTHY`, `MCKINLEY’S`, `MCDONALD’S`, `YOUTUBE` and `POWERPOINT` are no
  longer flagged, while names missing from that list (`MCDONALD`, `MACARTHUR`) and camel-case
  ones (`EBAY`) still are. A mixed-case token such as `iPHONE` or `McKINLEY` is now searched
  case-insensitively, so it gets suggestions in the dictionary's case (`iPhone`, `McKinley`)
  ([#148](https://github.com/cmlarsen/writemorebetter/issues/148)). Rules binaries move to format
  version 19 (`Spell::phrases`); rebuild the data with `scripts/build-data.sh` to pick up the list.
- **Contractions, apostrophes and hyphens are handled as in LanguageTool 6.6.** Hyphens at either
  end of a word are now tokens of their own (`didn't-->` is `didn ' t - - >`, `Go-` is `Go -`),
  and a mixed-case contraction such as `don'T` is split at its apostrophe because the tagger,
  like LanguageTool's, only tries the lowercase form of a title-case or all-uppercase word. A
  `\1` reference in a suggestion uses the token's normalized text (`were’re` → `we're`, which
  `Rules::suggest` re-curls to `we’re` for curly input), an
  antipattern only blocks a match when it overlaps a matched token rather than a skipped one or
  the match's edge, `skip="-1"` is no longer capped at 20 tokens (an antipattern is now skipped
  when a literal it requires is absent from the sentence, so the uncapped skip does not slow
  long sentences down), and `suppress_misspelled`
  drops suggestions that could not be synthesized (no more `(anyone)` for `anyone’s`).
  `TYPOS/APOS_RE/1`, `TYPOS/WERE_RE/0`, `GRAMMAR/MISSING_APOSTROPHE_T/0`,
  `TYPOS/APOSTROPHE_PLURAL_/1`, `GRAMMAR/SINGED_CONTRACT/0`, `PUNCTUATION/DASH_RULE/0`,
  `PUNCTUATION/TWO_HYPHENS/0`, `PUNCTUATION/ENUMERATION_AND_DASHES/0`, `TYPOGRAPHY/ARROWS/0` and
  nine other rules now pass their LanguageTool examples. `RULES_FORMAT_VERSION` is now 13, so
  rules binaries must be rebuilt
  ([#77](https://github.com/cmlarsen/writemorebetter/issues/77),
  [#115](https://github.com/cmlarsen/writemorebetter/issues/115)).
- **Four more LanguageTool rules compile and pass.** The rule compiler now supports
  `<match no="…" postag="…"/>` inside a `<token>` (the referenced token's synthesized forms are
  matched, `GRAMMAR/REPEATED_VERBS/0` and `/1`), a quantifier on the first token of an `<or>`/`<and>`
  group (it applies to the whole group, `CONFUSED_WORDS/MULTIPLE_MULTIPLY/4`), and `raw_pos="yes"`
  on patterns that only test token text (`TYPOS/CONTRACTION_CASE/0`). The tokenizer now splits a
  mixed-case contraction such as `I'Ve` at the apostrophe, as LanguageTool 6.6 does, which also
  makes `TYPOS/APOS_RE/1` pass. Tokenizer binaries move to format version 11 and rules binaries to
  14, so binaries built by earlier versions must be recompiled
  ([#75](https://github.com/cmlarsen/writemorebetter/issues/75)).
- **Spell option changes were ignored while the variant stayed the same.** Changing `top_n`,
  `max_distance` or `freq_weight` through `Spell::options_mut` now applies to spelling suggestions
  and `Speller::suggest` without switching the variant
  ([#132](https://github.com/cmlarsen/writemorebetter/issues/132)).
- **All-caps proper nouns were tagged as unknown words.** The English tagger now looks up an
  all-uppercase word in title case when neither its own form nor its lowercase form is in the
  dictionary, as LanguageTool 6.6 does, so `SCHWARZWALD` is tagged `Schwarzwald/NNP` and
  `TYPOS/IM_I_M/0` passes ([#111](https://github.com/cmlarsen/writemorebetter/issues/111)).
- **A very long sentence was checked in quadratic time.** Three spots did O(sentence) work per
  match or per word: a rule with antipatterns re-ran every antipattern over the whole sentence for
  each candidate match, a disambiguation change re-scanned (and hashed) every token of the sentence
  for each match, and the chunker's beam search copied each hypothesis's full history at every
  word. Antipattern spans are now computed once per rule and sentence, a change only looks at the
  tokens inside its match, and the beam search keeps shared back-pointers. A 5,000-word sentence
  with no terminator now checks in ~0.8 s instead of ~6.6 s; output is unchanged
  ([#101](https://github.com/cmlarsen/writemorebetter/issues/101)).
- **Disambiguation rebuilt the whole sentence after every applied rule.** It now keeps one
  matchable copy of the sentence and refreshes only the tokens a rule changed. Finding a match
  group's tokens and a change's tokens now binary-searches the sentence instead of scanning every
  token per match, and the chunker no longer scans the whole sentence per word (its internal
  tokenizer) or per chunk (plural lookup and chunk assignment). Sequential tokenization of a
  7,000-token sentence drops from ~0.26 s to ~0.16 s, and of a ~60,000-token one from ~28 s to
  ~3.5 s; output is unchanged ([#125](https://github.com/cmlarsen/writemorebetter/issues/125)).
- **Edge cases in the rule compiler and tagger.** A possessive or counted optional regex assertion
  (`x$?+y`, `x$?{2}y`) now compiles to `xy` as Java matches it, instead of re-binding the
  quantifier to the previous atom; a bounded lookbehind with more than 256 variants now reports
  that cap instead of "unbounded length"; and the all-caps title-case lookup no longer runs for
  compound-split parts that disable lowercase lookup. No shipped rule or config hits these, so
  checking output is unchanged ([#117](https://github.com/cmlarsen/writemorebetter/issues/117)).

### Changed

- **The data binaries no longer depend on the build machine's architecture.** Every hashed
  collection in the tokenizer and rules binaries (the tagger's tag store, the spellchecker's maps
  and whitelist, the chunker's tag dictionary, builtin word lists and replacement tables, filter
  tables and text-engine group maps) is now written in sorted key order instead of hash-table
  iteration order, which differs between x86_64 and aarch64, and the chunker model's parameter
  arrays are laid out in sorted context order. Checking output is unchanged.
  `TOKENIZER_FORMAT_VERSION` is now 8 and `RULES_FORMAT_VERSION` 11, so binaries built by earlier
  versions must be rebuilt
  ([#103](https://github.com/cmlarsen/writemorebetter/issues/103)).
- The tokenizer binary format version is now 9 (`TaggerLangOptions` gained
  `title_case_all_uppercase`), so tokenizer binaries must be rebuilt with this engine.
- **Em dashes, en dashes and every other non-word character are now tokens of their own**, as in
  LanguageTool 6.6. The English tokenizer used to split on a short fixed list, so `Wait—no` or
  `{{cite` stayed one token; it now uses the word-character class of LanguageTool 6.6's English
  word tokenizer, which `build/make_build_dir.py` reads from the release and writes to
  `tokenizer_options.json` as `word_chars`. In fiction, a misspelling beside an em dash is now
  flagged on its own (`Patenice`, not `Patenice—always`), and rules such as a/an and
  simple-replace fire on words next to dashes. Nine more rules pass their LanguageTool examples,
  including `TYPOS/NON_STANDARD_WORD/0`, `TYPOS/WORD_CONTAINS_UNDERSCORE/0`,
  `TYPOS/LIGATURES/0` and `PUNCTUATION/DASH_RULE/4`. The spellchecker no longer flags a token
  made only of marks or format characters (a lone right-to-left mark, say), which can now stand
  alone. Tokenizer binaries move to format version 10 and rules binaries to 12
  ([#112](https://github.com/cmlarsen/writemorebetter/issues/112)).

# 0.7.1 (2026-10-12)

### Fixed

- **The Swift package failed to resolve from GitHub.** `Package.swift` read the released
  XCFramework URL and checksum from a sibling `release.json`, which SwiftPM cannot see when
  it compiles a remote dependency's manifest, so every consumer hit a `fatalError` at
  manifest load. The two values are now literals in `Package.swift`, rewritten by the
  release workflow, and the consumer check resolves the package as a git dependency
  ([#107](https://github.com/cmlarsen/writemorebetter/pull/107)).

# 0.7.0 (2026-10-12)

**First writemorebetter release.**

An embeddable English grammar, style and spelling checker for WASM, native Rust and Swift that runs
LanguageTool's open rules with no JVM. It ships the LanguageTool 6.6 rules only, and continues past
nlprule's inherited 0.6.4.

### Added

- **Swift bindings** (`swift/`): a UniFFI crate, an XCFramework build and a SwiftPM package, with
  `Checker`, UTF-16 suggestion offsets that map straight to `NSRange`, and a thread-safe user
  dictionary ([#95](https://github.com/cmlarsen/writemorebetter/pull/95)).
- **WASM package** (`wasm/`): `writemorebetter-wasm`, built for `wasm32-unknown-unknown` with the
  pure-Rust `regex-fancy` backend and no `include_bytes!`, so the LGPL data stays separate
  ([#28](https://github.com/cmlarsen/writemorebetter/pull/28),
  [#33](https://github.com/cmlarsen/writemorebetter/pull/33)).
- **A runtime user dictionary** for character names, invented places and coinages, in Rust, WASM and
  Swift ([#94](https://github.com/cmlarsen/writemorebetter/pull/94); closes
  [#86](https://github.com/cmlarsen/writemorebetter/issues/86)).
- **A `fiction` rule profile**, opt-in, that turns off the rules that misfire on fiction voice
  ([#29](https://github.com/cmlarsen/writemorebetter/pull/29)).
- **English builtin rules** for the Java-coded checks — word repeat, a/an, comma/parenthesis
  whitespace, unpaired brackets and quotes, uppercase sentence start, word coherency,
  wrong-word-in-context, and the simple-replace tables
  ([#10](https://github.com/cmlarsen/writemorebetter/pull/10),
  [#15](https://github.com/cmlarsen/writemorebetter/pull/15),
  [#19](https://github.com/cmlarsen/writemorebetter/pull/19),
  [#21](https://github.com/cmlarsen/writemorebetter/pull/21),
  [#22](https://github.com/cmlarsen/writemorebetter/pull/22),
  [#25](https://github.com/cmlarsen/writemorebetter/pull/25),
  [#26](https://github.com/cmlarsen/writemorebetter/pull/26),
  [#27](https://github.com/cmlarsen/writemorebetter/pull/27)); text-level builtins run once per
  paragraph ([#11](https://github.com/cmlarsen/writemorebetter/pull/11),
  [#12](https://github.com/cmlarsen/writemorebetter/pull/12)).
- **A public stand-in corpus** with planted errors, so recall can be measured without the private
  corpus ([#6](https://github.com/cmlarsen/writemorebetter/pull/6)).
- **A fuzzing harness and deterministic CI stress run** ([#99](https://github.com/cmlarsen/writemorebetter/pull/99)).
- **A reproducible data build** from pinned inputs, `scripts/build-data.sh`, used by CI and the
  release workflow alike ([#97](https://github.com/cmlarsen/writemorebetter/pull/97)), plus a
  self-describing binary-format header ([#37](https://github.com/cmlarsen/writemorebetter/pull/37)).
- **A documented release flow** (`docs/RELEASING.md`): one dispatched Release workflow builds the
  LGPL data and the Swift XCFramework, verifies a consumer against `release.json`, then commits,
  tags and publishes ([#105](https://github.com/cmlarsen/writemorebetter/pull/105)).

### Changed

- **The engine now runs LanguageTool 6.6 English rules**: a build directory generated from the pinned
  6.6 desktop release ([#17](https://github.com/cmlarsen/writemorebetter/pull/17),
  [#24](https://github.com/cmlarsen/writemorebetter/pull/24)), the 6.6 rule structure
  ([#31](https://github.com/cmlarsen/writemorebetter/pull/31)), contraction tokenization
  ([#35](https://github.com/cmlarsen/writemorebetter/pull/35)), inflection synthesis for
  `<match postag_replace>` ([#30](https://github.com/cmlarsen/writemorebetter/pull/30)), rule filters
  ([#38](https://github.com/cmlarsen/writemorebetter/pull/38)), two-digit backreferences, `+DT`/`+INDT`
  and `include_skipped` ([#40](https://github.com/cmlarsen/writemorebetter/pull/40)), no-suggestion
  rules and typed examples ([#34](https://github.com/cmlarsen/writemorebetter/pull/34)), and chunker
  and match-spacing fixes ([#42](https://github.com/cmlarsen/writemorebetter/pull/42)). Grammar
  conformance went from 3,749/4,366 = 85.9% (nlprule 0.6.4) to 5,977/6,098 = 98.0%.
- **Memory came down** from nlprule's ~145 MB loaded footprint to ~100 MB peak on a 150 KB check
  (speller on): the tagger FSTs are queried directly instead of inflated at load
  ([#8](https://github.com/cmlarsen/writemorebetter/pull/8)), the speller checks the shared FST
  ([#13](https://github.com/cmlarsen/writemorebetter/pull/13)), the multiword matcher is a contiguous
  NFA ([#16](https://github.com/cmlarsen/writemorebetter/pull/16)), the chunker model is smaller
  ([#18](https://github.com/cmlarsen/writemorebetter/pull/18)), `srx`'s one-pass DFAs are dropped
  ([#20](https://github.com/cmlarsen/writemorebetter/pull/20)), and rule-engine parts are interned
  ([#41](https://github.com/cmlarsen/writemorebetter/pull/41),
  [#43](https://github.com/cmlarsen/writemorebetter/pull/43)). See `docs/MEMORY.md`.
- **Check time came down** with a compile-time literal prefilter and sentence batching
  ([#45](https://github.com/cmlarsen/writemorebetter/pull/45)), a sustained run of retained
  instruction and memory wins ([#48](https://github.com/cmlarsen/writemorebetter/pull/48)–[#68](https://github.com/cmlarsen/writemorebetter/pull/68);
  ledger in `docs/PERF.md`), and chunker work
  ([#88](https://github.com/cmlarsen/writemorebetter/pull/88)–[#91](https://github.com/cmlarsen/writemorebetter/pull/91)).
- **The speller is integrated into `Rules`**: upstream nlprule's unmerged spellcheck branch was
  merged, with a UTF-8 dash fix and prose tests
  ([#1](https://github.com/cmlarsen/writemorebetter/pull/1)).
- **The data is LanguageTool 6.6 only**, packaged as separately loaded LGPL-2.1 files with a license
  and provenance and loaded per LGPL §6
  ([#97](https://github.com/cmlarsen/writemorebetter/pull/97)).
- Maps and sets serialized into the data binaries use a fixed hasher, so builds are byte-reproducible
  run to run on a platform ([#97](https://github.com/cmlarsen/writemorebetter/pull/97));
  cross-architecture determinism is [#103](https://github.com/cmlarsen/writemorebetter/issues/103).
- CI gates the LanguageTool conformance pass rate and public-corpus recall
  ([#5](https://github.com/cmlarsen/writemorebetter/pull/5),
  [#7](https://github.com/cmlarsen/writemorebetter/pull/7),
  [#32](https://github.com/cmlarsen/writemorebetter/pull/32)).
- The workspace crates move to 0.7.0.

### Fixed

- **A reachable panic** in `apply_suggestions` when a suggestion span started at zero at a later
  sentence's first token ([#99](https://github.com/cmlarsen/writemorebetter/pull/99)).
- **Pathological slowness**: a rule-matching quadratic on long token runs
  (`MatchGraph::fill_empty`, [#100](https://github.com/cmlarsen/writemorebetter/pull/100),
  [#102](https://github.com/cmlarsen/writemorebetter/pull/102)) and the speller's blowup on a single
  very long token ([#99](https://github.com/cmlarsen/writemorebetter/pull/99)). The residual
  long-sentence cost in disambiguation and the chunker is tracked in
  [#101](https://github.com/cmlarsen/writemorebetter/issues/101).
- **Curly apostrophes** are kept in suggestions instead of being normalized
  ([#36](https://github.com/cmlarsen/writemorebetter/pull/36)).
- LanguageTool 6.6 segmentation and builtin regressions — paragraph breaks, curly-apostrophe
  normalization and split contractions ([#98](https://github.com/cmlarsen/writemorebetter/pull/98),
  [#96](https://github.com/cmlarsen/writemorebetter/issues/96)).

### Removed

- The LanguageTool 5.2 rule set, its CI leg and its golden files
  ([#97](https://github.com/cmlarsen/writemorebetter/pull/97)).
- N-gram confusion rules are explicitly out of scope for the on-device engine
  ([#85](https://github.com/cmlarsen/writemorebetter/issues/85)).
- The Python bindings are not shipped; the upstream `python/` crate stays at its upstream state
  ([#93](https://github.com/cmlarsen/writemorebetter/issues/93)).

Entries below 0.7.0 are upstream nlprule's, reproduced from its own changelog.

# 0.6.4

## Internal improvements

- Decrease time it takes to load the `Tokenizer` by ~ 40% (#70).
- Tag lookup is backed by a vector instead of a hashmap now.

## Breaking changes

- The tagger now returns iterators over tags instead of allocating a vector.
- Remove `get_group_members` function.

# 0.6.3

## Fixes

- Fix a bug where calling `Rule::suggest` in parallel across threads would cause a panic (#68, thanks @drahnr!)

# 0.6.2

## Internal improvements

Speed up loading the `Tokenizer` by ~ 25% (#66).

# 0.6.1

## Fixes

- Build Python wheels in container for full manylinux2014 compliance, now works for glibc 2.17 and above (thanks @dvwright!)
- Speed up loading the `Tokenizer` by avoiding an allocation (thanks @drahnr!)

# 0.6.0

## Fixes

- Fix a significant bug where text with multiple sentences would sometimes cause an error if one of the latter sentences matches some pattern (#61, #63, thanks @drahnr!).

## Breaking changes

- Remove `multiword_tags` on tokens (now part of the regular tags).
- Make fields of the `Word` private and add getter methods.
- `Word` constructor is now called `new` instead of `new_with_tags`.

## New features

- Adds `as_str` convenience method to multiple structs (`WordId`, `PosId`, `Word`).

# 0.5.3

- Restore `FromIterator` and `IntoIterator` impl on `Rules` (#58, thanks @drahnr!)
- Add `Clone` derives on `Tokenizer` and `Rules` (and, accordingly, on their fields)

# 0.5.1

## Breaking changes

- Changes the focus from `Vec<Token>` to `Sentence` (#54). `pipe` and `sentencize` return iterators over `Sentence` / `IncompleteSentence` now.
- Removes the special `SENT_START` token (now only used internally). Each token corresponds to at least one character in the input text now.
- Makes the fields of `Token` and `IncompleteToken` private and adds getter methods (#54).
- `char_span` and `byte_span` are replaced by a `Span` struct which keeps track of char and byte indices at the same time (#54). To e.g. get the byte range, use `token.span().byte()`.
- Spans are relative to the input text now, not anymore to sentence boundaries (#53, thanks @drahnr!).

## New features

- The regex backend can now be chosen from Oniguruma or fancy-regex with the features `regex-onig` and `regex-fancy`. `regex-onig` is the default.
- nlprule now compiles to WebAssembly. WebAssembly support is guaranteed for future versions and tested in CI.
- A new selector API to select individual rules (details documented in `nlprule::rule::id`). For example:

```rust
use nlprule::{Tokenizer, Rules, rule::id::Category};
use std::convert::TryInto;

let mut rules = Rules::new("path/to/en_rules.bin")?;

// disable rules named "confusion_due_do" in category "confused_words"
rules
   .select_mut(
       &Category::new("confused_words")
           .join("confusion_due_do")
           .into(),
   )
   .for_each(|rule| rule.disable());

// disable all grammar rules
rules
   .select_mut(&Category::new("grammar").into())
   .for_each(|rule| rule.disable());

// a string syntax where slashes are the separator is also supported
rules
    .select_mut(&"confused_words/confusion_due_do".try_into()?)
    .for_each(|rule| rule.enable());
```

# 0.4.6

## Breaking changes

- `.validate()` in `nlprule-build` now returns a `Result<()>` to encourage calling it after `.postprocess()`.

## Fixes

- Fixes an error where `Cursor` position in `nlprule-build` was not reset appropriately.
- Use `fs_err` everywhere for better error messages.

# 0.4.5

## New features
- A `transform` function in `nlprule-build` to transform binaries immediately after acquiring them. Suited for e. g. compressing the binaries before caching them.

## Fixes
- Require `srx=^0.1.2` to include a patch for out of bounds access.

# 0.4.4

## Breaking changes

This is a patch release but there are some small breaking changes to the public API:
- `from_reader` and `new` methods of the `Tokenizer` and `Rules` now return an `nlprule::Error` instead of `bincode:Error`.
- `tag_store` and `word_store` methods of the `Tagger` are now private.

## New features

- The `nlprule-build` crate now has a `postprocess` method to allow e.g. compression of the produced binaries (#32, thanks @drahnr!).

## Internal improvements

- Newtypes for `PosIdInt` and `WordIdInt` to clarify use of ids in the tagger (#31).
- Newtype for indices into the match graph (`GraphId`). All graph ids are validated at build-time now (also fixed an error where invalid graph ids in the XML files were ignored through this) (#31).
- Reduced size of the English tokenizer through better serialization of the chunker. From 15MB (7.7MB gzipped) to 11MB (6.9MB gzipped).
- Reduce allocations through making more use of iterators internally (#30). Improves speed but there is no significant benchmark improvement on my machine.
- Improve error handling by propagating more errors in the `compile` module instead of panicking and better build-time validation. Reduces `unwrap`s from ~80 to ~40.

# 0.4.3

## Breaking changes

- `nlprule` does sentence segmentation internally now using [srx](https://github.com/bminixhofer/srx). The Python API has changed, removing the `SplitOn` class and the `*_sentence` methods:

```python
tokenizer = Tokenizer.load("en")
rules = Rules.load("en", tokenizer)

rules.correct("He wants that you send him an email.") # this takes an arbitrary text
```

- `new_from` is now called `from_reader` in the Rust API (thanks @drahnr!)
- `Token.text` and `IncompleteToken.text` are now called `Token.sentence` / `IncompleteToken.sentence` to avoid confusion with `Token.word.text`.
- `Tokenizer.tokenize` is now private. Use `Tokenizer.pipe` instead (also does sentence segmentation).

## New features
- Support for Spanish (*experimental*).
- A new multiword tagger improves tagging of e. g. named entities for English and Spanish.
- Adds the `nlprule-build` crate which makes using the correct binaries in Rust easier (thanks @drahnr for the suggestion and discussion!)
- Scripts and docs in `build/README.md` to make creating the nlprule build directories easier and more reproducible.
- Full support for LanguageTool *unifications*.
- Binary size of the `Tokenizer` improved a lot. Now roughly x6 smaller for German and x2 smaller for English.
- New iterator helpers for `Rules` (thanks @drahnr!)
- A method `.sentencize` on the `Tokenizer` which does only sentence segmentation and nothing else.

# 0.3.0

__BREAKING: `suggestion.text` is now more accurately called `suggestion.replacements`__

- Lots of speed improvements: NLPRule is now roughly 2.5x to 5x faster for German and English, respectively.

- Rules have more information in the public API now: See #5 

# 0.2.2

- Python 3.9 support (fixes #7)

# 0.2.1

- Fix precedence of Rule IDs over Rule Group IDs.

# 0.2.0

- Updated to LT version 5.2.
- Suggestions now have a `message` and `source` attribute (#5):

```python
suggestions = rules.suggest_sentence("She was not been here since Monday.")
for s in suggestions:
  print(s.start, s.end, s.text, s.source, s.message)

# prints:
# 4 16 ['was not', 'has not been'] WAS_BEEN.1 Did you mean was not or has not been?
```

- NLPRule is parallelized by default now. Parallelism can be turned off by setting the `NLPRULE_PARALLELISM` environment variable to false.
