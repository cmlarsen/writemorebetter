//! Flags a sentence that does not start with an uppercase letter.

use crate::types::{Sentence, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The identifier of the rule this builtin reimplements.
const ID: &str = "UPPERCASE_SENTENCE_START";

/// Message shown for a sentence that starts with a lowercase letter.
const MESSAGE: &str = "This sentence does not start with an uppercase letter.";

/// Flags a sentence that does not start with an uppercase letter.
///
/// The rule is checked on all sentences of one paragraph because whether a
/// lowercase word really starts a new sentence can only be decided from the
/// previous one. The logic follows the corresponding rule of the rule set; in
/// addition, a sentence that continues after an ellipsis is never flagged,
/// because fiction uses an ellipsis in the middle of a sentence where the
/// sentence splitter may treat it as a boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UppercaseSentenceStart {
    /// Tokens that may start a sentence in lowercase, e.g. `"pH"`.
    exceptions: Vec<String>,
}

impl UppercaseSentenceStart {
    pub(crate) fn new(exceptions: Vec<String>) -> Self {
        UppercaseSentenceStart { exceptions }
    }

    /// The rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks the sentences of one paragraph and returns the resulting suggestions.
    pub fn check(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        self.check_with_previous(sentences, &[])
    }

    /// Checks the sentences of one paragraph, using the end of the preceding
    /// paragraph as context: its last sentence with tokens and any
    /// whitespace-only sentences after it. The corresponding LanguageTool rule
    /// is text-level, so the first sentence of a paragraph is only checked
    /// against the sentence before it; without that context a paragraph after a
    /// line that is not a real sentence end (`Hello;\n\nworld.`) would be
    /// flagged. Only the sentences of the paragraph itself are reported.
    pub fn check_with_previous(
        &self,
        sentences: &[Sentence],
        previous: &[Sentence],
    ) -> Vec<Suggestion> {
        // A paragraph that consists of a single word is not useful to complain
        // about; the missing capitalization may just be a typo.
        let mut with_tokens = sentences.iter().filter(|x| !x.tokens().is_empty());
        match (with_tokens.next(), with_tokens.next()) {
            (None, _) => return Vec::new(),
            (Some(sentence), None) if sentence.tokens().len() == 1 => return Vec::new(),
            _ => {}
        }

        let suppress_before = (!previous.is_empty()).then(|| sentences[0].span().char().start);
        let mut context: Vec<&Sentence> = Vec::with_capacity(previous.len() + sentences.len());
        context.extend(previous.iter());
        context.extend(sentences.iter());

        self.run(&context, suppress_before)
    }

    /// The rule loop. `suppress_before` is the char position at which the
    /// reportable paragraph starts; suggestions before it come from the context
    /// sentence and are dropped.
    fn run(&self, sentences: &[&Sentence], suppress_before: Option<usize>) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        // The last token of the previous sentence, with a trailing quote or
        // whitespace removed.
        let mut last_token = String::new();
        // Whether the previous sentence was the marker of a numbered list.
        let mut numbered_list = false;
        // Whether the previous sentence ends with an ellipsis. The sentence
        // splitter treats an ellipsis followed by a lowercase word as a
        // sentence boundary, but in fiction the word continues the sentence.
        let mut after_ellipsis = false;

