// Tests for the WASM user-dictionary API (addWords/removeWords/setWords/words).
//
// Usage:
//   cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm
//   wasm-bindgen --target nodejs --out-dir wasm/pkg \
//     target/wasm32-unknown-unknown/release/writemorebetter_wasm.wasm
//   node wasm/test.mjs
//
// Requires the English binaries in storage/ (see wasm/README.md).

import { checkStyleRecords } from "./style-records-test.mjs";
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

// enableRules turns on off-by-default rules by selector; unknown selectors throw.
{
  const picky = new Checker(
    readFileSync(join(root, "storage", "en_tokenizer.bin")),
    readFileSync(join(root, "storage", "en_rules.bin")),
    undefined,
    "fiction",
  );
  const opinion = "😀 “It’s, in my opinion, wrong…” she said—quietly.\u2028It was a free gift.";
  const fired = (prefix) =>
    JSON.parse(picky.check(opinion)).filter((item) => item.source.startsWith(prefix));
  eq(fired("PLAIN_ENGLISH/"), [], "plain-English rules are off by default");
  check(picky.enableRules(["PLAIN_ENGLISH/IN_MY_OPINION", "REDUNDANCY"]) > 1, "enableRules counts");
  const hits = fired("PLAIN_ENGLISH/IN_MY_OPINION");
  eq(hits.length, 1, "enabled rule fires");
  check(
    hits.length === 1 && opinion.slice(hits[0].start16, hits[0].end16).includes("in my opinion"),
    `enabled rule has UTF-16 span: ${JSON.stringify(hits)}`,
  );
  check(fired("REDUNDANCY/").length > 0, "enabling a category the fiction profile disabled re-enables it");
  let error = "";
  try {
    picky.enableRules(["NO_SUCH_CATEGORY"]);
  } catch (err) {
    error = String(err);
  }
  check(error.includes("NO_SUCH_CATEGORY"), `unknown selector throws: ${error}`);
  eq(picky.enableRules([]), 0, "an empty list enables nothing");
  picky.free();
}

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

// check() also returns UTF-16 offsets (`start16`/`end16`) alongside the scalar fields.
// Both conventions must describe the same flagged text, including after astral characters.
function checkSpans(text) {
  const chars = [...text];
  return JSON.parse(checker.check(text)).map((item) => {
    const flagged = chars.slice(item.start, item.end).join("");
    check(flagged.length > 0, "flagged span is not empty");
    eq(text.slice(item.start16, item.end16), flagged, "start16/end16 match the scalar span");
    return { ...item, flagged };
  });
}

// 😀 adds 1 UTF-16 unit, a ZWJ family (👩‍👩‍👧) adds 3, and 🇺🇸 adds 2, so every later span shifts by 6.
const utf16Text = "😀\u{1F469}\u200D\u{1F469}\u200D\u{1F467}🇺🇸 “She was not been—again…”\u2028He was not been too…";
const utf16Spans = checkSpans(utf16Text);
const utf16Grammar = utf16Spans.filter((item) => item.flagged === "was not been");
eq(utf16Grammar.length, 2, "both grammar errors are flagged");
for (const item of utf16Grammar) {
  eq(item.start16, item.start + 6, "astral characters shift the UTF-16 start by their extra units");
  eq(item.end16, item.end + 6, "astral characters shift the UTF-16 end by their extra units");
}
check(utf16Grammar[1].start16 > utf16Grammar[0].end16,
  "a later span sits after an em dash, an ellipsis and U+2028");

// A flagged span can itself contain a non-ASCII character (here, a curly apostrophe).
const contractionSpans = checkSpans("😀 was’nt here—again…\u2028ok.");
const contraction = contractionSpans.find((item) => item.source.startsWith("TYPOS/INCORRECT_CONTRACTIONS"));
check(Boolean(contraction), "curly-apostrophe contraction is flagged");
if (contraction) {
  eq(contraction.flagged, "was’nt", "flagged span keeps its curly apostrophe");
  eq(contraction.start16, contraction.start + 1, "one astral character shifts the span by one unit");
}

