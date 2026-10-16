import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
const require = createRequire(import.meta.url);
const { Checker } = require("./pkg/writemorebetter_wasm.js");
const checker = new Checker(readFileSync("storage/en_tokenizer.bin"), readFileSync("storage/en_rules.bin"));
const text = readFileSync("bench/public/moby150k.txt", "utf8");
const methods = ["echoes", "repeatedPhrases", "cliches", "dialogueTags", "sentenceLength", "sentenceStarts"];
const times = Object.fromEntries(["analysis", ...methods].map(k => [k, []]));
for (let i = 0; i < 4; i++) {
  let start = performance.now();
  const document = checker.styleDocument(text, '{"hardWrapped":true}');
  if (i) times.analysis.push(performance.now() - start);
  for (const method of methods) {
    start = performance.now();
    document[method]();
    if (i) times[method].push(performance.now() - start);
  }
  document.free();
}
console.log(JSON.stringify({inputBytes: Buffer.byteLength(text), warmups: 1, iterations: 3,
  medianMs: Object.fromEntries(Object.entries(times).map(([k,v]) => [k, v.sort((a,b) => a-b)[1]])),
  peakRssBytes: process.resourceUsage().maxRSS * 1024}, null, 2));
checker.free();
