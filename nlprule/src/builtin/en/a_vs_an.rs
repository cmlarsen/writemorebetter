//! Flags the wrong indefinite article before a word, e.g. `"a apple"`.
//!
//! The choice between `a` and `an` depends on the *sound* the following word
//! starts with, which is not fully predictable from spelling. A small set of
//! words that contradict the spelling rule (`an hour`, `a university`) comes
//! from external word lists; everything else falls back to a spelling heuristic.
//! This mirrors the behavior of the corresponding LanguageTool rule, including
//! the treatment of quotes, hyphens and abbreviations.

use std::collections::HashSet;

use crate::types::{Sentence, Span, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_A_VS_AN";

/// Which article a word needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Determiner {
    /// Only "a" is correct.
    A,
    /// Only "an" is correct.
    An,
    /// Both are acceptable (e.g. "a historical" / "an historical").
    AOrAn,
    /// The word could not be classified; do not complain.
    Unknown,
}

/// Flags the wrong indefinite article before a word, e.g. `"a apple"`.
///
/// Words whose spelling disagrees with their pronunciation are read from two
/// word lists (words that always take `a`, words that always take `an`); words
/// not in the lists are classified by their first letter, with a few
/// prefix-based exceptions such as `one-` and `uni-`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AVsAn {
    /// Words (stored lowercased unless they came from a `*`-prefixed,
    /// case-sensitive line) that take "a".
    requires_a: HashSet<String>,
    /// Words that take "an", with the same case normalization as `requires_a`.
    requires_an: HashSet<String>,
}

impl AVsAn {
    pub(crate) fn new(requires_a: HashSet<String>, requires_an: HashSet<String>) -> Self {
        AVsAn {
            requires_a,
            requires_an,
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks one sentence and returns the suggestions for wrong articles.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let mut suggestions = Vec::new();

        // Index of the most recent indefinite article, kept only while the
        // tokens in between are quotelike delimiters.
        let mut article: Option<usize> = None;

        for i in 0..tokens.len() {
            let token = &tokens[i];

            if let Some(article_index) = article {
                let article_text = tokens[article_index].word().text().as_str();
                let sentence_start = article_index == 0;
                let (equals_a, equals_an) = if sentence_start {
                    (
                        article_text.eq_ignore_ascii_case("a"),
                        article_text.eq_ignore_ascii_case("an"),
                    )
                } else {
                    (article_text == "a", article_text == "an")
                };

                if equals_a || equals_an {
                    let determiner = self.determiner_for(token);

                    let replacement = match (equals_a, determiner) {
                        (true, Determiner::An) => Some(if starts_with_uppercase(article_text) {
                            "An"
                        } else {
                            "an"
                        }),
                        (false, Determiner::A) => Some(if starts_with_uppercase(article_text) {
                            "A"
                        } else {
                            "a"
                        }),
                        _ => None,
                    };

                    if let Some(replacement) = replacement {
                        let message = if equals_a {
                            format!(
                                "Use \"{}\" instead of \"{}\" if the following word starts with a \
                                 vowel sound, e.g. \"an article\", \"an hour\".",
                                replacement, article_text
                            )
                        } else {
                            format!(
                                "Use \"{}\" instead of \"{}\" if the following word doesn't start \
                                 with a vowel sound, e.g. \"a sentence\", \"a university\".",
                                replacement, article_text
                            )
                        };

                        suggestions.push(Suggestion::new(
                            format!("BUILTIN/{}", ID),
                            message,
                            Span::from_positions(
                                tokens[article_index].span().start(),
                                token.span().end(),
                            ),
                            vec![replacement.to_owned()],
                        ));
                    }
                }
            }

            article = if is_article_text(token.word().text().as_str()) {
                Some(i)
            } else if is_delimiter(token.word().text().as_str()) && next_token_len(tokens, i) > 1 {
                article
            } else {
                None
            };
        }

        suggestions
    }

    /// Classifies the word of `token` as taking "a" or "an".
    fn determiner_for(&self, token: &Token) -> Determiner {
        let text = token.word().text().as_str();
        let mut word = text;

        // Only the part before the first hyphen or apostrophe is relevant, e.g.
        // for "one-way". A leading "a-" is kept whole to avoid false alarms on
        // "a-levels".
        if let Some(before) = text.split(['-', '\'']).next() {
            if !before.eq_ignore_ascii_case("a") {
                word = before;
            }
        }

        // Quotelike characters are removed once the hyphen part is isolated,
        // e.g. a token `"apple"` is treated as `apple`.
        let cleaned;
        if token.has_space_before() || word != "-" {
            cleaned = cleanup(word);
            if cleaned.is_empty() {
                return Determiner::Unknown;
            }
            word = &cleaned;
        }

        let lower = word.to_lowercase();
        let mut determiner = Determiner::Unknown;

        if self.requires_a.contains(&lower) || self.requires_a.contains(word) {
            determiner = Determiner::A;
        }
        if self.requires_an.contains(&lower) || self.requires_an.contains(word) {
            determiner = if determiner == Determiner::A {
                Determiner::AOrAn
            } else {
                Determiner::An
            };
        }

        if determiner == Determiner::Unknown {
            let first = word.chars().next().expect("word is not empty");
            determiner = if is_all_uppercase(word) || is_mixed_case(word) {
                // An all-uppercase or mixed-case word is likely an abbreviation
                // whose pronunciation is unknown, so never complain.
                Determiner::Unknown
            } else if matches_an_prefix(&lower)
                || (is_vowel(first) && !matches_an_exception_prefix(&lower))
            {
                Determiner::An
            } else {
                Determiner::A
            };
        }

        determiner
    }
}

/// Whether `text` is the indefinite article "a" or "an" (any capitalization).
fn is_article_text(text: &str) -> bool {
    text.eq_ignore_ascii_case("a") || text.eq_ignore_ascii_case("an")
}

/// Whether every character of `text` is one of the quote or bracket characters
/// that may sit between an article and its noun.
fn is_delimiter(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| {
            matches!(
                c,
                '-' | '"' | '\u{201c}' | '\'' | '\u{2018}' | '\u{2019}' | '(' | ')' | '[' | ']'
            )
        })
}

