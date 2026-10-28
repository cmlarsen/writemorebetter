use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::{Deserialize, Serialize};

use super::sentence_starts::penn_to_universal;
use super::Split;
use crate::document::leading_elision;
use crate::{Document, Mode, Paragraph, Span, StyleError, Token};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyOptions {
    #[serde(alias = "minimum_minority")]
    pub minimum_minority: usize,
    /// The house form of each punctuation check, as a style preset sets it. A check with a
    /// preferred form counts every occurrence of the other forms in `nonPreferred`.
    pub preferred: ConsistencyPreferred,
}
impl Default for ConsistencyOptions {
    fn default() -> Self {
        Self {
            minimum_minority: 1,
            preferred: ConsistencyPreferred::default(),
        }
    }
}

/// A preferred form per punctuation check, named as the report names its forms. `None` leaves a
/// check without a preference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyPreferred {
    #[serde(alias = "double_quotes", skip_serializing_if = "Option::is_none")]
    pub double_quotes: Option<String>,
    #[serde(alias = "single_quotes", skip_serializing_if = "Option::is_none")]
    pub single_quotes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apostrophes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ellipses: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dashes: Option<String>,
    #[serde(alias = "clock_times", skip_serializing_if = "Option::is_none")]
    pub clock_times: Option<String>,
}

