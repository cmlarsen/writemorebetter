//! Flags compounds written as separate words, e.g. `part time` for `part-time`.
//!
//! The behaviour follows the LanguageTool `EN_COMPOUNDS` rule. Runs of up to
//! five tokens, starting at each token, are normalized (hyphens become spaces,
//! a sentence-initial capital is lowered) and looked up in the compounds list;
//! the longest run found is reported with the hyphenated and/or joined spelling
//! the list allows for it. Suggestions the speller rejects are dropped, and a
//! match without suggestions is not reported. Tokens covered by one of the
//! rule's antipatterns are skipped entirely. Each match reports the
//! sub-rule id LanguageTool gives it, e.g. `EN_COMPOUNDS_PART_TIME`.
//!
//! The compounds list and the antipatterns are LanguageTool data (LGPL-2.1),
//! read from the build directory's `builtin/compounds.txt` and
//! `builtin/compounds_antipatterns.txt` and serialized into the rules binary;
//! only the matching logic lives in this source file.

use std::collections::VecDeque;

use crate::{
    spell::Speller,
    types::{DefaultHashSet, Position, Sentence, Span, Suggestion},
    utils::regex::Regex,
};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_COMPOUNDS";

/// The maximum number of words of a compound.
const MAX_TERMS: usize = 5;

const MESSAGE_WITH_HYPHEN: &str = "This word is normally spelled with a hyphen.";
const MESSAGE_WITHOUT_HYPHEN: &str = "This word is normally spelled as one.";
const MESSAGE_WITH_OR_WITHOUT_HYPHEN: &str =
    "This expression is normally spelled as one or with a hyphen.";

/// One token of an antipattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum PatternToken {
    /// The token text, compared case-insensitively (stored lowercase).
    Text(String),
    /// A case-insensitive regular expression the whole token text must match.
    TextRegex(Regex),
    /// A regular expression the whole part-of-speech tag of one reading must match.
    PosRegex(Regex),
}

impl PatternToken {
    fn matches(&self, token: &CompoundToken) -> bool {
        match self {
            PatternToken::Text(text) => token.text.to_lowercase() == *text,
            PatternToken::TextRegex(regex) => regex.is_match(token.text),
            PatternToken::PosRegex(regex) => token.tags.iter().any(|tag| regex.is_match(tag)),
        }
    }
}

/// The compounds list, split by the spellings it allows, in LanguageTool's
/// `CompoundRuleData` form: every entry is a listed compound with spaces in
/// place of hyphens, looked up case-sensitively.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CompoundData {
    /// Every listed compound, written as separate words.
    #[serde(serialize_with = "crate::types::sorted::set")]
    incorrect: DefaultHashSet<String>,
    /// Compounds that may be suggested joined into one word.
    #[serde(serialize_with = "crate::types::sorted::set")]
    joined: DefaultHashSet<String>,
    /// Compounds whose joined suggestion lowercases the later words.
    joined_lowercase: Vec<String>,
    /// Compounds that may be suggested with hyphens.
    #[serde(serialize_with = "crate::types::sorted::set")]
    dash: DefaultHashSet<String>,
}

impl CompoundData {
    /// Reads the lines of the compounds list. A trailing `+` allows only the
    /// joined spelling, `*` only the hyphenated one, `?` the joined one with
    /// lowercased later words, `$` all three, and no suffix both the joined and
    /// the hyphenated spelling.
    pub(crate) fn new(lines: Vec<String>) -> Self {
        let mut data = CompoundData::default();
        for line in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .replace('-', " ");
            let (compound, joined, dash, lowercase) = if let Some(x) = line.strip_suffix('+') {
                (x, true, false, false)
            } else if let Some(x) = line.strip_suffix('*') {
                (x, false, true, false)
            } else if let Some(x) = line.strip_suffix('?') {
                (x, true, false, true)
            } else if let Some(x) = line.strip_suffix('$') {
                (x, true, true, true)
            } else {
                (line.as_str(), true, true, false)
            };
            if joined {
                data.joined.insert(compound.to_owned());
            }
            if dash {
                data.dash.insert(compound.to_owned());
            }
            if lowercase {
                data.joined_lowercase.push(compound.to_owned());
            }
            data.incorrect.insert(compound.to_owned());
        }
        data
    }
}

/// Flags a compound written as separate words, e.g. `"part time"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Compounds {
    data: CompoundData,
    antipatterns: Vec<Vec<PatternToken>>,
}

/// A token as the rule sees it: the sentence start, a real token or padding
/// after the sentence end.
struct CompoundToken<'a> {
    text: &'a str,
    tags: Vec<&'a str>,
    start: Position,
    end: Position,
    space_before: bool,
    sentence_start: bool,
}

