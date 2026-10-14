//! Flags more than one empty line between two paragraphs.
//!
//! The behaviour follows the LanguageTool `EMPTY_LINE` rule, which is off by
//! default: a paragraph followed by three or more empty lines is reported at
//! its last token, without a suggestion.

use crate::types::{Sentence, Suggestion};
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
            .filter_map(|sentence| sentence.tokens().last())
            .map(|token| {
                Suggestion::new(
                    format!("BUILTIN/{}", ID),
                    MESSAGE.to_owned(),
                    token.span().clone(),
                    Vec::new(),
                )
            })
            .collect()
    }
}

fn ends_with_empty_lines(text: &str) -> bool {
    text.ends_with("\n\n\n\n")
        || text.ends_with("\n\r\n\r\n\r\n\r")
        || text.ends_with("\r\n\r\n\r\n\r\n")
}
