//! Narrow English confusion patterns adapted from Harper (Apache-2.0).
//!
//! These patterns use word tokens and their existing spans. No LanguageTool
//! rule data is embedded in the engine.

use crate::types::{Sentence, Suggestion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HarperConfusion {
    WaistWaste,
    BellowBelow,
    FeelFell,
}

impl HarperConfusion {
    pub fn id(self) -> &'static str {
        match self {
            Self::WaistWaste => "HARPER_WAIST_WASTE",
            Self::BellowBelow => "HARPER_BELLOW_BELOW",
            Self::FeelFell => "HARPER_FEEL_FELL",
        }
    }

    pub fn check(self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let word = |i: usize| {
            tokens
                .get(i)
                .map(|t| t.word().text().as_str())
                .unwrap_or("")
        };
        let is = |i: usize, expected: &str| word(i).eq_ignore_ascii_case(expected);
        let mut found = Vec::new();

        for i in 0..tokens.len() {
            let replacement = match self {
                Self::WaistWaste if is(i, "waist") => {
                    let before = i > 0 && is(i - 1, "a");
                    let after = is(i + 1, "of");
                    let preceding = i >= 2
                        && [
                            "complete", "great", "huge", "total", "utter", "real", "horrible",
                        ]
                        .iter()
                        .any(|w| is(i - 2, w));
                    let nouns = ["effort", "money", "resources", "space", "time"];
                    let following = nouns.iter().any(|w| is(i + 2, w))
                        || (["my", "your", "his", "her", "its", "our", "their"]
                            .iter()
                            .any(|w| is(i + 2, w))
                            && nouns.iter().any(|w| is(i + 3, w)));
                    if after && (following || (before && preceding)) {
                        Some("waste")
                    } else {
                        None
                    }
                }
                Self::BellowBelow if is(i, "bellow") => {
                    if [
                        "a", "an", "the", "this", "that", "these", "those", "my", "your", "his",
                        "her", "its", "our", "their", "one", "two", "three", "four", "five", "six",
                        "seven", "eight", "nine", "ten", "eleven", "twelve", "hundred", "thousand",
                    ]
                    .iter()
                    .any(|w| is(i + 1, w))
                        || word(i + 1)
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit())
                    {
                        Some("below")
                    } else {
                        None
                    }
                }
                Self::FeelFell if is(i, "fell") => {
                    let after = [
                        "comfortable",
                        "free",
                        "good",
                        "that",
                        "we",
                        "you",
                        "i",
                        "it",
                    ]
                    .iter()
                    .any(|w| is(i + 1, w));
                    let negative =
                        i >= 2 && ["did", "does"].iter().any(|w| is(i - 2, w)) && is(i - 1, "n't");
                    if after || negative {
                        Some("feel")
                    } else {
                        None
                    }
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
                    tokens[i].span().clone(),
                    vec![replacement],
                ));
            }
        }
        found
    }
}
