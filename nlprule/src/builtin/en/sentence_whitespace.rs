//! Flags a sentence that directly follows the previous one without whitespace.
//!
//! The behaviour follows the LanguageTool `SENTENCE_WHITESPACE` rule: when the
//! sentence splitter ends a sentence that is not followed by whitespace, as in
//! `"He came.Then he left."`, the first token of the next sentence is reported.

use crate::types::{Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

use super::multiple_whitespace::pieces;

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "SENTENCE_WHITESPACE";

const MESSAGE: &str = "Add a space between sentences.";

/// Flags a missing space between two sentences, e.g. `"Hello world.This is it."`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SentenceWhitespace;

impl SentenceWhitespace {
    pub(crate) fn new() -> Self {
        SentenceWhitespace
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the consecutive sentences of one paragraph. `previous` is the
    /// last sentence of the preceding paragraph, if any; without it the first
    /// sentence is the start of the text and is never flagged.
    pub fn check_with_previous(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        let mut previous_ends_with_whitespace = previous.map(ends_with_whitespace);

        for sentence in sentences {
            if previous_ends_with_whitespace == Some(false) {
                if let Some(first) = pieces(sentence).into_iter().next() {
                    suggestions.push(Suggestion::new(
                        format!("BUILTIN/{}", ID),
                        MESSAGE.to_owned(),
                        Span::from_positions(first.start, first.end),
                        vec![format!(" {}", first.text)],
                    ));
                }
            }
            previous_ends_with_whitespace = Some(ends_with_whitespace(sentence));
        }

        suggestions
    }
}

/// Whether the last token of `sentence` is a single whitespace character in the
/// sense of Java's `String.trim`, which strips control characters and the
/// space, or a no-break space.
fn ends_with_whitespace(sentence: &Sentence) -> bool {
    matches!(
        sentence.text().chars().next_back(),
        Some(c) if c <= ' ' || c == '\u{a0}'
    )
}
