//! Filters for grammar rules (`<filter class="…" args="…"/>`).
//!
//! LanguageTool runs a Java `RuleFilter` after a pattern rule has matched: it
//! can veto the match, rewrite its suggestions, or adjust the matched range.
//! This module is the on-device equivalent. Every filter the English rules use
//! that needs no data beyond the tagger, the synthesizer and regular
//! expressions has one variant; the filters that need a speller suggestion
//! model, LanguageTool's multitoken-speller data, or example handling that
//! another milestone owns stay compile errors that name the class (see
//! `compile/impls.rs`).
//!
//! Arguments follow LanguageTool's `RuleFilterEvaluator`: a value starting with
//! a backslash is a reference to the text of that numbered pattern token
//! (`\1` is the first token of the pattern), everything else is a literal.
//! Filters that run on `<regexp>` rules (a [`RegexRuleFilter`] in LanguageTool)
//! receive their arguments verbatim; see [`ArgValue::Static`].

pub(crate) mod date;

use serde::{Deserialize, Serialize};

use crate::rule::engine::composition::{GraphId, MatchGraph, MatchSentence};
use crate::types::{DefaultHashMap, Position, Span};
use crate::utils::regex::Regex;

pub(crate) use date::Date;
use date::*;

/// LanguageTool's rule tests run with a fixed "current date"
/// (`TestHackHelper`): January 1st, 2014. The rule test harness passes this to
/// the time-dependent filters so their embedded examples are reproducible.
pub(crate) fn test_date() -> Date {
    Date {
        year: 2014,
        month: 1,
        day: 1,
    }
}

/// A resolved filter argument: either a literal string or a reference to the
/// text matched by a numbered pattern token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum ArgValue {
    Static(String),
    Token(usize),
}

impl ArgValue {
    /// Resolves the argument to the string LanguageTool would pass to Java.
    pub(crate) fn resolve<'t>(
        &self,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
    ) -> String {
        match self {
            ArgValue::Static(text) => text.clone(),
            ArgValue::Token(index) => token_text(sentence, graph, *index).unwrap_or_default(),
        }
    }
}

/// The state of one rule match that a filter may inspect and change. Positions
/// and the text span are in the coordinates of the matched sentence (the same
/// coordinates the [`Suggestion`][crate::types::Suggestion] uses).
#[derive(Debug, Clone)]
pub(crate) struct FilterMatch {
    pub from: Position,
    pub to: Position,
    pub message: String,
    pub replacements: Vec<String>,
}

/// One filter per LanguageTool filter class the English rules use.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum RuleFilter {
    UnderlineSpaces(UnderlineSpaces),
    OrdinalSuffix,
    ApostropheType(ApostropheType),
    RegexAntiPattern(RegexAntiPattern),
    DateRangeChecker(DateRange),
    Adverb(Adverb),
    FutureDate(FutureDate),
    DateCheck(DateCheck),
    YmdDateCheck(YmdDateCheck),
    NewYearDate(NewYearDate),
}

impl RuleFilter {
    /// Runs the filter. `now` overrides "today" for the time-dependent filters
    /// (the rule test harness passes LanguageTool's fixed test date).
    pub(crate) fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
        now: Option<Date>,
    ) -> Option<FilterMatch> {
        match self {
            RuleFilter::UnderlineSpaces(f) => f.accept(m, sentence),
            RuleFilter::OrdinalSuffix => Some(ordinal_suffix(m)),
            RuleFilter::ApostropheType(f) => f.accept(m, sentence, graph),
            RuleFilter::RegexAntiPattern(f) => f.accept(m, sentence),
            RuleFilter::DateRangeChecker(f) => f.accept(m, sentence, graph),
            RuleFilter::Adverb(f) => f.accept(m, sentence, graph),
            RuleFilter::FutureDate(f) => f.accept(m, sentence, graph, now),
            RuleFilter::DateCheck(f) => f.accept(m, sentence, graph, now),
            RuleFilter::YmdDateCheck(f) => f.accept(m, sentence, graph, now),
            RuleFilter::NewYearDate(f) => f.accept(m, sentence, graph, now),
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn group_index(graph: &MatchGraph, n: usize) -> Option<usize> {
    graph.get_index_opt(GraphId(n))
}

fn group<'g, 't>(
    graph: &'g MatchGraph<'t>,
    n: usize,
) -> Option<&'g crate::rule::engine::composition::Group> {
    group_index(graph, n).map(|i| graph.by_index(i))
}

/// The text of the numbered pattern token, like LanguageTool indexing its
/// `patternTokens` array.
fn token_text<'t>(
    sentence: &'t MatchSentence<'t>,
    graph: &MatchGraph<'t>,
    n: usize,
) -> Option<String> {
    group(graph, n)
        .and_then(|g| g.tokens(sentence).last())
        .map(|token| token.word().as_str().to_string())
}

