// Where the wasm package's RSS goes, in a fresh Node process.
//
// Reports `process.memoryUsage()` and the wasm module's linear memory size at four points:
//   (a) after instantiating the module (requiring wasm/pkg),
//   (b) after reading storage/en_tokenizer.bin and storage/en_rules.bin into JS Buffers,
//   (c) after `new Checker(...)`,
//   (d) after `checker.check(moby150k)`.
// It also reports Node's baseline RSS for an empty script, and shows the effect of dropping the
// JS-side binary buffers after construction (the wasm heap cannot free the allocator's high-water
// mark, but the JS copies can go).
//
// Usage:
//   node wasm/memory.mjs                 # speller off, then en_US, fresh process each
//   BWB_MODE=off node wasm/memory.mjs    # one mode, in this process
//
// Every run is a fresh child process: Node's RSS never shrinks, so a stage silently masks the
// next one if they share a process across modes.

import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const MODES = ["off", "en_US"];

if (!process.env.BWB_MODE) {
  // Baseline: an empty script, so the reader can subtract Node itself.
  const empty = spawnSync(process.execPath, ["-e", "console.log(JSON.stringify(process.memoryUsage()))"], {
    encoding: "utf8",
  });
  const m = JSON.parse(empty.stdout);
  const mb = (x) => (x / 1024 ** 2).toFixed(1);
  console.log(`\n=== node baseline (empty script) ===`);
  console.log(`rss=${mb(m.rss)}MB heapUsed=${mb(m.heapUsed)}MB external=${mb(m.external)}MB arrayBuffers=${mb(m.arrayBuffers)}MB`);

  for (const mode of MODES) {
    const r = spawnSync(
      process.execPath,
      ["--expose-gc", ...process.execArgv, fileURLToPath(import.meta.url)],
      { stdio: "inherit", env: { ...process.env, BWB_MODE: mode } },
    );
    if (r.status !== 0) process.exit(r.status ?? 1);
  }
  process.exit(0);
}

const mode = process.env.BWB_MODE;
const spell = mode === "off" ? undefined : mode;

function mem() {
  const m = process.memoryUsage();
  return `rss=${(m.rss / 1024 ** 2).toFixed(1)}MB heapUsed=${(m.heapUsed / 1024 ** 2).toFixed(1)}MB ` +
    `external=${(m.external / 1024 ** 2).toFixed(1)}MB arrayBuffers=${(m.arrayBuffers / 1024 ** 2).toFixed(1)}MB`;
}

// wasm-bindgen 0.2.129's nodejs output keeps the instantiated `wasm` local, so it is not a
// public export. Capture the instance at construction time: `WebAssembly.Instance` is called once
// by the generated module loader, and `instance.exports.memory` is the linear memory.
const RealInstance = WebAssembly.Instance;
let wasmInstance = null;
WebAssembly.Instance = function (...args) {
  const instance = new RealInstance(...args);
  wasmInstance = instance;
  return instance;
};

console.log(`\n=== speller: ${mode} (fresh process) ===`);

const { Checker } = require(join(here, "pkg", "writemorebetter_wasm.js"));
WebAssembly.Instance = RealInstance;

const linear = () => {
  const bytes = wasmInstance.exports.memory.buffer.byteLength;
  return `wasmLinear=${(bytes / 1024 ** 2).toFixed(1)}MB (${bytes} B)`;
};
console.log(`(a) after instantiating module: ${mem()} ${linear()}`);

const tokPath = join(root, "storage", "en_tokenizer.bin");
const rulesPath = join(root, "storage", "en_rules.bin");
let tokenizerBytes = readFileSync(tokPath);
let rulesBytes = readFileSync(rulesPath);
console.log(`(b) after reading binaries (${tokenizerBytes.length} + ${rulesBytes.length} B): ${mem()} ${linear()}`);

const tConstruct = process.hrtime.bigint();
const checker = new Checker(tokenizerBytes, rulesBytes, spell);
const constructMs = Number(process.hrtime.bigint() - tConstruct) / 1e6;
console.log(`(c) after construction (${constructMs.toFixed(1)} ms): ${mem()} ${linear()}`);

// Callers are not expected to hold the binary bytes after construction; show what dropping them
// buys on the JS side.
tokenizerBytes = null;
rulesBytes = null;
if (typeof globalThis.gc === "function") globalThis.gc();
console.log(`    after dropping the JS buffers + gc: ${mem()} ${linear()}`);

const moby = join(root, "storage", "moby150k.txt");
if (!existsSync(moby)) {
  console.error(`note: ${moby} missing; skipping the check stage (see wasm/README.md)`);
  process.exit(0);
}
const text = readFileSync(moby, "utf8");
const tCheck = process.hrtime.bigint();
const json = checker.check(text);
const checkMs = Number(process.hrtime.bigint() - tCheck) / 1e6;
console.log(`(d) after checking moby150k (${JSON.parse(json).length} suggestions, ${checkMs.toFixed(1)} ms): ${mem()} ${linear()}`);

if (typeof globalThis.gc === "function") globalThis.gc();
console.log(`    after check + gc: ${mem()} ${linear()}`);
