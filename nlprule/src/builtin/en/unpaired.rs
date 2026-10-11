//! Flags unpaired brackets and unpaired quotation marks.
//!
//! These two builtins reimplement the behaviour of LanguageTool's
//! `EnglishUnpairedBracketsRule` and `EnglishUnpairedQuotesRule` (both derive
//! from `GenericUnpairedBracketsRule` / `GenericUnpairedQuotesRule`), which run
//! Java-side rather than from rule XML. The rule ids are kept so that existing
//! suppressions and tests continue to work.
//!
//! Both rules are paragraph-level: they see every sentence of one paragraph, so
//! a symbol opened in one sentence and closed in a later one still pairs.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// LanguageTool rule id for unpaired brackets.
const BRACKET_ID: &str = "EN_UNPAIRED_BRACKETS";
/// LanguageTool rule id for unpaired quotation marks.
const QUOTE_ID: &str = "EN_UNPAIRED_QUOTES";

/// Brackets the rule knows about, in matching order.
const OPEN_BRACKETS: [char; 3] = ['[', '(', '{'];
/// Closing counterparts of [`OPEN_BRACKETS`], in the same order.
const CLOSE_BRACKETS: [char; 3] = [']', ')', '}'];

/// Opening quotation marks, in the order used by LanguageTool's English rule.
const START_QUOTES: [char; 4] = ['\u{201c}', '"', '\'', '\u{2018}'];
/// Closing counterparts of [`START_QUOTES`], in the same order.
const END_QUOTES: [char; 4] = ['\u{201d}', '"', '\'', '\u{2019}'];

/// A flattened token of a paragraph together with the sentence it came from.
struct Piece<'s> {
    text: &'s str,
    start: Position,
    end: Position,
    sentence: usize,
}

/// Flattens all tokens of a paragraph into one list so that a token's
/// neighbours can be inspected even across a sentence boundary.
fn flatten<'s>(sentences: &'s [Sentence<'_>]) -> Vec<Piece<'s>> {
    let mut pieces = Vec::new();
    for (sentence, s) in sentences.iter().enumerate() {
        for token in s.tokens() {
            pieces.push(Piece {
                text: token.word().text().as_str(),
                start: token.span().start(),
                end: token.span().end(),
                sentence,
            });
        }
    }
    pieces
}

/// The first character of a token, or `'\0'` for an empty token.
fn first_char(text: &str) -> char {
    text.chars().next().unwrap_or('\0')
}

/// Whether a token contains at least one alphanumeric character.
fn is_word(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric())
}

/// Whether token `k` starts a sentence.
fn is_sentence_start(pieces: &[Piece], k: usize) -> bool {
    k == 0 || pieces[k - 1].sentence != pieces[k].sentence
}

/// Whether token `k` directly abuts token `k - 1` (no whitespace between).
fn adjacent_prev(pieces: &[Piece], k: usize) -> bool {
    k > 0 && pieces[k - 1].end.byte == pieces[k].start.byte
}

/// Whether token `k` directly abuts token `k + 1` (no whitespace between).
fn adjacent_next(pieces: &[Piece], k: usize) -> bool {
    k + 1 < pieces.len() && pieces[k + 1].start.byte == pieces[k].end.byte
}

/// The `PUNCTUATION` character class of the unpaired-quotes rule: punctuation
/// except the quote characters and the underscore.
fn quote_punctuation(c: char) -> bool {
    (c.is_ascii_punctuation() && !matches!(c, '"' | '\'' | '_'))
        || matches!(c, '\u{2026}' | '\u{2013}' | '\u{2014}')
}

/// The `PUNCT_MARKS` character class of the unpaired-quotes rule.
fn punctuation_mark(c: char) -> bool {
    matches!(c, '?' | '.' | '!' | ',')
}

/// The opening mark that matches a closing mark, and vice versa.
fn corresponding_start(sym: char) -> char {
    let index = END_QUOTES
        .iter()
        .position(|&c| c == sym)
        .expect("symbol is a closing quote");
    START_QUOTES[index]
}

/// Whether `a` and `b` are a bracket pair (both a valid opening/closing order).
fn brackets_match(a: char, b: char) -> bool {
    OPEN_BRACKETS
        .iter()
        .position(|&c| c == a)
        .zip(CLOSE_BRACKETS.iter().position(|&c| c == b))
        .is_some_and(|(i, j)| i == j)
}