// A span that ends at the very end of the text, after an emoji.
const endText = "😀 I saw teh";
const endSpan = checkSpans(endText).find((item) => item.flagged === "teh");
check(Boolean(endSpan), "a spelling error at text end is flagged");
if (endSpan) {
  eq([endSpan.start16, endSpan.end16], [9, 12], "end-of-text span uses UTF-16 offsets");
  eq(endSpan.end16, endText.length, "end-of-text end16 is the UTF-16 length");
}

// A flag (two regional indicators) at the end of the text.
const flagEndSpan = checkSpans("😀 He saw teh 🇺🇸").find((item) => item.flagged === "teh");
check(Boolean(flagEndSpan), "a text ending in a flag still reports spans");
if (flagEndSpan) {
  eq([flagEndSpan.start16, flagEndSpan.end16], [10, 13], "a trailing flag does not shift earlier spans");
}

// A leading BOM and CRLF line endings keep the reported offsets aligned.
const crlfBomGrammar = checkSpans("\uFEFF😀 She was not been here.\r\nHe was not been too.")
  .filter((item) => item.flagged === "was not been");
eq(crlfBomGrammar.length, 2, "BOM and CRLF text still finds both errors");
if (crlfBomGrammar.length === 2) {
  eq(crlfBomGrammar[1].start16, crlfBomGrammar[1].start + 1,
    "the BOM adds no unit and the emoji adds one");
}

const documentText = "😀 “Zoë’s goin’—again…”\u2028She stayed.\r\n\r\nZoë walked with O’Brien.";
const documentOptions = JSON.stringify({ skip_names: ["Zoë", "O'Brien"] });
const document = JSON.parse(checker.analyzeDocument(documentText, documentOptions));
const unskipped = JSON.parse(checker.analyzeDocument(documentText));
eq(JSON.parse(checker.analyzeDocument(documentText, "{}")), unskipped, "empty document options use defaults");
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
  if (!paragraph.text.includes("\u2028")) {
    eq(projected, JSON.parse(checker.analyze(paragraph.text)), "ordinary document paragraphs retain analysis fields");
  } else {
    eq(paragraph.sentences.length, 2, "terminal U+2028 separates document sentences");
  }
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
check(skippedTokens.some((token) => token.text === "Zoë"), "skip names preserve entered case");
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
    check((token.readings.some((reading) => reading.lemma === lemma) || (!token.readings.length && lemma === token.text.toLowerCase())), "lemma position resolves to a reading or surface fallback");
  }
}
for (const [paragraphIndex, paragraph] of document.paragraphs.entries()) {
  for (const [sentenceIndex, sentence] of paragraph.sentences.entries()) {
    for (const [tokenIndex, token] of sentence.tokens.entries()) {
      if (!token.is_word || token.is_skipped) continue;
      for (const lemma of new Set(token.readings.length ? token.readings.map((reading) => reading.lemma) : [token.text.toLowerCase()])) {
        const occurrences = document.lemma_index[lemma].filter((position) =>
          position.paragraph === paragraphIndex && position.sentence === sentenceIndex && position.token === tokenIndex);
        eq(occurrences.length, 1, "each eligible lemma position appears exactly once");
      }
    }
  }
}
eq(JSON.parse(checker.analyzeDocument(" \n\u2028")),
  { paragraphs: [], dialogue: [], lemma_index: {}, word_count: 0 }, "empty document");
const phraseDocument = JSON.parse(checker.analyzeDocument("Port Royal met Port.",
  JSON.stringify({ skip_names: ["Port Royal"] })));
const phraseTokens = phraseDocument.paragraphs.flatMap((paragraph) => paragraph.sentences.flatMap((sentence) => sentence.tokens));
eq(phraseTokens.filter((token) => token.is_word && token.is_skipped).map((token) => token.text),
  ["Port", "Royal"], "multi-token names excluded without skipping partial matches");