impl ConsistencyPreferred {
    /// Each check's name, preference and allowed forms, in report order.
    fn checks(&self) -> [(&'static str, Option<&str>, &'static [&'static str]); 6] {
        [
            ("doubleQuotes", self.double_quotes.as_deref(), &QUOTE_FORMS),
            ("singleQuotes", self.single_quotes.as_deref(), &QUOTE_FORMS),
            ("apostrophes", self.apostrophes.as_deref(), &QUOTE_FORMS),
            ("ellipses", self.ellipses.as_deref(), &ELLIPSIS_FORMS),
            ("dashes", self.dashes.as_deref(), &DASH_FORMS),
            ("clockTimes", self.clock_times.as_deref(), &CLOCK_FORMS),
        ]
    }

    pub(crate) fn validate(&self) -> Result<(), StyleError> {
        for (check, preferred, forms) in self.checks() {
            if let Some(form) = preferred.filter(|form| !forms.contains(form)) {
                return Err(StyleError::options(format!(
                    "preferred.{check} must be one of {}, got {form:?}",
                    forms.join(", ")
                )));
            }
        }
        Ok(())
    }
}
impl ConsistencyOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_minority(mut self, value: usize) -> Self {
        self.minimum_minority = value;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyVariant {
    pub form: String,
    pub counts: Split<usize>,
    pub spans: Vec<Span>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyCheck {
    pub mixed: bool,
    pub variants: Vec<ConsistencyVariant>,
    /// The preferred form from the options, if one was set.
    pub preferred: Option<String>,
    /// Occurrences of every form other than `preferred`; 0 without a preference.
    pub non_preferred: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyCount {
    pub form: String,
    pub counts: Split<usize>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyWord {
    pub word: String,
    pub variants: Vec<ConsistencyVariant>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsistencyReport {
    pub double_quotes: ConsistencyCheck,
    pub single_quotes: ConsistencyCheck,
    pub apostrophes: ConsistencyCheck,
    pub ellipses: ConsistencyCheck,
    pub dashes: ConsistencyCheck,
    /// Times on the hour with *a.m.* or *p.m.*: `6:00 a.m.` or `6 a.m.`
    pub clock_times: ConsistencyCheck,
    pub dash_characters: Vec<ConsistencyCount>,
    pub capitalization: Vec<ConsistencyWord>,
    pub hyphenation: Vec<ConsistencyWord>,
}

const QUOTE_FORMS: [&str; 2] = ["curly", "straight"];
const ELLIPSIS_FORMS: [&str; 3] = ["character", "periods", "spacedPeriods"];
const DASH_FORMS: [&str; 5] = [
    "emClosed",
    "emSpaced",
    "enSpaced",
    "doubleHyphen",
    "spacedHyphen",
];
const CLOCK_FORMS: [&str; 2] = ["withMinutes", "hourOnly"];
const DASH_CHARACTERS: [&str; 3] = ["hyphen", "enDash", "emDash"];

// Closed words that read as two ordinary words when open (`every one`, `may be`, `any way`) are
// different words, not hyphenation variants; an open pair needs a hyphenated form in the document
// when either part is one of these.
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "after", "all", "an", "and", "any", "as", "at", "be", "by", "can", "down",
    "each", "else", "even", "ever", "every", "for", "from", "her", "him", "his", "how", "in",
    "into", "it", "its", "may", "me", "my", "no", "not", "of", "off", "on", "one", "or", "our",
    "out", "over", "self", "so", "some", "that", "the", "their", "them", "there", "these", "they",
    "this", "those", "thy", "to", "under", "up", "us", "what", "when", "where", "who", "why",
    "will", "with", "within", "without", "you", "your",
];

/// Positions of a paragraph's characters, with one extra entry for its end.
struct Positions {
    chars: Vec<char>,
    at: Vec<(usize, usize, usize)>,
}

impl Positions {
    fn new(paragraph: &Paragraph) -> Self {
        let (mut byte, mut char, mut utf16) = (
            paragraph.span.byte.start,
            paragraph.span.char.start,
            paragraph.span.utf16.start,
        );
        let mut chars = Vec::new();
        let mut at = Vec::new();
        for ch in paragraph.text.chars() {
            chars.push(ch);
            at.push((byte, char, utf16));
            byte += ch.len_utf8();
            char += 1;
            utf16 += ch.len_utf16();
        }
        at.push((byte, char, utf16));
        Self { chars, at }
    }

    fn span(&self, range: Range<usize>) -> Span {
        let (start, end) = (self.at[range.start], self.at[range.end]);
        Span {
            byte: start.0..end.0,
            char: start.1..end.1,
            utf16: start.2..end.2,
        }
    }

    fn index(&self, byte: usize) -> usize {
        self.at.partition_point(|position| position.0 < byte)
    }

    fn get(&self, index: Option<usize>) -> Option<char> {
        index.and_then(|index| self.chars.get(index).copied())
    }
}

fn record(variant: &mut ConsistencyVariant, span: Span, mode: Mode) {
    for count in variant.counts.all_and_mode_mut(mode) {
        *count += 1;
    }
    variant.spans.push(span);
}

fn variants(forms: &[&str]) -> Vec<ConsistencyVariant> {
    forms
        .iter()
        .map(|form| ConsistencyVariant {
            form: (*form).to_owned(),
            ..Default::default()
        })
        .collect()
}

fn mixed(variants: &[ConsistencyVariant], minimum_minority: usize) -> bool {
    let mut counts: Vec<_> = variants.iter().map(|v| v.counts.all).collect();
    counts.sort_unstable_by(|a, b| b.cmp(a));
    counts
        .get(1)
        .is_some_and(|&minority| minority >= minimum_minority.max(1))
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Open,
    Closed,
    Boundary,
}

fn side(ch: Option<char>) -> Side {
    match ch {
        Some('\u{2028}' | '\u{2029}' | '\n' | '\r') | None => Side::Boundary,
        Some(ch) if ch.is_whitespace() => Side::Open,
        Some(ch) if ch.is_alphanumeric() => Side::Closed,
        _ => Side::Boundary,
    }
}

/// `Some(true)` for a spaced dash, `Some(false)` for a closed one, `None` when its neighbours
/// do not decide (quote marks or paragraph edges on both sides, or one space and one letter).
fn spaced(before: Side, after: Side) -> Option<bool> {
    let sides = [before, after];
    let open = sides.contains(&Side::Open);
    let closed = sides.contains(&Side::Closed);
    match (open, closed) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Whether a closing `’` (one not followed by a letter or digit) comes before the line ends or
/// another `‘` opens.
fn closes_on_line(rest: &[char]) -> bool {
    for (i, ch) in rest.iter().enumerate() {
        match ch {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' | '‘' => return false,
            '’' if !rest.get(i + 1).is_some_and(|next| next.is_alphanumeric()) => return true,
            _ => {}
        }
    }
    false
}

fn punctuation(
    document: &Document,
    positions: &Positions,
    report: &mut ConsistencyReport,
    dash_characters: &mut [Split<usize>; 3],
) {
    let chars = &positions.chars;
    let alphanumeric =
        |index: Option<usize>| positions.get(index).is_some_and(char::is_alphanumeric);
    let digit = |index: Option<usize>| positions.get(index).is_some_and(|ch| ch.is_ascii_digit());
    let mut open_single = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let before = index.checked_sub(1);
        let after = Some(index + 1);
        let mode = document.mode_at(positions.at[index].0);
        let mut end = index + 1;
        match ch {
            '“' | '”' => record(
                &mut report.double_quotes.variants[0],
                positions.span(index..end),
                mode,
            ),
            '"' if !digit(before) => record(
                &mut report.double_quotes.variants[1],
                positions.span(index..end),
                mode,
            ),
            '\'' | '‘' | '’' => {
                let form = usize::from(ch == '\'');
                let rest: String = chars[index + 1..].iter().take(12).collect();
                let apostrophe = match (alphanumeric(before), alphanumeric(after)) {
                    (true, true) => Some(true),
                    // An opening `‘` before an elision (`‘Tis’`) is a quote when a closing `’`
                    // follows on its line.
                    (false, true) if ch == '‘' && closes_on_line(&chars[index + 1..]) => {
                        open_single = true;
                        Some(false)
                    }
                    (false, true) if leading_elision(&rest) => Some(true),
                    (false, true) if ch == '\'' && digit(before) => None,
                    (false, true) => {
                        open_single = true;
                        Some(false)
                    }
                    // A foot mark (`5'`) inside a quotation leaves the quote open.
                    (true, false) if ch == '\'' && digit(before) => None,
                    (true, false) if open_single => {
                        open_single = false;
                        Some(false)
                    }
                    (true, false) => Some(true),
                    (false, false) => {
                        open_single = false;
                        Some(false)
                    }
                };
                let check = match apostrophe {
                    Some(true) => Some(&mut report.apostrophes),
                    Some(false) => Some(&mut report.single_quotes),
                    None => None,
                };
                if let Some(check) = check {
                    record(&mut check.variants[form], positions.span(index..end), mode);
                }
            }
            '…' => record(
                &mut report.ellipses.variants[0],
                positions.span(index..end),
                mode,
            ),
            '.' => {
                while chars.get(end) == Some(&'.') {
                    end += 1;
                }
                if end - index >= 3 {
                    record(
                        &mut report.ellipses.variants[1],
                        positions.span(index..end),
                        mode,
                    );
                } else if end - index == 1 {
                    let mut dots = 1;
                    while matches!(chars.get(end), Some(' ' | '\u{a0}' | '\u{202f}'))
                        && chars.get(end + 1) == Some(&'.')
                    {
                        dots += 1;
                        end += 2;
                    }
                    if dots >= 3 {
                        record(
                            &mut report.ellipses.variants[2],
                            positions.span(index..end),
                            mode,
                        );
                    } else {
                        end = index + 1;
                    }
                }
            }
            '—' | '–' => {
                while chars.get(end) == Some(&ch) {
                    end += 1;
                }
                let character = if ch == '—' { 2 } else { 1 };
                for count in dash_characters[character].all_and_mode_mut(mode) {
                    *count += end - index;
                }
                // Emphasis marks around a dash (`—_Genesis_`) are transparent.
                let emphasis = |index: &usize| matches!(chars.get(*index), Some('_' | '*'));
                // Indentation before a paragraph's first dash is the paragraph edge.
                let left = (0..index)
                    .rev()
                    .find(|i| !emphasis(i))
                    .filter(|_| !chars[..index].iter().all(|ch| ch.is_whitespace()));
                let right = (end..chars.len()).find(|i| !emphasis(i));
                // A dash after a closing quote and a space introduces an attribution (`.” —Pope`).
                let attribution = left.is_some_and(|left| {
                    side(positions.get(Some(left))) == Side::Open
                        && left
                            .checked_sub(1)
                            .is_some_and(|quote| matches!(chars[quote], '”' | '"'))
                });
                let spacing = spaced(side(positions.get(left)), side(positions.get(right)))
                    .filter(|_| !attribution);
                let form = match (ch, spacing) {
                    ('—', Some(false)) => Some(0),
                    ('—', Some(true)) => Some(1),
                    ('–', Some(true)) => Some(2),
                    _ => None,
                };
                if let Some(form) = form {
                    record(
                        &mut report.dashes.variants[form],
                        positions.span(index..end),
                        mode,
                    );
                }
            }
            '-' | '‐' => {
                while matches!(chars.get(end), Some('-' | '‐')) {
                    end += 1;
                }
                for count in dash_characters[0].all_and_mode_mut(mode) {
                    *count += end - index;
                }
                let form = if end - index >= 2 {
                    Some(3)
                } else {
                    let open = |ch: Option<char>| side(ch) == Side::Open;
                    (open(positions.get(before)) && open(positions.get(Some(end)))).then_some(4)
                };
                if let Some(form) = form {
                    record(
                        &mut report.dashes.variants[form],
                        positions.span(index..end),
                        mode,
                    );
                }
            }
            _ => {}
        }
        index = end;
    }
}

/// Times on the hour from 1 to 12 with *a.m.* or *p.m.* in any form: `6:00 a.m.` has minutes,
/// `6 a.m.` the hour only. Other times (`6:30 a.m.`, `18:00`) have no on-the-hour choice.
fn clock_times(document: &Document, positions: &Positions, check: &mut ConsistencyCheck) {
    let chars = &positions.chars;
    let at = |index: usize| chars.get(index).copied();
    let mut index = 0;
    while index < chars.len() {
        let start = index;
        if !chars[start].is_ascii_digit()
            || start.checked_sub(1).is_some_and(|before| {
                chars[before].is_alphanumeric() || matches!(chars[before], ':' | '.' | ',')
            })
        {
            index += 1;
            continue;
        }
        while at(index).is_some_and(|ch| ch.is_ascii_digit()) {
            index += 1;
        }
        let hour: String = chars[start..index].iter().collect();
        if !hour
            .parse::<u8>()
            .is_ok_and(|hour| (1..=12).contains(&hour))
            || hour.starts_with('0')
        {
            continue;
        }
        let mut end = index;
        let minutes = at(end) == Some(':');
        if minutes {
            if at(end + 1) != Some('0')
                || at(end + 2) != Some('0')
                || at(end + 3).is_some_and(|ch| ch.is_ascii_digit())
            {
                continue;
            }
            end += 3;
        }
        if at(end).is_some_and(|ch| matches!(ch, ' ' | '\u{a0}' | '\u{202f}')) {
            end += 1;
        }
        if !at(end).is_some_and(|ch| matches!(ch, 'a' | 'A' | 'p' | 'P')) {
            continue;
        }
        end += 1;
        let dotted = at(end) == Some('.');
        if dotted {
            end += 1;
        }
        if !at(end).is_some_and(|ch| matches!(ch, 'm' | 'M')) {
            continue;
        }
        end += 1;
        if at(end).is_some_and(char::is_alphanumeric) {
            continue;
        }
        // `a.m.` keeps its final period; after `am` a period ends the sentence.
        if dotted && at(end) == Some('.') {
            end += 1;
        }
        let mode = document.mode_at(positions.at[start].0);
        let form = usize::from(!minutes);
        record(&mut check.variants[form], positions.span(start..end), mode);
        index = end;
    }
}

struct Word {
    range: Range<usize>,
    text: String,
    /// First word of the sentence, or the first after a quotation opens, a colon, a line
    /// separator, or a mid-sentence `.`, `!`, `?` or `…`.
    initial: bool,
    /// Separated from the previous word by spaces alone.
    joined: bool,
    /// Follows a comma or semicolon: a reflowed verse line or a vocative when capitalized.
    clause: bool,
    skipped: bool,
    token: Option<usize>,
}

fn joiner(ch: char) -> bool {
    matches!(ch, '-' | '‐' | '\'' | '’')
}

fn words(positions: &Positions, range: Range<usize>, tokens: &[Token]) -> Vec<Word> {
    let chars = &positions.chars;
    let mut words = Vec::new();
    let mut index = range.start;
    let mut gap_start = range.start;
    while index < range.end {
        if !chars[index].is_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while index < range.end
            && (chars[index].is_alphabetic()
                || (joiner(chars[index])
                    && index + 1 < range.end
                    && chars[index + 1].is_alphabetic()))
        {
            index += 1;
        }
        let gap = &chars[gap_start..start];
        let initial = words.is_empty()
            || gap.iter().any(|ch| {
                matches!(
                    ch,
                    '“' | '"' | '‘' | ':' | '.' | '!' | '?' | '…' | '\u{2028}' | '\u{2029}'
                )
            })
            || gap
                .windows(2)
                .any(|pair| pair[0].is_whitespace() && pair[1] == '\'');
        let joined = !words.is_empty()
            && !gap.is_empty()
            && gap
                .iter()
                .all(|ch| ch.is_whitespace() && !matches!(ch, '\u{2028}' | '\u{2029}'));
        let (byte_start, byte_end) = (positions.at[start].0, positions.at[index].0);
        let overlapping = |token: &Token| {
            token.span.byte.start < byte_end && byte_start < token.span.byte.end && token.is_word
        };
        words.push(Word {
            range: start..index,
            text: chars[start..index].iter().collect(),
            initial,
            joined,
            clause: gap.iter().any(|ch| matches!(ch, ',' | ';')),
            skipped: tokens
                .iter()
                .any(|token| overlapping(token) && token.is_skipped),
            token: tokens
                .iter()
                .position(|token| token.span.byte.start == byte_start),
        });
        gap_start = index;
    }
    words
}

fn fold(text: &str) -> String {
    text.to_lowercase().replace('’', "'").replace('‐', "-")
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Case {
    Lower,
    Capitalized,
}

fn case(text: &str) -> Option<Case> {
    let mut letters = text.chars().filter(|ch| ch.is_alphabetic());
    let first = letters.next()?;
    let rest: Vec<_> = letters.collect();
    if rest.is_empty() || !rest.iter().all(|ch| ch.is_lowercase()) {
        return None;
    }
    if first.is_lowercase() {
        Some(Case::Lower)
    } else if first.is_uppercase() {
        Some(Case::Capitalized)
    } else {
        None
    }
}

/// A short paragraph without closing punctuation whose words are capitalized reads as a title.
fn title(paragraph: &Paragraph) -> bool {
    let text = paragraph.text.trim();
    let words: Vec<_> = text.split_whitespace().collect();
    words.len() <= 10
        && !text.ends_with(['.', '!', '?', '…', '"', '”', '’', '\'', ',', ';', ':'])
        && words
            .iter()
            .filter(|word| word.chars().count() > 3)
            .all(|word| word.chars().next().is_some_and(char::is_uppercase))
}

/// Whether each character lies between `_` or `*` emphasis marks, which set off titles of works.
fn emphasis(positions: &Positions) -> Vec<bool> {
    let chars = &positions.chars;
    let mut inside = vec![false; chars.len()];
    let mut open: Option<char> = None;
    for index in 0..chars.len() {
        let ch = chars[index];
        if matches!(ch, '_' | '*') {
            let before = index.checked_sub(1).map(|i| chars[i]);
            let after = chars.get(index + 1).copied();
            let word = |ch: Option<char>| ch.is_some_and(char::is_alphanumeric);
            match open {
                Some(mark) if mark == ch && !word(after) => open = None,
                None if !word(before) && after.is_some_and(|ch| !ch.is_whitespace()) => {
                    open = Some(ch)
                }
                _ => {}
            }
        }
        inside[index] = open.is_some();
    }
    inside
}

/// A sentence in which at least half of the longer mid-sentence words are capitalized is a title
/// or a contents line (`The Whiteness of the Whale.`), not prose.
fn title_case(words: &[Word]) -> bool {
    let long: Vec<_> = words
        .iter()
        .filter(|word| !word.initial && word.text.chars().count() > 3)
        .collect();
    let capitalized = long
        .iter()
        .filter(|word| word.text.chars().next().is_some_and(char::is_uppercase))
        .count();
    capitalized >= 2 && capitalized * 2 >= long.len()
}

/// Universal POS families of a token's readings, without proper nouns.
fn families(token: &Token) -> BTreeSet<&'static str> {
    token
        .readings
        .iter()
        .filter(|reading| !reading.pos.starts_with("NNP"))
        .filter_map(|reading| penn_to_universal(&reading.pos))
        .collect()
}

struct Occurrence {
    span: Span,
    mode: Mode,
    surface: String,
    families: Option<BTreeSet<&'static str>>,
}

fn word_groups<K: Ord + Copy>(
    groups: BTreeMap<String, BTreeMap<K, Vec<Occurrence>>>,
    minimum_minority: usize,
    keep: impl Fn(&BTreeMap<K, Vec<Occurrence>>) -> bool,
) -> Vec<ConsistencyWord> {
    let mut words: Vec<_> = groups
        .into_iter()
        .filter(|(_, forms)| keep(forms))
        .map(|(word, forms)| {
            let mut variants: Vec<_> = forms
                .into_values()
                .map(|occurrences| {
                    let mut variant = ConsistencyVariant {
                        form: occurrences[0].surface.clone(),
                        ..Default::default()
                    };
                    for occurrence in occurrences {
                        record(&mut variant, occurrence.span, occurrence.mode);
                    }
                    variant
                })
                .collect();
            variants.sort_by(|a, b| b.counts.all.cmp(&a.counts.all).then(a.form.cmp(&b.form)));
            ConsistencyWord { word, variants }
        })
        .filter(|word| mixed(&word.variants, minimum_minority))
        .collect();
    words.sort_by(|a, b| {
        let total =
            |word: &ConsistencyWord| word.variants.iter().map(|v| v.counts.all).sum::<usize>();
        total(b).cmp(&total(a)).then(a.word.cmp(&b.word))
    });
    words
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Join {
    Hyphenated,
    Open,
    Closed,
}

pub fn report(
    document: &Document,
    options: &ConsistencyOptions,
) -> Result<ConsistencyReport, StyleError> {
    if options.minimum_minority == 0 {
        return Err(StyleError::options(
            "minimumMinority must be at least 1 occurrence",
        ));
    }
    options.preferred.validate()?;
    let mut report = ConsistencyReport {
        double_quotes: ConsistencyCheck {
            mixed: false,
            variants: variants(&QUOTE_FORMS),
            ..Default::default()
        },
        single_quotes: ConsistencyCheck {
            mixed: false,
            variants: variants(&QUOTE_FORMS),
            ..Default::default()
        },
        apostrophes: ConsistencyCheck {
            mixed: false,
            variants: variants(&QUOTE_FORMS),
            ..Default::default()
        },
        ellipses: ConsistencyCheck {
            mixed: false,
            variants: variants(&ELLIPSIS_FORMS),
            ..Default::default()
        },
        dashes: ConsistencyCheck {
            mixed: false,
            variants: variants(&DASH_FORMS),
            ..Default::default()
        },
        clock_times: ConsistencyCheck {
            mixed: false,
            variants: variants(&CLOCK_FORMS),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut dash_characters: [Split<usize>; 3] = Default::default();
    let mut capitals: BTreeMap<String, BTreeMap<Case, Vec<Occurrence>>> = BTreeMap::new();
    let mut joins: BTreeMap<String, BTreeMap<Join, Vec<Occurrence>>> = BTreeMap::new();
    let mut hyphenated: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut closed: BTreeSet<String> = BTreeSet::new();
    let mut sentence_words = Vec::new();
    let paragraph_positions: Vec<_> = document.paragraphs.iter().map(Positions::new).collect();
    for (paragraph_index, paragraph) in document.paragraphs.iter().enumerate() {
        let positions = &paragraph_positions[paragraph_index];
        punctuation(document, positions, &mut report, &mut dash_characters);
        clock_times(document, positions, &mut report.clock_times);
        let titled = title(paragraph);
        let emphasized = emphasis(positions);
        for sentence in &paragraph.sentences {
            let range =
                positions.index(sentence.span.byte.start)..positions.index(sentence.span.byte.end);
            let words = words(positions, range, &sentence.tokens);
            let headline = title_case(&words);
            for (index, word) in words.iter().enumerate() {
                if word.skipped {
                    continue;
                }
                let folded = fold(&word.text);
                let occurrence = || Occurrence {
                    span: positions.span(word.range.clone()),
                    mode: document.mode_at(positions.at[word.range.start].0),
                    surface: word.text.clone(),
                    families: word
                        .token
                        .filter(|_| document.token_details)
                        .map(|token| families(&sentence.tokens[token])),
                };
                if folded.contains('-') {
                    let parts: Vec<_> = folded.split('-').map(str::to_owned).collect();
                    hyphenated.entry(parts.concat()).or_insert(parts);
                } else {
                    closed.insert(folded.clone());
                }
                let name_run = |neighbour: Option<&Word>| {
                    neighbour.is_some_and(|neighbour| {
                        !neighbour.initial
                            && !neighbour.skipped
                            && neighbour
                                .text
                                .chars()
                                .next()
                                .is_some_and(char::is_uppercase)
                    })
                };
                let Some(shape) = case(&word.text) else {
                    continue;
                };
                if titled
                    || headline
                    || emphasized[word.range.start]
                    || word.initial
                    || folded == "i"
                    || FUNCTION_WORDS.contains(&folded.as_str())
                    || folded.starts_with("i'")
                    || (shape == Case::Capitalized
                        && (word.clause
                            || (word.joined && index > 0 && name_run(words.get(index - 1)))
                            || words
                                .get(index + 1)
                                .is_some_and(|next| next.joined && name_run(Some(next)))))
                {
                    continue;
                }
                capitals
                    .entry(folded)
                    .or_default()
                    .entry(shape)
                    .or_default()
                    .push(occurrence());
            }
            sentence_words.push((paragraph_index, sentence, words));
        }
    }
    let longest = hyphenated.values().map(Vec::len).max().unwrap_or(0);
    for (paragraph, sentence, words) in &sentence_words {
        let positions = &paragraph_positions[*paragraph];
        for (index, word) in words.iter().enumerate() {
            if word.skipped {
                continue;
            }
            let folded = fold(&word.text);
            let span = |last: usize| positions.span(word.range.start..words[last].range.end);
            let mode = document.mode_at(positions.at[word.range.start].0);
            let occurrence = |last: usize, surface: String| Occurrence {
                span: span(last),
                mode,
                surface,
                families: None,
            };
            if hyphenated.contains_key(&folded.replace('-', "")) && folded.contains('-') {
                joins
                    .entry(folded.replace('-', ""))
                    .or_default()
                    .entry(Join::Hyphenated)
                    .or_default()
                    .push(occurrence(index, folded.clone()));
                continue;
            }
            if folded.contains('-') {
                continue;
            }
            let mut length = 2;
            let mut parts = vec![folded.clone()];
            while length <= longest.max(2) && index + length <= words.len() {
                let next = &words[index + length - 1];
                if !next.joined || next.skipped || next.text.contains(['-', '‐']) {
                    break;
                }
                parts.push(fold(&next.text));
                let key = parts.concat();
                let seeded = hyphenated.get(&key).is_some_and(|hyphen| *hyphen == parts);
                let unseeded = length == 2
                    && closed.contains(&key)
                    && parts.iter().all(|part| {
                        part.chars().count() >= 3 && !FUNCTION_WORDS.contains(&part.as_str())
                    });
                // An open form counts only where a hyphen would join it: before another word.
                let attributive = words.get(index + length).is_some_and(|following| {
                    following.joined
                        && sentence
                            .tokens
                            .get(following.token.unwrap_or(usize::MAX))
                            .is_none_or(|token| {
                                token.readings.is_empty()
                                    || families(token).iter().any(|f| matches!(*f, "NOUN" | "ADJ"))
                            })
                });
                if (seeded && (attributive || closed.contains(&key))) || unseeded {
                    joins
                        .entry(key)
                        .or_default()
                        .entry(Join::Open)
                        .or_default()
                        .push(occurrence(index + length - 1, parts.join(" ")));
                    break;
                }
                length += 1;
            }
        }
    }
    for (paragraph, _, words) in &sentence_words {
        let positions = &paragraph_positions[*paragraph];
        for word in words.iter().filter(|word| !word.skipped) {
            let folded = fold(&word.text);
            if !folded.contains('-') {
                if let Some(forms) = joins.get_mut(&folded) {
                    forms.entry(Join::Closed).or_default().push(Occurrence {
                        span: positions.span(word.range.clone()),
                        mode: document.mode_at(positions.at[word.range.start].0),
                        surface: folded,
                        families: None,
                    });
                }
            }
        }
    }
    report.capitalization = word_groups(capitals, options.minimum_minority, |forms| {
        // A capitalized homograph with no common-word reading in common (`May`/`may`) is a name,
        // and a capitalized verb or adverb mid-sentence is verse or a title, not a house style.
        let shared = |case: Case| -> Option<BTreeSet<&'static str>> {
            let occurrences = forms.get(&case)?;
            occurrences.iter().map(|o| o.families.clone()).try_fold(
                BTreeSet::new(),
                |mut all, families| {
                    all.extend(families?);
                    Some(all)
                },
            )
        };
        match (shared(Case::Lower), shared(Case::Capitalized)) {
            (Some(lower), Some(upper)) => lower
                .intersection(&upper)
                .any(|family| matches!(*family, "NOUN" | "ADJ")),
            _ => true,
        }
    });
    report.hyphenation = word_groups(joins, options.minimum_minority, |forms| forms.len() >= 2);
    report.dash_characters = DASH_CHARACTERS
        .iter()
        .zip(dash_characters)
        .map(|(form, counts)| ConsistencyCount {
            form: (*form).to_owned(),
            counts,
        })
        .collect();
    for check in [
        &mut report.double_quotes,
        &mut report.single_quotes,
        &mut report.apostrophes,
        &mut report.ellipses,
        &mut report.dashes,
        &mut report.clock_times,
    ] {
        check.mixed = mixed(&check.variants, options.minimum_minority);
    }
    let preferred = options.preferred.checks();
    for (check, (_, form, _)) in [
        &mut report.double_quotes,
        &mut report.single_quotes,
        &mut report.apostrophes,
        &mut report.ellipses,
        &mut report.dashes,
        &mut report.clock_times,
    ]
    .into_iter()
    .zip(preferred)
    {
        let Some(form) = form else { continue };
        check.non_preferred = check
            .variants
            .iter()
            .filter(|variant| variant.form != form)
            .map(|variant| variant.counts.all)
            .sum();
        check.preferred = Some(form.to_owned());
    }
    Ok(report)
}
