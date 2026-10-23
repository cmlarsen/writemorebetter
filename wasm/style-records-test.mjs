import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

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
    ["AdverbsReport", document.adverbs()],
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
  const transitionText = readFileSync(new URL("../style/tests/fixtures/transition-records.txt", import.meta.url), "utf8");
  const transitionDocument = checker.styleDocument(transitionText);
  results.push(["TransitionsReport", transitionDocument.transitions('{"minimumSentences":1}')]);
  results.push(["PronounStartsReport", transitionDocument.pronounStarts('{"minimumSentences":1}')]);
  const consistencyText = readFileSync(new URL("../style/tests/fixtures/consistency-records.txt", import.meta.url), "utf8");
  const consistencyDocument = checker.styleDocument(consistencyText);
  results.push(["ConsistencyReport", consistencyDocument.consistency()]);
  consistencyDocument.free();
  results.push(["ReadabilityReport", transitionDocument.readability('{"minimumSentences":1}')]);
  const passiveText = readFileSync(new URL("../style/tests/fixtures/passive-records.txt", import.meta.url), "utf8");
  const passiveDocument = checker.styleDocument(passiveText);
  results.push(["PassiveReport", passiveDocument.passive('{"minimumSentences":1}')]);
  results.push(["PassiveReport", passiveDocument.passive('{"minimumSentences":1}', "publicStandIn")]);
  assert.equal(JSON.parse(passiveDocument.passive('{"minimumSentences":1}')).groups.all.band, null);
  passiveDocument.free();
  const clarityText = readFileSync(new URL("../style/tests/fixtures/clarity.txt", import.meta.url), "utf8");
  const clarityDocument = checker.styleDocument(clarityText);
  results.push(["ClarityReport", clarityDocument.clarity('{"minimumSentences":1}')]);
  const houseRules = readFileSync(new URL("../style/tests/fixtures/house-style/bundle.yml", import.meta.url), "utf8");
  const houseDocument = checker.styleDocument("“Towards it,” she said, sort of. One, two, three, four, five.");
  const lookaround = '\n---\nname: House.Look\nextends: existence\nmessage: m\ntokens: ["(?<!x)b"]\n';
  results.push(["HouseStyleReport", houseDocument.houseStyle(undefined, houseRules + lookaround)]);
  houseDocument.free();
  const spellingText = readFileSync(new URL("../style/tests/fixtures/regional-spelling-records.txt", import.meta.url), "utf8");
  const spellingDocument = checker.styleDocument(spellingText);
  const spelling = spellingDocument.regionalSpelling();
  results.push(["RegionalSpellingReport", spelling]);
  assert.deepEqual(JSON.parse(spelling).flags.map(flag => [flag.form, flag.reason]), [["focused", "form"], ["color", "system"]]);
  spellingDocument.free();
  const emotionDocument = checker.styleDocument("She was afraid. “I was so angry,” he said.");
  results.push(["EmotionTellsReport", emotionDocument.emotionTells('{"includeDialogue":true}')]);
  emotionDocument.free();
  const marked = { start: 0, end: 2, voice: "excluded" };
  validate(marked, "MarkedSpan", "MarkedSpan"); // an input record: the options below send it
  const voiceDocument = checker.styleDocument("“Fine,” he thought. I’m done, she thought. She pressed “Send.” Keep moving, he said.",
    JSON.stringify({ markedSpans: [marked] }));
  results.push(["VoicesReport", voiceDocument.voices()]);
  voiceDocument.free();
  const driftDocument = checker.styleDocument("😀 He sat. He waited. … I crossed the room. She smiled. He nodded.");
  results.push(["PersonDriftReport", driftDocument.personDrift('{"person":"third"}')]);
  driftDocument.free();
  const vagueDocument = checker.styleDocument("It was sort of cold. “Well, it’s a nice thing,” she said. She made a decision after the arrival of the train.");
  results.push(["VagueWordsReport", vagueDocument.vagueWords()]);
  results.push(["HiddenVerbsReport", vagueDocument.hiddenVerbs()]);
  assert.deepEqual(JSON.parse(vagueDocument.hiddenVerbs()).matches.map(m => m.pattern), ["lightVerb", "ofPhrase"]);
  vagueDocument.free();
  const presetDocument = checker.styleDocument("“Mom, I saw 7 crows at 5 AM,” I said. James’ dad walked towards it—okay—“slowly”.");
  results.push(["PresetReport", presetDocument.preset()]);
  results.push(["PresetReport", presetDocument.preset("uk")]);
  results.push(["PresetChecksReport", presetDocument.presetChecks('{"numbers":{"spellOutThrough":9},"familyTitles":true}')]);
  assert.equal(JSON.parse(presetDocument.preset()).checks.matches.length, 4);
  presetDocument.free();
  const alliterationDocument = checker.styleDocument("“Please put the pail down,” Peter pleaded patiently.");
  results.push(["AlliterationReport", alliterationDocument.alliteration()]);
  alliterationDocument.free();
  const { titleCase } = createRequire(import.meta.url)("./pkg/writemorebetter_wasm.js");
  results.push(["TitleCaseReport", titleCase('["Gone With The Wind", "the long way home"]')]);
  for (const [type, json] of results) validate(JSON.parse(json), type, type);
  transitionDocument.free();
  normDocument.free();
  pacingDocument.free();
  clarityDocument.free();
  assert.deepEqual([...visited].sort(), [...interfaces.keys()].sort(), "fixture exercises every declared result record");
  const stats = JSON.parse(document.stats());
  assert.throws(() => validate({ ...stats, bogus: true }, "DocumentStats", "negative control"));
  assert.throws(() => validate({ ...stats, paragraphs: "wrong" }, "DocumentStats", "negative control"));
  document.free();
}