eq(phraseDocument.word_count, 2, "multi-token names excluded from denominator");
let invalidDocumentOptions = false;
try { checker.analyzeDocument("text", "invalid"); } catch { invalidDocumentOptions = true; }
check(invalidDocumentOptions, "invalid document options throw");
check(JSON.parse(checker.check("The the dog barked.")).length > 0, "check works after document analysis");
eq(JSON.parse(checker.analyze(prose)), analysis, "document analysis preserves tokenizer analysis");
eq(JSON.parse(checker.analyzeDocument(documentText, documentOptions)), document,
  "check and analyze preserve document analysis");

const namesText = "Will said he will go. Anna's hat. Anna’s coat. Charles' car.";
const namesDoc = JSON.parse(checker.analyzeDocument(namesText, JSON.stringify({skipNames: ["Will", "Anna", "Charles"]})));
const namesTokens = namesDoc.paragraphs.flatMap(p => p.sentences.flatMap(s => s.tokens));
check(namesTokens.find(t => t.text === "Will").is_skipped, "capitalized name skipped");
check(!namesTokens.find(t => t.text === "will").is_skipped, "modal will retained");
check(namesTokens.filter(t => ["'s", "’s", "'", "’"].includes(t.text)).every(t => t.is_skipped), "name possessives skipped");
const detached = JSON.parse(checker.analyzeDocument("Anna 's hat.", JSON.stringify({skipNames: ["Anna"]})));
eq(detached.word_count, 2, "detached clitic stays in denominator");
check(detached.paragraphs.flatMap(p => p.sentences.flatMap(s => s.tokens)).filter(t => t.is_whitespace || t.text === "'s").every(t => !t.is_skipped), "detached clitic and trailing whitespace retained");
const compact = JSON.parse(checker.analyzeDocument(documentText, JSON.stringify({skipNames: ["Zoë", "O'Brien"], includeTokenDetails: false})));
eq(compact.word_count, document.word_count, "compact counts unchanged");
eq(compact.lemma_index, document.lemma_index, "compact index unchanged");
check(compact.paragraphs.flatMap(p => p.sentences.flatMap(s => s.tokens)).every(t => !("readings" in t) && !("chunks" in t)), "compact output omits token details");
let unknownOption = false;
try { checker.analyzeDocument("text", '{"skpi_names":[]}'); } catch { unknownOption = true; }
check(unknownOption, "unknown document options rejected");
const unknownDocument = JSON.parse(checker.analyzeDocument("Zzqvorth sang. Zzqvorth sang."));
eq(unknownDocument.lemma_index.zzqvorth.length, 2, "unknown surface indexed");