// ---------------------------------------------------------------------------
// Unpaired brackets
// ---------------------------------------------------------------------------

/// Flags brackets that are never closed within a paragraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpairedBrackets {}

impl UnpairedBrackets {
    /// Creates the rule.
    pub(crate) fn new() -> Self {
        UnpairedBrackets {}
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        BRACKET_ID
    }

    /// Checks all sentences of one paragraph and returns the suggestions.
    pub fn check(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let pieces = flatten(sentences);
        let mut stack: Vec<(char, Position, Position, usize, bool)> = Vec::new();

        for (k, piece) in pieces.iter().enumerate() {
            let sym = first_char(piece.text);
            if piece.text.chars().count() != 1 {
                continue;
            }
            if OPEN_BRACKETS.contains(&sym) {
                if is_bracket_exception(&pieces, k, sym) {
                    continue;
                }
                stack.push((sym, piece.start, piece.end, piece.sentence, true));
            } else if CLOSE_BRACKETS.contains(&sym) {
                if sym == ')' && is_bracket_exception(&pieces, k, sym) {
                    continue;
                }
                let closes_top = stack
                    .last()
                    .is_some_and(|&(open, ..)| brackets_match(open, sym));
                // A ")" directly after a word or number is a list marker such as
                // "a)" or "1)", not a real closing bracket, unless it is
                // actually closing an open "(".
                if sym == ')' && !closes_top && follows_word_or_number(&pieces, k) {
                    continue;
                }
                if closes_top {
                    stack.pop();
                } else {
                    stack.push((sym, piece.start, piece.end, piece.sentence, false));
                }
            }
        }

        report_brackets(sentences, &stack)
    }
}

/// Whether an opening bracket is part of a smiley such as `:)` or `;-)`.
fn is_bracket_exception(pieces: &[Piece], k: usize, sym: char) -> bool {
    if !matches!(sym, '(' | ')') || k == 0 || !adjacent_prev(pieces, k) {
        return false;
    }
    match first_char(pieces[k - 1].text) {
        ':' | ';' => true,
        '-' => {
            k > 1
                && adjacent_prev(pieces, k - 1)
                && matches!(first_char(pieces[k - 2].text), ':' | ';')
        }
        _ => false,
    }
}

/// Whether token `k` directly follows a word or number, e.g. the `)` in `2)`.
fn follows_word_or_number(pieces: &[Piece], k: usize) -> bool {
    if k == 0 || !adjacent_prev(pieces, k) {
        return false;
    }
    let previous = pieces[k - 1].text;
    previous
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Turns the remaining stack into suggestions. An opening bracket survives only
/// if it can be reported without risking a bracket that continues in the next
/// paragraph: LanguageTool reports it when the sentence ends like a real
/// sentence, or when the bracket is not in the paragraph's last sentence.
fn report_brackets(
    sentences: &[Sentence],
    stack: &[(char, Position, Position, usize, bool)],
) -> Vec<Suggestion> {
    let size = stack.len();
    let last = sentences.len().saturating_sub(1);

    // A balanced set of symbols with one extra in the middle, e.g. `({"})`,
    // is reported as that middle symbol only.
    if size > 2 && size % 2 == 1 {
        let symmetric = (0..size / 2).all(|i| {
            let a = stack[i].0;
            let b = stack[size - 1 - i].0;
            OPEN_BRACKETS
                .iter()
                .position(|&c| c == a)
                .zip(CLOSE_BRACKETS.iter().position(|&c| c == b))
                .is_some_and(|(x, y)| x == y)
        });
        if symmetric {
            let loc = &stack[size / 2];
            return vec![bracket_suggestion(loc.0, loc.1, loc.2)];
        }
    }

    stack
        .iter()
        .filter(|&&(_, _, _, sentence, opening)| {
            !opening || ends_like_real_sentence(sentences[sentence].text()) || last > sentence
        })
        .map(|&(sym, start, end, _, _)| bracket_suggestion(sym, start, end))
        .collect()
}

/// Whether a sentence ends with `.`, `?` or `!` after trimming whitespace.
fn ends_like_real_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|c| matches!(c, '.' | '?' | '!'))
}

fn bracket_suggestion(sym: char, start: Position, end: Position) -> Suggestion {
    Suggestion::new(
        format!("BUILTIN/{}", BRACKET_ID),
        format!(
            "Unpaired bracket {}: is a matching {} missing?",
            sym,
            matching_bracket(sym)
        ),
        Span::from_positions(start, end),
        Vec::new(),
    )
}

