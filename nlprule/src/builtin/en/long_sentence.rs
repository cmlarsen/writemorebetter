//! Flags sentences with more words than a maximum.

use crate::types::{Sentence, Span, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "TOO_LONG_SENTENCE";

/// The word limit LanguageTool 6.6 uses for English.
const DEFAULT_MAX_WORDS: usize = 40;

/// Tokens that open a quote or aside whose words are not counted, matched by
/// position with [`CLOSING_QUOTES`].
const OPENING_QUOTES: &[&str] = &[
    "\"", "\u{201c}", "\u{201e}", "\u{ab}", "(", "[", "{", "\u{2014}",
];

/// Tokens that close the quote opened by the token at the same position in
/// [`OPENING_QUOTES`].
const CLOSING_QUOTES: &[&str] = &[
    "\"", "\u{201d}", "\u{201c}", "\u{bb}", ")", "]", "}", "\u{2014}",
];

/// Flags a sentence with more than `max_words` words between two `:`, `;` or
/// line breaks.
///
/// Only tokens that start with a letter count as words, and words inside
/// quotes, brackets or a pair of em dashes are not counted. A sentence that
/// contains a sentence end followed by a quote (`.”`, `?"`), as dialogue does,
/// is never flagged. The match spans the sentence from its first to its last
/// word, including a full stop, question mark or exclamation mark right after
/// it. LanguageTool tags the rule as picky, so it is off by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongSentence {
    max_words: usize,
}

impl LongSentence {
    pub(crate) fn new() -> Self {
        LongSentence {
            max_words: DEFAULT_MAX_WORDS,
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks one sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        if has_quoted_sentence_end(sentence.text()) {
            return Vec::new();
        }

        let tokens = sentence.tokens();
        let items = with_line_breaks(sentence);
        let message = format!(
            "This sentence is over {} words long. Consider splitting it up, as shorter \
             sentences make the text easier to read.",
            self.max_words
        );
        let mut suggestions = Vec::new();
        let mut quote: Option<usize> = None;
        let mut first_word: Option<&Token> = None;
        let mut i = 0;

        while i < items.len() {
            let mut words = 0;
            while let Some(Some(token)) = items.get(i) {
                let text = token.word().text().as_str();
                if matches!(text, ":" | ";") {
                    break;
                }
                match quote {
                    None => quote = OPENING_QUOTES.iter().position(|x| *x == text),
                    Some(open) if CLOSING_QUOTES[open] == text => quote = None,
                    Some(_) => {}
                }
                if quote.is_none() && is_word(text) {
                    let first = *first_word.get_or_insert(token);
                    if words == self.max_words {
                        suggestions.push(Suggestion::new(
                            format!("BUILTIN/{}", ID),
                            message.clone(),
                            Span::from_positions(first.span().start(), match_end(tokens)),
                            Vec::new(),
                        ));
                        break;
                    }
                    words += 1;
                }
                i += 1;
            }
            i += 1;
        }

        suggestions
    }
}

/// Whether the sentence contains `.`, `?` or `!` directly followed by a quote.
fn has_quoted_sentence_end(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '?' | '!' | '.')
            && matches!(
                chars.peek(),
                Some('"' | '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{bb}' | '\u{ab}')
            )
        {
            return true;
        }
    }
    false
}

/// The tokens of the sentence with `None` in place of each line break between
/// them, which ends a counted stretch like `:` and `;` do.
fn with_line_breaks<'a, 't>(sentence: &'a Sentence<'t>) -> Vec<Option<&'a Token<'t>>> {
    let text = sentence.text();
    let offset = sentence.span().byte().start;
    let mut gap_start = offset;
    let mut items = Vec::with_capacity(sentence.tokens().len());
    for token in sentence.tokens() {
        let gap = text
            .get(gap_start - offset..token.span().byte().start - offset)
            .unwrap_or("");
        items.extend(gap.matches('\n').map(|_| None));
        items.push(Some(token));
        gap_start = token.span().byte().end;
    }
    items
}

/// Whether a token counts as a word: its first UTF-16 unit is a letter.
fn is_word(text: &str) -> bool {
    text.chars()
        .next()
        .map_or(false, |c| c.is_alphabetic() && (c as u32) <= 0xffff)
}

/// The end of the last word of the sentence, or of the `.`, `?` or `!` that
/// directly follows it.
fn match_end(tokens: &[Token]) -> crate::types::Position {
    let last = tokens
        .iter()
        .rposition(|x| is_word(x.word().text().as_str()))
        .unwrap_or(tokens.len() - 1);
    match tokens.get(last + 1) {
        Some(next)
            if next.span().byte().start == tokens[last].span().byte().end
                && matches!(next.word().text().as_str(), "." | "?" | "!") =>
        {
            next.span().end()
        }
        _ => tokens[last].span().end(),
    }
}
