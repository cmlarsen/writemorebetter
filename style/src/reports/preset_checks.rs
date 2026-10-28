//! Built-in checks that a style preset turns on: house rules that the house-style regex schema
//! can't express cleanly, because they need number context, quotation structure or token readings.
//! Like every style report, they diagnose. A match's `replacement` is at most a fixed term from the
//! check's own table (`a.m.`, `mom`, a moved period); no check builds replacement prose.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use super::Split;
use crate::document::leading_elision;
use crate::{Document, Mode, Paragraph, Span, StyleError, Token};

pub const REPORT: &str = "presetChecks";

/// Which built-in checks run, and how. Everything is off by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetChecks {
    /// Numerals that prose spells out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numbers: Option<NumberCheck>,
    /// Where periods and commas go relative to a closing quotation mark.
    #[serde(alias = "quote_punctuation", skip_serializing_if = "Option::is_none")]
    pub quote_punctuation: Option<QuotePunctuation>,
    /// The possessive of a singular name ending in *s*.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub possessives: Option<Possessives>,
    /// The form of *a.m.* and *p.m.* after a time.
    #[serde(alias = "time_abbreviations", skip_serializing_if = "Option::is_none")]
    pub time_abbreviations: Option<TimeAbbreviations>,
    /// Clock times: a spelled-out hour with *a.m.* or *p.m.*, the hour before *o'clock*, and
    /// doubled time words (*a.m. in the morning*).
    #[serde(alias = "clock_times", skip_serializing_if = "Option::is_none")]
    pub clock_times: Option<ClockTimeCheck>,
    /// *Mom* and *Dad* capitalized as names and lowercase after *my*, *the* or a possessive.
    #[serde(alias = "family_titles")]
    pub family_titles: bool,
}

impl PresetChecks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn with_numbers(mut self, numbers: Option<NumberCheck>) -> Self {
        self.numbers = numbers;
        self
    }

    pub fn with_quote_punctuation(mut self, style: Option<QuotePunctuation>) -> Self {
        self.quote_punctuation = style;
        self
    }

    pub fn with_possessives(mut self, style: Option<Possessives>) -> Self {
        self.possessives = style;
        self
    }

    pub fn with_time_abbreviations(mut self, style: Option<TimeAbbreviations>) -> Self {
        self.time_abbreviations = style;
        self
    }

    pub fn with_clock_times(mut self, check: Option<ClockTimeCheck>) -> Self {
        self.clock_times = check;
        self
    }

    pub fn with_family_titles(mut self, on: bool) -> Self {
        self.family_titles = on;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), StyleError> {
        match &self.numbers {
            Some(numbers) if numbers.spell_out_through > MAX_SPELL_OUT => {
                Err(StyleError::options(format!(
                    "numbers.spellOutThrough must be at most {MAX_SPELL_OUT}, got {}",
                    numbers.spell_out_through
                )))
            }
            _ => Ok(()),
        }
    }
}

const MAX_SPELL_OUT: u32 = 999;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NumberCheck {
    /// Whole numbers from zero through this one are spelled out (100 in fiction, 9 in news).
    #[serde(alias = "spell_out_through")]
    pub spell_out_through: u32,
    /// Ages stay numerals (`5 years old`, `a 5-year-old`, `aged 7`), as news styles set them.
    #[serde(alias = "ages_as_numerals")]
    pub ages_as_numerals: bool,
    /// A sentence that needs a numeral for a larger quantity keeps its small ones as numerals too
    /// (`Of 250 guests, 12 stayed`), so the numbers it compares read alike.
    #[serde(alias = "match_larger")]
    pub match_larger: bool,
}

impl Default for NumberCheck {
    fn default() -> Self {
        Self {
            spell_out_through: 100,
            ages_as_numerals: false,
            match_larger: true,
        }
    }
}

impl NumberCheck {
    pub fn new(spell_out_through: u32) -> Self {
        Self {
            spell_out_through,
            ..Self::default()
        }
    }

    pub fn with_ages_as_numerals(mut self, on: bool) -> Self {
        self.ages_as_numerals = on;
        self
    }

