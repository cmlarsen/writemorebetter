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
// [{ start: 4, end: 16, source: "GRAMMAR/WAS_BEEN/1",
//    message: "Did you mean was not or has not been?",
//    replacements: ["was not", "has not been"] }]
```

`start`/`end` are character offsets (Unicode scalar values), not byte offsets. Pass `undefined`
(or `""`) as `spell_variant` to leave spelling off. Pass `undefined` (or `""`) as `profile` to
disable nothing; `"fiction"` applies the fiction profile (see `docs/PROFILE.md`) and any other
name is an error.

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

`JSON.parse(checker.analyze(text))` returns sentences and tokens with original text, paired
lemma/POS readings, chunk tags, and `is_whitespace` / `is_punctuation` flags. Sentence and token
`start` / `end` count UTF-16 code units; use `text.slice(start, end)` to recover the surface.
This differs from `check()`'s existing Unicode scalar offsets. Analysis reuses the loaded
tokenizer without running grammar or spelling checks. See [the analysis contract](../docs/ANALYSIS.md).

`JSON.parse(checker.analyzeDocument(paragraphs))` adds paragraph boundaries, quoted-dialogue
spans and a lemma index. All offsets index the returned `text` (paragraphs joined with one LF).
See [the document contract](../style/README.md).

`JSON.parse(checker.sentenceReports(paragraphs, undefined))` returns sentence-length and
sentence-start reports. Pass an optional JSON string to configure thresholds and independent
narration/dialogue baselines. Report ranges retain all three offset conventions: use
`text.slice(span.utf16.start, span.utf16.end)` on the LF-joined input paragraphs.
See [sentence-report options and conventions](../style/README.md#sentence-reports-244).

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