const reportText = "😀.\u2028\n\n“She ran. She left. She slept. She wept.” “Fine,” he said. He grinned at her and he grinned again.";
const retained = checker.styleDocument(reportText);
const options = JSON.stringify({minimumSentences: 4});
const lengthReport = JSON.parse(retained.sentenceLength(options));
const startReport = JSON.parse(retained.sentenceStarts(options));
eq(lengthReport.sentences, startReport.sentences, "reports own identical sentence rows");
eq(lengthReport.sentences.filter(r => r.mode === "dialogue").length, 5, "sentence modes");
eq(lengthReport.groups.dialogue.histogram.find(b => b.words === 2).sentences, 4, "word histogram");
eq(startReport.groups.dialogue.categories.find(c => c.category === "subject").percent, 80, "subject percent");
const firstStart = startReport.starts[1];
eq(reportText.slice(firstStart.opener.start, firstStart.opener.end), "She", "UTF-16 opener after emoji");
check(typeof lengthReport.groups.all.meanDifference === "number", "bundled baseline by default");
eq(JSON.parse(retained.sentenceLength(options, "none")).groups.all.meanDifference, null, "baseline disabled per call");
const custom = JSON.parse(retained.sentenceLength(options, '{"mean":1}'));
eq(custom.groups.all.meanDifference, custom.groups.all.mean - 1, "custom baseline per call");
const calls = ["echoes", "repeatedPhrases", "cliches", "dialogueTags", "adverbs", "sentenceLength", "sentenceStarts", "sensoryBalance", "pacing", "overusedWords", "transitions", "pronounStarts", "emotionTells", "stats"];
for (const method of calls) {
  const fresh = checker.styleDocument(reportText);
  eq(JSON.parse(retained[method]()), JSON.parse(fresh[method]()), `shared analysis matches fresh ${method}`);
  fresh.free();
}
const stats = JSON.parse(retained.stats());
eq(JSON.parse(retained.echoes()).words, stats.words.all, "echo denominator");
eq(JSON.parse(retained.repeatedPhrases()).words, stats.words.all, "phrase denominator");
const tags = JSON.parse(retained.dialogueTags());
eq(reportText.slice(tags.tags[0].quotes[0].start, tags.tags[0].quotes[0].end), "“Fine,”", "UTF-16 tag quote");
const clicheText = "😀.\u2028She kicked the bucket.";
const cd = checker.styleDocument(clicheText);
const match = JSON.parse(cd.cliches()).matches[0];
eq(clicheText.slice(match.span.start, match.span.end), "kicked the bucket", "UTF-16 cliché after emoji");
cd.free();
const long = checker.styleDocument(Array(50).fill("word").join(" ") + ".");
eq(JSON.parse(long.sentenceLength()).groups.all.longSentences, [], "long lens opt-in");
eq(JSON.parse(long.sentenceLength('{"includeLongSentences":true}')).groups.all.longSentences, [0], "long lens selected");
long.free();
function styleError(call) { try { call(); } catch (error) { return error; } return null; }
const misspelt = styleError(() => retained.sentenceLength('{"minimumSentneces":1}'));
check(misspelt instanceof Error && misspelt.name === "StyleError", "typed report error");
eq(misspelt.kind, "invalidOptions", "unknown options rejected");
const invalid = styleError(() => retained.sentenceStarts(undefined, '{"universalPercent":{"Noun":20}}'));
eq(invalid.kind, "invalidBaseline", "invalid baseline rejected");
eq(invalid.report, "sentenceStarts", "error names report");
eq(styleError(() => checker.styleDocument("text", '{"typo":true}')).kind, "invalidOptions", "document options validated");
const compactStyle = checker.styleDocument(reportText, '{"includeTokenDetails":false}');
for (const method of ["echoes", "repeatedPhrases", "dialogueTags", "adverbs", "sentenceStarts", "sensoryBalance", "pacing", "overusedWords", "pronounStarts", "emotionTells"]) {
  eq(styleError(() => compactStyle[method]()).kind, "needsTokenDetails", `compact rejects ${method}`);
}
eq(styleError(() => compactStyle.diction(undefined, '{"hope":1.5}')).kind, "needsTokenDetails", "compact rejects diction");
eq(JSON.parse(compactStyle.sentenceLength()), JSON.parse(retained.sentenceLength()), "compact length identical");
eq(JSON.parse(compactStyle.transitions()), JSON.parse(retained.transitions()), "compact transitions identical");
eq(JSON.parse(compactStyle.cliches()), JSON.parse(retained.cliches()), "compact clichés identical");
compactStyle.free();
retained.free();
const spanText = "😀.\u2028He walked. She was walking. The old lamp glowed. The old lamp broke. The old lamp faded.";
const spanDocument = checker.styleDocument(spanText);
const echo = JSON.parse(spanDocument.echoes()).echoes.find(e => e.label === "walk");
eq(spanText.slice(echo.earlier.start, echo.earlier.end), "walked", "UTF-16 earlier echo after emoji");
eq(spanText.slice(echo.later.start, echo.later.end), "walking", "UTF-16 later echo after emoji");
const phrase = JSON.parse(spanDocument.repeatedPhrases()).phrases[0];
for (const span of phrase.spans) eq(spanText.slice(span.start, span.end), "The old lamp", "UTF-16 phrase after emoji");
spanDocument.free();
const senseText = "😀.\u2028The red lamp’s glow—a bell… “Perfume,” she said.";
const senseDocument = checker.styleDocument(senseText);
const sensory = JSON.parse(senseDocument.sensoryBalance('{"minimumSenseWords":1}'));
eq(sensory.senseWords.map(w => senseText.slice(w.span.start, w.span.end)), ["red", "glow", "bell", "Perfume"], "UTF-16 sense words after emoji");
eq(sensory.senseWords.map(w => w.modality), ["sight", "sight", "sound", "smell"], "sense modalities");
eq(sensory.senseWords[3].mode, "dialogue", "sense word mode");
const emotionText = "😀.\u2028She’d felt angry—then afraid… “I was so angry,” he said.";
const emotionDocument = checker.styleDocument(emotionText);
const emotions = JSON.parse(emotionDocument.emotionTells('{"includeDialogue":true}'));
eq(emotions.tells.map(t => emotionText.slice(t.span.start, t.span.end)), ["felt angry", "was so angry"], "UTF-16 emotion tells after emoji");
eq(emotions.tells.map(t => t.mode), ["narration", "dialogue"], "emotion tell modes");
eq(JSON.parse(emotionDocument.emotionTells()).tells.length, 1, "emotion tells skip dialogue by default");
eq(styleError(() => emotionDocument.emotionTells('{"includeDialog":true}')).kind, "invalidOptions", "emotion tells reject unknown options");
emotionDocument.free();
eq(sensory.groups.all.words, JSON.parse(senseDocument.stats()).words.all, "sensory denominator");
eq(styleError(() => senseDocument.sensoryBalance('{"minimumStrength":9}')).kind, "invalidOptions", "sensory options validated");
eq(styleError(() => senseDocument.sensoryBalance('{"singleSensePercent":85}')).kind, "invalidOptions", "removed singleSensePercent option rejected");
eq(sensory.scenes[0].singleSense, undefined, "scenes carry no singleSense flag");
const bundledDiction = JSON.parse(senseDocument.diction());
check(bundledDiction.tableWords > 14000, "diction without a table uses the bundled table");
eq(JSON.parse(senseDocument.diction('{"minimumRatedWords":1}')).tableWords, bundledDiction.tableWords, "options without a table use the bundled table");
eq(styleError(() => senseDocument.diction(undefined, "lamp")).kind, "invalidTable", "malformed table rejected");
const dictionReport = JSON.parse(senseDocument.diction('{"minimumRatedWords":1}', "lamp\t4.8\nbell\t4.5\n"));
eq(dictionReport.tableWords, 2, "TSV table parsed");
eq(dictionReport.sentences.find(s => s.ratedWords === 2).mean, 4.65, "sentence mean from caller table");
eq(JSON.parse(senseDocument.diction(undefined, '{"lamp":4.8}')).tableWords, 1, "JSON table parsed");
senseDocument.free();
const participleText = "😀.\u2028The floorboards were creaking. The clock kept ticking. Someone was knocking. The roses had withered. The lamp dimmed.";
const participleDocument = checker.styleDocument(participleText);
const participles = JSON.parse(participleDocument.sensoryBalance());
eq(participles.senseWords.map(w => participleText.slice(w.span.start, w.span.end)), ["creaking", "ticking", "knocking", "withered", "dimmed"], "participles retain eligible surface ratings and UTF-16 spans");
participleDocument.free();
const touchDocument = checker.styleDocument("He felt broken glass. He felt warm breath. She felt smooth stone. He felt sharp pain. He felt over the table. He felt ashamed. He felt obliged to go. She felt sure someone was watching. He felt alone in space.");
eq(JSON.parse(touchDocument.sensoryBalance()).senseWords.filter(w => w.lemma === "feel").length, 5, "object modifiers and groping preserve literal touch");
touchDocument.free();
const attributionDocument = checker.styleDocument("“I’m stiff,” she complained. “Listen,” she whispered.");
eq(JSON.parse(attributionDocument.sensoryBalance()).senseWords.filter(w => ["complain", "whisper"].includes(w.lemma)), [], "speech attributions do not count as sound");
attributionDocument.free();
for (const selector of [undefined, null, "bundled"]) {
  eq(JSON.parse(retainedBaseline(selector)), JSON.parse(retainedBaseline()), "absent/null/bundled baseline selects bundled");
}
function retainedBaseline(selector) {
  const d = checker.styleDocument("She ran. She waited.");
  try { return d.sentenceLength('{"minimumSentences":1}', selector); } finally { d.free(); }
}
const selectorDoc = checker.styleDocument("“Go,” she said. She ran.");
for (const method of ["sentenceLength", "sentenceStarts", "dialogueTags", "overusedWords"]) {
  const option = {dialogueTags: '{"minimumTags":1}', overusedWords: '{"minimumCount":1}'}[method] ?? '{"minimumSentences":1}';
  eq(selectorDoc[method](option, null), selectorDoc[method](option), `${method}: real null means bundled`);
  eq(selectorDoc[method](option, "bundled"), selectorDoc[method](option), `${method}: explicit bundled`);
  eq(styleError(() => selectorDoc[method](option, "null")).kind, "invalidBaseline", `${method}: JSON null is not a selector`);
  const bad = styleError(() => selectorDoc[method](option, "{"));
  eq(bad.kind, "invalidBaseline", `${method}: malformed baseline`);
  eq(bad.report, method, `${method}: malformed baseline names report`);
}
const noComparison = JSON.parse(selectorDoc.sentenceStarts('{"minimumSentences":1}', "none"));
check(noComparison.groups.all.universal.every(s => s.difference === null), "starts: explicit none");
const customTags = JSON.parse(selectorDoc.dialogueTags('{"minimumTags":1}', '{"saidAskedPercent":50}'));
eq(customTags.saidAsked.difference, 50, "tags: custom baseline");
eq(JSON.parse(selectorDoc.dialogueTags('{"minimumTags":1}', "none")).saidAsked.difference, null, "tags: explicit none");
selectorDoc.free();
const overusedText = "😀.\u2028She glanced up. She glanced away. She just glanced back, very slowly.";
const overusedDoc = checker.styleDocument(overusedText);
const overusedOptions = '{"minimumCount":3,"minimumRatio":2}';
const overused = JSON.parse(overusedDoc.overusedWords(overusedOptions, '{"lemmas":{"glance":100},"fillers":{"just":2000}}'));
eq(overused.overused.map(w => [w.lemma, w.count]), [["glance", 3]], "overused: custom table flags");
for (const span of overused.overused[0].spans) eq(overusedText.slice(span.start, span.end), "glanced", "overused: UTF-16 spans after emoji");
eq(overused.fillers.map(f => f.word).sort(), ["just", "very"], "overused: fillers");
eq(overused.fillers.find(f => f.word === "very").ratio, null, "overused: filler without reference");
const noTable = JSON.parse(overusedDoc.overusedWords(overusedOptions, "none"));
eq(noTable.overused, [], "overused: no table, no comparison");
eq(noTable.fillers.length, 2, "overused: fillers run without a table");
check(JSON.parse(overusedDoc.overusedWords()).fillers.every(f => typeof f.ratio === "number"), "overused: bundled filler rates");
eq(styleError(() => overusedDoc.overusedWords(undefined, '{"fillers":{"glance":1}}')).report, "overusedWords", "overused: filler keys validated");
overusedDoc.free();


