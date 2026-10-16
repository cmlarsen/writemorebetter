use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{PercentileBand, Share};
use crate::error::{percent_in_range, StyleError};
use crate::{Document, Span, Token};

pub(crate) const REPORT: &str = "dialogueTags";

const SUBJECT_PRONOUNS: &[&str] = &["i", "you", "he", "she", "it", "we", "they", "one"];
const DETERMINERS: &[&str] = &[
    "the", "a", "an", "his", "her", "my", "your", "our", "their", "its", "this", "that", "these",
    "those", "each", "every", "another", "some",
];
const AUXILIARIES: &[&str] = &["be", "have", "do"];
const PARTICLES: &[&str] = &["on", "in", "out", "up"];
const SPEECH_VERBS: &str = include_str!("../../data/dialogue/speech-verbs.txt");
const NON_SPEECH_VERBS: &str = include_str!("../../data/dialogue/non-speech-verbs.txt");
const ADVERB_EXCLUSIONS: &str = include_str!("../../data/dialogue/adverb-exclusions.txt");
const OBJECT_VERBS: &str = include_str!("../../data/dialogue/object-verbs.txt");
const OBJECT_PRONOUNS: &[&str] = &["me", "him", "her", "us", "them", "you", "it"];
const MAX_SUBJECT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DialogueTagPosition {
    Before,
    After,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum DialogueTagVerbClass {
    Speech,
    NonSpeech,
    Unlisted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTag {
    pub span: Span,
    pub clause: Span,
    pub verb: Span,
    pub verb_text: String,
    pub lemma: String,
    pub class: DialogueTagVerbClass,
    pub showy: bool,
    pub position: DialogueTagPosition,
    pub adverb: Option<Span>,
    pub quotes: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagActionBeat {
    pub span: Span,
    pub quotes: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagUntaggedRun {
    pub span: Span,
    pub quotes: Vec<Span>,
    pub exchanges: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagVerbCount {
    pub lemma: String,
    pub class: DialogueTagVerbClass,
    pub count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagBaseline {
    #[serde(alias = "said_asked_percent")]
    pub said_asked_percent: Option<f64>,
    #[serde(alias = "adverb_percent")]
    pub adverb_percent: Option<f64>,
    #[serde(alias = "non_speech_percent")]
    pub non_speech_percent: Option<f64>,
    pub said_asked_band: Option<PercentileBand>,
    pub adverb_band: Option<PercentileBand>,
    pub non_speech_band: Option<PercentileBand>,
}

impl DialogueTagBaseline {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_said_asked_percent(mut self, percent: f64) -> Self {
        self.said_asked_percent = Some(percent);
        self
    }
    pub fn with_adverb_percent(mut self, percent: f64) -> Self {
        self.adverb_percent = Some(percent);
        self
    }
    pub fn with_non_speech_percent(mut self, percent: f64) -> Self {
        self.non_speech_percent = Some(percent);
        self
    }

    pub fn bundled() -> Option<Self> {
        None
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        for band in [
            &self.said_asked_band,
            &self.adverb_band,
            &self.non_speech_band,
        ]
        .into_iter()
        .flatten()
        {
            band.validate(REPORT, 100.0)?;
        }
        for (name, value) in [
            ("saidAskedPercent", self.said_asked_percent),
            ("adverbPercent", self.adverb_percent),
            ("nonSpeechPercent", self.non_speech_percent),
        ] {
            if let Some(value) = value {
                percent_in_range(REPORT, name, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagOptions {
    #[serde(alias = "max_untagged_exchanges")]
    pub max_untagged_exchanges: usize,
    #[serde(alias = "minimum_tags")]
    pub minimum_tags: usize,
}

impl Default for DialogueTagOptions {
    fn default() -> Self {
        Self {
            max_untagged_exchanges: 12,
            minimum_tags: 15,
        }
    }
}

impl DialogueTagOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_max_untagged_exchanges(mut self, exchanges: usize) -> Self {
        self.max_untagged_exchanges = exchanges;
        self
    }
    pub fn with_minimum_tags(mut self, tags: usize) -> Self {
        self.minimum_tags = tags;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DialogueTagReport {
    pub tags: Vec<DialogueTag>,
    pub beats: Vec<DialogueTagActionBeat>,
    pub untagged_runs: Vec<DialogueTagUntaggedRun>,
    pub other_verbs: Vec<DialogueTagVerbCount>,
    pub neutral_verbs: Vec<DialogueTagVerbCount>,
    pub showy_verbs: Vec<DialogueTagVerbCount>,
    pub low_confidence: bool,
    pub dialogue_spans: usize,
    pub speech_spans: usize,
    pub tagged_spans: usize,
    pub exchanges: usize,
    pub untagged_exchanges: usize,
    pub said_asked: Share,
    pub with_adverb: Share,
    pub non_speech: Share,
}

struct Lists {
    speech: BTreeSet<&'static str>,
    non_speech: BTreeSet<&'static str>,
    adverb_exclusions: BTreeSet<&'static str>,
    object: BTreeSet<&'static str>,
    neutral: BTreeSet<&'static str>,
    showy: BTreeSet<&'static str>,
}

fn lists() -> &'static Lists {
    static LISTS: OnceLock<Lists> = OnceLock::new();
    LISTS.get_or_init(|| {
        let parse = |data: &'static str| -> BTreeSet<&'static str> {
            data.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect()
        };
        Lists {
            speech: parse(SPEECH_VERBS)
                .union(&parse(include_str!("../../data/dialogue/showy-verbs.txt")))
                .copied()
                .collect(),
            non_speech: parse(NON_SPEECH_VERBS),
            adverb_exclusions: parse(ADVERB_EXCLUSIONS),
            object: parse(OBJECT_VERBS),
            neutral: parse(include_str!("../../data/dialogue/neutral-verbs.txt")),
            showy: parse(include_str!("../../data/dialogue/showy-verbs.txt")),
        }
    })
}

pub fn speech_verbs() -> impl Iterator<Item = &'static str> {
    lists().speech.iter().copied()
}

pub fn non_speech_verbs() -> impl Iterator<Item = &'static str> {
    lists().non_speech.iter().copied()
}

#[derive(Clone, Copy)]
struct Item<'a> {
    token: &'a Token,
    sentence: usize,
}

struct Piece {
    dialogue: usize,
    tokens: Range<usize>,
    starts: bool,
    ends: bool,
    last: Option<char>,
}

struct Parse {
    verb: usize,
    lemma: String,
    class: DialogueTagVerbClass,
    adverb: Option<usize>,
    extent: Range<usize>,
}

struct Paragraph<'a> {
    items: Vec<Item<'a>>,
    pieces: Vec<Piece>,
}

fn lower(token: &Token) -> String {
    token.text.to_lowercase()
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token
        .readings
        .iter()
        .any(|reading| reading.pos.starts_with(prefix))
}

fn is_verb(token: &Token) -> bool {
    token.is_word && !token.is_skipped && has_pos(token, "VB")
}

fn is_finite(token: &Token) -> bool {
    token.is_word
        && !token.is_skipped
        && token
            .readings
            .iter()
            .any(|reading| matches!(reading.pos.as_str(), "VBD" | "VBZ" | "VBP" | "MD"))
}

fn is_pronoun(token: &Token) -> bool {
    SUBJECT_PRONOUNS.contains(&lower(token).as_str())
}

fn is_determiner(token: &Token) -> bool {
    DETERMINERS.contains(&lower(token).as_str())
}

fn is_name(token: &Token) -> bool {
    token.is_skipped
        || (token.is_word && token.text.chars().next().is_some_and(char::is_uppercase))
        || has_pos(token, "NNP")
}

fn is_nominal(token: &Token) -> bool {
    token.is_word
        && (has_pos(token, "NN") || has_pos(token, "JJ") || has_pos(token, "VBG") || is_name(token))
}

fn is_adverb(token: &Token) -> bool {
    let text = lower(token);
    token.is_word
        && !token.is_skipped
        && text.ends_with("ly")
        && has_pos(token, "RB")
        && !lists().adverb_exclusions.contains(text.as_str())
}

fn is_modifier(token: &Token) -> bool {
    is_adverb(token) || (token.is_word && has_pos(token, "RB"))
}

fn is_auxiliary(token: &Token) -> bool {
    has_pos(token, "MD")
        || token.readings.iter().any(|reading| {
            reading.pos.starts_with("VB") && AUXILIARIES.contains(&reading.lemma.as_str())
        })
}

fn is_clitic(token: &Token) -> bool {
    matches!(
        token.text.replace('’', "'").as_str(),
        "'d" | "'s" | "'ll" | "'ve" | "'re" | "'m"
    )
}

fn is_possessive(token: &Token) -> bool {
    token.text.replace('’', "'").to_lowercase() == "'s"
}

fn is_person(token: &Token) -> bool {
    is_pronoun(token)
        || token.is_skipped
        || (is_name(token) && !is_determiner(token) && !has_pos(token, "RB"))
}

fn takes_object(lemma: &str) -> bool {
    lists().object.contains(lemma)
}

pub(crate) fn lead_in(token: &Token) -> bool {
    is_verb(token) && {
        let (lemma, class, _) = verb_lemma(token, None);
        class != DialogueTagVerbClass::Unlisted && !takes_object(&lemma)
    }
}

fn stops(token: &Token) -> bool {
    !token.text.is_empty() && token.text.chars().all(|ch| matches!(ch, '!' | '?'))
}

fn ends_sentence(token: &Token) -> bool {
    !token.text.is_empty()
        && token
            .text
            .chars()
            .all(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

fn verb_lemma(token: &Token, next: Option<&Token>) -> (String, DialogueTagVerbClass, bool) {
    let lists = lists();
    let particle = next
        .filter(|next| next.is_word)
        .map(lower)
        .filter(|word| PARTICLES.contains(&word.as_str()));
    let lemmas: BTreeSet<String> = token
        .readings
        .iter()
        .filter(|reading| reading.pos.starts_with("VB"))
        .map(|reading| reading.lemma.to_lowercase())
        .collect();
    for (set, class) in [
        (&lists.speech, DialogueTagVerbClass::Speech),
        (&lists.non_speech, DialogueTagVerbClass::NonSpeech),
    ] {
        if let Some(particle) = &particle {
            if let Some(lemma) = lemmas
                .iter()
                .map(|lemma| format!("{lemma} {particle}"))
                .find(|phrase| set.contains(phrase.as_str()))
            {
                return (lemma, class, true);
            }
        }
        if let Some(lemma) = lemmas.iter().find(|lemma| set.contains(lemma.as_str())) {
            return (lemma.clone(), class, false);
        }
    }
    let lemma = lemmas.into_iter().next().unwrap_or_else(|| lower(token));
    (lemma, DialogueTagVerbClass::Unlisted, false)
}

fn listed(token: &Token, next: Option<&Token>) -> bool {
    is_verb(token) && verb_lemma(token, next).1 != DialogueTagVerbClass::Unlisted
}

pub(crate) fn following_tag(tokens: &[&Token], bare: bool) -> bool {
    let items = tokens
        .iter()
        .enumerate()
        .take_while(|(i, _)| *i == 0 || !ends_sentence(tokens[i - 1]))
        .map(|(_, token)| Item { token, sentence: 0 })
        .collect::<Vec<_>>();
    let paragraph = Paragraph {
        pieces: Vec::new(),
        items,
    };
    !paragraph.items.is_empty()
        && paragraph
            .after_tag(&(0..paragraph.items.len()))
            .is_some_and(|p| {
                p.class != DialogueTagVerbClass::Unlisted
                    && p.lemma != "reflect"
                    && (!bare || paragraph.bare(&(0..paragraph.items.len()), p.extent.end))
            })
}

fn span_of(items: &[Item], range: Range<usize>) -> Span {
    Span::covering(
        &items[range.start].token.span,
        &items[range.end - 1].token.span,
    )
}

fn first_word(items: &[Item], clause: &Range<usize>) -> usize {
    clause
        .clone()
        .find(|&index| items[index].token.is_word)
        .unwrap_or(clause.start)
}

impl<'a> Paragraph<'a> {
    fn word(&self, index: usize) -> Option<&'a Token> {
        self.items
            .get(index)
            .map(|item| item.token)
            .filter(|token| token.is_word)
    }

    fn next_word(&self, index: usize, end: usize) -> Option<&'a Token> {
        self.items[index + 1..end]
            .iter()
            .map(|item| item.token)
            .find(|token| token.is_word)
    }

    fn words(&self, range: &Range<usize>) -> Vec<usize> {
        range
            .clone()
            .filter(|&index| self.items[index].token.is_word)
            .collect()
    }

    fn head(&self, range: &Range<usize>) -> Range<usize> {
        let sentence = self.items[range.start].sentence;
        let mut end = range.start;
        while end < range.end && self.items[end].sentence == sentence {
            end += 1;
            if stops(self.items[end - 1].token) {
                break;
            }
        }
        range.start..end
    }

    fn tail(&self, range: &Range<usize>) -> Range<usize> {
        let sentence = self.items[range.end - 1].sentence;
        let mut start = range.end;
        while start > range.start {
            let previous = start - 1;
            if self.items[previous].sentence != sentence
                || (previous != range.end - 1 && stops(self.items[previous].token))
            {
                break;
            }
            start = previous;
        }
        start..range.end
    }

    fn last_text(&self, range: &Range<usize>) -> &str {
        &self.items[range.end - 1].token.text
    }

    fn subject(&self, words: &[usize], start: usize, length: usize) -> bool {
        if self.items[words[start]..=words[start + length - 1]]
            .iter()
            .any(|item| matches!(item.token.text.as_str(), "," | ";" | ":" | "—"))
        {
            return false;
        }
        let tokens: Vec<&Token> = words[start..start + length]
            .iter()
            .map(|&index| self.items[index].token)
            .collect();
        let first = tokens[0];
        if is_pronoun(first) {
            return length == 1;
        }
        (is_determiner(first) || is_name(first) || is_nominal(first))
            && !is_possessive(tokens[length - 1])
            && !is_determiner(tokens[length - 1])
            && tokens[1..].iter().all(|token| {
                is_possessive(token)
                    || (is_nominal(token) && !is_pronoun(token) && !is_determiner(token))
            })
    }

    fn verb_parse(&self, index: usize, clause_end: usize) -> (String, DialogueTagVerbClass, bool) {
        verb_lemma(self.items[index].token, self.next_word(index, clause_end))
    }

    fn adverb_at(&self, index: Option<&usize>) -> Option<usize> {
        index
            .copied()
            .filter(|&index| is_adverb(self.items[index].token))
    }

    fn manner_after(&self, words: &[usize], position: usize) -> Option<usize> {
        let mut position = position;
        while let Some(&index) = words.get(position) {
            if matches!(
                lower(self.items[index].token).as_str(),
                "very" | "quite" | "rather" | "so" | "too" | "more" | "most" | "almost"
            ) {
                position += 1;
            } else {
                break;
            }
        }
        self.adverb_at(words.get(position))
    }

    fn adverb_after_object(&self, words: &[usize], position: usize) -> Option<usize> {
        let object = words.get(position).map(|&index| self.items[index].token)?;
        let length = if OBJECT_PRONOUNS.contains(&lower(object).as_str()) {
            1
        } else {
            words[position..]
                .iter()
                .take(2)
                .take_while(|&&index| {
                    let token = self.items[index].token;
                    is_name(token) && !is_pronoun(token) && !is_adverb(token)
                })
                .count()
        };
        (length > 0)
            .then(|| self.manner_after(words, position + length))
            .flatten()
    }

    fn bare(&self, clause: &Range<usize>, end: usize) -> bool {
        for item in &self.items[end..clause.end] {
            let token = item.token;
            if !token.is_word {
                if token.is_punctuation {
                    return true;
                }
                continue;
            }
            if has_pos(token, "IN") && !matches!(lower(token).as_str(), "that" | "to") {
                return true;
            }
            if !(is_adverb(token)
                || lower(token) == "to"
                || OBJECT_PRONOUNS.contains(&lower(token).as_str())
                || is_name(token))
            {
                return false;
            }
        }
        true
    }

    fn after_tag(&self, clause: &Range<usize>) -> Option<Parse> {
        let words = self.words(clause);
        let mut offset = 0;
        if words.len() > 1 {
            let first = self.items[words[0]].token;
            if has_pos(first, "RB") && !has_pos(first, "NN") && !is_verb(first) {
                offset = 1;
            }
        }
        let words = &words[offset..];
        let mut fallback = None;
        if let Some(&verb) = words.first() {
            if is_verb(self.items[verb].token) {
                let (lemma, class, particle) = self.verb_parse(verb, clause.end);
                let after = 1 + usize::from(particle);
                let mut adverb = self.manner_after(words, after);
                let subject_start = adverb
                    .and_then(|a| words.iter().position(|w| *w == a))
                    .map_or(after, |p| p + 1);
                if let Some(length) = (1..=MAX_SUBJECT)
                    .filter(|&length| subject_start + length <= words.len())
                    .find(|&length| {
                        self.subject(words, subject_start, length)
                            && words.get(subject_start + length).is_none_or(|&next| {
                                let next = self.items[next].token;
                                self.items[words[subject_start + length - 1] + 1..]
                                    .iter()
                                    .take_while(|item| {
                                        item.token.span.byte.start < next.span.byte.start
                                    })
                                    .any(|item| item.token.text == ",")
                                    || (!is_nominal(next) && !is_possessive(next))
                            })
                    })
                {
                    let subject_end = subject_start + length;
                    if adverb.is_none() {
                        adverb = self.manner_after(words, subject_end);
                    }
                    let last = adverb.map_or(words[subject_end - 1], |adverb| {
                        adverb.max(words[subject_end - 1])
                    });
                    let parse = Parse {
                        verb,
                        lemma,
                        class,
                        adverb,
                        extent: verb..last + 1,
                    };
                    if class != DialogueTagVerbClass::Unlisted {
                        return Some(parse);
                    }
                    if is_finite(self.items[verb].token)
                        && !AUXILIARIES.contains(&parse.lemma.as_str())
                    {
                        fallback = Some(parse);
                    }
                }
            }
        }
        for length in 1..=MAX_SUBJECT.min(words.len().saturating_sub(1)) {
            if !self.subject(words, 0, length) {
                continue;
            }
            let mut position = length;
            let mut adverb = self.manner_after(words, position);
            let mut auxiliary = false;
            while position < words.len() {
                let token = self.items[words[position]].token;
                if is_modifier(token) {
                    if is_adverb(token) {
                        adverb = Some(words[position]);
                    }
                } else if !auxiliary
                    && (is_auxiliary(token) || is_clitic(token))
                    && words
                        .get(position + 1)
                        .is_some_and(|&next| is_verb(self.items[next].token))
                {
                    auxiliary = true;
                } else {
                    break;
                }
                position += 1;
            }
            let Some(&verb) = words.get(position) else {
                continue;
            };
            let token = self.items[verb].token;
            if !is_verb(token) {
                continue;
            }
            let (lemma, class, particle) = self.verb_parse(verb, clause.end);
            let after = position + 1 + usize::from(particle);
            if adverb.is_none() {
                adverb = self
                    .manner_after(words, after)
                    .or_else(|| self.adverb_after_object(words, after));
            }
            let verb_end = words[after - 1];
            let end = adverb.map_or(verb_end, |adverb| adverb.max(verb_end));
            let parse = Parse {
                verb,
                lemma,
                class,
                adverb,
                extent: words[0]..end + 1,
            };
            if class != DialogueTagVerbClass::Unlisted {
                return Some(parse);
            }
            if fallback.is_none()
                && (is_finite(token) || auxiliary)
                && !AUXILIARIES.contains(&parse.lemma.as_str())
            {
                fallback = Some(parse);
            }
        }
        fallback
    }

    fn before_tag(&self, clause: &Range<usize>, quote: Option<char>) -> Option<Parse> {
        let words = self.words(clause);
        let tail = words.len().saturating_sub(4);
        for position in (tail..words.len()).rev() {
            let verb = words[position];
            let token = self.items[verb].token;
            if !listed(token, self.next_word(verb, clause.end)) {
                continue;
            }
            let trailing = &words[position + 1..];
            let (lemma, class, particle) = self.verb_parse(verb, clause.end);
            let trailing = &trailing[usize::from(particle).min(trailing.len())..];
            let allowed = trailing.iter().all(|&index| {
                let token = self.items[index].token;
                is_adverb(token)
                    || is_name(token)
                    || has_pos(token, "PRP")
                    || matches!(lower(token).as_str(), "to" | "the")
                    || (has_pos(token, "NN") && !is_verb(token))
            });
            let subject_before = words[..position].iter().any(|&index| {
                let token = self.items[index].token;
                is_pronoun(token) || is_name(token) || has_pos(token, "NN")
            });
            let subject_after = trailing.first().is_some_and(|&index| {
                is_pronoun(self.items[index].token) || is_name(self.items[index].token)
            });
            if !allowed || !(subject_before || subject_after) {
                continue;
            }
            if takes_object(&lemma) {
                let head = words[..position].iter().rev().copied().find(|&index| {
                    let token = self.items[index].token;
                    !(is_adverb(token) || is_auxiliary(token) || is_clitic(token))
                });
                let person = head
                    .into_iter()
                    .chain(trailing.first().copied())
                    .any(|index| is_person(self.items[index].token));
                let exclaimed = matches!(quote, Some('!' | '?'))
                    && head.is_some_and(|index| has_pos(self.items[index].token, "NN"));
                let punctuated = matches!(self.last_text(clause), "," | ":");
                if !((person || exclaimed) && punctuated && speech_end(quote)) {
                    continue;
                }
            }
            let adverb = trailing
                .iter()
                .copied()
                .find(|&index| is_adverb(self.items[index].token))
                .or_else(|| {
                    position
                        .checked_sub(1)
                        .and_then(|p| self.adverb_at(words.get(p)))
                });
            let start = self.phrase_start(verb, clause.start);
            let verb_end = if particle { words[position + 1] } else { verb };
            let end = trailing.last().copied().unwrap_or(verb_end).max(verb_end);
            return Some(Parse {
                verb,
                lemma,
                class,
                adverb,
                extent: start.min(adverb.unwrap_or(start))..end + 1,
            });
        }
        let &verb = words.last()?;
        let token = self.items[verb].token;
        let subject = words.len() >= 2 && {
            let previous = self.items[words[words.len() - 2]].token;
            is_pronoun(previous) || is_name(previous)
        };
        if !subject || !is_finite(token) {
            return None;
        }
        let (lemma, class, _) = self.verb_parse(verb, clause.end);
        if class != DialogueTagVerbClass::Unlisted || AUXILIARIES.contains(&lemma.as_str()) {
            return None;
        }
        Some(Parse {
            verb,
            lemma,
            class,
            adverb: None,
            extent: words[words.len() - 2]..verb + 1,
        })
    }

    fn phrase_start(&self, verb: usize, floor: usize) -> usize {
        let mut start = verb;
        while start > floor && verb - start < MAX_SUBJECT + 1 {
            let token = self.items[start - 1].token;
            let title =
                token.text == "." && start >= floor + 2 && is_name(self.items[start - 2].token);
            let auxiliary = (is_auxiliary(token) || is_clitic(token))
                && self.items[start..verb]
                    .iter()
                    .all(|item| is_modifier(item.token));
            if !title
                && !(token.is_word
                    && (is_nominal(token)
                        || is_pronoun(token)
                        || is_determiner(token)
                        || is_modifier(token)
                        || auxiliary))
            {
                break;
            }
            start -= 1;
        }
        start
    }

    fn attached_after(&self, piece: &Piece, clause: &Range<usize>) -> bool {
        let Some(&first) = self.words(clause).first() else {
            return false;
        };
        let lowercase = self.items[first]
            .token
            .text
            .chars()
            .next()
            .is_some_and(char::is_lowercase);
        matches!(piece.last, Some(',' | ':' | ';'))
            || lowercase
            || self.after_tag(clause).is_some_and(|parse| {
                parse.class == DialogueTagVerbClass::Speech
                    && parse.lemma != "reflect"
                    && self.bare(clause, parse.extent.end)
            })
    }

    fn attached_before(&self, clause: &Range<usize>) -> bool {
        matches!(self.last_text(clause), "," | ":")
            || self.word(clause.end - 1).is_some_and(|token| {
                lead_in(token) && verb_lemma(token, None).1 == DialogueTagVerbClass::Speech
            })
    }

    fn has_verb(&self, clause: &Range<usize>) -> bool {
        let words = self.words(clause);
        words.iter().enumerate().any(|(position, &index)| {
            is_finite(self.items[index].token)
                && !position
                    .checked_sub(1)
                    .is_some_and(|previous| is_determiner(self.items[words[previous]].token))
        })
    }
}

pub(crate) fn starts_copula(token: &Token) -> bool {
    token.is_word
        && token.text.chars().next().is_some_and(char::is_lowercase)
        && token.readings.iter().any(|reading| {
            reading.pos.starts_with("VB") && AUXILIARIES.contains(&reading.lemma.as_str())
        })
}

fn speech_end(last: Option<char>) -> bool {
    last.is_none_or(|ch| matches!(ch, '.' | '!' | '?' | ',' | ';' | ':' | '…' | '—' | '–'))
}

fn last_char(text: &str) -> Option<char> {
    text.chars()
        .rev()
        .skip(1)
        .find(|ch| !ch.is_whitespace() && !matches!(ch, '\u{2060}' | '\u{200B}' | '\u{FEFF}'))
}

fn paragraphs(document: &Document) -> Vec<Paragraph<'_>> {
    document
        .paragraphs
        .iter()
        .map(|paragraph| {
            let items: Vec<Item> = paragraph
                .sentences
                .iter()
                .enumerate()
                .flat_map(|(sentence, tokens)| {
                    tokens
                        .tokens
                        .iter()
                        .filter(|token| !token.is_whitespace)
                        .map(move |token| Item { token, sentence })
                })
                .collect();
            let range = &paragraph.span.byte;
            let first = document
                .dialogue
                .partition_point(|span| span.byte.end <= range.start);
            let pieces = document
                .dialogue
                .iter()
                .enumerate()
                .skip(first)
                .take_while(|(_, span)| span.byte.start < range.end)
                .filter(|(dialogue, _)| document.is_speech(*dialogue))
                .filter_map(|(dialogue, span)| {
                    let first =
                        items.partition_point(|item| item.token.span.byte.start < span.byte.start);
                    let end =
                        items.partition_point(|item| item.token.span.byte.end <= span.byte.end);
                    if first >= end {
                        return None;
                    }
                    let last = end - 1;
                    let ends = span.byte.end <= range.end;
                    let local = span.byte.start.max(range.start) - range.start
                        ..span.byte.end.min(range.end) - range.start;
                    Some(Piece {
                        dialogue,
                        tokens: first..last + 1,
                        starts: span.byte.start >= range.start,
                        ends,
                        last: if ends {
                            paragraph.text.get(local).and_then(last_char)
                        } else {
                            None
                        },
                    })
                })
                .collect();
            Paragraph { items, pieces }
        })
        .collect()
}

struct Builder<'a> {
    tags: Vec<DialogueTag>,
    beats: Vec<DialogueTagActionBeat>,
    attributed: BTreeSet<usize>,
    tagged: BTreeSet<usize>,
    paragraph: Option<&'a Paragraph<'a>>,
    dialogue: &'a [Span],
}

impl<'a> Builder<'a> {
    fn tag(
        &mut self,
        clause: Range<usize>,
        parse: Parse,
        position: DialogueTagPosition,
        dialogue: Vec<usize>,
    ) {
        let items = &self.paragraph.unwrap().items;
        let token = items[parse.verb].token;
        self.attributed.extend(&dialogue);
        self.tagged.extend(&dialogue);
        self.tags.push(DialogueTag {
            span: span_of(items, parse.extent),
            clause: span_of(items, first_word(items, &clause)..clause.end),
            verb: token.span.clone(),
            verb_text: token.text.clone(),
            showy: lists().showy.contains(parse.lemma.as_str())
                || parse.class == DialogueTagVerbClass::NonSpeech,
            lemma: parse.lemma,
            class: parse.class,
            position,
            adverb: parse
                .adverb
                .filter(|_| parse.class == DialogueTagVerbClass::Speech)
                .map(|index| items[index].token.span.clone()),
            quotes: quotes(self.dialogue, &dialogue),
        });
    }

    fn beat(&mut self, clause: Range<usize>, dialogue: Vec<usize>) {
        let items = &self.paragraph.unwrap().items;
        self.attributed.extend(&dialogue);
        self.beats.push(DialogueTagActionBeat {
            span: span_of(items, first_word(items, &clause)..clause.end),
            quotes: quotes(self.dialogue, &dialogue),
        });
    }
}

fn quotes(dialogue: &[Span], indexes: &[usize]) -> Vec<Span> {
    indexes
        .iter()
        .map(|&index| dialogue[index].clone())
        .collect()
}

pub fn report(
    document: &Document,
    options: &DialogueTagOptions,
    baseline: Option<&DialogueTagBaseline>,
) -> Result<DialogueTagReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let paragraphs = paragraphs(document);
    let mut builder = Builder {
        tags: Vec::new(),
        beats: Vec::new(),
        attributed: BTreeSet::new(),
        tagged: BTreeSet::new(),
        paragraph: None,
        dialogue: &document.dialogue,
    };
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if paragraph.items.is_empty() {
            continue;
        }
        builder.paragraph = Some(paragraph);
        if paragraph.pieces.is_empty()
            && !document.paragraphs[index].scene_break_before
            && paragraph
                .items
                .iter()
                .filter(|item| item.token.is_word)
                .count()
                <= 12
            && paragraph.items.iter().any(|item| {
                item.token.readings.iter().any(|reading| {
                    reading.pos.starts_with("VB")
                        && matches!(
                            reading.lemma.as_str(),
                            "cut"
                                | "interrupt"
                                | "nod"
                                | "shake"
                                | "smile"
                                | "shrug"
                                | "answer"
                                | "reply"
                        )
                })
            })
        {
            if let Some(previous) = index
                .checked_sub(1)
                .and_then(|i| paragraphs[i].pieces.last())
                .filter(|piece| piece.ends)
            {
                let clause = paragraph.head(&(0..paragraph.items.len()));
                builder.beat(clause, vec![previous.dialogue]);
            }
        }
        let following = paragraphs
            .get(index + 1)
            .and_then(|next| next.pieces.first())
            .filter(|piece| piece.starts && piece.tokens.start == 0);
        let mut boundaries = vec![0];
        for piece in &paragraph.pieces {
            boundaries.push(piece.tokens.start);
            boundaries.push(piece.tokens.end);
        }
        boundaries.push(paragraph.items.len());
        for (segment, bounds) in boundaries.chunks(2).enumerate() {
            let range = bounds[0]..bounds[1];
            if paragraph.words(&range).is_empty() {
                continue;
            }
            let previous = segment
                .checked_sub(1)
                .map(|k| &paragraph.pieces[k])
                .filter(|piece| piece.ends);
            let next = paragraph.pieces.get(segment).filter(|piece| piece.starts);
            let head = paragraph.head(&range);
            let tail = paragraph.tail(&range);
            let single = head == tail;
            let target = match next {
                Some(next) => Some(next),
                None if segment == paragraph.pieces.len() => following,
                None => None,
            };
            let quote = target.and_then(|piece| piece.last);
            let mut head_used = false;
            if let Some(previous) = previous {
                if paragraph.attached_after(previous, &head) {
                    if let Some(parse) = paragraph.after_tag(&head) {
                        let terminal = ends_sentence(paragraph.items[head.end - 1].token);
                        match next {
                            Some(next) if head.end == range.end && !terminal => builder.tag(
                                head.clone(),
                                parse,
                                DialogueTagPosition::Split,
                                vec![previous.dialogue, next.dialogue],
                            ),
                            _ => builder.tag(
                                head.clone(),
                                parse,
                                DialogueTagPosition::After,
                                vec![previous.dialogue],
                            ),
                        }
                    }
                    head_used = true;
                } else if paragraph.has_verb(&head) {
                    let leads = single
                        && target.is_some()
                        && paragraph.attached_before(&tail)
                        && paragraph.before_tag(&tail, quote).is_some();
                    if !leads {
                        let mut dialogue = vec![previous.dialogue];
                        if single {
                            dialogue.extend(next.map(|next| next.dialogue));
                        }
                        builder.beat(head.clone(), dialogue);
                        head_used = true;
                    }
                }
            }
            if single && head_used {
                continue;
            }
            let Some(target) = target.map(|piece| piece.dialogue) else {
                continue;
            };
            if paragraph.attached_before(&tail) {
                if let Some(parse) = paragraph.before_tag(&tail, quote) {
                    builder.tag(tail, parse, DialogueTagPosition::Before, vec![target]);
                    continue;
                }
            }
            if next.is_some() && paragraph.has_verb(&tail) {
                builder.beat(tail, vec![target]);
            }
        }
    }
    let Builder {
        tags,
        beats,
        attributed,
        tagged,
        ..
    } = builder;
    let (untagged_runs, untagged_exchanges) =
        untagged_runs(document, &paragraphs, &attributed, options);
    let speech_spans = paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.pieces.iter().map(|piece| piece.dialogue))
        .collect::<BTreeSet<_>>()
        .len();
    let exchanges = paragraphs
        .iter()
        .filter(|paragraph| paragraph.pieces.iter().any(|piece| piece.starts))
        .count();
    let mut verbs: BTreeMap<(String, DialogueTagVerbClass), usize> = BTreeMap::new();
    for tag in &tags {
        *verbs.entry((tag.lemma.clone(), tag.class)).or_default() += 1;
    }
    let said_asked = verbs
        .iter()
        .filter(|((lemma, _), _)| matches!(lemma.as_str(), "say" | "ask"))
        .map(|(_, count)| count)
        .sum();
    let counts = |predicate: &dyn Fn(&str, DialogueTagVerbClass) -> bool| {
        verbs
            .iter()
            .filter(|((lemma, class), _)| predicate(lemma, *class))
            .map(|((lemma, class), count)| DialogueTagVerbCount {
                lemma: lemma.clone(),
                class: *class,
                count: *count,
            })
            .collect::<Vec<_>>()
    };
    let neutral_verbs = counts(&|lemma, _| lists().neutral.contains(lemma));
    let showy_verbs = counts(&|lemma, class| {
        lists().showy.contains(lemma) || class == DialogueTagVerbClass::NonSpeech
    });
    let mut other_verbs: Vec<DialogueTagVerbCount> = verbs
        .into_iter()
        .filter(|((lemma, _), _)| !matches!(lemma.as_str(), "say" | "ask"))
        .map(|((lemma, class), count)| DialogueTagVerbCount {
            lemma,
            class,
            count,
        })
        .collect();
    other_verbs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.lemma.cmp(&b.lemma)));
    let with_adverb = tags.iter().filter(|tag| tag.adverb.is_some()).count();
    let non_speech = tags
        .iter()
        .filter(|tag| tag.class == DialogueTagVerbClass::NonSpeech)
        .count();
    let enough = tags.len() >= options.minimum_tags.max(1);
    let share = |count: usize, baseline: Option<f64>, band: Option<&PercentileBand>| {
        let mut share = Share::new(count, tags.len(), enough, baseline);
        share.highlighted = PercentileBand::highlights(band, share.percent);
        share
    };
    Ok(DialogueTagReport {
        said_asked: share(
            said_asked,
            baseline.and_then(|b| b.said_asked_percent),
            baseline.and_then(|b| b.said_asked_band.as_ref()),
        ),
        with_adverb: share(
            with_adverb,
            baseline.and_then(|b| b.adverb_percent),
            baseline.and_then(|b| b.adverb_band.as_ref()),
        ),
        non_speech: share(
            non_speech,
            baseline.and_then(|b| b.non_speech_percent),
            baseline.and_then(|b| b.non_speech_band.as_ref()),
        ),
        neutral_verbs,
        showy_verbs,
        low_confidence: tags.len() < 50,
        dialogue_spans: document.dialogue.len(),
        speech_spans,
        tagged_spans: tagged.len(),
        exchanges,
        untagged_exchanges,
        other_verbs,
        untagged_runs,
        beats,
        tags,
    })
}

