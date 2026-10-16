import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

export function checkStyleRecords(checker) {
  const declarations = readFileSync(new URL("./pkg/writemorebetter_wasm.d.ts", import.meta.url), "utf8");
  const interfaces = new Map([...declarations.matchAll(/export interface (\w+)(?:<[^>]+>)?\s*\{([^}]+)\}/g)]
    .map(([, name, body]) => [name, [...body.matchAll(/(\w+):\s*([^;]+);/g)].map(([, key, type]) => [key, type.trim()])]));
  const aliases = new Map([...declarations.matchAll(/export type (\w+) = ([^;]+);/g)].map(([, name, type]) => [name, type]));
  const visited = new Set();
  function validate(value, type, path) {
    const union = type.split(" | ");
    if (union.length > 1) {
      if (value === null && union.includes("null")) return;
      if (union.every(t => t.startsWith('"'))) { assert(union.includes(JSON.stringify(value)), path); return; }
      return validate(value, union.filter(t => t !== "null").join(" | "), path);
    }
    if (["number", "string", "boolean"].includes(type)) { assert.equal(typeof value, type, path); return; }
    if (aliases.has(type)) return validate(value, aliases.get(type), path);
    const array = /^Array<(.+)>$/.exec(type);
    if (array) {
      assert(Array.isArray(value), path);
      value.forEach((item, index) => validate(item, array[1], `${path}[${index}]`));
      return;
    }
    const split = /^Split<(.+)>$/.exec(type);
    const name = split ? "Split" : type;
    const fields = interfaces.get(name);
    assert(fields, `unknown declaration ${type}`);
    assert(value && typeof value === "object" && !Array.isArray(value), path);
    assert.deepEqual(Object.keys(value).sort(), fields.map(([key]) => key).sort(), `${path} keys`);
    visited.add(name);
    for (const [key, fieldType] of fields) validate(value[key], split ? fieldType.replace(/\bT\b/g, split[1]) : fieldType, `${path}.${key}`);
  }
  const text = readFileSync(new URL("../style/tests/fixtures/binding-records.txt", import.meta.url), "utf8");
  const document = checker.styleDocument(text);
  const lengthOptions = '{"minimumSentences":1,"includeLongSentences":true}';
  const tagOptions = '{"minimumTags":1,"maxUntaggedExchanges":2}';
  const tagBaseline = '{"saidAskedPercent":50,"adverbPercent":10,"nonSpeechPercent":5}';
  const results = [
    ["DocumentStats", document.stats()],
    ["EchoReport", document.echoes()],
    ["RepeatedPhraseReport", document.repeatedPhrases()],
    ["ClicheReport", document.cliches()],
    ["DialogueTagReport", document.dialogueTags(tagOptions, tagBaseline)],
    ["SentenceLengthReport", document.sentenceLength(lengthOptions)],
    ["SentenceStartsReport", document.sentenceStarts('{"minimumSentences":1}')],
    ["OverusedWordsReport", document.overusedWords('{"minimumCount":1,"minimumRatio":1,"minimumReferencePerMillion":0,"includeNouns":true}')],
  ];
  const normText = readFileSync(new URL("../style/tests/fixtures/norm-records.txt", import.meta.url), "utf8");
  const normDocument = checker.styleDocument(normText);
  const table = '{"hope": 1.5, "idea": 1.6, "freedom": 1.3, "lamp": 4.8}';
  results.push(["SensoryBalanceReport", normDocument.sensoryBalance('{"minimumSenseWords":1}')]);
  results.push(["DictionReport", normDocument.diction(undefined, table)]);
  const pacingText = readFileSync(new URL("../style/tests/fixtures/pacing.txt", import.meta.url), "utf8");
  const pacingDocument = checker.styleDocument(pacingText);
  results.push(["PacingReport", pacingDocument.pacing()]);
  for (const [type, json] of results) validate(JSON.parse(json), type, type);
  normDocument.free();
  pacingDocument.free();
  assert.deepEqual([...visited].sort(), [...interfaces.keys()].sort(), "fixture exercises every declared result record");
  const stats = JSON.parse(document.stats());
  assert.throws(() => validate({ ...stats, bogus: true }, "DocumentStats", "negative control"));
  assert.throws(() => validate({ ...stats, paragraphs: "wrong" }, "DocumentStats", "negative control"));
  document.free();
}
