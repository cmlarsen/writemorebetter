# House-style rules

Issue #305 (part of #296) adds a **house-style rule engine**: the caller supplies rules in
[Vale](https://vale.sh)'s YAML rule format, and the report returns a span, rule name, level and
message for every match. This is the engine only. The style presets (Chicago fiction, AP, UK) and
the new Chicago rules in #305 come later, after #309 and #310, and will ship as rule bundles for
this engine.

Vale is MIT-licensed. Its docs define the schema; this crate's implementation is its own, with no Vale
code. Like every style report, house style **diagnoses and never rewrites**. A substitution
rule may name its preferred term, as Vale does, but the report returns that term exactly as the
rule lists it. It never adjusts case or builds replacement prose.

| Surface | House style |
|---|---|
| Rust | `house_style::report(&Document, &HouseStyleRules, &HouseStyleOptions)` |
| Rules | `HouseStyleRules::parse(text)` for a bundle; `HouseStyleRule::parse(name, text)` for one Vale file; `HouseStyleRules::new(Vec<HouseStyleRule>)` |
| Retained Rust | `document.house_style(options_json, Some(rules_text))` |
| WASM | `document.houseStyle(optionsJSON?, rulesText)` |
| Swift | `document.houseStyle(rules:optionsJson:) -> HouseStyleReport` |

The rules are a required input, like diction's table. If they are missing, the report returns an
`invalidRules` error instead of an empty report. The retained methods cache the last parsed rule
text, so calling again with the same rules does not re-parse or recompile them. House style works
on compact documents (`includeTokenDetails: false`), because it matches the text, not token readings.

## Options

| Option | Default | Meaning |
|---|---|---|
| `skipDialogue` | `false` | Leave quoted speech out entirely: no matches in it, and it never counts toward consistency or occurrence |

Options use camelCase JSON with snake_case aliases, and unknown keys are an `invalidOptions` error.
Rule files keep Vale's own lowercase keys (`ignorecase`, `nonword`) so that a Vale file reads
unchanged.

## Rule schema

Four Vale rule types (`extends`) are supported:

| `extends` | Keys | Flags |
|---|---|---|
| `existence` | `tokens`, `exceptions` | each match of any token |
| `substitution` | `swap` (pattern → preferred term), `exceptions` | each match of a key, with the listed term as `replacement` |
| `consistency` | `either` (pattern ↔ pattern) | when both variants appear in one scope, every match of the one used second |
| `occurrence` | `token`, `max`, `min` | a scope with more than `max` or fewer than `min` matches of `token` |

Every rule also takes `message` (required), `level` (`suggestion`, the default, `warning` or
`error`), `scope` (`text`, the default, `paragraph` or `sentence`), `ignorecase`, `nonword`,
`limit` (at most this many matches per rule), `link` (passed through to each match) and `name`.

- **Patterns are regular expressions**, as in Vale. Rust's `regex` syntax is close to Go's RE2,
  which Vale uses. Neither supports lookaround. The `regex` crate is already a dependency of
  `nlprule`, so it adds no new crate, though WASM links more of its code (see Measurements).
- **Messages** substitute `%s` like Vale's: the matched text for existence and consistency, and the
  preferred term then the matched text for substitution (`"Use '%s' instead of '%s'."`). `%%` is a
  literal `%`, and a `%s` with no argument left stays as it is.
- **Substitution** skips a match that is already one of the listed terms. A value may list several
  with `|` (`e-?mail: email|e-mail`), and `replacement` keeps the whole value.
- **Consistency** keeps the variant that appears first in the scope. The match's `replacement` is
  that variant's pattern, as the rule lists it.
- **Occurrence** reports `count`. When there are too many matches, the span runs from the first match
  to the last. When there are too few, it covers the whole scope (sentence, paragraph, or the first
  to last paragraph for `text`).
- **Scopes.** Matches never cross a paragraph break. `sentence` also keeps them inside a sentence,
  and sets the unit for consistency and occurrence: the whole text, each paragraph, or each sentence.
  Headings that the document pass skips are not searched.
- **Accepted but unused Vale keys**: `description`, `action`, `vocab` and `code`. The engine never
  rewrites text, has no vocabularies, and reads plain prose. Any other key is an error, including
  Vale keys that this engine does not implement (`raw`, `append`, and scopes such as `heading`
  or `code`), so a typo can't silently disable a rule.
- **Strict validation.** A key that another rule type uses (such as `swap` on an existence rule) is
  an error. So are a missing required key, an unsupported `extends`, level or scope, an invalid
  regex, a duplicate rule name and a duplicate YAML key.

## Bundles

`HouseStyleRules::parse` reads YAML or JSON. JSON is tried first, and YAML second. A Vale style is a
directory of rule files named after the rule. Since this API takes no directory, a bundle is one
text in any of these shapes:

```yaml
# 1. One rule per YAML document, each with `name` (concatenated Vale files).
name: House.Toward
extends: substitution
message: "Use '%s' instead of '%s'."
ignorecase: true
swap:
  towards: toward
---
name: House.Hedging
extends: existence
message: "Consider cutting '%s'."
tokens: [a bit, sort of]
```

```yaml
# 2. A map from name to rule; a list of rules with `name` works too, bare or under `rules`.
rules:
  House.Hedging:
    extends: existence
    message: "Consider cutting '%s'."
    tokens: [a bit, sort of]
```

`HouseStyleRules::to_json()` writes `{"rules": [...]}`, and `parse` reads it back as an equal bundle.
That makes it the canonical form for storing an imported style. The fixture
`style/tests/fixtures/house-style/bundle.yml` round-trips in tests.

YAML uses [`saphyr-parser`](https://crates.io/crates/saphyr-parser) 0.2 (MIT OR Apache-2.0, pure
Rust, depends only on `arraydeque` and `thiserror`). The engine turns its event stream into JSON
values and reads them with the same `serde` schema as JSON input. `serde_yaml` is unmaintained, and
accepting JSON only would make every imported Vale style need a conversion step. Plain scalars
follow the YAML 1.2 core schema: `true`/`false`, null and integers resolve, and floats stay text.
Any scalar can fill a text field, so `tokens: [24, ok]` works. Anchors parse; aliases are rejected,
which keeps a small rule file from expanding exponentially.

## Matching prose

- **Word boundaries.** Unless a rule sets `nonword: true`, a match must start and end at word
  boundaries. Letters, digits, `_`, and an apostrophe between two letters count as word characters,
  so `don` does not match in *don't* and `clock` does not match in *o'clock*. A word followed by a
  clitic (*'s*, *'d*, *'ll*, *'re*, *'ve*, *'m*) still ends there, so `advisor` matches in
  *advisor's*. Hyphens, em dashes, ellipses and quotation marks are boundaries. After a rejected
  match, the search retries one character later, so a shorter alternative that starts inside it is
  still found.
- **Curly quotes and whitespace.** Before matching, `’`, `‘` and `ʼ` fold to `'` and every whitespace
  character (U+2028, tab, no-break space, a hard-wrapped newline) folds to a space, in both text and
  patterns. A rule written with straight apostrophes and spaces then matches fiction typography.
  The fold maps one character to one character, so spans map back exactly. Spans are reported in
  bytes, chars and UTF-16 natively, and as UTF-16 `{start, end}` in the bindings.
- **Overlaps.** Within one rule, overlapping matches keep the longest one, and on a tie the one
  listed first (`a lot of` over `a lot` over `lot`). Separate rules report their matches
  independently, as in Vale.
- **Modes and names.** Each match carries `mode` (narration or dialogue, from the shared document
  definition). A match that touches a skipped name is dropped.
- **Speed.** Each rule's patterns are combined into one `RegexSet`, which tells in a single pass which
  patterns can match in a paragraph. Only those patterns then search it for positions.

## Result

```json
{"rules": 4, "matches": [{"rule": "House.Toward", "extends": "substitution", "level": "error", "message": "Use 'toward' instead of 'Towards'.", "span": {"start": 1, "end": 8}, "mode": "dialogue", "replacement": "toward", "count": null, "link": null}], "groups": {"all": {"suggestions": 0, "warnings": 1, "errors": 2, "words": 10}, "narration": {}, "dialogue": {}}}
```

`matches` are sorted by span start, longer spans first, then by rule order. `groups` count matches
per level and give the word counts for narration and dialogue. These counts agree with the other
reports' `words`.

## Measurements (2026-10-20)

Measurements were taken on `bench/public/moby150k.txt` (149,091 bytes) through the Node WASM release
build, with the document already built and the mean of 5 runs after a warm-up call:

| Rules | Time | Matches |
|---|---|---|
| fixture bundle (4 rules) | 7.1 ms | 79 |
| 200-token existence + 100-key substitution | 9.1 ms (136 ms without the `RegexSet` prefilter) | 1,294 |

Parsing, compiling and running the fixture bundle cold takes 27 ms.

The release WASM module (`wasm32-unknown-unknown`, before `wasm-opt`) grows from 5,628,245 to
5,989,974 bytes, +361,729 (+6.4%); gzipped, from 1,491,863 to 1,574,279, +82,416 (+5.5%). YAML
accounts for 108,518 bytes (28,468 gzipped) of that: a build with the YAML path removed was
5,881,456 bytes. The rest is the report itself and the `regex` matching and `RegexSet` code it
links. `nlprule`'s rules use `fancy-regex` in the WASM build, so linking `regex` there is not free.
JSON-only input would save the YAML share.
