use serde::{Deserialize, Serialize};

use super::echoes::fold;
use super::{word_split, Share, Split};
use crate::{Document, Mode, Sentence, Span, StyleError, Token};

pub const REPORT: &str = "clarity";

/// Closed-class Penn tags that make a word a glue word.
const CLOSED: [&str; 14] = [
    "DT", "PDT", "IN", "CC", "PRP", "PRP$", "WP", "WP$", "WDT", "TO", "MD", "EX", "RP", "POS",
];
const AUXILIARIES: [&str; 3] = ["be", "have", "do"];
/// Subordinating conjunctions the chunker often labels PP or ADVP instead of SBAR.
const SUBORDINATORS: [&str; 18] = [
    "after", "although", "as", "because", "before", "if", "lest", "once", "since", "than",
    "though", "till", "unless", "until", "whereas", "whether", "while", "whilst",
];
const RELATIVES: [&str; 4] = ["WP", "WP$", "WDT", "WRB"];
const BOUNDARIES: &str = ",;:()[]{}—–…\"“”«»!?.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClarityOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
    #[serde(alias = "minimum_sentence_words")]
    pub minimum_sentence_words: usize,
    #[serde(alias = "sticky_glue_percent")]
    pub sticky_glue_percent: f64,
    #[serde(alias = "long_clause_words")]
    pub long_clause_words: usize,
}

impl Default for ClarityOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
            minimum_sentence_words: 15,
            sticky_glue_percent: 65.0,
            long_clause_words: 20,
        }
    }
}