/// The bracket that would match `sym`.
fn matching_bracket(sym: char) -> char {
    if let Some(i) = OPEN_BRACKETS.iter().position(|&c| c == sym) {
        CLOSE_BRACKETS[i]
    } else {
        OPEN_BRACKETS[CLOSE_BRACKETS.iter().position(|&c| c == sym).unwrap()]
    }
}

// ---------------------------------------------------------------------------
// Unpaired quotation marks
// ---------------------------------------------------------------------------

/// Flags quotation marks that are never paired within a paragraph.
///
/// Because a `check` call sees a single paragraph only, an opening mark that is
/// still open at the end of the paragraph is never reported: it may be the
/// first half of a quotation that continues in the next paragraph, which is
/// normal fiction typography. A closing mark with no opening mark before it is
/// likewise treated as the tail of such a continuation. Only a genuinely
/// unmatched closing mark (one that follows opening marks in the same
/// paragraph) is reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpairedQuotes {}

impl UnpairedQuotes {
    /// Creates the rule.
    pub(crate) fn new() -> Self {
        UnpairedQuotes {}
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        QUOTE_ID
    }

    /// Checks all sentences of one paragraph and returns the suggestions.
    pub fn check(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let pieces = flatten(sentences);
        let mut opening: Vec<(char, Position, Position)> = Vec::new();
        let mut suggestions = Vec::new();
        let mut last_apostrophe: Option<char> = None;
        let mut was_inch = false;
        let mut seen_opening = false;

        for k in 0..pieces.len() {
            let sym = first_char(pieces[k].text);
            if pieces[k].text.chars().count() != 1 {
                continue;
            }
            if START_QUOTES.contains(&sym) && is_opening_quote(&pieces, k) {
                seen_opening = true;
                if is_apostrophe_beginning(&pieces, k) {
                    last_apostrophe = Some(sym);
                    continue;
                }
                if sym == '"' {
                    was_inch = false;
                }
                if last_apostrophe == Some(sym) {
                    last_apostrophe = None;
                }
                if let Some(index) = opening.iter().position(|&(c, ..)| c == sym) {
                    remove_all_inner(index as isize - 1, &mut opening);
                }
                opening.push((sym, pieces[k].start, pieces[k].end));
            } else if END_QUOTES.contains(&sym) && is_closing_quote(&pieces, k, &opening) {
                if is_apostrophe_beginning(&pieces, k) {
                    last_apostrophe = Some(sym);
                    continue;
                }
                let is_inch_symbol = sym == '"';
                let is_inch =
                    is_inch_symbol && sentence_has_inch(sentences[pieces[k].sentence].text());
                let start_symbol = corresponding_start(sym);
                if let Some(index) = opening.iter().position(|&(c, ..)| c == start_symbol) {
                    remove_all_inner(index as isize, &mut opening);
                    opening.remove(index);
                    if last_apostrophe == Some(start_symbol) {
                        last_apostrophe = None;
                    }
                    if is_inch {
                        was_inch = true;
                    }
                } else if !is_ending_apostrophe(&pieces, k)
                    && !is_inch
                    && (!is_inch_symbol || !was_inch)
                {
                    if last_apostrophe != Some(sym) {
                        if seen_opening {
                            suggestions.push(quote_suggestion(sym, pieces[k].start, pieces[k].end));
                        }
                    } else {
                        last_apostrophe = None;
                    }
                } else if is_inch || (is_inch_symbol && was_inch) {
                    was_inch = false;
                }
            }
        }

        suggestions
    }
}

/// A quote character directly followed by a word is an apostrophe (or an
/// elision) rather than an opening mark: `don’t`, `Marta’s`, `’90s`, `’tis`.
/// The left single quotation mark is always an opening mark, following fiction
/// convention.
fn is_apostrophe_beginning(pieces: &[Piece], k: usize) -> bool {
    let sym = first_char(pieces[k].text);
    if !matches!(sym, '\'' | '\u{2019}') {
        return false;
    }
    k + 1 < pieces.len() && adjacent_next(pieces, k) && is_word(pieces[k + 1].text)
}

/// A quote character directly preceded by a word is a trailing apostrophe:
/// `the Joneses’`, `rock ’n’`, `somethin’`.
fn is_ending_apostrophe(pieces: &[Piece], k: usize) -> bool {
    let sym = first_char(pieces[k].text);
    if !matches!(sym, '\'' | '\u{2019}') {
        return false;
    }
    k > 0 && adjacent_prev(pieces, k) && is_word(pieces[k - 1].text)
}

