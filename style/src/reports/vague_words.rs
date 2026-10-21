use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::reports::echoes::fold;
use crate::reports::{word_split, Split};
use crate::{Document, Mode, Span, StyleError, Token};

pub(crate) const REPORT: &str = "vagueWords";

const LIST: &str = include_str!("../../data/vague/vague-words.tsv");
const FILLERS: &str = include_str!("../../data/vague/fillers.txt");

/// Fillers other checks already flag, left out so a word is never reported twice: LanguageTool's
/// `TIRED_INTENSIFIERS` (*really*, *very*, *quite*, *extremely*) and the overused-words report's
/// filler list (*actually*, *just*, *simply*).
pub const COVERED: [&str; 7] = [
    "really",
    "very",
    "quite",
    "extremely",
    "actually",
    "just",
    "simply",
];
/// Discourse fillers, counted only when they stand alone between commas or at a clause edge:
/// *It was, like, huge*, *Well, I*, *right?*.
const DISCOURSE: [&str; 9] = [
    "like", "well", "right", "ok", "okay", "whatever", "whenever", "wherever", "whoever",
];
/// Intensifiers, counted only before an adjective or adverb: *so tired*, *too late*.
const INTENSIFIERS: [&str; 6] = ["so", "too", "most", "much", "real", "rather"];
/// Words after an intensifier that make a fixed phrase instead: *so on*, *so far*, *as much as*.
const NOT_INTENSIFIED: [&str; 11] = [
    "on", "far", "forth", "there", "then", "that", "as", "than", "long", "not", "up",
];
/// Verbs that delay the action they introduce, counted before *to* or a gerund: *began to run*.
const DELAYING: [&str; 3] = ["start", "begin", "try"];
const DETERMINERS: [&str; 18] = [
    "a", "an", "the", "this", "that", "these", "those", "what", "some", "any", "every", "each",
    "no", "one", "all", "another", "which", "same",
];
const CLAUSE_PUNCTUATION: [&str; 9] = [",", "?", "!", ".", "…", "—", "–", ";", ":"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VagueWordCategory {
    /// From the bundled filler list: *basically*, *probably*, *began to*, *like*.
    Filler,
    /// A phrase that softens what follows: *sort of*, *a bit*, *seemed to*.
    Hedge,
    /// A count left unsaid: *a lot*, *a number of*, *various*.
    Quantity,
    /// A noun that names nothing in particular: *thing*, *stuff*.
    Noun,
    /// An adjective that judges without describing: *nice*, *amazing*, *weird*.
    Adjective,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct VagueWordsOptions {
    /// Include the bundled filler list.
    pub fillers: bool,
}
impl Default for VagueWordsOptions {
    fn default() -> Self {
        Self { fillers: true }
    }
}
impl VagueWordsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_fillers(mut self, value: bool) -> Self {
        self.fillers = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VagueWord {
    pub span: Span,
    pub sentence: usize,
    /// The list entry: *sort of*, *thing*, *seem to*.
    pub term: String,
    pub category: VagueWordCategory,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VagueWordTerm {
    pub term: String,
    pub category: VagueWordCategory,
    pub count: usize,
    pub narration: usize,
    pub dialogue: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VagueWordsGroup {
    pub matches: usize,
    pub words: usize,
    pub per_thousand_words: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VagueWordsReport {
    pub matches: Vec<VagueWord>,
    /// One row per term found, most frequent first.
    pub terms: Vec<VagueWordTerm>,
    pub groups: Split<VagueWordsGroup>,
}

struct Entry {
    term: &'static str,
    words: Vec<&'static str>,
    category: VagueWordCategory,
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let listed = LIST
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let (category, term) = line.split_once('\t').expect("vague words TSV");
                let category = match category {
                    "hedge" => VagueWordCategory::Hedge,
                    "quantity" => VagueWordCategory::Quantity,
                    "noun" => VagueWordCategory::Noun,
                    "adjective" => VagueWordCategory::Adjective,
                    _ => unreachable!("vague words category {category}"),
                };
                (term, category)
            });
        let fillers = FILLERS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .filter(|term| !COVERED.contains(term))
            .map(|term| (term, VagueWordCategory::Filler));
        let mut entries: Vec<_> = listed
            .chain(fillers)
            .map(|(term, category)| Entry {
                term,
                words: term.split_whitespace().collect(),
                category,
            })
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.words.len()));
        entries
    })
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token.readings.iter().any(|r| r.pos.starts_with(prefix))
}

