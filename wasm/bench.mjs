// Benchmark the writemorebetter wasm package in Node.
//
// Loads wasm/pkg (built with `cargo build --target wasm32-unknown-unknown` + wasm-bindgen),
// reads storage/en_tokenizer.bin and storage/en_rules.bin as bytes (they are *not* embedded in
// the module), and measures construction time, per-check time, suggestion counts and
// process.memoryUsage() with the speller off and on (en_US).
//
// Usage:
//   node wasm/bench.mjs                 # both modes, 3 timed runs per input
//   BWB_RUNS=1 node wasm/bench.mjs      # one timed run (quick)
//   node --expose-gc wasm/bench.mjs     # also force GC between phases
//
// Inputs: bench/public/story1.txt and the first 150000 bytes of Moby-Dick in
// storage/moby150k.txt (see README.md for how to fetch it).

import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const MODES = ["off", "en_US"];

// Run each mode in a fresh child process so its memory numbers are not polluted by the other's
// (V8 cannot free the wasm heap between runs and Node needs --expose-gc to force a collection).
if (!process.env.BWB_MODE) {
  const runs = process.env.BWB_RUNS ?? "3";
  for (const mode of MODES) {
    const r = spawnSync(
      process.execPath,
      [...process.execArgv, fileURLToPath(import.meta.url)],
      { stdio: "inherit", env: { ...process.env, BWB_MODE: mode, BWB_RUNS: runs } },
    );
    if (r.status !== 0) process.exit(r.status ?? 1);
  }
  process.exit(0);
}

const mode = process.env.BWB_MODE;
const runs = Number(process.env.BWB_RUNS ?? "3");
const spell = mode === "off" ? undefined : mode;

const { Checker } = require(join(here, "pkg", "writemorebetter_wasm.js"));

const tokenizerBytes = readFileSync(join(root, "storage", "en_tokenizer.bin"));
const rulesBytes = readFileSync(join(root, "storage", "en_rules.bin"));

const inputs = [["story1", join(root, "bench", "public", "story1.txt")]];
const moby = join(root, "storage", "moby150k.txt");
if (existsSync(moby)) {
  inputs.push(["moby150k", moby]);
} else {
  console.error(`note: ${moby} missing; skipping Moby-Dick (see wasm/README.md)`);
}

function mem() {
  const m = process.memoryUsage();
  return `rss=${(m.rss / 1024 ** 2).toFixed(1)}MB heapUsed=${(m.heapUsed / 1024 ** 2).toFixed(1)}MB ` +
    `external=${(m.external / 1024 ** 2).toFixed(1)}MB arrayBuffers=${(m.arrayBuffers / 1024 ** 2).toFixed(1)}MB`;
}

const texts = inputs.map(([name, path]) => [name, readFileSync(path, "utf8")]);

console.log(`\n=== speller: ${mode} ===`);
const tConstruct = process.hrtime.bigint();
const checker = new Checker(tokenizerBytes, rulesBytes, spell);
const constructMs = Number(process.hrtime.bigint() - tConstruct) / 1e6;
console.log(`construction: ${constructMs.toFixed(1)} ms`);
console.log(`memory after construction: ${mem()}`);

for (const [name, text] of texts) {
  const times = [];
  let suggestions = 0;
  for (let i = 0; i < runs; i++) {
    const t = process.hrtime.bigint();
    const json = checker.check(text);
    times.push(Number(process.hrtime.bigint() - t) / 1e6);
    suggestions = JSON.parse(json).length;
  }
  const sorted = [...times].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  console.log(
    `${name}: ${text.length} chars, ${suggestions} suggestions, check ` +
      `best=${sorted[0].toFixed(1)}ms median=${median.toFixed(1)}ms ` +
      `over ${runs} run(s) [${times.map((t) => t.toFixed(1)).join(", ")}]`,
  );
  console.log(`memory after checking ${name}: ${mem()}`);
}

if (typeof globalThis.gc === "function") globalThis.gc();
