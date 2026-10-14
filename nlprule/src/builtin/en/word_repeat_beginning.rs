//! Flags successive sentences that begin with the same word.

use crate::types::{Sentence, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "ENGLISH_WORD_REPEAT_BEGINNING_RULE";

/// Message for a sentence that begins with the same adverb as the previous one.
const ADVERB_MESSAGE: &str = "Two successive sentences begin with the same adverb.";

/// Message for the third of three sentences that begin with the same word.
const WORD_MESSAGE: &str = "Three successive sentences begin with the same word.";

/// Advice appended to both messages.
const THESAURUS_MESSAGE: &str =
    "Consider rewording the sentence or use a thesaurus to find a synonym.";

/// Adverbs that add to the previous sentence, in the order LanguageTool 6.6 suggests them.
const ADD_ADVERBS: &[&str] = &["Besides", "Moreover", "Also", "Additionally", "Furthermore"];

/// Adverbs that contrast with the previous sentence, in suggestion order.
const CONTRAST_ADVERBS: &[&str] = &["Nonetheless", "Alternatively", "Nevertheless"];

/// Adverbs that emphasize the previous sentence, in suggestion order.
const EMPHASIS_ADVERBS: &[&str] = &[
    "Absolutely",
    "Clearly",
    "Obviously",
    "Definitely",
    "Indeed",
    "Importantly",
    "Undoubtedly",
];

/// Adverbs that explain the previous sentence, in suggestion order.
const EXPLAIN_ADVERBS: &[&str] = &["Especially", "Specifically", "Particularly"];

/// Expressions suggested after the other adding adverbs.
const ADD_EXPRESSIONS: &[&str] = &["In addition", "As well as"];

/// Expressions suggested after the other contrasting adverbs.
const CONTRAST_EXPRESSIONS: &[&str] = &["Even so", "On the other hand"];

/// Flags a sentence that begins with the same word as the two sentences before
/// it, or with the same adverb (`"Moreover"`, `"Indeed"`, ...) as the sentence
/// before it.
///
/// The corresponding LanguageTool rule runs over the whole text, so the
/// sentences before a paragraph count as well. Only the first token is
/// compared, and a sentence is only checked if it has at least three tokens
/// and the previous sentence is a single line ending in `.`, `?` or `!`; a
/// sentence after closing dialogue (`“Go.” He left.`) is therefore never
/// flagged. `The`, `A`, `An` and list punctuation such as `:` or `—` as one of
/// the first three tokens prevent a match.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WordRepeatBeginning {}

impl WordRepeatBeginning {
    pub(crate) fn new() -> Self {
        WordRepeatBeginning {}
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the sentences of one paragraph. `previous` are the sentences
    /// before the paragraph, oldest first; only the last two are used.
    /// `ends_text` tells whether the paragraph is the end of the text. Only the
    /// sentences of the paragraph itself are reported.
    pub fn check_with_previous(
        &self,
        sentences: &[Sentence],
        previous: &[Sentence],
        ends_text: bool,
    ) -> Vec<Suggestion> {
        let context = &previous[previous.len().saturating_sub(2)..];
        let mut suggestions = Vec::new();
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut previous_sentence: Option<&Sentence> = None;

        for (i, sentence) in context.iter().chain(sentences).enumerate() {
            let reported = i >= context.len();
            let ends_text = ends_text && i == context.len() + sentences.len() - 1;
            let tokens = sentence.tokens();
            let token_count =
                1 + tokens.len() + usize::from(ends_text && ends_in_whitespace(sentence));
            let token = tokens.first().map_or("", |x| x.word().text().as_str());

            if reported && token_count > 3 {
                let third = tokens.get(2).map_or("", |x| x.word().text().as_str());
                if is_word(token)
                    && last_token == token
                    && !is_exception(token)
                    && !is_exception(tokens[1].word().text().as_str())
                    && !is_exception(third)
                    && previous_sentence.is_some_and(ends_like_sentence)
                {
                    let message = if is_adverb(token) {
                        Some(ADVERB_MESSAGE)
                    } else if before_last_token == token {
                        Some(WORD_MESSAGE)
                    } else {
                        None
                    };
                    if let Some(message) = message {
                        suggestions.push(Suggestion::new(
                            format!("BUILTIN/{}", ID),
                            format!("{} {}", message, THESAURUS_MESSAGE),
                            tokens[0].span().clone(),
                            replacements(&tokens[0]),
                        ));
                    }
                }
            }

            before_last_token = std::mem::replace(&mut last_token, token.to_owned());
            previous_sentence = Some(sentence);
        }

        suggestions
    }
}

/// Whether LanguageTool keeps a whitespace token at the end of the sentence. It
/// does so only for the last sentence of the text, whose last token is marked
/// as the paragraph end.
fn ends_in_whitespace(sentence: &Sentence) -> bool {
    sentence
        .text()
        .chars()
        .last()
        .is_some_and(|c| c.is_whitespace() || c == '\u{feff}' || c == '\u{200b}')
}

/// Whether `token` can start a repetition: anything except a single
/// non-letter character such as `…` or `“`.
fn is_word(token: &str) -> bool {
    let mut utf16 = token.encode_utf16();
    match (utf16.next(), utf16.next()) {
        (Some(_), None) => token.chars().all(char::is_alphabetic),
        _ => true,
    }
}

/// Tokens that never count as a repeated beginning, e.g. list markers such as
/// `"2007: ..."` and the articles.
fn is_exception(token: &str) -> bool {
    matches!(
        token,
        ":" | "\u{2013}"
            | "-"
            | "\u{2714}\u{fe0f}"
            | "\u{27a1}\u{fe0f}"
            | "\u{2014}"
            | "\u{2b50}\u{fe0f}"
            | "\u{26a0}\u{fe0f}"
            | "The"
            | "A"
            | "An"
    )
}

/// Whether the previous sentence is a single line that ends in `.`, `?` or
/// `!` after Java's `String.trim`, so that e.g. table cells or dialogue that
/// ends in a closing quote do not count.
fn ends_like_sentence(sentence: &Sentence) -> bool {
    let text = sentence.text().trim_matches(|c: char| c <= ' ');
    let mut chars = text.chars();
    matches!(chars.next_back(), Some('.' | '?' | '!'))
        && chars.next().is_some()
        && !text
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
}

fn is_adverb(token: &str) -> bool {
    [
        ADD_ADVERBS,
        CONTRAST_ADVERBS,
        EMPHASIS_ADVERBS,
        EXPLAIN_ADVERBS,
    ]
    .iter()
    .any(|adverbs| adverbs.contains(&token))
}

/// The replacements for a repeated first token: a linking phrase before a
/// personal pronoun, or the other adverbs of the same kind.
fn replacements(token: &Token) -> Vec<String> {
    let text = token.word().text().as_str();

    if token
        .word()
        .tags()
        .iter()
        .any(|tag| tag.pos().as_str() == "PRP")
    {
        let adapted = if text == "I" {
            text.to_owned()
        } else {
            text.to_lowercase()
        };
        return ["Furthermore, ", "Likewise, ", "Not only that, but "]
            .iter()
            .map(|prefix| format!("{}{}", prefix, adapted))
            .collect();
    }

    for (adverbs, expressions) in [
        (ADD_ADVERBS, ADD_EXPRESSIONS),
        (CONTRAST_ADVERBS, CONTRAST_EXPRESSIONS),
        (EMPHASIS_ADVERBS, &[][..]),
        (EXPLAIN_ADVERBS, &[][..]),
    ] {
        if adverbs.contains(&text) {
            return adverbs
                .iter()
                .chain(expressions)
                .filter(|x| **x != text)
                .map(|x| (*x).to_owned())
                .collect();
        }
    }

    Vec::new()
}
