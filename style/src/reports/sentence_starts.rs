use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{covering, dialogue};
use crate::{Document, Sentence, Span, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Category {
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
pub const CATEGORIES: [Category; 10] = [
    Category::Subject,
    Category::Adverb,
    Category::Gerund,
    Category::Infinitive,
    Category::SubordinatingConjunction,
    Category::CoordinatingConjunction,
    Category::PastParticiple,
    Category::Inversion,
    Category::Introductory,
    Category::Other,
];
pub const UNIVERSAL_TAGS: [&str; 12] = [
    "ADJ", "ADP", "ADV", "CONJ", "DET", "NOUN", "NUM", "PRON", "PRT", "VERB", ".", "X",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Options {
    pub minimum_sentences: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            minimum_sentences: 200,
        }
    }
}
impl Options {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Baseline {
    pub category_percent: BTreeMap<Category, f64>,
    #[serde(deserialize_with = "universal_baseline")]
    pub universal_percent: BTreeMap<String, f64>,
}
fn universal_baseline<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, f64>, D::Error> {
    let values = BTreeMap::<String, f64>::deserialize(deserializer)?;
    if values.iter().any(|(tag, value)| {
        !UNIVERSAL_TAGS.contains(&tag.as_str())
            || !value.is_finite()
            || !(0.0..=100.0).contains(value)
    }) {
        return Err(serde::de::Error::custom(
            "universal baseline requires universal-12 keys and percentages in 0..=100",
        ));
    }
    Ok(values)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Count {
    pub count: f64,
    pub percent: Option<f64>,
    pub difference_percentage_points: Option<f64>,
    pub spans: Vec<Span>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Start {
    pub span: Span,
    pub word: Option<String>,
    pub category: Category,
    pub ambiguous: bool,
    pub universal_candidates: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Group {
    pub sentence_count: usize,
    pub ambiguous_starts: usize,
    pub unknown_starts: usize,
    pub categories: BTreeMap<Category, Count>,
    pub universal: BTreeMap<String, Count>,
    pub starts: Vec<Start>,
    pub same_word_runs: Vec<Span>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Report {
    pub all: Group,
    pub narration: Group,
    pub dialogue: Group,
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

fn nominal_category(tokens: &[&Token], opening_quotes: &[char]) -> Category {
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
                            return Category::Other;
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
            return Category::Subject;
        }
    }
    Category::Other
}

/// Deterministic opening-function heuristic; see STYLE-SENTENCES.md for priority and limits.
pub fn classify(sentence: &Sentence) -> Category {
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
        return Category::Other;
    };
    if first.is_skipped {
        return Category::Other;
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
        return Category::Introductory;
    }
    if has(first, &["TO"]) && second.is_some_and(|t| has(t, &["VB"])) {
        return Category::Infinitive;
    }
    if words.starts_with("much as ") || words.starts_with("now that ") || words == "now that" {
        return Category::SubordinatingConjunction;
    }
    if [
        "although", "because", "if", "unless", "though", "whereas", "while", "when", "whenever",
        "whether",
    ]
    .contains(&word.as_str())
        && has(first, &["IN", "WRB", "CC", "DT", "WDT", "WP", "RB"])
    {
        return Category::SubordinatingConjunction;
    }
    if ["once", "since", "after", "before", "until", "as"].contains(&word.as_str()) {
        return if clause(&tokens[1..]) || (words.starts_with("as far as ") && clause(&tokens[3..]))
        {
            Category::SubordinatingConjunction
        } else {
            Category::Introductory
        };
    }
    if word == "for" {
        return if has(first, &["CC"])
            && second.is_some_and(|t| subject_pronoun(t) || existential(t))
        {
            Category::CoordinatingConjunction
        } else {
            Category::Introductory
        };
    }
    if has(first, &["CC"]) && ["and", "but", "or", "nor", "yet", "so"].contains(&word.as_str()) {
        return Category::CoordinatingConjunction;
    }
    if has(first, &["VBG"]) {
        return if has(first, &["NN", "NNS"]) && second.is_some_and(finite) {
            Category::Subject
        } else {
            Category::Gerund
        };
    }
    if has(first, &["VBN"])
        && !has(first, &["VB", "VBP"])
        && (!has(first, &["VBD"]) || second.is_some_and(|t| t.text == "," || has(t, &["IN"])))
    {
        return Category::PastParticiple;
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
        return Category::Inversion;
    }
    if word == "there" {
        return if second.is_some_and(finite) {
            Category::Subject
        } else {
            Category::Adverb
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
        return Category::Introductory;
    }
    if has(first, &["RB", "RBR", "RBS", "WRB"]) {
        return Category::Adverb;
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
        return Category::Other;
    }
    if subject(first) {
        return nominal_category(&tokens, &opening_quotes);
    }
    if has(first, &["UH"]) {
        return Category::Introductory;
    }
    if first.is_word
        && first.readings.is_empty()
        && first.text.chars().next().is_some_and(char::is_uppercase)
    {
        return nominal_category(&tokens, &opening_quotes);
    }
    Category::Other
}

fn add(group: &mut Group, start: &Start) {
    group.sentence_count += 1;
    group.ambiguous_starts += usize::from(start.ambiguous);
    group.unknown_starts += usize::from(start.universal_candidates.is_empty());
    let count = group.categories.entry(start.category).or_default();
    count.count += 1.0;
    count.spans.push(start.span.clone());
    for tag in &start.universal_candidates {
        let count = group.universal.entry(tag.clone()).or_default();
        count.count += 1.0 / start.universal_candidates.len() as f64;
        count.spans.push(start.span.clone());
    }
    group.starts.push(start.clone());
}
fn finish(group: &mut Group, options: &Options, baseline: Option<&Baseline>) {
    for category in CATEGORIES {
        group.categories.entry(category).or_default();
    }
    for tag in UNIVERSAL_TAGS {
        group.universal.entry(tag.into()).or_default();
    }
    let enough = group.sentence_count >= options.minimum_sentences && group.sentence_count != 0;
    for (category, count) in &mut group.categories {
        count.percent = enough.then(|| 100.0 * count.count / group.sentence_count as f64);
        count.difference_percentage_points = count
            .percent
            .zip(baseline.and_then(|b| b.category_percent.get(category)))
            .map(|(a, b)| a - b);
    }
    let known = group.sentence_count - group.unknown_starts;
    for (tag, count) in &mut group.universal {
        count.percent = (enough && known > 0).then(|| 100.0 * count.count / known as f64);
        count.difference_percentage_points = count
            .percent
            .zip(baseline.and_then(|b| b.universal_percent.get(tag)))
            .map(|(a, b)| a - b);
    }
}

pub fn report(document: &Document, options: &Options, baseline: Option<&Baseline>) -> Report {
    let mut result = Report {
        all: Group::default(),
        narration: Group::default(),
        dialogue: Group::default(),
    };
    for paragraph in &document.paragraphs {
        let mut previous: Option<(String, bool, Span, usize)> = None;
        for sentence in &paragraph.sentences {
            let tokens = opening_tokens(sentence);
            let first = tokens.first().copied().filter(|t| !t.is_skipped);
            let universal_first = tokens
                .get(usize::from(
                    tokens.first().is_some_and(|t| t.is_punctuation),
                ))
                .copied()
                .filter(|t| !t.is_skipped);
            let start = Start {
                span: sentence.span.clone(),
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
            let speech = dialogue(document, sentence);
            if tokens.first().is_some_and(|t| t.is_skipped) {
                flush(&mut result, previous.take());
                continue;
            }
            add(&mut result.all, &start);
            add(
                if speech {
                    &mut result.dialogue
                } else {
                    &mut result.narration
                },
                &start,
            );
            if let Some(word) = start.word.as_ref() {
                if let Some((last, mode, span, count)) = &mut previous {
                    if word == last && speech == *mode {
                        *span = covering(span, &start.span);
                        *count += 1;
                        continue;
                    }
                }
            }
            flush(&mut result, previous.take());
            previous = start.word.map(|word| (word, speech, start.span, 1));
        }
        flush(&mut result, previous.take());
    }
    for group in [&mut result.all, &mut result.narration, &mut result.dialogue] {
        finish(group, options, baseline);
    }
    result
}
fn flush(result: &mut Report, run: Option<(String, bool, Span, usize)>) {
    if let Some((_, speech, span, count)) = run {
        if count >= 3 {
            result.all.same_word_runs.push(span.clone());
            (if speech {
                &mut result.dialogue
            } else {
                &mut result.narration
            })
            .same_word_runs
            .push(span);
        }
    }
}
