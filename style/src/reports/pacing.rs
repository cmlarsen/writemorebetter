use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::reports::echoes::fold;
use crate::reports::{word_split, Split};
use crate::{Document, Span, StyleError, Token};

const COGNITION: &str = include_str!("../../data/pacing/cognition.txt");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PacingOptions {
    #[serde(alias = "minimum_paragraphs")]
    pub minimum_paragraphs: usize,
    #[serde(alias = "more_than_words")]
    pub more_than_words: usize,
    #[serde(alias = "minimum_sentence_length")]
    pub minimum_sentence_length: f64,
    #[serde(alias = "minimum_past_perfect_percent")]
    pub minimum_past_perfect_percent: f64,
    #[serde(alias = "minimum_cognition_percent")]
    pub minimum_cognition_percent: f64,
}

impl Default for PacingOptions {
    fn default() -> Self {
        Self {
            minimum_paragraphs: 3,
            more_than_words: 400,
            minimum_sentence_length: 18.0,
            minimum_past_perfect_percent: 20.0,
            minimum_cognition_percent: 20.0,
        }
    }
}

impl PacingOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_paragraphs(mut self, value: usize) -> Self {
        self.minimum_paragraphs = value;
        self
    }
    pub fn with_more_than_words(mut self, value: usize) -> Self {
        self.more_than_words = value;
        self
    }
    pub fn with_minimum_sentence_length(mut self, value: f64) -> Self {
        self.minimum_sentence_length = value;
        self
    }
    pub fn with_minimum_past_perfect_percent(mut self, value: f64) -> Self {
        self.minimum_past_perfect_percent = value;
        self
    }
    pub fn with_minimum_cognition_percent(mut self, value: f64) -> Self {
        self.minimum_cognition_percent = value;
        self
    }

    fn validate(&self) -> Result<(), StyleError> {
        if self.minimum_paragraphs == 0 {
            return Err(StyleError::options("minimumParagraphs must be positive"));
        }
        if !self.minimum_sentence_length.is_finite() || self.minimum_sentence_length < 0.0 {
            return Err(StyleError::options(
                "minimumSentenceLength must be finite and nonnegative",
            ));
        }
        for value in [
            self.minimum_past_perfect_percent,
            self.minimum_cognition_percent,
        ] {
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(StyleError::options("pacing percentages must be in 0..=100"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PacingSignal {
    SentenceLength,
    PastPerfect,
    Cognition,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PacingSignals {
    pub words: Split<usize>,
    pub sentences: usize,
    pub verb_phrases: usize,
    pub past_perfect: usize,
    pub cognition: usize,
    pub dialogue_percent: Option<f64>,
    pub mean_sentence_length: Option<f64>,
    pub past_perfect_percent: Option<f64>,
    pub cognition_percent: Option<f64>,
}

impl PacingSignals {
    fn finish(&mut self) {
        let percent = |count, total| (total > 0).then(|| 100.0 * count as f64 / total as f64);
        self.dialogue_percent = percent(self.words.dialogue, self.words.all);
        self.mean_sentence_length =
            (self.sentences > 0).then(|| self.words.all as f64 / self.sentences as f64);
        self.past_perfect_percent = percent(self.past_perfect, self.verb_phrases);
        self.cognition_percent = percent(self.cognition, self.verb_phrases);
    }

    fn triggered(&self, options: &PacingOptions) -> Vec<PacingSignal> {
        [
            (
                PacingSignal::SentenceLength,
                self.mean_sentence_length,
                options.minimum_sentence_length,
            ),
            (
                PacingSignal::PastPerfect,
                self.past_perfect_percent,
                options.minimum_past_perfect_percent,
            ),
            (
                PacingSignal::Cognition,
                self.cognition_percent,
                options.minimum_cognition_percent,
            ),
        ]
        .into_iter()
        .filter_map(|(signal, value, threshold)| value.filter(|v| *v >= threshold).map(|_| signal))
        .collect()
    }

    fn add(&mut self, other: &Self) {
        self.words.all += other.words.all;
        self.words.narration += other.words.narration;
        self.words.dialogue += other.words.dialogue;
        self.sentences += other.sentences;
        self.verb_phrases += other.verb_phrases;
        self.past_perfect += other.past_perfect;
        self.cognition += other.cognition;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PacingParagraph {
    pub span: Span,
    pub scene_break_before: bool,
    pub signals: PacingSignals,
    pub has_speech: bool,
    pub triggered_signals: Vec<PacingSignal>,
    pub slow: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PacingRun {
    pub span: Span,
    pub paragraphs: Range<usize>,
    pub signals: PacingSignals,
    pub triggered_signals: Vec<PacingSignal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PacingReport {
    pub words: Split<usize>,
    pub paragraphs: Vec<PacingParagraph>,
    pub slow_runs: Vec<PacingRun>,
}

fn tag(token: &Token, pos: &str) -> bool {
    token
        .readings
        .iter()
        .any(|r| r.pos.split(':').next() == Some(pos))
}

fn verb(token: &Token) -> bool {
    token.is_word
        && !token.is_skipped
        && token.readings.iter().any(|r| r.pos.starts_with("VB"))
        && (!token.chunks.iter().any(|c| c.contains("-NP"))
            || token.chunks.iter().any(|c| c.ends_with("-VP")))
}

fn cognition(token: &Token) -> bool {
    token
        .readings
        .iter()
        .filter(|r| r.pos.starts_with("VB"))
        .any(|r| {
            let candidate = fold(&r.lemma);
            COGNITION.lines().any(|lemma| lemma == candidate)
        })
}

fn next_verb(tokens: &[&Token], index: usize) -> Option<usize> {
    let mut next = index + 1;
    while tokens.get(next).is_some_and(|t| {
        !t.is_skipped && (tag(t, "RB") || tag(t, "RBR") || tag(t, "RBS")) && !verb(t)
    }) {
        next += 1;
    }
    tokens.get(next).filter(|t| verb(t)).map(|_| next)
}

fn sentence_verbs(tokens: &[Token], signals: &mut PacingSignals) {
    let tokens: Vec<_> = tokens.iter().filter(|t| !t.is_whitespace).collect();
    let mut index = 0;
    while index < tokens.len() {
        let first = tokens[index];
        if !verb(first) {
            index += 1;
            continue;
        }
        signals.verb_phrases += 1;
        let next = next_verb(&tokens, index);
        let surface = fold(&first.text);
        let had = surface == "had"
            && tag(first, "VBD")
            && first.readings.iter().any(|r| r.lemma == "have");
        let contracted = surface == "'d";
        let perfect = (had || contracted)
            && next
                .is_some_and(|n| tag(tokens[n], "VBN") && (!contracted || !tag(tokens[n], "VB")));
        signals.past_perfect += usize::from(perfect);
        let mut head = index;
        while let Some(next) = next_verb(&tokens, head) {
            let auxiliary = tokens[head].readings.iter().any(|r| {
                r.pos.starts_with("VB") && matches!(r.lemma.as_str(), "be" | "have" | "do")
            }) || (head == index && perfect);
            if !auxiliary {
                break;
            }
            head = next;
        }
        signals.cognition += usize::from(cognition(tokens[head]));
        index = head + 1;
    }
}

fn close(
    start: usize,
    end: usize,
    rows: &[PacingParagraph],
    options: &PacingOptions,
    runs: &mut Vec<PacingRun>,
) {
    if start == end {
        return;
    }
    let mut signals = PacingSignals::default();
    for row in &rows[start..end] {
        signals.add(&row.signals);
    }
    signals.finish();
    let triggered_signals = signals.triggered(options);
    if (end - start >= options.minimum_paragraphs || signals.words.all > options.more_than_words)
        && triggered_signals.len() >= 2
    {
        runs.push(PacingRun {
            span: Span::covering(&rows[start].span, &rows[end - 1].span),
            paragraphs: start..end,
            signals,
            triggered_signals,
        });
    }
}

pub fn report(document: &Document, options: &PacingOptions) -> Result<PacingReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details("pacing"));
    }
    options.validate()?;
    let mut paragraphs = Vec::new();
    let mut slow_runs = Vec::new();
    let mut start = 0;
    for paragraph in &document.paragraphs {
        let index = paragraphs.len();
        if paragraph.scene_break_before {
            close(start, index, &paragraphs, options, &mut slow_runs);
            start = index;
        }
        let mut signals = PacingSignals::default();
        let mut has_speech = false;
        for sentence in &paragraph.sentences {
            signals.sentences += 1;
            for token in sentence.words() {
                signals.words.all += 1;
                *signals
                    .words
                    .mode_mut(document.mode_at(token.span.byte.start)) += 1;
            }
            has_speech |= document.sentence_has_speech(sentence);
            sentence_verbs(&sentence.tokens, &mut signals);
        }
        signals.finish();
        let triggered_signals = signals.triggered(options);
        let slow = !has_speech && signals.words.all > 0 && triggered_signals.len() >= 2;
        paragraphs.push(PacingParagraph {
            span: paragraph.span.clone(),
            scene_break_before: paragraph.scene_break_before,
            signals,
            has_speech,
            triggered_signals,
            slow,
        });
        if !slow {
            close(start, index, &paragraphs, options, &mut slow_runs);
            start = index + 1;
        }
    }
    close(
        start,
        paragraphs.len(),
        &paragraphs,
        options,
        &mut slow_runs,
    );
    Ok(PacingReport {
        words: word_split(document),
        paragraphs,
        slow_runs,
    })
}
