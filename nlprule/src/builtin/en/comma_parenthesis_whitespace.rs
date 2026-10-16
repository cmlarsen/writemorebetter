//! Flags whitespace around commas, full stops, parentheses and quotation marks.
//!
//! The behaviour follows the LanguageTool `COMMA_PARENTHESIS_WHITESPACE` rule:
//! whitespace after an opening parenthesis and before a closing parenthesis,
//! space around a comma or full stop, a missing space after a comma between
//! words and whitespace on both sides of a quotation mark are reported.

use crate::types::{Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "COMMA_PARENTHESIS_WHITESPACE";

const MESSAGE_NO_SPACE_AFTER: &str = "Don\u{2019}t put a space after the opening parenthesis.";
const MESSAGE_NO_SPACE_BEFORE: &str = "Don\u{2019}t put a space before the closing parenthesis.";
const MESSAGE_NO_SPACE_BEFORE_DOT: &str = "Don\u{2019}t put a space before the full stop.";
const MESSAGE_MISSING_SPACE_AFTER_COMMA: &str = "Put a space after the comma.";
const MESSAGE_SPACE_AFTER_COMMA: &str = "Put a space after the comma, but not before the comma.";
const MESSAGE_NO_SPACE_AROUND_QUOTES: &str =
    "Don\u{2019}t put a space on both sides of a quote symbol.";

/// Flags whitespace around commas, full stops, parentheses and quotation marks.
///
/// The rule works on the tokens of one sentence. Whitespace is not tokenized,
/// so a token only carries whether whitespace precedes it; the original text is
/// used to recover the whitespace runs. Anything taken from LanguageTool's
/// hardcoded exception list (the top level domains that may directly follow a
/// full stop) lives in `domains`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommaParenthesisWhitespace {
    /// Top level domains that may follow a full stop without a space, e.g.
    /// `foo.org`. Any two-letter sequence is treated as a domain as well.
    domains: Vec<String>,
}

/// One element of the token stream the rule walks over: either a real token or
/// the run of whitespace that precedes a token.
struct Piece<'a> {
    text: &'a str,
    start: Position,
    end: Position,
    whitespace: bool,
}