fn group_span<'t>(graph: &MatchGraph<'t>, n: usize) -> Option<Span> {
    group(graph, n).map(|g| g.span.clone())
}

/// Whether the token text carries a typographic (curly) apostrophe, mirroring
/// `AnalyzedTokenReadings.hasTypographicApostrophe`.
fn is_typographical_apostrophe(text: &str) -> bool {
    text.contains('\u{2019}')
}

// ---------------------------------------------------------------------------
// UnderlineSpacesFilter
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.UnderlineSpacesFilter`: extends the highlighted range
/// over whitespace before and/or after the marker.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct UnderlineSpaces {
    pub before: bool,
    pub after: bool,
}

impl UnderlineSpaces {
    fn accept<'t>(
        &self,
        mut m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
    ) -> Option<FilterMatch> {
        let text = sentence.text();
        let sentence_start = sentence.span().char().start;
        let from = m.from.char - sentence_start;
        let to = m.to.char - sentence_start;

        if self.before {
            if from > 0 {
                let prev = text[char_to_byte(text, from - 1)..char_to_byte(text, from)].to_string();
                if is_whitespace(&prev) {
                    m.from = Position {
                        byte: m.from.byte - prev.len(),
                        char: m.from.char - 1,
                    };
                }
            }
        }
        if self.after {
            if to < text.chars().count() {
                let next = text[char_to_byte(text, to)..char_to_byte(text, to + 1)].to_string();
                if is_whitespace(&next) {
                    m.to = Position {
                        byte: m.to.byte + next.len(),
                        char: m.to.char + 1,
                    };
                }
            }
        }

        Some(m)
    }
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

// ---------------------------------------------------------------------------
// OrdinalSuffixFilter
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.en.OrdinalSuffixFilter`: fixes the ordinal suffix of
/// the first suggestion ("11nd" -> "11th").
fn ordinal_suffix(mut m: FilterMatch) -> FilterMatch {
    let ordinal: String = m
        .replacements
        .first()
        .map(|s| s.chars().filter(char::is_ascii_digit).collect())
        .unwrap_or_default();

    let suffix = {
        let n: i64 = ordinal.parse().unwrap_or(0);
        if (11..=13).contains(&(n % 100)) {
            "th"
        } else {
            match n % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            }
        }
    };

    m.replacements = vec![format!("{}{}", ordinal, suffix)];
    m
}

// ---------------------------------------------------------------------------
// ApostropheTypeFilter
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.patterns.ApostropheTypeFilter`: keeps the match only
/// if the chosen word contains (or does not contain) a typographic apostrophe.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ApostropheType {
    /// `wordFrom`: a 1-based pattern token index, or the marker.
    pub word_from: WordFrom,
    pub has_typographical_apostrophe: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum WordFrom {
    Index(usize),
    Marker,
}

impl ApostropheType {
    fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
    ) -> Option<FilterMatch> {
        let index = match self.word_from {
            WordFrom::Index(i) => i,
            WordFrom::Marker => {
                // LanguageTool: skip tokens before the match, then step one
                // further (the marker token itself).
                let mut pos = 0;
                while let Some(g) = group(graph, pos + 1) {
                    if g.span.char().start >= m.from.char {
                        break;
                    }
                    pos += 1;
                }
                pos + 1
            }
        };

        let text = token_text(sentence, graph, index)?;
        if is_typographical_apostrophe(&text) == self.has_typographical_apostrophe {
            Some(m)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// RegexAntiPatternFilter
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.patterns.RegexAntiPatternFilter`: vetoes a regex
/// rule's match when one of the antipatterns overlaps it.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct RegexAntiPattern {
    pub regexes: Vec<Regex>,
}

