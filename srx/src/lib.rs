//! A simple and reasonably fast Rust implementation of the
//! [Segmentation Rules eXchange 2.0 standard](https://www.unicode.org/uli/pas/srx/srx20.html)
//! for text segmentation. `srx` is *not* fully compliant with the standard.
//!
//! This crate is intended for segmentation of plaintext so markup information (`<formathandle>` and `segmentsubflows`)
//! is ignored.
//!
//! Not complying with the SRX spec, overlapping matches of the same `<rule>` are not found which could
//! lead to different behavior in a few edge cases.
//!
//! ## Example
//!
//! ```
//! use std::str::FromStr;
//! use srx::SRX;
//!
//! let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
//! <srx version="2.0">
//!   <header segmentsubflows="no" cascade="no"></header>
//!   <body>
//!     <languagerules>
//!       <languagerule languagerulename="English">
//!         <rule break="no"><beforebreak>\b(?:e\.g|U\.K|Mr)\.\s</beforebreak><afterbreak></afterbreak></rule>
//!         <rule break="yes"><beforebreak>[\.!?]\s</beforebreak><afterbreak>\p{Lu}</afterbreak></rule>
//!       </languagerule>
//!     </languagerules>
//!     <maprules>
//!       <languagemap languagepattern="en.*" languagerulename="English"></languagemap>
//!     </maprules>
//!   </body>
//! </srx>"#;
//!
//! let srx = SRX::from_str(xml)?;
//! let english_rules = srx.language_rules("en");
//!
//! assert_eq!(
//!     english_rules.split("e.g. U.K. and Mr. do not split. SRX is a rule-based format.").collect::<Vec<_>>(),
//!     vec!["e.g. U.K. and Mr. do not split. ", "SRX is a rule-based format."]
//! );
//! # Ok::<(), srx::Error>(())
//! ```
//!
//! ## Features
//!
//! - `serde`: Serde serialization and deserialization support for [SRX].
//! - `from_xml`: [SRX::from_reader] method and [std::str::FromStr] implementation to load from an XML file in SRX format.
//!
//! ## A note on regular expressions
//!
//! This crate uses the [`regex` crate](https://github.com/rust-lang/regex) for parsing and executing
//! regular expressions. The `regex` crate is mostly compatible with the
//! [regular expression standard](https://www.unicode.org/uli/pas/srx/srx20.html#Intro_RegExp) from the SRX specification.
//! However, some metacharacters such as `\Q` and `\E` are not supported.
//!
//! To still be able to use files containing unsupported rules and to parse useful SRX files
//! such as
//! [`segment.srx` from LanguageTool](https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/resources/org/languagetool/resource/segment.srx)
//! which does not comply with the standard by e. g. using look-ahead and look-behind, `srx`
//! ignores `<rule>` elements with invalid regular expressions and provides information about
//! them via the [SRX::errors] function.
#![cfg_attr(docsrs, feature(doc_cfg))] // see https://stackoverflow.com/a/61417700
#[cfg(feature = "serde")]
extern crate serde_crate as serde;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use std::{
    collections::HashMap,
    ops::Range,
    sync::{Arc, OnceLock},
};

use regex::Regex as RegexCrate;
use regex_automata::{meta::Regex, Anchored, Input, MatchKind, PatternID};

const AFTER_BREAK_GROUP: &str = "srx_after_break";

/// Joins a rule's `before_break` and `after_break` into the single pattern the rule is compiled from.
pub(crate) fn rule_pattern(before_break: &str, after_break: &str) -> String {
    format!("(?:{before_break})(?P<{AFTER_BREAK_GROUP}>{after_break})")
}

/// Splits a pattern built by [`rule_pattern`] back into `before_break` and `after_break`.
fn split_rule_pattern(pattern: &str) -> Option<(&str, &str)> {
    let marker = format!(")(?P<{AFTER_BREAK_GROUP}>");
    let index = pattern.rfind(&marker)?;
    let before_break = pattern.strip_prefix("(?:")?.get(..index - 3)?;
    let after_break = pattern[index + marker.len()..].strip_suffix(')')?;
    Some((before_break, after_break))
}

/// Builds the matcher for a `before_break(after_break)` rule pattern.
///
/// The one-pass DFA is disabled: it is only a specialization used to speed up
/// capture searches, and the meta engine falls back to the bounded backtracker
/// with identical match results. Building the ~85 English rules with it enabled
/// allocates several megabytes of one-pass programs that dwarf the rest of the
/// rule set, so disabling it cuts the loaded rule size roughly threefold.
pub(crate) fn build_regex(pattern: &str) -> Result<Regex, regex_automata::meta::BuildError> {
    Regex::builder()
        .configure(Regex::config().onepass(false))
        .build(pattern)
}

