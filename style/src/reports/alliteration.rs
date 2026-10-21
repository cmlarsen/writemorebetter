//! Alliteration: runs of stressed words in one sentence that start with the same consonant sound,
//! read from CMUdict's first phoneme with a spelling fallback (see [`crate::pronunciation`]).
//! Fiction uses alliteration on purpose, so this is an informational lens with no verdict. See
//! `docs/STYLE-ALLITERATION.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{word_split, Split};
use crate::pronunciation::initial_sound;
use crate::{Document, Mode, Span, StyleError, Token};

pub const REPORT: &str = "alliteration";

// Words that are unstressed in running speech: articles, pronouns, prepositions, conjunctions,
// auxiliaries and their contractions. They never join or break a run.
const FUNCTION_WORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "this",
    "that",
    "these",
    "those",
    "some",
    "any",
    "no",
    "each",
    "every",
    "my",
    "your",
    "his",
    "her",
    "its",
    "our",
    "their",
    "i",
    "me",
    "you",
    "he",
    "him",
    "she",
    "it",
    "we",
    "us",
    "they",
    "them",
    "who",
    "whom",
    "whose",
    "which",
    "what",
    "there",
    "here",
    "of",
    "to",
    "in",
    "on",
    "at",
    "by",
    "for",
    "from",
    "with",
    "into",
    "onto",
    "upon",
    "unto",
    "as",
    "than",
    "about",
    "above",
    "across",
    "after",
    "against",
    "along",
    "among",
    "amongst",
    "around",
    "before",
    "behind",
    "below",
    "beneath",
    "beside",
    "besides",
    "between",
    "beyond",
    "during",
    "except",
    "inside",
    "outside",
    "through",
    "throughout",
    "till",
    "toward",
    "towards",
    "within",
    "without",
    "via",
    "per",
    "and",
    "but",
    "or",
    "nor",
    "so",
    "if",
    "then",
    "when",
    "where",
    "why",
    "how",
    "while",
    "whether",
    "though",
    "although",
    "because",
    "unless",
    "until",
    "be",
    "am",
    "is",
    "are",
    "was",
    "were",
    "been",
    "being",
    "have",
    "has",
    "had",
    "having",
    "do",
    "does",
    "did",
    "will",
    "would",
    "shall",
    "should",
    "can",
    "could",
    "may",
    "might",
    "must",
    "not",
    "can't",
    "won't",
    "don't",
    "doesn't",
    "didn't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
    "hasn't",
    "haven't",
    "hadn't",
    "shouldn't",
    "wouldn't",
    "couldn't",
    "mustn't",
    "needn't",
    "shan't",
    "ain't",
    "o",
    "oh",
];
// Clitics: contracted forms of a function word (`she'd`, `it's`, `we're`), and the possessive.
const CLITICS: &[&str] = &["s", "d", "ll", "re", "ve", "m"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AlliterationOptions {
    /// Fewest different words sharing a sound that make a run (at least 2).
    #[serde(alias = "minimum_words")]
    pub minimum_words: usize,
    /// Most stressed words with another sound allowed between two words of a run.
    #[serde(alias = "maximum_gap")]
    pub maximum_gap: usize,
    /// Leave dialogue out: no runs and no word counts there.
    #[serde(alias = "skip_dialogue")]
    pub skip_dialogue: bool,
}

impl Default for AlliterationOptions {
    fn default() -> Self {
        Self {
            minimum_words: 3,
            maximum_gap: 0,
            skip_dialogue: false,
        }
    }
}

impl AlliterationOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_minimum_words(mut self, value: usize) -> Self {
        self.minimum_words = value;
        self
    }

    pub fn with_maximum_gap(mut self, value: usize) -> Self {
        self.maximum_gap = value;
        self
    }

    pub fn with_skip_dialogue(mut self, skip: bool) -> Self {
        self.skip_dialogue = skip;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AlliterationRun {
    /// From the first word of the run to the last.
    pub span: Span,
    /// The shared first sound: a [`crate::pronunciation::CONSONANTS`] label.
    pub sound: String,
    /// Each word of the run, in order.
    pub spans: Vec<Span>,
    /// The words as written, in order.
    pub words: Vec<String>,
    pub sentence: usize,
    pub mode: Mode,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AlliterationGroup {
    pub words: usize,
    pub runs: usize,
    pub per_thousand_words: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AlliterationReport {
    pub runs: Vec<AlliterationRun>,
    pub groups: Split<AlliterationGroup>,
}

fn fold(text: &str) -> String {
    text.chars()
        .map(|c| if c == '’' { '\'' } else { c })
        .flat_map(char::to_lowercase)
        .collect()
}

/// A word that carries stress in a sentence: not a function word, a number or a skipped name.
fn stressed(token: &Token) -> bool {
    if !token.is_word || token.is_skipped || !token.text.chars().any(char::is_alphabetic) {
        return false;
    }
    let word = fold(&token.text);
    // A clitic the tokenizer splits off (`’s`, `’d`, `n’t`) belongs to the word before it.
    if word == "n't"
        || word
            .strip_prefix('\'')
            .is_some_and(|clitic| CLITICS.contains(&clitic))
    {
        return false;
    }
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    let function = |word: &str| FUNCTION_WORDS.contains(&word);
    !(function(word)
        || word
            .split_once('\'')
            .is_some_and(|(stem, clitic)| CLITICS.contains(&clitic) && function(stem)))
}

struct Open {
    members: Vec<(usize, String)>,
    last: usize,
}

pub fn report(
    document: &Document,
    options: &AlliterationOptions,
) -> Result<AlliterationReport, StyleError> {
    if options.minimum_words < 2 {
        return Err(StyleError::options("minimumWords must be at least 2"));
    }
    let mut runs = Vec::new();
    for (index, sentence) in document.sentences().enumerate() {
        // Stressed words with their mode; a mode change starts a new stretch.
        let words: Vec<(&Token, Mode)> = sentence
            .tokens
            .iter()
            .filter(|token| stressed(token))
            .map(|token| (token, document.mode_at(token.span.byte.start)))
            .filter(|(_, mode)| !(options.skip_dialogue && *mode == Mode::Dialogue))
            .collect();
        for stretch in words.chunk_by(|a, b| a.1 == b.1) {
            let mode = stretch[0].1;
            let mut open: BTreeMap<&'static str, Open> = BTreeMap::new();
            let mut close = |run: Open, sound: &str| {
                let mut distinct: Vec<&str> = run.members.iter().map(|(_, w)| w.as_str()).collect();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() >= options.minimum_words {
                    let tokens: Vec<&Token> =
                        run.members.iter().map(|(i, _)| stretch[*i].0).collect();
                    runs.push(AlliterationRun {
                        span: Span::covering(&tokens[0].span, &tokens[tokens.len() - 1].span),
                        sound: sound.to_owned(),
                        spans: tokens.iter().map(|t| t.span.clone()).collect(),
                        words: tokens.iter().map(|t| t.text.clone()).collect(),
                        sentence: index,
                        mode,
                    });
                }
            };
            for (at, (token, _)) in stretch.iter().enumerate() {
                let sound = initial_sound(&token.text);
                if sound == "-" {
                    continue;
                }
                let word = fold(&token.text);
                match open.get_mut(sound) {
                    Some(run) if at - run.last - 1 <= options.maximum_gap => {
                        run.members.push((at, word));
                        run.last = at;
                    }
                    _ => {
                        let fresh = Open {
                            members: vec![(at, word)],
                            last: at,
                        };
                        if let Some(done) = open.insert(sound, fresh) {
                            close(done, sound);
                        }
                    }
                }
            }
            for (sound, run) in open {
                close(run, sound);
            }
        }
    }
    runs.sort_by_key(|run| (run.span.byte.start, run.span.byte.end));
    let words = word_split(document);
    let mut groups: Split<AlliterationGroup> = Split::default();
    for (mode, group) in [
        (None, &mut groups.all),
        (Some(Mode::Narration), &mut groups.narration),
        (Some(Mode::Dialogue), &mut groups.dialogue),
    ] {
        group.words = match mode {
            None if options.skip_dialogue => words.narration,
            None => words.all,
            Some(Mode::Dialogue) if options.skip_dialogue => 0,
            Some(mode) => *words.mode(mode),
        };
        group.runs = runs
            .iter()
            .filter(|run| mode.is_none_or(|mode| mode == run.mode))
            .count();
        group.per_thousand_words = if group.words == 0 {
            0.0
        } else {
            1000.0 * group.runs as f64 / group.words as f64
        };
    }
    Ok(AlliterationReport { runs, groups })
}
