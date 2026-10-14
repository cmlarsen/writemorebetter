//! Flags paragraphs with more words than a maximum.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "TOO_LONG_PARAGRAPH";

/// The word limit LanguageTool 6.6 uses.
const DEFAULT_MAX_WORDS: usize = 220;

/// Flags a paragraph with more than `max_words + 5` words.
///
/// A paragraph ends at a blank line or at the end of the text; a single line
/// break does not end it, and a paragraph that contains one is never flagged
/// (lists, hard-wrapped text). Every token except single punctuation marks
/// counts as a word. The match spans the words at positions `max_words - 1`
/// and `max_words`. LanguageTool tags the rule as picky and turns it off by
/// default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongParagraph {
    max_words: usize,
}

impl LongParagraph {
    pub(crate) fn new() -> Self {
        LongParagraph {
            max_words: DEFAULT_MAX_WORDS,
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the sentences of one paragraph. `previous` are the sentences
    /// before it, oldest first; `ends_text` tells whether the paragraph is the
    /// end of the text.
    pub fn check_with_previous(
        &self,
        sentences: &[Sentence],
        previous: &[Sentence],
        ends_text: bool,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let mut has_line_breaks = previous
            .last()
            .is_some_and(|x| !ends_with_blank_line(x.text()) && has_line_break(x));
        let mut words = 0;
        let mut start = Position::default();
        let mut end = Position::default();

        for (n, sentence) in sentences.iter().enumerate() {
            let paragraph_end = ends_with_blank_line(sentence.text())
                || sentences.get(n + 1).map_or(ends_text, |next| {
                    next.text().starts_with('\n') || next.text().starts_with("\r\n")
                });
            if !paragraph_end && has_line_break(sentence) {
                has_line_breaks = true;
            }

            for token in sentence.tokens() {
                if !is_non_word(token.word().text().as_str()) {
                    words += 1;
                    if words == self.max_words {
                        end = token.span().end();
                    } else if words + 1 == self.max_words {
                        start = token.span().start();
                    }
                }
            }

            if paragraph_end {
                if words > self.max_words + 5 && !has_line_breaks {
                    suggestions.push(Suggestion::new(
                        format!("BUILTIN/{}", ID),
                        format!(
                            "This paragraph is over {} words long here, consider revising it.",
                            self.max_words
                        ),
                        Span::from_positions(start, end),
                        Vec::new(),
                    ));
                }
                words = 0;
                has_line_breaks = false;
            }
        }

        suggestions
    }
}

fn ends_with_blank_line(text: &str) -> bool {
    text.ends_with("\n\n") || text.ends_with("\n\r\n\r") || text.ends_with("\r\n\r\n")
}

/// Whether the sentence contains a line break after its leading ones.
fn has_line_break(sentence: &Sentence) -> bool {
    sentence.text().trim_start_matches('\n').contains('\n')
}

/// Whether a token is a single punctuation mark, which is not counted as a word.
fn is_non_word(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => matches!(
            c,
            '.' | '?'
                | '!'
                | '\u{2026}'
                | ':'
                | ';'
                | ','
                | '~'
                | '\u{2019}'
                | '\''
                | '"'
                | '\u{201e}'
                | '\u{201c}'
                | '\u{201d}'
                | '\u{bb}'
                | '\u{ab}'
                | '\u{201a}'
                | '\u{2018}'
                | '\u{203a}'
                | '\u{2039}'
                | '('
                | ')'
                | '['
                | ']'
                | '-'
                | '\u{2013}'
                | '\u{2014}'
                | '*'
                | '\u{d7}'
                | '\u{2217}'
                | '\u{b7}'
                | '+'
                | '\u{f7}'
                | '/'
                | '='
        ),
        _ => false,
    }
}
