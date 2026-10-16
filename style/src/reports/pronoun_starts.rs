use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{sentence_starts, Share, Split};
use crate::{Document, Mode, Span, StyleError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
}
impl Default for PronounStartsOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
        }
    }
}
impl PronounStartsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, value: usize) -> Self {
        self.minimum_sentences = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStart {
    pub span: Span,
    pub sentence: usize,
    pub pronoun: String,
    pub mode: Mode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsRun {
    pub span: Span,
    pub pronoun: String,
    pub sentences: std::ops::Range<usize>,
    pub mode: Mode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsShare {
    pub pronoun: String,
    #[serde(flatten)]
    pub share: Share,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsGroup {
    pub sentences: usize,
    pub excluded_sentences: usize,
    pub low_confidence: bool,
    pub pronoun_starts: Share,
    pub pronouns: Vec<PronounStartsShare>,
    pub runs: Vec<PronounStartsRun>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsReport {
    pub starts: Vec<PronounStart>,
    pub groups: Split<PronounStartsGroup>,
}

pub fn report(
    document: &Document,
    options: &PronounStartsOptions,
) -> Result<PronounStartsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details("pronounStarts"));
    }
    let mut groups: Split<PronounStartsGroup> = Split::default();
    let mut starts = Vec::new();
    let mut run: Option<PronounStartsRun> = None;
    let flush = |run: &mut Option<PronounStartsRun>, groups: &mut Split<PronounStartsGroup>| {
        if let Some(run) = run.take().filter(|r| r.sentences.len() >= 3) {
            for group in groups.all_and_mode_mut(run.mode) {
                group.runs.push(run.clone());
            }
        }
    };
    let mut index = 0;
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before || paragraph.chapter_start {
            flush(&mut run, &mut groups);
        }
        for sentence in &paragraph.sentences {
            let mode = document.sentence_mode(sentence);
            let tokens = sentence_starts::opening_tokens(sentence);
            let first = tokens.first().copied();
            for group in groups.all_and_mode_mut(mode) {
                group.sentences += 1;
                group.excluded_sentences += usize::from(first.is_some_and(|t| t.is_skipped));
            }
            let pronoun = first.filter(|t| {
                !t.is_skipped
                    && t.readings.iter().any(|r| {
                        let tag = sentence_starts::normalize_pos(&r.pos);
                        matches!(tag, "PRP" | "WP")
                    })
            });
            if let Some(token) = pronoun {
                let word = token.text.to_lowercase();
                starts.push(PronounStart {
                    span: token.span.clone(),
                    sentence: index,
                    pronoun: word.clone(),
                    mode,
                });
                if let Some(previous) = &mut run {
                    if previous.pronoun == word && previous.mode == mode {
                        previous.span = Span::covering(&previous.span, &sentence.span);
                        previous.sentences.end = index + 1;
                        index += 1;
                        continue;
                    }
                }
                flush(&mut run, &mut groups);
                run = Some(PronounStartsRun {
                    span: sentence.span.clone(),
                    pronoun: word,
                    sentences: index..index + 1,
                    mode,
                });
            } else {
                flush(&mut run, &mut groups);
            }
            index += 1;
        }
    }
    flush(&mut run, &mut groups);
    for (mode, group) in [
        (None, &mut groups.all),
        (Some(Mode::Narration), &mut groups.narration),
        (Some(Mode::Dialogue), &mut groups.dialogue),
    ] {
        let eligible = group.sentences - group.excluded_sentences;
        let enough = eligible >= options.minimum_sentences;
        group.low_confidence = eligible < 200.max(options.minimum_sentences);
        let mut counts = BTreeMap::new();
        for start in starts.iter().filter(|s| mode.is_none_or(|m| m == s.mode)) {
            *counts.entry(start.pronoun.clone()).or_insert(0) += 1;
        }
        group.pronoun_starts = Share::new(counts.values().sum(), eligible, enough, None);
        group.pronouns = counts
            .into_iter()
            .map(|(pronoun, count)| PronounStartsShare {
                pronoun,
                share: Share::new(count, eligible, enough, None),
            })
            .collect();
    }
    Ok(PronounStartsReport { starts, groups })
}
