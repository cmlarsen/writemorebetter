import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";

const require = createRequire(import.meta.url);
const { Checker } = require("./pkg/writemorebetter_wasm.js");
const path = process.argv[2];
if (!path) throw new Error("usage: node --expose-gc wasm/document-bench.mjs TEXT [--compact] [--hard-wrapped]");
const text = readFileSync(path, "utf8");
const checker = new Checker(
  readFileSync(fileURLToPath(new URL("../storage/en_tokenizer.bin", import.meta.url))),
  readFileSync(fileURLToPath(new URL("../storage/en_rules.bin", import.meta.url))),
  undefined, "fiction",
);
const options = {
  include_token_details: !process.argv.includes("--compact"),
  hard_wrapped: process.argv.includes("--hard-wrapped"),
};
global.gc?.();
const loadedRSS = process.memoryUsage().rss;
const runs = [];
for (let run = 0; run < 3; run++) {
  const started = performance.now();
  const json = checker.analyzeDocument(text, JSON.stringify(options));
  const elapsed = performance.now() - started;
  const rssBeforeParse = process.memoryUsage().rss;
  const document = JSON.parse(json);
  runs.push({ milliseconds: elapsed, json_bytes: Buffer.byteLength(json), rss_before_parse: rssBeforeParse,
    rss_after_parse: process.memoryUsage().rss, words: document.word_count, paragraphs: document.paragraphs.length });
}
console.log(JSON.stringify({ bytes: Buffer.byteLength(text), options, loaded_rss: loadedRSS,
  peak_rss: process.resourceUsage().maxRSS * 1024, runs }, null, 2));
checker.free();
