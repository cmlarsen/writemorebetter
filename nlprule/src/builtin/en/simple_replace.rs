//! Table-driven "wrong word / wrong phrase" replacement rules.
//!
//! A family of English rules only carries a table of wrong forms and their
//! replacements; the matching logic is shared. Two shapes exist upstream and
//! both are reproduced here:
//!
//! * a single-token variant that is case-sensitive and flags one token at a
//!   time (used for missing apostrophes in contractions), and
//! * a phrase variant that is case-insensitive and can match several
//!   whitespace-separated words.
//!
//! The tables themselves are LanguageTool data (LGPL-2.1) and are loaded from
//! `build/builtin-data/en/simple_replace_*.txt` at compile time; only the
//! matching logic lives in this MIT/Apache-2.0 source file.

use crate::types::{DefaultHashMap, Sentence, Span, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The maximum number of whitespace-separated words a phrase table entry may
/// span.
const MAX_TOKENS_IN_MULTIWORD: usize = 20;

/// How a table's wrong forms are matched against the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMode {
    /// One token at a time; the entry is looked up in a case-sensitive map.
    Single,
    /// Runs of whitespace-separated tokens; entries are matched
    /// case-insensitively as whole words or phrases.
    Phrase,
}

/// One line of a replacement table: the wrong form(s) and the suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Wrong forms; any of these triggers the suggestion.
    wrong: Vec<String>,
    /// Suggested replacements for the wrong forms.
    replacements: Vec<String>,
    /// Optional per-entry message that replaces the rule's message verbatim.
    message: Option<String>,
}

/// A table-driven replacement rule, registered once per data table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleReplace {
    /// The identifier of the LanguageTool rule this table belongs to.
    id: String,
    /// Message shown for a match; `$match` and `$suggestions` are substituted.
    message: String,
    /// Separator used before the last suggestion in `$suggestions`.
    suggestions_separator: String,
    /// Whether the table is matched token-by-token or as phrases.
    mode: MatchMode,
    /// Whether lookup keys are matched case-sensitively.
    case_sensitive: bool,
    /// Whether matches carry the table's replacements. A table without
    /// suggestions (e.g. profanity) only lists wrong forms, one per line.
    has_suggestions: bool,
    /// The parsed table lines.
    entries: Vec<Entry>,
    /// Single-token lookup (`MatchMode::Single`), keyed by the wrong form.
    #[serde(serialize_with = "crate::types::sorted::map")]
    single: DefaultHashMap<String, usize>,
    /// Phrase lookup for wrong forms without a space (lowercased key).
    #[serde(serialize_with = "crate::types::sorted::map")]
    no_space: DefaultHashMap<String, usize>,
    /// Phrase lookup for wrong forms that contain a space (lowercased key).
    #[serde(serialize_with = "crate::types::sorted::map")]
    space: DefaultHashMap<String, usize>,
    /// For phrases, the first word -> the longest phrase with that first word.
    #[serde(serialize_with = "crate::types::sorted::map")]
    start_space: DefaultHashMap<String, usize>,
    /// For single words, the first character -> the longest key with it.
    #[serde(serialize_with = "crate::types::sorted::map")]
    start_no_space: DefaultHashMap<char, usize>,
}

impl SimpleReplace {
    /// Builds a rule from the raw lines of a replacement table.
    pub fn new(
        id: &str,
        mode: MatchMode,
        case_sensitive: bool,
        message: &str,
        suggestions_separator: &str,
        lines: Vec<String>,
    ) -> Self {
        let entries = match mode {
            MatchMode::Single => parse_single_mode(&lines, case_sensitive),
            MatchMode::Phrase => parse_phrase_mode(&lines, true),
        };

        let mut rule = SimpleReplace {
            id: id.to_owned(),
            message: message.to_owned(),
            suggestions_separator: suggestions_separator.to_owned(),
            mode,
            case_sensitive,
            has_suggestions: true,
            entries,
            single: DefaultHashMap::default(),
            no_space: DefaultHashMap::default(),
            space: DefaultHashMap::default(),
            start_space: DefaultHashMap::default(),
            start_no_space: DefaultHashMap::default(),
        };
        rule.build_index();
        rule
    }

