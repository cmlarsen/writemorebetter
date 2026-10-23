//! Flags paragraphs whose Flesch reading ease is too high or too low.
//!
//! The behaviour follows LanguageTool 6.6's `ReadabilityRule`, which English registers twice:
//! `READABILITY_RULE_SIMPLE` (text too easy) and `READABILITY_RULE_DIFFICULT` (text too hard).
//! Both are text-level rules, off by default, in the `TEXT_ANALYSIS` category.
//!
//! For each LanguageTool paragraph (see [`crate::document`]) with at least 10 words, the rule
//! computes the Flesch reading ease `206.835 - 1.015 * words/sentences - 84.6 *
//! syllables/words` and its level, from 0 (below 30, very difficult) to 6 (90 and above, very
//! easy). `READABILITY_RULE_SIMPLE` flags a paragraph whose level is above the configured one,
//! `READABILITY_RULE_DIFFICULT` one whose level is below it; the default level is 3. Every
//! sentence counts, also one made only of whitespace (LanguageTool's sentence splitter keeps
//! `" \n\n"` after `"Short para. "` as a sentence of its own), and every token counts as a word
//! except a single punctuation mark. Syllables come from LanguageTool's vowel-group counter.
//! The match covers the paragraph's first words: from the first token of its first sentence
//! with a token to the third token of its first sentence with at least three.
//!
//! The paragraph matches are only reported when the score of the whole text crosses the same
//! threshold. For `READABILITY_RULE_SIMPLE` LanguageTool's condition also needs more than one
//! paragraph, so a one-paragraph text is never flagged as too simple; `READABILITY_RULE_DIFFICULT`
//! has no such condition (an operator-precedence quirk in LanguageTool, kept as is).
//!
//! Where LanguageTool's match would have no end (a paragraph of 10 or more words with no
//! sentence of three tokens, e.g. `"Yes. No. …"`), LanguageTool 6.6 throws and the whole check
//! fails; this rule reports nothing for that paragraph instead.

use crate::{
    document::{DocumentFix, DocumentMatch, LtParagraph, Offset},
    types::{Position, Sentence, Span},
    Error,
};
use serde::{Deserialize, Serialize};

/// The identifier of the rule that flags too easy paragraphs.
const SIMPLE_ID: &str = "READABILITY_RULE_SIMPLE";
/// The identifier of the rule that flags too difficult paragraphs.
const DIFFICULT_ID: &str = "READABILITY_RULE_DIFFICULT";

/// LanguageTool's default level, and the range its option allows.
const DEFAULT_LEVEL: usize = 3;
const LEVEL_RANGE: std::ops::RangeInclusive<usize> = 0..=6;

/// The fewest words a paragraph needs to be scored.
const MIN_WORDS: u32 = 10;

/// Flags paragraphs that read too easily or, with `too_easy` unset, too hard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Readability {
    too_easy: bool,
    level: usize,
}

