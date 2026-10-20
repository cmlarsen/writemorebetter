//! Narrow English confusion patterns adapted from Harper (Apache-2.0).
//!
//! These patterns use word tokens and their existing spans. No LanguageTool
//! rule data is embedded in the engine. Each pattern is narrower than
//! Harper's so that ordinary fiction narration is not flagged; a line break
//! (`\n`, `\r`, U+2028, U+2029) between two tokens ends a match.

use crate::types::{Sentence, Suggestion};
use serde::{Deserialize, Serialize};

/// A Harper confusion pair checked as a builtin rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HarperConfusion {
    /// `a waist of time` → `waste`.
    WaistWaste,
    /// `right bellow the surface` → `below`.
    BellowBelow,
    /// `please fell free` → `feel`.
    FeelFell,
    /// `couldn't quiet understand` → `quite`, `very quite` → `quiet`.
    QuiteQuiet,
}

/// Adjectives Harper accepts directly before `waste of`.
const WASTE_ADJECTIVES: &[&str] = &[
    "complete", "great", "huge", "total", "utter", "real", "horrible",
];

/// Words that precede `a waste of`.
const WASTE_LEADS: &[&str] = &[
    "is", "was", "'s", "are", "were", "be", "been", "just", "really", "such", "what",
];

/// Nouns that complete `waste of`.
const WASTE_NOUNS: &[&str] = &["effort", "money", "resources", "space", "time"];

const POSSESSIVES: &[&str] = &["my", "your", "his", "her", "its", "our", "their"];

/// Possessive pronouns that open `Hers was a waist of…`, where *waist* is meant.
const POSSESSIVE_PRONOUNS: &[&str] = &["mine", "yours", "his", "hers", "ours", "theirs"];

const NUMBERS: &[&str] = &[
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "twenty", "thirty", "forty", "fifty", "hundred", "thousand",
];

/// Words after which `bellow` sits in a position that only `below` fills:
/// degree adverbs, forms of *be*, motion and position verbs, and `from`.
const BELOW_LEADS: &[&str] = &[
    "right",
    "far",
    "directly",
    "slightly",
    "deep",
    "way",
    "well",
    "somewhere",
    "from",
    "is",
    "was",
    "'s",
    "are",
    "were",
    "fell",
    "sank",
    "sunk",
    "dropped",
    "went",
    "descended",
    "dove",
    "dived",
    "lay",
    "lies",
    "lying",
    "hid",
    "hidden",
];

/// Words after `bellow` that start a noun phrase. `that` is left out: after
/// the verb it introduces a clause (`heard him bellow that the ship…`).
const BELOW_OBJECTS: &[&str] = &[
    "a", "an", "the", "these", "those", "my", "your", "his", "her", "its", "our", "their",
];

/// Words after which `fell` can only be a base-form verb.
const BASE_VERB_LEADS: &[&str] = &[
    "to", "please", "can", "could", "will", "would", "should", "might", "must", "may", "'ll", "'d",
    "do", "does", "did", "n't",
];

/// Words after a base-form `fell` that only *feel* takes. Pronouns and
/// determiners are left out because *fell* (cut down) takes an object.
const FEEL_COMPLEMENTS: &[&str] = &[
    "comfortable",
    "free",
    "good",
    "better",
    "bad",
    "worse",
    "well",
    "right",
    "sick",
    "safe",
    "sorry",
    "fine",
    "guilty",
    "terrible",
    "awful",
    "strange",
    "happy",
    "sad",
];

/// Intensifiers that never precede `quite`, so `quiet` was meant.
const QUIET_INTENSIFIERS: &[&str] = &[
    "very",
    "extremely",
    "eerily",
    "deathly",
    "strangely",
    "oddly",
    "unusually",
    "suspiciously",
];

/// Words after which `quiet <adjective>` reads as `quite <adjective>`. `'s`
/// is left out because it is also the possessive (`the captain's quiet
/// steady voice`); it counts only after a pronoun in `QUITE_SUBJECTS`.
const QUITE_LEADS: &[&str] = &["is", "was", "are", "were", "'re", "'m", "am", "it"];

/// Subjects after which `'s` is *is* or *has*, never the possessive.
const QUITE_SUBJECTS: &[&str] = &["it", "that", "he", "she", "there", "what", "who"];

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