    /// Builds a phrase rule from a table that lists wrong forms only. Its
    /// matches carry no replacements; any `=replacement` part is ignored.
    pub fn without_suggestions(id: &str, message: &str, lines: Vec<String>) -> Self {
        let mut rule = SimpleReplace {
            id: id.to_owned(),
            message: message.to_owned(),
            suggestions_separator: ", ".to_owned(),
            mode: MatchMode::Phrase,
            case_sensitive: false,
            has_suggestions: false,
            entries: parse_phrase_mode(&lines, false),
            single: DefaultHashMap::default(),
            no_space: DefaultHashMap::default(),
            space: DefaultHashMap::default(),
            start_space: DefaultHashMap::default(),
            start_no_space: DefaultHashMap::default(),
        };
        rule.build_index();
        rule
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Fills the lookup maps (and their length hints) from the parsed entries.
    fn build_index(&mut self) {
        for (index, entry) in self.entries.iter().enumerate() {
            for wrong in &entry.wrong {
                let key = if self.case_sensitive {
                    wrong.clone()
                } else {
                    wrong.to_lowercase()
                };

                match self.mode {
                    MatchMode::Single => {
                        self.single.insert(key, index);
                    }
                    MatchMode::Phrase => {
                        if key.contains(' ') {
                            let words = key.split(' ').count();
                            if let Some(first) = key.split(' ').next() {
                                let longest = self.start_space.entry(first.to_owned()).or_insert(0);
                                *longest = (*longest).max(words);
                            }
                            self.space.insert(key, index);
                        } else {
                            if let Some(first) = key.chars().next() {
                                let len = key.chars().count();
                                let longest = self.start_no_space.entry(first).or_insert(0);
                                *longest = (*longest).max(len);
                            }
                            self.no_space.insert(key, index);
                        }
                    }
                }
            }
        }
    }

    /// Checks one sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        match self.mode {
            MatchMode::Single => self.check_single(sentence),
            MatchMode::Phrase => self.check_phrase(sentence),
        }
    }

    /// Single-token matching: look the token up and flag it if it is a wrong form.
    fn check_single(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();

        for token in sentence.tokens() {
            if token.ignore_spelling() {
                continue;
            }

            let original = token.word().text().as_str();
            let lookup = if self.case_sensitive {
                original.to_owned()
            } else {
                original.to_lowercase()
            };

            let Some(&index) = self.single.get(&lookup) else {
                continue;
            };

            let mut replacements = self.entries[index].replacements.clone();
            if is_all_uppercase(original) {
                replacements = replacements.iter().map(|x| x.to_uppercase()).collect();
            }
            replacements.retain(|replacement| replacement != original);
            if replacements.is_empty() {
                continue;
            }

            let message = self.message(original, &replacements);
            suggestions.push(Suggestion::new(
                format!("BUILTIN/{}", self.id),
                message,
                token.span().clone(),
                replacements,
            ));
        }

        suggestions
    }

    /// Phrase matching: for every token, try the longest phrase that starts there.
    fn check_phrase(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let tokens = sentence.tokens();

        let mut sent_start = 0;
        while sent_start < tokens.len() && is_punctuation_start(token_text(&tokens[sent_start])) {
            sent_start += 1;
        }

        let mut suggestions: Vec<Suggestion> = Vec::new();

        for start in sent_start..tokens.len() {
            let first_word = no_space_run(tokens, start).to_lowercase();

            if let Some(&max_words) = self.start_space.get(&first_word) {
                let mut key = String::new();
                let mut spaces = 0;
                let mut end = start;
                while end < tokens.len() && end - start < MAX_TOKENS_IN_MULTIWORD {
                    if end > start && tokens[end].has_space_before() {
                        key.push(' ');
                        spaces += 1;
                    }
                    key.push_str(token_text(&tokens[end]));
                    if spaces + 1 > max_words {
                        break;
                    }
                    if spaces > 0 {
                        if let Some(&index) = self.space.get(&key.to_lowercase()) {
                            self.create_match(
                                &mut suggestions,
                                index,
                                start,
                                end,
                                &key,
                                tokens,
                                sent_start,
                            );
                        }
                    }
                    end += 1;
                }
            }

            if let Some(first) = first_word.chars().next() {
                if self.start_no_space.contains_key(&first) {
                    let mut key = String::new();
                    let mut end = start;
                    while end < tokens.len() && end - start < MAX_TOKENS_IN_MULTIWORD {
                        if end > start && tokens[end].has_space_before() {
                            break;
                        }
                        key.push_str(token_text(&tokens[end]));
                        if let Some(&index) = self.no_space.get(&key.to_lowercase()) {
                            self.create_match(
                                &mut suggestions,
                                index,
                                start,
                                end,
                                &key,
                                tokens,
                                sent_start,
                            );
                        }
                        end += 1;
                    }
                }
            }
        }

        suggestions
    }

    /// Adds a match for `entries[index]` covering `tokens[start..=end]`, keeping
    /// only the longest match as the corresponding upstream rule does.
    #[allow(clippy::too_many_arguments)]
    fn create_match(
        &self,
        suggestions: &mut Vec<Suggestion>,
        index: usize,
        start: usize,
        end: usize,
        original: &str,
        tokens: &[Token],
        sent_start: usize,
    ) {
        let from = tokens[start].span().start();
        let to = tokens[end].span().end();

        // The previous match already covers this one entirely.
        if let Some(last) = suggestions.last() {
            if last.span().start() <= from && last.span().end() >= to {
                return;
            }
        }

        let entry = &self.entries[index];
        let first_word_is_camel_case = entry
            .replacements
            .iter()
            .any(|replacement| is_camel_case(replacement.split(' ').next().unwrap_or("")));
        let all_uppercase = is_all_uppercase(original);
        let capitalized = is_capitalized_word(original.split(' ').next().unwrap_or(""));

        let mut replacements = Vec::new();
        for replacement in &entry.replacements {
            let mut adjusted = replacement.clone();
            if !first_word_is_camel_case && (start == sent_start || capitalized) {
                adjusted = uppercase_first_char(replacement);
            }
            if all_uppercase {
                adjusted = replacement.to_uppercase();
            }
            if replacement != original && adjusted != original && !replacements.contains(&adjusted)
            {
                replacements.push(adjusted.clone());
            }
            if adjusted == original {
                // The original is correct, so there is nothing to fix here.
                replacements.clear();
                break;
            }
        }

        if self.has_suggestions && replacements.is_empty() {
            return;
        }
        if !self.has_suggestions {
            replacements.clear();
        }

        let message = match &entry.message {
            Some(message) => message.clone(),
            None => self.message(original, &entry.replacements),
        };

        // The new match covers the previous one entirely; drop the previous one.
        if let Some(last) = suggestions.last() {
            if last.span().start() >= from && last.span().end() <= to {
                suggestions.pop();
            }
        }

        suggestions.push(Suggestion::new(
            format!("BUILTIN/{}", self.id),
            message,
            Span::from_positions(from, to),
            replacements,
        ));
    }

    /// Renders the rule message, substituting `$match` and `$suggestions`.
    fn message(&self, original: &str, replacements: &[String]) -> String {
        let mut rendered = String::new();
        for (i, replacement) in replacements.iter().enumerate() {
            if i > 0 {
                if i == replacements.len() - 1 {
                    rendered.push_str(&self.suggestions_separator);
                } else {
                    rendered.push_str(", ");
                }
            }
            rendered.push_str(replacement);
        }

        self.message
            .replacen("$match", &java_replacement(original), 1)
            .replacen("$suggestions", &java_replacement(&rendered), 1)
    }
}

