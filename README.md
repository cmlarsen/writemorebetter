# writemorebetter

An embeddable grammar, style and spelling checker that runs LanguageTool's open English rules with no
JVM — in the browser and Electron (WASM), in native Rust, and in Swift (iOS/macOS). It was built for
fiction prose: Subplot, a writing app for fiction authors, is the first consumer.

LanguageTool is the source of the rules; writemorebetter is an independent project and is **not
affiliated with LanguageTool or LanguageTooler GmbH.**

**Status: 0.7.0, the first release.** English only, running the LanguageTool 6.6 rules. Measured
numbers, each from its source doc:

| Metric | Measured |
|---|---|
| Grammar conformance (rules passing LanguageTool's embedded examples, of all 6,098 `<rule>` elements) | 5,977 / 6,098 = 98.0% — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| Disambiguation conformance (rules passing their examples, of all 1,060 `<rule>` elements) | 1,012 / 1,060 = 95.5% — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| Public stand-in corpus recall | 55 / 60 — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| Private fiction corpus recall | 46 / 50 — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| Peak RSS, 150 KB of Moby-Dick, speller on | 100 MB — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| vs. a local LanguageTool server | ~2.5× faster on a full novel at about a seventh of the memory; comparable on short documents — [`docs/FINDINGS.md`](docs/FINDINGS.md) |
| Swift load / check | ~0.1 s load; ~110 ms per 1,000 words — [`swift/README.md`](swift/README.md) |

Conformance counts every `<rule>` element in LanguageTool's English XML, including rules that do not
compile; the CI gate counts only the rules the compiler can run, so it prints 5,977 / 6,032 for
grammar.

The engine is fast, low-resource and offline: no Java, no network, no server. See
[`docs/FINDINGS.md`](docs/FINDINGS.md) for the evaluation behind the project and
[`docs/PERF.md`](docs/PERF.md) / [`docs/PROFILE-BREAKDOWN.md`](docs/PROFILE-BREAKDOWN.md) for the
speed and memory work.

## Install and use

### Swift (iOS/macOS)

Add the package and the two LGPL data files ([below](#data-files-and-licensing)) as app resources:

```swift
// Package.swift
.package(url: "https://github.com/cmlarsen/writemorebetter", exact: "0.8.0"),
```

```swift
import WriteMoreBetter

// Tokenizer init deserializes two multi-megabyte binaries, so load one checker once — at
// startup, off the main thread — and share it.
let checker = try Checker(
    tokenizerPath: Bundle.main.path(forResource: "en_tokenizer", ofType: "bin")!,
    rulesPath: Bundle.main.path(forResource: "en_rules", ofType: "bin")!,
    spellVariant: "en_US",   // nil or "" leaves spelling off
    profile: "fiction"       // the default
)

// A check is not free (~110 ms per 1,000 words), so call it off the main thread. Pass one
// paragraph per call: editors like NSTextView separate paragraphs with a single newline, and a
// heading or a line without terminal punctuation would otherwise run into the next line.
for suggestion in checker.check(text: paragraph) {
    // start/end are UTF-16 offsets; suggestion.nsRange is ready for NSRange APIs.
    textStorage.addAttribute(.underlineStyle, value: 1, range: suggestion.nsRange)
}
```

Load failures throw `CheckError`; nothing panics across the boundary. A user dictionary for
character names and coinages is available: `checker.addWords(words:)`, `removeWords`, `setWords`,
`clearWords`, `words()`. It is thread-safe and applies to the next check. See
[`swift/README.md`](swift/README.md).

### WASM / JS

The package is not on npm; build it from the checkout (the module is ~1.7 MiB, the data stays out):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129
cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
wasm-bindgen --target nodejs --out-dir wasm/pkg \
  target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
```

```js
const { Checker } = require("./wasm/pkg/writemorebetter_wasm.js");

const checker = new Checker(tokenizerBytes, rulesBytes, "en_US", "fiction");
const suggestions = JSON.parse(checker.check(paragraph));
// start/end are character offsets (Unicode scalar values), not byte offsets.
```

The constructor copies the two byte buffers into wasm memory; read them, construct once and drop
them. `addWords` / `removeWords` / `setWords` / `words` work as in Swift. See
[`wasm/README.md`](wasm/README.md).

### Rust

The engine crate is `nlprule` (0.7.0), inherited from upstream; it is not published to crates.io, so
depend on the repository:

```toml
[dependencies]
nlprule = { git = "https://github.com/cmlarsen/writemorebetter", tag = "0.7.0" }
```

```rust
use nlprule::profile::Profile;
use nlprule::{Rules, Tokenizer};

let tokenizer = Tokenizer::new("storage/66/en_tokenizer.bin")?;
let mut rules = Rules::new("storage/66/en_rules.bin")?;
rules.apply_profile(&Profile::by_name("fiction").unwrap());
if let Ok(variant) = rules.spell().variant("en_US") {
    rules.spell_mut().options_mut().variant = Some(variant);
}

for suggestion in rules.suggest(paragraph, &tokenizer) {
    println!("{}: {}", suggestion.source(), suggestion.message());
}
```

That snippet is [`nlprule/examples/check.rs`](nlprule/examples/check.rs), which CI builds.
`rules.add_words([...])`, `remove_words`, `set_words`, `clear_words` and `words()` manage the user
dictionary. `Rules::suggest` is parallel by default (rayon); set `NLPRULE_PARALLELISM=off` for one
thread.

### Tokenizer analysis

`tokenizer.analyze(text)` returns owned sentences and tokens with original surface text, lemma/POS
readings, chunk tags, whitespace/punctuation flags, and byte, character and UTF-16 spans. Swift's
`checker.analyze(text:)` and WASM's `checker.analyze(text)` use the same loaded tokenizer and return
UTF-16 offsets. See [`docs/ANALYSIS.md`](docs/ANALYSIS.md) for the data contract and reference tests.

For whole-document prose reports, `writemorebetter_style::analyze_document(&tokenizer, &paragraphs)`
builds paragraph structure, quoted-dialogue spans and a lemma index. Swift and WASM expose it as
`checker.analyzeDocument`. See [`style/README.md`](style/README.md) for offsets and quotation heuristics.
Sentence-length and sentence-start reports are available through `reports::sentences::report` in Rust
and `checker.sentenceReports` in Swift/WASM, with caller-supplied baselines and separate dialogue statistics.

## Data files and licensing

The checker needs two compiled data files, `en_tokenizer.bin` and `en_rules.bin`, from the
`writemorebetter-data-<version>.zip` asset on the [GitHub release](https://github.com/cmlarsen/writemorebetter/releases).
The zip also carries `SHA256SUMS`, the LGPL-2.1 license text and a provenance README.

The data is compiled from LanguageTool 6.6 resources and is **LGPL-2.1**. Under LGPL §6 an app must
let the user replace it, so ship the two files as **separate files** (for example app bundle
resources) loaded at runtime — never link them into the application, and never inline them into engine
source. The engine code is MIT/Apache-2.0, at your option. The data zip is the only LGPL part.

Rebuild the data reproducibly from pinned inputs with [`scripts/build-data.sh`](scripts/build-data.sh);
CI and the release workflow both call it. See [`binaries/README.md`](binaries/README.md).

## Profiles and the user dictionary

`profile` names a set of disabled rules; `"default"` (or `""`) disables nothing, and `"fiction"`
turns off the rules that misfire on fiction voice — straight-to-curly quote and dash typography,
dialogue informality ("gonna", "You okay?"), comma-before-"and" style, and redundancy advice. See
[`docs/PROFILE.md`](docs/PROFILE.md).

The user dictionary (`add_words` and friends) whitelists character names, invented places and
coinages so the speller stops flagging them. Matching is case-sensitive except that a lowercase entry
also accepts its sentence-initial capitalized form; every entry accepts a possessive; curly and
straight apostrophes match each other; entries may be phrases or contain an internal apostrophe. The
caller owns persistence — the words are not written into a binary.

## Known limitations

- **N-gram confusion rules are out of scope on-device** (their/there, then/than, …): they need
  LanguageTool's multi-gigabyte n-gram index ([#85](https://github.com/cmlarsen/writemorebetter/issues/85)).
- **Rule coverage has gaps** — see the roadmap in
  [#87](https://github.com/cmlarsen/writemorebetter/issues/87).
- **The user dictionary does no Unicode normalization**, so entries must match the speller's
  tokenization byte-for-byte ([#86](https://github.com/cmlarsen/writemorebetter/issues/86)).
- **Python bindings are not shipped**; the upstream `python/` crate is unmaintained
  ([#93](https://github.com/cmlarsen/writemorebetter/issues/93)).

## Repository layout

- `nlprule/` — the engine crate: tokenizer, tagger, disambiguation, grammar rules, speller, builtins.
- `style/` — whole-document analysis for prose reports, separate from grammar suggestions.
- `build/` — the binary builder and the scripts that generate a LanguageTool build directory; `srx/`
  — vendored sentence segmentation.
- `swift/` — UniFFI bindings and the Swift package; `wasm/` — the WebAssembly package.
- `binaries/` — the LGPL data README and license; `scripts/build-data.sh` — the reproducible data build.
- `bench/` — the comparison harness and the public stand-in corpus; `docs/` — evaluation, perf and
  design notes.
- `python/` — upstream nlprule's Python crate, not built or shipped.

## Developing

Gates before review: `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`, the WASM build
(`cargo build --target wasm32-unknown-unknown --no-default-features --features regex-fancy -p nlprule`),
and `bench/tools/golden.sh --check` for the LT 6.6 output. CI also gates the LanguageTool conformance
pass rate (its embedded rule examples) and public-corpus recall, runs the Swift and fuzz jobs, and
compares each PR's speed and memory against its merge base on the Performance workflow.

`bench/corpus/` holds private, unpublished fiction; it is git-ignored and absent from clones. Use
`bench/public/` for anything that must be public. Releases are cut from `main` with the Release
workflow; see [`docs/RELEASING.md`](docs/RELEASING.md).

## License

Engine code: MIT or Apache-2.0, at your option. It includes nlprule, copyright Benjamin Minixhofer,
under the same licenses; see [`NOTICE`](NOTICE). Compiled rule and dictionary data derived from
LanguageTool are LGPL-2.1 and ship as separate files (see
[`binaries/README.md`](binaries/README.md)). writemorebetter is not affiliated with LanguageTool or
LanguageTooler GmbH.
