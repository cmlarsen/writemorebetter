//! Title case for headings such as chapter titles. The caller passes heading strings, not prose;
//! each word is checked against a headline style: Chicago-style or AP-style headline
//! capitalization as we understand common practice, or sentence case. Written from our own
//! reading of those conventions, not ported or copied from any style manual or library. See
//! `docs/STYLE-TITLE-CASE.md`.
//!
//! Like every report it diagnoses: a match's `replacement` is the same word with its first letter
//! raised or the word lowered, never other wording.

use serde::{Deserialize, Serialize};

use crate::{Span, StyleError};

pub const REPORT: &str = "titleCase";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TitleCaseStyle {
    /// Headline style in the Chicago manner: articles, coordinating conjunctions, *to*, *as* and
    /// prepositions of any length stay lowercase.
    #[default]
    Chicago,
    /// Headline style in the AP manner: words of four or more letters are capitalized, including
    /// prepositions (*With*, *From*); shorter articles, conjunctions and prepositions are not.
    Ap,
    /// Sentence case: only the first word (and proper nouns) capitalized.
    Sentence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TitleCaseOptions {
    pub style: TitleCaseStyle,
    /// Names (characters, places) kept as written, matched word by word, ignoring case.
    #[serde(alias = "skip_names")]
    pub skip_names: Vec<String>,
}

impl TitleCaseOptions {
    pub fn new(style: TitleCaseStyle) -> Self {
        Self {
            style,
            skip_names: Vec::new(),
        }
    }

    pub fn with_skip_names(mut self, names: Vec<String>) -> Self {
        self.skip_names = names;
        self
    }
}

/// Why a word should be capitalized or lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TitleCaseRule {
    /// The heading's first word is capitalized.
    FirstWord,
    /// The last word, of the heading or of a title before a colon, is capitalized.
    LastWord,
    /// The first word after a colon, question mark or exclamation mark is capitalized.
    AfterColon,
    /// A major word (noun, verb, adjective, adverb, pronoun, subordinating conjunction; in AP
    /// style any word of four or more letters) is capitalized.
    MajorWord,
    /// An article, coordinating conjunction, *to*, *as* or preposition stays lowercase.
    MinorWord,
    /// In sentence case, a word after the first is lowercase unless it is a proper noun; add
    /// names to `skipNames` to keep them.
    SentenceCase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TitleCaseMatch {
    /// Index of the heading in the input.
    pub heading: usize,
    /// The word (one part of a hyphenated compound) within that heading.
    pub span: Span,
    pub word: String,
    /// The same word recased.
    pub replacement: String,
    pub rule: TitleCaseRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TitleCaseReport {
    pub style: TitleCaseStyle,
    pub headings: usize,
    /// Headings with at least one match.
    pub flagged_headings: usize,
    pub matches: Vec<TitleCaseMatch>,
}

const ARTICLES: &[&str] = &["a", "an", "the"];
// `n` is the clipped *and* of `rock ’n’ roll`.
const CONJUNCTIONS: &[&str] = &["and", "but", "or", "nor", "n"];
// Prepositions that are seldom anything else in a heading, lowercase in Chicago style.
const PREPOSITIONS: &[&str] = &[
    "of",
    "to",
    "as",
    "at",
    "by",
    "for",
    "from",
    "in",
    "into",
    "on",
    "onto",
    "upon",
    "with",
    "within",
    "without",
    "among",
    "amongst",
    "amid",
    "amidst",
    "against",
    "between",
    "toward",
    "towards",
    "during",
    "despite",
    "via",
    "per",
    "versus",
    "vs",
    "unto",
    "beneath",
    "beside",
    "throughout",
    "underneath",
    "o'er",
];
// Words that are prepositions before an object (`through the Night`) but adverbs, particles or
// conjunctions otherwise (`Looking Up`, `Before You Go`). Lowercase is expected only when the
// next word opens a noun phrase; elsewhere either case passes.
const AMBIGUOUS: &[&str] = &[
    "about", "above", "across", "after", "along", "around", "before", "behind", "below", "beyond",
    "down", "inside", "like", "near", "off", "out", "outside", "over", "past", "round", "since",
    "through", "till", "under", "until", "up", "than",
];
// AP style keeps these lowercase as well as the short ones above.
const AP_CONJUNCTIONS: &[&str] = &["so", "yet"];
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "my", "your", "his", "her", "its", "our", "their", "this", "that", "these",
    "those", "every", "each", "some", "any", "no",
];
// Prefixes that do not stand alone; the part after them stays as written (`Anti-intellectual`,
// `Pre-War`).
const PREFIXES: &[&str] = &[
    "anti", "bi", "co", "counter", "de", "extra", "inter", "intra", "macro", "micro", "mid",
    "multi", "non", "post", "pre", "pro", "pseudo", "re", "semi", "sub", "super", "trans", "tri",
    "ultra", "un",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Major,
    Minor,
    Ambiguous,
}

fn fold(text: &str) -> String {
    text.chars()
        .map(|c| if c == '’' { '\'' } else { c })
        .flat_map(char::to_lowercase)
        .collect()
}

fn class(word: &str, style: TitleCaseStyle) -> Class {
    let word = word.trim_matches('\'');
    let short = word.chars().count() <= 3;
    if ARTICLES.contains(&word) || CONJUNCTIONS.contains(&word) {
        return Class::Minor;
    }
    match style {
        TitleCaseStyle::Ap if AP_CONJUNCTIONS.contains(&word) => Class::Minor,
        TitleCaseStyle::Ap if !short => Class::Major,
        _ if PREPOSITIONS.contains(&word) => Class::Minor,
        _ if AMBIGUOUS.contains(&word) => Class::Ambiguous,
        _ => Class::Major,
    }
}

fn is_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{2011}')
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’')
}