impl RegexAntiPattern {
    fn accept<'t>(&self, m: FilterMatch, sentence: &'t MatchSentence<'t>) -> Option<FilterMatch> {
        let text = sentence.text();
        let from = m.from.char - sentence.span().char().start;
        let to = m.to.char - sentence.span().char().start;

        for regex in &self.regexes {
            for mat in regex.find_iter(text) {
                let start = byte_to_char(text, mat.start());
                let end = byte_to_char(text, mat.end());
                // partial overlap is enough to filter out a match
                if (start <= to && end >= to) || (start <= from && end >= from) {
                    return None;
                }
            }
        }
        Some(m)
    }
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

// ---------------------------------------------------------------------------
// DateRangeChecker
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.DateRangeChecker`: keeps the match only if `x >= y`,
/// i.e. the range looks like it was written backwards. The two arguments are
/// references to the two neighbouring day tokens (`x:\2 y:\4`).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DateRange {
    pub x: ArgValue,
    pub y: ArgValue,
}

impl DateRange {
    fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
    ) -> Option<FilterMatch> {
        let x = self.x.resolve(sentence, graph);
        let y = self.y.resolve(sentence, graph);

        match (x.parse::<i64>(), y.parse::<i64>()) {
            // a range written backwards: LanguageTool returns the match
            (Ok(x), Ok(y)) if x >= y => Some(m),
            // a valid range, or something that is not a range at all
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// AdverbFilter
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.en.AdverbFilter`: rewrites a suggested adverb into an
/// adjective. The adverb -> adjective table is LanguageTool data and is loaded
/// from the build directory.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct Adverb {
    #[serde(serialize_with = "crate::types::sorted::map")]
    pub adverb_to_adjective: DefaultHashMap<String, String>,
    pub adverb: ArgValue,
    pub noun: ArgValue,
}

impl Adverb {
    fn accept<'t>(
        &self,
        mut m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
    ) -> Option<FilterMatch> {
        let adverb = self.adverb.resolve(sentence, graph);
        let noun = self.noun.resolve(sentence, graph);

        if let Some(adjective) = self.adverb_to_adjective.get(&adverb) {
            if *adjective != adverb {
                m.replacements = vec![format!("{} {}", adjective, noun)];
            }
        }
        Some(m)
    }
}

// ---------------------------------------------------------------------------
// date filters
// ---------------------------------------------------------------------------

/// `org.languagetool.rules.en.FutureDateFilter`: keeps the match only when the
/// date in the text is in the future.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct FutureDate {
    pub year: ArgValue,
    pub month: ArgValue,
    pub day: ArgValue,
}

impl FutureDate {
    fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
        now: Option<Date>,
    ) -> Option<FilterMatch> {
        let date = self.date(sentence, graph)?;
        let current = now.unwrap_or_else(today);
        if date.is_after(current) {
            Some(m)
        } else {
            None
        }
    }

    fn date<'t>(&self, sentence: &'t MatchSentence<'t>, graph: &MatchGraph<'t>) -> Option<Date> {
        let year: i32 = self.year.resolve(sentence, graph).parse().ok()?;
        let month = date_arg_month(&self.month.resolve(sentence, graph))?;
        let day = date_arg_day(&self.day.resolve(sentence, graph))?;
        Date::new(year, month, day)
    }
}

/// `org.languagetool.rules.en.NewYearDateFilter` (and its `yyyy-mm-dd`
/// variant): accepts matches in January for a date whose year is one behind.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct NewYearDate {
    pub year: ArgValue,
    pub month: ArgValue,
    pub day: ArgValue,
    /// The `yyyy-mm-dd` variant (`YMDNewYearDateFilter`) passes the whole date
    /// as one argument instead of separate ones.
    pub date: Option<ArgValue>,
}

