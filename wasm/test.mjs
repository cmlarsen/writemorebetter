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

if (failures > 0) {
  console.error(`\n${failures} assertion(s) failed`);
  process.exit(1);
}
console.log("wasm binding tests passed");
