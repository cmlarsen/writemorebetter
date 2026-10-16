import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
const require = createRequire(import.meta.url);
const { Checker } = require("./pkg/writemorebetter_wasm.js");
const checker = new Checker(readFileSync("storage/en_tokenizer.bin"), readFileSync("storage/en_rules.bin"));
const excerpt = readFileSync("bench/public/moby150k.txt", "utf8");
const text = Array(4).fill(excerpt).join("\r\n\r\n");
const options = JSON.stringify({hardWrapped: true, includeTokenDetails: false});
const all = process.argv[2] === "--all";
const request = all ? undefined : JSON.stringify({sentenceLength: {}, sentenceStarts: {}, baselines: "bundled"});
checker.styleReports(text, request, options);
const timesMs = [];
let bytes = 0;
for (let i = 0; i < 3; i++) {
  const started = performance.now();
  const result = checker.styleReports(text, request, options);
  timesMs.push(performance.now() - started);
  const parsed = JSON.parse(result);
  if (!parsed.sentenceLength || !parsed.sentenceStarts) throw new Error("missing combined report");
  bytes = Buffer.byteLength(result);
}
console.log(JSON.stringify({reports: all ? "all" : "sentences", inputBytes: Buffer.byteLength(text), iterations: 3, warmups: 1, timesMs,
  medianMs: [...timesMs].sort((a,b) => a-b)[1], jsonBytes: bytes, peakRssBytes: process.resourceUsage().maxRSS * 1024}, null, 2));
checker.free();
