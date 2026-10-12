# writemorebetter-swift

Swift bindings for the writemorebetter checker (English), generated with [UniFFI](https://mozilla.github.io/uniffi-rs/)
proc-macros. The crate builds a static library; `scripts/build-xcframework.sh` packages the four Apple slices
into `dist/WriteMoreBetterFFI.xcframework` and regenerates the Swift sources under
`swift/Sources/WriteMoreBetter/Generated/`.

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

## Building the XCFramework

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
./scripts/build-xcframework.sh
```

The script builds `aarch64-apple-darwin` + `x86_64-apple-darwin` (lipo'd into one `macos-arm64_x86_64` slice),
`aarch64-apple-ios`, and `aarch64-apple-ios-sim` + `x86_64-apple-ios` (one `ios-arm64_x86_64-simulator` slice),
all with nlprule's native `regex-onig` backend. It then runs `uniffi-bindgen-swift` in library mode and calls
`xcodebuild -create-xcframework`. Everything lands in `dist/` (git-ignored) plus the generated Swift sources,
which are committed so the package always has a source target.

## The Swift package

`Package.swift` at the repository root publishes the `WriteMoreBetter` library over two targets: the Swift
target (generated bindings plus `Suggestion+NSRange.swift`) and the `WriteMoreBetterFFI` binary target.

Resolution picks the binary target when the manifest is evaluated:

- `dist/WriteMoreBetterFFI.xcframework` exists — a checkout you just ran the build script in. The binary target
  is path-based, so `.package(path:)` works offline and against unreleased changes.
- It does not exist — a released tag, where `dist/` is git-ignored. The binary target uses the `url` and
  `checksum` in `release.json`.

`release.json` is a separate generated manifest rather than values rewritten inside `Package.swift`: the
hand-written manifest is never machine-edited (no diff churn to review, and a bad rewrite cannot break
resolution), and the one file that changes per release is exactly the machine-generated one. The release
workflow builds the framework zip, computes its checksum, commits the updated `release.json` and only then
tags and uploads the asset, so a tag's `release.json` always matches the asset next to it.

SwiftPM caches evaluated manifests by manifest contents, not by the filesystem. A checkout that was resolved
before `scripts/build-xcframework.sh` ran keeps the `release.json` branch until `swift package purge-cache`
(or a fresh clone) refreshes it.

## Tests

`swift/Tests/WriteMoreBetterTests` loads the LT 6.6 binaries from `storage/66/` (the repository's
per-rule-set storage convention; override with `WMB_DATA_DIR`), checks a sentence with a known error,
asserts UTF-16 offsets on text that mixes an emoji, curly quotes, an em dash, a combining mark and
U+2028, and calls `check` concurrently from eight threads. Run them with:

```sh
cargo run --release --all-features --bin compile -- \
    --build-dir data/en-lt66 --tokenizer-out storage/66/en_tokenizer.bin --rules-out storage/66/en_rules.bin
./scripts/build-xcframework.sh
swift test
```

`.github/workflows/swift.yml` does exactly this on a macOS runner, generating `data/en-lt66` the same way the
6.6 job in `ci.yml` does and failing if the committed bindings drift from the freshly generated ones.
