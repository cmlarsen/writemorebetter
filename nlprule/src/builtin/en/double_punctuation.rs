//! Flags two consecutive dots or commas.
//!
//! The behaviour follows the LanguageTool `DOUBLE_PUNCTUATION` rule: a pair of
//! dots or commas, possibly separated by whitespace, is reported. Ellipses
//! (three or more dots, or a dot next to `…`) and dots after `?` or `!` are left
//! alone.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "DOUBLE_PUNCTUATION";

const MESSAGE_DOTS: &str = "Two consecutive dots";
const MESSAGE_COMMAS: &str = "Two consecutive commas";

/// Flags two consecutive dots or commas, e.g. `"He left.."` or `"no,,"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoublePunctuation;

impl DoublePunctuation {
    pub(crate) fn new() -> Self {
        DoublePunctuation
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();
        let text = |i: usize| tokens[i].word().text().as_str();
        let mut suggestions = Vec::new();
        let mut dots = 0;
        let mut commas = 0;

        for i in 0..tokens.len() {
            let token = text(i);
            let next = if i + 1 < tokens.len() {
                text(i + 1)
            } else {
                ""
            };
            let second_previous = if i >= 2 { text(i - 2) } else { "" };

            if token == "." {
                dots += 1;
                commas = 0;
            } else if token == "," {
                commas += 1;
                dots = 0;
            }

            let flagged = if dots == 2
                && !matches!(next, "." | "\u{2026}" | "/" | "\\")
                && !matches!(second_previous, "?" | "!" | "\u{2026}" | ".")
            {
                dots = 0;
                Some((MESSAGE_DOTS, vec![".".to_owned(), "\u{2026}".to_owned()]))
            } else if commas == 2 && next != "," {
                commas = 0;
                Some((MESSAGE_COMMAS, vec![",".to_owned()]))
            } else {
                None
            };

            if let Some((message, replacements)) = flagged {
                let span = tokens[i].span();
                suggestions.push(Suggestion::new(
                    format!("BUILTIN/{}", ID),
                    message.to_owned(),
                    Span::from_positions(previous_char(sentence, span.start()), span.end()),
                    replacements,
                ));
            }

            if token != "." && token != "," {
                dots = 0;
                commas = 0;
            }
        }

        suggestions
    }
}

/// The position one character before `position`, which lies inside `sentence`.
fn previous_char(sentence: &Sentence, position: Position) -> Position {
    let base = sentence.span().start();
    let before = &sentence.text()[..position.byte - base.byte];
    match before.chars().next_back() {
        Some(c) => Position {
            byte: position.byte - c.len_utf8(),
            char: position.char - 1,
        },
        None => position,
    }
}