impl ClarityOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, value: usize) -> Self {
        self.minimum_sentences = value;
        self
    }
    pub fn with_minimum_sentence_words(mut self, value: usize) -> Self {
        self.minimum_sentence_words = value;
        self
    }
    pub fn with_sticky_glue_percent(mut self, value: f64) -> Self {
        self.sticky_glue_percent = value;
        self
    }
    pub fn with_long_clause_words(mut self, value: usize) -> Self {
        self.long_clause_words = value;
        self
    }

    fn validate(&self) -> Result<(), StyleError> {
        if self.minimum_sentence_words == 0 {
            return Err(StyleError::options("minimumSentenceWords must be positive"));
        }
        if self.long_clause_words == 0 {
            return Err(StyleError::options("longClauseWords must be positive"));
        }
        if !self.sticky_glue_percent.is_finite()
            || !(0.0..=100.0).contains(&self.sticky_glue_percent)
        {
            return Err(StyleError::options("stickyGluePercent must be in 0..=100"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum ClarityClauseIntroducer {
    Subordinator,
    Relative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClarityStickySentence {
    pub span: Span,
    pub sentence: usize,
    pub mode: Mode,
    pub words: usize,
    pub glue_words: usize,
    pub glue_percent: f64,
    pub glue: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClarityLongClause {
    pub span: Span,
    pub sentence: usize,
    pub mode: Mode,
    pub introducer: String,
    pub introducer_span: Span,
    pub kind: ClarityClauseIntroducer,
    pub words: usize,
    pub sentence_words: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClarityGroup {
    pub sentences: usize,
    pub eligible_sentences: usize,
    pub low_confidence: bool,
    pub glue: Share,
    pub sticky: Share,
    pub subordinate_clauses: usize,
    pub long_clauses: usize,
    pub long_clause_sentences: Share,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ClarityReport {
    pub words: Split<usize>,
    pub sticky_sentences: Vec<ClarityStickySentence>,
    pub long_clauses: Vec<ClarityLongClause>,
    pub groups: Split<ClarityGroup>,
}

fn base(pos: &str) -> &str {
    pos.split(':').next().unwrap_or(pos)
}

fn has_tag(token: &Token, tags: &[&str]) -> bool {
    token.readings.iter().any(|r| tags.contains(&base(&r.pos)))
}

fn has_prefix(token: &Token, prefixes: &[&str]) -> bool {
    token
        .readings
        .iter()
        .any(|r| prefixes.iter().any(|p| r.pos.starts_with(p)))
}

fn in_chunk(token: &Token, kind: &str) -> bool {
    token.chunks.iter().any(|c| c.contains(kind))
}

/// Closed-class words, with the chunk resolving words that also have an open-class reading.
pub(crate) fn is_glue(token: &Token) -> bool {
    if token
        .readings
        .iter()
        .any(|r| r.pos.starts_with("VB") && AUXILIARIES.contains(&fold(&r.lemma).as_str()))
    {
        return true;
    }
    if !has_tag(token, &CLOSED) {
        return false;
    }
    if in_chunk(token, "-VP") && has_prefix(token, &["VB"]) && !has_tag(token, &["MD"]) {
        return false;
    }
    if token.chunks.iter().any(|c| c.starts_with("E-NP")) && has_prefix(token, &["NN", "JJ", "CD"])
    {
        return false;
    }
    if has_prefix(token, &["RB"])
        && !has_tag(token, &["EX"])
        && !["-PP", "-SBAR", "-PRT"]
            .iter()
            .any(|kind| in_chunk(token, kind))
    {
        return false;
    }
    !(in_chunk(token, "-ADJP") && has_prefix(token, &["JJ"]))
}

fn counted(token: &Token) -> bool {
    token.is_word && !token.is_skipped
}

fn is_boundary(token: &Token) -> bool {
    !token.is_word
        && !token.is_whitespace
        && (token.text == "--" || token.text.chars().any(|c| BOUNDARIES.contains(c)))
}

fn introducer(
    tokens: &[&Token],
    index: usize,
    first_word: bool,
    question: bool,
) -> Option<ClarityClauseIntroducer> {
    let token = tokens[index];
    let word = fold(&token.text);
    if token.chunks.iter().any(|c| c == "B-SBAR") {
        // Bare "so" is a discourse marker; "so that" introduces a clause.
        let so_that = tokens
            .get(index + 1)
            .is_some_and(|t| fold(&t.text) == "that");
        return (word != "so" || so_that).then_some(ClarityClauseIntroducer::Subordinator);
    }
    if has_tag(token, &RELATIVES) && word != "however" {
        let wh_adverb = has_tag(token, &["WRB"]);
        if first_word && (!wh_adverb || question) {
            return None;
        }
        // "that" is relative only as a one-word noun phrase, not as a determiner.
        let lone = token.chunks.iter().any(|c| c.starts_with("E-NP"))
            && token.chunks.iter().any(|c| c.starts_with("B-NP"));
        if word == "that" && !lone {
            return None;
        }
        return Some(ClarityClauseIntroducer::Relative);
    }
    if SUBORDINATORS.contains(&word.as_str()) && has_tag(token, &["IN", "CC"]) {
        // A clause, not a phrase: a noun phrase followed by a verb phrase.
        return subject_and_verb(tokens, index + 1)
            .then_some(ClarityClauseIntroducer::Subordinator);
    }
    None
}

/// Whether a noun phrase starts at `start` and a verb phrase follows it, adverbs aside.
fn subject_and_verb(tokens: &[&Token], start: usize) -> bool {
    let mut next = start;
    while tokens.get(next).is_some_and(|t| in_chunk(t, "-NP")) {
        next += 1;
    }
    if next == start {
        return false;
    }
    while tokens.get(next).is_some_and(|t| in_chunk(t, "-ADVP")) {
        next += 1;
    }
    tokens.get(next).is_some_and(|t| in_chunk(t, "-VP"))
}

struct OpenClause {
    introducer: usize,
    kind: ClarityClauseIntroducer,
    last: usize,
    words: usize,
    mode: Mode,
    has_verb: bool,
}

/// Subordinate clauses in a sentence, each running from its introducer to the next punctuation
/// boundary, change of mode, or unpunctuated main clause (a new subject and verb after a content
/// word, once the clause has its own verb); `sentence` and `sentence_words` are filled by the caller.
fn clauses(document: &Document, sentence: &Sentence) -> Vec<ClarityLongClause> {
    let tokens: Vec<_> = sentence
        .tokens
        .iter()
        .filter(|t| !t.is_whitespace)
        .collect();
    let question = tokens
        .iter()
        .rev()
        .find(|t| {
            !t.is_word && (t.text.contains('?') || t.text.contains('.') || t.text.contains('!'))
        })
        .is_some_and(|t| t.text.contains('?'));
    let mut result = Vec::new();
    let mut open: Option<OpenClause> = None;
    let mut seen_word = false;
    let close = |open: &mut Option<OpenClause>, result: &mut Vec<_>| {
        if let Some(clause) = open.take() {
            let first = tokens[clause.introducer];
            result.push(ClarityLongClause {
                span: Span::covering(&first.span, &tokens[clause.last].span),
                sentence: 0,
                mode: clause.mode,
                introducer: fold(&first.text),
                introducer_span: first.span.clone(),
                kind: clause.kind,
                words: clause.words,
                sentence_words: 0,
            });
        }
    };
    for (index, token) in tokens.iter().enumerate() {
        if is_boundary(token) {
            close(&mut open, &mut result);
            continue;
        }
        if !counted(token) {
            continue;
        }
        let mode = document.mode_at(token.span.byte.start);
        if open.as_ref().is_some_and(|clause| {
            let previous = tokens[clause.last];
            clause.mode != mode
                || (clause.has_verb
                    && token.chunks.iter().any(|c| c.starts_with("B-NP"))
                    && subject_and_verb(&tokens, index)
                    && !is_glue(previous)
                    && !in_chunk(previous, "-VP"))
        }) {
            close(&mut open, &mut result);
        }
        if let Some(clause) = open.as_mut() {
            clause.words += 1;
            clause.last = index;
            clause.has_verb |= in_chunk(token, "-VP");
        } else if let Some(kind) = introducer(&tokens, index, !seen_word, question) {
            open = Some(OpenClause {
                introducer: index,
                kind,
                last: index,
                words: 1,
                mode,
                has_verb: false,
            });
        }
        seen_word = true;
    }
    close(&mut open, &mut result);
    result
}

#[derive(Default)]
struct Tally {
    words: usize,
    glue: usize,
    sticky: usize,
    long_sentences: usize,
}

pub fn report(document: &Document, options: &ClarityOptions) -> Result<ClarityReport, StyleError> {
    options.validate()?;
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let mut sticky_sentences = Vec::new();
    let mut long_clauses = Vec::new();
    let mut groups: Split<ClarityGroup> = Split::default();
    let mut tallies: Split<Tally> = Split::default();
    for (index, sentence) in document.sentences().enumerate() {
        let mode = document.sentence_mode(sentence);
        let counted: Vec<_> = sentence.words().collect();
        let glue: Vec<_> = counted.iter().filter(|t| is_glue(t)).collect();
        let eligible = counted.len() >= options.minimum_sentence_words;
        let percent = 100.0 * glue.len() as f64 / counted.len().max(1) as f64;
        let is_sticky = eligible && percent >= options.sticky_glue_percent;
        if is_sticky {
            sticky_sentences.push(ClarityStickySentence {
                span: sentence.span.clone(),
                sentence: index,
                mode,
                words: counted.len(),
                glue_words: glue.len(),
                glue_percent: percent,
                glue: glue.iter().map(|t| t.span.clone()).collect(),
            });
        }
        let mut has_long = false;
        for clause in clauses(document, sentence) {
            for group in groups.all_and_mode_mut(clause.mode) {
                group.subordinate_clauses += 1;
            }
            if clause.words >= options.long_clause_words {
                has_long = true;
                for group in groups.all_and_mode_mut(clause.mode) {
                    group.long_clauses += 1;
                }
                long_clauses.push(ClarityLongClause {
                    sentence_words: counted.len(),
                    sentence: index,
                    ..clause
                });
            }
        }
        for group in groups.all_and_mode_mut(mode) {
            group.sentences += 1;
            group.eligible_sentences += usize::from(eligible);
        }
        for tally in tallies.all_and_mode_mut(mode) {
            tally.words += counted.len();
            tally.glue += glue.len();
            tally.sticky += usize::from(is_sticky);
            tally.long_sentences += usize::from(has_long);
        }
    }
    for (group, tally) in [
        (&mut groups.all, &tallies.all),
        (&mut groups.narration, &tallies.narration),
        (&mut groups.dialogue, &tallies.dialogue),
    ] {
        let enough = group.sentences >= options.minimum_sentences;
        group.low_confidence = group.sentences < 200.max(options.minimum_sentences);
        group.glue = Share::new(tally.glue, tally.words, enough, None);
        group.sticky = Share::new(tally.sticky, group.eligible_sentences, enough, None);
        group.long_clause_sentences =
            Share::new(tally.long_sentences, group.sentences, enough, None);
    }
    Ok(ClarityReport {
        words: word_split(document),
        sticky_sentences,
        long_clauses,
        groups,
    })
}
