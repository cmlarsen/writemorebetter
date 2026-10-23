//! Flags two successive paragraphs that begin with the same word.
//!
//! The behaviour follows LanguageTool 6.6's `ParagraphRepeatBeginningRule`
//! (`PARAGRAPH_REPEAT_BEGINNING_RULE`), a text-level rule in the `STYLE` category that is off by
//! default. It compares the first sentence of each LanguageTool paragraph (see
//! [`crate::document`]) with the first sentence of the next one:
//!
//! - a shared leading quote or bracket (`’ ' " „ “ ” » « ‚ ‘ › ‹ ( ) [ ]`) is skipped;
//! - the first word must start with a letter (so `—She` and `…and` never match);
//! - a shared determiner (a token tagged `DT`: `The`, `A`, `This`, …) is skipped when the earlier
//!   sentence has a token after it, and the next words are compared instead (`The dog` / `The
//!   cat` is no match);
//! - the tokens are compared case-sensitively, with `’` normalized to `'` as LanguageTool's
//!   tagger does.
//!
//! A repeat gives two matches with the message "Same beginning as last paragraph": one on the
//! earlier paragraph, from its first token to the end of the repeated word, and one on the later
//! paragraph. LanguageTool ends the later one at the earlier one's end offset relative to its own
//! sentence, so when the two sentences start with different whitespace (`"  She ran.\n\nShe
//! hid."`) the later match is longer or shorter than the word (`She h`); this is kept. A later
//! match that would be empty or inverted, where LanguageTool 6.6 throws and fails the whole
//! check, is left out.
//!
//! A paragraph whose first sentence has no token (a whitespace-only sentence, as LanguageTool's
//! sentence splitter makes of `"\n\n"` at the start of a text) matches nothing.

use crate::{
    document::{At, Beginning, BeginningToken, DocumentFix, DocumentMatch, LtParagraph, Offset},
    types::{Sentence, Span},
};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "PARAGRAPH_REPEAT_BEGINNING_RULE";

const MESSAGE: &str = "Same beginning as last paragraph";

/// How many tokens of a sentence the comparison can reach: a quote, a determiner and a word.
const COMPARED_TOKENS: usize = 3;

/// Flags successive paragraphs that begin with the same word.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParagraphRepeatBeginning {}

impl ParagraphRepeatBeginning {
    pub(crate) fn new() -> Self {
        ParagraphRepeatBeginning {}
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// The matches in a text made of `paragraphs`, in text order.
    pub(crate) fn resolve(&self, paragraphs: &[LtParagraph]) -> Vec<DocumentMatch> {
        let mut matches = Vec::new();
        for pair in paragraphs.windows(2) {
            let (Some(last), Some(next)) = (&pair[0].beginning, &pair[1].beginning) else {
                continue;
            };
            let Some(n) = equal_beginning(last, next) else {
                continue;
            };
            let end = last.tokens[n].end;
            let first = last.tokens[0].start;
            if first.offset.char < end.offset.char {
                matches.push(self.mark(first, end.offset));
            }
            // LanguageTool's `nextPos + endPos`: the earlier word's end, relative to the start of
            // the earlier sentence, applied to the start of the later sentence.
            let start = next.tokens[0].start.offset;
            let shift = |x: fn(&Offset) -> u32| {
                i64::from(x(&start)) + i64::from(x(&last.lead)) - i64::from(x(&next.lead))
                    + i64::from(x(&end.offset))
                    - i64::from(x(&first.offset))
            };
            let char = shift(|x| x.char);
            // The byte offset of that char: where a token of the later sentence starts or ends
            // there (the usual case), else an estimate, which `DocumentMatch::to_suggestion`
            // corrects from the text.
            let end = next
                .tokens
                .iter()
                .flat_map(|x| [x.start.offset, x.end.offset])
                .find(|x| i64::from(x.char) == char)
                .or_else(|| {
                    let char = u32::try_from(char).ok().filter(|x| *x > start.char)?;
                    // At least one byte per char.
                    let byte = u32::try_from(shift(|x| x.byte))
                        .unwrap_or(0)
                        .max(start.byte + (char - start.char));
                    Some(Offset { byte, char })
                });
            if let Some(end) = end.filter(|x| start.char < x.char) {
                matches.push(self.mark(next.tokens[0].start, end));
            }
        }
        matches
    }

    fn mark(&self, start: At, end: Offset) -> DocumentMatch {
        DocumentMatch::new(
            start.digest,
            format!("BUILTIN/{}", ID),
            MESSAGE.to_owned(),
            Span::from_positions(start.offset.position(), end.position()),
            DocumentFix::Mark,
        )
    }
}

/// LanguageTool's `numCharEqualBeginning`: the index of the earlier sentence's token whose end
/// ends both matches, if the sentences begin alike.
fn equal_beginning(last: &Beginning<At>, next: &Beginning<At>) -> Option<usize> {
    let (a, b) = (&last.tokens, &next.tokens);
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let mut n = 0;
    if a[n].is_quote() && a[n].hash == b[n].hash {
        if a.len() <= n + 1 || b.len() <= n + 1 {
            return None;
        }
        n += 1;
    }
    if !a[n].starts_with_letter() {
        return None;
    }
    if a.len() > n + 1 && a[n].is_determiner() && a[n].hash == b[n].hash {
        if b.len() <= n + 1 {
            return None;
        }
        n += 1;
    }
    if !a[n].starts_with_letter() {
        return None;
    }
    (a[n].hash == b[n].hash).then_some(n)
}

/// The first tokens of `sentence`, which starts at `start` in the digest's text, as the rule
/// compares them; `None` for a sentence without tokens. `offset` turns a position in the checked
/// text into an offset in the digest's text.
pub(crate) fn beginning(
    sentence: &Sentence,
    offset: impl Fn(crate::types::Position) -> Offset,
) -> Option<Beginning<Offset>> {
    if sentence.tokens().is_empty() {
        return None;
    }
    let tokens = sentence
        .tokens()
        .iter()
        .take(COMPARED_TOKENS)
        .map(|token| {
            let text = token.word().text().as_str();
            let mut chars = text.chars();
            let first = chars.next();
            let is_quote = chars.next().is_none()
                && first.is_some_and(|c| {
                    matches!(
                        c,
                        '\u{2019}'
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
                    )
                });
            // Java's `Character.isLetter(charAt(0))` sees the first UTF-16 unit, a surrogate
            // for an astral char.
            let starts_with_letter = first.is_some_and(|c| c.is_alphabetic() && c.len_utf16() == 1);
            let is_determiner = token
                .word()
                .tags()
                .iter()
                .any(|tag| tag.pos().as_str().starts_with("DT"));
            BeginningToken::new(
                offset(token.span().start()),
                offset(token.span().end()),
                text,
                is_quote,
                starts_with_letter,
                is_determiner,
            )
        })
        .collect();
    let lead = Offset::new(sentence.tokens()[0].span().start() - sentence.span().start());
    Some(Beginning { lead, tokens })
}
