use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{sentence_starts, PercentileBand, Split};
use crate::{Document, Mode, Sentence, Span, StyleError, Token};

pub(crate) const REPORT: &str = "passive";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PassiveOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
}
impl Default for PassiveOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
        }
    }
}
impl PassiveOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, value: usize) -> Self {
        self.minimum_sentences = value;
        self
    }
}

/// Per-mode p5–p95 bands for passives per 100 sentences.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PassiveBaseline {
    pub bands: Split<Option<PercentileBand>>,
}
impl PassiveBaseline {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_band(mut self, mode: Option<Mode>, band: PercentileBand) -> Self {
        match mode {
            None => self.bands.all = Some(band),
            Some(mode) => *self.bands.mode_mut(mode) = Some(band),
        }
        self
    }
    pub fn bundled() -> Option<Self> {
        static BUNDLED: OnceLock<PassiveBaseline> = OnceLock::new();
        Some(
            BUNDLED
                .get_or_init(|| {
                    super::bundled_baseline(include_str!("../../data/baselines/passive.json"))
                })
                .clone(),
        )
    }
    pub fn validate(&self) -> Result<(), StyleError> {
        for band in [&self.bands.all, &self.bands.narration, &self.bands.dialogue]
            .into_iter()
            .flatten()
        {
            band.validate(REPORT, f64::MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PassiveAuxiliary {
    Be,
    Get,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PassiveMatch {
    pub span: Span,
    pub participle: Span,
    pub sentence: usize,
    pub auxiliary: PassiveAuxiliary,
    pub progressive: bool,
    pub agent: bool,
    pub mode: Mode,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PassiveGroup {
    pub sentences: usize,
    pub low_confidence: bool,
    pub passives: usize,
    pub per_hundred_sentences: Option<f64>,
    pub band: Option<PercentileBand>,
    pub highlighted: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PassiveReport {
    pub passives: Vec<PassiveMatch>,
    pub groups: Split<PassiveGroup>,
}

/// Participles read as adjectives after *be*/*get* unless a *by*-agent follows.
const ADJECTIVAL: [&str; 57] = [
    "accustomed",
    "acquainted",
    "alarmed",
    "amazed",
    "amused",
    "annoyed",
    "ashamed",
    "astonished",
    "bored",
    "complicated",
    "concerned",
    "confused",
    "convinced",
    "delighted",
    "depressed",
    "determined",
    "devoted",
    "disappointed",
    "disgusted",
    "distracted",
    "dressed",
    "drunk",
    "embarrassed",
    "engaged",
    "exhausted",
    "excited",
    "fascinated",
    "frightened",
    "frustrated",
    "horrified",
    "impressed",
    "interested",
    "involved",
    "irritated",
    "lost",
    "married",
    "obliged",
    "obsessed",
    "offended",
    "overwhelmed",
    "pleased",
    "prepared",
    "puzzled",
    "related",
    "relieved",
    "satisfied",
    "scared",
    "shocked",
    "startled",
    "stunned",
    "surprised",
    "terrified",
    "thrilled",
    "tired",
    "upset",
    "wet",
    "worried",
];
const DEGREE: [&str; 16] = [
    "very",
    "so",
    "too",
    "quite",
    "rather",
    "extremely",
    "incredibly",
    "terribly",
    "awfully",
    "fairly",
    "pretty",
    "more",
    "most",
    "less",
    "least",
    "somewhat",
];
const NEGATION: [&str; 3] = ["not", "n't", "never"];
/// Tagged RB but heads a noun phrase (`there was nothing left`).
const INDEFINITE: [&str; 8] = [
    "nothing",
    "something",
    "anything",
    "everything",
    "nobody",
    "somebody",
    "anybody",
    "everybody",
];
const OBJECTS: [&str; 7] = ["me", "him", "her", "us", "them", "you", "it"];
const SUBJECTS: [&str; 7] = ["i", "you", "he", "she", "it", "we", "they"];
const PERSONAL: [&str; 6] = ["i", "you", "he", "she", "we", "they"];

fn folded(token: &Token) -> String {
    super::echoes::fold(&token.text)
}
fn has(token: &Token, tags: &[&str]) -> bool {
    token
        .readings
        .iter()
        .any(|r| tags.contains(&r.pos.as_str()))
}
fn auxiliary(token: &Token) -> Option<PassiveAuxiliary> {
    let verb = |lemma: &str| {
        token
            .readings
            .iter()
            .any(|r| r.lemma == lemma && r.pos.starts_with("VB"))
    };
    if verb("be") {
        Some(PassiveAuxiliary::Be)
    } else if verb("get") {
        Some(PassiveAuxiliary::Get)
    } else {
        None
    }
}
fn adverb(token: &Token) -> bool {
    let word = folded(token);
    NEGATION.contains(&word.as_str())
        || (has(token, &["RB", "RBR", "RBS"])
            && !has(token, &["VBN"])
            && !INDEFINITE.contains(&word.as_str()))
}
fn contraction(token: &Token) -> bool {
    matches!(folded(token).as_str(), "'s" | "'re" | "'m")
}

/// Finds a passive whose auxiliary chain starts at `at`, returning it and the participle index.
fn passive_at(
    tokens: &[&Token],
    at: usize,
    sentence: &Sentence,
) -> Option<(PassiveAuxiliary, bool, bool, usize)> {
    let mut kind = auxiliary(tokens[at])?;
    let opening = at == 0;
    let mut progressive = false;
    let mut next = at + 1;
    let mut skipped = 0;
    let mut subject = false;
    let mut degree = false;
    while let Some(&token) = tokens.get(next) {
        let gap = sentence.text.get(
            tokens[next - 1].span.byte.end - sentence.span.byte.start
                ..token.span.byte.start - sentence.span.byte.start,
        );
        if !gap.is_some_and(|gap| gap.chars().all(char::is_whitespace)) {
            return None;
        }
        if let Some(chained) = auxiliary(token).filter(|_| has(token, &["VBG", "VBN"])) {
            progressive |= has(token, &["VBG"]);
            kind = chained;
        } else if has(token, &["VBN"]) {
            break;
        } else if opening && !subject && SUBJECTS.contains(&folded(token).as_str()) {
            subject = true;
        } else if adverb(token) && skipped < 3 {
            skipped += 1;
            degree = DEGREE.contains(&folded(token).as_str());
        } else {
            return None;
        }
        next += 1;
    }
    let participle = tokens.get(next)?;
    if degree {
        return None;
    }
    let word = folded(participle);
    let after = tokens.get(next + 1).map(|t| folded(t));
    let following = tokens.get(next + 2);
    let agent = after.as_deref() == Some("by");
    let personal = at > 0 && PERSONAL.contains(&folded(tokens[at - 1]).as_str());
    let clause_end = tokens
        .get(next + 1)
        .is_none_or(|t| !t.is_word || after.as_deref() == Some("with"));
    let adjectival = match word.as_str() {
        "gone" | "come" | "arrived" => true,
        "used" | "supposed" => {
            after.as_deref() == Some("to")
                && (word == "supposed" || following.is_some_and(|t| !has(t, &["VB"])))
        }
        "done" | "finished" => personal && clause_end,
        _ => ADJECTIVAL.contains(&word.as_str()) && !agent,
    };
    // `’s` + participle + object is usually *has* (perfect), not a passive.
    let perfect = contraction(tokens[at])
        && next == at + 1
        && tokens
            .get(next + 1)
            .is_some_and(|t| t.is_word && !has(t, &["IN", "TO", "RB", "RP"]));
    // A base form before an object is a bare infinitive (`all you can do is let him go`).
    let infinitive = participle
        .readings
        .iter()
        .any(|r| r.lemma == word && r.pos.starts_with("VB"))
        && after.as_deref().is_some_and(|word| OBJECTS.contains(&word));
    (!adjectival && !perfect && !infinitive).then_some((kind, progressive, agent, next))
}

pub fn report(
    document: &Document,
    options: &PassiveOptions,
    baseline: Option<&PassiveBaseline>,
) -> Result<PassiveReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let mut passives = Vec::new();
    let mut groups: Split<PassiveGroup> = Split::default();
    for (index, sentence) in document.sentences().enumerate() {
        if sentence.word_count() == 0 {
            continue;
        }
        let mode = document.sentence_mode(sentence);
        for group in groups.all_and_mode_mut(mode) {
            group.sentences += 1;
        }
        let tokens = sentence_starts::opening_tokens(sentence);
        let mut at = 0;
        while at < tokens.len() {
            if tokens[at].is_skipped {
                at += 1;
                continue;
            }
            match passive_at(&tokens, at, sentence) {
                Some((auxiliary, progressive, agent, participle)) => {
                    passives.push(PassiveMatch {
                        span: Span::covering(&tokens[at].span, &tokens[participle].span),
                        participle: tokens[participle].span.clone(),
                        sentence: index,
                        auxiliary,
                        progressive,
                        agent,
                        mode,
                    });
                    for group in groups.all_and_mode_mut(mode) {
                        group.passives += 1;
                    }
                    at = participle + 1;
                }
                None => at += 1,
            }
        }
    }
    for (mode, group) in [
        (None, &mut groups.all),
        (Some(Mode::Narration), &mut groups.narration),
        (Some(Mode::Dialogue), &mut groups.dialogue),
    ] {
        let enough = group.sentences >= options.minimum_sentences && group.sentences > 0;
        group.low_confidence = group.sentences < 200.max(options.minimum_sentences);
        group.per_hundred_sentences =
            enough.then(|| 100.0 * group.passives as f64 / group.sentences as f64);
        group.band = baseline.and_then(|baseline| match mode {
            None => baseline.bands.all.clone(),
            Some(mode) => baseline.bands.mode(mode).clone(),
        });
        group.highlighted =
            PercentileBand::highlights(group.band.as_ref(), group.per_hundred_sentences);
    }
    Ok(PassiveReport { passives, groups })
}
