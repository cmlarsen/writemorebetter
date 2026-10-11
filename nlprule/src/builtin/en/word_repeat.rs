//! Flags a word that is immediately repeated.

use crate::types::{Sentence, Span, Suggestion, Token};
use serde::{Deserialize, Serialize};
use unicase::UniCase;

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "ENGLISH_WORD_REPEAT_RULE";

/// Message shown for a suspected repeated word.
const MESSAGE: &str = "Possible typo: you repeated a word.";

/// Flags a word that is immediately repeated, e.g. `"the the"`.
///
/// A repetition is skipped when it is covered by an entry in `exceptions` or by
/// one of the structural exceptions taken from the corresponding LanguageTool
/// rule: runs of three or more identical words, letter-by-letter spellings such
/// as `"b a s i c a l l y"` and a few case-sensitive pairs such as `"may May"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordRepeat {
    /// Allowed repetitions. Every entry is a lowercased sequence of words; a
    /// repetition is permitted if one of these sequences occurs as a
    /// contiguous run of words that contains the repeated pair.
    exceptions: Vec<Vec<String>>,
}

/// A word in a token stream. Usually this is one token, but words containing an
/// apostrophe such as `"don’t"` are split by the tokenizer and are merged back
/// together here.
struct WordUnit {
    /// Index of the first token of this word.
    start: usize,
    /// Index just past the last token of this word.
    end: usize,
    /// The word text with its original casing.
    text: String,
    /// The lowercased word text, used for comparisons.
    lower: String,
}

impl WordRepeat {
    pub(crate) fn new(exceptions: Vec<Vec<String>>) -> Self {
        WordRepeat { exceptions }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence for repeated words and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let units = word_units(tokens);
        let mut suggestions = Vec::new();

        for i in 1..units.len() {
            let prev = &units[i - 1];
            let cur = &units[i];

            // The two words must be directly adjacent; any token in between
            // (for example a closing quote) breaks the repetition.
            if cur.start != prev.end {
                continue;
            }
            if UniCase::new(&prev.text) != UniCase::new(&cur.text) {
                continue;
            }
            if self.is_allowed(tokens, &units, i) {
                continue;
            }

            // The span reaches from the start of the first word to the end of
            // the second one, i.e. it also covers the whitespace in between.
            let start = tokens[prev.start].span().start();
            let end = tokens[cur.end - 1].span().end();

            suggestions.push(Suggestion::new(
                format!("BUILTIN/{}", ID),
                MESSAGE.to_owned(),
                Span::from_positions(start, end),
                vec![prev.text.clone()],
            ));
        }

        suggestions
    }

    /// Whether the repetition of `units[i]` should be ignored.
    fn is_allowed(&self, tokens: &[Token], units: &[WordUnit], i: usize) -> bool {
        repeated_three_or_more(units, i)
            || spelled_out_letters(units, i)
            || case_sensitive_pair(units, i)
            || repeated_wait(units, i)
            || contraction_after_verb(tokens, units, i)
            || self.matches_exception(units, i)
    }

    /// Checks whether an entry of the exception list covers the pair at `i`.
    fn matches_exception(&self, units: &[WordUnit], i: usize) -> bool {
        if self.exceptions.is_empty() {
            return false;
        }

        let words: Vec<&str> = units.iter().map(|unit| unit.lower.as_str()).collect();

        for sequence in &self.exceptions {
            let len = sequence.len();
            if len == 0 || len > words.len() {
                continue;
            }
            // The window has to contain both words of the repetition, so it
            // may start anywhere from `i + 1 - len` to `i - 1`.
            let min_start = (i + 1).saturating_sub(len);
            let max_start = i - 1;
            for start in min_start..=max_start {
                if start + len <= words.len() && words[start..start + len] == sequence[..] {
                    return true;
                }
            }
        }

        false
    }
}