struct Element {
    span: Span,
    text: String,
}

struct Word {
    elements: Vec<Element>,
    starts_segment: bool,
    ends_segment: bool,
}

/// Splits a heading into words, and words into hyphen-separated parts, with spans. Apostrophes
/// and hyphens join only between letters or digits; every other character separates words.
fn words(heading: &str) -> Vec<Word> {
    // (byte, char, utf16) offset of each char, then of the end.
    let mut offsets = Vec::with_capacity(heading.len() + 1);
    let (mut chars, mut units) = (0, 0);
    for (byte, c) in heading.char_indices() {
        offsets.push((byte, chars, units, c));
        chars += 1;
        units += c.len_utf16();
    }
    let end = (heading.len(), chars, units, ' ');
    let at = |i: usize| offsets.get(i).copied().unwrap_or(end);
    let span = |from: usize, to: usize| Span {
        byte: at(from).0..at(to).0,
        char: at(from).1..at(to).1,
        utf16: at(from).2..at(to).2,
    };
    let mut words: Vec<Word> = Vec::new();
    let mut boundary = true;
    let mut i = 0;
    while i < offsets.len() {
        let c = at(i).3;
        if !c.is_alphanumeric() {
            if matches!(c, ':' | '?' | '!') {
                boundary = true;
                if let Some(last) = words.last_mut() {
                    last.ends_segment = true;
                }
            }
            i += 1;
            continue;
        }
        let mut elements = Vec::new();
        let mut start = i;
        loop {
            i += 1;
            let c = at(i).3;
            let joins = i + 1 < offsets.len() && at(i + 1).3.is_alphanumeric();
            if i < offsets.len() && (c.is_alphanumeric() || (is_apostrophe(c) && joins)) {
                continue;
            }
            elements.push(Element {
                span: span(start, i),
                text: heading[at(start).0..at(i).0].to_owned(),
            });
            if i < offsets.len() && is_hyphen(c) && joins {
                i += 1;
                start = i;
                continue;
            }
            break;
        }
        words.push(Word {
            elements,
            starts_segment: boundary,
            ends_segment: false,
        });
        boundary = false;
    }
    if let Some(last) = words.last_mut() {
        last.ends_segment = true;
    }
    words
}

fn capitalized(text: &str) -> Option<bool> {
    let mut letters = text.chars().filter(|c| c.is_alphabetic());
    let first = letters.next()?;
    let rest_upper = letters.filter(|c| c.is_uppercase()).count();
    match (first.is_uppercase(), rest_upper) {
        (_, 0) => Some(first.is_uppercase()),
        // `FBI`, `McCall`, `iPhone`: kept as written.
        _ => None,
    }
}

