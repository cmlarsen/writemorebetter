//! Flags a word that is easily confused with another one, based on its context.
//!
//! The behaviour follows the LanguageTool `ENGLISH_WRONG_WORD_IN_CONTEXT`
//! rule. A table maps pairs of confusable words to the contexts in which each
//! of them typically occurs. When exactly one of the two words occurs in a
//! sentence and only the *other* word's context matches, the occurrence is
//! flagged and the part of the word that distinguishes the pair is replaced.
//!
//! The word and context patterns come from the table and are translated to
//! Rust `regex` syntax when the rules binary is built, so this module only
//! contains the matching logic.

use crate::types::{Sentence, Suggestion};
use crate::utils::{regex::Regex, uppercase_first_letter_or_digit};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "ENGLISH_WRONG_WORD_IN_CONTEXT";

/// One row of the confusion table: a pair of confusable words with the
/// fragment that has to be replaced, the contexts in which each word occurs
/// and a short explanation of each word.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confusion {
    /// The two word patterns, already wrapped in word boundaries.
    words: [Regex; 2],
    /// The fragment of each word that is replaced when the other word is
    /// meant, e.g. `prescr` for `prescribe`.
    matches: [String; 2],
    /// The context in which each word typically occurs.
    contexts: [Regex; 2],
    /// A short description of each word (may be empty).
    explanations: [String; 2],
}

impl Confusion {
    /// Creates a row from its already translated patterns and text.
    pub(crate) fn new(
        words: [Regex; 2],
        matches: [String; 2],
        contexts: [Regex; 2],
        explanations: [String; 2],
    ) -> Self {
        Confusion {
            words,
            matches,
            contexts,
            explanations,
        }
    }
}

/// Flags words that are easily confused with another word in context.
///
/// Each row of the table holds two words. A row can only fire when exactly one
/// of the two words occurs in the sentence, so a sentence that uses both words
/// is never flagged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrongWordInContext {
    confusions: Vec<Confusion>,
}

impl WrongWordInContext {
    pub(crate) fn new(confusions: Vec<Confusion>) -> Self {
        WrongWordInContext { confusions }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the consecutive sentences of one paragraph. The rule never looks
    /// across sentence boundaries, so every sentence is checked on its own.
    pub fn check_sentences(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        sentences
            .iter()
            .flat_map(|sentence| self.check(sentence))
            .collect()
    }

    /// Checks one sentence and returns the confusion suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let mut suggestions = Vec::new();

        for confusion in &self.confusions {
            // Find the first token matching each of the two words. When both
            // words occur, the rule cannot tell which one is wrong.
            let mut word_match: [Option<usize>; 2] = [None, None];
            for (index, token) in tokens.iter().enumerate() {
                if word_match[0].is_some() && word_match[1].is_some() {
                    break;
                }
                let text = token.word().text().as_str();
                for (side, word) in confusion.words.iter().enumerate() {
                    if word_match[side].is_none() && word.is_match(text) {
                        word_match[side] = Some(index);
                    }
                }
            }

            let (found, other) = match (word_match[0], word_match[1]) {
                (Some(_), None) => (0, 1),
                (None, Some(_)) => (1, 0),
                _ => continue,
            };

            // Search the sentence for the contexts of both words, regardless
            // of which token matches. The search covers the matched word's own
            // token, exactly like the LanguageTool rule.
            let mut context_match = [false, false];
            for token in tokens {
                if context_match[found] && context_match[other] {
                    break;
                }
                let text = token.word().text().as_str();
                for (side, context) in confusion.contexts.iter().enumerate() {
                    if !context_match[side] && context.is_match(text) {
                        context_match[side] = true;
                    }
                }
            }

            // Flag only when the context of the word that is *not* present
            // matches and the context of the word that is present does not.
            if !context_match[other] || context_match[found] {
                continue;
            }

            let index = word_match[found].expect("the matched side has a token index");
            let wrong_word = tokens[index].word().text().as_str();
            let replacement = preserve_case(
                &replace_first_ignore_case(
                    wrong_word,
                    &confusion.matches[found],
                    &confusion.matches[other],
                ),
                wrong_word,
            );

            suggestions.push(Suggestion::new(
                format!("BUILTIN/{}", ID),
                confusing_message(
                    wrong_word,
                    &replacement,
                    &confusion.explanations[found],
                    &confusion.explanations[other],
                ),
                tokens[index].span().clone(),
                vec![replacement],
            ));
        }

        suggestions
    }
}

/// Builds the message shown for a suspected confusion. The explanation is only
/// included when both words have one.
fn confusing_message(
    wrong_word: &str,
    replacement: &str,
    explanation_wrong_word: &str,
    explanation_replacement: &str,
) -> String {
    if explanation_replacement.is_empty() || explanation_wrong_word.is_empty() {
        format!(
            "Possibly confused word: Did you mean \"{}\" instead of \"{}\"?",
            replacement, wrong_word
        )
    } else {
        format!(
            "Possibly confused word: Did you mean \"{}\" (= {}) instead of \"{}\" (= {})?",
            replacement, explanation_replacement, wrong_word, explanation_wrong_word
        )
    }
}

/// Replaces the first occurrence of `needle` in `text`, ignoring case.
///
/// The fragments in the table are plain alphanumeric substrings, so they are
/// matched literally rather than as a regular expression. The replacement
/// happens on char boundaries, so non-ASCII text stays valid UTF-8.
fn replace_first_ignore_case(text: &str, needle: &str, replacement: &str) -> String {
    let Some(start) = find_ignore_case(text, needle) else {
        return text.to_owned();
    };

    let end = start + needle.len();
    let mut output = String::with_capacity(text.len() + replacement.len());
    output.push_str(&text[..start]);
    output.push_str(replacement);
    output.push_str(&text[end..]);
    output
}

/// Finds the byte offset of the first case-insensitive occurrence of `needle`,
/// which may only contain ASCII alphanumeric characters.
fn find_ignore_case(text: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }

    let needle: Vec<char> = needle.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    if needle.len() > text_chars.len() {
        return None;
    }

    // Byte offset of every char plus the end of the string, so a match can be
    // turned back into a byte offset without slicing at a non-boundary.
    let mut offsets: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    offsets.push(text.len());

    for start in 0..=(text_chars.len() - needle.len()) {
        if needle
            .iter()
            .zip(&text_chars[start..])
            .all(|(needle_char, text_char)| needle_char.eq_ignore_ascii_case(text_char))
        {
            return Some(offsets[start]);
        }
    }

    None
}

/// Applies the capitalization of `model` to `text`, mirroring LanguageTool's
/// case preservation for suggestions.
fn preserve_case(text: &str, model: &str) -> String {
    if model.is_empty() {
        return text.to_owned();
    }

    if is_capitalized_word(model) {
        return uppercase_first_letter_or_digit(&text.to_lowercase());
    }

    if is_all_uppercase(model) {
        return text.to_uppercase();
    }

    text.to_owned()
}

/// Whether `text` starts with an uppercase letter and has no other letter that
/// is not lowercase.
fn is_capitalized_word(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {}
        _ => return false,
    }

    !chars.any(|c| c.is_alphabetic() && !c.is_lowercase())
}

/// Whether `text` contains no lowercase letter (so `"123"` counts as
/// all-uppercase).
fn is_all_uppercase(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphabetic() && c.is_lowercase())
}