fn build_fancy_regex(pattern: &str) -> Result<fancy_regex::Regex, fancy_regex::Error> {
    fancy_regex::RegexBuilder::new(pattern)
        .backtrack_limit(usize::MAX)
        .build()
}

#[cfg(feature = "from_xml")]
mod from_xml;
#[cfg(feature = "from_xml")]
mod utils;
#[cfg(feature = "from_xml")]
pub use from_xml::Error;

/// Newtype denoting a language (`languagerulename` attribute in SRX).
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Language(pub String);

/// A single SRX rule. In SRX, consists of one `before_break` and one `after_break` Regex.
/// For efficiency this crate compiles these regexes into one regex of the form
/// `(?:before_break)(?P<srx_after_break>after_break)` and uses the start of the named group as the split index.
/// Patterns serialized before the group was named are of the form `before_break(after_break)`;
/// for those the first capture group is used.
///
/// Serialization is implemented by hand: the compiled matcher is serialized as
/// the pattern string it was built from, exactly like the previous
/// `serde_regex` representation of `regex::Regex`, so the on-disk format is
/// unchanged. Deserialization rebuilds the matcher with [`Rule::from_pattern`].
#[derive(Debug, Clone)]
#[non_exhaustive]
struct Rule {
    /// The `(?:before_break)(?P<srx_after_break>after_break)` pattern the rule was compiled from.
    regex_str: String,
    regex: Matcher,
    split_group: usize,
    /// Built on first use, since most `break="no"` rules never match.
    exception: OnceLock<Option<Exception>>,
    do_break: bool,
}

/// How a `break="no"` rule is checked at a position, like the Java SRX implementation
/// LanguageTool uses: it suppresses a break at every index where some match of `before_break`
/// ends and `after_break` matches. One search of the combined pattern yields only one split per
/// match start, so `["”'’]\s*` + `\s*\p{Ll}` would miss the index after the first of two spaces.
#[derive(Debug, Clone)]
struct Exception {
    /// `before_break` with [`MatchKind::All`], so an anchored search finds its longest match.
    before_break: Regex,
    /// `after_break`, or `None` when it is empty.
    after_break: Option<Regex>,
}

impl Exception {
    fn new(pattern: &str) -> Option<Self> {
        let (before_break, after_break) = split_rule_pattern(pattern)?;
        let before_break = Regex::builder()
            .configure(
                Regex::config()
                    .onepass(false)
                    .hybrid(false)
                    .dfa(false)
                    .match_kind(MatchKind::All),
            )
            .build(before_break)
            .ok()?;
        let after_break = if after_break.is_empty() {
            None
        } else {
            Some(
                Regex::builder()
                    .configure(Regex::config().onepass(false).hybrid(false).dfa(false))
                    .build(after_break)
                    .ok()?,
            )
        };
        Some(Exception {
            before_break,
            after_break,
        })
    }

    /// Pushes every index at which a match of `before_break` starting at `start` ends and
    /// `after_break` matches.
    fn split_indices(&self, text: &str, start: usize, out: &mut Vec<usize>) {
        let mut end = text.len();
        while let Some(found) = self
            .before_break
            .search(&Input::new(text).range(start..end).anchored(Anchored::Yes))
        {
            let split = found.end();
            let after_matches = self.after_break.as_ref().is_none_or(|after_break| {
                after_break.is_match(Input::new(text).range(split..).anchored(Anchored::Yes))
            });
            if after_matches {
                out.push(split);
            }
            if split == start {
                break;
            }
            end = split - 1;
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for Rule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        // Field order and types match the derived `regex::Regex` + `do_break`
        // representation: a string, then a bool.
        let mut state = serializer.serialize_struct("Rule", 2)?;
        state.serialize_field("regex", &self.regex_str)?;
        state.serialize_field("do_break", &self.do_break)?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(crate = "serde_crate")]
        struct Repr {
            regex: String,
            do_break: bool,
        }

        let Repr { regex, do_break } = Repr::deserialize(deserializer)?;
        Rule::from_pattern(regex, do_break).map_err(serde::de::Error::custom)
    }
}

/// The compiled form of a rule pattern. SRX patterns are Java regexes; the few that use
/// look-around, which `regex-automata` does not support, are compiled with `fancy-regex`.
#[derive(Debug, Clone)]
enum Matcher {
    Automata(Regex),
    Fancy(Arc<fancy_regex::Regex>),
}

