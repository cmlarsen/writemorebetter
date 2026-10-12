# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions after the
inherited 0.6.4 track this project's own release line.

## 0.7.0 — first writemorebetter release - Unreleased

The first writemorebetter release: an embeddable English grammar, style and spelling checker for
WASM, native Rust and Swift that runs LanguageTool's open rules with no JVM. It ships the
LanguageTool 6.6 rules only, and continues past nlprule's inherited 0.6.4.

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
