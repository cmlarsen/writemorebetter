# M6-WASM: fancy-regex conformance cost and wasm package

Measured 2026-10-11 on macOS aarch64 (15 logical CPUs, 24 GiB), rustc/cargo 1.97.1, Node v26.8.1,
wasm-bindgen 0.2.129 (pinned to match the `wasm-bindgen` crate already in `Cargo.lock`), wasm-opt
116 (binaryen). Data binaries compiled from `data/en` with `--all-features` as in CI.

> Caveat: a concurrent `cargo` build from another session was running on the machine during these
> measurements, so check times are upper bounds (see the note in `docs/BASELINE.md`). Memory is
> unaffected by load.

## Part 1 — conformance per regex backend

Binaries compiled with `--all-features` (the `compile` feature pulls in `regex-all-test`, so the
compiler validates every pattern against **both** backends). The conformance binaries were then run
with one backend at a time; the fancy run was verified to genuinely link fancy (`nm` shows 0 onig
symbols and 182 fancy symbols).

| Binary | onig | fancy |
|---|---|---|
| `test` (grammar) | 3749 / 3749 | 3749 / 3749 |
| `test_disambiguation` | 843 / 843 | 843 / 843 |

**Grammar rule IDs that pass with onig but fail with fancy: none.** Zero warnings were emitted by
the fancy run, and the same is true of disambiguation. The minimal failing set the task asked about
is empty.

The reason is structural: because `compile` enables `regex-all-test`, `Regex::new` fails if *either*
backend rejects a pattern, so any pattern fancy cannot compile is already excluded when the binary is
built. The fancy-only run therefore never has to compile a regex at the point of matching. There is
no runtime conformance cost.

The cost shows up at **compile time** instead. The compiler reported 4 regex rejections, and the
error strings identify the backend:

| Error | Count | Backend | Rule |
|---|---|---|---|
| `Look-behind assertion without constant size` | 2 | fancy-regex | see below |
| `Oniguruma error: target of repeat operator is invalid` | 2 | onig | — |

The two fancy rejections are the only rules in `grammar.xml` with variable-length look-behind, which
Oniguruma supports and fancy-regex 0.5 does not:

- `PUNCTUATION/UNIT_SPACE/0` — `(?<![A-Z\$€£¥฿฿=][0-9\.]{0,5})` (sibling rules `/1`, `/2`, `/3` remain)
- `TYPOGRAPHY/MULTIPLICATION_SIGN/1` — `(?<!([a-vyz]|[a-vyz]\d|…|[a-vyz]\d{5}))` (sibling `/0` remains)

Both rules are absent from the compiled set (a probe listing rule IDs shows the index gap), so an
onig-only compile would have **3751** runnable grammar rules instead of 3749 — an inferred 2-rule
compile-time cost, not a measured one, since the `compile` feature cannot produce an onig-only
binary without editing `Cargo.toml`. Neither dropped rule's examples were evaluated. The 2 onig
rejections are shared by both backends and are the same 2 the baseline counted.

## Part 2 — wasm package

New crate `wasm/` (`writemorebetter-wasm`, cdylib + rlib, MIT OR Apache-2.0), depending on `nlprule`
with `default-features = false, features = ["regex-fancy"]` and `wasm-bindgen`. The LGPL data
binaries are **not** embedded: the wasm module is 1.67 MiB while the data is 20.7 MiB, and a
tokenizer-only marker string is absent from the module. JS reads the two files and passes the bytes
to `new Checker(tokenizerBytes, rulesBytes, spellVariant)`; `check(text)` returns the JSON array of
`{start, end, source, message, replacements}` with character offsets.

Build: `cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm`, then
`wasm-bindgen --target nodejs`, then `wasm-opt -Oz` (all succeeded).

### Size

| Stage | Raw | gzip -9 |
|---|---|---|
| `cargo build` `.wasm` | 2,398,817 B (2.29 MiB) | 708,473 B |
| after `wasm-bindgen` | 2,379,395 B (2.27 MiB) | 691,020 B |
| after `wasm-opt -Oz` | 1,746,712 B (1.67 MiB) | 642,371 B |

### Node runtime

