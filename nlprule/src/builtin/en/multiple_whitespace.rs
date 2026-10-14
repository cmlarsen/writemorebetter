//! Flags a run of repeated whitespace characters.
//!
//! The behaviour follows the LanguageTool `WHITESPACE_RULE`: two or more
//! whitespace characters in a row are reported and replaced by the first one.
//! Line breaks are never part of a run. Indentation after a line break inside a
//! sentence is left alone, but indentation that opens a sentence, or follows a
//! `\r\n`, is reported. [`MultipleWhitespace::skip_indentation`] leaves all
//! indentation alone.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "WHITESPACE_RULE";

const MESSAGE: &str = "Possible typo: you repeated a whitespace";

/// Flags a run of repeated whitespace, e.g. two spaces between words.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MultipleWhitespace {
    #[serde(skip)]
    skip_indentation: bool,
}

/// One token in the sense of LanguageTool: a real token, or a single
/// whitespace character (`\r\n` is two tokens).
pub(crate) struct Piece<'a> {
    pub(crate) text: &'a str,
    pub(crate) start: Position,
    pub(crate) end: Position,
}

impl MultipleWhitespace {
    pub(crate) fn new() -> Self {
        MultipleWhitespace::default()
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Leaves the whitespace at the start of a line alone: after a line break
    /// (including U+2028 and U+2029) and at the start of the text. Fiction
    /// manuscripts indent paragraphs with spaces, which LanguageTool reports
    /// whenever the previous paragraph ends a sentence.
    pub fn skip_indentation(&mut self) {
        self.skip_indentation = true;
    }

    /// Checks the consecutive sentences of one paragraph. `previous` is the
    /// last sentence of the preceding paragraph, if any; without it the first
    /// sentence starts the text.
    pub fn check_with_previous(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
    ) -> Vec<Suggestion> {
        let mut line_start = previous.is_none_or(ends_line);
        let mut suggestions = Vec::new();

        for sentence in sentences {
            suggestions.extend(self.check_sentence(sentence, line_start));
            line_start = ends_line(sentence);
        }

        suggestions
    }

    fn check_sentence(&self, sentence: &Sentence, line_start: bool) -> Vec<Suggestion> {
        let pieces = pieces(sentence);
        let removable = |piece: &str| {
            is_removable_white(piece) && !(self.skip_indentation && is_line_end(piece))
        };
        let mut suggestions = Vec::new();

        let mut i = 0;
        if self.skip_indentation && line_start {
            i = after_indentation(&pieces, i);
        }
        while i < pieces.len() {
            if self.skip_indentation && is_line_end(pieces[i].text) {
                i = after_indentation(&pieces, i + 1);
                continue;
            }
            if is_first_white(pieces[i].text) {
                let first = i;
                i += 1;
                while i < pieces.len() && removable(pieces[i].text) {
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

/// The index of the first piece from `i` on that is not indentation.
fn after_indentation(pieces: &[Piece], mut i: usize) -> usize {
    while i < pieces.len() && is_first_white(pieces[i].text) && !is_line_end(pieces[i].text) {
        i += 1;
    }
    i
}

fn ends_line(sentence: &Sentence) -> bool {
    matches!(
        sentence.text().chars().next_back(),
        Some('\n' | '\r' | '\u{2028}' | '\u{2029}')
    )
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
/// [`char::is_whitespace`], U+0085 and the figure space U+2007 do not count,
/// and the separators U+001C to U+001F do, as in Java.
pub(crate) fn is_whitespace(piece: &str) -> bool {
    let mut chars = piece.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => {
            (c.is_whitespace() && c != '\u{85}' && c != '\u{2007}')
                || ('\u{1c}'..='\u{1f}').contains(&c)
        }
        _ => false,
    }
}

fn is_linebreak(piece: &str) -> bool {
    matches!(piece, "\n" | "\r")
}

fn is_line_end(piece: &str) -> bool {
    matches!(piece, "\n" | "\r" | "\u{2028}" | "\u{2029}")
}

fn is_first_white(piece: &str) -> bool {
    is_whitespace(piece) && !is_linebreak(piece)
}

fn is_removable_white(piece: &str) -> bool {
    is_first_white(piece) && piece != "\t"
}
