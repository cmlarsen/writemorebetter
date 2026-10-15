# Document benchmark measurements

Issue [#243](https://github.com/cmlarsen/writemorebetter/issues/243) adds the Rust document model.
Measurements use public data only. No private corpus or fiction-baseline export was accessed.

## Reproduce

Build the native release probes with the English 6.6 tokenizer staged at
`storage/en_tokenizer.bin`:

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

`--benchmark TOKENIZER TEXT [ITERATIONS]` loads the tokenizer once, warms up once, then measures
`analyze_document(&tokenizer, &text, &DocumentOptions::default())`. Each duration includes construction
of the owned document and its lemma index; dropping the returned document, reading input, loading
the tokenizer, and serializing aggregate JSON are outside the measured interval. The document is
consumed through `black_box`. Output includes tokenizer load time separately and the sorted durations,
minimum, median and maximum in milliseconds. `/usr/bin/time -l` is the macOS RSS measurement; on Linux
use `/usr/bin/time -v` and account for its RSS units. Run without competing builds for useful timing.
This benchmark measures document construction, not a report or grammar/spelling checking.

The dialogue mode needs no tokenizer. Supply one object per input line and receive one object per
detected span, with no output for documents without detected dialogue:

```sh
printf '%s\n' '{"file":"sample.txt","text":"She said, \"Hello.\""}' | \
  bench/tools/wmb-document/target/release/wmb-document --dialogue
```

Every output record has `file`, `byte: {start,end}`, `utf16: {start,end}`, and `pattern: "dialogue"`.
Offsets are absolute, half-open ranges into the supplied text; quotation delimiters are included.
The Python adapter batches the annotated LitBank prefixes in one probe process and uses the shared
exact-span scorer, requiring both byte and UTF-16 ranges to match. Unknown file identifiers fail and
duplicate predictions count as extra false positives. The pinned sources, fixed split, alignment and
annotation-prefix conventions remain in [the benchmark README](../bench/style/README.md).

## LitBank before and after

Measured 2026-10-15. The Python baseline was run before switching tier 1 to the Rust probe. Gold
documents and original text prefixes are identical; the new detector also recognizes single quotes.
Percentages below are micro-aggregated exact-span scores, not overlap scores or speaker attribution.

| Detector / split | Documents | Exact matches | Predicted | Gold | Precision | Recall | F1 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Python baseline / dev | 20 | 398 | 450 | 422 | 88.4444% | 94.3128% | 91.2844% |
| Preliminary Rust / dev | 20 | 413 | 479 | 422 | 86.2213% | 97.8673% | 91.6759% |
| Final Rust document / dev | 20 | 410 | 470 | 422 | 87.2340% | 97.1564% | 91.9283% |
| Python baseline / test | 80 | 1,116 | 1,313 | 1,343 | 84.9962% | 83.0975% | 84.0361% |
| Preliminary Rust / test | 80 | 1,234 | 1,457 | 1,343 | 84.6946% | 91.8838% | 88.1429% |
| Final Rust document / test | 80 | 1,226 | 1,411 | 1,343 | 86.8887% | 91.2882% | 89.0341% |

The preliminary Rust result improved test F1 by 4.1067 percentage points and recall by 8.7863 points,
but precision fell by 0.3016 points and failed the existing floor. Preliminary dev diagnostics found
15 true and 15 false single-quote spans, and 398 true and 51 false double-quote spans.
Thirteen single-quote false positives did not overlap any
gold quotation; two overlap with incorrect endpoints. Procedural quotation pairing still identifies
some quoted material that LitBank does not label direct speech.

The final policy requires punctuation immediately before a single-quote closer (`.!?,:;—…`), leaving
word-final apostrophes inside the speech open. This avoids cutting at plural possessives or dropped-g
forms, with deliberately conservative coverage for unpunctuated single-quoted fragments. A dev-only
state-machine simulation exactly reproduced the preliminary detector before evaluating this policy;
it predicted the final dev counts above. The test split was measured only after the policy decision.
Final test precision improves by 1.8925 points, recall by 8.1906 points and F1 by 4.9980 points over
Python. All old floors pass; the three LitBank floors are raised to the full-precision final values
in `tier1-baseline.json`, with no changes to EWT floors or metric definitions.

The historical Python `quote_spans` helper remains in the harness for its existing fixture tests but
is no longer the tier 1 predictor. The new harness test verifies original-prefix truncation, non-BMP
UTF-16 offsets, duplicates and unknown file identifiers using invented text.

## Native document construction

The comparison target is **1 second for the public 150 KB excerpt**, as proposed in issue #243.
Input `bench/public/moby150k.txt` is 149,091 bytes, SHA-256
`5f801f9e4cec85f424b7c8c1359a45348959440229008208ce16f9d6f38c8446`.
Measurements use Apple M5 Pro, arm64 macOS, Rust 1.97.1, native release builds, and
`NLPRULE_PARALLELISM=off`. The English tokenizer is loaded separately; no rules binary is loaded.

| Run | Iterations after warmup | Minimum | Median | Maximum | Tokenizer load | Process peak RSS |
|---|---:|---:|---:|---:|---:|---:|
| Initial, concurrent probe build | 7 | 1,541.558 ms | 1,975.859 ms | 5,184.025 ms | 91.553 ms | 60.91 MiB |
| Repeat, local probe builds finished | 7 | 1,849.392 ms | 3,491.561 ms | 4,571.081 ms | 97.929 ms | 58.09 MiB |
| Coordinator run before final name handling | 5 | 890.333 ms | 936.558 ms | 1,494.933 ms | 64.288 ms | 64.94 MiB |

| Final committed core, builds finished | 5 | 854.592 ms | 867.399 ms | 921.136 ms | 84.728 ms | 65.22 MiB |

The second run's sorted durations were 1,849.392, 1,975.160, 2,292.994, 3,491.561, 3,993.763,
4,535.890 and 4,571.081 ms. Neither run meets the 1-second target. Both show substantial scheduling
contention (34,947 and 57,969 involuntary context switches respectively); the second run has no
concurrent build started by this benchmark worker, but other work on the shared machine remained
uncontrolled. These observations establish the measured budget shortfall, not an idle-machine
latency claim. The coordinator's final five sorted durations were 890.333, 916.407, 936.558, 1,449.384
and 1,494.933 ms, with 6,562 involuntary context switches. Its median meets the 1-second target;
its slowest observations exceed it, so this is a measured median target rather than a latency ceiling.
The final committed-core run, after all builds finished, measured 854.592, 863.674, 867.399,
892.787 and 921.136 ms: every observed iteration meets the target. Tokenizer loading is excluded.
RSS is whole-process peak across tokenizer load, warmup and all iterations, converted
from macOS bytes to MiB; it is not an incremental allocation measurement.

## WASM document binding

Measured 2026-10-15 on the same Apple M5 Pro, with Node v26.8.1 and Rust 1.97.1.
The release `wasm32-unknown-unknown` build uses `regex-fancy` and the repository's
`opt-level=s` setting. All builds had finished before this run. The input is the same
149,091-byte `bench/public/moby150k.txt` identified above.

| Call | Document binding time |
|---|---:|
| 1 | 1,592.660 ms |
| 2 | 1,244.451 ms |
| 3 | 1,128.374 ms |
| Mean | 1,321.828 ms |
| Median | 1,244.451 ms |

One checker is constructed before timing, with tokenizer and rules loading excluded.
There is no warmup call: the first observation includes any first-call V8 optimization work.
Each interval measures `checker.analyze_document(text)` through its return to JavaScript,
including document construction, binding projection to UTF-16 `start`/`end` records,
Rust JSON serialization and string transfer. JavaScript JSON parsing is outside each
interval. Every result contains 25,743 words and 2,308 paragraphs; its JSON string is
11,506,268 UTF-8 bytes. All three observations exceed the proposed 1-second target.
This measures the JSON binding with a different regex backend from the native probe,
so the difference does not isolate the cost of WebAssembly execution or JSON serialization.
The shared machine's other workloads are uncontrolled.

Reproduce from the repository root after staging the English binaries:

```sh
cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
wasm-bindgen --target nodejs --out-dir wasm/pkg \
  target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
node --input-type=module <<'JS'
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
const require = createRequire(import.meta.url);
const { Checker } = require('./wasm/pkg/writemorebetter_wasm.js');
const checker = new Checker(
  readFileSync('storage/en_tokenizer.bin'), readFileSync('storage/en_rules.bin'));
const text = readFileSync('bench/public/moby150k.txt', 'utf8');
for (let run = 1; run <= 3; run++) {
  const start = performance.now();
  const result = checker.analyze_document(text);
  const ms = performance.now() - start;
  const document = JSON.parse(result);
  console.log({ run, ms, word_count: document.word_count,
    paragraphs: document.paragraphs.length, json_bytes: Buffer.byteLength(result) });
}
JS
```
