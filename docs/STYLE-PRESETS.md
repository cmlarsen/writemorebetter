# Style presets

Issue #305 (part of #296) adds **style presets**: one named, versioned JSON file that drives

1. the grammar checker's rule opt-in: a [rule profile](PROFILE.md), the rules to turn off and on by
   [selector](PROFILE.md#enabling-rules), and the speller variant;
2. the [consistency](STYLE-CONSISTENCY.md) report's preferred forms;
3. [house-style](STYLE-HOUSE-STYLE.md) rules in Vale's format, as a rule map or an embedded bundle;
4. the built-in **preset checks**, house rules that the regex schema can't express cleanly
   (numbers, quote punctuation, possessives, *a.m.*/*p.m.* and *Mom*/*Dad*).

Three presets are built in: **`chicago-fiction`** (the default), **`ap`** and **`uk`**. A user preset
can extend a built-in one and override any part of it. Like every style report, a preset only
diagnoses. A match's `replacement` is at most a fixed term that the rule or check lists (`toward`,
`a.m.`, `mom`, a period moved past a quotation mark). Nothing builds replacement prose.

The presets describe common editorial conventions in our own words. `chicago-fiction` is modeled on
how US book publishers apply *The Chicago Manual of Style* to fiction, and `ap` on the *AP
Stylebook*. Neither copies text or tables from those books, and neither is affiliated with or
endorsed by their publishers.

| Surface | Call |
|---|---|
| Rust | `StylePreset::builtin(name)`, `StylePreset::parse(json)`, `StylePreset::load(name_or_json)`, `StylePreset::names()`, `preset.to_json()` |
| Rust, grammar | `preset.apply_to_rules(&mut rules) -> Result<usize, StyleError>` |
| Rust, reports | `preset.report(&document) -> PresetReport`; `document.preset(Some(name_or_json))` on a `StyleDocument` |
| Rust, checks alone | `preset_checks::report(&Document, &PresetChecks)`; `document.preset_checks(options_json)` |
| WASM | `stylePresetNames()`, `loadStylePreset(name)`, `parseStylePreset(json)`, `checker.applyPreset(nameOrJson)`, `document.preset(nameOrJson?)`, `document.presetChecks(optionsJSON?)` |
| Swift | `StylePresets.names`, `StylePresets.load(_:)`, `StylePresets.parse(_:)`, `StylePresets.summary(_:)`, `checker.applyPreset(preset:)`, `document.preset(_:) -> PresetReport`, `document.presetChecks(optionsJson:) -> PresetChecksReport` |

An app keeps the author's choice as a name or a preset file and passes the same string to both sides:

```js
const checker = new Checker(tokenizerBytes, rulesBytes, "en_US");
checker.applyPreset(preset);            // "chicago-fiction", "uk", or a user preset's JSON
const report = JSON.parse(checker.styleDocument(text).preset(preset));
// report.consistency, report.houseStyle, report.checks
```

`apply_to_rules` checks every selector and the speller variant before it changes anything, so an
unknown one is an `invalidPreset` error that leaves the rules as they were. A profile cannot be undone
on a loaded rule set, so apply a preset to freshly loaded rules (in Swift, a checker built with
`profile: ""`; the preset brings its own profile). To switch presets, load the rules again. The
report side has no such state: `document.preset(...)` can be called with any preset at any time, and
the last parsed preset is cached, so a repeated call doesn't reparse or recompile its rules.

## File format

```json
{
  "format": 1,
  "name": "my-house",
  "version": "1.2.0",
  "description": "Our imprint's fiction style.",
  "extends": "chicago-fiction",
  "grammar": {
    "profile": "fiction",
    "rules": {"STYLE/SERIAL_COMMA_ON": true, "PLAIN_ENGLISH": true, "STYLE/GONNA": false},
    "spellVariant": "en_US"
  },
  "consistency": {"minimumMinority": 1, "preferred": {"dashes": "emClosed", "ellipses": "character"}},
  "houseStyle": {
    "options": {"skipDialogue": false},
    "rules": {"House.Toward": {"extends": "substitution", "message": "Use '%s' instead of '%s'.", "swap": {"towards": "toward"}}},
    "vale": "name: House.Hedging\nextends: existence\nmessage: \"Consider cutting '%s'.\"\ntokens: [sort of]\n"
  },
  "checks": {
    "numbers": {"spellOutThrough": 100, "agesAsNumerals": false, "matchLarger": true},
    "quotePunctuation": "inside",
    "possessives": "apostropheS",
    "timeAbbreviations": "dotted",
    "familyTitles": true
  }
}
```