        for sentence in sentences {
            let tokens = sentence.tokens();
            // A whitespace-only sentence (the blank line after `"End.  \n\n"`) leaves the
            // previous sentence as the context, as in LanguageTool, and so does the ellipsis,
            // but it does end a numbered list marker (`"1.  \n\nthe item."` is flagged).
            if tokens.is_empty() {
                numbered_list = false;
                continue;
            }

            let check_pos = check_token_pos(tokens);
            let check_token = tokens[check_pos].word().text().as_str();

            let mut last = tokens.len() - 1;
            if last > 0 && is_whitespace_or_quote(tokens[last].word().text().as_str()) {
                last -= 1;
            }
            let last_text = tokens[last].word().text().as_str();

            let mut prevent = false;
            // A sentence after a comma or semicolon continues the previous one,
            // e.g. a dialogue tag after a comma inside a quote.
            if last_token == "," || last_token == ";" {
                prevent = true;
            }
            // An ellipsis continues a sentence instead of ending it.
            if after_ellipsis {
                prevent = true;
            }
            // A token that contains a digit is not checked.
            if check_token.chars().any(|c| c.is_ascii_digit()) {
                prevent = true;
            }
            // Both this sentence and the previous one have to look like a
            // sentence, otherwise the lowercase word is a continuation.
            if !is_sentence_end_or_empty(&last_token) && !is_sentence_end(last_text) {
                prevent = true;
            }
            if !sentence.text().replace('\u{a0}', " ").trim().is_empty() {
                last_token = last_text.to_owned();
            }
            // Lowercase list markers such as `"a)"` and `"iv."` are allowed.
            if check_pos + 1 < tokens.len()
                && is_numeral(check_token)
                && matches!(tokens[check_pos + 1].word().text().as_str(), "." | ")")
            {
                prevent = true;
            }
            if numbered_list || is_url(check_token) || is_email(check_token) {
                prevent = true;
            }

            if let Some(first) = check_token.chars().next() {
                let capitalized = uppercase_first_char(check_token);
                if capitalized != check_token
                    && !prevent
                    && first.is_lowercase()
                    && !only_lowercase_start(check_token)
                    && !self.exceptions.iter().any(|x| x == check_token)
                    && !is_camel_case(check_token)
                    && suppress_before
                        .is_none_or(|before| tokens[check_pos].span().char().start >= before)
                {
                    suggestions.push(Suggestion::new(
                        format!("BUILTIN/{}", ID),
                        MESSAGE.to_owned(),
                        tokens[check_pos].span().clone(),
                        vec![capitalized],
                    ));
                }
            }

            numbered_list = digit_dot(sentence.text()) || linebreak_digit_dot(sentence.text());
            after_ellipsis = ends_with_ellipsis(sentence.text());
        }

        suggestions
    }
}

/// The index of the token that has to be capitalized. A leading quote is
/// skipped so that `"hello"` is checked at `hello`, not at the quote.
fn check_token_pos(tokens: &[Token]) -> usize {
    if tokens.len() >= 2 && is_quote_start(tokens[0].word().text().as_str()) {
        1
    } else {
        0
    }
}

/// A single character that may be a trailing quote or whitespace.
fn is_whitespace_or_quote(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(c), None)
            if matches!(
                c,
                ' ' | '"' | '\'' | '\u{201e}' | '\u{00ab}' | '\u{00bb}' | '\u{2018}' | '\u{2019}'
                    | '\u{201c}' | '\u{201d}' | '\n'
            )
    )
}

/// Characters that may open a quote.
fn is_quote_start(token: &str) -> bool {
    matches!(
        token,
        "\"" | "'"
            | "\u{201e}"
            | "\u{00bb}"
            | "\u{00ab}"
            | "\u{201c}"
            | "\u{2018}"
            | "\u{a1}"
            | "\u{bf}"
    )
}

/// Whether `token` ends a sentence. The empty string counts as a sentence end
/// so that the first sentence of a paragraph is always checked.
fn is_sentence_end_or_empty(token: &str) -> bool {
    token.is_empty() || is_sentence_end(token)
}

/// Whether `token` is a sentence-ending punctuation mark.
fn is_sentence_end(token: &str) -> bool {
    matches!(token, "." | "?" | "!" | "\u{2026}")
}

/// Whether the text of a sentence ends with an ellipsis, ignoring trailing
/// whitespace and quote characters.
fn ends_with_ellipsis(text: &str) -> bool {
    let trimmed = text
        .trim_end_matches(char::is_whitespace)
        .trim_end_matches(is_quote_char);
    trimmed.ends_with('\u{2026}') || trimmed.ends_with("...")
}

/// A character that may close a quote.
fn is_quote_char(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '\u{201e}'
            | '\u{00ab}'
            | '\u{00bb}'
            | '\u{2018}'
            | '\u{2019}'
            | '\u{201c}'
            | '\u{201d}'
    )
}

/// Uppercases the first letter or digit of `token`, leaving leading punctuation
/// in place. Mirrors the corresponding string helper of the rule set.
fn uppercase_first_char(token: &str) -> String {
    let mut chars = token.char_indices();
    match chars.next() {
        None => String::new(),
        Some((_, first)) => {
            if first.is_alphanumeric() {
                let mut out = String::new();
                out.extend(first.to_uppercase());
                out.push_str(&token[first.len_utf8()..]);
                out
            } else {
                match token.char_indices().find(|(_, c)| c.is_alphanumeric()) {
                    Some((i, c)) => {
                        let mut out = token[..i].to_owned();
                        out.extend(c.to_uppercase());
                        out.push_str(&token[i + c.len_utf8()..]);
                        out
                    }
                    // Only punctuation: uppercasing does not change anything.
                    None => token.to_owned(),
                }
            }
        }
    }
}