`node wasm/bench.mjs`, 3 timed runs per input after construction; inputs are `bench/public/story1.txt`
(11,327 chars) and the first 150,000 bytes of Moby-Dick (`storage/moby150k.txt`, 147,880 chars).

| Speller | Construction | Check story1 (best / median) | Sugg. | Check moby150k (best / median) | Sugg. |
|---|---|---|---|---|---|
| off | 113.6 ms | 276.9 / 283.7 ms | 6 | 3314.2 / 3340.9 ms | 80 |
| en_US | 112.4 ms | 287.2 / 297.5 ms | 14 | 3759.1 / 3783.6 ms | 981 |

Memory (`process.memoryUsage()`, in a fresh process per mode):

| Stage | rss | heapUsed | external | arrayBuffers |
|---|---|---|---|---|
| after construction, off | 210.5 MB | 5.3 MB | 23.6 MB | 21.6 MB |
| after checking moby150k, off | 249.3 MB | 5.2 MB | 22.0 MB | 19.9 MB |
| after construction, en_US | 215.2 MB | 5.3 MB | 23.6 MB | 21.6 MB |
| after checking moby150k, en_US | 254.5 MB | 6.7 MB | 23.6 MB | 19.9 MB |

The first timed run of each input is consistently slower (e.g. story1 off: 439.5 ms vs 276.9 ms
after), which is V8 warm-up, not steady-state.

### Memory: where the construction RSS goes (M6B-WASM-MEMORY)

`node wasm/memory.mjs` reports `process.memoryUsage()` and the module's linear memory size
(`memory.buffer.byteLength`, reached by capturing the `WebAssembly.Instance`) at four points in a
fresh process, plus Node's baseline RSS for an empty script. A fresh process per stage per mode:
macOS RSS never shrinks, so one process cannot measure two stages independently.

Measured against `origin/main` at `fa45968`, with the English binaries rebuilt from `data/en` for
that tree (`en_tokenizer.bin` 14.9 MB + `en_rules.bin` 10.6 MB) and the speller off.

Before (default release profile; constructor took `&[u8]`):

| Stage | rss | wasm linear |
|---|---|---|
| Node baseline, empty script | 47.4 MB | — |
| (a) module instantiated | 59.5 MB | 1.5 MB |
| (b) both binaries read into JS Buffers | 83.9 MB | 1.5 MB |
| (c) after `new Checker(...)` | 246.2 MB | 77.1 MB |
| (d) after checking moby150k | 188.3 MB | 111.4 MB |

After (owned `Vec<u8>`, boxed regex cache, `opt-level=s`):

| Stage | rss | wasm linear |
|---|---|---|
| Node baseline, empty script | 47.4 MB | — |
| (a) module instantiated | 59.4 MB | 1.5 MB |
| (b) both binaries read into JS Buffers | 83.9 MB | 1.5 MB |
| (c) after `new Checker(...)` | 178.6 MB | 59.1 MB |
| (d) after checking moby150k | 158.9 MB | 107.4 MB |

`en_US` is the same at (c) (182.0 MB) and (d) (178.7 MB). (d) reads below (c) because `memory.mjs`
drops the JS Buffers after construction and the long check gives V8 time to collect them; the wasm
heap itself never shrinks.

Where the 246.2 MB went:

- **Node itself, 47.4 MB.** Unavoidable; the same for an empty script.
- **The wasm module, ~12.1 MB.** Baseline (Liftoff) code plus module metadata.
- **JS copies of the binaries, ~24.4 MB** (26.3 MB of `arrayBuffers`). `readFileSync` allocates two
  Buffers; wasm-bindgen then copies them into linear memory, so two copies exist during
  construction.
- **wasm linear memory, 77.1 MB.** This is the *peak* as well as the current size, because wasm
  memory never shrinks: the constructor's input copies, every bincode deserialization temporary,
  and the final structures all live in one monotonic heap.
- **V8 TurboFan tier-up code, ~81 MB.** The surprise and the largest single item.
  `node --liftoff-only` (baseline compiler only) takes construction RSS from 245.8 MB to 164.6 MB
  while linear memory is unchanged (77.1 MB): ~81 MB is optimized machine code V8 emits for the hot
  wasm functions, not data. It scales with the module's function sizes — `opt-level=3` inlines
  aggressively, and `wasm-opt -Oz` cannot undo that inlining.

