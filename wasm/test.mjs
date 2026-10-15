// Tests for the WASM user-dictionary API (addWords/removeWords/setWords/words).
//
// Usage:
//   cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
//   wasm-bindgen --target nodejs --out-dir wasm/pkg \
//     target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
//   node wasm/test.mjs
//
// Requires the English binaries in storage/ (see wasm/README.md).

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { Checker } = require(join(here, "pkg", "writemorebetter_wasm.js"));

const checker = new Checker(
  readFileSync(join(root, "storage", "en_tokenizer.bin")),
  readFileSync(join(root, "storage", "en_rules.bin")),
  "en_US",
);

let failures = 0;
function check(condition, message) {
  if (!condition) {
    failures += 1;
    console.error(`FAIL: ${message}`);
  }
}

// The spelling flags of `text`, as the flagged substrings.
function flags(text) {
  const chars = [...text];
  return JSON.parse(checker.check(text))
    .filter((item) => item.source.startsWith("SPELLCHECK"))
    .map((item) => chars.slice(item.start, item.end).join(""));
}

function eq(actual, expected, message) {
  check(
    JSON.stringify(actual) === JSON.stringify(expected),
    `${message}: got ${JSON.stringify(actual)}, want ${JSON.stringify(expected)}`,
  );
}

// A word the base dictionary does not know is flagged, and addWords suppresses it.
eq(flags("The Xylophonix arrived."), ["Xylophonix"], "invented name is flagged");
check(checker.addWords(["Xylophonix"]) === 1, "addWords returns the number added");
eq(flags("The Xylophonix arrived."), [], "added name is accepted");
eq(flags("Xylophonix's wand. Xylophonix\u2019s wand."), [], "possessives accepted");

// A lowercase entry also accepts the sentence-initial capitalized form; a capitalized entry
// does not make the lowercase form valid.
checker.setWords(["krelltarn"]);
eq(flags("Krelltarn arrived."), [], "lowercase entry accepts capitalized form");
eq(flags("the krelltarn"), [], "lowercase entry accepts itself");
checker.setWords(["Krelltarn"]);
eq(flags("the krelltarn"), ["krelltarn"], "capitalized entry does not accept lowercase");

// Accented names, internal apostrophes, em dashes and hyphenated coinages.
checker.setWords(["Zoë", "Ngũgĩ", "Zar'khan", "Marie-Claire"]);
eq(flags("Zoë met Ngũgĩ."), [], "accented names accepted");
eq(flags("Zar\u2019khan and Za" + "r'khan spoke."), [], "internal apostrophes accepted");
eq(flags("Ngũgĩ\u2014then she left."), [], "name next to an em dash accepted");
eq(flags("Marie-Claire and Marie-Claire's hat."), [], "hyphenated coinage accepted");

// Multi-word entry.
checker.setWords(["Xyzzy Plugh"]);
eq(flags("They reached Xyzzy Plugh at dusk."), [], "multi-word entry accepted");

// Added words are suggestion candidates for near misses.
checker.setWords(["Xylophonix"]);
const typo = JSON.parse(checker.check("the Xylophoni arrived")).find((item) =>
  item.source.startsWith("SPELLCHECK"),
);
check(
  typo && typo.replacements.includes("Xylophonix"),
  `typo suggests the added word: ${JSON.stringify(typo?.replacements)}`,
);

// words() lists canonicalized, sorted entries; removeWords and setWords update it.
checker.setWords(["Xylophonix", "O\u2019Brien", "  "]);
eq(JSON.parse(checker.words()), ["O'Brien", "Xylophonix"], "words() is canonical and sorted");
check(checker.addWords(["Xylophonix"]) === 0, "re-adding is not counted");
eq(checker.removeWords(["Xylophonix", "Absent"]), 1, "removeWords counts removals");
eq(JSON.parse(checker.words()), ["O'Brien"], "removeWords updates the list");
eq(checker.setWords(["Zoë", "Ngũgĩ"]), 2, "setWords replaces the list");
eq(flags("Xylophonix arrived."), ["Xylophonix"], "setWords drops the old entries");