/// Parses the single-token table format: `wrong=replacement`, with `|` for
/// alternative wrong forms and replacements. Comments start with `#`.
fn parse_single_mode(lines: &[String], case_sensitive: bool) -> Vec<Entry> {
    let mut entries = Vec::new();

    for line in lines {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if right.trim().is_empty() {
            continue;
        }
        entries.push(Entry {
            wrong: split_forms(left, case_sensitive),
            replacements: right.split('|').map(str::to_owned).collect(),
            message: None,
        });
    }

    entries
}

/// Parses the phrase table format: `wrong=replacement`, with `|` for alternatives
/// and an optional tab-separated message. `#` starts a comment; a comment can
/// follow an entry. Without `has_suggestions` a line needs no `=replacement`.
fn parse_phrase_mode(lines: &[String], has_suggestions: bool) -> Vec<Entry> {
    let mut entries = Vec::new();

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let (pair, message) = match line.split_once('\t') {
            Some((pair, message)) => (pair, Some(message.to_owned())),
            None => (line, None),
        };
        let (left, right) = match pair.split_once('=') {
            Some((left, right)) => (left, right),
            None if !has_suggestions => (pair, ""),
            None => continue,
        };

        entries.push(Entry {
            wrong: left.split('|').map(str::to_owned).collect(),
            replacements: right.split('|').map(str::to_owned).collect(),
            message,
        });
    }

    entries
}