fn noun_only(token: &Token) -> bool {
    !token.readings.is_empty() && token.readings.iter().all(|r| r.pos.starts_with("NN"))
}

/// A word that an adjective, adverb or participle reading could read as modified: *tired*, *late*.
fn modifiable(token: &Token) -> bool {
    token.is_word
        && !noun_only(token)
        && !has_pos(token, "DT")
        && !has_pos(token, "PRP")
        && (has_pos(token, "JJ") || has_pos(token, "RB") || has_pos(token, "VBN"))
}

fn verb_lemma(token: &Token, word: &str) -> bool {
    token
        .readings
        .iter()
        .any(|r| r.pos.starts_with("VB") && fold(&r.lemma) == word)
}

fn noun_lemma(token: &Token, word: &str) -> bool {
    token
        .readings
        .iter()
        .any(|r| r.pos.starts_with("NN") && !r.pos.starts_with("NNP") && fold(&r.lemma) == word)
}

struct Words<'a> {
    tokens: Vec<&'a Token>,
    folded: Vec<String>,
}

impl Words<'_> {
    fn edge(&self, at: Option<usize>) -> bool {
        at.and_then(|at| self.tokens.get(at))
            .is_none_or(|token| !token.is_word)
    }

    /// Standalone between punctuation or a sentence edge: *Well, I*, *was, like, huge*.
    fn standalone(&self, at: usize) -> bool {
        let before = at.checked_sub(1);
        let after = self.tokens.get(at + 1);
        self.edge(before)
            && after.is_none_or(|token| CLAUSE_PUNCTUATION.contains(&token.text.as_str()))
    }

    fn word_matches(&self, at: usize, word: &str, entry: &Entry) -> bool {
        let token = self.tokens[at];
        if !token.is_word || token.is_skipped {
            return false;
        }
        // A capital inside the sentence marks a name: "They flew to Nice."
        if at > 0
            && self.tokens[..at].iter().any(|t| t.is_word)
            && token.text.starts_with(char::is_uppercase)
            && matches!(
                entry.category,
                VagueWordCategory::Noun | VagueWordCategory::Adjective
            )
        {
            return false;
        }
        match entry.category {
            VagueWordCategory::Noun => noun_lemma(token, word),
            _ if DELAYING.contains(&word) || word == "seem" => verb_lemma(token, word),
            _ => self.folded[at] == word,
        }
    }

    fn guard(&self, entry: &Entry, at: usize, end: usize) -> bool {
        let token = self.tokens[at];
        let previous = at.checked_sub(1).map(|at| self.folded[at].as_str());
        let next = self.tokens.get(end).copied();
        match entry.term {
            // "sort of tired", not "the sort of man" or "kind of him".
            // "a strange sort of creak" and "her kind of music" name a type.
            "sort of" | "kind of" => {
                !previous.is_some_and(|word| DETERMINERS.contains(&word))
                    && !(at > 0
                        && (has_pos(self.tokens[at - 1], "PRP$")
                            || has_pos(self.tokens[at - 1], "JJ")))
                    && next
                        .is_some_and(|t| modifiable(t) || (has_pos(t, "VB") && !has_pos(t, "NN")))
            }
            // "a little tired", not "a little girl", "a bit of bread" or the attributive "a little
            // withered old man".
            "a bit" | "a little" => {
                next.is_some_and(modifiable)
                    && !self
                        .tokens
                        .get(end + 1)
                        .is_some_and(|t| noun_only(t) || has_pos(t, "JJ"))
            }
            "or so" => self.edge(Some(end)),
            // A sentence adverb before a comma or at the end of a clause: "In a way, it was", "it
            // helped, in a way." "In a way we never had" and "to a degree that" describe a
            // manner or an amount.
            "in a way" | "in some ways" | "to a degree" | "to some extent" => {
                next.is_none_or(|t| CLAUSE_PUNCTUATION.contains(&t.text.as_str()))
            }
            "seem to" => next.is_some_and(|t| has_pos(t, "VB") && !has_pos(t, "PRP")),
            _ => match entry.category {
                VagueWordCategory::Adjective | VagueWordCategory::Quantity
                    if entry.words.len() == 1 =>
                {
                    has_pos(token, "JJ")
                }
                VagueWordCategory::Hedge if entry.words.len() == 1 => has_pos(token, "RB"),
                VagueWordCategory::Filler => self.filler(entry.term, at),
                _ => true,
            },
        }
    }

    fn filler(&self, term: &str, at: usize) -> bool {
        let token = self.tokens[at];
        let next = self.tokens.get(at + 1).copied();
        if DISCOURSE.contains(&term) {
            return self.standalone(at);
        }
        if term == "so" && self.standalone(at) {
            return true;
        }
        if INTENSIFIERS.contains(&term) {
            let previous = at.checked_sub(1).map(|at| self.folded[at].as_str());
            // "the most beautiful", "her most vital" and "like most old" are superlatives or
            // quantities; "a most peculiar" intensifies.
            let fixed = match term {
                "most" => !matches!(previous, Some("a" | "an")),
                // "do so", "and so landed": a pro-form or a conjunction.
                "so" => {
                    matches!(previous, Some("do" | "and" | "or" | "but"))
                        || at > 0 && verb_lemma(self.tokens[at - 1], "do")
                }
                // "or rather", "would rather": a correction or a preference.
                "rather" => matches!(previous, Some("or" | "would" | "'d" | "had" | "far")),
                "much" => matches!(previous, Some("as")),
                _ => false,
            };
            return !fixed
                && has_pos(token, "RB")
                && next.is_some_and(|t| {
                    t.is_word
                        && !NOT_INTENSIFIED.contains(&fold(&t.text).as_str())
                        && !noun_only(t)
                        && !has_pos(t, "VBG")
                        && (has_pos(t, "JJ") || has_pos(t, "RB"))
                });
        }
        if DELAYING.contains(&term) {
            return next.is_some_and(|t| t.text == "to" || has_pos(t, "VBG"));
        }
        match term {
            // "might be", not "a giant in might".
            "might" => {
                has_pos(token, "MD") && next.is_some_and(|t| has_pos(t, "VB") || has_pos(t, "RB"))
            }
            "actual" => has_pos(token, "JJ"),
            _ => has_pos(token, "RB") || has_pos(token, "UH"),
        }
    }
}