impl Compounds {
    pub(crate) fn new(data: CompoundData, antipatterns: Vec<Vec<PatternToken>>) -> Self {
        Compounds { data, antipatterns }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks a sentence and returns the resulting suggestions. Suggestions
    /// `speller` rejects are dropped; without a speller all are kept.
    pub fn check(&self, sentence: &Sentence, speller: Option<&Speller>) -> Vec<Suggestion> {
        let start = sentence.span().start();
        let mut tokens = vec![CompoundToken {
            text: "",
            tags: vec!["SENT_START"],
            start,
            end: start,
            space_before: false,
            sentence_start: true,
        }];
        for token in sentence.tokens() {
            tokens.push(CompoundToken {
                text: token.word().text().as_str(),
                tags: token
                    .word()
                    .tags()
                    .iter()
                    .map(|x| x.pos().as_str())
                    .collect(),
                start: token.span().start(),
                end: token.span().end(),
                space_before: token.has_space_before(),
                sentence_start: false,
            });
        }
        let immunized = self.immunized(&tokens);

        let mut suggestions = Vec::new();
        let mut previous_start: Option<Position> = None;
        let mut queue: VecDeque<CompoundToken> = VecDeque::with_capacity(MAX_TERMS);
        let mut contains_digits = false;

        let padding = std::iter::repeat_with(|| None).take(MAX_TERMS);
        for (i, token) in tokens.into_iter().map(Some).chain(padding).enumerate() {
            let token = match token {
                Some(token) => token,
                None => {
                    let position = queue.front().map_or(start, |x| x.start);
                    CompoundToken {
                        text: "",
                        tags: Vec::new(),
                        start: position,
                        end: position,
                        space_before: false,
                        sentence_start: false,
                    }
                }
            };
            if i > 0 && immunized.get(i) == Some(&true) {
                continue;
            }
            if i > 0 {
                if let Some(suggestion) = self.check_queue(
                    &queue,
                    sentence,
                    speller,
                    &mut contains_digits,
                    &mut previous_start,
                ) {
                    suggestions.push(suggestion);
                }
            }
            if queue.len() == MAX_TERMS {
                queue.pop_front();
            }
            queue.push_back(token);
        }

        suggestions
    }

    /// Marks the tokens covered by a match of any antipattern.
    fn immunized(&self, tokens: &[CompoundToken]) -> Vec<bool> {
        let mut immunized = vec![false; tokens.len()];
        for antipattern in &self.antipatterns {
            if antipattern.len() > tokens.len() {
                continue;
            }
            for start in 0..=tokens.len() - antipattern.len() {
                if antipattern
                    .iter()
                    .zip(&tokens[start..])
                    .all(|(pattern, token)| pattern.matches(token))
                {
                    immunized[start..start + antipattern.len()].fill(true);
                }
            }
        }
        immunized
    }

    /// Looks up the runs that start at the head of `queue`, longest first, and
    /// returns the match for the first one that is a listed compound.
    fn check_queue(
        &self,
        queue: &VecDeque<CompoundToken>,
        sentence: &Sentence,
        speller: Option<&Speller>,
        contains_digits: &mut bool,
        previous_start: &mut Option<Position>,
    ) -> Option<Suggestion> {
        let head = queue.front()?;
        let mut runs: Vec<(String, String, &CompoundToken)> = Vec::new();
        let mut text = String::new();
        for (j, token) in queue.iter().enumerate() {
            if token.space_before {
                text.push(' ');
            }
            text.push_str(token.text);
            if j >= 1 || !head.sentence_start {
                let mut normalized = normalize(&text);
                if head.sentence_start {
                    normalized = lowercase_first_char(&normalized);
                }
                let end = runs
                    .iter()
                    .find(|(x, _, _)| *x == normalized)
                    .map_or(token, |(_, _, end)| *end);
                runs.push((normalized, java_trim(&text).to_owned(), end));
            }
        }

        let data = &self.data;
        for (normalized, original, end) in runs.iter().rev() {
            if normalized.split(' ').any(is_numeric) {
                *contains_digits = true;
            }
            let digits_key = (!data.incorrect.contains(normalized) && *contains_digits)
                .then(|| replace_digits(normalized));
            let digits_match = digits_key
                .as_ref()
                .is_some_and(|key| data.incorrect.contains(key));
            if !data.incorrect.contains(normalized) && !digits_match {
                continue;
            }

            if data.dash.contains(normalized) && !original.contains(' ') {
                return None;
            }
            let mut replacements = Vec::new();
            let mut message = MESSAGE_WITH_OR_WITHOUT_HYPHEN;
            if data.dash.contains(normalized) || digits_match {
                replacements.push(original.replace(' ', "-"));
                message = MESSAGE_WITH_HYPHEN;
            }
            if is_not_all_uppercase(original) && data.joined.contains(normalized) {
                let lowercase = data
                    .joined_lowercase
                    .iter()
                    .any(|x| normalized.contains(x.as_str()));
                replacements.push(merge_compound(original, lowercase));
                message = MESSAGE_WITHOUT_HYPHEN;
            }
            if normalized.split(' ').next().map(|x| x.chars().count()) == Some(1) {
                replacements = vec![original.replace(' ', "-")];
                message = MESSAGE_WITH_HYPHEN;
            } else if replacements.is_empty() || replacements.len() == 2 {
                message = MESSAGE_WITH_OR_WITHOUT_HYPHEN;
            }

            let base = sentence.span().start().byte;
            let covered = sentence
                .text()
                .get(head.start.byte - base..end.end.byte - base)
                .unwrap_or("");
            let replacements: Vec<String> = replacements
                .into_iter()
                .map(|x| collapse_hyphens(&x))
                .filter(|x| x != covered && speller.is_none_or(|s| is_correct_spell(s, x)))
                .collect();
            if replacements.is_empty() {
                return None;
            }

            if *previous_start == Some(head.start) {
                return None;
            }
            *previous_start = Some(head.start);
            return Some(Suggestion::new(
                format!("BUILTIN/{}", sub_rule_id(normalized)),
                message.to_owned(),
                Span::from_positions(head.start, end.end),
                replacements,
            ));
        }

        None
    }
}

/// `AbstractCompoundRule.normalize`: trims, turns ` - ` and `-` into spaces
/// and collapses whitespace.
fn normalize(text: &str) -> String {
    let text = java_trim(text).replace(" - ", " ").replace('-', " ");
    let mut result = String::with_capacity(text.len());
    let mut in_space = false;
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r') {
            if !in_space {
                result.push(' ');
            }
            in_space = true;
        } else {
            result.push(c);
            in_space = false;
        }
    }
    result
}