const pacingText = "😀 She remembered that he had gone… She knew that he had been alone.\nShe wondered why he had forgotten. She had never understood why he had gone.\nShe recalled how he had known. She had imagined that he had remembered.";
const pacingDocument = checker.styleDocument(pacingText);
const pacingReport = JSON.parse(pacingDocument.pacing());
eq(pacingReport.slowRuns.length, 1, "backstory pacing run");
eq(pacingReport.slowRuns[0].paragraphs, {start: 0, end: 3}, "pacing row range");
eq(pacingText.slice(pacingReport.slowRuns[0].span.start, pacingReport.slowRuns[0].span.end), pacingText, "UTF-16 pacing run");
check(pacingReport.slowRuns[0].triggeredSignals.includes("pastPerfect"), "pacing explains past perfect");
eq(pacingReport.words, JSON.parse(pacingDocument.stats()).words, "pacing word convention");
eq(styleError(() => pacingDocument.pacing('{"minimumParagraphs":0}')).kind, "invalidOptions", "pacing options validated");
eq(styleError(() => pacingDocument.pacing('{"typo":0}')).kind, "invalidOptions", "pacing unknown options rejected");
pacingDocument.free();
const clarityText = "😀 It was as if he had been there for all of the time that she was in it with him. “It’s the man who had sailed every ocean on the map and carried letters across the cold northern sea for twenty long years,” she said—and smiled…";
const clarityDocument = checker.styleDocument(clarityText);
const clarity = JSON.parse(clarityDocument.clarity());
eq(clarity.stickySentences.length, 1, "sticky sentence flagged");
eq(clarityText.slice(clarity.stickySentences[0].glue[0].start, clarity.stickySentences[0].glue[0].end), "It", "UTF-16 glue span after emoji");
eq(clarity.longClauses.length, 1, "long clause flagged");
eq(clarityText.slice(clarity.longClauses[0].span.start, clarity.longClauses[0].span.end), "who had sailed every ocean on the map and carried letters across the cold northern sea for twenty long years", "UTF-16 clause span");
eq(clarity.longClauses[0].mode, "dialogue", "clause mode");
eq(clarity.words, JSON.parse(clarityDocument.stats()).words, "clarity word convention");
eq(styleError(() => clarityDocument.clarity('{"typo":0}')).kind, "invalidOptions", "clarity unknown options rejected");
eq(styleError(() => checker.styleDocument(clarityText, '{"includeTokenDetails":false}').clarity()).kind, "needsTokenDetails", "compact rejects clarity");
clarityDocument.free();

