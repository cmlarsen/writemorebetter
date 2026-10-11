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

use std::{collections::HashMap, ops::Range};

use regex::Regex as RegexCrate;
use regex_automata::meta::Regex;

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
/// For efficiency this crate compiles these regexes into one regex of the form `before_break(after_break)`
/// and uses the start of the first capture group as the split index.
///
/// Serialization is implemented by hand: the compiled matcher is serialized as
/// the pattern string it was built from, exactly like the previous
/// `serde_regex` representation of `regex::Regex`, so the on-disk format is
/// unchanged. Deserialization rebuilds the matcher with [`build_regex`].
#[derive(Debug, Clone)]
#[non_exhaustive]
struct Rule {
    /// The `before_break(after_break)` pattern the rule was compiled from.
    regex_str: String,
    regex: Regex,
    do_break: bool,
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
        let compiled = build_regex(&regex).map_err(serde::de::Error::custom)?;
        Ok(Rule {
            regex_str: regex,
            regex: compiled,
            do_break,
        })
    }
}

impl Rule {
    /// Gets all byte indices in the text at which this rule matches.
    /// Contrary to the SRX 2.0 spec this does not find overlapping matches.
    fn match_indices<'a>(&'a self, text: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.regex.captures_iter(text).filter_map(|x| {
            // generally it is guaranteed that a regex has
            // at least one match, but be lenient about
            // errors in the srx xml files and drop those without
            x.get_group(1).map(|span| span.start)
        })
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

        'outer: for rule in &self.rules {
            for byte_index in rule.match_indices(text) {
                if byte_index >= text.len() {
                    continue 'outer;
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
                    let regex_str = format!("{}({})", before, after);
                    Rule {
                        regex: build_regex(&regex_str).expect("test rule is valid"),
                        regex_str,
                        do_break: *do_break,
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn match_indices_correct() {
        let rules = rules_for(&[("abc", "d+fg", true)]);

        assert_eq!(
            rules.rules[0]
                .match_indices("abcddfgxxx")
                .collect::<Vec<_>>(),
            vec![3_usize]
        );
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