fn untagged_runs(
    document: &Document,
    paragraphs: &[Paragraph],
    attributed: &BTreeSet<usize>,
    options: &DialogueTagOptions,
) -> (Vec<DialogueTagUntaggedRun>, usize) {
    let mut runs = Vec::new();
    let mut untagged = 0;
    let mut current: Vec<Vec<usize>> = Vec::new();
    let mut close = |current: &mut Vec<Vec<usize>>| {
        if current.len() > options.max_untagged_exchanges {
            let dialogue: Vec<usize> = current.iter().flatten().copied().collect();
            let first = &document.dialogue[dialogue[0]];
            let last = &document.dialogue[*dialogue.last().unwrap()];
            runs.push(DialogueTagUntaggedRun {
                span: Span::covering(first, last),
                exchanges: current.len(),
                quotes: quotes(&document.dialogue, &dialogue),
            });
        }
        current.clear();
    };
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if document.paragraphs[index].scene_break_before {
            close(&mut current);
        }
        let starting: Vec<usize> = paragraph
            .pieces
            .iter()
            .filter(|piece| piece.starts)
            .map(|piece| piece.dialogue)
            .collect();
        if starting.is_empty() {
            if paragraph.pieces.is_empty() && !paragraph.items.is_empty() {
                close(&mut current);
            }
            continue;
        }
        if starting.iter().any(|index| attributed.contains(index)) {
            close(&mut current);
        } else {
            untagged += 1;
            current.push(starting);
        }
    }
    close(&mut current);
    (runs, untagged)
}