| Key | Meaning |
|---|---|
| `format` | Required. The file format; this engine reads `1` and rejects anything else |
| `name` | Required, non-empty. Never inherited: a file that `extends` a built-in names itself |
| `version` | The preset's own version, any string. The built-ins start at `1.0.0` and change it whenever their settings change. Not inherited |
| `description` | Free text. Not inherited |
| `extends` | A built-in preset to start from (see below) |
| `grammar.profile` | `"default"` or `"fiction"`, applied first |
| `grammar.rules` | Selector → `true` (enable) or `false` (disable). Every `false` applies before every `true`, so `{"STYLE": false, "STYLE/ELLIPSIS": true}` keeps `ELLIPSIS` on. A disabled selector may use the builtin case form (`BUILTIN/WHITESPACE_RULE/INDENTATION`) |
| `grammar.spellVariant` | A speller dictionary such as `"en_GB"`; unset keeps the checker's |
| `consistency` | The consistency report's options: `minimumMinority` and `preferred` |
| `houseStyle.options` | The house-style report's options (`skipDialogue`) |
| `houseStyle.rules` | Rule name → Vale rule, with Vale's lowercase keys |
| `houseStyle.vale` | A Vale bundle as YAML or JSON text, in any shape `HouseStyleRules::parse` reads. Its rules run after `rules`; a name in both is an error |
| `checks` | The preset checks below; each is off when absent |

Validation is strict, as for every report's options. An unknown key at any level, a wrong type, an
unknown profile, preferred form, check value or `extends`, a format other than 1, and any error the
house-style engine would report (an unknown Vale key, a bad regex, a duplicate rule name) are
`StyleError::InvalidPreset` errors, and the message names the section. Apps can call
`parseStylePreset` to validate a user's file when it is saved, before it is used.

### Extending a preset