/// Java's `String.trim`: strips characters up to U+0020 from both ends.
fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

/// `StringUtils.uncapitalize`.
fn lowercase_first_char(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `StringUtils.isNumeric`: a non-empty string of digits.
fn is_numeric(text: &str) -> bool {
    !text.is_empty() && text.chars().all(char::is_numeric)
}

/// Replaces every run of ASCII digits with `\d+`, the placeholder the
/// compounds list uses for numbers.
fn replace_digits(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_digits = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                result.push_str("\\d+");
            }
            in_digits = true;
        } else {
            result.push(c);
            in_digits = false;
        }
    }
    result
}

/// `AbstractCompoundRule.isNotAllUppercase`: no word (other than `-`) is all
/// uppercase.
fn is_not_all_uppercase(text: &str) -> bool {
    text.split(' ')
        .filter(|part| *part != "-")
        .all(|part| part.chars().any(char::is_lowercase))
}

/// `AbstractCompoundRule.mergeCompound`: joins the words, optionally
/// lowercasing the first character of every word after the first.
fn merge_compound(text: &str, lowercase_later_words: bool) -> String {
    let text = text.replace('-', " ");
    let mut result = String::with_capacity(text.len());
    for (i, part) in text.split(' ').enumerate() {
        if i > 0 && lowercase_later_words {
            result.push_str(&lowercase_first_char(part));
        } else {
            result.push_str(part);
        }
    }
    result
}

/// Replaces every run of two or more hyphens with one.
fn collapse_hyphens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if c != '-' || !result.ends_with('-') {
            result.push(c);
        }
    }
    result
}

/// Whether the speller accepts `word`, as LanguageTool's English speller
/// does: the whole word, or else each of its hyphen-separated parts. Like
/// Morfologik with `ignore-numbers`, it accepts any word with a digit.
fn is_correct_spell(speller: &Speller, word: &str) -> bool {
    let is_correct = |word: &str| word.chars().any(char::is_numeric) || speller.is_correct(word);
    is_correct(word) || (word.contains('-') && word.split('-').all(is_correct))
}

/// `StringTools.toId` of `EN_COMPOUNDS_<compound>`: uppercase, with `'` as
/// `_Q_` and every other character outside `A-Z` and `À-Þ` as `_`.
fn sub_rule_id(compound: &str) -> String {
    format!("{}_{}", ID, compound)
        .to_uppercase()
        .replace('\'', "_Q_")
        .chars()
        .map(|c| match c {
            'A'..='Z' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{de}' => c,
            _ => '_',
        })
        .collect()
}