pub fn report(
    document: &Document,
    options: &VagueWordsOptions,
) -> Result<VagueWordsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let mut matches = Vec::new();
    for (index, sentence) in document.sentences().enumerate() {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|token| !token.is_whitespace)
            .collect();
        let folded = tokens.iter().map(|token| fold(&token.text)).collect();
        let words = Words { tokens, folded };
        let mut at = 0;
        while at < words.tokens.len() {
            let found = entries().iter().find(|entry| {
                (options.fillers || entry.category != VagueWordCategory::Filler)
                    && at + entry.words.len() <= words.tokens.len()
                    && entry
                        .words
                        .iter()
                        .enumerate()
                        .all(|(offset, word)| words.word_matches(at + offset, word, entry))
                    && words.guard(entry, at, at + entry.words.len())
            });
            let Some(entry) = found else {
                at += 1;
                continue;
            };
            let last = at + entry.words.len() - 1;
            matches.push(VagueWord {
                span: Span::covering(&words.tokens[at].span, &words.tokens[last].span),
                sentence: index,
                term: entry.term.to_owned(),
                category: entry.category,
                mode: document.mode_at(words.tokens[at].span.byte.start),
            });
            at = last + 1;
        }
    }
    let mut terms: BTreeMap<&str, VagueWordTerm> = BTreeMap::new();
    let mut groups: Split<VagueWordsGroup> = word_split(document).map(|words| VagueWordsGroup {
        words,
        ..Default::default()
    });
    for found in &matches {
        let term = terms
            .entry(found.term.as_str())
            .or_insert_with(|| VagueWordTerm {
                term: found.term.clone(),
                category: found.category,
                count: 0,
                narration: 0,
                dialogue: 0,
            });
        term.count += 1;
        match found.mode {
            Mode::Narration => term.narration += 1,
            Mode::Dialogue => term.dialogue += 1,
        }
        for group in groups.all_and_mode_mut(found.mode) {
            group.matches += 1;
        }
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        group.per_thousand_words =
            (group.words > 0).then(|| 1000.0 * group.matches as f64 / group.words as f64);
    }
    let mut terms: Vec<_> = terms.into_values().collect();
    terms.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.term.cmp(&b.term)));
    Ok(VagueWordsReport {
        matches,
        terms,
        groups,
    })
}