A file with `extends` is applied over that built-in preset as a JSON merge patch
([RFC 7396](https://www.rfc-editor.org/rfc/rfc7396)): objects merge key by key, any other value
replaces the inherited one, and `null` removes it. Since grammar rules, preferred forms, house-style
rules and their `swap`/`either` maps are all objects, a user preset overrides exactly what it names:

```json
{
  "format": 1,
  "name": "my-uk",
  "extends": "uk",
  "grammar": {"rules": {"STYLE/SERIAL_COMMA_ON": true}},
  "consistency": {"preferred": {"dashes": null}},
  "houseStyle": {"rules": {"Preset.OK": {"level": "error", "swap": {"o\\.k\\.": null}}}},
  "checks": {"numbers": null}
}
```

This turns the serial-comma rule back on, drops the dash preference, makes one inherited house rule
an error and removes one of its pairs, and turns the number check off. `null` with no base just means
"unset". `extends` names a built-in preset. The built-in `ap` and `uk` presets themselves extend
`chicago-fiction`, so `extends` chains, up to four levels.

`to_json()` (and `loadStylePreset`/`parseStylePreset`) writes the resolved preset: everything it
inherited, with no `extends`. Parsing that output gives the same preset, so it is the form to store.

## The built-in presets

| | `chicago-fiction` (default) | `ap` | `uk` |
|---|---|---|---|
| Profile | `fiction` | inherited | inherited |
| Grammar rules | `STYLE/SERIAL_COMMA_ON` | `STYLE/SERIAL_COMMA_OFF` | neither |
| Speller | the checker's | the checker's | `en_GB` |
| Preferred forms | curly quotes and apostrophes, closed em dash | spaced em dash | spaced en dash |
| House style | `Preset.Toward` (*towards* → *toward*, *afterwards* → *afterward*), `Preset.OK` (*okay*, *Ok*, *o.k.* → *OK*), `Preset.Meridiem` (*A.M.*/*P.M.* → *a.m.*/*p.m.*) | inherited | without `Preset.Toward` |
| Numbers | spell out zero through one hundred | one through nine; ages stay numerals; no same-sentence rule | inherited |
| Quote punctuation | inside | inherited | outside for quoted words and phrases |
| Possessives | *James’s* | *James’* | inherited |
| *a.m.* / *p.m.* | dotted | inherited | inherited |
| *Mom* / *Dad* | on | inherited | inherited |

No ellipsis form is preferred: book publishers set the character or spaced periods at typesetting,
and the consistency report still flags a manuscript that mixes them.

## Preset checks

`PresetChecks` is also the options object of `presetChecks`, which runs the checks without a preset.
Every match carries `check`, `message`, `span` (UTF-16 `{start, end}` in the bindings), `mode`
(narration or dialogue) and `replacement`. `counts` splits the matches by mode. Matches that touch a
skipped name are dropped. The checks read the text with curly and straight quotes and apostrophes,
em dashes, ellipses and U+2028 line separators alike, and work on compact documents. Family titles
and possessives use token readings when the document has them, and fall back to word lists and
position without them (see each check).

### `numbers`

Flags a whole number from zero through `spellOutThrough` written as a numeral, including ordinals
(`3rd`) and the number in a compound (`5-year-old`). It is left alone when it is:

- a **date**: next to a capitalized month name (`May 5`, `12 June`, `the 4th of July`), or written
  with `/` or `-` between numbers;
- a **time**: `5:30`, or followed by *a.m.*, *pm* and the like;
- an **address or label**: followed by capitalized words ending in a street name (`12 Baker
  Street`), or after a word such as *Chapter*, *page*, *Room*, *Apt.*, *No.*, *Route*, *Flight*;
- a **year or era**: 1000–2199 never counts as a larger number, and `AD 70` and `70 BC` are skipped;
- a **percentage, measurement or amount**: `45%`, `45 percent`, `5 km`, `$20`, `20°`;
- a **decimal or grouped number**: `2.5`, `0.75`, `1,000`;
- **part of a code or range**: `4x4`, `3D`, `B-52`, `3–2`, `5'10"`, a leading zero (`07`), a
  decade (`the 90s`, `’90s`);
- a **list marker** at the start of a paragraph (`1.`, `2)`);
- **listed after** a label, date or era: a numeral joined to one of those by *and*, *or*, *&*, *to*
  or *through* (`chapters 3 and 4`, `May 5 and 6`, `Rooms 12 and 14`), or by a comma inside a list
  (`May 5, 6, and 7`). A comma alone does not carry the reading into a plain number (`On May 5, 3
  people came` is flagged), but a conjunction does: `Chapter 2 and 4 cats` is read as two labels.

With `matchLarger` (the default), a sentence that has a numeral for a larger quantity keeps its
small ones as numerals too (`Of 250 guests, 12 stayed`), so the numbers it compares read alike. With
`agesAsNumerals`, ages stay numerals (`5 years old`, `a 6-year-old`, `aged 7`), as news styles set
them. `ap` sets `spellOutThrough: 9`, `agesAsNumerals: true` and `matchLarger: false`.

The message names the limit. The check offers no replacement: spelling a number out is the author's
wording.

### `quotePunctuation`

- `inside` (US): a period or comma directly after a closing quotation mark is flagged, and the
  replacement is the two marks swapped (`”.` → `.”`). This applies to speech, quoted words and
  scare quotes alike.
- `outside` (UK): speech keeps its period or comma inside the quotation marks, as British fiction
  sets dialogue. A quoted word or phrase that the document does not read as speech, and that opens
  with a lowercase letter (a fragment, not a quoted sentence), is flagged when the period or comma
  sits inside (`‘rubbish.’` → `‘rubbish’.`).

Closing quotation marks are read by direction (`”`) or by context (straight quotes, `’`), so
apostrophes (`don’t`, `dogs’`, `’em`, `rock ’n’ roll`), inch and foot marks (`5'10"`) and an
ellipsis after a quote (`”...`) are never flagged.

Known limitation of `outside`: a quoted word nested inside speech (`‘He called it “rubbish.”’`) is
not checked. The document's speech spans cover the outer quotation only, so the inner one has no
span to read as non-speech.

### `possessives`

- `apostropheS`: a capitalized word ending in *s*, followed by an apostrophe alone and then a word
  (`James’ car`), is flagged at the apostrophe with `’s` (or `'s`) as the replacement.
- `apostropheOnly`: the same names followed by `’s` (`James’s car`) are flagged, with the apostrophe
  alone as the replacement. A `’s` that stands for *is* or *has* (`Chris’s going`, `Ross's been
  gone`) is skipped: when its readings are not possessive, or when the next word is one that only
  follows *is* or *has* (`been`, `going`, `gone`, `got`, `here`, `there`, `not`, an article, …).

Plural names are left alone: after *the* (`the Joneses’ house`), and words whose readings are plural
nouns only (`Americans’`). Without readings (a compact document), a capitalized word that opens its
sentence is skipped instead, since it may be a plural common noun capitalized by position
(`Parents’ evening`). An apostrophe that closes a single quotation (`‘Ask Charles’ she said`) is
not a possessive. Lowercase nouns (`the boss’ office`) are not checked, because their plurals (`the
dogs’ bowls`) look the same.

### `timeAbbreviations`

After a time (`5`, `10:30`, with or without a space, including a no-break space), `dotted` flags
`AM`, `am`, `pm`, `a.m` and the like and lists `a.m.` or `p.m.`; `undotted` flags dotted forms and lists
`am` or `pm`. In dotted mode an undotted form's sentence-ending period is part of the span (`5 AM.` →
`5 a.m.`). In undotted mode a final period stays out of the span when it ends the sentence (the end
of the paragraph, or a space and then a capital or opening quote): `at 5 p.m. Then` → `at 5 pm.
Then`. Mid-sentence the span takes both periods, so `at 9 a.m. on Friday` → `at 9 am on Friday`. The
uppercase dotted forms `A.M.` and `P.M.` are left to the `Preset.Meridiem` house rule in dotted mode,
which flags them anywhere.

### `familyTitles`

*Mom*, *Dad*, *Mum*, *Grandma* and the other words in the check's table are capitalized when used as
a name and lowercase after a possessive or article:

- A capitalized form directly after *my*, *your*, *the*, *a*, *this* or a possessive (`Jake’s Mom`)
  is flagged with the lowercase form. *Mother*, *Grandmother* and *Grandfather* are checked in this
  direction too. *Father* is not, because of titles such as *Father Brown*.
- A lowercase *mom*, *dad*, *mum*, *grandma* and the like is flagged with the capitalized form
  when it is used as a name: at the start of a sentence or after a comma or quotation mark (`Thanks,
  mom`), or after a verb or preposition (`I asked mom`, `with dad`). It is left alone after a
  determiner, an adjective or noun (`a single dad`, `soccer mom`), before a noun it modifies (`dad
  jokes`), and in a list that shares a determiner (`my mom and dad`, `my mom, dad, and I`, `her
  sister, dad and aunt`). An apostrophe attached to a plural (`the kids' mom`, `the kids’ mom`) is a
  possessive; a straight quote with a space before it (`'dad, wait'`) opens a quotation. Without
  token readings, only a short list of verbs and prepositions marks name use.

## Grammar settings and recall

Measured 2026-10-21 with this branch's rules binary (format 32), `wmb-probe` with each preset's
profile, rules and speller, against main's defaults (`PROFILE=default`, en_US speller):

| Setting | Private corpus recall (50) | Private flags | Public recall (60) | Public flags (5 stories) |
|---|---|---|---|---|
| main's defaults | 46 | 254 | 55 | 98 |
| `fiction` profile alone | 46 | 176 | 55 | 85 |
| `chicago-fiction` | 46 | 178 | 55 | 85 |
| `ap` | 46 | 180 | 55 | 85 |
| `uk` | 46 | 214 | 55 | 97 |

Recall does not drop for any preset. The serial-comma rules add 2 (`chicago-fiction`) and 4 (`ap`)
flags on the private corpus and none on the public stories. `uk` adds British-spelling flags on
American text, as it should.

## Style-report flags on `bench/public`

`cargo test -p writemorebetter-style --test presets -- --ignored --nocapture public_counts` prints
these. Stories use line mode and `moby150k` hard-wrapped mode, as in the other style measurements.
"Non-preferred" sums the consistency report's `nonPreferred` counts.

| Preset | Text | Preset checks | House style | Non-preferred |
|---|---|---|---|---|
| `chicago-fiction` | story1 | 1 (quote punctuation) | 0 | 0 |
| | story2–5 | 0 | 0 | 0 |
| | moby150k | 1 (numbers) | 28 (`Preset.Toward`) | 1 |
| `ap` | story1 | 1 (quote punctuation) | 0 | 2 |
| | story2 | 0 | 0 | 1 |
| | story4 | 3 (possessives) | 0 | 0 |
| | story3, story5 | 0 | 0 | 0 |
| | moby150k | 3 (possessives) | 28 (`Preset.Toward`) | 204 |
| `uk` | story1 | 0 | 0 | 2 |
| | story2 | 0 | 0 | 1 |
| | story3–5 | 0 | 0 | 0 |
| | moby150k | 2 (numbers, quote punctuation) | 0 | 204 |

Every preset-check match was read by hand and is correct for its preset: a comma after a closing
scare quote (Chicago), apostrophe-s possessives of names ending in *s* (AP), a numeral for a small
number in prose, and a period inside the quotation marks of a quoted non-speech phrase (UK). The 28
house-style matches are Melville's *towards*. The non-preferred counts are the closed em dashes the
public texts use, under the `ap` and `uk` dash preferences, and one straight single quote in
`moby150k`.
