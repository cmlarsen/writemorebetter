use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::reports::echoes::fold;
use crate::reports::{word_split, Split};
use crate::{Document, Mode, Span, StyleError, Token};

pub(crate) const REPORT: &str = "hiddenVerbs";

const TABLE: &str = include_str!("../../data/hidden-verbs/nominalizations.tsv");

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE_MAP.get_or_init(|| {
        TABLE
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| line.split_once('\t').expect("hidden verbs TSV"))
            .collect()
    })
}

/// Verbs that carry little meaning of their own before a nominalization: *made a decision*,
/// *gave an explanation*, *reached a conclusion*. *Carry* needs *out* and *come* needs *to*.
const LIGHT_VERBS: [&str; 12] = [
    "make",
    "take",
    "give",
    "have",
    "conduct",
    "perform",
    "reach",
    "offer",
    "provide",
    "undertake",
    "carry",
    "come",
];
/// Words between the light verb and its noun: *a*, *her*, *no*, *quick*, *very*.
const MODIFIERS: [&str; 16] = [
    "a", "an", "the", "no", "some", "any", "another", "such", "his", "her", "its", "their", "our",
    "my", "your", "this",
];
/// Determiners that make *have* a light verb: *had no intention*, *had a discussion*.
const HAVE_FRAMES: [&str; 8] = ["a", "an", "the", "no", "any", "some", "another", "such"];
/// The nouns *come to* makes light: *came to a conclusion*. Before an event noun it is motion:
/// *came to the celebration*.
const COME_TO: [&str; 6] = [
    "agreement",
    "conclusion",
    "decision",
    "determination",
    "realization",
    "recognition",
];
/// Indirect objects: *gave her an explanation*.
const OBJECTS: [&str; 6] = ["me", "him", "her", "us", "them", "you"];
/// Determiners before *the X of*: *the destruction of*, *his arrival at* is not counted.
const DETERMINERS: [&str; 17] = [
    "a", "an", "the", "this", "that", "these", "those", "his", "her", "its", "their", "our", "my",
    "your", "any", "no", "every",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HiddenVerbPattern {
    /// A light verb and its nominalization: *made a decision*, *came to the realization*.
    LightVerb,
    /// A determiner, the nominalization and *of*: *the destruction of*.
    OfPhrase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct HiddenVerbsOptions {
    /// Include `ofPhrase` matches (*the arrival of*), not only light verbs.
    #[serde(alias = "of_phrases")]
    pub of_phrases: bool,
}
impl Default for HiddenVerbsOptions {
    fn default() -> Self {
        Self { of_phrases: true }
    }
}
impl HiddenVerbsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_of_phrases(mut self, value: bool) -> Self {
        self.of_phrases = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HiddenVerb {
    /// From the light verb to the noun, or from the determiner to *of*.
    pub span: Span,
    /// The nominalization's token.
    pub noun: Span,
    /// The light verb's token, for `lightVerb` matches.
    pub light_verb: Option<Span>,
    /// The nominalization's lemma: *decision*.
    pub nominalization: String,
    /// The verb WordNet derives it from: *decide*. Named, never substituted into the text.
    pub term: String,
    pub pattern: HiddenVerbPattern,
    pub sentence: usize,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HiddenVerbTerm {
    pub term: String,
    pub nominalization: String,
    pub count: usize,
    pub narration: usize,
    pub dialogue: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HiddenVerbsGroup {
    pub matches: usize,
    pub words: usize,
    pub per_thousand_words: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HiddenVerbsReport {
    pub matches: Vec<HiddenVerb>,
    /// One row per nominalization found, most frequent first.
    pub terms: Vec<HiddenVerbTerm>,
    pub groups: Split<HiddenVerbsGroup>,
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token.readings.iter().any(|r| r.pos.starts_with(prefix))
}

fn noun_only(token: &Token) -> bool {
    !token.readings.is_empty() && token.readings.iter().all(|r| r.pos.starts_with("NN"))
}

/// The table entry a common-noun reading of `token` names: *decisions* → (*decision*, *decide*).
fn nominalization(token: &Token) -> Option<(&'static str, &'static str)> {
    if !token.is_word || token.is_skipped || token.readings.iter().all(|r| r.pos.starts_with("NNP"))
    {
        return None;
    }
    let surface = fold(&token.text);
    token
        .readings
        .iter()
        .filter(|r| r.pos == "NN" || r.pos == "NNS")
        .map(|r| fold(&r.lemma))
        .chain(std::iter::once(surface).filter(|_| has_pos(token, "NN")))
        .find_map(|lemma| table().get_key_value(lemma.as_str()).map(|(k, v)| (*k, *v)))
}

struct Words<'a> {
    tokens: Vec<&'a Token>,
    folded: Vec<String>,
}

impl Words<'_> {
    fn light_verb(&self, at: usize) -> Option<&'static str> {
        let token = self.tokens[at];
        if token.is_skipped {
            return None;
        }
        LIGHT_VERBS.iter().copied().find(|verb| {
            token
                .readings
                .iter()
                .any(|r| r.pos.starts_with("VB") && fold(&r.lemma) == *verb)
        })
    }

    fn modifier(&self, at: usize) -> bool {
        let token = self.tokens[at];
        token.is_word
            && (MODIFIERS.contains(&self.folded[at].as_str())
                || OBJECTS.contains(&self.folded[at].as_str())
                || has_pos(token, "PRP$")
                || ((has_pos(token, "JJ") || has_pos(token, "RB") || has_pos(token, "CD"))
                    && !noun_only(token)))
    }

    /// *made a quick decision*: the light verb at `at` and the noun after it.
    fn light_frame(&self, at: usize) -> Option<(usize, &'static str, &'static str)> {
        let verb = self.light_verb(at)?;
        let mut next = at + 1;
        match verb {
            "carry" if self.folded.get(next).map(String::as_str) == Some("out") => next += 1,
            "come" if self.folded.get(next).map(String::as_str) == Some("to") => next += 1,
            "carry" | "come" => return None,
            _ => {}
        }
        let first = next;
        while next < self.tokens.len() && next - first < 4 && self.modifier(next) {
            // A modifier that is itself a table noun ends the walk: *made a decision*.
            if nominalization(self.tokens[next]).is_some() && has_pos(self.tokens[next], "NN") {
                break;
            }
            next += 1;
        }
        let (noun, term) = nominalization(*self.tokens.get(next)?)?;
        if verb == "come" && !COME_TO.contains(&noun) {
            return None;
        }
        let after = self.tokens.get(next + 1).copied();
        // A compound's modifier, not its head: *took the examination room*. *Today* and
        // *yesterday* also read as adverbs.
        if after.is_some_and(|t| {
            t.is_word && has_pos(t, "NN") && !has_pos(t, "RB") && !has_pos(t, "IN")
        }) {
            return None;
        }
        // Causative *make* and *have*: *made his isolation more complete*, *have the inquiry
        // hampered*.
        if matches!(verb, "make" | "have")
            && after.is_some_and(|t| {
                matches!(fold(&t.text).as_str(), "more" | "less")
                    || ((has_pos(t, "JJ") || has_pos(t, "VBN"))
                        && !has_pos(t, "IN")
                        && !has_pos(t, "TO"))
            })
        {
            return None;
        }
        // *Have* names a possession unless an article or a negation frames the noun: *had no
        // intention*, *have an argument*, but not *has expectations* or *had your statement*.
        if verb == "have"
            && ((first..next).any(|at| has_pos(self.tokens[at], "PRP$"))
                || !(first..next).any(|at| HAVE_FRAMES.contains(&self.folded[at].as_str())))
        {
            return None;
        }
        // LanguageTool's PLAIN_ENGLISH rule MAKE_DECISIONS_ABOUT already flags "make decision(s)
        // about", for the literal *make* only: *made decisions about* is still reported here.
        if self.folded[at] == "make"
            && noun == "decision"
            && next == first
            && self.folded.get(next + 1).map(String::as_str) == Some("about")
        {
            return None;
        }
        Some((next, noun, term))
    }

    /// *the destruction of*: the determiner before the noun at `at`, which *of* follows.
    fn of_frame(&self, at: usize) -> Option<(usize, &'static str, &'static str)> {
        let (noun, term) = nominalization(self.tokens[at])?;
        // *of course* is an adverb, not the noun's object: *Her arrival of course surprised him*.
        if self.folded.get(at + 1).map(String::as_str) != Some("of")
            || self.folded.get(at + 2).map(String::as_str) == Some("course")
        {
            return None;
        }
        let mut before = at.checked_sub(1)?;
        for _ in 0..2 {
            if DETERMINERS.contains(&self.folded[before].as_str()) {
                break;
            }
            let token = self.tokens[before];
            if !(has_pos(token, "JJ") && !noun_only(token)) {
                return None;
            }
            before = before.checked_sub(1)?;
        }
        DETERMINERS
            .contains(&self.folded[before].as_str())
            .then_some((before, noun, term))
    }
}

pub fn report(
    document: &Document,
    options: &HiddenVerbsOptions,
) -> Result<HiddenVerbsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let mut matches = Vec::new();
    for (index, sentence) in document.sentences().enumerate() {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|token| !token.is_whitespace)
            .collect();
        let folded = tokens.iter().map(|token| fold(&token.text)).collect();
        let words = Words { tokens, folded };
        let mut claimed = 0;
        // A light-verb match claims its noun, so *made the decision of* is reported once.
        for at in 0..words.tokens.len() {
            if at < claimed {
                continue;
            }
            let found = words
                .light_frame(at)
                .map(|(noun, nominalization, term)| {
                    (
                        at,
                        noun,
                        noun,
                        Some(at),
                        nominalization,
                        term,
                        HiddenVerbPattern::LightVerb,
                    )
                })
                .or_else(|| {
                    options
                        .of_phrases
                        .then(|| words.of_frame(at))
                        .flatten()
                        .map(|(first, nominalization, term)| {
                            (
                                first,
                                at + 1,
                                at,
                                None,
                                nominalization,
                                term,
                                HiddenVerbPattern::OfPhrase,
                            )
                        })
                });
            let Some((first, last, noun, verb, nominalization, term, pattern)) = found else {
                continue;
            };
            let mode = document.mode_at(words.tokens[noun].span.byte.start);
            if document.mode_at(words.tokens[first].span.byte.start) != mode {
                continue;
            }
            matches.push(HiddenVerb {
                span: Span::covering(&words.tokens[first].span, &words.tokens[last].span),
                noun: words.tokens[noun].span.clone(),
                light_verb: verb.map(|verb| words.tokens[verb].span.clone()),
                nominalization: nominalization.to_owned(),
                term: term.to_owned(),
                pattern,
                sentence: index,
                mode,
            });
            claimed = last + 1;
        }
    }
    let mut terms: BTreeMap<&str, HiddenVerbTerm> = BTreeMap::new();
    let mut groups: Split<HiddenVerbsGroup> = word_split(document).map(|words| HiddenVerbsGroup {
        words,
        ..Default::default()
    });
    for found in &matches {
        let term = terms
            .entry(found.nominalization.as_str())
            .or_insert_with(|| HiddenVerbTerm {
                term: found.term.clone(),
                nominalization: found.nominalization.clone(),
                count: 0,
                narration: 0,
                dialogue: 0,
            });
        term.count += 1;
        match found.mode {
            Mode::Narration => term.narration += 1,
            Mode::Dialogue => term.dialogue += 1,
        }
        for group in groups.all_and_mode_mut(found.mode) {
            group.matches += 1;
        }
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        group.per_thousand_words =
            (group.words > 0).then(|| 1000.0 * group.matches as f64 / group.words as f64);
    }
    let mut terms: Vec<_> = terms.into_values().collect();
    terms.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.nominalization.cmp(&b.nominalization))
    });
    Ok(HiddenVerbsReport {
        matches,
        terms,
        groups,
    })
}