impl CommaParenthesisWhitespace {
    pub(crate) fn new(domains: Vec<String>) -> Self {
        CommaParenthesisWhitespace {
            domains: domains.into_iter().map(|x| x.to_lowercase()).collect(),
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        self.check_sentence(sentence, None)
    }

    /// Checks the consecutive sentences of one paragraph.
    ///
    /// The sentence splitter used by the engine can break between a full stop
    /// and a directly following word (`the .Net framework` becomes `the .` and
    /// `Net framework`), so the first token of the following sentence is passed
    /// on as the token after the last one when the two are adjacent.
    pub fn check_sentences(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        for (i, sentence) in sentences.iter().enumerate() {
            let next = sentences
                .get(i + 1)
                .and_then(|next| adjacent_token(sentence, next));
            suggestions.extend(self.check_sentence(sentence, next));
        }

        suggestions
    }

    fn check_sentence(&self, sentence: &Sentence, after_last: Option<&str>) -> Vec<Suggestion> {
        let pieces = pieces(sentence);
        let mut suggestions = Vec::new();

        for i in 1..pieces.len() {
            let current = &pieces[i];
            let previous = &pieces[i - 1];
            let second_previous = &pieces[i.saturating_sub(2)];
            let previous_is_space = previous.whitespace;

            let next = pieces.get(i + 1);
            // The end of the sentence behaves like a whitespace token in
            // LanguageTool, so a final comma is never merged with a next word.
            let (next_text, next_is_space) = match next {
                Some(piece) => (piece.text, piece.whitespace),
                None => (after_last.unwrap_or(""), after_last.is_none()),
            };

            let mut replacement: Option<(Span, Vec<String>, &str)> = None;

            if current.whitespace && previous.text.starts_with('(') {
                // Whitespace directly after an opening parenthesis.
                replacement = Some((
                    Span::from_positions(previous.start, current.end),
                    vec![previous.text.to_owned()],
                    MESSAGE_NO_SPACE_AFTER,
                ));
            } else if current.whitespace
                && is_quote(previous.text)
                && second_previous.text == " "
                && !(i >= 3
                    && (is_ellipsis_dot(&pieces, i - 3) || is_any_quote(pieces[i - 3].text)))
                && !(!current.text.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
                    && pieces.get(i + 1).is_some_and(|piece| {
                        is_ellipsis_dot(&pieces, i + 1) || is_any_quote(piece.text)
                    }))
            {
                // Whitespace on both sides of a quotation mark.
                replacement = Some((
                    Span::from_positions(second_previous.start, current.end),
                    vec![format!("{} ", previous.text), format!(" {}", previous.text)],
                    MESSAGE_NO_SPACE_AROUND_QUOTES,
                ));
            } else if !current.whitespace
                && previous.text == ","
                && starts_with_word(current.text)
                && !contains_digit(second_previous.text)
                && !contains_digit(current.text)
                && second_previous.text != ","
            {
                // A comma followed directly by another word. Only tokens that
                // start with a letter or digit count as words, so punctuation
                // such as the em dash in `word,—word` is not flagged.
                replacement = Some((
                    Span::from_positions(previous.start, current.end),
                    vec![format!(", {}", current.text)],
                    MESSAGE_MISSING_SPACE_AFTER_COMMA,
                ));
            } else if previous_is_space {
                if current.text.starts_with(')') {
                    // Whitespace directly before a closing parenthesis.
                    replacement = Some((
                        Span::from_positions(previous.start, current.end),
                        vec![current.text.to_owned()],
                        MESSAGE_NO_SPACE_BEFORE,
                    ));
                } else if current.text == "," && !(i >= 2 && is_ellipsis_dot(&pieces, i - 2)) {
                    // Whitespace directly before a comma. A following word is
                    // merged in when no whitespace separates it from the comma.
                    let doubled = next.map(|piece| !piece.whitespace && piece.text == ",");
                    if !doubled.unwrap_or(false) {
                        let text = if next_is_space { "," } else { ", " };
                        replacement = Some((
                            Span::from_positions(previous.start, current.end),
                            vec![text.to_owned()],
                            MESSAGE_SPACE_AFTER_COMMA,
                        ));
                    }
                } else if current.text == "."
                    && !self.is_domain(next_text)
                    && !is_file_extension(next_text)
                    && !is_digit_or_dot(next_text)
                    && !in_spaced_ellipsis(&pieces, i)
                {
                    // Whitespace directly before a full stop. Figures such as
                    // `.5`, ellipses and commands such as `./script.sh` are
                    // excluded.
                    let after_next = pieces.get(i + 2).map(|piece| piece.text).unwrap_or("");
                    let command =
                        next_text == "/" && !after_next.is_empty() && is_letters(after_next);
                    if !command {
                        replacement = Some((
                            Span::from_positions(previous.start, current.end),
                            vec![".".to_owned()],
                            MESSAGE_NO_SPACE_BEFORE_DOT,
                        ));
                    }
                }
            }

            if let Some((span, replacements, message)) = replacement {
                suggestions.push(Suggestion::new(
                    format!("BUILTIN/{}", ID),
                    message.to_owned(),
                    span,
                    replacements,
                ));
            }
        }

        suggestions
    }

    /// Whether `token` is a domain that may follow a full stop without a space.
    fn is_domain(&self, token: &str) -> bool {
        let mut chars = token.chars();
        if let (Some(first), Some(second), None) = (chars.next(), chars.next(), chars.next()) {
            if first.is_ascii_alphabetic() && second.is_ascii_alphabetic() {
                return true;
            }
        }
        self.domains
            .iter()
            .any(|domain| domain.eq_ignore_ascii_case(token))
    }
}

/// The first token of `next` if it directly follows the last token of
/// `sentence` without any whitespace in between.
fn adjacent_token<'s, 't>(sentence: &'s Sentence<'t>, next: &'s Sentence<'t>) -> Option<&'s str> {
    let last = sentence.tokens().last()?;
    let first = next.tokens().first()?;
    if last.span().end() == first.span().start() {
        Some(first.word().text().0.as_ref())
    } else {
        None
    }
}

