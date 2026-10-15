# Document benchmark measurements

Issue [#243](https://github.com/cmlarsen/writemorebetter/issues/243) adds the shared document pass.
All workloads and evaluation sets here are public. No private corpus or fiction-baseline export was
accessed. These measurements diagnose quotation spans and construction costs, not speaker attribution
or the accuracy of future reports.

## Corrected LitBank results

The JSONL Rust probe now calls `dialogue_spans_with_options` with `hard_wrapped: true`: blank lines
separate paragraphs in LitBank's original hard-wrapped prose. The document API defaults to one
paragraph per nonempty line for editor input; callers can select hard-wrapped mode explicitly.
Offsets remain absolute into the original, unmodified annotated prefixes. Delimiters are included,
and exact matches require both UTF-8 byte and UTF-16 endpoints. Counts micro-aggregate all documents,
including those with no quotes; duplicates are extra false positives and unknown file IDs fail.

| Detector / split | Documents | Exact matches | Predicted | Gold | Precision | Recall | F1 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Python baseline / dev | 20 | 398 | 450 | 422 | 88.4444% | 94.3128% | 91.2844% |
| Initial Rust / dev | 20 | 413 | 479 | 422 | 86.2213% | 97.8673% | 91.6759% |
| Punctuated-single Rust, before recovery fix / dev | 20 | 410 | 470 | 422 | 87.2340% | 97.1564% | 91.9283% |
| Corrected recovery + elisions / dev | 20 | 411 | 475 | 422 | 86.5263% | 97.3934% | 91.6388% |
| Python baseline / test | 80 | 1,116 | 1,313 | 1,343 | 84.9962% | 83.0975% | 84.0361% |
| Initial Rust / test | 80 | 1,234 | 1,457 | 1,343 | 84.6946% | 91.8838% | 88.1429% |
| PR `8c0b578`, before recovery fix / test | 80 | 1,226 | 1,411 | 1,343 | 86.8887% | 91.2882% | 89.0341% |
| Corrected recovery + elisions / test | 80 | 1,234 | 1,447 | 1,343 | 85.2799% | 91.8838% | 88.4588% |

**The earlier precision gain was partly an artifact of a detector bug.** A leading elision or an
unmatched opener could leave quote state open indefinitely, suppressing later predictions, including
scare quotes that would have been false positives. The corrected detector drops an entire unfinished
span at a paragraph boundary unless the next paragraph repeats the same opener. An unfinished final
paragraph is dropped too. Common leading elisions, including `'em`, `'tis`, `'twas`, `'n'`, `'cause`,
`'90s` and `'eighties`, are apostrophes. Every adversarial review probe row has a checked-in fixture.
Malformed straight-quote parity can still be wrong within a paragraph; it cannot leak across unrelated
paragraphs. Single closers still require preceding speech punctuation; unpunctuated fragments and
British punctuation-outside forms can be missed.

**The initial detector revision was triggered by a test-split result:** initial test precision missed
the Python floor, after which the punctuated-single policy was justified with dev diagnostics and
measured on test. The recovery revision was triggered by review that included test-split diagnostics.
These are generic parser fixes, not document-specific rules, but this process does not support a claim
of untouched held-out evaluation. The corrected test precision/recall/F1 still exceed the original
Python baseline. `tier1-baseline.json` now records the corrected full-precision values, superseding the
bug-inflated floors. EWT thresholds, metric definitions and analysis behavior are unchanged. The
index's new unknown-word surface fallback does not change EWT's no-fallback linguistic scoring.

The historical Python `quote_spans` helper remains solely for historical fixture tests. The current
adapter uses the crate's probe and shared exact-span scorer. Thirteen harness tests cover original
prefixes, Unicode/UTF-16 offsets, duplicate predictions and unknown IDs; tier 2 smoke and tier 3's
synthetic-only self-test remain separate checks.

## Reproduce accuracy and native timing

```sh
cargo build --release --manifest-path bench/tools/wmb-document/Cargo.toml
cargo build --release --manifest-path bench/tools/wmb-analyze/Cargo.toml
python3 -m unittest discover -s bench/style -p 'test_*.py'
python3 bench/style/run.py tier1 --split dev \
  --analyzer bench/tools/wmb-analyze/target/release/wmb-analyze \
  --document-probe bench/tools/wmb-document/target/release/wmb-document
python3 bench/style/run.py tier1 --gate \
  --analyzer bench/tools/wmb-analyze/target/release/wmb-analyze \
  --document-probe bench/tools/wmb-document/target/release/wmb-document
NLPRULE_PARALLELISM=off /usr/bin/time -l \
  bench/tools/wmb-document/target/release/wmb-document \
  --benchmark storage/en_tokenizer.bin bench/public/moby150k.txt 5
```

`--benchmark TOKENIZER TEXT [ITERATIONS] [--compact] [--hard-wrapped]` loads one tokenizer, warms up
once, and measures construction of the owned document/index through `black_box`. Input reading,
loading, output serialization and dropping the returned document are outside each interval. The
native probe loads no rules binary. `/usr/bin/time -l` reports whole-process peak RSS in bytes on
macOS; on Linux use `-v` and account for its units. The target remains **1 second for 150 KB natively**.
Earlier pre-review measurements were 867.399 ms median (854.592–921.136 ms, five runs), 65.22 MiB peak
RSS on Apple M5 Pro. Initial runs under competing builds ranged to several seconds, so this is a
measured target, not a universal latency ceiling.

## Novel workload and compact output

The 150 KB input is the unchanged `bench/public/moby150k.txt`: 149,091 bytes, SHA-256
`5f801f9e4cec85f424b7c8c1359a45348959440229008208ce16f9d6f38c8446`.
The novel workload joins wrapped lines within blank-line paragraphs, then repeats that public excerpt
four times with blank paragraph separators (581,798 bytes, about 103,000 word tokens). It measures a
novel-length allocation workload, not an actual novel's linguistic distribution.

```sh
python3 - <<'PY'
from pathlib import Path
text = Path('bench/public/moby150k.txt').read_text()
paragraphs = [' '.join(line.strip() for line in p.splitlines())
              for p in text.split('\n\n') if p.strip()]
Path('/tmp/wmb-public-novel.txt').write_text('\n\n'.join(['\n\n'.join(paragraphs)] * 4))
PY
NLPRULE_PARALLELISM=off /usr/bin/time -l \
  bench/tools/wmb-document/target/release/wmb-document \
  --benchmark storage/en_tokenizer.bin /tmp/wmb-public-novel.txt 3 --hard-wrapped
# Repeat with --compact in a fresh process.
cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
wasm-bindgen --target nodejs --out-dir wasm/pkg \
  target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
node --expose-gc wasm/document-bench.mjs bench/public/moby150k.txt
node --expose-gc wasm/document-bench.mjs /tmp/wmb-public-novel.txt --hard-wrapped
node --expose-gc wasm/document-bench.mjs /tmp/wmb-public-novel.txt --hard-wrapped --compact
```

The default includes per-token readings/chunks. `include_token_details: false` discards them after
building the index: WASM omits the two JSON keys and Swift returns empty arrays. Counts and index
positions are identical, but POS/chunk-dependent reports require full details. Tokenization and
index construction still run, so compact output reduces returned/transient memory rather than
eliminating all analysis cost.

The WASM script constructs a fresh checker for each process, forces GC once after loading, and runs
three document calls. Each interval includes Rust projection/JSON serialization and the binding return;
JavaScript JSON parsing is outside the interval but included in the recorded after-parse RSS. It does
not force GC between calls. Process peak RSS includes JIT code, Rust linear memory, JS strings, parsed
objects, and allocator retention; it is not live document size. The checker loads both tokenizer and
rules, unlike the native tokenizer-only probe. Compare modes within each platform, not raw RSS between
platforms. All memory figures use MiB (2^20 bytes); JSON sizes are UTF-8 byte lengths.

## Measurements after review fixes (2026-10-15)

Apple M5 Pro, arm64 macOS, Rust 1.97.1, Node 26.8.1, release builds; native uses
`NLPRULE_PARALLELISM=off`. The machine is shared, so scheduling is uncontrolled. Native has one warmup
and five measured 150 KB iterations or three novel iterations; WASM has three calls without a warmup.

| Platform / workload / details | Median | Observed time range | Process peak RSS | Returned JSON bytes |
|---|---:|---:|---:|---:|
| Native / 149,091 bytes / full | 933.709 ms | 850.190–1,139.504 ms | 57.23 MiB | — |
| Native / 581,798 bytes / full | 3,365.144 ms | 3,228.138–3,366.702 ms | 125.45 MiB | — |
| Native / 581,798 bytes / compact | 3,185.759 ms | 3,170.222–3,228.439 ms | 90.91 MiB | — |
| WASM / 149,091 bytes / full | 1,204.343 ms | 1,111.190–1,285.923 ms | 324.39 MiB | 11,531,137 |
| WASM / 581,798 bytes / full | 4,040.312 ms | 4,019.882–4,127.404 ms | 686.52 MiB | 46,226,091 |
| WASM / 581,798 bytes / compact | 3,995.910 ms | 3,941.118–4,101.063 ms | 376.89 MiB | 33,286,371 |

The 150 KB native median still meets the stated target; its maximum exceeds it. The novel-length
WASM process started at 168.70 MiB RSS (full) / 168.39 MiB (compact). After parsing each call, RSS was
499.17 / 686.34 / 616.78 MiB for full output and 339.97 / 376.59 / 369.77 MiB for compact. Thus compact
output substantially reduces transient process memory, while its JSON is still large because text,
spans, flags and index positions remain. It does not make novel analysis suitable for the UI thread.

Rust workspace release tests (excluding the legacy Python binding as CI does), nine style fixture
checks, eleven Swift tests, Node WASM tests, thirteen Python harness tests and the corrected tier 1
gate pass locally. Generated Swift is committed byte-for-byte, including generator whitespace;
`.editorconfig` and `.gitattributes` protect that directory from whitespace trimming/check noise.
The deterministic engine stress workflow is triggered for workspace manifest changes, without claiming
that its engine harness fuzzes the new style APIs. Style's recovery fixtures directly exercise both
product paragraph modes, name case/possessives, compact output and unknown-word indexing.