// False friends run only while their mother tongue is selected.
function falseFriends(text) {
  return JSON.parse(checker.check(text))
    .filter((item) => item.source.startsWith("FALSE_FRIENDS/"))
    .map((item) => item.source);
}
const actual = "“It’s the actual problem—again…”";
check(JSON.parse(checker.motherTongues()).includes("de"), "motherTongues lists de");
eq(falseFriends(actual), [], "no false friends without a mother tongue");
checker.setMotherTongue("de-DE");
eq(falseFriends(actual), ["FALSE_FRIENDS/ACTUAL/0"], "setMotherTongue selects its rules");
checker.setMotherTongue("");
eq(falseFriends(actual), [], "an empty mother tongue clears the selection");
checker.setMotherTongue("de");
checker.setMotherTongue(undefined);
eq(falseFriends(actual), [], "undefined clears the selection");

// Analysis uses UTF-16 offsets directly with String.slice, including after emoji.
const prose = "😀 “She’s goin’—again…”\u2028\n\nHe stayed.";
const analysis = JSON.parse(checker.analyze(prose));
check(analysis.length > 0, "analysis has sentences");
for (const sentence of analysis) {
  eq(prose.slice(sentence.start, sentence.end), sentence.text, "sentence UTF-16 span");
  eq(sentence.tokens.map((token) => token.text).join(""), sentence.text, "tokens partition sentence");
  let cursor = sentence.start;
  for (const token of sentence.tokens) {
    eq(token.start, cursor, "tokens are contiguous");
    check(token.end > token.start, "no synthetic empty tokens");
    eq(prose.slice(token.start, token.end), token.text, "token UTF-16 span");
    cursor = token.end;
  }
  eq(cursor, sentence.end, "tokens cover sentence");
}
const analyzedTokens = analysis.flatMap((sentence) => sentence.tokens);
check(analyzedTokens.some((token) => token.is_whitespace), "whitespace is exposed");
check(analyzedTokens.find((token) => token.text === "—").is_punctuation, "punctuation is exposed");
eq(analyzedTokens.find((token) => token.text === "goin’").readings,
  [{ lemma: "go", pos: "VBG" }], "original g-dropped surface, LT66 reading");
eq(JSON.parse(checker.analyze(" \n\u2028")), [], "whitespace-only analysis");
check(JSON.parse(checker.check("The the dog barked.")).length > 0, "check works after analysis");
eq(JSON.parse(checker.analyze(prose)), analysis, "check does not mutate analysis");

// check() keeps its existing Unicode scalar convention, independently of analyze().
const checkProbe = "😀 She was not been here since Monday.";
const grammarHit = JSON.parse(checker.check(checkProbe)).find((item) =>
  item.replacements.includes("has not been"));
check(Boolean(grammarHit), "check still finds the reference grammar error");
if (grammarHit) {
  eq([...checkProbe].slice(grammarHit.start, grammarHit.end).join(""), "was not been",
    "check offsets remain Unicode scalar values");
}

const documentText = "😀 “Zoë’s goin’—again…”\u2028She stayed.\r\n\r\nZoë walked with O’Brien.";
const documentOptions = JSON.stringify({ skip_names: ["ZOË", "zoë's", "o'brien"] });
const document = JSON.parse(checker.analyze_document(documentText, documentOptions));
const unskipped = JSON.parse(checker.analyze_document(documentText));
eq(JSON.parse(checker.analyze_document(documentText, "{}")), unskipped, "empty document options use defaults");
eq(document.paragraphs.length, 2, "document paragraph boundaries");
eq(document.dialogue.map((span) => documentText.slice(span.start, span.end)),
  ["“Zoë’s goin’—again…”"], "dialogue UTF-16 span");