impl NewYearDate {
    fn accept<'t>(
        &self,
        mut m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
        now: Option<Date>,
    ) -> Option<FilterMatch> {
        let current = now.unwrap_or_else(today);
        if current.month != 1 {
            return None;
        }

        // (year, month, day, replacement placeholder, replacement value)
        let (date, placeholder, real_value) = if let Some(value) = &self.date {
            let token = value.resolve(sentence, graph);
            let parts: Vec<&str> = token.split('-').collect();
            if parts.len() != 3 {
                return None;
            }
            let year: i32 = parts[0].parse().ok()?;
            let month = date_arg_month(parts[1])?;
            let day = date_arg_day(parts[2])?;
            let date = Date::new(year, month, day)?;
            let real = format!("{}-{}-{}", year + 1, parts[1], parts[2]);
            (date, "{realDate}", real)
        } else {
            let year: i32 = self.year.resolve(sentence, graph).parse().ok()?;
            let month = date_arg_month(&self.month.resolve(sentence, graph))?;
            let day = date_arg_day(&self.day.resolve(sentence, graph))?;
            let date = Date::new(year, month, day)?;
            (date, "{realYear}", current.year.to_string())
        };

        if date.month != 12 && date.year + 1 == current.year {
            m.message = m
                .message
                .replace("{year}", &date.year.to_string())
                .replace(placeholder, &real_value);
            m.replacements = m
                .replacements
                .into_iter()
                .map(|r| r.replace(placeholder, &real_value))
                .collect();
            Some(m)
        } else {
            None
        }
    }
}

/// `org.languagetool.rules.en.DateCheckFilter`. LanguageTool changed the
/// argument convention between 5.2 and 6.6, so both are supported:
/// - 6.6 passes 1-based pattern-token *indices* and proposes corrected weekday
///   and day-of-month suggestions (`AbstractDateCheckWithSuggestionsFilter`).
/// - 5.2 passes the token *texts* and only vetoes the match
///   (`AbstractDateCheckFilter`).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DateCheck {
    pub kind: DateCheckKind,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum DateCheckKind {
    Index {
        week_day: usize,
        day: Option<usize>,
        month: Option<usize>,
        year: Option<usize>,
        date: Option<usize>,
    },
    Text {
        week_day: ArgValue,
        day: ArgValue,
        month: ArgValue,
        year: Option<ArgValue>,
    },
}

