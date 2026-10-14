//! Flags multi-word proper nouns written with the wrong capitalization.
//!
//! The behaviour follows the LanguageTool `EN_SPECIFIC_CASE` rule: every run of
//! up to as many tokens as the longest listed phrase has words is joined with
//! single spaces and looked up case-insensitively in a phrase list (e.g.
//! `Harry Potter`). A run whose casing differs from the listed spelling is
//! reported, unless it is all uppercase, or it starts the sentence and the
//! listed spelling starts lowercase (e.g. `vitamin C`).
//!
//! The phrase list is LanguageTool data (LGPL-2.1), copied into the build
//! directory as `builtin/specific_case.txt` and serialized into the rules
//! binary; only the matching logic lives in this source file.

use crate::types::{DefaultHashMap, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_SPECIFIC_CASE";

const MESSAGE_INITIAL_CAPITALS: &str = "If the term is a proper noun, use initial capitals.";
const MESSAGE_OTHER_CAPITALIZATION: &str =
    "If the term is a proper noun, use the suggested capitalization.";

/// Flags a listed phrase with the wrong capitalization, e.g. `"Harry potter"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecificCase {
    /// The lowercased phrase -> its proper spelling.
    #[serde(serialize_with = "crate::types::sorted::map")]
    phrases: DefaultHashMap<String, String>,
    /// The number of space-separated words of the longest phrase.
    max_words: usize,
}

impl SpecificCase {
    /// Builds the rule from the listed phrases, one per entry.
    pub(crate) fn new(lines: Vec<String>) -> Self {
        let mut rule = SpecificCase::default();
        for line in lines {
            rule.max_words = rule.max_words.max(line.split(' ').count());
            let phrase = line.trim();
            rule.phrases
                .insert(phrase.to_lowercase(), phrase.to_owned());
        }
        rule
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let mut suggestions = Vec::new();

        for start in 0..tokens.len() {
            let mut phrase = String::new();
            for end in start..tokens.len().min(start + self.max_words) {
                if end > start {
                    phrase.push(' ');
                }
                phrase.push_str(tokens[end].word().text().as_str());

                let Some(proper) = self.phrases.get(&phrase.to_lowercase()) else {
                    continue;
                };
                if is_all_uppercase(&phrase) || &phrase == proper {
                    continue;
                }
                if start == 0 && !starts_with_uppercase(proper) {
                    continue;
                }

                let message = if proper.split(' ').all(starts_with_uppercase) {
                    MESSAGE_INITIAL_CAPITALS
                } else {
                    MESSAGE_OTHER_CAPITALIZATION
                };
                suggestions.push(Suggestion::new(
                    format!("BUILTIN/{}", ID),
                    message.to_owned(),
                    Span::from_positions(tokens[start].span().start(), tokens[end].span().end()),
                    vec![proper.clone()],
                ));
            }
        }

        suggestions
    }
}

/// `StringTools.isAllUppercase`: no lowercase letter.
fn is_all_uppercase(text: &str) -> bool {
    !text.chars().any(char::is_lowercase)
}

/// `StringTools.startsWithUppercase`: the first character is uppercase.
fn starts_with_uppercase(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase)
}