impl Rule {
    fn from_pattern(
        regex_str: String,
        do_break: bool,
    ) -> Result<Self, Box<regex_automata::meta::BuildError>> {
        let (regex, split_group) = match build_regex(&regex_str) {
            Ok(regex) => {
                let split_group = regex
                    .group_info()
                    .to_index(PatternID::ZERO, AFTER_BREAK_GROUP)
                    .unwrap_or(1);
                (Matcher::Automata(regex), split_group)
            }
            Err(error) => {
                let regex = build_fancy_regex(&regex_str).map_err(|_| Box::new(error))?;
                let split_group = regex
                    .capture_names()
                    .position(|name| name == Some(AFTER_BREAK_GROUP))
                    .unwrap_or(1);
                (Matcher::Fancy(Arc::new(regex)), split_group)
            }
        };

        Ok(Rule {
            regex_str,
            regex,
            split_group,
            exception: OnceLock::new(),
            do_break,
        })
    }

    fn exception(&self) -> Option<&Exception> {
        match (&self.regex, self.do_break) {
            (Matcher::Automata(_), false) => self
                .exception
                .get_or_init(|| Exception::new(&self.regex_str))
                .as_ref(),
            _ => None,
        }
    }

    /// Gets all byte indices in the text at which this rule matches.
    /// Matches may overlap: the search for the next match resumes one character after the start of
    /// the previous one. For a `break="no"` rule every split of a match is found (see [`Exception`]).
    fn match_indices(&self, text: &str) -> Vec<usize> {
        let mut indices = Vec::new();
        let mut captures = match &self.regex {
            Matcher::Automata(regex) => Some(regex.create_captures()),
            Matcher::Fancy(_) => None,
        };
        let mut start = 0;

        while start <= text.len() {
            let (found_start, split) = match (&self.regex, captures.as_mut()) {
                (Matcher::Automata(regex), Some(captures)) => {
                    regex.search_captures(&Input::new(text).range(start..), captures);
                    let Some(found) = captures.get_match() else {
                        break;
                    };
                    let split = captures.get_group(self.split_group).map(|span| span.start);
                    (found.start(), split)
                }
                (Matcher::Fancy(regex), _) => {
                    let captures = match regex.captures_from_pos(text, start) {
                        Ok(Some(captures)) => captures,
                        Ok(None) | Err(_) => break,
                    };
                    let Some(found) = captures.get(0) else {
                        break;
                    };
                    let split = captures.get(self.split_group).map(|span| span.start());
                    (found.start(), split)
                }
                (Matcher::Automata(_), None) => break,
            };

            start = found_start + text[found_start..].chars().next().map_or(1, char::len_utf8);

            // generally it is guaranteed that a regex has
            // at least one match, but be lenient about
            // errors in the srx xml files and drop those without
            if let Some(split) = split {
                match self.exception() {
                    Some(exception) => exception.split_indices(text, found_start, &mut indices),
                    None => indices.push(split),
                }
            }
        }

        indices
    }

    /// Whether this rule breaks or prevents breaking.
    fn do_break(&self) -> bool {
        self.do_break
    }
}

