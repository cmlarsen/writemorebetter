use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{PercentileBand, SentenceRun, Share, Split};
use crate::error::{percent_in_range, StyleError};
use crate::{Document, Mode, Sentence, Span, Token};

pub(crate) const REPORT: &str = "sentenceStarts";
pub const SAME_WORD_MIN_SENTENCES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum SentenceStartCategory {
    Subject,
    Adverb,
    Gerund,
    Infinitive,
    SubordinatingConjunction,
    CoordinatingConjunction,
    PastParticiple,
    Inversion,
    Introductory,
    Other,
}
pub const CATEGORIES: [SentenceStartCategory; 10] = [
    SentenceStartCategory::Subject,
    SentenceStartCategory::Adverb,
    SentenceStartCategory::Gerund,
    SentenceStartCategory::Infinitive,
    SentenceStartCategory::SubordinatingConjunction,
    SentenceStartCategory::CoordinatingConjunction,
    SentenceStartCategory::PastParticiple,
    SentenceStartCategory::Inversion,
    SentenceStartCategory::Introductory,
    SentenceStartCategory::Other,
];
pub const UNIVERSAL_TAGS: [&str; 12] = [
    "ADJ", "ADP", "ADV", "CONJ", "DET", "NOUN", "NUM", "PRON", "PRT", "VERB", ".", "X",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
}

impl Default for SentenceStartsOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsBands {
    pub category_percent: BTreeMap<SentenceStartCategory, PercentileBand>,
    pub universal_percent: BTreeMap<String, PercentileBand>,
}

