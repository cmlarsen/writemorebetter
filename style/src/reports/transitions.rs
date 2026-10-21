use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{sentence_starts, Share, Split};
use crate::{Document, Mode, Span, StyleError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionType {
    Contrast,
    Cause,
    Time,
    Addition,
    Example,
    Condition,
}
impl TransitionType {
    pub const ALL: [Self; 6] = [
        Self::Contrast,
        Self::Cause,
        Self::Time,
        Self::Addition,
        Self::Example,
        Self::Condition,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionsOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
    #[serde(alias = "repeat_window")]
    pub repeat_window: usize,
}
impl Default for TransitionsOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
            repeat_window: 5,
        }
    }
}
impl TransitionsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, value: usize) -> Self {
        self.minimum_sentences = value;
        self
    }
    pub fn with_repeat_window(mut self, value: usize) -> Self {
        self.repeat_window = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionMatch {
    pub span: Span,
    pub sentence: usize,
    pub phrase: String,
    pub kind: TransitionType,
    pub opening: bool,
    pub mode: Mode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionRepeat {
    pub span: Span,
    pub phrase: String,
    pub spans: Vec<Span>,
    pub sentences: Vec<usize>,
    pub mode: Mode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionTypeShare {
    pub kind: TransitionType,
    pub containing: Share,
    pub opening: Share,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionsGroup {
    pub sentences: usize,
    pub low_confidence: bool,
    pub containing: Share,
    pub opening: Share,
    pub types: Vec<TransitionTypeShare>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TransitionsReport {
    pub matches: Vec<TransitionMatch>,
    pub repeats: Vec<TransitionRepeat>,
    pub groups: Split<TransitionsGroup>,
}

type TransitionEntry = (TransitionType, &'static str, Vec<&'static str>);

const SUBJECTS: [&str; 8] = ["i", "you", "he", "she", "it", "we", "they", "there"];
// Heads whose `if` clause is a complement ("asks if", "not sure if"), matched by lemma
// from the text so compact and detailed documents agree.
const IF_COMPLEMENTS: &[&str] = &[
    "ask", "wonder", "see", "know", "check", "decide", "remember", "doubt", "tell", "sure",
    "unsure",
];
const IRREGULAR_FORMS: &[(&str, &str)] = &[
    ("saw", "see"),
    ("seen", "see"),
    ("knew", "know"),
    ("known", "know"),
    ("told", "tell"),
];
const OBJECTS: [&str; 6] = ["me", "you", "him", "her", "us", "them"];
const BE_FORMS: [&str; 8] = ["is", "was", "were", "are", "be", "been", "being", "'s"];

fn inflects(word: &str, base: &str) -> bool {
    // "decide" → "decides", "decided", "deciding"; "ask" → "asks", "asked", "asking".
    let stem = base.strip_suffix('e').unwrap_or(base);
    word == base
        || IRREGULAR_FORMS.contains(&(word, base))
        || ["s", "ed", "ing"]
            .iter()
            .any(|suffix| word.strip_prefix(base) == Some(suffix))
        || ["es", "ed", "ing"]
            .iter()
            .any(|suffix| word.strip_prefix(stem) == Some(suffix))
}

fn is_if_complement(word: &str) -> bool {
    IF_COMPLEMENTS.iter().any(|base| inflects(word, base))
}

fn phrases() -> &'static Vec<TransitionEntry> {
    static PHRASES: OnceLock<Vec<TransitionEntry>> = OnceLock::new();
    PHRASES.get_or_init(|| {
        let mut phrases: Vec<_> = include_str!("../../data/transitions.tsv")
            .lines()
            .map(|line| {
                let (kind, phrase) = line.split_once('\t').expect("transition TSV");
                let kind = match kind {
                    "contrast" => TransitionType::Contrast,
                    "cause" => TransitionType::Cause,
                    "time" => TransitionType::Time,
                    "addition" => TransitionType::Addition,
                    "example" => TransitionType::Example,
                    "condition" => TransitionType::Condition,
                    _ => unreachable!(),
                };
                (
                    kind,
                    phrase,
                    if phrase == "what's more" {
                        vec!["what", "'s", "more"]
                    } else {
                        phrase.split_whitespace().collect::<Vec<_>>()
                    },
                )
            })
            .collect();
        phrases.sort_by_key(|(_, _, words)| std::cmp::Reverse(words.len()));
        phrases
    })
}

pub fn report(
    document: &Document,
    options: &TransitionsOptions,
) -> Result<TransitionsReport, StyleError> {
    if options.repeat_window == 0 {
        return Err(StyleError::options(
            "repeatWindow must be at least 1 sentence",
        ));
    }
    let mut matches = Vec::new();
    let mut repeats = Vec::new();
    let mut groups: Split<TransitionsGroup> = Split::default();
    let mut previous: BTreeMap<String, usize> = BTreeMap::new();
    let mut index: usize = 0;
    let mut last_mode = None;
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before
            || paragraph.chapter_start
            || last_mode == Some(Mode::Dialogue)
        {
            previous.clear();
        }
        for sentence in &paragraph.sentences {
            let mode = document.sentence_mode(sentence);
            if last_mode != Some(mode) {
                previous.clear();
            }
            last_mode = Some(mode);
            let tokens = sentence_starts::opening_tokens(sentence);
            for group in groups.all_and_mode_mut(mode) {
                group.sentences += 1;
            }
            let folded: Vec<_> = tokens
                .iter()
                .map(|t| super::echoes::fold(&t.text))
                .collect();
            let mut at = 0;
            while at < tokens.len() {
                let found = phrases().iter().find(|(_, phrase, words)| {
                    if at + words.len() > tokens.len() {
                        return false;
                    }
                    let selected = &tokens[at..at + words.len()];
                    if selected.iter().any(|token| token.is_skipped)
                        || !folded[at..at + words.len()]
                            .iter()
                            .zip(words)
                            .all(|(token, word)| token == word)
                    {
                        return false;
                    }
                    if !selected.windows(2).all(|pair| {
                        sentence
                            .text
                            .get(
                                pair[0].span.byte.end - sentence.span.byte.start
                                    ..pair[1].span.byte.start - sentence.span.byte.start,
                            )
                            .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                    }) {
                        return false;
                    }
                    let next = tokens.get(at + words.len());
                    let following = folded.get(at + words.len()).map(String::as_str);
                    let comma = next.is_some_and(|t| t.text == ",");
                    let comma_before = at > 0 && tokens[at - 1].text == ",";
                    let subject = following.is_some_and(|word| SUBJECTS.contains(&word));
                    match *phrase {
                        "and" | "but" => at == 0,
                        "so" => at == 0 && subject,
                        "then" => at == 0 && comma,
                        "thus" => (at == 0 && comma) || subject,
                        "besides" => comma,
                        "however" => comma || (comma_before && !next.is_some_and(|t| t.is_word)),
                        "finally" => comma || comma_before,
                        "instead" => following != Some("of"),
                        "if" => {
                            let prior = at.checked_sub(1).map(|i| folded[i].as_str());
                            // "Tell me if": skip an object pronoun to reach the head.
                            let head = match prior {
                                Some(word) if OBJECTS.contains(&word) && at >= 2 => {
                                    Some(folded[at - 2].as_str())
                                }
                                _ => prior,
                            };
                            prior != Some("as")
                                && !prior.is_some_and(is_if_complement)
                                && !head.is_some_and(is_if_complement)
                        }
                        // "was as long as it was wide" measures rather than conditions.
                        "as long as" if at > 0 && BE_FORMS.contains(&folded[at - 1].as_str()) => {
                            false
                        }
                        "as long as" | "provided that" => at == 0 || comma_before || subject,
                        _ => true,
                    }
                });
                if let Some((kind, phrase, words)) = found {
                    let span = Span::covering(&tokens[at].span, &tokens[at + words.len() - 1].span);
                    let current = matches.len();
                    if let Some(&prior) = previous.get(*phrase) {
                        let prior: &TransitionMatch = &matches[prior];
                        if index.saturating_sub(prior.sentence) < options.repeat_window {
                            repeats.push(TransitionRepeat {
                                span: Span::covering(&prior.span, &span),
                                phrase: (*phrase).to_owned(),
                                spans: vec![prior.span.clone(), span.clone()],
                                sentences: vec![prior.sentence, index],
                                mode,
                            });
                        }
                    }
                    previous.insert((*phrase).to_owned(), current);
                    matches.push(TransitionMatch {
                        span,
                        sentence: index,
                        phrase: (*phrase).to_owned(),
                        kind: *kind,
                        opening: at == 0,
                        mode,
                    });
                    at += words.len();
                } else {
                    at += 1;
                }
            }
            index += 1;
        }
    }
    for (mode, group) in [
        (None, &mut groups.all),
        (Some(Mode::Narration), &mut groups.narration),
        (Some(Mode::Dialogue), &mut groups.dialogue),
    ] {
        let eligible = group.sentences;
        let enough = eligible >= options.minimum_sentences;
        group.low_confidence = eligible < 200.max(options.minimum_sentences);
        let count = |kind: Option<TransitionType>, opening: bool| {
            matches
                .iter()
                .filter(|m| {
                    mode.is_none_or(|mode| mode == m.mode)
                        && kind.is_none_or(|kind| kind == m.kind)
                        && (!opening || m.opening)
                })
                .map(|m| m.sentence)
                .collect::<BTreeSet<_>>()
                .len()
        };
        group.containing = Share::new(count(None, false), eligible, enough, None);
        group.opening = Share::new(count(None, true), eligible, enough, None);
        group.types = TransitionType::ALL
            .into_iter()
            .map(|kind| TransitionTypeShare {
                kind,
                containing: Share::new(count(Some(kind), false), eligible, enough, None),
                opening: Share::new(count(Some(kind), true), eligible, enough, None),
            })
            .collect();
    }
    Ok(TransitionsReport {
        matches,
        repeats,
        groups,
    })
}
