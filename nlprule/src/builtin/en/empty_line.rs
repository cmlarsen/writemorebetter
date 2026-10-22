//! Flags more than one empty line between two paragraphs.
//!
//! The behaviour follows the LanguageTool `EMPTY_LINE` rule, which is off by
//! default: a paragraph followed by three or more empty lines is reported at
//! its last token, without a suggestion. Empty lines after trailing spaces, or
//! at the start of the text, are a whitespace-only sentence of their own and
//! are reported at their last line break.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EMPTY_LINE";

const MESSAGE: &str = "Please delete empty line below and use formatting instead";

/// Flags extra empty lines between paragraphs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmptyLine;

impl EmptyLine {
    pub(crate) fn new() -> Self {
        EmptyLine
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the consecutive sentences of one paragraph. With `ends_text`, the
    /// last sentence ends the text and is not checked: empty lines at the end
    /// of the text separate no paragraphs.
    pub fn check_text(&self, sentences: &[Sentence], ends_text: bool) -> Vec<Suggestion> {
        let checked = if ends_text {
            sentences.len().saturating_sub(1)
        } else {
            sentences.len()
        };

        sentences[..checked]
            .iter()
            .filter(|sentence| ends_with_empty_lines(sentence.text()))
            .map(|sentence| {
                Suggestion::new(
                    format!("BUILTIN/{}", ID),
                    MESSAGE.to_owned(),
                    report_span(sentence),
                    Vec::new(),
                )
            })
            .collect()
    }
}

/// The last token of `sentence`, or for a whitespace-only sentence (the blank lines after
/// `"End.  \n\n\n\n"`, or at the start of the text) its last character, as in LanguageTool.
fn report_span(sentence: &Sentence) -> Span {
    match sentence.tokens().last() {
        Some(token) => token.span().clone(),
        None => {
            let end = sentence.span().end();
            let last = sentence
                .text()
                .chars()
                .next_back()
                .map_or(0, char::len_utf8);
            Span::from_positions(
                Position {
                    byte: end.byte - last,
                    char: end.char - usize::from(last > 0),
                },
                end,
            )
        }
    }
}

fn ends_with_empty_lines(text: &str) -> bool {
    text.ends_with("\n\n\n\n")
        || text.ends_with("\n\r\n\r\n\r\n\r")
        || text.ends_with("\r\n\r\n\r\n\r\n")
}