    pub fn with_match_larger(mut self, on: bool) -> Self {
        self.match_larger = on;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum QuotePunctuation {
    /// Periods and commas always go inside the closing quotation mark (US).
    Inside,
    /// A quoted word or phrase that is not speech keeps the period or comma outside (UK).
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Possessives {
    /// *James's*: a singular name ending in *s* takes apostrophe-s.
    ApostropheS,
    /// *James'*: a singular name ending in *s* takes the apostrophe alone.
    ApostropheOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TimeAbbreviations {
    /// *a.m.* and *p.m.*
    Dotted,
    /// *am* and *pm*
    Undotted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClockTimeCheck {
    /// How the hour before *o'clock* is written.
    #[serde(alias = "o_clock")]
    pub o_clock: OClock,
}

impl ClockTimeCheck {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_o_clock(mut self, o_clock: OClock) -> Self {
        self.o_clock = o_clock;
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum OClock {
    /// *six o'clock*: the hour is spelled out (book style).
    #[default]
    Words,
    /// *6 o'clock*: the hour is a numeral (news style).
    Numerals,
    /// Either form.
    Either,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PresetCheck {
    Numbers,
    QuotePunctuation,
    Possessives,
    TimeAbbreviations,
    ClockTimes,
    FamilyTitles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetCheckMatch {
    pub check: PresetCheck,
    pub message: String,
    pub span: Span,
    pub mode: Mode,
    /// A fixed term from the check's table, when the check has one.
    pub replacement: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PresetChecksReport {
    pub matches: Vec<PresetCheckMatch>,
    /// Matches in narration, in dialogue, and in all.
    pub counts: Split<usize>,
}

pub fn report(
    document: &Document,
    checks: &PresetChecks,
) -> Result<PresetChecksReport, StyleError> {
    checks.validate()?;
    let mut found = Vec::new();
    for paragraph in &document.paragraphs {
        let text = Text::new(paragraph);
        if let Some(numbers) = &checks.numbers {
            let o_clock = checks.clock_times.is_some();
            number_matches(&text, paragraph, numbers, o_clock, &mut found);
        }
        if let Some(style) = checks.quote_punctuation {
            quote_matches(document, &text, style, &mut found);
        }
        if let Some(style) = checks.possessives {
            possessive_matches(&text, paragraph, style, &mut found);
        }
        if let Some(style) = checks.time_abbreviations {
            time_matches(&text, style, &mut found);
        }
        if let Some(check) = checks.clock_times {
            clock_matches(&text, check, &mut found);
        }
        if checks.family_titles {
            family_matches(paragraph, &mut found);
        }
    }
    let skipped: Vec<Range<usize>> = document
        .words()
        .filter(|token| token.is_skipped)
        .map(|token| token.span.byte.clone())
        .collect();
    let mut report = PresetChecksReport::default();
    found.sort_by_key(|(_, _, span, _)| (span.byte.start, std::cmp::Reverse(span.byte.end)));
    for (check, message, span, replacement) in found {
        if skipped
            .iter()
            .any(|name| name.start < span.byte.end && span.byte.start < name.end)
        {
            continue;
        }
        let mode = document.mode_at(span.byte.start);
        for count in report.counts.all_and_mode_mut(mode) {
            *count += 1;
        }
        report.matches.push(PresetCheckMatch {
            check,
            message,
            span,
            mode,
            replacement,
        });
    }
    Ok(report)
}

type Found = (PresetCheck, String, Span, Option<String>);

/// A paragraph's text with its characters indexed, so neighbours are looked up by char index and
/// slices always fall on char boundaries.
struct Text<'a> {
    text: &'a str,
    span: &'a Span,
    chars: Vec<(usize, char)>,
    // UTF-16 offset of each char within the paragraph, plus the paragraph's length.
    utf16: Vec<usize>,
}

impl<'a> Text<'a> {
    fn new(paragraph: &'a Paragraph) -> Self {
        let chars: Vec<(usize, char)> = paragraph.text.char_indices().collect();
        let mut utf16 = Vec::with_capacity(chars.len() + 1);
        let mut at = 0;
        for (_, ch) in &chars {
            utf16.push(at);
            at += ch.len_utf16();
        }
        utf16.push(at);
        Self {
            text: &paragraph.text,
            span: &paragraph.span,
            chars,
            utf16,
        }
    }

    fn char(&self, index: usize) -> Option<char> {
        self.chars.get(index).map(|(_, ch)| *ch)
    }

    fn before(&self, index: usize) -> Option<char> {
        index.checked_sub(1).and_then(|index| self.char(index))
    }

    /// The byte offset of char `index` within the paragraph (its length at the end).
    fn byte(&self, index: usize) -> usize {
        self.chars.get(index).map_or(self.text.len(), |(at, _)| *at)
    }

    /// Absolute document bytes for chars `start..end`.
    fn bytes(&self, start: usize, end: usize) -> Range<usize> {
        self.span.byte.start + self.byte(start)..self.span.byte.start + self.byte(end)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span {
            byte: self.bytes(start, end),
            char: self.span.char.start + start..self.span.char.start + end,
            utf16: self.span.utf16.start + self.utf16[start]
                ..self.span.utf16.start + self.utf16[end],
        }
    }

    fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.text[self.byte(start)..self.byte(end)]
    }

    /// The word that ends just before char `index`, past any spaces, with letters, digits, periods
    /// and apostrophes, as a char range.
    fn word_before(&self, index: usize) -> Option<Range<usize>> {
        let mut end = index;
        while end > 0 && self.chars[end - 1].1.is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && word_char(self.chars[start - 1].1) {
            start -= 1;
        }
        (start < end).then_some(start..end)
    }

    /// Up to `count` whitespace-separated words from char `index`, as char ranges with closing
    /// punctuation other than periods trimmed.
    fn words_after(&self, mut index: usize, count: usize) -> Vec<Range<usize>> {
        let mut words = Vec::new();
        while words.len() < count {
            while self.char(index).is_some_and(char::is_whitespace) {
                index += 1;
            }
            let start = index;
            while self.char(index).is_some_and(|ch| !ch.is_whitespace()) {
                index += 1;
            }
            let mut end = index;
            while end > start && !word_char(self.chars[end - 1].1) {
                end -= 1;
            }
            if start == index {
                break;
            }
            words.push(start..end);
            if end < index {
                break;
            }
        }
        words
    }
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '.' | '\'' | '’')
}

/// Lowercase with periods and curly apostrophes folded: `A.M.` → `am`, `St.` → `st`.
fn key(word: &str) -> String {
    word.chars()
        .filter(|ch| *ch != '.')
        .map(|ch| if ch == '’' { '\'' } else { ch })
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_apostrophe(ch: char) -> bool {
    matches!(ch, '\'' | '’')
}

// ---------------------------------------------------------------------------------------------
// Numbers

const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "jan",
    "feb",
    "mar",
    "apr",
    "jun",
    "jul",
    "aug",
    "sep",
    "sept",
    "oct",
    "nov",
    "dec",
];

/// Words before a number that make it a label rather than a quantity (`Chapter 5`, `Room 12`).
const LABELS: &[&str] = &[
    "chapter",
    "chapters",
    "page",
    "pages",
    "p",
    "pp",
    "room",
    "rooms",
    "apartment",
    "apt",
    "suite",
    "unit",
    "no",
    "nos",
    "number",
    "route",
    "highway",
    "interstate",
    "flight",
    "platform",
    "gate",
    "track",
    "channel",
    "verse",
    "verses",
    "act",
    "scene",
    "part",
    "section",
    "figure",
    "fig",
    "table",
    "volume",
    "vol",
    "issue",
    "episode",
    "season",
    "level",
    "exit",
    "pier",
    "ward",
    "bed",
    "cell",
    "locker",
    "bay",
    "dock",
    "terminal",
    "year",
    "grade",
    "psalm",
    "psalms",
    "line",
    "lines",
    "rule",
    "article",
    "box",
    "lane",
    "floor",
];

/// Words after a number that make it a measurement, percentage, time or era.
const UNITS: &[&str] = &[
    "percent", "per", "am", "pm", "ad", "bc", "bce", "ce", "km", "kg", "mg", "ml", "mm", "cm",
    "mph", "kph", "km/h", "lb", "lbs", "oz", "ft", "kb", "mb", "gb", "tb", "hz", "khz", "mhz",
    "ghz", "kw", "mw", "rpm", "fps", "bpm", "mmhg",
];

const ERAS: &[&str] = &["ad", "bc", "bce", "ce"];

const STREETS: &[&str] = &[
    "street",
    "st",
    "avenue",
    "ave",
    "road",
    "rd",
    "lane",
    "ln",
    "boulevard",
    "blvd",
    "drive",
    "dr",
    "way",
    "court",
    "ct",
    "place",
    "pl",
    "terrace",
    "crescent",
    "close",
    "square",
    "sq",
    "parkway",
    "pkwy",
    "row",
    "alley",
    "highway",
    "hwy",
    "circle",
    "mews",
    "gardens",
];

const AGE_WORDS: &[&str] = &["age", "aged", "ages"];

struct Numeral {
    chars: Range<usize>,
    /// The whole number written, with thousands separators read (`1,000`); `None` for a decimal,
    /// a time, a date or a number with a leading zero.
    amount: Option<u64>,
    /// Whether it reads as a plain quantity, not a date, time, label, measurement or code.
    quantity: bool,
}

/// Every run of digits in the paragraph, with its separators.
fn numerals(text: &Text, check: &NumberCheck) -> Vec<Numeral> {
    let mut found = Vec::new();
    // The end of the last numeral read as a label, date or era, for the numerals listed after it.
    let mut label_end = None;
    let mut index = 0;
    while index < text.chars.len() {
        if !text.chars[index].1.is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        let mut end = index;
        let mut joined = false;
        loop {
            while text.char(end).is_some_and(|ch| ch.is_ascii_digit()) {
                end += 1;
            }
            let separator = text
                .char(end)
                .is_some_and(|ch| matches!(ch, '.' | ',' | ':' | '/'));
            if separator && text.char(end + 1).is_some_and(|ch| ch.is_ascii_digit()) {
                joined = true;
                end += 1;
            } else {
                break;
            }
        }
        index = end;
        let digits = text.slice(start, end);
        let thousands = joined
            && digits.split(',').skip(1).all(|group| group.len() == 3)
            && !digits.contains(['.', ':', '/']);
        let amount = (!joined || thousands)
            .then(|| digits.replace(',', ""))
            .filter(|digits| digits.len() <= 15 && !(digits.len() > 1 && digits.starts_with('0')))
            .and_then(|digits| digits.parse::<u64>().ok());
        let labelled = labelled(text, start) || coordinated(text, label_end, start, end);
        if labelled {
            label_end = Some(end);
        }
        found.push(Numeral {
            chars: start..end,
            amount,
            quantity: !labelled && quantity(text, start, end, check),
        });
    }
    found
}

/// Whether the numeral at chars `start..end` reads as a plain quantity, not a date, time, address,
/// label, measurement, code or list marker.
fn quantity(text: &Text, start: usize, end: usize, check: &NumberCheck) -> bool {
    if text.before(start).is_some_and(|ch| {
        ch.is_alphanumeric()
            || matches!(
                ch,
                '_' | '-'
                    | '–'
                    | '—'
                    | '−'
                    | '.'
                    | ','
                    | '/'
                    | ':'
                    | '$'
                    | '£'
                    | '€'
                    | '¥'
                    | '#'
                    | '+'
                    | '\''
                    | '’'
                    | '‘'
                    | '@'
                    | '§'
                    | '¶'
            )
    }) {
        return false;
    }
    let mut end = end;
    match text.char(end) {
        Some(ch) if ch.is_alphabetic() => {
            // Ordinals (`5th`) are numbers too; any other attached letter makes a code (`3D`).
            let suffix: String = text.chars[end..]
                .iter()
                .map(|(_, ch)| *ch)
                .take_while(|ch| ch.is_alphanumeric())
                .collect();
            if !matches!(suffix.to_lowercase().as_str(), "st" | "nd" | "rd" | "th") {
                return false;
            }
            end += suffix.chars().count();
        }
        Some('%' | '°' | '′' | '″' | '\'' | '"' | '’' | '”' | '×' | '+' | '_' | '&') => {
            return false
        }
        Some('-' | '–' | '—' | '‐') if text.char(end + 1).is_some_and(|ch| ch.is_ascii_digit()) => {
            return false
        }
        Some('.' | ')') if text.chars[..start].iter().all(|(_, ch)| ch.is_whitespace()) => {
            // A list marker at the start of a paragraph: `1. Eggs`.
            return false;
        }
        _ => {}
    }
    let before = text
        .word_before(start)
        .map(|word| key(text.slice(word.start, word.end)));
    let after: Vec<String> = text
        .words_after(end, 4)
        .into_iter()
        .map(|word| key(text.slice(word.start, word.end)))
        .collect();
    // A hyphenated compound right after the numeral (`5-year-old`) is part of the next word.
    let compound = text
        .char(end)
        .filter(|ch| matches!(ch, '-' | '‐'))
        .map(|_| key(text.slice(end, text.words_after(end, 1).first().map_or(end, |w| w.end))));
    let next = after.first().map(String::as_str);
    if check.ages_as_numerals && before.is_some_and(|before| AGE_WORDS.contains(&before.as_str())) {
        return false;
    }
    if next.is_some_and(|next| MONTHS.contains(&next) && month_after(text, end)) {
        return false;
    }
    if next == Some("of")
        && after
            .get(1)
            .is_some_and(|word| MONTHS.contains(&word.as_str()))
    {
        return false;
    }
    if next.is_some_and(|next| UNITS.contains(&next)) {
        return false;
    }
    if check.ages_as_numerals {
        let aged = |words: &[String]| {
            matches!(
                words,
                [unit, old, ..] if matches!(unit.as_str(), "year" | "years" | "month" | "months" | "week" | "weeks" | "day" | "days") && old == "old"
            )
        };
        if aged(&after)
            || compound
                .as_deref()
                .is_some_and(|word| word.contains("-old") || word.ends_with("-year"))
        {
            return false;
        }
    }
    !address(text, end)
}

/// Whether the word right before the numeral at char `start` makes it a label, date or era
/// (`Chapter 5`, `May 5`, `AD 70`).
fn labelled(text: &Text, start: usize) -> bool {
    text.word_before(start).is_some_and(|word| {
        let before = key(text.slice(word.start, word.end));
        LABELS.contains(&before.as_str())
            || MONTHS.contains(&before.as_str())
            || ERAS.contains(&before.as_str())
    })
}

/// `chapters 3 and 4`, `May 5, 6, and 7`: a numeral listed right after one read as a label, date
/// or era (ending at char `label_end`) is read the same way. A comma alone joins only a numeral
/// that is itself followed by a comma or a conjunction, so `May 5, 3 people came` still counts.
fn coordinated(text: &Text, label_end: Option<usize>, start: usize, end: usize) -> bool {
    let Some(label_end) = label_end else {
        return false;
    };
    let gap = text.slice(label_end, start);
    let words: Vec<String> = gap
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|word| !word.is_empty())
        .map(key)
        .collect();
    match words.as_slice() {
        [] => {
            gap.contains(',')
                && (text.char(end) == Some(',')
                    || text.words_after(end, 1).first().is_some_and(|word| {
                        matches!(key(text.slice(word.start, word.end)).as_str(), "and" | "or")
                    }))
        }
        [word] => matches!(
            word.as_str(),
            "and" | "or" | "&" | "to" | "through" | "thru"
        ),
        _ => false,
    }
}

/// Whether the word after the numeral is capitalized, so `5 May` is a date while `5 may` is not.
fn month_after(text: &Text, end: usize) -> bool {
    text.words_after(end, 1)
        .first()
        .and_then(|word| text.char(word.start))
        .is_some_and(char::is_uppercase)
}

/// `12 Baker Street`: capitalized words after the numeral that end in a street name.
fn address(text: &Text, end: usize) -> bool {
    if text.char(end).is_some_and(|ch| !ch.is_whitespace()) {
        return false;
    }
    for word in text.words_after(end, 4) {
        let surface = text.slice(word.start, word.end);
        if !surface.chars().next().is_some_and(char::is_uppercase) {
            return false;
        }
        if STREETS.contains(&key(surface).as_str()) {
            return true;
        }
        if word.end < text.chars.len() && text.char(word.end).is_some_and(|ch| !ch.is_whitespace())
        {
            return false;
        }
    }
    false
}

/// `o_clock` leaves a numeral before *o'clock* to the clock-time check.
fn number_matches(
    text: &Text,
    paragraph: &Paragraph,
    check: &NumberCheck,
    o_clock: bool,
    out: &mut Vec<Found>,
) {
    let numerals = numerals(text, check);
    let limit = check.spell_out_through;
    // A sentence that needs a numeral for a larger number keeps its small ones as numerals too,
    // so the numbers it compares read alike.
    let sentence_of = |numeral: &Numeral| {
        let byte = text.bytes(numeral.chars.start, numeral.chars.end).start;
        paragraph
            .sentences
            .iter()
            .position(|sentence| sentence.span.byte.contains(&byte))
    };
    let year = |amount: u64| (1000..=2199).contains(&amount);
    let large: Vec<Option<usize>> = numerals
        .iter()
        .filter(|numeral| {
            check.match_larger
                && numeral.quantity
                && numeral
                    .amount
                    .is_some_and(|amount| amount > u64::from(limit) && !year(amount))
        })
        .map(sentence_of)
        .collect();
    let message = format!("Spell out whole numbers from zero through {limit} in prose.");
    for numeral in &numerals {
        if !numeral.quantity
            || numeral
                .amount
                .is_none_or(|amount| amount > u64::from(limit))
        {
            continue;
        }
        if large.contains(&sentence_of(numeral)) {
            continue;
        }
        if o_clock && o_clock_after(text, numeral.chars.end) {
            continue;
        }
        let mut end = numeral.chars.end;
        while text.char(end).is_some_and(char::is_alphabetic) {
            end += 1;
        }
        let message = if end > numeral.chars.end && century_after(text, end) {
            "Spell out centuries in prose, as in “the nineteenth century”.".to_string()
        } else {
            message.clone()
        };
        out.push((
            PresetCheck::Numbers,
            message,
            text.span(numeral.chars.start, end),
            None,
        ));
    }
}

/// Whether `century` or `centuries` follows the ordinal ending at char `end`, as its own word or
/// joined by a hyphen (`19th century`, `19th-century ruins`).
fn century_after(text: &Text, end: usize) -> bool {
    let start = match text.char(end) {
        Some('-' | '‐') => end + 1,
        Some(ch) if ch.is_whitespace() => end,
        _ => return false,
    };
    text.words_after(start, 1).first().is_some_and(|word| {
        let word = key(text.slice(word.start, word.end));
        word.starts_with("century") || word.starts_with("centuries")
    })
}

// ---------------------------------------------------------------------------------------------
// Quote punctuation

/// The char indices of closing quotation marks in the paragraph. Curly double quotes are read by
/// direction; straight ones and single quotes by what surrounds them, so `don’t`, `dogs’` and a
/// foot or inch mark after a digit are not quotes.
fn closers(text: &Text) -> Vec<usize> {
    let mut found = Vec::new();
    let mut double_open = false;
    let mut single_open = false;
    for (index, &(at, ch)) in text.chars.iter().enumerate() {
        let before = text.before(index);
        let after = text.char(index + 1);
        let letter_before = before.is_some_and(char::is_alphanumeric);
        let letter_after = after.is_some_and(char::is_alphanumeric);
        let space_before = before.is_none_or(char::is_whitespace);
        match ch {
            '“' => double_open = true,
            '”' if !space_before => {
                found.push(index);
                double_open = false;
            }
            '"' if before.is_some_and(|ch| ch.is_ascii_digit()) => {}
            '"' if double_open && !space_before => {
                found.push(index);
                double_open = false;
            }
            '"' if !letter_before => double_open = true,
            '‘' | '\''
                if !letter_before
                    && letter_after
                    && !leading_elision(&text.text[at + ch.len_utf8()..]) =>
            {
                single_open = true;
            }
            '’' | '\''
                if single_open
                    && !space_before
                    && !letter_after
                    && !before.is_some_and(|ch| ch.is_ascii_digit()) =>
            {
                found.push(index);
                single_open = false;
            }
            _ => {}
        }
    }
    found
}

fn quote_matches(document: &Document, text: &Text, style: QuotePunctuation, out: &mut Vec<Found>) {
    for closer in closers(text) {
        let quote = text.chars[closer].1;
        match style {
            QuotePunctuation::Inside => {
                let Some(mark @ ('.' | ',')) = text.char(closer + 1) else {
                    continue;
                };
                if text.char(closer + 2) == Some('.') {
                    continue;
                }
                let what = if mark == '.' { "period" } else { "comma" };
                out.push((
                    PresetCheck::QuotePunctuation,
                    format!("Put the {what} inside the closing quotation mark."),
                    text.span(closer, closer + 2),
                    Some(format!("{mark}{quote}")),
                ));
            }
            QuotePunctuation::Outside => {
                let Some(mark @ ('.' | ',')) = text.before(closer) else {
                    continue;
                };
                if closer >= 2 && text.char(closer - 2) == Some('.') {
                    continue;
                }
                // Speech keeps its punctuation inside; only a quoted word or phrase that is not
                // speech, and does not open like a sentence, moves it outside.
                let end = text.bytes(closer, closer + 1).end;
                let Some(index) = document
                    .dialogue
                    .iter()
                    .position(|span| span.byte.end == end)
                else {
                    continue;
                };
                let Some(quoted) = document.dialogue[index]
                    .byte
                    .start
                    .checked_sub(text.span.byte.start)
                    .and_then(|start| text.text.get(start..end - text.span.byte.start))
                else {
                    continue;
                };
                let first = quoted.chars().skip(1).find(|ch| ch.is_alphanumeric());
                if document.is_speech(index) || !first.is_some_and(char::is_lowercase) {
                    continue;
                }
                let what = if mark == '.' { "period" } else { "comma" };
                out.push((
                    PresetCheck::QuotePunctuation,
                    format!("Put the {what} outside the closing quotation mark of a quoted word or phrase."),
                    text.span(closer - 1, closer + 1),
                    Some(format!("{quote}{mark}")),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Possessives

fn token_at(paragraph: &Paragraph, byte: usize) -> Option<&Token> {
    paragraph
        .sentences
        .iter()
        .flat_map(|sentence| &sentence.tokens)
        .find(|token| token.span.byte.start == byte && token.is_word)
}

fn has_tag(token: &Token, prefixes: &[&str]) -> bool {
    token.readings.iter().any(|reading| {
        prefixes
            .iter()
            .any(|prefix| reading.pos.starts_with(prefix))
    })
}

/// Whether the word at char `start` is the first word of its sentence, past any opening quotes.
fn opens_sentence(text: &Text, paragraph: &Paragraph, start: usize) -> bool {
    let byte = text.bytes(start, start).start;
    paragraph
        .sentences
        .iter()
        .find(|sentence| sentence.span.byte.contains(&byte))
        .and_then(|sentence| {
            let from = sentence.span.byte.start.checked_sub(text.span.byte.start)?;
            text.text.get(from..byte - text.span.byte.start)
        })
        .is_some_and(|before| !before.chars().any(char::is_alphanumeric))
}

/// Words after `’s` that make it *is* or *has* (`Chris’s going`, `Ross’s been`), not a possessive.
const AFTER_CONTRACTED_S: &[&str] = &[
    "been", "being", "going", "gone", "got", "gotten", "coming", "done", "doing", "had", "here",
    "there", "not", "never", "always", "just", "still", "already", "really", "probably", "a", "an",
    "the", "so", "too", "very", "gonna", "dead", "alive", "late", "right", "fine", "okay",
];

fn possessive_matches(
    text: &Text,
    paragraph: &Paragraph,
    style: Possessives,
    out: &mut Vec<Found>,
) {
    let closers = closers(text);
    let mut index = 0;
    while index < text.chars.len() {
        let start = index;
        while text.char(index).is_some_and(char::is_alphabetic) {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let end = index;
        let word = text.slice(start, end);
        let capitalized = word.chars().next().is_some_and(char::is_uppercase)
            && word.chars().skip(1).any(char::is_lowercase);
        if !capitalized
            || !word.ends_with('s')
            || word.chars().count() < 2
            || text
                .before(start)
                .is_some_and(|ch| ch.is_alphanumeric() || is_apostrophe(ch))
            || !text.char(end).is_some_and(is_apostrophe)
        {
            continue;
        }
        // `the Joneses’ house` is a plural family name.
        let plural_family = text
            .word_before(start)
            .is_some_and(|word| key(text.slice(word.start, word.end)) == "the");
        let token = token_at(paragraph, text.bytes(start, end).start);
        // `Americans’ rights` is a plural; readings tell it from `James’`.
        let plural = token.is_some_and(|token| {
            has_tag(token, &["NNPS", "NNS"])
                && !token.readings.iter().any(|reading| reading.pos == "NNP")
        });
        // Without readings, a capitalized word that opens a sentence may be a plural common noun
        // (`Parents’ evening`), capitalized only by its position.
        let unread = token.is_none_or(|token| token.readings.is_empty());
        if plural_family || plural || (unread && opens_sentence(text, paragraph, start)) {
            continue;
        }
        let apostrophe = text.chars[end].1;
        match style {
            Possessives::ApostropheS => {
                // `James’ car`: an apostrophe and then the possessed word.
                let followed = text.char(end + 1).is_some_and(char::is_whitespace)
                    && text.char(end + 2).is_some_and(char::is_alphabetic);
                if followed && !closers.contains(&end) {
                    out.push((
                        PresetCheck::Possessives,
                        "Form the possessive of a singular name ending in s with apostrophe-s."
                            .to_owned(),
                        text.span(end, end + 1),
                        Some(format!("{apostrophe}s")),
                    ));
                }
            }
            Possessives::ApostropheOnly => {
                // `James’s car`
                if text.char(end + 1) != Some('s')
                    || text.char(end + 2).is_some_and(char::is_alphanumeric)
                {
                    continue;
                }
                // `Chris’s going`: readings or the next word say `’s` is *is* or *has*.
                let clitic = paragraph
                    .sentences
                    .iter()
                    .flat_map(|sentence| &sentence.tokens)
                    .find(|token| token.span.byte.start == text.bytes(end, end).start);
                let contracted = clitic
                    .is_some_and(|token| !token.readings.is_empty() && !has_tag(token, &["POS"]))
                    || text.words_after(end + 2, 1).first().is_some_and(|word| {
                        AFTER_CONTRACTED_S.contains(&key(text.slice(word.start, word.end)).as_str())
                    });
                if !contracted {
                    out.push((
                        PresetCheck::Possessives,
                        "Form the possessive of a singular name ending in s with the apostrophe alone."
                            .to_owned(),
                        text.span(end, end + 2),
                        Some(apostrophe.to_string()),
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Time abbreviations

/// *a.m.* or *p.m.* in any form (`AM`, `pm`, `a.m`, `P.M.`) starting at a char, ending at a word
/// boundary.
#[derive(Clone, Copy)]
struct Meridiem {
    start: usize,
    end: usize,
    /// `a` or `p`, as written.
    letter: char,
    first_dot: bool,
    second_dot: bool,
}

impl Meridiem {
    fn at(text: &Text, start: usize) -> Option<Self> {
        let letter = text
            .char(start)
            .filter(|ch| matches!(ch, 'a' | 'A' | 'p' | 'P'))?;
        let mut end = start + 1;
        let first_dot = text.char(end) == Some('.');
        if first_dot {
            end += 1;
        }
        if !text.char(end).is_some_and(|ch| matches!(ch, 'm' | 'M')) {
            return None;
        }
        end += 1;
        let second_dot = text.char(end) == Some('.');
        if second_dot {
            end += 1;
        }
        if text.char(end).is_some_and(char::is_alphanumeric) {
            return None;
        }
        Some(Self {
            start,
            end,
            letter,
            first_dot,
            second_dot,
        })
    }

    fn morning(&self) -> bool {
        self.letter.eq_ignore_ascii_case(&'a')
    }

    /// The end without an undotted form's final period, which ends the sentence (`ten am.`).
    fn word_end(&self) -> usize {
        if self.second_dot && !self.first_dot {
            self.end - 1
        } else {
            self.end
        }
    }

    /// An undotted *am* is the verb or a radio band when a word follows (`which one am I`, `one
    /// AM station`), so after a word only an *am* before punctuation or the paragraph end is a time.
    fn after_word(&self, text: &Text) -> bool {
        if self.first_dot || self.second_dot || !self.morning() {
            return true;
        }
        let mut index = self.end;
        while text.char(index).is_some_and(space) {
            index += 1;
        }
        !text.char(index).is_some_and(char::is_alphanumeric)
    }
}

/// A space that can sit inside a time (`5 a.m.`, `5\u{a0}PM`); a line break cannot.
fn space(ch: char) -> bool {
    matches!(ch, ' ' | '\u{a0}' | '\u{202f}')
}

fn time_matches(text: &Text, style: TimeAbbreviations, out: &mut Vec<Found>) {
    let mut index = 0;
    while index < text.chars.len() {
        if !text.chars[index].1.is_ascii_digit() {
            index += 1;
            continue;
        }
        while text
            .char(index)
            .is_some_and(|ch| ch.is_ascii_digit() || ch == ':' || ch == '.')
            && !(text.char(index) == Some('.')
                && !text.char(index + 1).is_some_and(|ch| ch.is_ascii_digit()))
        {
            index += 1;
        }
        let mut start = index;
        if text.char(start).is_some_and(space) {
            start += 1;
        }
        let Some(meridiem) = Meridiem::at(text, start) else {
            continue;
        };
        let Meridiem {
            end,
            first_dot,
            second_dot,
            ..
        } = meridiem;
        let surface = text.slice(start, end);
        // `A.M.` and `P.M.` are a substitution pair in the dotted presets' house-style rules.
        if style == TimeAbbreviations::Dotted && (surface == "A.M." || surface == "P.M.") {
            continue;
        }
        let lower = meridiem.letter.to_ascii_lowercase();
        let (wanted, span_end) = match style {
            TimeAbbreviations::Dotted => (format!("{lower}.m."), end),
            TimeAbbreviations::Undotted => {
                // A final period that also ends the sentence stays out of the span, and so does
                // one after an undotted `am.`; mid-sentence, `a.m.` loses both its periods.
                let span_end = if second_dot && (!first_dot || ends_sentence(text, end)) {
                    end - 1
                } else {
                    end
                };
                (format!("{lower}m"), span_end)
            }
        };
        if text.slice(start, span_end) != wanted {
            out.push((
                PresetCheck::TimeAbbreviations,
                format!("Write the time abbreviation as “{wanted}”."),
                text.span(start, span_end),
                Some(wanted),
            ));
        }
        index = end;
    }
}

/// Whether the text from char `index` reads as the end of a sentence: the end of the paragraph or
/// a space and then a capital, past any closing quotes or brackets.
fn ends_sentence(text: &Text, mut index: usize) -> bool {
    while text
        .char(index)
        .is_some_and(|ch| matches!(ch, '”' | '’' | '"' | '\'' | ')' | ']'))
    {
        index += 1;
    }
    match text.char(index) {
        None => true,
        Some(ch) if ch.is_whitespace() => {
            while text.char(index).is_some_and(char::is_whitespace) {
                index += 1;
            }
            text.char(index)
                .is_none_or(|ch| ch.is_uppercase() || matches!(ch, '“' | '"' | '‘' | '\''))
        }
        Some(_) => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Clock times

const HOURS: [&str; 12] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve",
];

/// Words that spell the minutes after an hour (`thirty`, `forty-five`, `oh five`).
const MINUTES: &[&str] = &[
    "oh",
    "o",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
];

/// Phrases that repeat what *a.m.* or *p.m.* already says, by whether they mean the morning.
const DOUBLED: &[(bool, &[&str])] = &[
    (true, &["in", "the", "morning"]),
    (true, &["this", "morning"]),
    (false, &["in", "the", "afternoon"]),
    (false, &["in", "the", "evening"]),
    (false, &["at", "night"]),
    (false, &["this", "afternoon"]),
    (false, &["this", "evening"]),
    (false, &["tonight"]),
];

fn parts(word: &str) -> impl Iterator<Item = &str> {
    word.split(['-', '‐'])
}

/// An hour spelled out, alone or hyphenated to its minutes (`eight`, `eight-thirty`).
fn hour_word(word: &str) -> bool {
    let mut parts = parts(word);
    parts.next().is_some_and(|hour| HOURS.contains(&hour))
        && parts.all(|part| MINUTES.contains(&part))
}

fn minute_word(word: &str) -> bool {
    parts(word).all(|part| MINUTES.contains(&part))
}

/// The word ending at char `end` made of letters and hyphens, as a char range.
fn letters_before(text: &Text, end: usize) -> Option<Range<usize>> {
    let mut start = end;
    while start > 0 {
        let ch = text.chars[start - 1].1;
        if !(ch.is_alphabetic() || (start < end && matches!(ch, '-' | '‐'))) {
            break;
        }
        start -= 1;
    }
    while start < end && !text.chars[start].1.is_alphabetic() {
        start += 1;
    }
    (start < end).then_some(start..end)
}

/// The start of a time spelled out in words ending just before char `index` across spaces
/// (`eight`, `six thirty`, `ten forty-five`, `nine oh five`).
fn spelled_time(text: &Text, index: usize) -> Option<usize> {
    // Up to three number words, nearest first.
    let mut words: Vec<(usize, String)> = Vec::new();
    let mut at = index;
    while words.len() < 3 {
        let mut end = at;
        while end > 0 && space(text.chars[end - 1].1) {
            end -= 1;
        }
        if end == at {
            break;
        }
        let Some(word) = letters_before(text, end) else {
            break;
        };
        let surface = text.slice(word.start, word.end).to_lowercase();
        if !hour_word(&surface) && !minute_word(&surface) {
            break;
        }
        words.push((word.start, surface));
        at = word.start;
    }
    // The hour is the farthest word that has only minutes after it: `six thirty`, not `thirty`.
    (0..words.len())
        .rev()
        .find(|&hour| {
            hour_word(&words[hour].1) && words[..hour].iter().all(|(_, w)| minute_word(w))
        })
        .map(|hour| words[hour].0)
}

/// Whether *o'clock* is the next word after char `end`.
fn o_clock_after(text: &Text, end: usize) -> bool {
    let mut start = end;
    while text.char(start).is_some_and(space) {
        start += 1;
    }
    start > end && o_clock_at(text, start)
}

/// Whether *o'clock* (any apostrophe or case) is the word starting at char `start`.
fn o_clock_at(text: &Text, start: usize) -> bool {
    // Most words starting with `o` (`of`, `on`, `or`) fail here without allocating.
    if !matches!(text.char(start), Some('o' | 'O'))
        || !text.char(start + 1).is_some_and(is_apostrophe)
    {
        return false;
    }
    let word: String = (start..start + 7)
        .filter_map(|index| text.char(index))
        .collect();
    key(&word) == "o'clock" && !text.char(start + 7).is_some_and(char::is_alphanumeric)
}

fn clock_matches(text: &Text, check: ClockTimeCheck, out: &mut Vec<Found>) {
    let mut index = 0;
    while index < text.chars.len() {
        let start = index;
        index += 1;
        // A meridiem may follow its numeral directly (`6pm`); anything else starts a word.
        let after_digit = text.before(start).is_some_and(|ch| ch.is_ascii_digit())
            && matches!(text.char(start), Some('a' | 'A' | 'p' | 'P'));
        if text.before(start).is_some_and(char::is_alphanumeric) && !after_digit {
            continue;
        }
        if text.char(start).is_some_and(|ch| ch.is_ascii_digit()) {
            o_clock_numeral(text, start, check.o_clock, out);
            continue;
        }
        if matches!(text.char(start), Some('o' | 'O')) {
            if o_clock_at(text, start) {
                o_clock_word(text, start, check.o_clock, out);
            }
            continue;
        }
        let Some(meridiem) = Meridiem::at(text, start) else {
            continue;
        };
        let Some(time) = time_before(text, &meridiem) else {
            continue;
        };
        if !meridiem.after_word(text) && time != Time::Numeral {
            continue;
        }
        let surface = text.slice(meridiem.start, meridiem.word_end());
        match time {
            Time::Spelled(hour) => {
                let twelve = text.slice(hour, meridiem.start).trim_end_matches(space);
                let instead = match (twelve.eq_ignore_ascii_case("twelve"), meridiem.morning()) {
                    (true, true) => "write “midnight”",
                    (true, false) => "write “noon”",
                    (false, true) => "spell the time out with “in the morning”",
                    (false, false) => {
                        "spell the time out with “in the afternoon” or “in the evening”"
                    }
                };
                out.push((
                    PresetCheck::ClockTimes,
                    format!("Use numerals with “{surface}”, or {instead} instead."),
                    text.span(hour, meridiem.word_end()),
                    None,
                ));
            }
            Time::OClock(o_clock) => out.push((
                PresetCheck::ClockTimes,
                format!(
                    "Use “{}” or “{surface}”, not both.",
                    text.slice(o_clock, o_clock + 7)
                ),
                text.span(o_clock, meridiem.word_end()),
                None,
            )),
            Time::Numeral => {}
        }
        doubled(text, &meridiem, out);
        index = meridiem.end;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Time {
    /// `8 a.m.`, `8:30 a.m.`
    Numeral,
    /// `eight a.m.`, from the char where the hour starts.
    Spelled(usize),
    /// `six o'clock a.m.`, from the char where *o'clock* starts.
    OClock(usize),
}

/// What reads as a time before *a.m.* or *p.m.*, if anything does.
fn time_before(text: &Text, meridiem: &Meridiem) -> Option<Time> {
    let mut end = meridiem.start;
    while end > 0 && space(text.chars[end - 1].1) {
        end -= 1;
    }
    if end > 0 && text.chars[end - 1].1.is_ascii_digit() {
        return (end == meridiem.start || end + 1 == meridiem.start).then_some(Time::Numeral);
    }
    if end == meridiem.start {
        return None;
    }
    if let Some(word) = text.word_before(meridiem.start) {
        if key(text.slice(word.start, word.end)) == "o'clock" {
            return Some(Time::OClock(word.start));
        }
    }
    spelled_time(text, meridiem.start).map(Time::Spelled)
}

/// `5 p.m. in the evening`: a phrase after *a.m.* or *p.m.* that says the same thing.
fn doubled(text: &Text, meridiem: &Meridiem, out: &mut Vec<Found>) {
    // `at 9 a.m. This morning, …` starts a new sentence.
    if meridiem.second_dot && ends_sentence(text, meridiem.end) {
        return;
    }
    if !text.char(meridiem.end).is_some_and(space) {
        return;
    }
    let after = text.words_after(meridiem.end, 3);
    let keys: Vec<String> = after
        .iter()
        .map(|word| key(text.slice(word.start, word.end)))
        .collect();
    for (morning, phrase) in DOUBLED {
        if *morning != meridiem.morning()
            || keys.len() < phrase.len()
            || !keys
                .iter()
                .zip(phrase.iter())
                .all(|(word, want)| word == want)
        {
            continue;
        }
        let mut end = after[phrase.len() - 1].end;
        while text.before(end) == Some('.') {
            end -= 1;
        }
        out.push((
            PresetCheck::ClockTimes,
            format!(
                "“{}” and “{}” say the same thing; keep one.",
                text.slice(meridiem.start, meridiem.end),
                text.slice(after[0].start, end)
            ),
            text.span(meridiem.start, end),
            None,
        ));
        return;
    }
}

/// `6 o'clock` in the `words` style: the numeral at char `start`.
fn o_clock_numeral(text: &Text, start: usize, style: OClock, out: &mut Vec<Found>) {
    if style != OClock::Words
        || text
            .before(start)
            .is_some_and(|ch| matches!(ch, ':' | '.' | ','))
    {
        return;
    }
    let mut end = start;
    while text.char(end).is_some_and(|ch| ch.is_ascii_digit()) {
        end += 1;
    }
    // Only an hour from 1 to 12 reads as a clock time (`0 o'clock`, `99 o'clock` do not).
    let hour = text.slice(start, end);
    if hour.starts_with('0')
        || !hour
            .parse::<u8>()
            .is_ok_and(|hour| (1..=12).contains(&hour))
        || !o_clock_after(text, end)
    {
        return;
    }
    out.push((
        PresetCheck::ClockTimes,
        "Spell out the hour before “o’clock”.".to_string(),
        text.span(start, end),
        None,
    ));
}

/// `six o'clock` in the `numerals` style: *o'clock* from char `start`.
fn o_clock_word(text: &Text, start: usize, style: OClock, out: &mut Vec<Found>) {
    if style != OClock::Numerals || !text.before(start).is_some_and(space) {
        return;
    }
    let mut end = start;
    while end > 0 && space(text.chars[end - 1].1) {
        end -= 1;
    }
    let Some(word) = letters_before(text, end) else {
        return;
    };
    if HOURS.contains(&text.slice(word.start, word.end).to_lowercase().as_str()) {
        out.push((
            PresetCheck::ClockTimes,
            "Use a numeral for the hour before “o’clock”.".to_string(),
            text.span(word.start, word.end),
            None,
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Family titles

/// Kinship words: lowercase form, capitalized form, and whether lowercase use as a name is checked.
/// *mother* and *father* are left out of that direction: lowercase they are usually common nouns.
const KIN: &[(&str, &str, bool)] = &[
    ("mom", "Mom", true),
    ("mommy", "Mommy", true),
    ("momma", "Momma", true),
    ("mama", "Mama", true),
    ("mum", "Mum", true),
    ("mummy", "Mummy", true),
    ("dad", "Dad", true),
    ("daddy", "Daddy", true),
    ("papa", "Papa", true),
    ("grandma", "Grandma", true),
    ("grandpa", "Grandpa", true),
    ("granny", "Granny", true),
    ("nana", "Nana", true),
    ("mother", "Mother", false),
    ("grandmother", "Grandmother", false),
    ("grandfather", "Grandfather", false),
];

const DETERMINERS: &[&str] = &[
    "my", "your", "his", "her", "our", "their", "its", "the", "a", "an", "this", "that", "whose",
    "one's", "every", "each", "any", "some", "no", "another",
];

/// Family nouns that can head a list of relatives sharing one determiner (`my sister, dad and I`).
const RELATIVES: &[&str] = &[
    "sister",
    "sisters",
    "brother",
    "brothers",
    "aunt",
    "aunts",
    "uncle",
    "uncles",
    "cousin",
    "cousins",
    "wife",
    "husband",
    "son",
    "sons",
    "daughter",
    "daughters",
    "parents",
    "grandparents",
    "stepmom",
    "stepdad",
    "stepmother",
    "stepfather",
    "father",
    "grandad",
    "granddad",
    "kids",
    "children",
    "family",
];

/// Words that come right before a kinship word used as a name (`asked Mom`, `with Dad`) in
/// compact documents, which have no part-of-speech readings to go on.
const NAME_CONTEXT: &[&str] = &[
    "to", "with", "for", "from", "at", "by", "tell", "told", "ask", "asked", "call", "called",
    "said", "says", "thank", "love", "hi", "hey", "bye", "oh", "like", "than", "about", "without",
    "see", "saw", "help", "helped", "and", "or", "but", "ignore", "ignored", "hug", "hugged",
    "kiss", "kissed", "text", "texted", "phone", "phoned", "visit", "visited", "where's", "where",
];

/// Nouns that make a kinship word a modifier (`dad jokes`, `mom jeans`).
const KIN_MODIFIED: &[&str] = &[
    "joke", "jokes", "jeans", "bod", "guilt", "friend", "friends", "group", "groups", "life",
    "brain", "blog", "blogs", "mode", "bag", "car", "vibes", "energy",
];

fn family_matches(paragraph: &Paragraph, out: &mut Vec<Found>) {
    for sentence in &paragraph.sentences {
        let words: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|token| !token.is_whitespace)
            .collect();
        let lower = |index: usize| words[index].text.to_lowercase().replace('’', "'");
        // Whether the kinship word at `index` follows a determiner or possessive directly.
        let determined = |index: usize| -> bool {
            let Some(previous) = index.checked_sub(1).map(|i| words[i]) else {
                return false;
            };
            let previous_text = lower(index - 1);
            // A split-off `'s` is possessive unless its readings say it is `is` or `has`.
            let clitic = previous_text == "'s"
                && (previous.readings.is_empty() || has_tag(previous, &["POS"]));
            // A lone apostrophe is possessive only right after a plural (`the kids' mom`); with
            // a space before it, it opens a quotation.
            let plural_possessive = previous_text == "'"
                && index >= 2
                && words[index - 2].is_word
                && words[index - 2].span.byte.end == previous.span.byte.start
                && lower(index - 2).ends_with('s');
            DETERMINERS.contains(&previous_text.as_str())
                || clitic
                || plural_possessive
                || (previous_text.len() > 2 && previous_text.ends_with("'s"))
                || has_tag(previous, &["PRP$", "DT"])
        };
        let family_word = |index: usize| {
            let word = lower(index);
            KIN.iter().any(|(kin, ..)| *kin == word) || RELATIVES.contains(&word.as_str())
        };
        // Whether the word at `index` is listed after a family word with a determiner, so it
        // shares that determiner: `my mom and dad`, `her mom, dad and sister`.
        let shares_determiner = |index: usize| -> bool {
            let mut at = index;
            loop {
                let mut before = at;
                while before > 0 && matches!(lower(before - 1).as_str(), "," | "and" | "or" | "&") {
                    before -= 1;
                }
                if before == at || before == 0 || !family_word(before - 1) {
                    return false;
                }
                if determined(before - 1) {
                    return true;
                }
                at = before - 1;
            }
        };
        for (index, token) in words.iter().enumerate() {
            if !token.is_word || token.is_skipped {
                continue;
            }
            let folded = token.text.to_lowercase();
            let Some(&(lowercase, capitalized, as_name)) =
                KIN.iter().find(|(lowercase, ..)| *lowercase == folded)
            else {
                continue;
            };
            if token.text == capitalized {
                if index > 0 && determined(index) {
                    out.push((
                        PresetCheck::FamilyTitles,
                        format!("Lowercase “{lowercase}” after a possessive or article; capitalize it only when it is used as a name."),
                        token.span.clone(),
                        Some(lowercase.to_owned()),
                    ));
                }
                continue;
            }
            if token.text != lowercase || !as_name {
                continue;
            }
            if words
                .get(index + 1)
                .is_some_and(|next| KIN_MODIFIED.contains(&next.text.to_lowercase().as_str()))
            {
                continue;
            }
            let named = match index.checked_sub(1).map(|i| words[i]) {
                None => true,
                Some(_) if determined(index) || shares_determiner(index) => false,
                // At the start of a sentence, or after a comma or a quotation mark: a name or a
                // vocative (`Thanks, mom`).
                Some(previous) if !previous.is_word => {
                    matches!(
                        previous.text.as_str(),
                        "," | "“" | "\"" | "‘" | "'" | "—" | "(" | "…" | ";" | ":"
                    )
                }
                Some(previous) => {
                    let previous_text = lower(index - 1);
                    if matches!(previous_text.as_str(), "and" | "or" | "&") {
                        true
                    } else if !previous.readings.is_empty() {
                        !has_tag(
                            previous,
                            &["JJ", "VBG", "VBN", "DT", "PRP$", "CD", "POS", "NN"],
                        )
                    } else {
                        NAME_CONTEXT.contains(&previous_text.as_str())
                    }
                }
            };
            if named {
                out.push((
                    PresetCheck::FamilyTitles,
                    format!("Capitalize “{capitalized}” when it is used as a name."),
                    token.span.clone(),
                    Some(capitalized.to_owned()),
                ));
            }
        }
    }
}
