// Smoke test for the browser (`wasm-bindgen --target web`) build that the Release workflow
// publishes as writemorebetter-web-<version>.zip. It loads the package the way a web consumer
// does (the default-export init with `module_or_path`) and calls the API surface a browser
// app depends on, failing on any throw or malformed result.
//
//   node wasm/web-smoke.mjs <dir with writemorebetter_wasm.js and _bg.wasm> <data dir>
//
// It prints a stable summary on stdout, so two builds (for example before and after
// wasm-opt) can be compared with diff.
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [pkgDir, dataDir] = process.argv.slice(2).map((p) => p && resolve(p));
if (!pkgDir || !dataDir) {
  console.error("usage: node wasm/web-smoke.mjs <pkg dir> <data dir>");
  process.exit(2);
}

const wasm = await import(pathToFileURL(join(pkgDir, "writemorebetter_wasm.js")).href);
await wasm.default({ module_or_path: readFileSync(join(pkgDir, "writemorebetter_wasm_bg.wasm")) });

function assert(condition, message) {
  if (!condition) {
    console.error(`web-smoke: ${message}`);
    process.exit(1);
  }
}

const checker = new wasm.Checker(
  readFileSync(join(dataDir, "en_tokenizer.bin")),
  readFileSync(join(dataDir, "en_rules.bin")),
  "en_US",
  "fiction",
);

const added = checker.setWords(["Zarneth"]);
assert(added === 1, `setWords returned ${added}, expected 1`);

// Curly quotes, an em dash, an ellipsis and U+2028, plus one misspelling to find.
const text = "“She dont know,” Zarneth said — he was tired… They went hoem.";
const suggestions = JSON.parse(checker.check(text));
assert(Array.isArray(suggestions), "check did not return a JSON array");
assert(suggestions.length > 0, "check found nothing in a sentence with a misspelling");
console.log("check", JSON.stringify(suggestions));

const doc = checker.styleDocument("He walked slowly. He walked quickly. He walked very slowly to the door.");
const reports = [
  "cliches",
  "echoes",
  "adverbs",
  "dialogueTags",
  "emotionTells",
  "sentenceStarts",
  "passive",
  "vagueWords",
  "preset",
];
for (const name of reports) {
  assert(typeof doc[name] === "function", `StyleDocument has no ${name}()`);
  const report = doc[name]();
  assert(typeof report === "string", `${name}() returned ${typeof report}, expected a JSON string`);
  JSON.parse(report);
  console.log(name, report);
}
doc.free();
checker.free();
console.log("ok");