/// An ordered set of rules.
/// Rules are executed in order.
/// Once a rule matches on an index, no other rule can match at the same index.
/// Each rule either breaks (i. e. splits the text at this index) or prevents breaking.
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
#[derive(Debug, Clone, Default)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// Obtain the ranges for text segments. Guaranteed to be at character bounds.
    pub fn split_ranges(&self, text: &str) -> Vec<Range<usize>> {
        let mut segments = Vec::new();

        // TODO use a proper tri-state enum here
        let mut masked_bytes: Vec<Option<bool>> = vec![None; text.len()];

        for rule in &self.rules {
            for byte_index in rule.match_indices(text) {
                if byte_index >= text.len() {
                    continue;
                }

                if masked_bytes[byte_index].is_none() {
                    masked_bytes[byte_index] = Some(rule.do_break());
                }
            }
        }

        let mut prev_byte_pos = 0;

        // Iterate over characters, we don't want no half characters in the output ranges
        for (byte_pos, _c) in text.char_indices() {
            if let Some(true) = masked_bytes[byte_pos] {
                segments.push(prev_byte_pos..byte_pos);
                prev_byte_pos = byte_pos;
            }
        }

        // Deal with the trailing element, which is by definition
        // not required to be suffixed by a gap char.
        if text[prev_byte_pos..].chars().next().is_some() {
            segments.push(prev_byte_pos..text.len());
        }

        segments
    }

    /// Split text into segments.
    pub fn split<'a, 'b>(&self, text: &'a str) -> impl Iterator<Item = &'a str> + 'b
    where
        'a: 'b,
    {
        self.split_ranges(text)
            .into_iter()
            .map(move |range| &text[range])
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

/// An entry of the `<maprules>` element.
/// Associates a regex with a [Language].
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
#[derive(Debug, Clone)]
struct LanguageRegex {
    #[cfg_attr(feature = "serde", serde(with = "serde_regex"))]
    regex: RegexCrate,
    language: Language,
}

/// The SRX root.
/// Does not execute rules on is own.
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
#[derive(Debug, Clone)]
pub struct SRX {
    cascade: bool,
    map: Vec<LanguageRegex>,
    rules: HashMap<Language, Vec<Rule>>,
    errors: HashMap<Language, Vec<String>>,
}

impl SRX {
    /// Gets the rules for a language code by
    /// - aggregating rules from all [Language]s with a matching `<languagepattern>` (if the SRX is set to be cascading)
    /// - finding the first matching `<languagepattern>` (if the SRX is set to be not cascading)
    ///
    /// Result should be cached instead of calling this repeatedly as it clones the rules.
    pub fn language_rules<S: AsRef<str>>(&self, lang_code: S) -> Rules {
        let mut rules = Vec::new();

        for item in &self.map {
            if item.regex.is_match(lang_code.as_ref()) {
                rules.extend(self.rules.get(&item.language).expect("languagerulename in <languagemap> must have a corresponding entry in <languagerules>").iter().cloned());
                if !self.cascade {
                    break;
                }
            }
        }

        Rules { rules }
    }

    /// Maps [Language]s to a vector of string representations of errors which occured during parsing regular expressions for this language.
    pub fn errors(&self) -> &HashMap<Language, Vec<String>> {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a [`Rules`] set from `(before_break, after_break, do_break)` triples.
    fn rules_for(patterns: &[(&str, &str, bool)]) -> Rules {
        Rules {
            rules: patterns
                .iter()
                .map(|(before, after, do_break)| {
                    Rule::from_pattern(rule_pattern(before, after), *do_break)
                        .expect("test rule is valid")
                })
                .collect(),
        }
    }

    #[test]
    fn match_indices_correct() {
        let rules = rules_for(&[("abc", "d+fg", true)]);

        assert_eq!(rules.rules[0].match_indices("abcddfgxxx"), vec![3_usize]);
    }

    #[test]
    fn example_splits_correct() {
        // Reproduces the SRX spec example (https://www.unicode.org/uli/pas/srx/srx20.html#AppExample)
        // with the no-break rules ordered before the break rule.
        let rules = rules_for(&[
            (r"\sMr\.", r"\s", false),
            (r"\sU\.K\.", r"\s", false),
            (r"[\.\?!]+", r"\s", true),
        ]);

        let text =
            "The U.K. Prime Minister, Mr. Blair, was seen out with his family today. He is well.";
        assert_eq!(
            rules.split(text).collect::<Vec<_>>(),
            vec![
                "The U.K. Prime Minister, Mr. Blair, was seen out with his family today.",
                " He is well."
            ]
        );
    }

    #[test]
    fn before_break_groups_do_not_move_the_split() {
        let rules = rules_for(&[(r"\b(Mr|Dr)\.\s", "", false), (r"[\.!?]\s", "", true)]);

        assert_eq!(
            rules
                .split("Mr. Bush left. Dr. Doe stayed.")
                .collect::<Vec<_>>(),
            vec!["Mr. Bush left. ", "Dr. Doe stayed."]
        );
    }

    #[test]
    fn matches_may_overlap() {
        let rules = rules_for(&[
            (r"[\.\s]\p{L}{1,2}\.\s", r"[\p{N}\p{Ll}]", false),
            (r"[\.!?\u{2026}][\u{201D}]*\s", "", true),
        ]);

        assert_eq!(
            rules
                .split("Thelen et. al. 2001 said\u{2026} \u{201c}It\u{2019}s fine.\u{201d} Then\u{2014}gone.")
                .collect::<Vec<_>>(),
            vec![
                "Thelen et. al. 2001 said\u{2026} ",
                "\u{201c}It\u{2019}s fine.\u{201d} ",
                "Then\u{2014}gone."
            ]
        );
    }

    #[test]
    fn look_around_rules_are_supported() {
        let rules = rules_for(&[
            (r"\s(?!(on|it))\p{L}{1,2}\.\s", r"\p{Ll}", false),
            (r"\.\s", "", true),
        ]);

        assert_eq!(
            rules
                .split("Wait 2 hr. or so\u{2014}it\u{2019}s fine. Go on. now\u{2026}")
                .collect::<Vec<_>>(),
            vec![
                "Wait 2 hr. or so\u{2014}it\u{2019}s fine. ",
                "Go on. ",
                "now\u{2026}"
            ]
        );
    }

    #[test]
    fn look_around_rules_scan_long_text_without_a_match() {
        let rules = rules_for(&[(
            r"[\.\s ](?!(on|it))\p{L}{1,2}\.[\s ]",
            r"[\p{N}\p{Ll}]",
            false,
        )]);
        let filler = "\u{201c}She walked to the door\u{2026}\u{201d} he said\u{2014}then left. "
            .repeat(30_000);
        let text = format!("{filler}Wait 2 hr. or so");
        let Matcher::Fancy(regex) = &rules.rules[0].regex else {
            panic!("look-around rule must compile with fancy-regex");
        };

        let found = regex
            .captures_from_pos(&text, 0)
            .expect("a long scan must not hit the backtrack limit")
            .and_then(|captures| captures.get(0))
            .map(|found| found.start());

        assert_eq!(found, Some(filler.len() + "Wait 2".len()));
        assert_eq!(
            rules.rules[0].match_indices(&text),
            vec![filler.len() + "Wait 2 hr. ".len()]
        );
    }

    #[test]
    fn look_around_rule_errors_stop_the_scan() {
        let regex_str = rule_pattern(r"\s(?!(on|it))\p{L}{1,2}\.\s", r"\p{Ll}");
        let regex = fancy_regex::RegexBuilder::new(&regex_str)
            .backtrack_limit(1)
            .build()
            .expect("test rule is valid");
        let mut rule = Rule::from_pattern(regex_str, false).expect("test rule is valid");
        rule.regex = Matcher::Fancy(Arc::new(regex));
        let text = "\u{201c}Wait\u{2026}\u{201d} she said\u{2014}2 hr. or so. ".repeat(200);

        assert_eq!(rule.match_indices(&text), Vec::<usize>::new());
    }

    #[test]
    fn no_break_rules_hold_at_every_split_of_a_match() {
        let rules = rules_for(&[
            (r#"["”'’][\s\u00A0]*"#, r"[\s\u00A0]*\p{Ll}", false),
            (r"[\.!?…][”’]*\s", "", true),
        ]);

        assert_eq!(
            rules
                .split("\u{201c}Wait!\u{201d}  she\u{2019}d cried\u{2028}then stopped\u{2014}gone\u{2026} \u{201c}Go.\u{201d}  He went.")
                .collect::<Vec<_>>(),
            vec![
                "\u{201c}Wait!\u{201d}  she\u{2019}d cried\u{2028}then stopped\u{2014}gone\u{2026} ",
                "\u{201c}Go.\u{201d} ",
                " He went."
            ]
        );
    }

    #[test]
    fn split_rule_pattern_inverts_rule_pattern() {
        assert_eq!(
            split_rule_pattern(&rule_pattern(r"\b(Mr|Dr)\.\s", r"(?:\p{Lu})")),
            Some((r"\b(Mr|Dr)\.\s", r"(?:\p{Lu})"))
        );
        assert_eq!(split_rule_pattern(&rule_pattern("", "")), Some(("", "")));
        assert_eq!(split_rule_pattern(r"\b(Mr|Dr)\.(\s)"), None);
    }

    #[test]
    fn split_is_lossless_for_arbitrary_text() {
        // The rules are deliberately unrelated to the text; the invariant is only
        // that concatenating the segments reproduces the input.
        let rules = rules_for(&[
            (r"\be\.g\.\s", "", false),
            (r"[\.!?…][\u{00BB}\u{2019}\u{201D}]*\s", "", true),
        ]);
        for text in [
            "e.g. U.K. and Mr. do not split. SRX is a 👒🍏🍱-based format 🐱",
            "one. two? three! four…",
            "no terminator here",
            "",
        ] {
            assert_eq!(
                text.len(),
                rules.split(text).fold(0, |acc, x| acc + x.len()),
                "segments must partition the input for {:?}",
                text
            );
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip_is_stable() {
        let rules = rules_for(&[("abc", "d+fg", true), (r"\bwww\.", r"\w", false)]);

        let buf = bincode::serialize(&rules).expect("serialize");
        let back: Rules = bincode::deserialize(&buf).expect("deserialize");
        assert_eq!(back.len(), 2);
        assert_eq!(bincode::serialize(&back).expect("reserialize"), buf);
    }
}
