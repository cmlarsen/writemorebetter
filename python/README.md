# writemorebetter for Python

English grammar, spelling and fiction style checking without a JVM. writemorebetter runs
LanguageTool's open English rules (6.6) in Rust; this package is its Python binding, built with
PyO3 and maturin. It was built for fiction prose: dialogue, curly quotes, em dashes and deliberate
fragments.

LanguageTool is the source of the rules; writemorebetter is an independent project and is **not
affiliated with LanguageTool or LanguageTooler GmbH.**

## Install

The package is not on PyPI yet. Build a wheel from the checkout (needs a Rust toolchain):

```sh
pip install maturin
cd python
maturin build --release        # wheel in ../target/wheels/
pip install ../target/wheels/writemorebetter-*.whl
```

or `maturin develop --release` inside a virtualenv. CI also builds abi3 wheels (CPython 3.9+) for
Linux, macOS and Windows on every PR; download them from the run's `python-wheels-*` artifacts.

### Data files

The wheel holds engine code only (MIT OR Apache-2.0). The checker also needs two compiled data
files, `en_tokenizer.bin` and `en_rules.bin`, from the `writemorebetter-data-<version>.zip` asset on
the [GitHub release](https://github.com/cmlarsen/writemorebetter/releases), or built with
`scripts/build-data.sh`. Use the data release whose version matches the package's.

The data is compiled from LanguageTool resources and is **LGPL-2.1**, so it ships separately from
the wheel: keep the two files as separate, user-replaceable files loaded at runtime. Do not
vendor them into a wheel of your own.

## Use

```python
from writemorebetter import Checker

# Loading deserializes two multi-megabyte files: build one checker and reuse it.
checker = Checker.from_dir("writemorebetter-data", spell_variant="en_US")
# or Checker("…/en_tokenizer.bin", "…/en_rules.bin", spell_variant="en_US", profile="fiction")

text = "“I’ll recieve it tomorrow,” she said. She was went home."
for s in checker.check(text):
    print(s.start, s.end, repr(text[s.start:s.end]), s.source, s.message, s.replacements)
```

`check` returns `Suggestion` objects with `start`, `end` (exclusive), `source` (the rule, such as
`GRAMMAR/WAS_BEEN/1` or `SPELLCHECK/SINGLE`), `message` and `replacements`. Pass one paragraph per
call where you can: a heading or a line without terminal punctuation otherwise runs into the
next line's sentence.

### Offsets

Every offset is a **Python string index** (a Unicode code point), so `text[s.start:s.end]` is the
flagged span. That holds after curly quotes, em dashes and ellipses (several UTF-8 bytes, one code
point), after emoji (one code point, two UTF-16 units) and across U+2028. This differs from the
other bindings on purpose: Swift and the WASM `start16`/`end16` fields count UTF-16 units, and
Rust spans count bytes or code points. Analysis, document and style-report spans are code points
too.

### Constructor

`Checker(tokenizer_path, rules_path, *, spell_variant=None, profile="fiction")`

- `spell_variant` selects a speller dictionary (`"en_US"`, `"en_GB"`, …); `None` or `""` leaves
  spelling off.
- `profile`: `"fiction"` (the default) turns off rules that misfire on fiction voice — straight-to-
  curly typography, dialogue informality, comma style, redundancy advice; `None`, `""` or
  `"default"` disables nothing. See
  [`docs/PROFILE.md`](https://github.com/cmlarsen/writemorebetter/blob/main/docs/PROFILE.md).
  Python and Swift default to `"fiction"`; the WASM constructor defaults to no profile when
  `profile` is omitted, so pass it explicitly when results must match across bindings.
- Errors: `OSError` (`FileNotFoundError`, …) when a file can't be read, `WriteMoreBetterError`
  when it is corrupt or from another version, `ValueError` for an unknown profile or variant.

### User dictionary

```python
checker.add_words(["Zorbath", "Port Royal"])   # returns how many were new
checker.remove_words(["Port Royal"])
checker.set_words(["Zorbath", "Quillon"])      # replaces the whole dictionary
checker.clear_words()
checker.words()                                # sorted list
```

For character names, invented places and coinages. Entries may be phrases or contain an internal
apostrophe; curly and straight apostrophes match each other. Matching is case-sensitive except
that a lowercase entry also accepts its sentence-initial capitalized form, and every entry accepts
a possessive. Changes apply to the next `check`; persisting the list is up to you.

### Rules

- `checker.enable_rules(["PLAIN_ENGLISH", "STYLE/PASSIVE_VOICE"])` turns on rules that are off by
  default (LanguageTool's picky rules) by profile selector; an unknown selector raises
  `ValueError` and enables nothing.
- `checker.set_mother_tongue("de")` runs the false-friend rules for that language;
  `checker.mother_tongues()` lists them.
- `checker.apply_preset("chicago-fiction")` applies a style preset's grammar settings.

### Analysis and style reports

```python
sentences = checker.analyze(text)           # sentences → tokens with lemma/POS readings
document = checker.analyze_document(text)   # paragraphs, dialogue spans, lemma index

style = checker.style_document(chapter)     # analyze once, then run any reports on it
style.stats()
style.adverbs()
style.echoes({"window": 50})
style.dialogue_tags(baseline="none")
style.house_style(rules=open("house.yml").read())
style.preset("chicago-fiction")
```

Reports return plain `dict`s with the same records as the WASM and Swift bindings (typed in
[`wasm/style-results.d.ts`](https://github.com/cmlarsen/writemorebetter/blob/main/wasm/style-results.d.ts)),
with spans as `{"start", "end"}` code-point ranges. Options are a `dict` (or JSON text). Any bad
report input raises `StyleError`, a `ValueError` whose `kind` (e.g. `"invalidOptions"`),
`report` (camelCase, or `None`) and `message` are always set.

The reports are `echoes`, `repeated_phrases`, `cliches`, `dialogue_tags`, `sentence_length`,
`sentence_starts`, `transitions`, `pronoun_starts`, `adverbs`, `sensory_balance`, `pacing`,
`diction`, `overused_words`, `consistency`, `readability`, `passive`, `clarity`, `house_style`,
`regional_spelling`, `emotion_tells`, `preset_checks`, `preset`, `alliteration`, `vague_words`,
`hidden_verbs`, `voices` and `tense_shift`. `voices` lists quoted and tagged unquoted speech and thought, quoted
terms and spans you mark with the document option `markedSpans`, whose offsets default to code
points in Python (`markedSpanUnits`: `"char"`; `"utf16"` and `"byte"` also work); invalid spans
are skipped and listed in `ignoredMarkedSpans`. `tense_shift` (opt-in, narration only) flags
runs of narration sentences that leave their scene's tense; pass `{"tense": "past"}` to declare
it instead of inferring it. Module functions: `style_preset_names()`, `load_style_preset(name)`,
`parse_style_preset(json)` and `title_case(headings)`. See
[`docs/STYLE-REPORTS.md`](https://github.com/cmlarsen/writemorebetter/blob/main/docs/STYLE-REPORTS.md).

### Threads

A `Checker` is thread-safe and every method releases the GIL while it works or waits, so threads
can check different paragraphs in parallel against one checker. The user-dictionary and rule
methods take a write lock, so they wait for checks already running (without blocking other
Python threads) and apply to the next check.

## Testing

```sh
cd python
maturin develop --release
pip install pytest
WMB_DATA_DIR=/path/to/writemorebetter-data pytest tests
```

Without `WMB_DATA_DIR` the tests look for the data in the checkout's `storage/` and `dist/data/`.

## License

Engine code: MIT OR Apache-2.0, at your option. The wheel bundles style data tables under their
own licenses (CMU Pronouncing Dictionary, CC BY word norms), listed in `NOTICE` with the license
texts under `licenses/` in the wheel's metadata. The rule and dictionary data are LGPL-2.1 and
are not part of the wheel.
