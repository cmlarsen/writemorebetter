//! Flags a sentence that directly follows the previous one without whitespace.
//!
//! The behaviour follows the LanguageTool `SENTENCE_WHITESPACE` rule: when the
//! sentence splitter ends a sentence that is not followed by whitespace, as in
//! `"He came.Then he left."`, the first token of the next sentence is reported.
//!
//! A line or paragraph break between the sentences counts as whitespace,
//! U+2028 and U+2029 included. LanguageTool 6.6 tests the previous sentence
//! with Java's `String.trim`, which keeps those two, so it flags the sentence
//! after a U+2029; LanguageTool fixed that after 6.6 (commit `86c5f3a9`), and
//! this port follows the fix.

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
                if let Some(first) = pieces(sentence)
                    .into_iter()
                    .next()
                    .filter(|first| !is_line_break(first.text))
                {
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

/// Whether the last token of `sentence` is a single whitespace character: one
/// that Java's `String.trim` strips (control characters and the space), a
/// no-break space, or one of the line breaks NEL (U+0085), U+2028 and U+2029
/// that LanguageTool's `StringTools.containsLineBreak` adds after 6.6.
fn ends_with_whitespace(sentence: &Sentence) -> bool {
    matches!(
        sentence.text().chars().next_back(),
        Some(c) if c <= ' ' || matches!(c, '\u{85}' | '\u{a0}' | '\u{2028}' | '\u{2029}')
    )
}

/// Whether `piece` is a line break as LanguageTool's `StringTools.containsLineBreak`
/// counts them; a sentence that opens with one is not flagged. The sentence
/// splitter currently leaves a break at the end of the previous sentence, so
/// `ends_with_whitespace` catches it first; this keeps the upstream check in
/// case the split changes (e.g. #179).
fn is_line_break(piece: &str) -> bool {
    matches!(
        piece,
        "\n" | "\r" | "\u{b}" | "\u{c}" | "\u{85}" | "\u{2028}" | "\u{2029}"
    )
}
