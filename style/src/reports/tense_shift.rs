//! The tense-shift lens (#355): narration sentences that leave a scene's tense. Opt-in and
//! narration-only; it diagnoses and never rewrites. See docs/STYLE-TENSE-SHIFT.md.
//!
//! Each sentence is classified past, present, mixed or unclassified by its main-clause finite
//! chains with explicit subjects (`narrative::sentence_tense`). A scene's tense is declared,
//! inferred from its own votes, or inherited from a strongly one-tense document. Two or more
//! consecutive minority-tense sentences are a run (bridging at most two neutral sentences); a
//! single sentence is flagged only with `includeSingles`, and past in a present scene only with
//! `includeReverse`. A present run that ends a past scene beside *now* or *still* is the
//! narrator's coda and is counted, not flagged.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::narrative::{
    scene_votes, scenes, tense_tally, NarrativeTense, SentenceTense, SentenceVote, TenseThresholds,
};
use crate::{Document, Sentence, Span, StyleError};

pub use crate::narrative::TenseExemption;

pub(crate) const REPORT: &str = "tenseShift";

/// A whole document must reach this share over at least `INHERIT_SENTENCES` classified sentences
/// before a short scene inherits its tense.
const INHERIT_PERCENT: f64 = 90.0;
const INHERIT_SENTENCES: usize = 30;
/// A reverse run (past in a present scene) needs at least this many sentences.
const REVERSE_RUN: usize = 3;
/// A run bridges at most this many neutral sentences (unclassified sentences, dialogue tags in
/// the scene tense, backstory past perfect) between two minority sentences. A longer gap, such
/// as an exchange of tagged dialogue, splits it.
const MAX_GAP: usize = 2;

/// A declared tense for one scene, by its index in the report's `scenes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SceneTense {
    pub scene: usize,
    pub tense: NarrativeTense,
}

