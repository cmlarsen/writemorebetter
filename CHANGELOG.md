# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions after the
inherited 0.6.4 track this project's own release line. Each version is a top-level
`# <version>` heading; the Release workflow copies that section verbatim into the GitHub
release notes (see [`docs/RELEASING.md`](docs/RELEASING.md)).

# Unreleased

### Added

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

### Fixed

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
  version is now 26 and the tokenizer binary format version is now 17.

- **A `scope="next"` exception after an optional token depends on whether the token matched.**
  The exception checks the next word's first reading only when the last token that matched has
  no `skip`, decided per match instead of once when the rule is compiled
  ([#151](https://github.com/cmlarsen/writemorebetter/issues/151)).

- **Repeated tokens follow LanguageTool 6.6's `min` and `max`.** `min="2"` without `max`
  matches exactly two words instead of up to 20, `max="-1"` has no limit instead of 20, and a
  word an antipattern immunized no longer trips a `scope="next"` exception inside a `skip`
  window ([#214](https://github.com/cmlarsen/writemorebetter/pull/214)).

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
