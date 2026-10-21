# writemorebetter-wasm

A minimal WebAssembly package for the writemorebetter checker (English), built for
`wasm32-unknown-unknown` with the pure-Rust `regex-fancy` backend so it runs in browsers, Electron
and Node without a JVM.

The compiled rule and dictionary binaries (`en_tokenizer.bin`, `en_rules.bin`) are LGPL-2.1 resources
derived from LanguageTool's data. They are **not** embedded in this crate — there is no
`include_bytes!` — so the engine code stays MIT/Apache-2.0. The JavaScript caller reads the two files
and passes their bytes to the constructor.

## API

```js
const { Checker } = require("./wasm/pkg/writemorebetter_wasm.js");

const checker = new Checker(tokenizerBytes, rulesBytes, "en_US", "fiction"); // spell_variant and profile optional/undefined
const suggestions = JSON.parse(checker.check("She was not been here since Monday."));
// [{ start: 4, end: 16, start16: 4, end16: 16, source: "GRAMMAR/WAS_BEEN/1",
//    message: "Did you mean was not or has not been?",
//    replacements: ["was not", "has not been"] }]
```

Each suggestion carries two offset pairs. `start16` / `end16` count **UTF-16 code units** into the
input — the same convention as `analyze()` and `analyzeDocument()` — so `text.slice(start16, end16)`
returns the flagged text even after emoji. `start` / `end` count Unicode scalar values (characters),
not bytes; that is the **older** convention, kept so existing callers do not break, and to be
deprecated later. Prefer `start16` / `end16`. Pass `undefined`
(or `""`) as `spell_variant` to leave spelling off. Pass `undefined` (or `""`) as `profile` to
disable nothing; `"fiction"` applies the fiction profile (see `docs/PROFILE.md`) and any other
name is an error.

`checker.enableRules(["PLAIN_ENGLISH", "STYLE/PASSIVE_VOICE"])` turns on rules that are off by
default, on top of the profile, and returns how many it enabled; an unknown selector throws and
enables nothing. See "Enabling rules" in `docs/PROFILE.md` for the selectors.

### User dictionary

Add words the app knows (character names, invented places, coinages) so the speller stops
flagging them. The caller owns persistence: the words are not written into any binary, and
`Checker` does not store them anywhere else. Changes apply to the next `check`.

```js
checker.addWords(["Xylophonix", "Port Royal", "Ngũgĩ"]); // -> number newly added
checker.removeWords(["Xylophonix"]);                     // -> number removed
checker.setWords(["Zoë", "Ngũgĩ"]);                      // -> number now set
JSON.parse(checker.words());                             // -> ["Ngũgĩ", "Zoë"]
```

Matching rules (see `Spell::add_words` in `nlprule/src/spell/mod.rs`):

- Matching is case-sensitive, except that a lowercase entry also accepts its sentence-initial
  capitalized form. An added capitalized name does **not** make the lowercase form valid.
- Every entry accepts a possessive (`Xylophonix's` / `Xylophonix’s`), and curly and straight
  apostrophes match each other.
- An entry may be a whitespace-separated phrase (`"Port Royal"`) or contain an internal
  apostrophe (`"Zar'khan"`).
- Single-word entries are also spelling-suggestion candidates, so a near miss can suggest them.

### Mother tongue

Writers whose first language is not English can opt in to the false-friend rules for speakers of
their language, compiled from LanguageTool's `false-friends.xml`. They are off until a mother
tongue is selected, and only that language's rules run.

```js
JSON.parse(checker.motherTongues()); // -> ["ca", "de", "es", "fr", "ga", "gl", "it", ...]
checker.setMotherTongue("de");       // flags e.g. "actual" for German speakers
checker.setMotherTongue(undefined);  // back to the default: no false-friend rules
```

Run `node wasm/test.mjs` after building `wasm/pkg` to exercise the API.

The two byte buffers are consumed by the constructor: wasm-bindgen copies them into wasm linear
memory, and the module does not need the JS copies afterwards. **Do not keep `tokenizerBytes` /
`rulesBytes` alive** — read them, construct once, and drop them. The copies were ~21 MB of the
process RSS when retained, and the wasm heap cannot give back the memory it used to read them
(see `memory.mjs` and REPORT.md).

## Tokenizer analysis

`JSON.parse(checker.analyze(text))` returns sentences and tagged tokens with original text,
paired lemma/POS readings, chunk tags, and `is_whitespace` / `is_punctuation` flags. Sentence
and token `start` / exclusive `end` count **UTF-16 code units** into the original input:
use `text.slice(start, end)` to recover their text, including after emoji. `check()` exposes the same
convention in its `start16` / `end16` fields, while its `start` / `end` stay **Unicode scalar**
offsets (the older convention, to be deprecated). Both methods share
the checker's one loaded tokenizer. See [the analysis contract](../docs/ANALYSIS.md).

## Document analysis

```js
const document = JSON.parse(checker.analyzeDocument(text, JSON.stringify({
  skip_names: ["Zoë", "O’Brien"],
})));
const allWords = JSON.parse(checker.analyzeDocument(text));
```

