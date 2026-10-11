// Compare suggestion counts between the wasm (fancy-regex) build and the native onig build.
//
// Native side: bench/tools/wmb-probe with SPELL=en_US and SHOW=1 (its lines look like
//   "42 | GRAMMAR/WAS_BEEN/1 | \"was not been\" -> [\"was not\", \"has not been\"] | Did you mean ..."),
// parsed the same way as bench/tools/flags.py.
// Wasm side: wasm/pkg, speller en_US, suggestions as returned by Checker.check().
//
// Usage: node wasm/compare.mjs [file ...]
//   defaults to bench/public/story1..5.txt and storage/moby150k.txt (if present).

import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, basename } from "node:path";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { Checker } = require(join(here, "pkg", "writemorebetter_wasm.js"));
const probe = join(root, "bench", "tools", "wmb-probe", "target", "release", "wmb-probe");

let files = process.argv.slice(2).map((f) => [basename(f), f]);
if (files.length === 0) {
  files = [1, 2, 3, 4, 5].map((n) => [`story${n}`, join(root, "bench", "public", `story${n}.txt`)]);
  const moby = join(root, "storage", "moby150k.txt");
  if (existsSync(moby)) files.push(["moby150k", moby]);
}

// The binaries are read inline: they are copied into wasm linear memory by the constructor and
// should not be kept alive in JS afterwards (see wasm/README.md).
const checker = new Checker(
  readFileSync(join(root, "storage", "en_tokenizer.bin")),
  readFileSync(join(root, "storage", "en_rules.bin")),
  "en_US",
);

// Rust's `{:?}` escaping: \n \r \t \0 \\ \" \' and \u{XXXX}.
function unescapeRust(inner) {
  let out = "";
  for (let i = 0; i < inner.length; i++) {
    if (inner[i] !== "\\") {
      out += inner[i];
      continue;
    }
    const e = inner[++i];
    if (e === "u" && inner[i + 1] === "{") {
      const j = inner.indexOf("}", i);
      out += String.fromCodePoint(parseInt(inner.slice(i + 2, j), 16));
      i = j;
    } else if (e === "n") out += "\n";
    else if (e === "r") out += "\r";
    else if (e === "t") out += "\t";
    else if (e === "0") out += "\0";
    else out += e;
  }
  return out;
}

// "42 | RULE | \"flagged\" -> [\"a\", \"b\"] | message"
function parseNativeLine(line) {
  const p = line.split(" | ");
  if (p.length < 4 || !/^\d+$/.test(p[0])) return null;
  const m = /^(".*?"|'.*?') -> (\[.*\])$/.exec(p[2]);
  const flagged = unescapeRust((m ? m[1] : p[2]).slice(1, -1));
  const replacements = m
    ? [...m[2].matchAll(/"((?:\\.|[^"\\])*)"/g)].map((x) => unescapeRust(x[1]))
    : [];
  return { line: Number(p[0]), source: p[1], flagged, replacements, message: p[3] };
}

function native(file) {
  const out = spawnSync(probe, [join(root, "storage"), file], {
    env: { ...process.env, SPELL: "en_US", SHOW: "1" },
    encoding: "utf8",
  });
  if (out.status !== 0) throw new Error(`wmb-probe failed for ${file}: ${out.stderr}`);
  return out.stdout.split("\n").map(parseNativeLine).filter(Boolean);
}

function wasm(text) {
  const chars = [...text];
  const items = JSON.parse(checker.check(text));
  return items.map((s) => {
    let line = 1;
    for (let i = 0; i < s.start; i++) if (chars[i] === "\n") line++;
    return {
      line,
      source: s.source,
      flagged: chars.slice(s.start, s.end).join(""),
      replacements: s.replacements,
      message: s.message,
    };
  });
}

const key = (s) => `${s.line}|${s.source}|${s.flagged}|${JSON.stringify(s.replacements)}`;

let totalDifferences = 0;
for (const [name, path] of files) {
  const text = readFileSync(path, "utf8");
  const nat = native(path);
  const was = wasm(text);

  const counts = new Map();
  for (const s of nat) counts.set(key(s), (counts.get(key(s)) ?? 0) - 1);
  for (const s of was) counts.set(key(s), (counts.get(key(s)) ?? 0) + 1);
  const diffs = [...counts].filter(([, n]) => n !== 0);
  totalDifferences += diffs.length;

  console.log(
    `${name}: native(onig)=${nat.length} wasm(fancy)=${was.length} ` +
      `${nat.length === was.length ? "counts match" : `COUNT DIFF ${was.length - nat.length > 0 ? "+" : ""}${was.length - nat.length}`}` +
      `, differing suggestions=${diffs.length}`,
  );
  for (const [k, n] of diffs.slice(0, 10)) {
    const [line, source, flagged, repl] = k.split("|");
    console.log(`    ${n > 0 ? "+wasm" : "+native"} line ${line} ${source} ${JSON.stringify(flagged)} -> ${repl}`);
  }
  if (diffs.length > 10) console.log(`    ... and ${diffs.length - 10} more`);
}

console.log(totalDifferences === 0 ? "\nAll suggestion sets identical." : `\nTotal differing suggestion keys: ${totalDifferences}`);
