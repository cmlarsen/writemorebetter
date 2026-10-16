# writemorebetter-swift

Swift bindings for the writemorebetter checker (English), generated with [UniFFI](https://mozilla.github.io/uniffi-rs/)
proc-macros. The crate builds a dynamic library; `scripts/build-xcframework.sh` wraps the Apple slices in
`WriteMoreBetterFFI.framework` bundles inside `dist/WriteMoreBetterFFI.xcframework` and regenerates the Swift
sources under `swift/Sources/WriteMoreBetter/Generated/`. The package supports macOS 12 and iOS 16.

The compiled rule and dictionary binaries (`en_tokenizer.bin`, `en_rules.bin`) are LGPL-2.1 resources derived
from LanguageTool's data. They are **not** part of the XCFramework — the crate never embeds them — so the app
ships them as resources and passes their paths to the constructor.

## API

```swift
import WriteMoreBetter

let checker = try Checker(                                   // profile defaults to "fiction"
    tokenizerPath: Bundle.main.path(forResource: "en_tokenizer", ofType: "bin")!,
    rulesPath: Bundle.main.path(forResource: "en_rules", ofType: "bin")!,
    spellVariant: "en_US"                                    // nil or "" leaves spelling off
)
for suggestion in checker.check(text: editor.text) {
    // suggestion.start / suggestion.end are UTF-16 offsets; suggestion.nsRange is ready for NSRange APIs.
    print(suggestion.source, suggestion.message, suggestion.replacements)
}
```

Loading deserializes both binaries (about 100 ms and 90 MB of resident memory); `Checker` is `Send + Sync`, so
load one instance at startup off the main thread and share it. `check` and every other call are safe from any
thread, but are not free (~110 ms per 1000 words on an M-series Mac), so run them off the main thread too.

Failures throw `CheckError`: `TokenizerFile`/`RulesFile` (read from disk), `Tokenizer`/`Rules` (deserialize),
`UnknownProfile`, `UnknownSpellVariant`. Nothing panics across the boundary; a Rust panic surfaces as a Swift
error.

## User dictionary

Add the words the app knows — character names, invented places, coinages — so the speller stops flagging them.
The caller owns persistence: nothing is written into a binary. The rule set sits behind a read/write lock, so
these calls can run while other threads are checking.

```swift
checker.addWords(words: ["Xylophonix", "Port Royal", "Ngũgĩ"])  // -> number newly added
checker.removeWords(words: ["Xylophonix"])                     // -> number removed
checker.setWords(words: ["Zoë", "Ngũgĩ"])                      // -> number now set
checker.clearWords()
checker.words()                                               // -> ["Ngũgĩ", "Zoë"]
```

Matching is case-sensitive except that a lowercase entry also accepts its sentence-initial capitalized form
(an added capitalized name does not make the lowercase form valid). Every entry accepts a possessive, curly and
straight apostrophes match each other, and an entry may be a whitespace-separated phrase or contain an internal
apostrophe.

## Mother tongue

False-friend rules (compiled from LanguageTool's `false-friends.xml`) flag English words that a speaker of
another language is likely to misuse, such as "handy" for a German speaker. They run only while a mother tongue
is selected; none is selected by default.

```swift
checker.motherTongues()                     // -> ["ca", "de", "es", "fr", "ga", "gl", "it", ...]
checker.setMotherTongue(motherTongue: "de") // flags e.g. "handy" and "actual" for German speakers
checker.setMotherTongue(motherTongue: nil)  // back to the default: no false-friend rules (so does "")
```

A region suffix is ignored (`"de-DE"` selects `"de"`). The change applies to the next `check`, and like the
dictionary calls it is safe while other threads are checking.

## Building the XCFramework

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
./scripts/build-xcframework.sh
```

The script builds `aarch64-apple-darwin` + `x86_64-apple-darwin` (lipo'd into one `macos-arm64_x86_64` slice),
`aarch64-apple-ios`, and `aarch64-apple-ios-sim` + `x86_64-apple-ios` (one `ios-arm64_x86_64-simulator` slice),
all with nlprule's native `regex-onig` backend and with a macOS 12.0 / iOS 16.0 minimum OS version, the
minimums `Package.swift` declares, pinned per target so the host proc-macros stay on the host default. It then
runs `uniffi-bindgen-swift` in library mode and calls `xcodebuild -create-xcframework`. Everything lands in
`dist/` (git-ignored) plus the generated Swift sources, which are committed so the package always has a source
target.

If you ran an earlier checkout and hit [#258](https://github.com/cmlarsen/writemorebetter/issues/258) here, run `cargo clean` once first: cargo reuses the stale proc-macro dylibs.

Each slice is a dynamic framework (`Headers/`, `Modules/module.modulemap`, `Info.plist`; macOS uses the
versioned bundle layout) whose binary exports only the UniFFI C functions. That lets an app link it next to
another Rust library: a Rust static library carries its own copy of `std` and of shared crates, so two of them
in one app collide on duplicate symbols, and two static XCFrameworks with a top-level `module.modulemap` collide
in Xcode's `include/` directory. An app embeds the framework the way it embeds any SwiftPM binary framework.

The build never runs Xcode's `strip`: the linker drops local symbols and debug info itself (`-Wl,-x -Wl,-S`,
with rustc's own `strip` off), so an app's own strip during archiving finds nothing left to remove. The `strip` in Xcode 27 removes the `.llvm.<hash>` suffixes
LLVM gives promoted local functions, which merged two different functions into one symbol in the old static
library ([#238](https://github.com/cmlarsen/writemorebetter/issues/238)), and it leaves a dylib that `ld`
rejects with "mis-aligned LINKEDIT string pool".

## The Swift package

`Package.swift` at the repository root publishes the `WriteMoreBetter` library over two targets: the Swift
target (generated bindings plus `Suggestion+NSRange.swift`) and the `WriteMoreBetterFFI` binary target.

Resolution picks the binary target when the manifest is evaluated:

- `dist/WriteMoreBetterFFI.xcframework` exists — a checkout you just ran the build script in. The binary target
  is path-based, so `.package(path:)` works offline and against unreleased changes.
- It does not exist — a released tag, where `dist/` is git-ignored. The binary target uses the `url` and
  `checksum` literals in `Package.swift`.

Those two values have to be literals in `Package.swift` rather than a sibling file: SwiftPM compiles a remote
dependency's manifest from a virtual filesystem where nothing but the manifest itself is readable, so a manifest
that reads a neighbouring file with `FileManager` fails at manifest load for every consumer. The release workflow
rewrites the literals, commits, and only then tags and uploads the asset, so a tag's literals always match the
asset next to it.

SwiftPM caches evaluated manifests by manifest contents, not by the filesystem. A checkout that was resolved
before `scripts/build-xcframework.sh` ran keeps the released-URL branch until `swift package purge-cache`
(or a fresh clone) refreshes it.

## Tokenizer analysis

`checker.analyze(text: text)` returns owned sentences and tokens with original text, paired
lemma/POS readings, chunk tags and `isWhitespace` / `isPunctuation` flags. Sentence and token
`start` / exclusive `end` are absolute UTF-16 offsets, matching `Suggestion` and `NSRange`.
It reuses the checker's loaded tokenizer without running grammar or spelling rules.
See [the analysis contract](../docs/ANALYSIS.md).

## Tests

`swift/Tests/WriteMoreBetterTests` loads the LT 6.6 binaries from `storage/66/` (the repository's
per-rule-set storage convention; override with `WMB_DATA_DIR`), checks a sentence with a known error,
asserts UTF-16 offsets on text that mixes an emoji, curly quotes, an em dash, a combining mark and
U+2028, flags a German false friend only after `setMotherTongue`, edits the user dictionary from one
thread while four others check and list it, and calls `check` concurrently from eight threads. Run them
with:

```sh
cargo run --release --all-features --bin compile -- \
    --build-dir data/en-lt66 --tokenizer-out storage/66/en_tokenizer.bin --rules-out storage/66/en_rules.bin
./scripts/build-xcframework.sh
swift test
```

`.github/workflows/swift.yml` does exactly this on a macOS runner, generating `data/en-lt66` the same way the
6.6 job in `ci.yml` does and failing if the committed bindings drift from the freshly generated ones.

## Document analysis

```swift
let document = checker.analyzeDocument(
    text: text,
    options: DocumentOptions(skipNames: ["Zoë", "O’Brien"])
)
let allWords = checker.analyzeDocument(text: text, options: nil)
```

`Document` is a Swift record containing `paragraphs` with nested `sentences` and `tokens`,
`dialogue` spans, `lemmaIndex` mapping each lemma to zero-based `TokenPosition` records
(`paragraph`, `sentence`, `token`), and `wordCount`. All paragraph, sentence, token, and
dialogue `start` / exclusive `end` offsets count UTF-16 code units in the original text.
Tokens preserve readings, chunks, `isWhitespace` and `isPunctuation`, and add `isWord`
and `isSkipped`. Skipped names stay in the document but are excluded from `wordCount`
and `lemmaIndex`; matching is case-sensitive, accepts ALL CAPS, and normalizes curly apostrophes. A lowercase
entry also accepts its first character capitalized; `Will` preserves the modal `will`. Adjacent
possessive clitics of skipped names are excluded as well.
The document pass uses the checker's existing tokenizer alongside `analyze` and `check`.

Document options default to full token details and one paragraph per nonempty line. For hard-wrapped
imports use `hardWrapped: true` (WASM also accepts `hard_wrapped`); blank lines then separate
paragraphs. Set `includeTokenDetails: false` (WASM: `include_token_details`) to reduce memory: WASM
omits token `readings`/`chunks` keys and Swift returns empty arrays, while counts and the index remain
unchanged. WASM accepts camelCase/snake_case option names and rejects unknown keys. See
[the document conventions](../docs/STYLE-DOCUMENT.md) and
[novel-length time/memory measurements](../docs/STYLE-DOCUMENT-BENCHMARKS.md).

## Style reports

`checker.styleReports(text:requestJson:documentOptions:)` is the one entry point for every
prose report: echoes, repeated phrases, clichés, dialogue tags, sentence length and sentence starts.
It builds the document once for all selected reports, reuses the cliché list compiled on the first
call (cached on the `Checker`), and performs no grammar or spelling checks. Omitting the request runs
every report except echoes with bundled baselines; echoes are an opt-in lens selected with
`{"echoes":{}}`. A request selects only the reports it names:

```swift
let json = try checker.styleReports(
    text: text,
    requestJson: #"{"sentenceLength": {}, "sentenceStarts": {"minimumSentences": 50}, "baselines": "bundled"}"#,
    documentOptions: DocumentOptions(skipNames: ["Mara"])
)
```

The result has `stats` (word, sentence and dialogue counts every report agrees with), the
`sentences` table that sentence reports index into, and one member per selected report. Keys and enum
values are camelCase and every span is `{start, end}` in UTF-16 code units, suitable for `NSRange`.
Errors throw `CheckError.ReportInput` with a message naming the kind (invalid options, invalid
baseline for a named report). Typed Swift records mirroring the Rust report types come with the
bindings follow-up; until then decode the JSON (for example with `Codable` structs). The request schema and one JSON example per
report are in [STYLE-REPORTS.md](../docs/STYLE-REPORTS.md#one-call-for-every-report).

The separate `sentenceLength(text:optionsJson:baselineJson:documentOptions:)` and `sentenceStarts(…)` methods are removed: each rebuilt the document,
and `styleReports` with `{"sentenceLength": {}}` or `{"sentenceStarts": {}}` replaces them.
`styleReports` always keeps token readings internally, so `includeTokenDetails` does not change its
output.