/// Splits the tokens into words and merges tokens that belong to one word,
/// e.g. the three tokens `"don"`, `"’"`, `"t"` become the single word `"don’t"`.
fn word_units(tokens: &[Token]) -> Vec<WordUnit> {
    let mut units = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let text = tokens[i].word().text().as_str();
        if !is_word(text) {
            i += 1;
            continue;
        }

        let mut merged = text.to_owned();
        let mut end = i + 1;

        while end + 1 < tokens.len()
            && is_apostrophe(tokens[end].word().text().as_str())
            && !tokens[end].has_space_before()
            && is_word(tokens[end + 1].word().text().as_str())
            && !tokens[end + 1].has_space_before()
        {
            merged.push_str(tokens[end].word().text().as_str());
            merged.push_str(tokens[end + 1].word().text().as_str());
            end += 2;
        }

        units.push(WordUnit {
            start: i,
            end,
            lower: merged.to_lowercase(),
            text: merged,
        });
        i = end;
    }

    units
}

/// A token counts as a word if it contains at least one alphabetic character.
/// This skips numbers (e.g. `"10 10"`) and pure punctuation (e.g. `"… …"`).
fn is_word(token: &str) -> bool {
    token.chars().any(char::is_alphabetic)
}

/// One of the characters that may join two parts of a word, e.g. `"don’t"`.
fn is_apostrophe(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '\u{00b4}' | '`'))
}

/// Whether the word at `i` is part of a run of three or more identical words.
fn repeated_three_or_more(units: &[WordUnit], i: usize) -> bool {
    let word = &units[i].lower;
    let next =
        i + 1 < units.len() && units[i + 1].start == units[i].end && &units[i + 1].lower == word;
    let prev = i >= 2 && units[i - 2].end == units[i - 1].start && &units[i - 2].lower == word;
    next || prev
}

/// Detects spellings letter by letter, e.g. `"b a s i c a l l y"`.
fn spelled_out_letters(units: &[WordUnit], i: usize) -> bool {
    i >= 2
        && i + 1 < units.len()
        && is_single_letter(&units[i - 2].text)
        && is_single_letter(&units[i].text)
        && is_single_letter(&units[i + 1].text)
}

fn is_single_letter(token: &str) -> bool {
    let mut chars = token.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_alphabetic())
}

/// Case-sensitive pairs such as `"may May"`, `"will Will"` and the sentence-
/// initial `"May May"` / `"Will Will"`.
fn case_sensitive_pair(units: &[WordUnit], i: usize) -> bool {
    let prev = units[i - 1].text.as_str();
    let cur = units[i].text.as_str();

    if cur.ends_with("ay") {
        if (prev == "may" && cur == "May") || (prev == "May" && cur == "may") {
            return true;
        }
        units.len() > 1 && units[0].text == "May" && units[1].text == "May"
    } else if cur.ends_with("ill") {
        if (prev == "will" && cur == "Will") || (prev == "Will" && cur == "will") {
            return true;
        }
        units.len() > 1 && units[0].text == "Will" && units[1].text == "Will"
    } else {
        false
    }
}

/// `"wait wait"` is only allowed at the start of a sentence.
fn repeated_wait(units: &[WordUnit], i: usize) -> bool {
    i == 1 && units[i].lower == "wait"
}

/// `"do do n't"`, `"did did n't"` and `"does does n't"` are allowed: the second
/// verb starts a negated contraction. The tokenizer splits `"n't"` into three
/// tokens, so the contraction is detected from the following tokens.
fn contraction_after_verb(tokens: &[Token], units: &[WordUnit], i: usize) -> bool {
    if !matches!(units[i].lower.as_str(), "do" | "did" | "does") {
        return false;
    }

    let j = units[i].end;
    j + 2 < tokens.len()
        && tokens[j].word().text().as_str().eq_ignore_ascii_case("n")
        && is_apostrophe(tokens[j + 1].word().text().as_str())
        && tokens[j + 2]
            .word()
            .text()
            .as_str()
            .eq_ignore_ascii_case("t")
}
