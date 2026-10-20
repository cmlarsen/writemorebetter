use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::StyleError;
use crate::reports::{norms, word_split, SentenceRun, Split};
use crate::{Document, Mode, Span};

pub(crate) const REPORT: &str = "diction";
const BUNDLED: &str = include_str!("../../data/norms/concreteness.tsv");

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConcretenessTable {
    ratings: HashMap<String, f64>,
}

impl ConcretenessTable {
    pub fn bundled() -> &'static Self {
        static TABLE: OnceLock<ConcretenessTable> = OnceLock::new();
        TABLE.get_or_init(|| Self::from_tsv(BUNDLED).expect("bundled concreteness table parses"))
    }

    pub fn new(ratings: impl IntoIterator<Item = (String, f64)>) -> Result<Self, StyleError> {
        let mut table = HashMap::new();
        for (word, rating) in ratings {
            let word = crate::reports::echoes::fold(word.trim());
            if word.is_empty() {
                return Err(StyleError::table(REPORT, "a table entry has an empty word"));
            }
            if !rating.is_finite() {
                return Err(StyleError::table(
                    REPORT,
                    format!("the rating for {word:?} is not a finite number"),
                ));
            }
            table.insert(word, rating);
        }
        if table.is_empty() {
            return Err(StyleError::table(REPORT, "the table has no entries"));
        }
        Ok(Self { ratings: table })
    }

    pub fn parse(text: &str) -> Result<Self, StyleError> {
        if text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .starts_with('{')
        {
            Self::from_json(text)
        } else {
            Self::from_tsv(text)
        }
    }

    pub fn from_json(text: &str) -> Result<Self, StyleError> {
        let ratings: HashMap<String, f64> =
            serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .map_err(|error| StyleError::table(REPORT, error))?;
        Self::new(ratings)
    }

    pub fn from_tsv(text: &str) -> Result<Self, StyleError> {
        let mut lines = text
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'));
        let mut columns = (0, 1);
        let mut rows = Vec::new();
        if let Some((number, first)) = lines.next() {
            let header: Vec<_> = first.split('\t').map(str::trim).collect();
            let word = header.iter().position(|c| c.eq_ignore_ascii_case("word"));
            let rating = header.iter().position(|c| {
                c.eq_ignore_ascii_case("conc.m") || c.eq_ignore_ascii_case("concreteness")
            });
            match (word, rating) {
                (Some(word), Some(rating)) => columns = (word, rating),
                _ => rows.push((number, first)),
            }
        }
        rows.extend(lines);
        let entries = rows
            .into_iter()
            .map(|(number, line)| {
                let fields: Vec<_> = line.split('\t').collect();
                let field = |index: usize| fields.get(index).map(|f| f.trim());
                let word = field(columns.0).filter(|w| !w.is_empty());
                let rating = field(columns.1).and_then(|r| r.parse::<f64>().ok());
                word.zip(rating)
                    .map(|(word, rating)| (word.to_owned(), rating))
                    .ok_or_else(|| {
                        StyleError::table(
                            REPORT,
                            format!("line {} is not a word and a numeric rating", number + 1),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(entries)
    }

    pub fn len(&self) -> usize {
        self.ratings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ratings.is_empty()
    }

    pub fn get(&self, word: &str) -> Option<f64> {
        self.ratings.get(word).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DictionOptions {
    #[serde(alias = "abstract_below")]
    pub abstract_below: f64,
    #[serde(alias = "minimum_rated_words")]
    pub minimum_rated_words: usize,
    #[serde(alias = "minimum_run_sentences")]
    pub minimum_run_sentences: usize,
}

impl Default for DictionOptions {
    fn default() -> Self {
        Self {
            abstract_below: 2.85,
            minimum_rated_words: 3,
            minimum_run_sentences: 3,
        }
    }
}

impl DictionOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_abstract_below(mut self, rating: f64) -> Self {
        self.abstract_below = rating;
        self
    }
    pub fn with_minimum_rated_words(mut self, words: usize) -> Self {
        self.minimum_rated_words = words;
        self
    }
    pub fn with_minimum_run_sentences(mut self, sentences: usize) -> Self {
        self.minimum_run_sentences = sentences;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DictionSentence {
    pub span: Span,
    pub mode: Mode,
    pub words: usize,
    pub rated_words: usize,
    pub mean: Option<f64>,
    #[serde(rename = "abstract")]
    pub is_abstract: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DictionParagraph {
    pub span: Span,
    pub sentences: std::ops::Range<usize>,
    pub rated_words: usize,
    pub mean: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DictionGroup {
    pub sentences: usize,
    pub rated_words: usize,
    pub mean: Option<f64>,
    pub abstract_sentences: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DictionReport {
    pub words: Split<usize>,
    pub table_words: usize,
    pub sentences: Vec<DictionSentence>,
    pub paragraphs: Vec<DictionParagraph>,
    pub groups: Split<DictionGroup>,
    pub abstract_runs: Vec<SentenceRun>,
}

fn mean(sum: f64, count: usize, minimum: usize) -> Option<f64> {
    (count > 0 && count >= minimum).then(|| sum / count as f64)
}

fn close(
    run: &mut Vec<usize>,
    sentences: &[DictionSentence],
    minimum: usize,
    runs: &mut Vec<SentenceRun>,
) {
    if let (Some(&first), Some(&last)) = (run.first(), run.last()) {
        if run.len() >= minimum {
            runs.push(SentenceRun {
                span: Span::covering(&sentences[first].span, &sentences[last].span),
                sentences: first..last + 1,
            });
        }
    }
    run.clear();
}

pub fn report(
    document: &Document,
    table: &ConcretenessTable,
    options: &DictionOptions,
) -> Result<DictionReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    if !options.abstract_below.is_finite() {
        return Err(StyleError::options("abstractBelow must be a finite number"));
    }
    let minimum = options.minimum_rated_words.max(1);
    let mut sentences = Vec::new();
    let mut paragraphs = Vec::new();
    let mut totals: Split<(f64, usize, usize, usize)> = Split::default();
    let mut runs = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let minimum_run = options.minimum_run_sentences.max(1);
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before {
            close(&mut run, &sentences, minimum_run, &mut runs);
        }
        let first = sentences.len();
        let (mut paragraph_sum, mut paragraph_rated) = (0.0, 0);
        for sentence in &paragraph.sentences {
            let mode = document.sentence_mode(sentence);
            let (mut sum, mut rated) = (0.0, 0);
            for word in norms::words(sentence) {
                if let Some(norms::Found { value: rating, .. }) = norms::lookup(
                    word.token,
                    word.at_start,
                    |word| table.ratings.get(word),
                    |_| true,
                ) {
                    sum += rating;
                    rated += 1;
                }
            }
            let sentence_mean = mean(sum, rated, minimum);
            let narration = mode == Mode::Narration && !document.sentence_has_speech(sentence);
            let is_abstract =
                narration && sentence_mean.is_some_and(|m| m < options.abstract_below);
            for group in totals.all_and_mode_mut(mode) {
                group.0 += sum;
                group.1 += rated;
                group.2 += 1;
                group.3 += usize::from(is_abstract);
            }
            paragraph_sum += sum;
            paragraph_rated += rated;
            sentences.push(DictionSentence {
                span: sentence.span.clone(),
                mode,
                words: sentence.word_count(),
                rated_words: rated,
                mean: sentence_mean,
                is_abstract,
            });
            let index = sentences.len() - 1;
            if !narration {
                close(&mut run, &sentences, minimum_run, &mut runs);
            } else if is_abstract {
                run.push(index);
            } else if sentence_mean.is_some() {
                close(&mut run, &sentences, minimum_run, &mut runs);
            }
        }
        if first < sentences.len() {
            paragraphs.push(DictionParagraph {
                span: paragraph.span.clone(),
                sentences: first..sentences.len(),
                rated_words: paragraph_rated,
                mean: mean(paragraph_sum, paragraph_rated, minimum),
            });
        }
    }
    close(&mut run, &sentences, minimum_run, &mut runs);
    Ok(DictionReport {
        words: word_split(document),
        table_words: table.len(),
        sentences,
        paragraphs,
        groups: totals.map(|(sum, rated, count, abstract_sentences)| DictionGroup {
            sentences: count,
            rated_words: rated,
            mean: mean(sum, rated, 1),
            abstract_sentences,
        }),
        abstract_runs: runs,
    })
}
