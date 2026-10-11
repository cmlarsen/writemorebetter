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

`wasm/pkg/` is git-ignored.

## Run the benchmarks

```sh
# first 150000 bytes of Moby-Dick
curl -sSL -o storage/pg2701.txt https://www.gutenberg.org/cache/epub/2701/pg2701.txt
head -c 150000 storage/pg2701.txt > storage/moby150k.txt

node wasm/bench.mjs          # construction time, per-check time, memory, with speller off and on
node wasm/compare.mjs        # suggestion sets vs the native onig build (bench/tools/wmb-probe)
```

`bench.mjs` writes to `storage/moby150k.txt`'s sibling `bench/public/story1.txt`; set `BWB_RUNS=1` for
a quick single-run pass. `compare.mjs` needs the release `wmb-probe`:

```sh
cargo build --release --manifest-path bench/tools/wmb-probe/Cargo.toml
```
