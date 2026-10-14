//! Flags a run of repeated whitespace characters.
//!
//! The behaviour follows the LanguageTool `WHITESPACE_RULE`: two or more
//! whitespace characters in a row are reported and replaced by the first one.
//! Line breaks are never part of a run, and the indentation that follows a line
//! break is left alone.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "WHITESPACE_RULE";

const MESSAGE: &str = "Possible typo: you repeated a whitespace";

/// Flags a run of repeated whitespace, e.g. two spaces between words.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultipleWhitespace;

/// One token in the sense of LanguageTool: a real token, or a single
/// whitespace character (`\r\n` is two tokens).
pub(crate) struct Piece<'a> {
    pub(crate) text: &'a str,
    pub(crate) start: Position,
    pub(crate) end: Position,
}

impl MultipleWhitespace {
    pub(crate) fn new() -> Self {
        MultipleWhitespace
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let pieces = pieces(sentence);
        let mut suggestions = Vec::new();

        let mut i = 0;
        while i < pieces.len() {
            if is_first_white(pieces[i].text) {
                let first = i;
                i += 1;
                while i < pieces.len() && is_removable_white(pieces[i].text) {
                    i += 1;
                }
                i -= 1;
                if i > first {
                    suggestions.push(Suggestion::new(
                        format!("BUILTIN/{}", ID),
                        MESSAGE.to_owned(),
                        Span::from_positions(pieces[first].start, pieces[i].end),
                        vec![pieces[first].text.to_owned()],
                    ));
                }
            } else if is_linebreak(pieces[i].text) {
                i += 1;
                while i < pieces.len() && is_removable_white(pieces[i].text) {
                    i += 1;
                }
            }
            i += 1;
        }

        suggestions
    }
}

/// Splits `sentence` into its tokens and the single whitespace characters
/// between them, in text order.
pub(crate) fn pieces<'a>(sentence: &'a Sentence<'a>) -> Vec<Piece<'a>> {
    let base = sentence.span().start();
    let text = sentence.text();
    let mut result = Vec::new();
    let mut position = base;

    for token in sentence.tokens() {
        let span = token.span();
        push_gap(text, base, &mut result, position, span.start());
        result.push(Piece {
            text: &text[span.byte().start - base.byte..span.byte().end - base.byte],
            start: span.start(),
            end: span.end(),
        });
        position = span.end();
    }
    push_gap(text, base, &mut result, position, sentence.span().end());

    result
}

/// Appends the whitespace between the absolute positions `from` and `to`,
/// one character per piece.
fn push_gap<'a>(
    text: &'a str,
    base: Position,
    result: &mut Vec<Piece<'a>>,
    from: Position,
    to: Position,
) {
    let gap = &text[from.byte - base.byte..to.byte - base.byte];
    let mut start = from;
    for (offset, c) in gap.char_indices() {
        let end = Position {
            byte: start.byte + c.len_utf8(),
            char: start.char + 1,
        };
        result.push(Piece {
            text: &gap[offset..offset + c.len_utf8()],
            start,
            end,
        });
        start = end;
    }
}

/// Whether `piece` is a whitespace character in LanguageTool's sense. Unlike
/// [`char::is_whitespace`], U+0085 and the figure space U+2007 do not count.
pub(crate) fn is_whitespace(piece: &str) -> bool {
    let mut chars = piece.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_whitespace() && c != '\u{85}' && c != '\u{2007}',
        _ => false,
    }
}

fn is_linebreak(piece: &str) -> bool {
    matches!(piece, "\n" | "\r")
}

fn is_first_white(piece: &str) -> bool {
    is_whitespace(piece) && !is_linebreak(piece)
}

fn is_removable_white(piece: &str) -> bool {
    is_first_white(piece) && piece != "\t"
}