impl SceneTense {
    pub fn new(scene: usize, tense: NarrativeTense) -> Self {
        Self { scene, tense }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShiftOptions {
    /// The document's declared tense; `None` infers each scene's.
    pub tense: Option<NarrativeTense>,
    /// Per-scene declarations; they beat `tense`.
    #[serde(alias = "scene_tenses")]
    pub scene_tenses: Vec<SceneTense>,
    #[serde(alias = "minimum_scene_sentences")]
    pub minimum_scene_sentences: usize,
    /// Validated 50 < x <= 100.
    #[serde(alias = "minimum_majority_percent")]
    pub minimum_majority_percent: f64,
    /// Validated >= 2.
    #[serde(alias = "minimum_run")]
    pub minimum_run: usize,
    #[serde(alias = "include_singles")]
    pub include_singles: bool,
    #[serde(alias = "include_reverse")]
    pub include_reverse: bool,
    #[serde(alias = "inherit_document_tense")]
    pub inherit_document_tense: bool,
}

impl Default for TenseShiftOptions {
    fn default() -> Self {
        Self {
            tense: None,
            scene_tenses: Vec::new(),
            minimum_scene_sentences: 6,
            minimum_majority_percent: 70.0,
            minimum_run: 2,
            include_singles: false,
            include_reverse: false,
            inherit_document_tense: true,
        }
    }
}

impl TenseShiftOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_tense(mut self, tense: Option<NarrativeTense>) -> Self {
        self.tense = tense;
        self
    }
    pub fn with_scene_tenses(mut self, scene_tenses: Vec<SceneTense>) -> Self {
        self.scene_tenses = scene_tenses;
        self
    }
    pub fn with_minimum_scene_sentences(mut self, value: usize) -> Self {
        self.minimum_scene_sentences = value;
        self
    }
    pub fn with_minimum_majority_percent(mut self, value: f64) -> Self {
        self.minimum_majority_percent = value;
        self
    }
    pub fn with_minimum_run(mut self, value: usize) -> Self {
        self.minimum_run = value;
        self
    }
    pub fn with_include_singles(mut self, value: bool) -> Self {
        self.include_singles = value;
        self
    }
    pub fn with_include_reverse(mut self, value: bool) -> Self {
        self.include_reverse = value;
        self
    }
    pub fn with_inherit_document_tense(mut self, value: bool) -> Self {
        self.inherit_document_tense = value;
        self
    }

    fn validate(&self, scenes: usize) -> Result<(), StyleError> {
        let percent = self.minimum_majority_percent;
        if !percent.is_finite() || percent <= 50.0 || percent > 100.0 {
            return Err(StyleError::options(
                "minimumMajorityPercent must be above 50 and at most 100",
            ));
        }
        if self.minimum_run < 2 {
            return Err(StyleError::options("minimumRun must be at least 2"));
        }
        if self.minimum_scene_sentences == 0 {
            return Err(StyleError::options(
                "minimumSceneSentences must be positive",
            ));
        }
        if let Some(declared) = self.scene_tenses.iter().find(|d| d.scene >= scenes) {
            return Err(StyleError::options(format!(
                "sceneTenses names scene {} but the document has {scenes}",
                declared.scene
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TenseShiftStrength {
    /// `minimumRun` or more minority-tense sentences, at most two neutral sentences apart.
    Run,
    /// One sentence, with `includeSingles`.
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TenseSource {
    Declared,
    Inferred,
    Inherited,
    Undetermined,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShiftVotes {
    pub past: usize,
    pub present: usize,
    pub mixed: usize,
    pub unclassified: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShiftScene {
    pub scene: usize,
    pub span: Span,
    pub paragraphs: Range<usize>,
    /// Global sentence indexes.
    pub sentences: Range<usize>,
    pub tense: Option<NarrativeTense>,
    pub source: TenseSource,
    pub votes: TenseShiftVotes,
    pub narration_words: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShift {
    /// From the first to the last minority sentence.
    pub span: Span,
    /// Global sentence indexes.
    pub sentences: Range<usize>,
    /// The voting verb heads of the minority sentences.
    pub verbs: Vec<Span>,
    pub scene: usize,
    pub expected: NarrativeTense,
    pub found: NarrativeTense,
    pub strength: TenseShiftStrength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShiftExemptionCount {
    pub reason: TenseExemption,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TenseShiftReport {
    pub scenes: Vec<TenseShiftScene>,
    pub shifts: Vec<TenseShift>,
    pub narration_words: usize,
    /// Shifts per 1,000 narration words; `None` without narration.
    pub per_thousand_words: Option<f64>,
    /// Chains and sentences that did not vote, by reason; zero counts are left out.
    pub exemptions: Vec<TenseShiftExemptionCount>,
}

fn other(tense: NarrativeTense) -> NarrativeTense {
    match tense {
        NarrativeTense::Past => NarrativeTense::Present,
        NarrativeTense::Present => NarrativeTense::Past,
    }
}

fn class_of(tense: NarrativeTense) -> SentenceTense {
    match tense {
        NarrativeTense::Past => SentenceTense::Past,
        NarrativeTense::Present => SentenceTense::Present,
    }
}

/// From the first non-whitespace token of the first sentence to the last of the last.
fn sentences_span(all: &[&Sentence], sentences: &Range<usize>) -> Span {
    let mut tokens = all[sentences.clone()]
        .iter()
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| !token.is_whitespace);
    let first = tokens.next().expect("a flagged sentence has tokens");
    let last = tokens.next_back().unwrap_or(first);
    Span::covering(&first.span, &last.span)
}

#[derive(PartialEq, Eq)]
enum Step {
    Minority,
    Neutral,
    End,
}

struct Flagger<'a> {
    /// Every sentence in document order, so a run's sentences are found by index.
    sentences: &'a [&'a Sentence],
    options: &'a TenseShiftOptions,
    scene: usize,
    tense: NarrativeTense,
}

impl Flagger<'_> {
    fn step(&self, vote: &SentenceVote, exemptions: &mut BTreeMap<TenseExemption, usize>) -> Step {
        match vote.class {
            SentenceTense::Unclassified => Step::Neutral,
            SentenceTense::Mixed => Step::End,
            // A dialogue tag in the scene tense does not end a run (*The room is dark. “Hello?”
            // she called. The air is cold.*): tags hold the habitual tense while narration slips.
            class if class == class_of(self.tense) => {
                if vote.tag_only {
                    Step::Neutral
                } else {
                    Step::End
                }
            }
            // Past perfect in a present scene is backstory.
            SentenceTense::Past if vote.past_perfect_only => {
                *exemptions.entry(TenseExemption::PastPerfect).or_default() += 1;
                Step::Neutral
            }
            _ => Step::Minority,
        }
    }

    /// `last_end` is the scene's last sentence in its own tense (`Step::End`): a forward run after
    /// it ends the scene.
    fn close(
        &self,
        run: &[usize],
        votes: &[(usize, SentenceVote)],
        last_end: Option<usize>,
        exemptions: &mut BTreeMap<TenseExemption, usize>,
        shifts: &mut Vec<TenseShift>,
    ) {
        let (Some(&first), Some(&last)) = (run.first(), run.last()) else {
            return;
        };
        let reverse = self.tense == NarrativeTense::Present;
        // The present-day coda of a retrospective past narrative (*The motel is still here. So
        // are we.*): a present run after the scene's last past sentence, beside *now* or *still*.
        let trailing = last_end.is_some_and(|end| first > end);
        if !reverse && trailing && run.iter().any(|&k| votes[k].1.narrator_now) {
            *exemptions.entry(TenseExemption::Coda).or_default() += run.len();
            return;
        }
        let strength = if reverse {
            let anchored = run
                .iter()
                .copied()
                .chain(first.checked_sub(1))
                .any(|k| votes[k].1.backstory);
            if !self.options.include_reverse
                || run.len() < self.options.minimum_run.max(REVERSE_RUN)
                || anchored
            {
                return;
            }
            TenseShiftStrength::Run
        } else if run.len() >= self.options.minimum_run {
            TenseShiftStrength::Run
        } else if run.len() > 1 {
            // Shorter than `minimumRun` but more than one sentence: neither a run nor a single.
            return;
        } else {
            let vote = &votes[first].1;
            let anchored = votes[..first]
                .iter()
                .rev()
                .filter(|(_, vote)| {
                    matches!(vote.class, SentenceTense::Past | SentenceTense::Present)
                })
                .take(2)
                .any(|(_, vote)| vote.class == class_of(self.tense));
            if !(self.options.include_singles && vote.specific && !vote.gnomic && anchored) {
                return;
            }
            TenseShiftStrength::Single
        };
        let sentences = votes[first].0..votes[last].0 + 1;
        let mut verbs = Vec::new();
        for &k in run {
            let (index, vote) = &votes[k];
            let sentence = self.sentences[*index];
            verbs.extend(vote.voting.iter().map(|&t| sentence.tokens[t].span.clone()));
        }
        shifts.push(TenseShift {
            span: sentences_span(self.sentences, &sentences),
            sentences,
            verbs,
            scene: self.scene,
            expected: self.tense,
            found: other(self.tense),
            strength,
        });
    }

    fn flag(
        &self,
        votes: &[(usize, SentenceVote)],
        exemptions: &mut BTreeMap<TenseExemption, usize>,
        shifts: &mut Vec<TenseShift>,
    ) {
        let steps: Vec<Step> = votes
            .iter()
            .map(|(_, vote)| self.step(vote, exemptions))
            .collect();
        let last_end = steps.iter().rposition(|step| *step == Step::End);
        let mut run: Vec<usize> = Vec::new();
        let mut gap = 0;
        for (k, step) in steps.iter().enumerate() {
            match step {
                Step::Minority => {
                    if gap > MAX_GAP {
                        self.close(&run, votes, last_end, exemptions, shifts);
                        run.clear();
                    }
                    run.push(k);
                    gap = 0;
                }
                Step::Neutral => gap += 1,
                Step::End => {
                    self.close(&run, votes, last_end, exemptions, shifts);
                    run.clear();
                    gap = 0;
                }
            }
        }
        self.close(&run, votes, last_end, exemptions, shifts);
    }
}

pub fn report(
    document: &Document,
    options: &TenseShiftOptions,
) -> Result<TenseShiftReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let scenes = scenes(document);
    options.validate(scenes.len())?;
    let thresholds = TenseThresholds {
        minimum_sentences: options.minimum_scene_sentences,
        minimum_majority_percent: options.minimum_majority_percent,
    };
    let votes: Vec<Vec<(usize, SentenceVote)>> = scenes
        .iter()
        .map(|scene| scene_votes(document, scene))
        .collect();
    let (past, present) = votes.iter().fold((0, 0), |(past, present), scene| {
        let (p, q) = tense_tally(scene);
        (past + p, present + q)
    });
    let document_tense = TenseThresholds {
        minimum_sentences: INHERIT_SENTENCES,
        minimum_majority_percent: INHERIT_PERCENT,
    }
    .infer(past, present);

    let all: Vec<&Sentence> = document.sentences().collect();
    let mut exemptions: BTreeMap<TenseExemption, usize> = BTreeMap::new();
    let mut shifts = Vec::new();
    let mut records = Vec::new();
    let mut narration_words = 0;
    for (scene, votes) in scenes.iter().zip(&votes) {
        let mut tally = TenseShiftVotes::default();
        for (_, vote) in votes {
            for reason in &vote.exemptions {
                *exemptions.entry(*reason).or_default() += 1;
            }
            match vote.class {
                SentenceTense::Past => tally.past += 1,
                SentenceTense::Present => tally.present += 1,
                SentenceTense::Mixed => tally.mixed += 1,
                SentenceTense::Unclassified => tally.unclassified += 1,
            }
        }
        let declared = options
            .scene_tenses
            .iter()
            .rev()
            .find(|declared| declared.scene == scene.index)
            .map(|declared| declared.tense)
            .or(options.tense);
        let classified = tally.past + tally.present;
        let (tense, source) = if let Some(tense) = declared {
            (Some(tense), TenseSource::Declared)
        } else if let Some(tense) = thresholds.infer(tally.past, tally.present) {
            (Some(tense), TenseSource::Inferred)
        } else {
            let inherited = document_tense.filter(|&tense| {
                let (same, opposite) = match tense {
                    NarrativeTense::Past => (tally.past, tally.present),
                    NarrativeTense::Present => (tally.present, tally.past),
                };
                options.inherit_document_tense
                    && classified < options.minimum_scene_sentences
                    && opposite <= same
            });
            match inherited {
                Some(tense) => (Some(tense), TenseSource::Inherited),
                None => (None, TenseSource::Undetermined),
            }
        };
        if let Some(tense) = tense {
            Flagger {
                sentences: &all,
                options,
                scene: scene.index,
                tense,
            }
            .flag(votes, &mut exemptions, &mut shifts);
        }
        let words: usize = scene
            .paragraphs
            .clone()
            .flat_map(|paragraph| &document.paragraphs[paragraph].sentences)
            .map(|sentence| document.sentence_narration_words(sentence).count())
            .sum();
        narration_words += words;
        records.push(TenseShiftScene {
            scene: scene.index,
            span: scene.span.clone(),
            paragraphs: scene.paragraphs.clone(),
            sentences: scene.sentences.clone(),
            tense,
            source,
            votes: tally,
            narration_words: words,
        });
    }
    let per_thousand_words =
        (narration_words > 0).then(|| 1000.0 * shifts.len() as f64 / narration_words as f64);
    Ok(TenseShiftReport {
        scenes: records,
        shifts,
        narration_words,
        per_thousand_words,
        exemptions: exemptions
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(reason, count)| TenseShiftExemptionCount { reason, count })
            .collect(),
    })
}