impl DateCheck {
    fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
        now: Option<Date>,
    ) -> Option<FilterMatch> {
        match &self.kind {
            DateCheckKind::Index {
                week_day,
                day,
                month,
                year,
                date,
            } => accept_date_check_index(
                m, sentence, graph, now, *week_day, *day, *month, *year, *date,
            ),
            DateCheckKind::Text {
                week_day,
                day,
                month,
                year,
            } => {
                accept_date_check_text(m, sentence, graph, now, week_day, day, month, year.as_ref())
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn accept_date_check_index<'t>(
    mut m: FilterMatch,
    sentence: &'t MatchSentence<'t>,
    graph: &MatchGraph<'t>,
    now: Option<Date>,
    week_day: usize,
    day_idx: Option<usize>,
    month_idx: Option<usize>,
    year_idx: Option<usize>,
    date_idx: Option<usize>,
) -> Option<FilterMatch> {
    let current = now.unwrap_or_else(today);

    let (day, month, year, day_str, month_str, year_str, is_full_date) =
        if let Some(date_idx) = date_idx {
            let token = token_text(sentence, graph, date_idx)?;
            let parts: Vec<&str> = token.split('-').collect();
            if parts.len() != 3 {
                return None;
            }
            let year: i32 = parts[0].parse().ok()?;
            let month = date_arg_month(parts[1])?;
            let day = date_arg_day(parts[2])?;
            (
                day,
                month,
                year,
                parts[2].to_string(),
                parts[1].to_string(),
                parts[0].to_string(),
                true,
            )
        } else {
            let day_str = token_text(sentence, graph, day_idx?)?;
            let month_str = token_text(sentence, graph, month_idx?)?;
            let year_str = year_idx.and_then(|i| token_text(sentence, graph, i));
            let year = match &year_str {
                Some(y) => y.parse().ok()?,
                None => current.year,
            };
            let day = date_arg_day(&day_str)?;
            let month = date_arg_month(&month_str)?;
            (
                day,
                month,
                year,
                day_str,
                month_str,
                year_str.unwrap_or_default(),
                false,
            )
        };

    let date = Date::new(year, month, day)?;
    let claimed = date_token_weekday(sentence, graph, week_day)?;
    let real = date.weekday_java();
    if claimed == real {
        return None;
    }

    // If the weekday matches the date in the current year, suggest the year.
    if let Some(change_year_date) = Date::new(current.year, month, day) {
        if claimed == change_year_date.weekday_java() {
            let year_pos = year_idx.or(date_idx).or(day_idx).unwrap_or(week_day);
            let span = group_span(graph, year_pos)?;
            m.from = span.start();
            m.to = span.end();
            m.message = "This date is wrong. Did you mean \"{currentYear}\"?"
                .replace("{currentYear}", &current.year.to_string());
            m.replacements = vec![if is_full_date {
                format!("{}-{}-{}", current.year, month_str, day_str)
            } else {
                current.year.to_string()
            }];
            return Some(m);
        }
    }

    let week_day_span = group_span(graph, week_day)?;
    let day_span = group_span(graph, day_idx.or(date_idx)?)?;
    let start = if week_day_span.start() < day_span.start() {
        week_day_span.start()
    } else {
        day_span.start()
    };
    let end = if week_day_span.end() > day_span.end() {
        week_day_span.end()
    } else {
        day_span.end()
    };
    m.from = start;
    m.to = end;

    let region = sentence.slice(Span::from_positions(start, end)).to_string();

    let real_name = weekday_name_java(real)?;
    let claimed_name = weekday_name_java(claimed)?;

    m.message = m
        .message
        .replace("{realDay}", real_name)
        .replace("{day}", claimed_name)
        .replace("{currentYear}", &current.year.to_string());

    let mut new_replacements = Vec::new();

    // 1. change the day of week
    let week_day_text = token_text(sentence, graph, week_day).unwrap_or_default();
    let new_region = replace_substring(
        &region,
        Span::from_positions(start, end),
        &week_day_span,
        &preserve_case(real_name, &week_day_text),
    );
    if !new_region.is_empty() {
        new_replacements.push(new_region);
    }

    // 2. change the day of month
    if let Some(corrected_day) = find_new_day_of_month(day, month, year, claimed) {
        let day_replacement = if is_full_date {
            format!("{}-{}-{}", year_str, month_str, corrected_day)
        } else {
            get_day_str_like_original(&corrected_day.to_string(), &day_str)
        };
        let new_region = replace_substring(
            &region,
            Span::from_positions(start, end),
            &day_span,
            &day_replacement,
        );
        if !new_region.is_empty() {
            new_replacements.push(new_region);
        }
    }

    m.replacements = new_replacements;
    Some(m)
}

/// The 5.2 `DateCheckFilter`/`YMDDateCheckFilter` behaviour: arguments are the
/// token texts and the filter only vetoes.
fn accept_date_check_text<'t>(
    m: FilterMatch,
    sentence: &'t MatchSentence<'t>,
    graph: &MatchGraph<'t>,
    now: Option<Date>,
    week_day: &ArgValue,
    day: &ArgValue,
    month: &ArgValue,
    year: Option<&ArgValue>,
) -> Option<FilterMatch> {
    let current = now.unwrap_or_else(today);
    let claimed = day_of_week_from_name(&week_day.resolve(sentence, graph))?;
    let year = match year {
        Some(y) => y.resolve(sentence, graph).parse().ok()?,
        None => current.year,
    };
    let day = date_arg_day(&day.resolve(sentence, graph))?;
    let month = date_arg_month(&month.resolve(sentence, graph))?;
    let date = Date::new(year, month, day)?;

    if claimed == date.weekday_java() {
        None
    } else {
        Some(m)
    }
}

/// The 5.2-only `org.languagetool.rules.en.YMDDateCheckFilter` (built on the
/// veto-only `AbstractDateCheckFilter`): a `yyyy-mm-dd` token whose weekday does
/// not match. It produces no suggestions.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct YmdDateCheck {
    pub date: ArgValue,
    pub week_day: ArgValue,
}

impl YmdDateCheck {
    fn accept<'t>(
        &self,
        m: FilterMatch,
        sentence: &'t MatchSentence<'t>,
        graph: &MatchGraph<'t>,
        _now: Option<Date>,
    ) -> Option<FilterMatch> {
        let token = self.date.resolve(sentence, graph);
        let parts: Vec<&str> = token.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let year: i32 = parts[0].parse().ok()?;
        let month = date_arg_month(parts[1])?;
        let day = date_arg_day(parts[2])?;
        let date = Date::new(year, month, day)?;
        let claimed = day_of_week_from_name(&self.week_day.resolve(sentence, graph))?;
        if claimed == date.weekday_java() {
            None
        } else {
            Some(m)
        }
    }
}