/// Splits the tokens of `sentence` into real tokens and the whitespace runs
/// between them. The result starts with an empty sentinel that stands for the
/// beginning of the sentence.
fn pieces<'a>(sentence: &'a Sentence<'a>) -> Vec<Piece<'a>> {
    let base = sentence.span().start();
    let text = sentence.text();
    let mut result = vec![Piece {
        text: "",
        start: base,
        end: base,
        whitespace: false,
    }];

    let mut previous_end = base;
    for token in sentence.tokens() {
        let start = token.span().start();
        let end = token.span().end();

        if token.has_space_before() {
            result.push(Piece {
                text: slice_between(text, base, previous_end, start),
                start: previous_end,
                end: start,
                whitespace: true,
            });
        }

        result.push(Piece {
            text: token.word().text().as_str(),
            start,
            end,
            whitespace: false,
        });
        previous_end = end;
    }

    result
}

/// Slices the local sentence text between two absolute positions.
fn slice_between(text: &str, base: Position, start: Position, end: Position) -> &str {
    let bytes = (start.byte - base.byte)..(end.byte - base.byte);
    debug_assert!(text.is_char_boundary(bytes.start) && text.is_char_boundary(bytes.end));
    &text[bytes]
}

/// A token counts as a quote if it is a single quote character.
fn is_quote(token: &str) -> bool {
    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => matches!(
            c,
            '\'' | '"' | '\u{2019}' | '\u{201d}' | '\u{201c}' | '\u{00ab}' | '\u{00bb}'
        ),
        _ => false,
    }
}

fn is_any_quote(token: &str) -> bool {
    is_quote(token) || token == "\u{2018}"
}

/// The characters that may follow a comma; anything else (punctuation such as
/// an em dash) is not a word and is left alone.
fn starts_with_word(token: &str) -> bool {
    matches!(token.chars().next(), Some(c) if c.is_alphanumeric())
}

fn contains_digit(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_digit())
}

fn is_digit_or_dot(token: &str) -> bool {
    matches!(token.chars().next(), Some(c) if c == '.' || c.is_ascii_digit())
}

fn is_ellipsis_dot(pieces: &[Piece], i: usize) -> bool {
    pieces[i].text == "." && !pieces[i].whitespace && in_spaced_ellipsis(pieces, i)
}

fn in_spaced_ellipsis(pieces: &[Piece], i: usize) -> bool {
    let is_dot = |piece: &Piece| !piece.whitespace && piece.text == ".";
    let mut dots = 1;

    let mut j = i;
    while j >= 2 && pieces[j - 1].whitespace && is_dot(&pieces[j - 2]) {
        dots += 1;
        j -= 2;
    }

    let mut j = i;
    while j + 2 < pieces.len() && pieces[j + 1].whitespace && is_dot(&pieces[j + 2]) {
        dots += 1;
        j += 2;
    }

    dots >= 3
}

fn is_letters(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_ascii_alphabetic())
}

/// Whether `token` looks like a file name part such as `.exe`, `.MP3` or
/// `.exe-Datei`: three or four letters, optionally followed by a hyphen and
/// more, or the short forms `ai`, `mp3` and `mp4`.
fn is_file_extension(token: &str) -> bool {
    let base = match token.split_once('-') {
        Some((base, rest)) => {
            if rest.is_empty() {
                return false;
            }
            base
        }
        None => token,
    };

    let chars: Vec<char> = base.chars().collect();
    if (3..=4).contains(&chars.len())
        && (chars.iter().all(|c| c.is_ascii_lowercase())
            || chars.iter().all(|c| c.is_ascii_uppercase()))
    {
        return true;
    }

    matches!(base, "ai" | "mp3" | "mp4" | "MP3" | "MP4")
}
