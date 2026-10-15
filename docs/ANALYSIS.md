# Owned tokenizer analysis

`Tokenizer::analyze(&str) -> Vec<analysis::AnalysisSentence>` exposes the existing sentence-splitting,
tagging, chunking and disambiguation pipeline. The result owns all strings and can outlive both the
input and tokenizer. It runs no grammar or spelling checks and needs no rules binary. It does not
change tokenization, tagging, rule matching, existing `pipe()`, binary formats, or loading behavior.

```rust
use nlprule::Tokenizer;
let tokenizer = Tokenizer::new("storage/en_tokenizer.bin")?;
let text = "😀 “She’s goin’—again…”";
let sentences = tokenizer.analyze(text);
for sentence in sentences {
    for token in sentence.tokens {
        assert_eq!(text.get(token.span.byte.clone()), Some(token.text.as_str()));
        println!("{} {:?}", token.text, token.readings);
    }
}
```

The public types are in `nlprule::analysis`:

- `AnalysisSentence`: original sentence text, span, and tokens.
- `AnalysisToken`: original surface text, span, paired lemma/POS readings after disambiguation,
  chunk tags, and `is_whitespace` / `is_punctuation` flags.
- `AnalysisReading`: one lemma and POS pair. Several readings preserve genuine ambiguity; neither
  their order nor the tag set is a cross-language contract.
- `AnalysisSpan`: absolute, half-open `byte`, `char` (Unicode scalar) and `utf16` ranges into the input.

Normalized apostrophes stay confined to linguistic readings; surface text is always taken from the
original input at checked UTF-8 boundaries. Internal matcher sentinel readings (empty POS, UNKNOWN,
SENT_START, SENT_END) are omitted. Unknown tokens have no readings. Chunk tags retain the pipeline's
existing strings. Unicode punctuation categories determine `is_punctuation`; emoji are symbols,
and a contraction with letters is not punctuation.

Whitespace gaps omitted by the internal pipeline are reconstructed with empty readings/chunks.
Each input token is returned once; token spans partition each returned sentence, without synthetic
sentence-marker tokens. Sentence boundaries and trailing-whitespace inclusion follow `pipe()`.
Empty or whitespace-only input returns an empty vector. This method does not impose new paragraph
boundaries or detect dialogue.

`analyze()` uses the caller's already-loaded tokenizer, as does `Rules::suggest()`. It does not load
another tokenizer, and it does not keep the raw token copies needed by a few grammar rules.
Separate calls to `analyze()` and `suggest()` analyze their respective inputs independently.

## Swift and WASM

Swift's `Checker.analyze(text:)` returns owned `[AnalysisSentence]` records. WASM's `analyze(text)`
returns a JSON array. Both reuse the tokenizer already owned by `Checker`.

Sentence and token records expose `text`, absolute `start` and exclusive `end`, and tokens expose
`readings`, `chunks`, and whitespace/punctuation flags. All binding offsets count **UTF-16 code
units**, suitable for JavaScript `text.slice(start, end)` or Swift `NSRange`. A lemma/POS reading is
`{"lemma":"go","pos":"VBG"}`. JSON flags use `is_whitespace` / `is_punctuation`; Swift uses
`isWhitespace` / `isPunctuation`.

WASM `check()` retains its existing Unicode scalar offsets. Its output is intentionally unchanged;
observe the convention documented for each method.

## Independent reference and verification

The invented inputs and checked-in transcripts under
[`nlprule/tests/fixtures/analysis`](../nlprule/tests/fixtures/analysis/README.md) come from the pinned
LanguageTool 6.6 CLI's `--taggeronly` output. Tests assert selected independent lemma/POS readings
and a chunk tag, including a multiword entry and `goin'`. Span tests cover curly quotes/apostrophes,
em dash, ellipsis, U+2028, paragraph breaks, combining marks, and non-BMP characters. Swift and Node
slice the original strings using UTF-16 offsets and alternate `analyze()` with `check()` on the same
checker. The export does not claim to fix the engine's existing analysis/conformance gaps.

The optional Rust example prints only counts and timings:

```sh
cargo build --release -p nlprule --example analyze
NLPRULE_PARALLELISM=off /usr/bin/time -l target/release/examples/analyze \
    storage/en_tokenizer.bin bench/public/moby150k.txt
```

### Validation for #242

Compared with main at `5259287`, using freshly compiled English 6.6 data:

| Gate | Base | analyze API |
|---|---:|---:|
| Runnable grammar examples | 6,074 / 6,074 | 6,074 / 6,074 |
| Disambiguation examples | 1,016 / 1,016 | 1,016 / 1,016 |
| Public verified-error recall | 55 / 60 | 55 / 60 |

The passing conformance IDs and public recall probe outputs are identical. Recall used
`python3 bench/tools/recall.py --truth bench/public/truth.json --runs <probe-output-directory>`.
The default private-corpus recall gate could not run: this checkout lacks
`bench/corpus/truth.json`. Public recall does not establish private-corpus recall.

Rust workspace tests (excluding the unmaintained Python binding) passed 487 tests; Swift passed
nine tests, and the Node WASM binding tests passed. Clippy and Rust formatting checks passed.
All 12 existing public golden suggestion files matched.

On an Apple M5 Pro, release builds, `NLPRULE_PARALLELISM=off`, `moby150k.txt` (149,091 bytes),
fiction profile and `en_US` spelling enabled, five fresh-process runs of `bench/tools/perf.py`
gave these medians:

| Measurement | Base | analyze API |
|---|---:|---:|
| Check time | 4.104 s | 4.196 s |
| Peak RSS | 87.47 MiB | 89.78 MiB |
| Loaded RSS | 68.28 MiB | 68.62 MiB |
| Load time | 0.103 s | 0.098 s |

Check-time ranges overlap (base 3.871–4.349 s; feature 3.781–4.331 s). These measurements do
not establish a performance improvement. The tokenizer/check pipeline and loading code are
unchanged. One tokenizer-only example run on the same text took 1.010 s to return 1,408
sentences and 56,203 tokens, with 60.84 MiB peak RSS measured by `/usr/bin/time -l`.