const documentTokens = document.paragraphs.flatMap((paragraph) => paragraph.sentences.flatMap((sentence) => sentence.tokens));
for (const paragraph of document.paragraphs) {
  eq(documentText.slice(paragraph.start, paragraph.end), paragraph.text, "paragraph UTF-16 span");
  const projected = paragraph.sentences.map((sentence) => ({
    text: sentence.text, start: sentence.start - paragraph.start, end: sentence.end - paragraph.start,
    tokens: sentence.tokens.map(({ is_word, is_skipped, ...token }) => ({
      ...token, start: token.start - paragraph.start, end: token.end - paragraph.start,
    })),
  }));
  eq(projected, JSON.parse(checker.analyze(paragraph.text)), "document retains every analysis field");
  for (const sentence of paragraph.sentences) {
    eq(documentText.slice(sentence.start, sentence.end), sentence.text, "document sentence UTF-16 span");
    eq(sentence.tokens.map((token) => token.text).join(""), sentence.text, "document tokens partition sentence");
    for (const token of sentence.tokens) {
      eq(documentText.slice(token.start, token.end), token.text, "document token UTF-16 span");
      check(Array.isArray(token.chunks) && Array.isArray(token.readings), "document exposes chunks and readings");
      eq(token.is_word, /[\p{L}\p{N}]/u.test(token.text), "document word flag");
    }
  }
}
const skippedTokens = documentTokens.filter((token) => token.is_skipped);
check(skippedTokens.some((token) => token.text === "Zoë"), "skip names ignore case");
check(skippedTokens.some((token) => token.text === "O’Brien"), "skip names normalize apostrophes");
eq(document.word_count, documentTokens.filter((token) => token.is_word && !token.is_skipped).length,
  "word count excludes skipped names");
eq(unskipped.word_count - document.word_count, skippedTokens.filter((token) => token.is_word).length,
  "skip names reduce denominator");
check(documentTokens.some((token) => token.is_whitespace), "document whitespace exposed");
check(documentTokens.find((token) => token.text === "—").is_punctuation, "document punctuation exposed");
check(documentTokens.find((token) => token.text === "goin’").readings.some((reading) =>
  reading.lemma === "go" && reading.pos === "VBG"), "document preserves tagged readings");
check(Object.keys(document.lemma_index).length > 0, "document lemma index populated");
for (const [lemma, positions] of Object.entries(document.lemma_index)) {
  for (const position of positions) {
    const token = document.paragraphs[position.paragraph].sentences[position.sentence].tokens[position.token];
    check(token.is_word && !token.is_skipped, "lemma positions exclude skipped names");
    check(token.readings.some((reading) => reading.lemma === lemma), "lemma position resolves to its reading");
  }
}
for (const [paragraphIndex, paragraph] of document.paragraphs.entries()) {
  for (const [sentenceIndex, sentence] of paragraph.sentences.entries()) {
    for (const [tokenIndex, token] of sentence.tokens.entries()) {
      if (!token.is_word || token.is_skipped) continue;
      for (const lemma of new Set(token.readings.map((reading) => reading.lemma))) {
        const occurrences = document.lemma_index[lemma].filter((position) =>
          position.paragraph === paragraphIndex && position.sentence === sentenceIndex && position.token === tokenIndex);
        eq(occurrences.length, 1, "each eligible lemma position appears exactly once");
      }
    }
  }
}
eq(JSON.parse(checker.analyze_document(" \n\u2028")),
  { paragraphs: [], dialogue: [], lemma_index: {}, word_count: 0 }, "empty document");
const phraseDocument = JSON.parse(checker.analyze_document("Port Royal met Port.",
  JSON.stringify({ skip_names: ["port royal"] })));
const phraseTokens = phraseDocument.paragraphs.flatMap((paragraph) => paragraph.sentences.flatMap((sentence) => sentence.tokens));
eq(phraseTokens.filter((token) => token.is_word && token.is_skipped).map((token) => token.text),
  ["Port", "Royal"], "multi-token names excluded without skipping partial matches");
eq(phraseDocument.word_count, 2, "multi-token names excluded from denominator");
let invalidDocumentOptions = false;
try { checker.analyze_document("text", "invalid"); } catch { invalidDocumentOptions = true; }
check(invalidDocumentOptions, "invalid document options throw");
check(JSON.parse(checker.check("The the dog barked.")).length > 0, "check works after document analysis");
eq(JSON.parse(checker.analyze(prose)), analysis, "document analysis preserves tokenizer analysis");
eq(JSON.parse(checker.analyze_document(documentText, documentOptions)), document,
  "check and analyze preserve document analysis");

if (failures > 0) {
  console.error(`\n${failures} assertion(s) failed`);
  process.exit(1);
}
console.log("wasm binding tests passed");