The fixes, each measured on this tree (speller off, default V8 unless noted):

| Build | wasm linear | RSS after construction |
|---|---|---|
| `origin/main` | 77.1 MB | 246.2 MB |
| + take the bytes by value and box the regex cache | 59.1 MB | 233.1 MB |
| + `opt-level=s` | 59.1 MB | 180.5 MB |

1. **Take the input bytes by value (`Vec<u8>`), not `&[u8]`.** wasm-bindgen's generated shim for a
   borrowed slice allocates a wasm buffer, copies into it, and never frees it, so every construction
   leaked both binaries for the life of the process. Owning them lets each be released as soon as it
   has been deserialized.
2. **Box the lazily-cached regex (`OnceCell<Box<Regex>>`).** `OnceCell` stores its value inline; with
   ~70k pattern regexes the slot itself cost several MB (rules' linear memory 30.0 → 27.0 MB on the
   earlier base). Together, 1 + 2 cut linear memory 77.1 → 59.1 MB.
3. **Compile wasm32 with `opt-level=s`** (`.cargo/config.toml`). This roughly halves the tier-up
   code (81 → 33 MB) and takes construction RSS 233.1 → 180.5 MB. Checking gets slower — story1
   272 → 352 ms, moby150k 3793 → 4691 ms (median, `en_US`) — and `opt-level=2`/`3` both keep the
   ~80 MB of JIT code. `opt-level=z` saves ~1.5 MB more of wasm but is ~13% slower again, so `s` is
   the fastest profile with the smaller JIT footprint; reverting is a one-line change.
4. **Do not retain the byte buffers in JS** (README.md, `bench.mjs`, `compare.mjs`). The
   constructor ignores them after the call, so a long-lived process should release them
   (`arrayBuffers` 26.3 → 0.1 MB). This does not lower the construction peak, because the Buffers
   exist during the call.

The allocator was left alone: linear memory tracks the live structures closely — a probe on the
earlier (21.2 MB) binaries that loads only the tokenizer reported 38.4 MB = 11.6 MB input copy +
26.8 MB structures, against ~34 MB native with wider pointers — so dlmalloc fragmentation is not a
large term and swapping in `lol_alloc`/`talc` was not justified.

Net: construction RSS 246.2 → 178.6 MB and linear memory 77.1 → 59.1 MB. Native is unaffected:
`SPELL= /usr/bin/time -l wmb-probe storage moby150k.txt` still reports 34 MB after the tokenizer,
55 MB after the rules, ~78 MB peak RSS.

## Part 3 — wasm (fancy) vs native onig

`node wasm/compare.mjs` runs the wasm checker with `en_US` and `bench/tools/wmb-probe` (`SPELL=en_US`,
`SHOW=1`, native onig), and compares normalized suggestion sets
`(line, source, flagged text, replacements)`.

| Input | native onig | wasm (fancy) | differing suggestions |
|---|---|---|---|
| story1 | 14 | 14 | 0 |
| story2 | 16 | 16 | 0 |
| story3 | 16 | 16 | 0 |
| story4 | 23 | 23 | 0 |
| story5 | 28 | 28 | 0 |
| moby150k | 981 | 981 | 0 |

All suggestion sets are identical; there are no differing suggestions to list.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --workspace --exclude nlprule-python --all-features` | pass (0 errors; the wasm crate emits none) |
| `cargo test --workspace --exclude nlprule-python --all-features --release` | pass |
| `python3 bench/tools/public_recall.py --min 51` | pass, 51/60 |
| disambiguation conformance (`test_disambiguation`) | 843 / 843 runnable |
| grammar conformance (`test`) | 3749 / 3749 runnable |

M6B-WASM-MEMORY changed `nlprule`'s lazily-cached regex to a boxed `OnceCell` (matching behavior,
serialized format and conformance unchanged) and the wasm crate's constructor signature and build
profile; no edits to nlprule matching/regex translation, builtins, tagger, or `.github/`. `storage/`,
`data/` and `wasm/pkg/` are not committed.