const lensText = "😀.\u2028“AS A RESULT, she’s tired—still… fine.”\u2028As a result, she left. As a result, she slept.\nShe was home. She was tired.\n\nShe was done.";
const lensDoc = checker.styleDocument(lensText);
const transitions = JSON.parse(lensDoc.transitions('{"minimumSentences":1}'));
eq(transitions.matches.length, 3, "transitions: multiword matches");
for (const match of transitions.matches) eq(lensText.slice(match.span.start, match.span.end).toLowerCase(), "as a result", "transitions: UTF-16 phrase span");
eq(transitions.repeats.length, 1, "transitions: no mode bridging");
eq(JSON.parse(lensDoc.transitions('{"repeatWindow":1}')).repeats.length, 0, "transitions: repeat window override");
eq(styleError(() => lensDoc.transitions('{"repeatWindow":0}')).kind, "invalidOptions", "transitions: zero window rejected");
const pronouns = JSON.parse(lensDoc.pronounStarts('{"minimumSentences":1}'));
eq(pronouns.groups.narration.runs.length, 1, "pronouns: run crosses paragraph");
eq(pronouns.groups.narration.runs[0].pronoun, "she", "pronouns: named run");
eq(pronouns.groups.all.pronounStarts.difference, null, "pronouns: no baseline");
check(pronouns.groups.all.lowConfidence, "pronouns: small sample marked");
eq(JSON.parse(lensDoc.pronounStarts()).groups.all.pronounStarts.percent, null, "pronouns: default sample floor");
eq(styleError(() => lensDoc.pronounStarts('{"typo":1}')).kind, "invalidOptions", "pronouns: unknown option");
eq(styleError(() => lensDoc.adverbs('{"typo":1}')).kind, "invalidOptions", "adverbs: unknown option");
lensDoc.free();
const pronounGuardDoc = checker.styleDocument("Her coat was red. Hers was blue. Nobody left. This was easy. That’s fine. One evening it rained. What’s more, she paid.");
eq(JSON.parse(pronounGuardDoc.pronounStarts()).starts.map(s => s.pronoun), ["hers", "nobody", "this", "that"], "pronouns: determiners, nominal one and what’s more excluded");
eq(JSON.parse(pronounGuardDoc.transitions()).matches[0].phrase, "what's more", "transitions: curly contraction phrase");
pronounGuardDoc.free();
const turnsDoc = checker.styleDocument("“She left,” I said.\n\n“She did,” he said.\n\n“She will,” I said.");
const turnPronouns = JSON.parse(turnsDoc.pronounStarts('{"minimumSentences":1}'));
eq([turnPronouns.starts.length, turnPronouns.groups.dialogue.runs.length], [3, 0], "pronouns: dialogue turns reset runs");
turnsDoc.free();
const repeatTurnsDoc = checker.styleDocument("“But I can’t,” she said.\n\n“But you must,” he said.\n\n“But why?” she asked.");
const turnTransitions = JSON.parse(repeatTurnsDoc.transitions());
eq([turnTransitions.matches.length, turnTransitions.repeats.length], [3, 0], "transitions: dialogue turns reset repeats");
repeatTurnsDoc.free();