impl HarperConfusion {
    /// The rule ID, e.g. `HARPER_FEEL_FELL`.
    pub fn id(self) -> &'static str {
        match self {
            Self::WaistWaste => "HARPER_WAIST_WASTE",
            Self::BellowBelow => "HARPER_BELLOW_BELOW",
            Self::FeelFell => "HARPER_FEEL_FELL",
            Self::QuiteQuiet => "HARPER_QUITE_QUIET",
        }
    }

    /// Checks one sentence.
    pub fn check(self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let offset = sentence.span().byte().start;
        // The number of line breaks before each token, so that tokens on
        // different lines never count as adjacent.
        let mut lines = Vec::with_capacity(tokens.len());
        let mut line = 0;
        for (i, token) in tokens.iter().enumerate() {
            if i > 0 {
                let start = tokens[i - 1].span().byte().end.saturating_sub(offset);
                let end = token.span().byte().start.saturating_sub(offset);
                if sentence
                    .text()
                    .get(start..end)
                    .is_some_and(|gap| gap.chars().any(is_line_break))
                {
                    line += 1;
                }
            }
            lines.push(line);
        }
        let mut found = Vec::new();

        for (i, token) in tokens.iter().enumerate() {
            // The word at `j`, or "" past the sentence or a line break.
            let word = |j: usize| {
                if lines.get(j) == Some(&lines[i]) {
                    tokens[j].word().text().as_str()
                } else {
                    ""
                }
            };
            let is = |j: usize, expected: &str| word(j).eq_ignore_ascii_case(expected);
            let any = |j: usize, list: &[&str]| list.iter().any(|w| is(j, w));
            let before = |n: usize, list: &[&str]| i >= n && any(i - n, list);
            let has_tag = |j: usize, pred: &dyn Fn(&str) -> bool| {
                !word(j).is_empty()
                    && tokens[j]
                        .word()
                        .tags()
                        .iter()
                        .any(|tag| pred(tag.pos().as_str()))
            };
            let is_number = |j: usize| {
                any(j, NUMBERS) || word(j).chars().next().is_some_and(|c| c.is_ascii_digit())
            };

            let replacement = match self {
                Self::WaistWaste if is(i, "waist") && is(i + 1, "of") => {
                    let adjective = before(1, WASTE_ADJECTIVES) && before(2, &["a"]);
                    // `Hers was a waist of twenty inches` describes a waist.
                    let lead = before(1, &["a"])
                        && before(2, WASTE_LEADS)
                        && !before(3, POSSESSIVE_PRONOUNS)
                        && !is_number(i + 2);
                    let noun = any(i + 2, WASTE_NOUNS)
                        || (any(i + 2, POSSESSIVES) && any(i + 3, WASTE_NOUNS));
                    // `noun` alone would match compounds like `the waist of
                    // her money belt`, so it still needs `a` before `waist`.
                    (adjective || lead || (noun && before(1, &["a"]))).then_some("waste")
                }
                Self::BellowBelow if is(i, "bellow") => {
                    let object = any(i + 1, BELOW_OBJECTS) || is_number(i + 1);
                    // `might as well bellow the answer` is the verb.
                    let as_well = before(1, &["well"]) && before(2, &["as"]);
                    (object && before(1, BELOW_LEADS) && !as_well).then_some("below")
                }
                Self::FeelFell if is(i, "fell") => {
                    let base = before(1, BASE_VERB_LEADS);
                    // `Fell free to…` opening a sentence or a line of dialogue.
                    let opening = tokens[..i].iter().enumerate().all(|(j, t)| {
                        lines[j] != lines[i]
                            || !t.word().text().as_str().chars().any(char::is_alphanumeric)
                    }) && is(i + 1, "free")
                        && is(i + 2, "to");
                    // `could fell good timber`: a noun after the complement
                    // makes it an object of *fell*. Words that can also be
                    // modifiers or function words (`now`, `enough`) don't.
                    let object = has_tag(i + 2, &|pos| pos.starts_with("NN"))
                        && !has_tag(i + 2, &|pos| {
                            ["JJ", "RB", "IN", "TO", "RP", "CC", "DT", "PRP", "WP", "WRB"]
                                .iter()
                                .any(|prefix| pos.starts_with(prefix))
                        });
                    ((base && any(i + 1, FEEL_COMPLEMENTS) && !object) || opening).then_some("feel")
                }
                Self::QuiteQuiet if is(i, "quite") => {
                    before(1, QUIET_INTENSIFIERS).then_some("quiet")
                }
                Self::QuiteQuiet if is(i, "quiet") => {
                    // `couldn't quiet understand`: a bare verb follows. Nouns,
                    // pronouns and particles (`won't quiet down`) do not
                    // qualify, since *quiet* is also a transitive verb.
                    let verb = before(1, &["n't"])
                        && has_tag(i + 1, &|pos| pos == "VB")
                        && !has_tag(i + 1, &|pos| {
                            pos.starts_with("PRP")
                                || matches!(pos, "DT" | "IN" | "RP" | "NNS" | "NNP")
                        });
                    // `was quiet remarkable`: an adjective that is nothing else.
                    let lead =
                        before(1, QUITE_LEADS) || (before(1, &["'s"]) && before(2, QUITE_SUBJECTS));
                    let adjective = lead
                        && !is(i + 1, "enough")
                        && has_tag(i + 1, &|pos| pos == "JJ")
                        && !has_tag(i + 1, &|pos| {
                            pos.starts_with("NN")
                                || pos.starts_with("RB")
                                || matches!(pos, "IN" | "RP" | "VBN" | "VBG")
                        });
                    (verb || adjective).then_some("quite")
                }
                _ => None,
            };

            if let Some(replacement) = replacement {
                let original = word(i);
                let replacement = if original
                    .chars()
                    .all(|c| !c.is_alphabetic() || c.is_uppercase())
                {
                    replacement.to_uppercase()
                } else if original.chars().next().is_some_and(char::is_uppercase) {
                    let mut chars = replacement.chars();
                    chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str()
                } else {
                    replacement.to_owned()
                };
                found.push(Suggestion::new(
                    format!("BUILTIN/{}", self.id()),
                    format!("Did you mean “{}”?", replacement),
                    token.span().clone(),
                    vec![replacement],
                ));
            }
        }
        found
    }
}
