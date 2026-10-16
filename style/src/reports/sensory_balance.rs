use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::StyleError;
use crate::reports::{norms, word_split, Split};
use crate::{Document, Mode, Span, Token};

pub(crate) const REPORT: &str = "sensoryBalance";

const NORMS: &str = include_str!("../../data/norms/lancaster-perceptual.tsv");
const NOT_SENSORY: &str = include_str!("../../data/norms/not-sensory.txt");

fn not_sensory(lemma: &str) -> bool {
    NOT_SENSORY.lines().any(|line| line == lemma)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SensoryModality {
    Sight,
    Sound,
    Touch,
    Smell,
    Taste,
    Interoception,
}

impl SensoryModality {
    pub const ALL: [Self; 6] = [
        Self::Sight,
        Self::Sound,
        Self::Touch,
        Self::Smell,
        Self::Taste,
        Self::Interoception,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct PerceptualNorm {
    pub modality: SensoryModality,
    pub strength: f64,
    pub margin: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PerceptualNorms {
    words: HashMap<String, PerceptualNorm>,
}

impl PerceptualNorms {
    pub fn bundled() -> &'static Self {
        static BUNDLED: OnceLock<PerceptualNorms> = OnceLock::new();
        BUNDLED.get_or_init(|| Self::from_tsv(NORMS).expect("bundled norms parse"))
    }

    pub fn from_tsv(text: &str) -> Result<Self, StyleError> {
        let mut words = HashMap::new();
        for (number, line) in text.lines().enumerate() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let invalid = || {
                StyleError::table(
                    REPORT,
                    format!(
                        "line {} is not word, modality letter, strength and margin in tenths",
                        number + 1
                    ),
                )
            };
            let fields: Vec<_> = line.split('\t').collect();
            let [word, letter, strength, margin] = fields[..] else {
                return Err(invalid());
            };
            let modality = match letter {
                "v" => SensoryModality::Sight,
                "a" => SensoryModality::Sound,
                "h" => SensoryModality::Touch,
                "o" => SensoryModality::Smell,
                "g" => SensoryModality::Taste,
                "i" => SensoryModality::Interoception,
                _ => return Err(invalid()),
            };
            let tenths = |field: &str| {
                field
                    .parse::<u8>()
                    .ok()
                    .filter(|value| *value <= 50)
                    .map(|value| f64::from(value) / 10.0)
                    .ok_or_else(invalid)
            };
            words.insert(
                crate::reports::echoes::fold(word),
                PerceptualNorm {
                    modality,
                    strength: tenths(strength)?,
                    margin: tenths(margin)?,
                },
            );
        }
        Ok(Self { words })
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn get(&self, word: &str) -> Option<&PerceptualNorm> {
        self.words.get(word)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SensoryBalanceOptions {
    #[serde(alias = "minimum_strength")]
    pub minimum_strength: f64,
    #[serde(alias = "dominance_margin")]
    pub dominance_margin: f64,
    #[serde(alias = "sight_margin")]
    pub sight_margin: f64,
    #[serde(alias = "minimum_sense_words")]
    pub minimum_sense_words: usize,
    #[serde(alias = "single_sense_percent")]
    pub single_sense_percent: f64,
}

impl Default for SensoryBalanceOptions {
    fn default() -> Self {
        Self {
            minimum_strength: 4.0,
            dominance_margin: 1.0,
            sight_margin: 3.0,
            minimum_sense_words: 20,
            single_sense_percent: 85.0,
        }
    }
}

impl SensoryBalanceOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_strength(mut self, strength: f64) -> Self {
        self.minimum_strength = strength;
        self
    }
    pub fn with_dominance_margin(mut self, margin: f64) -> Self {
        self.dominance_margin = margin;
        self
    }
    pub fn with_sight_margin(mut self, margin: f64) -> Self {
        self.sight_margin = margin;
        self
    }
    pub fn with_minimum_sense_words(mut self, words: usize) -> Self {
        self.minimum_sense_words = words;
        self
    }
    pub fn with_single_sense_percent(mut self, percent: f64) -> Self {
        self.single_sense_percent = percent;
        self
    }

    fn validate(&self) -> Result<(), StyleError> {
        if !(self.minimum_strength.is_finite() && (0.0..=5.0).contains(&self.minimum_strength)) {
            return Err(StyleError::options("minimumStrength must be in 0..=5"));
        }
        for (name, margin) in [
            ("dominanceMargin", self.dominance_margin),
            ("sightMargin", self.sight_margin),
        ] {
            if !(margin.is_finite() && (0.0..=5.0).contains(&margin)) {
                return Err(StyleError::options(format!("{name} must be in 0..=5")));
            }
        }
        if !(self.single_sense_percent.is_finite()
            && self.single_sense_percent > 0.0
            && self.single_sense_percent <= 100.0)
        {
            return Err(StyleError::options(
                "singleSensePercent must be a percentage in (0, 100]",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SenseWord {
    pub span: Span,
    pub lemma: String,
    pub modality: SensoryModality,
    pub strength: f64,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SensoryShare {
    pub modality: SensoryModality,
    pub count: usize,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SensoryGroup {
    pub words: usize,
    pub sense_words: usize,
    pub modalities: Vec<SensoryShare>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SensoryUnit {
    pub span: Span,
    pub chapter: usize,
    pub groups: Split<SensoryGroup>,
    pub single_sense: Option<SensoryModality>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SensoryBalanceReport {
    pub words: Split<usize>,
    pub sense_words: Vec<SenseWord>,
    pub groups: Split<SensoryGroup>,
    pub chapters: Vec<SensoryUnit>,
    pub scenes: Vec<SensoryUnit>,
}

#[derive(Default, Clone)]
struct Tally {
    words: usize,
    counts: [usize; 6],
}

impl Tally {
    fn group(&self, minimum: usize) -> SensoryGroup {
        let sense_words = self.counts.iter().sum();
        SensoryGroup {
            words: self.words,
            sense_words,
            modalities: SensoryModality::ALL
                .iter()
                .zip(self.counts)
                .map(|(&modality, count)| SensoryShare {
                    modality,
                    count,
                    percent: (sense_words > 0 && sense_words >= minimum)
                        .then(|| 100.0 * count as f64 / sense_words as f64),
                })
                .collect(),
        }
    }
}

struct Unit {
    first: Span,
    last: Span,
    chapter: usize,
    tally: Split<Tally>,
}

impl Unit {
    fn finish(self, options: &SensoryBalanceOptions) -> SensoryUnit {
        let narration = &self.tally.narration;
        let sense_words: usize = narration.counts.iter().sum();
        let single_sense = (sense_words >= options.minimum_sense_words.max(1))
            .then(|| {
                SensoryModality::ALL
                    .iter()
                    .zip(narration.counts)
                    .max_by_key(|(_, count)| *count)
                    .filter(|(_, count)| {
                        100.0 * *count as f64 / sense_words as f64 >= options.single_sense_percent
                    })
                    .map(|(&modality, _)| modality)
            })
            .flatten();
        SensoryUnit {
            span: Span::covering(&self.first, &self.last),
            chapter: self.chapter,
            groups: self
                .tally
                .map(|tally| tally.group(options.minimum_sense_words)),
            single_sense,
        }
    }
}

fn classify(norm: &PerceptualNorm, options: &SensoryBalanceOptions) -> bool {
    let margin = if norm.modality == SensoryModality::Sight {
        options.sight_margin
    } else {
        options.dominance_margin
    };
    norm.strength >= options.minimum_strength && norm.margin >= margin
}

fn figurative(lemma: &str, next: Option<&Token>) -> bool {
    matches!(lemma, "see" | "feel")
        && next.is_some_and(|next| {
            let word = crate::reports::echoes::fold(&next.text);
            matches!(
                word.as_str(),
                "that" | "if" | "whether" | "how" | "why" | "what" | "like" | "as"
            ) || (lemma == "feel"
                && next.readings.iter().any(|r| r.pos.starts_with("JJ"))
                && !next.readings.iter().any(|r| r.pos.starts_with("NN")))
        })
}

pub fn report(
    document: &Document,
    norms_table: &PerceptualNorms,
    options: &SensoryBalanceOptions,
) -> Result<SensoryBalanceReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    options.validate()?;
    let mut sense_words = Vec::new();
    let mut total: Split<Tally> = Split::default();
    let mut chapters = Vec::new();
    let mut scenes = Vec::new();
    let mut chapter: Option<Unit> = None;
    let mut scene: Option<Unit> = None;
    let mut chapter_index = 0;
    for paragraph in &document.paragraphs {
        if let Some(unit) = chapter.take_if(|_| paragraph.chapter_start) {
            chapters.push(unit.finish(options));
            chapter_index += 1;
        }
        if paragraph.scene_break_before || paragraph.chapter_start {
            scenes.extend(scene.take().map(|unit| unit.finish(options)));
        }
        for unit in [&mut chapter, &mut scene] {
            let unit = unit.get_or_insert_with(|| Unit {
                first: paragraph.span.clone(),
                last: paragraph.span.clone(),
                chapter: chapter_index,
                tally: Split::default(),
            });
            unit.last = paragraph.span.clone();
        }
        let words: Vec<_> = paragraph.sentences.iter().flat_map(norms::words).collect();
        for (index, &(token, at_start)) in words.iter().enumerate() {
            let mode = document.mode_at(token.span.byte.start);
            let next = words
                .get(index + 1)
                .map(|(next, _)| *next)
                .filter(|next| next.span.byte.start <= token.span.byte.end + 1);
            let sense = norms::lookup(token, at_start, |word| norms_table.words.get(word))
                .filter(|found| {
                    !not_sensory(&found.lemma)
                        && !figurative(&found.lemma, next)
                        && classify(found.value, options)
                })
                .map(|found| (found.lemma, (found.value.modality, found.value.strength)));
            for tally in [
                &mut total,
                &mut chapter.as_mut().unwrap().tally,
                &mut scene.as_mut().unwrap().tally,
            ] {
                for group in tally.all_and_mode_mut(mode) {
                    group.words += 1;
                    if let Some((_, (modality, _))) = &sense {
                        group.counts[*modality as usize] += 1;
                    }
                }
            }
            if let Some((lemma, (modality, strength))) = sense {
                sense_words.push(SenseWord {
                    span: token.span.clone(),
                    lemma,
                    modality,
                    strength,
                    mode,
                });
            }
        }
    }
    chapters.extend(chapter.map(|unit| unit.finish(options)));
    scenes.extend(scene.map(|unit| unit.finish(options)));
    let words = word_split(document);
    Ok(SensoryBalanceReport {
        words,
        sense_words,
        groups: total.map(|tally| tally.group(options.minimum_sense_words)),
        chapters,
        scenes,
    })
}
