use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{sentence_starts, Share, Split};
use crate::{Document, Mode, Span, StyleError, Token};

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
    pub words: usize,
    pub pronoun_words: Share,
    pub word_pronouns: Vec<PronounStartsShare>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PronounStartsReport {
    pub starts: Vec<PronounStart>,
    pub groups: Split<PronounStartsGroup>,
}

fn is_pronoun(tokens: &[&Token]) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };
    let word = first.text.to_lowercase();
    let has = |token: &Token, tags: &[&str]| {
        token
            .readings
            .iter()
            .any(|r| tags.contains(&sentence_starts::normalize_pos(&r.pos)))
    };
    let next = tokens.get(1).copied();
    let nominal = next.is_some_and(|t| {
        has(
            t,
            &[
                "NN", "NNS", "NNP", "NNPS", "JJ", "JJR", "JJS", "PRP$", "DT", "PDT", "VBG",
            ],
        )
    });
    if ["her", "his", "whose", "what", "which", "one"].contains(&word.as_str()) && nominal {
        return false;
    }
    if word == "what"
        && next.is_some_and(|t| ["'s", "’s"].contains(&t.text.as_str()))
        && tokens
            .get(2)
            .is_some_and(|t| t.text.eq_ignore_ascii_case("more"))
    {
        return false;
    }
    if [
        "this", "that", "these", "those", "each", "either", "neither",
    ]
    .contains(&word.as_str())
    {
        let following = tokens
            .iter()
            .skip(1)
            .find(|t| !has(t, &["RB", "RBR", "RBS"]));
        return following.is_none_or(|t| {
            !t.is_word
                || ["'s", "’s"].contains(&t.text.as_str())
                || (!nominal && has(t, &["VBD", "VBP", "VBZ", "MD"]))
        });
    }
    [
        "i",
        "you",
        "he",
        "she",
        "it",
        "we",
        "they",
        "me",
        "him",
        "her",
        "us",
        "them",
        "mine",
        "yours",
        "his",
        "hers",
        "ours",
        "theirs",
        "myself",
        "yourself",
        "yourselves",
        "himself",
        "herself",
        "itself",
        "ourselves",
        "themselves",
        "oneself",
        "whose",
        "someone",
        "somebody",
        "something",
        "nobody",
        "nothing",
        "everyone",
        "everybody",
        "everything",
        "anyone",
        "anybody",
        "anything",
    ]
    .contains(&word.as_str())
        || has(first, &["PRP", "WP"])
}

fn is_word_pronoun(token: &Token, previous: Option<&Token>) -> bool {
    const FORMS: &[&str] = &[
        "i",
        "me",
        "you",
        "he",
        "him",
        "she",
        "her",
        "it",
        "we",
        "us",
        "they",
        "them",
        "my",
        "mine",
        "your",
        "yours",
        "his",
        "hers",
        "its",
        "our",
        "ours",
        "their",
        "theirs",
        "myself",
        "yourself",
        "yourselves",
        "himself",
        "herself",
        "itself",
        "ourselves",
        "themselves",
        "oneself",
        "thou",
        "thee",
        "thy",
        "thine",
        "ye",
    ];
    let word = token.text.to_lowercase();
    if !FORMS.contains(&word.as_str()) || token.text == "US" {
        return false;
    }
    // The tagger keeps only the noun reading in "all mine", "both mine".
    if word == "mine"
        && previous
            .is_some_and(|prior| matches!(prior.text.to_lowercase().as_str(), "all" | "both"))
    {
        return true;
    }
    // The tagger also leaves a possessive reading on the noun in "coal mine".
    if word == "mine"
        && previous.is_some_and(|prior| {
            prior.readings.iter().any(|reading| {
                matches!(
                    sentence_starts::normalize_pos(&reading.pos),
                    "NN" | "JJ" | "DT" | "PRP$"
                )
            })
        })
    {
        return false;
    }
    token
        .readings
        .iter()
        .any(|reading| matches!(sentence_starts::normalize_pos(&reading.pos), "PRP" | "PRP$"))
        || matches!(word.as_str(), "thou" | "ye")
}

pub fn report(
    document: &Document,
    options: &PronounStartsOptions,
) -> Result<PronounStartsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details("pronounStarts"));
    }
    let mut groups: Split<PronounStartsGroup> = Split::default();
    let mut word_counts: Split<BTreeMap<String, usize>> = Split::default();
    let word_totals = super::word_split(document);
    for sentence in document.sentences() {
        let mut previous = None;
        for token in sentence.words() {
            if is_word_pronoun(token, previous) {
                let mode = document.mode_at(token.span.byte.start);
                let word = token.text.to_lowercase();
                *word_counts.all.entry(word.clone()).or_default() += 1;
                *word_counts.mode_mut(mode).entry(word).or_default() += 1;
            }
            previous = Some(token);
        }
    }
    let mut starts = Vec::new();
    let mut run: Option<PronounStartsRun> = None;
    let mut runs = Vec::new();
    let flush = |run: &mut Option<PronounStartsRun>, runs: &mut Vec<PronounStartsRun>| {
        runs.extend(run.take().filter(|r| r.sentences.len() >= 3));
    };
    let mut index = 0;
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before
            || paragraph.chapter_start
            || run.as_ref().is_some_and(|r| r.mode == Mode::Dialogue)
        {
            flush(&mut run, &mut runs);
        }
        for sentence in &paragraph.sentences {
            let mode = document.sentence_mode(sentence);
            let tokens = sentence_starts::opening_tokens(sentence);
            let first = tokens.first().copied();
            for group in groups.all_and_mode_mut(mode) {
                group.sentences += 1;
                group.excluded_sentences += usize::from(first.is_some_and(|t| t.is_skipped));
            }
            let pronoun = first.filter(|t| !t.is_skipped && is_pronoun(&tokens));
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
                flush(&mut run, &mut runs);
                run = Some(PronounStartsRun {
                    span: sentence.span.clone(),
                    pronoun: word,
                    sentences: index..index + 1,
                    mode,
                });
            } else {
                flush(&mut run, &mut runs);
            }
            index += 1;
        }
    }
    flush(&mut run, &mut runs);
    let reported = if runs.iter().any(|r| r.mode == Mode::Narration) {
        sentence_starts::report(document, &Default::default(), None)?
            .groups
            .narration
            .same_word_runs
    } else {
        Vec::new()
    };
    for run in runs {
        if run.mode == Mode::Narration
            && reported.iter().any(|r| {
                r.sentences.start <= run.sentences.start && run.sentences.end <= r.sentences.end
            })
        {
            continue;
        }
        for group in groups.all_and_mode_mut(run.mode) {
            group.runs.push(run.clone());
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
        let words = match mode {
            None => word_totals.all,
            Some(Mode::Narration) => word_totals.narration,
            Some(Mode::Dialogue) => word_totals.dialogue,
        };
        let word_counts = match mode {
            None => &word_counts.all,
            Some(Mode::Narration) => &word_counts.narration,
            Some(Mode::Dialogue) => &word_counts.dialogue,
        };
        group.words = words;
        group.pronoun_words = Share::new(word_counts.values().sum(), words, words > 0, None);
        group.word_pronouns = word_counts
            .iter()
            .map(|(pronoun, &count)| PronounStartsShare {
                pronoun: pronoun.clone(),
                share: Share::new(count, words, words > 0, None),
            })
            .collect();
    }
    Ok(PronounStartsReport { starts, groups })
}