impl Readability {
    /// `READABILITY_RULE_SIMPLE` with `too_easy`, `READABILITY_RULE_DIFFICULT` without.
    pub(crate) fn new(too_easy: bool) -> Self {
        Readability {
            too_easy,
            level: DEFAULT_LEVEL,
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        if self.too_easy {
            SIMPLE_ID
        } else {
            DIFFICULT_ID
        }
    }

    /// The level a paragraph is compared with, 0 (very difficult) to 6 (very easy).
    pub fn level(&self) -> usize {
        self.level
    }

    /// Sets the level a paragraph's level is compared with: `READABILITY_RULE_SIMPLE` flags
    /// paragraphs above it, `READABILITY_RULE_DIFFICULT` those below it. This is the option
    /// LanguageTool's rule offers, which allows 0 (very difficult) to 6 (very easy); the default
    /// is 3.
    ///
    /// # Errors
    /// [`Error::OptionOutOfRange`] if `level` is above 6; the rule keeps its previous value.
    pub fn set_level(&mut self, level: usize) -> Result<(), Error> {
        if !LEVEL_RANGE.contains(&level) {
            return Err(Error::OptionOutOfRange {
                option: "readability level",
                value: level,
                min: *LEVEL_RANGE.start(),
                max: *LEVEL_RANGE.end(),
            });
        }
        self.level = level;
        Ok(())
    }

    fn flags(&self, level: usize) -> bool {
        if self.too_easy {
            level > self.level
        } else {
            level < self.level
        }
    }

    fn message(&self, level: usize) -> String {
        let (simple, few) = if self.too_easy {
            ("simple", "few")
        } else {
            ("difficult", "many")
        };
        format!(
            "Readability: The text of this paragraph is too {} {{Level {}: {}}}. Too {} words \
             per sentence and too {} syllables per word.",
            simple, level, LEVEL_NAMES[level], few, few
        )
    }

    /// The matches in a text made of `paragraphs`, in text order. `lengths` are the lengths of
    /// the digests' texts, which bound a match that would reach into the next digest.
    pub(crate) fn resolve(
        &self,
        paragraphs: &[LtParagraph],
        lengths: &[Position],
    ) -> Vec<DocumentMatch> {
        let (mut sentences, mut words, mut syllables) = (0u64, 0u64, 0u64);
        for paragraph in paragraphs {
            sentences += u64::from(paragraph.sentences);
            words += u64::from(paragraph.words);
            syllables += u64::from(paragraph.syllables);
        }
        let level = reading_level(reading_ease(sentences, words, syllables));
        // LanguageTool's `nParagraph > 1 && tooEasyTest && rLevel > level || !tooEasyTest &&
        // rLevel < level`.
        if !(self.flags(level) && (!self.too_easy || paragraphs.len() > 1)) {
            return Vec::new();
        }

        paragraphs
            .iter()
            .filter(|paragraph| paragraph.words >= MIN_WORDS)
            .filter_map(|paragraph| {
                let level = reading_level(reading_ease(
                    u64::from(paragraph.sentences),
                    u64::from(paragraph.words),
                    u64::from(paragraph.syllables),
                ));
                if !self.flags(level) {
                    return None;
                }
                let (start, end) = (paragraph.mark_start?, paragraph.mark_end?);
                // A paragraph checked in pieces (e.g. one line at a time) can have its third
                // word in the next piece: the match then ends with the first one.
                let end = if end.digest == start.digest {
                    end.offset
                } else {
                    Offset::new(lengths[start.digest])
                };
                Some(DocumentMatch::new(
                    start.digest,
                    format!("BUILTIN/{}", self.id()),
                    self.message(level),
                    Span::from_positions(start.offset.position(), end.position()),
                    DocumentFix::Mark,
                ))
            })
            .collect()
    }
}

const LEVEL_NAMES: [&str; 7] = [
    "Very difficult",
    "Difficult",
    "Fairly difficult",
    "Medium",
    "Fairly easy",
    "Easy",
    "Very easy",
];

/// LanguageTool's English Flesch reading ease. With no words it is NaN, as in LanguageTool,
/// whose level is then 6.
fn reading_ease(sentences: u64, words: u64, syllables: u64) -> f64 {
    let asl = words as f64 / sentences as f64;
    let asw = syllables as f64 / words as f64;
    206.835 - (1.015 * asl) - (84.6 * asw)
}

/// The level of a reading ease, 0 (very difficult) to 6 (very easy).
fn reading_level(ease: f64) -> usize {
    [30.0, 50.0, 60.0, 70.0, 80.0, 90.0]
        .iter()
        .position(|limit| ease < *limit)
        .unwrap_or(6)
}

/// The words and syllables of `sentence` as the rule counts them: every token except a single
/// punctuation mark is a word.
pub(crate) fn count(sentence: &Sentence) -> (u32, u32) {
    let mut words = 0;
    let mut syllables = 0;
    for token in sentence.tokens() {
        let text = token.word().text().as_str();
        if !super::long_paragraph::is_non_word(text) {
            words += 1;
            syllables += syllable_count(text);
        }
    }
    (words, syllables)
}

/// LanguageTool's `simpleSyllablesCount`: vowel groups, with a few English diphthongs and
/// endings, over the UTF-16 units of `word` as in Java. At least 1 for a non-empty word.
pub(crate) fn syllable_count(word: &str) -> u32 {
    let units: Vec<u16> = word.encode_utf16().collect();
    let is = |unit: u16, chars: &str| chars.bytes().any(|c| u16::from(c) == unit);
    let is_vowel = |unit: u16| is(unit, "aeiouyAEIOUY");
    match units.len() {
        0 => return 0,
        1 => return 1,
        _ => {}
    }
    let mut count: i64 = 0;
    let mut last_double = false;
    for pair in units.windows(2) {
        let (c, next) = (pair[0], pair[1]);
        if !is_vowel(c) {
            last_double = false;
        } else if last_double {
            count += 1;
            last_double = false;
        } else if (is(c, "eE") && is(next, "aoeiy"))
            || (is(c, "aA") && is(next, "eiu"))
            || (is(c, "oO") && is(next, "oiua"))
            || (is(c, "uU") && is(next, "ia"))
            || (is(c, "iI") && is(next, "eo"))
        {
            last_double = true;
        } else {
            count += 1;
        }
    }
    let last = units[units.len() - 1];
    let before = units[units.len() - 2];
    if (is(before, "e") && is(last, "sd")) || (is(before, "u") && is(last, "e")) {
        count -= 1;
    } else if is_vowel(last) && !is(last, "e") {
        count += 1;
    }
    count.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_syllables_as_languagetool() {
        for (word, expected) in [
            ("", 0),
            ("a", 1),
            ("I", 1),
            ("cat", 1),
            ("the", 1),
            ("tea", 1),
            ("each", 1),
            ("eat", 1),
            ("simple", 1),
            ("readability", 5),
            ("institutional", 5),
            ("considerations", 5),
            ("played", 2),
            ("goes", 1),
            ("blue", 1),
            ("queue", 2),
            ("happy", 2),
            ("1999", 1),
            ("\u{2014}", 1),
            ("\u{2026}", 1),
            ("n't", 1),
            ("\u{00c9}mile", 1),
            // One astral char is two UTF-16 units, as in Java.
            ("\u{1F600}", 1),
        ] {
            assert_eq!(syllable_count(word), expected, "{:?}", word);
        }
    }

    #[test]
    fn levels() {
        assert_eq!(reading_level(29.9), 0);
        assert_eq!(reading_level(30.0), 1);
        assert_eq!(reading_level(69.9), 3);
        assert_eq!(reading_level(89.9), 5);
        assert_eq!(reading_level(90.0), 6);
        assert_eq!(reading_level(f64::NAN), 6);
        assert_eq!(reading_level(reading_ease(0, 0, 0)), 6);
    }

    #[test]
    fn level_option() {
        let mut rule = Readability::new(true);
        assert_eq!(rule.level(), 3);
        assert!(rule.set_level(7).is_err());
        assert_eq!(rule.level(), 3);
        rule.set_level(0).unwrap();
        assert_eq!(rule.level(), 0);
        assert_eq!(rule.id(), SIMPLE_ID);
        assert_eq!(Readability::new(false).id(), DIFFICULT_ID);
    }
}
