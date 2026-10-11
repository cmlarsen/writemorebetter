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

Out of scope and unchanged: no edits to nlprule matching/regex translation, builtins, tagger, or
`.github/`. `storage/`, `data/` and `wasm/pkg/` are not committed.