/// The weekday claimed in the text, from the numbered pattern token.
fn date_token_weekday<'t>(
    sentence: &'t MatchSentence<'t>,
    graph: &MatchGraph<'t>,
    n: usize,
) -> Option<u32> {
    let text = token_text(sentence, graph, n)?;
    day_of_week_from_name(&text)
}

/// A `month` argument: a number or an English month name (1 = January).
fn date_arg_month(value: &str) -> Option<u32> {
    if is_numeric(value) {
        value.trim().parse().ok()
    } else {
        month_from_name(value)
    }
}

/// A `day` argument: a number, possibly with an ordinal suffix.
fn date_arg_day(value: &str) -> Option<u32> {
    leading_number(value).map(|x| x as u32).filter(|x| *x >= 1)
}

fn find_new_day_of_month(day: u32, month: u32, year: i32, day_of_week: u32) -> Option<u32> {
    for difference in 1..7u32 {
        if day > difference {
            if let Some(date) = Date::new(year, month, day - difference) {
                if date.weekday_java() == day_of_week {
                    return Some(day - difference);
                }
            }
        }
        if day + difference < 32 {
            if let Some(date) = Date::new(year, month, day + difference) {
                if date.weekday_java() == day_of_week {
                    return Some(day + difference);
                }
            }
        }
    }
    None
}

/// `DateCheckFilter.getDayStrLikeOriginal`: appends an ordinal suffix unless the
/// original day was written numerically.
fn get_day_str_like_original(day: &str, original: &str) -> String {
    if is_numeric(original) {
        return day.to_string();
    }
    let number: u32 = day.parse().unwrap_or(0);
    let suffix = if (11..=13).contains(&(number % 100)) {
        "th"
    } else {
        match number % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{}{}", day, suffix)
}

/// Replaces the part of `region` covered by `inner` (a sub-span of `outer`) with
/// `replacement`, keeping the surrounding text exactly as it appears.
fn replace_substring(region: &str, outer: Span, inner: &Span, replacement: &str) -> String {
    let offset = outer.start().char;
    let start = inner.start().char - offset;
    let end = inner.end().char - offset;
    let region_chars: Vec<char> = region.chars().collect();
    if start > region_chars.len() || end > region_chars.len() || start > end {
        return region.to_string();
    }
    let mut out = String::new();
    out.extend(region_chars[..start].iter());
    out.push_str(replacement);
    out.extend(region_chars[end..].iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_with(replacements: &[&str]) -> FilterMatch {
        FilterMatch {
            from: Position { byte: 0, char: 0 },
            to: Position { byte: 0, char: 0 },
            message: String::new(),
            replacements: replacements.iter().map(|x| x.to_string()).collect(),
        }
    }

    #[test]
    fn ordinal_suffix_rewrites_the_first_suggestion() {
        assert_eq!(
            ordinal_suffix(match_with(&["11nd"])).replacements,
            vec!["11th"]
        );
        assert_eq!(
            ordinal_suffix(match_with(&["1nd"])).replacements,
            vec!["1st"]
        );
        assert_eq!(
            ordinal_suffix(match_with(&["102rd"])).replacements,
            vec!["102nd"]
        );
        assert_eq!(
            ordinal_suffix(match_with(&["111st"])).replacements,
            vec!["111th"]
        );
        assert_eq!(
            ordinal_suffix(match_with(&["4th"])).replacements,
            vec!["4th"]
        );
    }

    #[test]
    fn date_args_parse_month_names_and_numbers() {
        assert_eq!(date_arg_month("October"), Some(10));
        assert_eq!(date_arg_month("Oct"), Some(10));
        assert_eq!(date_arg_month("10"), Some(10));
        assert_eq!(date_arg_month("nonsense"), None);
        assert_eq!(date_arg_day("7th"), Some(7));
        assert_eq!(date_arg_day("7"), Some(7));
    }

    #[test]
    fn day_str_keeps_the_original_style() {
        assert_eq!(get_day_str_like_original("6", "7th"), "6th");
        assert_eq!(get_day_str_like_original("6", "7"), "6");
        assert_eq!(get_day_str_like_original("11", "12th"), "11th");
        assert_eq!(get_day_str_like_original("12", "12nd"), "12th");
    }
}
