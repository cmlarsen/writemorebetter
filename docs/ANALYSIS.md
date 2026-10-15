# Tokenizer analysis for prose reports

Issue #242 exposes the analysis that prose reports (#241) need. Load one tokenizer and reuse it:

```rust
let sentences = tokenizer.analyze("“She’s goin’—again…”");
for sentence in sentences {
    for token in sentence.tokens {
        println!("{} {:?}", token.text, token.readings);
    }
}
```

The result is owned (`nlprule::analysis::{AnalysisSentence, AnalysisToken, AnalysisReading,
AnalysisSpan}`), serializable, and independent of the tokenizer and input lifetimes. Each span
contains absolute, half-open `byte`, `char` (Unicode scalar), and `utf16` ranges. Surface text is
always sliced from the original input; normalized apostrophes stay confined to lemma/POS readings.
Readings are paired, so ambiguity never loses the relationship between a lemma and its POS tag.
Internal matcher sentinels (empty POS, UNKNOWN, SENT_START, SENT_END) are omitted. Unknown tokens
have no readings. Chunk tags are copied as assigned by the pipeline.

`analyze` runs sentence splitting, tagging, chunking and disambiguation exactly once. It skips raw
sentence copies and does not run grammar or spelling rules. Whitespace omitted by the internal
pipeline is reconstructed as tokens with empty readings/chunks. Tokens partition each returned
sentence, and no synthetic sentence marker is returned. Punctuation uses Unicode punctuation
categories; symbols such as emoji are neither punctuation nor whitespace. Sentence splitting,
including the treatment of trailing whitespace, follows `Tokenizer::pipe`. Empty and whitespace-only
input returns an empty vector. This call does not impose paragraph boundaries on the splitter;
paragraph structure and dialogue detection belong to the future document model (#243).

Swift's `Checker.analyze(text:)` returns `[AnalysisSentence]`; WASM's `checker.analyze(text)` returns
a JSON array. Both expose sentence/token `text`, `start`, `end`, and tokens with `readings`, `chunks`,
`isWhitespace` / `isPunctuation` (Swift) or `is_whitespace` / `is_punctuation` (JSON). Their offsets
are **UTF-16 code units**, so `NSRange` and JavaScript's `text.slice(start, end)` index the original
input directly. WASM's existing `check()` still uses Unicode scalar offsets; callers must observe
each method's documented convention.

## Independent reference probes

`nlprule/tests/fixtures/AnalysisProbe.java` prints readings and chunks from LanguageTool 6.6's
`JLanguageTool.analyzeText` on invented text: a multiword name, a dropped-g word, curly punctuation,
and a paragraph break. Run with a JDK and the unmodified 6.6 distribution:

```sh
java -cp "$LT_DIR/*:$LT_DIR/libs/*" nlprule/tests/fixtures/AnalysisProbe.java
cargo test -p nlprule --release --test analysis
```

The Rust expectations use those reference readings, rather than our existing output. The API is
an export of the current engine, not a promise that all analysis matches the reference: existing
engine conformance gaps remain. The fixtures test selected readings and splits, plus span coverage,
original surfaces, whitespace, punctuation and owned lifetimes. Swift and Node tests independently
slice strings with non-BMP characters using their platform's UTF-16 convention.

## Local timing

The example prints only aggregate counts and timings, so it can also be used on private text without
printing prose:

```sh
cargo build --release -p nlprule --example analyze
/usr/bin/time -l target/release/examples/analyze storage/en_tokenizer.bin bench/public/moby150k.txt
```

Measured locally on 2026-10-14, release, English 6.6 data, 149,091 bytes: 1,408 sentences and
56,203 tokens including whitespace. With `NLPRULE_PARALLELISM=off`, analysis took 1.020 s and
peak RSS was 61.0 MiB (tokenizer load 0.073 s). With the default pipeline parallelism, analysis
took 5.612 s and peak RSS was 58.2 MiB while other builds were running. These are analysis-only
measurements; they do not load grammar/spelling data. Default per-sentence disambiguation
parallelism has overhead on this workload; callers doing offline native statistics can use the
existing environment option. This API does not change the pipeline's parallelism policy.

The published-fiction baseline remains caller-supplied. The local `fiction-baseline` project ingests
and exports the EPUB corpus separately; no extracted text or corpus-derived baseline belongs in this
crate. Its author weighting and reviewed catalog should be preserved when that analysis is connected.