const houseRules = readFileSync(join(root, "style", "tests", "fixtures", "house-style", "bundle.yml"), "utf8");
const houseText = "“Towards the door,” she said, walking towards it — sort of. 😀 An e-mail.";
const houseDoc = checker.styleDocument(houseText);
const house = JSON.parse(houseDoc.houseStyle(undefined, houseRules));
eq(house.rules, 4, "houseStyle: YAML bundle parsed");
eq(house.matches.map(m => m.rule), ["House.Toward", "House.Toward", "House.Hedging", "House.Toward"], "houseStyle: matches in text order");
eq(house.matches.map(m => houseText.slice(m.span.start, m.span.end)), ["Towards", "towards", "sort of", "e-mail"], "houseStyle: UTF-16 spans");
eq([house.matches[0].mode, house.matches[0].replacement, house.matches[0].level], ["dialogue", "toward", "error"], "houseStyle: match fields");
eq(house.groups.dialogue.errors, 1, "houseStyle: dialogue group");
const houseJson = JSON.stringify([{ name: "T.Very", extends: "existence", message: "Cut '%s'.", ignorecase: true, tokens: ["sort of"] }]);
eq(JSON.parse(houseDoc.houseStyle(undefined, houseJson)).matches[0].message, "Cut 'sort of'.", "houseStyle: JSON rules");
eq(JSON.parse(houseDoc.houseStyle('{"skipDialogue":true}', houseRules)).matches.length, 3, "houseStyle: skipDialogue");
eq(styleError(() => houseDoc.houseStyle('{"skipDialog":true}', houseRules)).kind, "invalidOptions", "houseStyle: options validated");
eq(styleError(() => houseDoc.houseStyle()).kind, "invalidRules", "houseStyle: rules required");
const badRule = styleError(() => houseDoc.houseStyle(undefined, '{"name":"X","extends":"existence","message":"m","tokens":["("]}'));
eq([badRule.kind, badRule.report], ["invalidRules", "houseStyle"], "houseStyle: invalid rule");
check(badRule.message.includes("rule X"), "houseStyle: error names the rule");
eq(house.unsupported, [], "houseStyle: no unsupported rules");
const lookaround = JSON.parse(houseDoc.houseStyle(undefined, JSON.stringify([
  { name: "T.Look", extends: "existence", message: "m", tokens: ["(?<!x)sort"] },
  { name: "T.Sort", extends: "existence", message: "m", tokens: ["sort of"] },
])));
eq([lookaround.unsupported.map(u => u.rule), lookaround.matches.map(m => m.rule)], [["T.Look"], ["T.Sort"]], "houseStyle: an unsupported rule is skipped alone");
houseDoc.free();

checkStyleRecords(checker);
const independent = checker.styleDocument("She ran.");
checker.free();
eq(JSON.parse(independent.sentenceStarts()).groups.all.sentences, 1, "document outlives checker");
independent.free();

if (failures > 0) {
  console.error(`\n${failures} assertion(s) failed`);
  process.exit(1);
}
console.log("wasm binding tests passed");