The optional second argument is a JSON `DocumentOptions` object containing `skip_names`.
The result contains `paragraphs` with nested `sentences` and `tokens`, `dialogue` spans,
`lemma_index` mapping each lemma to `{ paragraph, sentence, token }` positions, and
`word_count`. Positions are zero-based. All paragraph, sentence, token, and dialogue
`start` / exclusive `end` offsets count UTF-16 code units in the original text.
Tokens retain readings, chunks, whitespace and punctuation flags and add `is_word` and
`is_skipped`. Skipped names remain in the document but are excluded from `word_count`
and `lemma_index`; matching is case-sensitive, accepts ALL CAPS, and normalizes curly apostrophes. A lowercase
entry also accepts its first character capitalized; `Will` preserves the modal `will`. Adjacent
possessive clitics of skipped names are excluded as well.
The document pass shares the checker's loaded tokenizer with `analyze()` and `check()`.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # must match the wasm-bindgen crate version
cargo install wasm-opt                             # optional; from binaryen
```

## Build the data binaries (LGPL, kept out of the engine)

```sh
mkdir -p data storage
curl -sSL -o data/en.zip https://f000.backblazeb2.com/file/nlprule/en.zip
unzip -q data/en.zip -d data
mkdir -p data/en/builtin
cp -r build/builtin-data/en/. data/en/builtin/
cargo run --release --all-features --bin compile -- \
  --build-dir data/en --tokenizer-out storage/en_tokenizer.bin --rules-out storage/en_rules.bin
```

## Build the wasm package

```sh
cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
wasm-bindgen --target nodejs --out-dir wasm/pkg \
  target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
# optional, smaller module in place:
wasm-opt -Oz -o wasm/pkg/writemorebetter_wasm_bg.wasm wasm/pkg/writemorebetter_wasm_bg.wasm
```

`wasm/pkg/` is git-ignored. `.cargo/config.toml` sets `-C opt-level=z` for
`wasm32-unknown-unknown`: the module is built for footprint, not throughput, and V8's TurboFan
tier-up turns the bigger `opt-level=3` functions into tens of MB of JIT code at load time (see
REPORT.md). `wasm-opt -Oz` runs after and cannot undo that inlining, so it is a compile-time flag.

## Run the benchmarks

```sh
# first 150000 bytes of Moby-Dick
curl -sSL -o storage/pg2701.txt https://www.gutenberg.org/cache/epub/2701/pg2701.txt
head -c 150000 storage/pg2701.txt > storage/moby150k.txt

node wasm/bench.mjs          # construction time, per-check time, memory, with speller off and on
node wasm/memory.mjs         # where the RSS goes (Node baseline, JS copies, wasm linear memory)
node wasm/compare.mjs        # suggestion sets vs the native onig build (bench/tools/wmb-probe)
```

`bench.mjs` writes to `storage/moby150k.txt`'s sibling `bench/public/story1.txt`; set `BWB_RUNS=1` for
a quick single-run pass. `compare.mjs` needs the release `wmb-probe`:

```sh
cargo build --release --manifest-path bench/tools/wmb-probe/Cargo.toml
```

Document options default to full token details and one paragraph per nonempty line. For hard-wrapped
imports use `hardWrapped: true` (WASM also accepts `hard_wrapped`); blank lines then separate
paragraphs. Set `includeTokenDetails: false` (WASM: `include_token_details`) to reduce memory: WASM
omits token `readings`/`chunks` keys and Swift returns empty arrays, while counts and the index remain
unchanged. WASM accepts camelCase/snake_case option names and rejects unknown keys. See
[the document conventions](../docs/STYLE-DOCUMENT.md) and
[novel-length time/memory measurements](../docs/STYLE-DOCUMENT-BENCHMARKS.md).

## Style reports

Create one retained analysis and call any reports independently:

```javascript
const document = checker.styleDocument(text, '{"skipNames":["Mara"]}');
const length = JSON.parse(document.sentenceLength());
const starts = JSON.parse(document.sentenceStarts());
const tags = JSON.parse(document.dialogueTags());
const cliches = JSON.parse(document.cliches());
const stats = JSON.parse(document.stats());
const echoes = JSON.parse(document.echoes());
const phrases = JSON.parse(document.repeatedPhrases());
const senses = JSON.parse(document.sensoryBalance());
const diction = JSON.parse(document.diction()); // bundled table; diction(undefined, tableText) overrides it
const overused = JSON.parse(document.overusedWords());
const transitions = JSON.parse(document.transitions());
const pronouns = JSON.parse(document.pronounStarts());
const adverbs = JSON.parse(document.adverbs());
document.free();
```

Methods take optional options JSON; `diction` also takes an optional word-to-concreteness table
(JSON object or TSV text) that replaces the bundled one; baseline-aware methods also take baseline JSON (omitted:
bundled; JSON object: override; `"none"`: no comparison). Results contain UTF-16 spans. Each
sentence report owns its sentence rows. Result types ship in the generated package (source: `style-results.d.ts`):

```typescript
import type { SentenceLengthReport } from "./pkg/writemorebetter_wasm";
const length: SentenceLengthReport = JSON.parse(document.sentenceLength());
```

Construction respects compact-document options. Echoes, phrases, overused words, pronoun starts, dialogue tags, adverbs,
starts, sensory balance and diction require token details and throw on compact documents. Reports run on the retained analysis; construct a new
object for changed text/options. See [the per-report API](../docs/STYLE-REPORTS.md#independent-reports-shared-analysis)
for options, result JSON examples, baselines and Subplot’s recommended default set and opt-in lenses.

A retained moby150k document (149,091 input bytes) holds about **11–12 MB of WASM linear memory**
in the reviewed measurement. Retain the chapter on screen and `free()` its document on navigation;
Swift callers should drop their retained reference. WASM frees space for reuse by later documents,
while its linear memory high-water mark does not shrink back to the OS. Avoid caching every chapter.

Transitions and pronoun starts are opt-in descriptive lenses. See [definitions, options and public
measurements](../docs/STYLE-TRANSITIONS.md); neither has a baseline argument yet.

The transitions lens also supports compact documents because it uses token text and position.

Passive voice (`passive`) reports passives per 100 sentences per mode, with spans, and takes a baseline
argument like the other banded reports. It has no default band yet (no comparison); `"publicStandIn"`
opts in to a provisional band from public text. See [STYLE-PASSIVE.md](../docs/STYLE-PASSIVE.md).
