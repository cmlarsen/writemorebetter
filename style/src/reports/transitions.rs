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
}
impl TransitionType {
    pub const ALL: [Self; 5] = [
        Self::Contrast,
        Self::Cause,
        Self::Time,
        Self::Addition,
        Self::Example,
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
    pub excluded_sentences: usize,
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

fn phrases() -> &'static Vec<(TransitionType, &'static str, Vec<&'static str>)> {
    static PHRASES: OnceLock<Vec<(TransitionType, &'static str, Vec<&'static str>)>> =
        OnceLock::new();
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
    if !document.token_details {
        return Err(StyleError::needs_token_details("transitions"));
    }
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
        if paragraph.scene_break_before || paragraph.chapter_start {
            previous.clear();
        }
        for sentence in &paragraph.sentences {
            let mode = document.sentence_mode(sentence);
            if last_mode != Some(mode) {
                previous.clear();
            }
            last_mode = Some(mode);
            let tokens = sentence_starts::opening_tokens(sentence);
            let excluded = tokens.first().is_some_and(|t| t.is_skipped);
            for group in groups.all_and_mode_mut(mode) {
                group.sentences += 1;
                group.excluded_sentences += usize::from(excluded);
            }
            if excluded {
                previous.clear();
                index += 1;
                continue;
            }
            let mut at = 0;
            while at < tokens.len() {
                let found = phrases().iter().find(|(_, phrase, words)| {
                    if at + words.len() > tokens.len() {
                        return false;
                    }
                    let selected = &tokens[at..at + words.len()];
                    if !selected.iter().zip(words).all(|(token, word)| {
                        !token.is_skipped && token.text.to_lowercase().replace('’', "'") == *word
                    }) {
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
                    let comma = next.is_some_and(|t| t.text == ",");
                    match *phrase {
                        "and" | "but" => at == 0,
                        "so" => {
                            at == 0
                                && next.is_some_and(|t| {
                                    ["i", "you", "he", "she", "it", "we", "they", "there"]
                                        .contains(&t.text.to_lowercase().as_str())
                                })
                        }
                        "then" => at == 0 && comma,
                        "however" | "besides" | "finally" => {
                            comma || (at > 0 && tokens[at - 1].text == ",")
                        }
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
        let eligible = group.sentences - group.excluded_sentences;
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