/// Splits a wrong-form field on `|`; case-insensitive tables are lowercased.
fn split_forms(left: &str, case_sensitive: bool) -> Vec<String> {
    left.split('|')
        .map(|form| {
            if case_sensitive {
                form.to_owned()
            } else {
                form.to_lowercase()
            }
        })
        .collect()
}

/// The text of a token.
fn token_text<'a>(token: &'a Token<'_>) -> &'a str {
    token.word().text().as_str()
}

/// The concatenation of the tokens from `start` that are not preceded by
/// whitespace, i.e. the first "word" as the upstream rule builds it.
fn no_space_run(tokens: &[Token], start: usize) -> String {
    let mut text = token_text(&tokens[start]).to_owned();
    let mut i = start + 1;
    while i < tokens.len() && !tokens[i].has_space_before() {
        text.push_str(token_text(&tokens[i]));
        i += 1;
    }
    text
}

/// Whether the token is a leading punctuation mark or number that should be
/// skipped when looking for the start of a sentence.
fn is_punctuation_start(text: &str) -> bool {
    if text.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_alphabetic())
}

/// Whether `text` contains no lowercase letters.
fn is_all_uppercase(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphabetic() && c.is_lowercase())
}

/// Whether `text` is capitalized, i.e. it starts uppercase and has no other
/// uppercase letters.
fn is_capitalized_word(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => {}
        _ => return false,
    }
    !chars.any(|c| c.is_alphabetic() && !c.is_lowercase())
}

/// Whether `text` looks like `lowerUpper` camel case, e.g. `iPhone`.
fn is_camel_case(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i].is_ascii_lowercase() {
        i += 1;
    }
    if i == 0 || i + 1 >= chars.len() {
        return false;
    }
    if !chars[i].is_ascii_uppercase() {
        return false;
    }
    chars[i + 1..].iter().all(|c| c.is_ascii_alphabetic())
}

/// `StringTools.uppercaseFirstChar`: uppercases the first letter or digit,
/// keeping any characters before it, or else the last character.
fn uppercase_first_char(text: &str) -> String {
    let Some((pos, c)) = text
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .or_else(|| text.char_indices().last())
    else {
        return String::new();
    };
    format!(
        "{}{}{}",
        &text[..pos],
        c.to_uppercase(),
        &text[pos + c.len_utf8()..]
    )
}

/// The literal text Java's `String.replaceFirst` inserts for `replacement`: a
/// backslash makes the next character literal and is dropped itself.
fn java_replacement(replacement: &str) -> String {
    let mut result = String::with_capacity(replacement.len());
    let mut chars = replacement.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => result.extend(chars.next()),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercase_first_char_skips_leading_non_alphanumerics() {
        assert_eq!(uppercase_first_char("(either one)"), "(Either one)");
        assert_eq!(
            uppercase_first_char("\u{201c}well\u{201d}"),
            "\u{201c}Well\u{201d}"
        );
        assert_eq!(uppercase_first_char("\u{2026}"), "\u{2026}");
        assert_eq!(uppercase_first_char(""), "");
    }

    #[test]
    fn java_replacement_drops_escaping_backslashes() {
        assert_eq!(java_replacement(r"a\b"), "ab");
        assert_eq!(java_replacement(r"a\\b"), r"a\b");
        assert_eq!(
            java_replacement("she\u{2019}d\u{2014}no"),
            "she\u{2019}d\u{2014}no"
        );
    }
}