impl SentenceStartsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, sentences: usize) -> Self {
        self.minimum_sentences = sentences;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsReference {
    #[serde(alias = "category_percent")]
    pub category_percent: BTreeMap<SentenceStartCategory, f64>,
    #[serde(alias = "universal_percent")]
    pub universal_percent: BTreeMap<String, f64>,
}

impl SentenceStartsReference {
    fn validate(&self) -> Result<(), StyleError> {
        SentenceStartsBaseline {
            category_percent: self.category_percent.clone(),
            universal_percent: self.universal_percent.clone(),
            ..Default::default()
        }
        .validate()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsBaseline {
    #[serde(alias = "category_percent")]
    pub category_percent: BTreeMap<SentenceStartCategory, f64>,
    #[serde(alias = "universal_percent")]
    pub universal_percent: BTreeMap<String, f64>,
    pub bands: Split<SentenceStartsBands>,
    pub narration: Option<SentenceStartsReference>,
    pub dialogue: Option<SentenceStartsReference>,
}

impl SentenceStartsBaseline {
    fn reference(&self) -> SentenceStartsReference {
        SentenceStartsReference {
            category_percent: self.category_percent.clone(),
            universal_percent: self.universal_percent.clone(),
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_category_percent(mut self, category: SentenceStartCategory, percent: f64) -> Self {
        self.category_percent.insert(category, percent);
        self
    }

    pub fn with_universal_percent(mut self, tag: impl Into<String>, percent: f64) -> Self {
        self.universal_percent.insert(tag.into(), percent);
        self
    }

    pub fn bundled() -> Option<Self> {
        static BUNDLED: OnceLock<SentenceStartsBaseline> = OnceLock::new();
        Some(
            BUNDLED
                .get_or_init(|| {
                    super::bundled_baseline(include_str!(
                        "../../data/baselines/sentence-starts.json"
                    ))
                })
                .clone(),
        )
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        for value in self.category_percent.values() {
            percent_in_range(REPORT, "categoryPercent", *value)?;
        }
        for (tag, value) in &self.universal_percent {
            if !UNIVERSAL_TAGS.contains(&tag.as_str()) {
                return Err(StyleError::baseline(
                    REPORT,
                    format!("universalPercent keys must be universal-12 tags, got {tag:?}"),
                ));
            }
            percent_in_range(REPORT, "universalPercent", *value)?;
        }
        for bands in [&self.bands.all, &self.bands.narration, &self.bands.dialogue] {
            for band in bands.category_percent.values() {
                band.validate(REPORT, 100.0)?;
            }
            for (tag, band) in &bands.universal_percent {
                if !UNIVERSAL_TAGS.contains(&tag.as_str()) {
                    return Err(StyleError::baseline(REPORT, "unknown universal band tag"));
                }
                band.validate(REPORT, 100.0)?;
            }
        }
        for baseline in [&self.narration, &self.dialogue].into_iter().flatten() {
            baseline.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStart {
    pub sentence: usize,
    pub opener: Option<Span>,
    pub word: Option<String>,
    pub category: SentenceStartCategory,
    pub ambiguous: bool,
    pub universal_candidates: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartCategoryShare {
    pub category: SentenceStartCategory,
    #[serde(flatten)]
    pub share: Share,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartUniversalShare {
    pub tag: String,
    pub weight: f64,
    pub percent: Option<f64>,
    pub difference: Option<f64>,
    pub highlighted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsGroup {
    pub sentences: usize,
    pub low_confidence: bool,
    pub excluded_sentences: usize,
    pub ambiguous_starts: usize,
    pub unknown_starts: usize,
    pub categories: Vec<SentenceStartCategoryShare>,
    pub universal: Vec<SentenceStartUniversalShare>,
    pub same_word_runs: Vec<SentenceRun>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceStartsReport {
    pub sentences: Vec<super::SentenceRow>,
    pub words: super::Split<usize>,
    pub starts: Vec<SentenceStart>,
    pub groups: Split<SentenceStartsGroup>,
}

pub fn normalize_pos(pos: &str) -> &str {
    if pos.starts_with("NN:") {
        "NN"
    } else if pos.starts_with("PRP$_") {
        "PRP$"
    } else if pos.starts_with("PRP_") {
        "PRP"
    } else if pos == "RB_SENT" {
        "RB"
    } else {
        pos
    }
}

pub fn penn_to_universal(pos: &str) -> Option<&'static str> {
    Some(match normalize_pos(pos) {
        "JJ" | "JJR" | "JJS" => "ADJ",
        "IN" => "ADP",
        "RB" | "RBR" | "RBS" | "WRB" => "ADV",
        "CC" => "CONJ",
        "DT" | "EX" | "PDT" | "WDT" => "DET",
        "NN" | "NNS" | "NNP" | "NNPS" => "NOUN",
        "CD" => "NUM",
        "PRP" | "PRP$" | "WP" | "WP$" => "PRON",
        "POS" | "RP" | "TO" => "PRT",
        "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ" | "MD" => "VERB",
        "PCT" | "#" | "$" | "''" | "(" | ")" | "," | "." | ":" | "``" | "-LRB-" | "-RRB-"
        | "HYPH" | "NFP" => ".",
        "FW" | "LS" | "SYM" | "UH" => "X",
        _ => return None,
    })
}

fn tags(token: &Token) -> BTreeSet<&str> {
    token
        .readings
        .iter()
        .map(|r| normalize_pos(&r.pos))
        .collect()
}
fn has(token: &Token, wanted: &[&str]) -> bool {
    tags(token).iter().any(|t| wanted.contains(t))
}
fn subject(token: &Token) -> bool {
    has(
        token,
        &[
            "NN", "NNS", "NNP", "NNPS", "PRP", "PRP$", "WP", "WP$", "WDT", "EX", "DT", "PDT", "JJ",
            "JJR", "JJS", "CD",
        ],
    )
}

pub fn opening_tokens(sentence: &Sentence) -> Vec<&Token> {
    let mut tokens: Vec<_> = sentence
        .tokens
        .iter()
        .filter(|t| !t.is_whitespace)
        .collect();
    let quotes = "\"'“”‘’«»‹›„‟‚‛`";
    let skip = tokens
        .iter()
        .take_while(|t| !t.text.is_empty() && t.text.chars().all(|c| quotes.contains(c)))
        .count();
    tokens.drain(..skip);
    tokens
}

fn subject_pronoun(token: &Token) -> bool {
    let word = token.text.to_lowercase();
    if ["me", "him", "her", "us", "them"].contains(&word.as_str()) {
        return false;
    }
    ["i", "you", "he", "she", "it", "we", "they"].contains(&word.as_str())
        || token.readings.iter().any(|r| r.pos.starts_with("PRP_S"))
}
fn existential(token: &Token) -> bool {
    token.text.eq_ignore_ascii_case("there") && has(token, &["EX"])
}
fn particle(token: &Token) -> bool {
    [
        "still", "up", "down", "out", "away", "here", "there", "on", "off", "back",
    ]
    .contains(&token.text.to_lowercase().as_str())
}
fn finite(token: &Token) -> bool {
    !particle(token)
        && (has(token, &["VBD", "VBZ", "MD"]) || (has(token, &["VBP"]) && !has(token, &["VB"])))
}

fn verb_chunk(token: &Token) -> bool {
    token.chunks.iter().any(|c| c == "B-VP" || c == "I-VP")
}

fn clause(tokens: &[&Token]) -> bool {
    tokens.first().is_some_and(|first| {
        (subject(first)
            || (first.readings.is_empty()
                && first.text.chars().next().is_some_and(char::is_uppercase)))
            && tokens
                .iter()
                .enumerate()
                .skip(1)
                .take_while(|(index, token)| {
                    let previous = tokens[index - 1];
                    token.text != ","
                        && token.text != ";"
                        && !(has(previous, &["NN", "NNS", "NNP", "NNPS", "DT"])
                            && (subject_pronoun(token)
                                || (has(token, &["DT", "PRP$", "PDT", "NNP"])
                                    && !has(previous, &["DT"]))
                                || (token.readings.is_empty()
                                    && token.text.chars().next().is_some_and(char::is_uppercase))))
                })
                .any(|(_, token)| main_predicate(token))
    })
}

fn main_predicate(token: &Token) -> bool {
    !particle(token)
        && ((verb_chunk(token) && has(token, &["VBD", "VBN", "VBP", "VBZ", "MD"]))
            || (finite(token) && !has(token, &["NN", "NNS", "JJ"])))
}

fn closing_quote(c: char) -> char {
    match c {
        '“' | '„' | '‟' => '”',
        '‘' | '‚' | '‛' => '’',
        '«' => '»',
        '‹' => '›',
        _ => c,
    }
}

fn nominal_category(tokens: &[&Token], opening_quotes: &[char]) -> SentenceStartCategory {
    let mut dependent = false;
    let mut relative_verb_seen = false;
    let mut relative_auxiliary = false;
    let mut after_comma = false;
    let mut infinitive = false;
    let mut quotes = opening_quotes.to_vec();
    let mut opening_depth = quotes.len();
    for (index, token) in tokens.iter().enumerate().skip(1) {
        let word = token.text.to_lowercase();
        if token.text.chars().all(|c| "\"'“”‘’«»‹›„‟‚‛`".contains(c)) {
            for c in token.text.chars() {
                if quotes.last() == Some(&c) {
                    quotes.pop();
                    if opening_depth > quotes.len() {
                        opening_depth = quotes.len();
                        if opening_depth == 0 {
                            return SentenceStartCategory::Other;
                        }
                    }
                } else {
                    quotes.push(closing_quote(c));
                }
            }
            continue;
        }
        if quotes.len() > opening_depth {
            continue;
        }
        if [
            "who", "whom", "whose", "which", "that", "since", "because", "although", "if", "when",
            "while",
        ]
        .contains(&word.as_str())
        {
            dependent = true;
            relative_verb_seen = false;
        }
        if has(token, &["TO"]) {
            infinitive = true;
        }
        if token.text == "," || token.text == ";" {
            infinitive = false;
            dependent = false;
            relative_verb_seen = false;
            after_comma = true;
            continue;
        }
        if after_comma && subject_pronoun(token) {
            dependent = true;
            relative_verb_seen = false;
            after_comma = false;
        }
        if dependent && relative_verb_seen && has(token, &["CC"]) {
            relative_verb_seen = false;
        }
        let nonfinite = infinitive
            && has(token, &["VB", "VBP", "VBN", "VBG"])
            && !has(token, &["VBD", "VBZ", "MD"]);
        if nonfinite {
            continue;
        }
        let auxiliary = [
            "am", "is", "are", "was", "were", "be", "been", "being", "do", "does", "did", "have",
            "has", "had",
        ]
        .contains(&word.as_str())
            || has(token, &["MD"]);
        if dependent && !relative_verb_seen && (verb_chunk(token) || main_predicate(token)) {
            relative_verb_seen = true;
            relative_auxiliary = auxiliary;
            if !subject_pronoun(tokens[0]) {
                continue;
            }
        } else if dependent
            && relative_verb_seen
            && relative_auxiliary
            && has(token, &["VBN", "VBG", "VB"])
        {
            relative_auxiliary = auxiliary;
            continue;
        }
        let plural_predicate = has(token, &["VBP"])
            && has(tokens[index - 1], &["NNS"])
            && !has(tokens[index - 1], &["VBZ"]);
        if main_predicate(token)
            || plural_predicate
            || (subject_pronoun(tokens[0]) && verb_chunk(token))
        {
            return SentenceStartCategory::Subject;
        }
    }
    SentenceStartCategory::Other
}

/// Deterministic opening-function heuristic; see STYLE-SENTENCES.md for priority and limits.
pub fn classify(sentence: &Sentence) -> SentenceStartCategory {
    let opening_quotes: Vec<_> = sentence
        .tokens
        .iter()
        .filter(|t| !t.is_whitespace)
        .take_while(|t| {
            !t.text.is_empty() && t.text.chars().all(|c| "\"'“”‘’«»‹›„‟‚‛`".contains(c))
        })
        .flat_map(|t| t.text.chars().map(closing_quote))
        .collect();
    let tokens = opening_tokens(sentence);
    let Some(first) = tokens.first() else {
        return SentenceStartCategory::Other;
    };
    if first.is_skipped {
        return SentenceStartCategory::Other;
    }
    let second = tokens.get(1).copied();
    let word = first.text.to_lowercase();
    let words = tokens
        .iter()
        .take(5)
        .map(|t| t.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    if [
        "however",
        "therefore",
        "meanwhile",
        "finally",
        "indeed",
        "instead",
        "moreover",
        "nevertheless",
        "furthermore",
        "well",
        "yes",
        "oh",
        "ah",
        "ay",
    ]
    .contains(&word.as_str())
        || (word == "no" && second.is_none_or(|t| !t.is_word))
    {
        return SentenceStartCategory::Introductory;
    }
    if has(first, &["TO"]) && second.is_some_and(|t| has(t, &["VB"])) {
        return SentenceStartCategory::Infinitive;
    }
    if words.starts_with("much as ") || words.starts_with("now that ") || words == "now that" {
        return SentenceStartCategory::SubordinatingConjunction;
    }
    if [
        "although", "because", "if", "unless", "though", "whereas", "while", "when", "whenever",
        "whether",
    ]
    .contains(&word.as_str())
        && has(first, &["IN", "WRB", "CC", "DT", "WDT", "WP", "RB"])
    {
        return SentenceStartCategory::SubordinatingConjunction;
    }
    if ["once", "since", "after", "before", "until", "as"].contains(&word.as_str()) {
        return if clause(&tokens[1..]) || (words.starts_with("as far as ") && clause(&tokens[3..]))
        {
            SentenceStartCategory::SubordinatingConjunction
        } else {
            SentenceStartCategory::Introductory
        };
    }
    if word == "for" {
        return if has(first, &["CC"])
            && second.is_some_and(|t| subject_pronoun(t) || existential(t))
        {
            SentenceStartCategory::CoordinatingConjunction
        } else {
            SentenceStartCategory::Introductory
        };
    }
    if has(first, &["CC"]) && ["and", "but", "or", "nor", "yet", "so"].contains(&word.as_str()) {
        return SentenceStartCategory::CoordinatingConjunction;
    }
    if has(first, &["VBG"]) {
        return if has(first, &["NN", "NNS"]) && second.is_some_and(finite) {
            SentenceStartCategory::Subject
        } else {
            SentenceStartCategory::Gerund
        };
    }
    if has(first, &["VBN"])
        && !has(first, &["VB", "VBP"])
        && (!has(first, &["VBD"]) || second.is_some_and(|t| t.text == "," || has(t, &["IN"])))
    {
        return SentenceStartCategory::PastParticiple;
    }
    let auxiliary = [
        "am", "is", "are", "was", "were", "do", "does", "did", "have", "has", "had", "can",
        "could", "will", "would", "shall", "should", "may", "might", "must",
    ]
    .contains(&word.as_str());
    let next = second
        .filter(|t| !["n't", "n’t"].contains(&t.text.as_str()))
        .or_else(|| {
            second
                .filter(|t| ["n't", "n’t"].contains(&t.text.as_str()))
                .and_then(|_| tokens.get(2).copied())
        });
    let question = sentence.text.contains('?');
    if auxiliary
        && has(first, &["VB", "VBD", "VBP", "VBZ", "MD"])
        && next.is_some_and(|t| {
            subject_pronoun(t)
                || existential(t)
                || (question && has(t, &["DT", "PDT", "NN", "NNS", "NNP", "NNPS"]))
        })
        && (question || !["do", "have"].contains(&word.as_str()))
    {
        return SentenceStartCategory::Inversion;
    }
    if word == "there" {
        return if second.is_some_and(finite) {
            SentenceStartCategory::Subject
        } else {
            SentenceStartCategory::Adverb
        };
    }
    if [
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
    {
        return nominal_category(&tokens, &opening_quotes);
    }
    if has(first, &["IN", "TO"]) && !has(first, &["DT", "WDT", "PRP", "PRP$"]) {
        return SentenceStartCategory::Introductory;
    }
    if has(first, &["RB", "RBR", "RBS", "WRB"]) {
        return SentenceStartCategory::Adverb;
    }
    if has(first, &["VB", "VBD", "VBP", "VBZ", "MD"])
        && !second.is_some_and(|t| {
            finite(t)
                && !["up", "down", "out", "away", "here", "there", "on", "off"]
                    .contains(&t.text.to_lowercase().as_str())
        })
    {
        if subject(first)
            && second
                .is_some_and(|t| has(t, &["IN", "NN", "NNS", "NNP", "NNPS", "JJ", "JJR", "JJS"]))
        {
            return nominal_category(&tokens, &opening_quotes);
        }
        return SentenceStartCategory::Other;
    }
    if subject(first) {
        return nominal_category(&tokens, &opening_quotes);
    }
    if has(first, &["UH"]) {
        return SentenceStartCategory::Introductory;
    }
    if first.is_word
        && first.readings.is_empty()
        && first.text.chars().next().is_some_and(char::is_uppercase)
    {
        return nominal_category(&tokens, &opening_quotes);
    }
    SentenceStartCategory::Other
}

#[derive(Default)]
struct Builder {
    sentences: usize,
    excluded: usize,
    ambiguous: usize,
    unknown: usize,
    categories: BTreeMap<SentenceStartCategory, usize>,
    universal: BTreeMap<&'static str, f64>,
    runs: Vec<SentenceRun>,
}

impl Builder {
    fn add(&mut self, start: &SentenceStart) {
        self.ambiguous += usize::from(start.ambiguous);
        self.unknown += usize::from(start.universal_candidates.is_empty());
        *self.categories.entry(start.category).or_default() += 1;
        for tag in &start.universal_candidates {
            let tag = UNIVERSAL_TAGS.iter().find(|known| *known == tag).unwrap();
            *self.universal.entry(tag).or_default() +=
                1.0 / start.universal_candidates.len() as f64;
        }
    }

    fn finish(
        self,
        options: &SentenceStartsOptions,
        baseline: Option<&SentenceStartsReference>,
        bands: Option<&SentenceStartsBands>,
    ) -> SentenceStartsGroup {
        let eligible = self.sentences - self.excluded;
        let enough = eligible >= options.minimum_sentences && eligible != 0;
        let known = eligible - self.unknown;
        SentenceStartsGroup {
            sentences: self.sentences,
            low_confidence: eligible < 200.max(options.minimum_sentences),
            excluded_sentences: self.excluded,
            ambiguous_starts: self.ambiguous,
            unknown_starts: self.unknown,
            categories: CATEGORIES
                .into_iter()
                .map(|category| SentenceStartCategoryShare {
                    category,
                    share: {
                        let mut share = Share::new(
                            self.categories.get(&category).copied().unwrap_or(0),
                            eligible,
                            enough,
                            baseline.and_then(|b| b.category_percent.get(&category).copied()),
                        );
                        share.highlighted = PercentileBand::highlights(
                            bands.and_then(|b| b.category_percent.get(&category)),
                            share.percent,
                        );
                        share
                    },
                })
                .collect(),
            universal: UNIVERSAL_TAGS
                .into_iter()
                .map(|tag| {
                    let weight = self.universal.get(tag).copied().unwrap_or(0.0);
                    let percent = (enough && known > 0).then(|| 100.0 * weight / known as f64);
                    SentenceStartUniversalShare {
                        tag: tag.to_owned(),
                        weight,
                        percent,
                        difference: percent
                            .zip(baseline.and_then(|b| b.universal_percent.get(tag)))
                            .map(|(a, b)| a - b),
                        highlighted: PercentileBand::highlights(
                            bands.and_then(|b| b.universal_percent.get(tag)),
                            percent,
                        ),
                    }
                })
                .collect(),
            same_word_runs: self.runs,
        }
    }
}

struct Run {
    second: Option<String>,
    same_two: bool,
    rhetorical: bool,
    word: String,
    mode: Mode,
    span: Span,
    first: usize,
    last: usize,
}

fn flush(groups: &mut Split<Builder>, run: Option<Run>) {
    if let Some(run) = run {
        if run.mode == Mode::Narration
            && run.last + 1 - run.first >= SAME_WORD_MIN_SENTENCES
            && !run.same_two
            && !run.rhetorical
        {
            for group in groups.all_and_mode_mut(run.mode) {
                group.runs.push(SentenceRun {
                    span: run.span.clone(),
                    sentences: run.first..run.last + 1,
                });
            }
        }
    }
}

pub fn report(
    document: &Document,
    options: &SentenceStartsOptions,
    baseline: Option<&SentenceStartsBaseline>,
) -> Result<SentenceStartsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let mut groups: Split<Builder> = Split::default();
    let mut starts = Vec::new();
    let mut index = 0;
    let mut previous: Option<Run> = None;
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before {
            flush(&mut groups, previous.take());
        }
        for sentence in &paragraph.sentences {
            let sentence_index = index;
            index += 1;
            let mode = document.sentence_mode(sentence);
            for group in groups.all_and_mode_mut(mode) {
                group.sentences += 1;
            }
            let tokens = opening_tokens(sentence);
            if tokens.first().is_some_and(|t| t.is_skipped) {
                for group in groups.all_and_mode_mut(mode) {
                    group.excluded += 1;
                }
                flush(&mut groups, previous.take());
                continue;
            }
            let first = tokens.first().copied();
            let universal_first = tokens
                .get(usize::from(
                    tokens.first().is_some_and(|t| t.is_punctuation),
                ))
                .copied()
                .filter(|t| !t.is_skipped);
            let start = SentenceStart {
                sentence: sentence_index,
                opener: first.map(|t| t.span.clone()),
                word: first.filter(|t| t.is_word).map(|t| t.text.to_lowercase()),
                category: classify(sentence),
                ambiguous: first.is_some_and(|t| tags(t).len() > 1),
                universal_candidates: universal_first
                    .map(|t| {
                        t.readings
                            .iter()
                            .filter_map(|r| penn_to_universal(&r.pos))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            for group in groups.all_and_mode_mut(mode) {
                group.add(&start);
            }
            if document.sentence_has_speech(sentence) {
                flush(&mut groups, previous.take());
                starts.push(start);
                continue;
            }
            let second = sentence.words().nth(1).map(|t| t.text.to_lowercase());
            let rhetorical = sentence
                .text
                .trim_end()
                .trim_end_matches(['”', '"', '\''])
                .ends_with(['?', '!']);
            if let Some(word) = start.word.as_ref() {
                if let Some(run) = &mut previous {
                    if *word == run.word && mode == run.mode {
                        run.span = Span::covering(&run.span, &sentence.span);
                        run.last = sentence_index;
                        run.same_two &= run.second == second;
                        run.rhetorical &= rhetorical;
                        starts.push(start);
                        continue;
                    }
                }
            }
            flush(&mut groups, previous.take());
            previous = start.word.clone().map(|word| Run {
                second,
                same_two: true,
                rhetorical,
                word,
                mode,
                span: sentence.span.clone(),
                first: sentence_index,
                last: sentence_index,
            });
            starts.push(start);
        }
    }
    flush(&mut groups, previous.take());
    Ok(SentenceStartsReport {
        sentences: super::sentence_table(document),
        words: super::word_split(document),
        starts,
        groups: Split {
            all: groups.all.finish(
                options,
                baseline.map(|b| b.reference()).as_ref(),
                baseline.map(|b| &b.bands.all),
            ),
            narration: groups.narration.finish(
                options,
                baseline.and_then(|b| b.narration.as_ref()),
                baseline.map(|b| &b.bands.narration),
            ),
            dialogue: groups.dialogue.finish(
                options,
                baseline.and_then(|b| b.dialogue.as_ref()),
                baseline.map(|b| &b.bands.dialogue),
            ),
        },
    })
}