fn recase(text: &str, upper: bool) -> String {
    if !upper {
        return text.to_lowercase();
    }
    let mut done = false;
    text.chars()
        .flat_map(|c| {
            if !done && c.is_alphabetic() {
                done = true;
                c.to_uppercase().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn check(heading: &str, index: usize, options: &TitleCaseOptions, out: &mut Vec<TitleCaseMatch>) {
    // An all-capitals heading is a typographic choice, not a capitalization error.
    if !heading.chars().any(char::is_lowercase) {
        return;
    }
    let skipped: Vec<String> = options
        .skip_names
        .iter()
        .flat_map(|name| name.split_whitespace().map(fold).collect::<Vec<_>>())
        .collect();
    let style = options.style;
    let words = words(heading);
    for (w, word) in words.iter().enumerate() {
        let next = words
            .get(w + 1)
            .filter(|_| !word.ends_segment)
            .map(|next| fold(&next.elements[0].text));
        let before_object = next
            .as_deref()
            .is_some_and(|next| DETERMINERS.contains(&next));
        for (e, element) in word.elements.iter().enumerate() {
            let Some(upper) = capitalized(&element.text) else {
                continue;
            };
            let folded = fold(&element.text);
            let stem = folded.split('\'').next().unwrap_or(&folded);
            // Numbers and words that start with one (`3rd`), single letters other than *a*
            // (`Plan B`), the pronoun *I* with its contractions (`I'm`) and skipped names keep
            // their case.
            if skipped.iter().any(|name| *name == folded || name == stem)
                || !element.text.chars().next().is_some_and(char::is_alphabetic)
                || (folded.chars().count() == 1 && folded != "a")
                || stem == "i"
            {
                continue;
            }
            let first = e == 0;
            let last = e + 1 == word.elements.len();
            let after_prefix =
                !first && PREFIXES.contains(&fold(&word.elements[e - 1].text).as_str());
            let expected = if first && w == 0 {
                Some((true, TitleCaseRule::FirstWord))
            } else if first && word.starts_segment {
                (style != TitleCaseStyle::Sentence).then_some((true, TitleCaseRule::AfterColon))
            } else if style == TitleCaseStyle::Sentence {
                Some((false, TitleCaseRule::SentenceCase))
            } else if after_prefix {
                None
            } else if last && word.ends_segment {
                Some((true, TitleCaseRule::LastWord))
            } else if first && word.elements.len() > 1 {
                // The first part of a compound is capitalized (`Up-to-Date`).
                Some((true, TitleCaseRule::MajorWord))
            } else {
                match class(&folded, style) {
                    Class::Major => Some((true, TitleCaseRule::MajorWord)),
                    Class::Minor => Some((false, TitleCaseRule::MinorWord)),
                    Class::Ambiguous if before_object && last => {
                        Some((false, TitleCaseRule::MinorWord))
                    }
                    Class::Ambiguous => None,
                }
            };
            if let Some((want, rule)) = expected {
                if want != upper {
                    out.push(TitleCaseMatch {
                        heading: index,
                        span: element.span.clone(),
                        word: element.text.clone(),
                        replacement: recase(&element.text, want),
                        rule,
                    });
                }
            }
        }
    }
}

/// Checks each heading against `options.style`.
pub fn report<S: AsRef<str>>(
    headings: &[S],
    options: &TitleCaseOptions,
) -> Result<TitleCaseReport, StyleError> {
    if options.skip_names.iter().any(|name| name.trim().is_empty()) {
        return Err(StyleError::options(
            "skipNames must not contain empty names",
        ));
    }
    let mut matches = Vec::new();
    let mut flagged_headings = 0;
    for (index, heading) in headings.iter().enumerate() {
        let before = matches.len();
        check(heading.as_ref(), index, options, &mut matches);
        flagged_headings += usize::from(matches.len() > before);
    }
    Ok(TitleCaseReport {
        style: options.style,
        headings: headings.len(),
        flagged_headings,
        matches,
    })
}

/// Parses the bindings' headings argument: a JSON array of strings.
pub fn parse_headings(json: &str) -> Result<Vec<String>, StyleError> {
    serde_json::from_str(json).map_err(|error| {
        StyleError::options(format!("headings must be a JSON array of strings: {error}"))
    })
}

/// Parses the bindings' options argument; `None` selects the defaults.
pub fn parse_options(json: Option<&str>) -> Result<TitleCaseOptions, StyleError> {
    json.map(serde_json::from_str)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(StyleError::options)
}