/// Whether `token` starts with a lowercase letter followed by an uppercase one,
/// e.g. `"iPhone"`.
fn only_lowercase_start(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(a), Some(b)) if a.is_ascii_lowercase() && b.is_ascii_uppercase()
    )
}

/// Whether `token` is camelCase, e.g. `"eBay"` or `"fooBar"`.
fn is_camel_case(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 3 {
        return false;
    }
    let mut i = 0;
    while i < chars.len() && chars[i].is_ascii_lowercase() {
        i += 1;
    }
    if i == 0 || i + 1 >= chars.len() || !chars[i].is_ascii_uppercase() {
        return false;
    }
    chars[i + 1..].iter().all(|c| c.is_ascii_alphabetic())
}

/// Whether `token` is a lowercase list marker: a single letter or a lowercase
/// roman numeral.
fn is_numeral(token: &str) -> bool {
    let mut chars = token.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_lowercase())
        || is_roman_numeral(token)
}

/// Whether `token` is a lowercase roman numeral such as `"iv"`.
fn is_roman_numeral(token: &str) -> bool {
    if token.is_empty() || !token.chars().all(|c| "mdclxvi".contains(c)) {
        return false;
    }

    let chars: Vec<char> = token.chars().collect();
    let mut i = 0;

    let mut count = 0;
    while i < chars.len() && chars[i] == 'm' && count < 4 {
        i += 1;
        count += 1;
    }

    if i + 1 < chars.len() && chars[i] == 'c' && matches!(chars[i + 1], 'm' | 'd') {
        i += 2;
    } else {
        if i < chars.len() && chars[i] == 'd' {
            i += 1;
        }
        let mut count = 0;
        while i < chars.len() && chars[i] == 'c' && count < 3 {
            i += 1;
            count += 1;
        }
    }

    if i + 1 < chars.len() && chars[i] == 'x' && matches!(chars[i + 1], 'c' | 'l') {
        i += 2;
    } else {
        if i < chars.len() && chars[i] == 'l' {
            i += 1;
        }
        let mut count = 0;
        while i < chars.len() && chars[i] == 'x' && count < 3 {
            i += 1;
            count += 1;
        }
    }

    if i + 1 < chars.len() && chars[i] == 'i' && matches!(chars[i + 1], 'x' | 'v') {
        i += 2;
    } else {
        if i < chars.len() && chars[i] == 'v' {
            i += 1;
        }
        let mut count = 0;
        while i < chars.len() && chars[i] == 'i' && count < 3 {
            i += 1;
            count += 1;
        }
    }

    i == chars.len()
}

/// Whether the sentence is a numbered list marker like `"1. "`.
fn digit_dot(text: &str) -> bool {
    let digits = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    if digits == 0 {
        return false;
    }
    let rest = &text[digits..];
    rest.starts_with(". ") && !rest.contains('\n')
}

/// Whether the sentence contains a line break followed by a numbered list
/// marker like `"\n1. "`.
fn linebreak_digit_dot(text: &str) -> bool {
    let mut parts = text.split('\n');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(second), None) => {
            let digits = second
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(second.len());
            digits > 0 && second[digits..].starts_with(". ")
        }
        _ => false,
    }
}

/// Whether `token` is a URL.
fn is_url(token: &str) -> bool {
    const PROTOCOLS: [&str; 18] = [
        "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
        "ssh", "data", "magnet", "smb", "slack", "spotify",
    ];

    if token.starts_with("www.") {
        return true;
    }
    for protocol in PROTOCOLS {
        if let Some(rest) = token.strip_prefix(protocol) {
            if rest.starts_with("://") {
                return true;
            }
        }
    }

    // A domain followed by a path, e.g. "example.com/foo".
    match token.find('/') {
        Some(slash) => {
            let labels: Vec<&str> = token[..slash].split('.').collect();
            labels.len() >= 2 && labels.iter().all(|label| is_domain_label(label))
        }
        None => false,
    }
}

/// Whether `label` is a domain label of a host name.
fn is_domain_label(label: &str) -> bool {
    label.len() >= 2
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && label
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Whether `token` is an e-mail address.
fn is_email(token: &str) -> bool {
    let token = token.strip_prefix('@').unwrap_or(token);
    let at = match token.find('@') {
        Some(at) => at,
        None => return false,
    };
    let (local, domain) = (&token[..at], &token[at + 1..]);
    if local.is_empty()
        || !local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~-.".contains(c))
    {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    labels.iter().all(|label| {
        !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }) && labels
        .last()
        .is_some_and(|label| label.len() >= 2 && label.chars().all(|c| c.is_ascii_alphabetic()))
}