/// The character length of the token following index `i`, or 0 at the end.
fn next_token_len(tokens: &[Token], i: usize) -> usize {
    tokens
        .get(i + 1)
        .map(|token| token.word().text().as_str().chars().count())
        .unwrap_or(0)
}

/// Removes everything that is not a letter, digit or one of a few in-word
/// punctuation characters.
fn cleanup(word: &str) -> String {
    word.chars()
        .filter(|c| {
            *c == '\u{3b1}'
                || c.is_ascii_alphanumeric()
                || matches!(c, '.' | ';' | ',' | ':' | '\'')
        })
        .collect()
}

/// Whether the first character of `text` is uppercase.
fn starts_with_uppercase(text: &str) -> bool {
    text.chars().next().map(char::is_uppercase).unwrap_or(false)
}

/// Whether `text` contains no lowercase letters (so `"123"` counts as
/// all-uppercase).
fn is_all_uppercase(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphabetic() && c.is_lowercase())
}

/// Whether `text` is capitalized, i.e. the first character is uppercase and the
/// remaining letters are lowercase.
fn is_capitalized_word(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    !chars.any(|c| c.is_alphabetic() && !c.is_lowercase())
}

/// Whether `text` contains a letter that is not lowercase.
fn is_not_all_lowercase(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic() && !c.is_lowercase())
}

/// Whether `text` is mixed case such as `MixedCase` or `mixedCase`.
fn is_mixed_case(text: &str) -> bool {
    !is_all_uppercase(text) && !is_capitalized_word(text) && is_not_all_lowercase(text)
}

/// `^(unidentif|uni[mn])[a-z]+$` (case-insensitive input).
fn matches_an_prefix(text: &str) -> bool {
    let has_suffix =
        |len: usize| text.len() > len && text[len..].chars().all(|c| c.is_ascii_lowercase());

    (text.starts_with("unidentif") && has_suffix("unidentif".len()))
        || ((text.starts_with("unim") || text.starts_with("unin")) && has_suffix(4))
}

/// `^(eu|one|uni|u[rst][aeiou])[a-z]*$` (case-insensitive input).
fn matches_an_exception_prefix(text: &str) -> bool {
    let suffix_is_lowercase =
        |prefix: &str| text.strip_prefix(prefix).is_some_and(is_lowercase_letters);

    if suffix_is_lowercase("eu") || suffix_is_lowercase("one") || suffix_is_lowercase("uni") {
        return true;
    }

    let mut chars = text.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some('u'), Some(second), Some(third))
            if matches!(second, 'r' | 's' | 't')
                && matches!(third, 'a' | 'e' | 'i' | 'o' | 'u') =>
        {
            chars.all(|c| c.is_ascii_lowercase())
        }
        _ => false,
    }
}

/// Whether `text` consists only of lowercase ASCII letters.
fn is_lowercase_letters(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_lowercase())
}

/// Whether `c` is one of the five vowels recognized by the rule.
fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
}