/// Whether a sentence contains a digit immediately followed by a double quote,
/// i.e. a measurement in inches such as `12"`.
fn sentence_has_inch(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars
        .windows(2)
        .any(|w| w[0].is_ascii_digit() && w[1] == '"')
}

/// LanguageTool's `isNotQuote`: a quote that stands alone between whitespace is
/// not treated as a quotation mark.
fn is_not_quote(pieces: &[Piece], k: usize, j: usize) -> bool {
    if (is_sentence_start(pieces, k) || !adjacent_prev(pieces, k))
        && (k + 1 >= pieces.len() || !adjacent_next(pieces, k))
    {
        return true;
    }
    if END_QUOTES[j] == START_QUOTES[j]
        && k > 0
        && k + 1 < pieces.len()
        && adjacent_prev(pieces, k)
        && adjacent_next(pieces, k)
    {
        let previous = first_char(pieces[k - 1].text);
        let next = pieces[k + 1].text;
        if quote_punctuation(previous) && next != "." && quote_punctuation(first_char(next)) {
            return true;
        }
    }
    false
}

/// LanguageTool's `isStartSymbolbefore`: whether a different opening symbol
/// appears earlier in the same whitespace-delimited run.
fn is_start_symbol_before(pieces: &[Piece], k: usize) -> bool {
    let current = pieces[k].text;
    let mut j = k as isize - 1;
    while j > 0 {
        let text = pieces[j as usize].text;
        if text != current && START_QUOTES.contains(&first_char(text)) {
            if is_sentence_start(pieces, j as usize) || !adjacent_prev(pieces, j as usize) {
                return true;
            }
        } else {
            return false;
        }
        j -= 1;
    }
    true
}

/// LanguageTool's `isOpeningQuote`, using only the token context available here.
fn is_opening_quote(pieces: &[Piece], k: usize) -> bool {
    let sym = first_char(pieces[k].text);
    let j = match START_QUOTES.iter().position(|&c| c == sym) {
        Some(j) => j,
        None => return false,
    };
    if is_not_quote(pieces, k, j) {
        return false;
    }
    if END_QUOTES.contains(&sym) {
        let attached_after_symbol = k + 1 < pieces.len()
            && adjacent_next(pieces, k)
            && k > 0
            && ((!punctuation_mark(first_char(pieces[k + 1].text))
                && quote_punctuation(first_char(pieces[k - 1].text)))
                || pieces[k - 1].text.ends_with('-'));
        return is_sentence_start(pieces, k)
            || !adjacent_prev(pieces, k)
            || attached_after_symbol
            || is_start_symbol_before(pieces, k);
    }
    true
}

/// LanguageTool's `isNotOpenSymbol`: a symmetric quote still has an opening
/// counterpart waiting on the stack.
fn is_not_open_symbol(j: usize, opening: &[(char, Position, Position)]) -> bool {
    if END_QUOTES[j] == START_QUOTES[j] {
        !opening.iter().any(|&(c, ..)| c == END_QUOTES[j])
    } else {
        true
    }
}

/// LanguageTool's `isClosingQuote`.
fn is_closing_quote(pieces: &[Piece], k: usize, opening: &[(char, Position, Position)]) -> bool {
    let sym = first_char(pieces[k].text);
    let j = match END_QUOTES.iter().position(|&c| c == sym) {
        Some(j) => j,
        None => return false,
    };
    !(is_not_quote(pieces, k, j) && is_not_open_symbol(j, opening))
}

/// Removes every open mark recorded after `index` (a negative index removes all
/// of them), mirroring LanguageTool's `removeAllOpenInnerQuotes`.
fn remove_all_inner(index: isize, opening: &mut Vec<(char, Position, Position)>) {
    let mut i = opening.len() as isize - 1;
    while i > index {
        opening.remove(i as usize);
        i -= 1;
    }
}

fn quote_suggestion(sym: char, start: Position, end: Position) -> Suggestion {
    Suggestion::new(
        format!("BUILTIN/{}", QUOTE_ID),
        format!(
            "Unpaired quotation mark {}: is a matching {} missing?",
            sym,
            corresponding_start(sym)
        ),
        Span::from_positions(start, end),
        Vec::new(),
    )
}
