//! Narrative helpers shared by the voice layer (#353) and the person-drift (#354) and tense-shift
//! (#355) lenses.
//!
//! `NarrativePerson` and `NarrativeTense` are the public API: the lenses' reports use them. The
//! rest of this module is shared lens machinery, public only so `style/tests/narrative.rs` can
//! pin its behaviour. It is hidden from the docs and may change without notice. Extend it here
//! rather than forking it inside a lens, because a change moves every lens that uses it.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::reports::dialogue_tags::{has_pos, lower};
use crate::{Document, Span, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum NarrativePerson {
    First,
    Second,
    Third,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum NarrativeTense {
    Past,
    Present,
}

/// A run of paragraphs between scene breaks or chapter starts.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub index: usize,
    pub paragraphs: Range<usize>,
    /// Global sentence indexes, as in `document.sentences().enumerate()`.
    pub sentences: Range<usize>,
    pub span: Span,
}

/// A new scene starts at paragraph 0 and at every paragraph with `scene_break_before` or
/// `chapter_start`, the rule sensory balance uses.
#[doc(hidden)]
pub fn scenes(document: &Document) -> Vec<Scene> {
    let mut scenes: Vec<Scene> = Vec::new();
    let mut sentence = 0;
    for (index, paragraph) in document.paragraphs.iter().enumerate() {
        if index == 0 || paragraph.scene_break_before || paragraph.chapter_start {
            scenes.push(Scene {
                index: scenes.len(),
                paragraphs: index..index,
                sentences: sentence..sentence,
                span: paragraph.span.clone(),
            });
        }
        let scene = scenes.last_mut().unwrap();
        sentence += paragraph.sentences.len();
        scene.paragraphs.end = index + 1;
        scene.sentences.end = sentence;
        scene.span = Span::covering(&scene.span, &paragraph.span);
    }
    scenes
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainClass {
    Past,
    PastPerfect,
    Present,
    Modal {
        present: bool,
    },
    /// A form that reads as past or present (*set*, *put*, *read*) with no singular subject to
    /// decide it. Lenses treat it as no evidence.
    Ambiguous,
}

/// A finite verb chain. Indexes point into the token slice given to `chains`.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chain {
    pub head: usize,
    pub end: usize,
    pub subject: Option<usize>,
    /// The index into `clause_starts` of the clause holding the head.
    pub clause: usize,
    pub class: ChainClass,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Subject,
    Object,
    Possessive,
    Reflexive,
}

fn normalized(token: &Token) -> String {
    lower(token).replace('’', "'")
}

fn chunk(token: &Token, label: &str) -> bool {
    token.chunks.iter().any(|chunk| chunk == label)
}

/// A common-noun reading; a capitalized word's NNP reading does not count (*Is it here?*).
fn common_noun(token: &Token) -> bool {
    token.readings.iter().any(|reading| {
        reading.pos == "NN" || reading.pos == "NNS" || reading.pos.starts_with("NN:")
    })
}

/// No chunk, or only the outside label `O`: the chunker made no phrase claim.
fn unchunked(token: &Token) -> bool {
    token.chunks.iter().all(|chunk| chunk == "O")
}

fn verbal(token: &Token) -> bool {
    has_pos(token, "VB") || has_pos(token, "MD")
}

/// *’Tis*, *’Twas*: the tagger reads them as nouns.
fn elided(token: &Token) -> Option<ChainClass> {
    match normalized(token).trim_start_matches('\'') {
        "tis" => Some(ChainClass::Present),
        "twas" => Some(ChainClass::Past),
        _ => None,
    }
}

fn readings<'a>(token: &'a Token, pos: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    token
        .readings
        .iter()
        .filter(move |reading| reading.pos == pos)
        .map(|reading| reading.lemma.as_str())
}

fn finite(token: &Token) -> bool {
    ["VBD", "VBZ", "VBP", "MD"]
        .iter()
        .any(|pos| readings(token, pos).next().is_some())
}

fn negation(token: &Token) -> bool {
    matches!(normalized(token).as_str(), "not" | "n't" | "never")
}

fn adverb_only(token: &Token) -> bool {
    has_pos(token, "RB") && !verbal(token) && !has_pos(token, "NN")
}

const SUBJECT_PRONOUNS: [&str; 7] = ["i", "you", "he", "she", "it", "we", "they"];

fn subject_of(tokens: &[&Token], head: usize) -> Option<usize> {
    let mut index = head;
    loop {
        index = index.checked_sub(1)?;
        let token = tokens[index];
        if token.is_word && (adverb_only(token) || negation(token)) && !has_pos(token, "PRP") {
            continue;
        }
        if !token.is_word {
            return None;
        }
        let pronoun = SUBJECT_PRONOUNS.contains(&lower(token).as_str())
            && token
                .readings
                .iter()
                .any(|reading| reading.pos == "PRP" || reading.pos.starts_with("PRP_S"));
        let noun_phrase_end = token.chunks.iter().any(|chunk| chunk.starts_with("E-NP"));
        let wh = has_pos(token, "WP") || has_pos(token, "WDT");
        return (pronoun || noun_phrase_end || has_pos(token, "EX") || wh).then_some(index);
    }
}

fn third_singular(token: &Token) -> bool {
    let word = lower(token);
    if SUBJECT_PRONOUNS.contains(&word.as_str()) {
        return matches!(word.as_str(), "he" | "she" | "it");
    }
    token.is_skipped
        || token.readings.iter().any(|reading| {
            reading.pos == "NN" || reading.pos.starts_with("NN:") || reading.pos == "NNP"
        })
}

/// Modals that are never nouns or names (*will*, *can*, *may* and *might* can be).
const STRICT_MODALS: [&str; 11] = [
    "would", "could", "should", "must", "shall", "'d", "'ll", "ca", "wo", "ought", "cannot",
];

const PRESENT_MODALS: [&str; 9] = [
    "will", "shall", "can", "ca", "may", "'ll", "wo", "won't", "can't",
];

fn class(tokens: &[&Token], head: usize, end: usize, subject: Option<usize>) -> ChainClass {
    let token = tokens[head];
    if let Some(class) = elided(token) {
        return class;
    }
    let word = normalized(token);
    let past: Vec<&str> = readings(token, "VBD").collect();
    let present: Vec<&str> = readings(token, "VBZ")
        .chain(readings(token, "VBP"))
        .collect();
    if (past.contains(&"have") || word == "'d")
        && tokens[head + 1..end]
            .iter()
            .any(|token| readings(token, "VBN").next().is_some())
    {
        return ChainClass::PastPerfect;
    }
    if readings(token, "MD").next().is_some() {
        // *must* has no past form, so past narration uses it too (*He must hurry.*).
        if word == "must" {
            return ChainClass::Ambiguous;
        }
        // A perfect modal (*will have gone*, *can’t have known*) refers to the past.
        let perfect = tokens[head + 1..end].windows(2).any(|pair| {
            readings(pair[0], "VB").any(|lemma| lemma == "have")
                && readings(pair[1], "VBN").next().is_some()
        });
        return ChainClass::Modal {
            present: !perfect && (PRESENT_MODALS.contains(&word.as_str()) || word == "cannot"),
        };
    }
    match (past.is_empty(), present.is_empty()) {
        (false, true) => ChainClass::Past,
        (true, false) => ChainClass::Present,
        (false, false) => {
            // One lemma read as past and present (*set*, *put*, *read*) is decided by a singular
            // subject (*she set*); a different present lemma (*fell* the trees) is a rarer verb.
            let same_lemma = past.iter().any(|lemma| present.contains(lemma));
            if !same_lemma || subject.is_some_and(|subject| third_singular(tokens[subject])) {
                ChainClass::Past
            } else {
                ChainClass::Ambiguous
            }
        }
        (true, true) => ChainClass::Ambiguous,
    }
}

/// Indexes where clauses start: 0, after , ; : — – …, and at subordinators and wh-words.
#[doc(hidden)]
pub fn clause_starts(tokens: &[&Token]) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, token) in tokens.iter().enumerate() {
        let boundary_after = matches!(token.text.as_str(), "," | ";" | ":" | "—" | "–" | "…");
        if boundary_after && index + 1 < tokens.len() {
            starts.push(index + 1);
        }
        let opens = index > 0
            && token.is_word
            && (chunk(token, "B-SBAR")
                || has_pos(token, "WDT")
                || has_pos(token, "WP")
                || has_pos(token, "WRB"));
        if opens {
            starts.push(index);
        }
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Finite verb chains in a run of non-whitespace tokens.
#[doc(hidden)]
pub fn chains(tokens: &[&Token]) -> Vec<Chain> {
    let starts = clause_starts(tokens);
    let mut chains = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // A particle the chunker opened a verb phrase on (*set it down*) is not a verb. An
        // unchunked word needs a verb-only reading (*Yes*, *Mine*, *Sick* also read as verbs). A
        // modal opens a chain even when the chunker put it in a noun phrase (*what chance would
        // there have been*), and the bare verb after an inverted modal belongs to it.
        let modal = readings(token, "MD").next().is_some()
            && STRICT_MODALS.contains(&normalized(token).as_str());
        let after_modal = readings(token, "VB").next().is_some()
            && readings(token, "VBD").next().is_none()
            && (index.saturating_sub(3)..index).any(|previous| {
                readings(tokens[previous], "MD").next().is_some()
                    && tokens[previous + 1..index].iter().all(|between| {
                        between.is_word
                            && (has_pos(between, "PRP")
                                || has_pos(between, "EX")
                                || adverb_only(between))
                    })
            });
        let starts_chain = token.is_word
            && !token.is_skipped
            && !has_pos(token, "RP")
            && !after_modal
            && (elided(token).is_some()
                || modal
                || (finite(token)
                    && (chunk(token, "B-VP")
                        || (unchunked(token)
                            && !common_noun(token)
                            && !["JJ", "PRP$", "UH", "RB"]
                                .iter()
                                .any(|pos| has_pos(token, pos))
                            && !index.checked_sub(1).is_some_and(|p| verbal(tokens[p]))))));
        if !starts_chain {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while let Some(next) = tokens.get(end) {
            let extends = next.is_word
                && (chunk(next, "I-VP")
                    || negation(next)
                    || adverb_only(next)
                    || (unchunked(next) && (has_pos(next, "VB") || has_pos(next, "TO"))));
            if !extends {
                break;
            }
            end += 1;
        }
        let subject = subject_of(tokens, index);
        chains.push(Chain {
            head: index,
            end,
            subject,
            clause: starts.partition_point(|&start| start <= index) - 1,
            class: class(tokens, index, end, subject),
        });
        index = end;
    }
    chains
}

const NUMERAL_CONTEXT: [&str; 16] = [
    "chapter", "part", "book", "act", "scene", "king", "queen", "henry", "war", "letter", "volume",
    "section", "appendix", "figure", "table", "canto",
];

fn first_person_i(tokens: &[&Token], index: usize) -> bool {
    let token = tokens[index];
    let surface = token.text.replace('’', "'");
    let pronoun = surface == "I" || matches!(surface.as_str(), "I'm" | "I'd" | "I'll" | "I've");
    if !pronoun {
        return false;
    }
    let numeral = !token.readings.is_empty()
        && token
            .readings
            .iter()
            .all(|reading| reading.pos.starts_with("NNP") || reading.pos == "CD");
    let previous = index.checked_sub(1).map(|previous| tokens[previous]);
    let named = previous.is_some_and(|previous| {
        NUMERAL_CONTEXT.contains(&lower(previous).as_str())
            || (!previous.readings.is_empty()
                && previous
                    .readings
                    .iter()
                    .all(|reading| reading.pos.starts_with("NNP")))
    });
    !numeral && !named
}

/// The grammatical person of a personal pronoun. *they*, *it* and *one* return `None`, and so does
/// a Roman-numeral or letter *I* (*Henry I*, *Chapter I*).
#[doc(hidden)]
pub fn pronoun_person(tokens: &[&Token], index: usize) -> Option<(NarrativePerson, Case)> {
    use Case::*;
    use NarrativePerson::*;
    let token = tokens[index];
    if !token.is_word || token.is_skipped {
        return None;
    }
    if token.text.starts_with('I') && token.text.chars().count() <= 4 {
        let surface = token.text.replace('’', "'");
        if surface == "I" || surface.starts_with("I'") {
            return first_person_i(tokens, index).then_some((First, Subject));
        }
    }
    if token.text == "US" {
        return None;
    }
    let possessive_her = || {
        has_pos(token, "PRP$")
            && tokens.get(index + 1).is_some_and(|next| {
                next.is_word && (has_pos(next, "NN") || has_pos(next, "JJ")) && !has_pos(next, "IN")
            })
    };
    Some(match normalized(token).as_str() {
        "me" => (First, Object),
        "my" | "mine" => (First, Possessive),
        "myself" => (First, Reflexive),
        "we" => (First, Subject),
        "us" => (First, Object),
        "our" | "ours" => (First, Possessive),
        "ourselves" => (First, Reflexive),
        "you" => (Second, Subject),
        "your" | "yours" => (Second, Possessive),
        "yourself" | "yourselves" => (Second, Reflexive),
        "he" | "she" => (Third, Subject),
        "him" => (Third, Object),
        "her" if possessive_her() => (Third, Possessive),
        "her" => (Third, Object),
        "his" | "hers" => (Third, Possessive),
        "himself" | "herself" => (Third, Reflexive),
        _ => return None,
    })
}

/// A clause that opens with a bare verb and no subject: *Get out.*, *Keep moving.* The verb must
/// be chunked as a verb phrase, or carry no noun or adjective reading, so noun-initial fragments
/// (*Run-down shack*) are not imperatives.
#[doc(hidden)]
pub fn imperative_start(tokens: &[&Token]) -> bool {
    let Some(first) = tokens.iter().position(|token| token.is_word) else {
        return false;
    };
    let token = tokens[first];
    if token.is_skipped || readings(token, "VB").next().is_none() {
        return false;
    }
    // Chunked as a verb phrase, or with no noun, name or adjective reading whatever the chunk
    // (*Breathe slowly* is chunked as a noun phrase; *Harry* is a name and a verb).
    let chunked = chunk(token, "B-VP");
    let bare = !common_noun(token) && !has_pos(token, "JJ") && !has_pos(token, "NNP");
    // *Do you…*, *Have you…*: a question, not an order.
    let question = tokens
        .get(first + 1)
        .is_some_and(|next| SUBJECT_PRONOUNS.contains(&lower(next).as_str()));
    (chunked || bare) && !question
}

fn opening_quote(text: &str, byte: usize, ch: char) -> bool {
    match ch {
        '“' | '"' => true,
        '‘' | '\'' => {
            let previous = text
                .get(..byte)
                .and_then(|before| before.chars().next_back());
            let after = text.get(byte + ch.len_utf8()..).unwrap_or("");
            !previous.is_some_and(char::is_alphanumeric)
                && after.chars().next().is_some_and(char::is_alphanumeric)
                && !crate::document::leading_elision(after)
        }
        _ => false,
    }
}

/// False when the paragraph has an opening quote outside every dialogue span, an unclosed quote
/// that dialogue detection could not pair, or more opening than closing double quotes (a
/// multi-paragraph quotation, whose span ends at the first nested closing quote: Father Mapple's
/// sermon in Moby-Dick quotes *“Jack, he’s robbed a widow;”* inside it). Voice and mode there are
/// unreliable.
#[doc(hidden)]
pub fn paragraph_quotes_balanced(document: &Document, paragraph: usize) -> bool {
    let Some(paragraph) = document.paragraphs.get(paragraph) else {
        return true;
    };
    let count = |quote: char| paragraph.text.chars().filter(|&ch| ch == quote).count();
    if count('“') > count('”') || count('"') % 2 == 1 {
        return false;
    }
    let base = paragraph.span.byte.start;
    paragraph.text.char_indices().all(|(local, ch)| {
        if !opening_quote(&paragraph.text, local, ch) {
            return true;
        }
        let byte = base + local;
        let index = document
            .dialogue
            .partition_point(|span| span.byte.end <= byte);
        document
            .dialogue
            .get(index)
            .is_some_and(|span| span.byte.start <= byte)
    })
}

// Tense votes (#355). `sentence_tense` and `scene_tense` drive the tense-shift lens and person
// drift's tense evidence (#354); a change here moves both lenses, so run both test suites.

/// Why a finite chain, or a whole sentence, does not vote on the narrative tense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum TenseExemption {
    /// A modal-headed chain, present (*can*, *will*) or past (*would*, *could*).
    Modal,
    /// Past perfect in a present scene: backstory, never a minority signal.
    PastPerfect,
    /// A relative, adverbial or comparative clause: *the house where she lives*, *as if*.
    Subordinate,
    /// A clause reported by a speech, cognition or perception verb, or after a display colon.
    Reported,
    /// A generic subject: *you*, *one*, *nobody*, a bare plural, *a man*, *that is*, *it is … to*.
    Generic,
    /// A habitual marker in the chain's clause: *always*, *often*, *every Sunday*, *whenever*.
    Habitual,
    /// A first-person narrator frame: *I remember*, *I suppose*.
    NarratorFrame,
    /// A comment clause set off by punctuation: *it turns out*, *it seems*, *the truth is*.
    Parenthetical,
    /// A question or exclamation: every chain in it.
    QuestionOrExclamation,
    /// No explicit subject: imperatives, inversions, subjectless coordinated verb phrases.
    NoSubject,
    /// A form that reads as past or present with no subject to decide it (*they set*).
    Ambiguous,
    /// A sentence with past and present main clauses, counted and never flagged.
    MixedSentence,
    /// A sentence in a paragraph with an unpaired quote.
    UnbalancedQuotes,
    /// A sentence of a present run that ends a past scene beside *now* or *still*: the narrator's
    /// present after the story (*The motel is still here.*). Counted per sentence.
    Coda,
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SentenceTense {
    Past,
    Present,
    Mixed,
    #[default]
    Unclassified,
}

/// One finite chain the vote looked at. Indexes point into `sentence.tokens`.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteChain {
    pub head: usize,
    /// One past the chain's last token.
    pub end: usize,
    pub subject: Option<usize>,
    pub class: ChainClass,
    /// Why the chain did not vote; `None` for a voting chain.
    pub exemption: Option<TenseExemption>,
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SentenceVote {
    pub class: SentenceTense,
    /// Indexes into `sentence.tokens` of the voting chain heads, in order.
    pub voting: Vec<usize>,
    /// One reason per chain that did not vote.
    pub exemptions: Vec<TenseExemption>,
    /// Every past vote is past perfect.
    pub past_perfect_only: bool,
    /// Every voting chain has a specific subject: a personal pronoun, a name, or a noun phrase
    /// opened by *the*, a possessive or a demonstrative determiner.
    pub specific: bool,
    /// One present-simple dynamic verb with a definite inanimate subject (*The river runs east.*).
    pub gnomic: bool,
    /// The sentence holds a voice span and every voting head is a speech or thought verb: a tag.
    pub tag_only: bool,
    /// A backstory anchor in the narration: *ago*, *last*, *once*, *years*, *back then*, *used
    /// to*, *when she was*, or a *remember*/*recall* verb.
    pub backstory: bool,
    /// *now* or *still* in the narration: with a run that ends a past scene, the narrator's
    /// present (`TenseExemption::Coda`).
    pub narrator_now: bool,
    /// Every chain in the sentence's narration, voting or not, in order.
    pub chains: Vec<VoteChain>,
}

/// What `sentence_tense` needs from outside the sentence.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenseContext {
    /// `paragraph_quotes_balanced` for the sentence's paragraph.
    pub balanced_quotes: bool,
}

impl Default for TenseContext {
    fn default() -> Self {
        Self {
            balanced_quotes: true,
        }
    }
}

impl TenseContext {
    pub fn new(document: &Document, paragraph: usize) -> Self {
        Self {
            balanced_quotes: paragraph_quotes_balanced(document, paragraph),
        }
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TenseThresholds {
    pub minimum_sentences: usize,
    pub minimum_majority_percent: f64,
}

impl Default for TenseThresholds {
    fn default() -> Self {
        Self {
            minimum_sentences: 6,
            minimum_majority_percent: 70.0,
        }
    }
}

impl TenseThresholds {
    /// The majority tense of `past` and `present` classified sentences, or `None` below the
    /// sentence floor or the majority share.
    pub fn infer(&self, past: usize, present: usize) -> Option<NarrativeTense> {
        let total = past + present;
        if total == 0 || total < self.minimum_sentences {
            return None;
        }
        let (tense, majority) = if past >= present {
            (NarrativeTense::Past, past)
        } else {
            (NarrativeTense::Present, present)
        };
        (100.0 * majority as f64 >= self.minimum_majority_percent * total as f64).then_some(tense)
    }
}

const SUBORDINATORS: [&str; 16] = [
    "that", "if", "whether", "because", "while", "as", "though", "although", "since", "unless",
    "until", "when", "where", "than", "like", "once",
];
const EXTRA_REPORTING: [&str; 8] = [
    "see", "hear", "notice", "learn", "tell", "explain", "promise", "warn",
];
const INDEFINITE: [&str; 22] = [
    "nobody",
    "somebody",
    "someone",
    "anyone",
    "anybody",
    "everyone",
    "everybody",
    "nothing",
    "something",
    "everything",
    "anything",
    "none",
    "each",
    "all",
    "some",
    "many",
    "few",
    "most",
    "both",
    "either",
    "neither",
    "noone",
];
const DEMONSTRATIVES: [&str; 4] = ["this", "that", "these", "those"];
/// *never* and *ever* are left out: a negated one-off action is narration (*He never looks at
/// her.* in a past scene is a slip), and *never* also reads as negation in `negation`.
const HABITUAL: [&str; 7] = [
    "always",
    "often",
    "usually",
    "sometimes",
    "seldom",
    "rarely",
    "whenever",
];
const TIME_NOUNS: [&str; 24] = [
    "day",
    "night",
    "morning",
    "evening",
    "afternoon",
    "week",
    "month",
    "year",
    "time",
    "hour",
    "summer",
    "winter",
    "spring",
    "autumn",
    "fall",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "weekend",
    "season",
];
/// *say* and *tell* are scene verbs as often as frames (*I tell him to hurry*, *We say goodbye*);
/// they are frames only in `speech_frame`'s shapes (*I tell you*, *I say,*, *I must say that*).
const FRAME: [&str; 18] = [
    "remember", "recall", "think", "know", "suppose", "wonder", "believe", "guess", "imagine",
    "admit", "confess", "mean", "hope", "doubt", "suspect", "swear", "grant", "expect",
];
/// Participles whose first-person present perfect is the narrator's experience up to now (*I have
/// known a lot of losses*, *we have heard no more*); others are scene actions (*I have counted
/// every boat*) unless an experiential adverb comes with them (*I have never forgotten it*).
const PERFECT_FRAME: [&str; 10] = [
    "know", "hear", "see", "be", "learn", "wonder", "think", "forget", "remember", "meet",
];
const PERFECT_ADVERBS: [&str; 7] = [
    "never", "ever", "often", "always", "since", "seldom", "rarely",
];
/// Comment clauses set off by punctuation: *The fog, it turns out, rolled in.* *But, the truth
/// is, the boats were empty.* The head lemma, with *it* or with *the* + noun as subject.
const PARENTHETICAL_IT: [&str; 4] = ["seem", "appear", "turn", "happen"];
const PARENTHETICAL_NOUNS: [&str; 4] = ["truth", "fact", "point", "trouble"];
/// Narrator-frame verbs beyond the issue's list, added from the #355 development labels: a first-
/// person narrator recollecting or judging in the present (*I recollect the wet housetops*, *I
/// picture my small self*, *I fear to approach it*). *feel*, *find* and *seem* are left out: they
/// are scene verbs as often as frames (*I feel the cold*).
const FRAME_EXTRA: [&str; 13] = [
    "recollect",
    "picture",
    "fear",
    "trust",
    "own",
    "fancy",
    "assure",
    "beg",
    "declare",
    "acknowledge",
    "regret",
    "venture",
    "presume",
];
/// Attitude adjectives after a first-person *be* (*I am glad to think*, *I am sorry to say*).
const FRAME_ADJECTIVES: [&str; 12] = [
    "glad",
    "sorry",
    "afraid",
    "thankful",
    "sure",
    "certain",
    "bound",
    "ashamed",
    "aware",
    "conscious",
    "persuaded",
    "convinced",
];
/// Deictics of the narrator's present: a present chain beside one is the narrator's now
/// (*Nowadays we have revues.*), not the scene's. *now* and *still* are left out: they are common
/// in past narration (*She was still holding it*), and exempting them cost 9 points of synthetic
/// run recall for little precision (docs/STYLE-TENSE-SHIFT.md).
const NARRATOR_NOW: [&str; 1] = ["nowadays"];
const DISPLAY_VERBS: [&str; 7] = [
    "said",
    "read",
    "wrote",
    "thought",
    "ran",
    "stated",
    "announced",
];
const DISPLAY_NOUNS: [&str; 10] = [
    "sign",
    "note",
    "letter",
    "notice",
    "label",
    "message",
    "text",
    "card",
    "inscription",
    "headline",
];
const STATIVE: [&str; 3] = ["be", "have", "do"];
/// Animate heads a gnomic subject must not have: *The man walks in* is a slip, not an aphorism.
const PERSON_NOUNS: [&str; 24] = [
    "man", "woman", "boy", "girl", "child", "kid", "baby", "mother", "father", "mom", "dad",
    "brother", "sister", "son", "daughter", "wife", "husband", "friend", "stranger", "doctor",
    "officer", "driver", "dog", "cat",
];
const BOUNDARIES: [&str; 10] = [",", ";", ":", "—", "–", "…", "(", ")", ".", "-"];

fn reporting_verbs() -> &'static std::collections::HashSet<&'static str> {
    static LIST: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        crate::reports::dialogue_tags::speech_verbs()
            .chain(crate::reports::pacing::cognition_lemmas())
            .chain(crate::voices::thought_verbs().iter().copied())
            .chain(EXTRA_REPORTING)
            .collect()
    })
}

fn tag_verbs() -> &'static std::collections::HashSet<&'static str> {
    static LIST: std::sync::OnceLock<std::collections::HashSet<&'static str>> =
        std::sync::OnceLock::new();
    LIST.get_or_init(|| {
        crate::reports::dialogue_tags::speech_verbs()
            .chain(crate::voices::thought_verbs().iter().copied())
            .collect()
    })
}

fn verb_lemmas(token: &Token) -> impl Iterator<Item = &str> {
    token
        .readings
        .iter()
        .filter(|reading| reading.pos.starts_with("VB"))
        .map(|reading| reading.lemma.as_str())
}

fn has_verb_lemma(token: &Token, list: impl Fn(&str) -> bool) -> bool {
    verb_lemmas(token).any(list)
}

fn boundary(token: &Token) -> bool {
    BOUNDARIES.contains(&token.text.as_str())
}

fn coordinator(token: &Token) -> bool {
    matches!(
        normalized(token).as_str(),
        "and" | "but" | "or" | "nor" | "yet" | "so" | "then"
    )
}

/// A writing or display verb or noun before a colon: *The sign read:*, *a note: back soon*.
#[doc(hidden)]
pub fn display_colon_lead_in(tokens: &[&Token], i: usize) -> bool {
    if tokens.get(i).is_none_or(|token| token.text != ":") {
        return false;
    }
    let mut index = i;
    let mut nearest = true;
    while let Some(previous) = index.checked_sub(1) {
        index = previous;
        let token = tokens[index];
        if !token.is_word {
            if boundary(token) {
                return false;
            }
            continue;
        }
        let word = normalized(token);
        if nearest && DISPLAY_VERBS.contains(&word.as_str()) {
            return true;
        }
        nearest = false;
        let singular = word.strip_suffix('s').unwrap_or(&word);
        if DISPLAY_NOUNS.contains(&word.as_str()) || DISPLAY_NOUNS.contains(&singular) {
            return true;
        }
    }
    false
}

/// The first token of the noun phrase that ends at `subject`.
fn noun_phrase_start(tokens: &[&Token], subject: usize) -> usize {
    let mut start = subject;
    while !tokens[start].chunks.iter().any(|c| c.starts_with("B-NP")) && start > 0 {
        let previous = tokens[start - 1];
        let inside = previous.is_word
            && previous
                .chunks
                .iter()
                .any(|c| c.starts_with("B-NP") || c.starts_with("I-NP"));
        if !inside {
            break;
        }
        start -= 1;
    }
    start
}

fn head_is_be(token: &Token) -> bool {
    has_verb_lemma(token, |lemma| lemma == "be")
}

fn reported(tokens: &[&Token], found: &[Chain], n: usize, np: usize) -> bool {
    let chain = found[n];
    // *or so she thinks*: *so* stands in for the reported clause.
    if np > 0
        && normalized(tokens[np - 1]) == "so"
        && has_verb_lemma(tokens[chain.head], |lemma| {
            reporting_verbs().contains(lemma)
        })
    {
        return true;
    }
    if (0..np).any(|i| display_colon_lead_in(tokens, i)) {
        return true;
    }
    // The nearest verb before the subject, with no boundary between: *She said the train leaves*,
    // *She knew that he lies*, *He told her she is late*.
    let mut index = np;
    let mut clause_marks = 0;
    while let Some(previous) = index.checked_sub(1) {
        index = previous;
        let token = tokens[index];
        if !token.is_word {
            if boundary(token) {
                return false;
            }
            continue;
        }
        if coordinator(token) {
            return false;
        }
        if matches!(normalized(token).as_str(), "that" | "if" | "whether") {
            clause_marks += 1;
            if clause_marks > 1 {
                return false;
            }
            continue;
        }
        let verb = token.readings.iter().any(|r| r.pos.starts_with("VB"))
            && token.chunks.iter().any(|c| c.ends_with("-VP"));
        if verb {
            return has_verb_lemma(token, |lemma| reporting_verbs().contains(lemma));
        }
    }
    false
}

fn subordinate(tokens: &[&Token], found: &[Chain], n: usize, start: usize, np: usize) -> bool {
    let chain = found[n];
    let subject = chain.subject.unwrap_or(chain.head);
    let word = normalized(tokens[subject]);
    let wh = has_pos(tokens[subject], "WP") || has_pos(tokens[subject], "WDT");
    // A demonstrative *that* opening a sentence or a coordinated or punctuated clause is a
    // pronoun subject (*That is the trouble*); after a noun it is a relative (*a sign that said*).
    let demonstrative = DEMONSTRATIVES.contains(&word.as_str())
        && subject == start
        && subject
            .checked_sub(1)
            .is_none_or(|previous| boundary(tokens[previous]) || coordinator(tokens[previous]));
    if wh && !demonstrative {
        return true;
    }
    // The subject's noun phrase can open before the clause start the chunker gave (*those rites
    // and ceremonies which are instituted*); then nothing lies between them.
    let opener = tokens
        .get(start..np)
        .unwrap_or_default()
        .iter()
        .any(|token| {
            token.is_word
                && (chunk(token, "B-SBAR")
                    || has_pos(token, "WDT")
                    || has_pos(token, "WP")
                    || has_pos(token, "WRB")
                    || SUBORDINATORS.contains(&normalized(token).as_str()))
        });
    if opener {
        return true;
    }
    // *the way you learn a scar*.
    if np >= 2 && normalized(tokens[np - 1]) == "way" && normalized(tokens[np - 2]) == "the" {
        return true;
    }
    // A finite chain earlier in the same clause with no coordinator before this subject: a
    // contact relative or complement (*the summer we met*, *It seemed she is gone*).
    found[..n].iter().any(|earlier| {
        earlier.clause == chain.clause
            && earlier.end <= np
            && !tokens[earlier.end..np]
                .iter()
                .any(|token| coordinator(token) || boundary(token))
    })
}

fn determiner(token: &Token) -> bool {
    has_pos(token, "DT")
        || has_pos(token, "PRP$")
        || has_pos(token, "POS")
        || has_pos(token, "CD")
        || has_pos(token, "PDT")
        || matches!(normalized(token).as_str(), "'s" | "'")
}

fn generic(tokens: &[&Token], chain: &Chain, subject: usize, np: usize) -> bool {
    let word = normalized(tokens[subject]);
    if matches!(word.as_str(), "you" | "one" | "ya" | "y'all")
        || INDEFINITE.contains(&word.as_str())
    {
        return true;
    }
    if tokens[np..=subject].iter().any(|token| {
        matches!(
            normalized(token).as_str(),
            "your" | "a" | "an" | "no" | "every"
        )
    }) {
        return true;
    }
    let head = tokens[chain.head];
    if DEMONSTRATIVES.contains(&word.as_str()) && np == subject && head_is_be(head) {
        return true;
    }
    if word == "it" && head_is_be(head) {
        let complement = tokens[chain.end..]
            .iter()
            .take_while(|token| !boundary(token))
            .any(|token| matches!(normalized(token).as_str(), "to" | "that" | "how"));
        if complement {
            return true;
        }
    }
    let token = tokens[subject];
    let pronoun = SUBJECT_PRONOUNS.contains(&word.as_str());
    let bare = !pronoun
        && !token.is_skipped
        && !has_pos(token, "NNP")
        && token
            .readings
            .iter()
            .any(|r| r.pos == "NNS" || r.pos == "NN:U")
        && !tokens[np..subject].iter().any(|t| determiner(t))
        && !tokens[np..subject]
            .iter()
            .any(|t| t.is_skipped || has_pos(t, "NNP"));
    bare
}

fn habitual_marker(tokens: &[&Token], i: usize) -> bool {
    let word = normalized(tokens[i]);
    HABITUAL.contains(&word.as_str())
        || (matches!(word.as_str(), "every" | "each")
            && tokens
                .get(i + 1)
                .is_some_and(|next| TIME_NOUNS.contains(&normalized(next).as_str())))
}

/// A habitual marker in the chain's own clause (*She always walks home*), or in a clause with no
/// finite chain next to it (*Every Sunday, she goes to church*), or a *whenever* clause anywhere in
/// the run. A marker inside another finite clause belongs to that clause: *He walks down the pier,
/// the way he did every morning* is a slip.
fn habitual(tokens: &[&Token], starts: &[usize], found: &[Chain], chain: &Chain) -> bool {
    let clause_range = |c: usize| starts[c]..starts.get(c + 1).copied().unwrap_or(tokens.len());
    let marked = |c: usize| clause_range(c).any(|i| habitual_marker(tokens, i));
    let finite = |c: usize| found.iter().any(|other| other.clause == c);
    if marked(chain.clause) || tokens.iter().any(|token| normalized(token) == "whenever") {
        return true;
    }
    let before = (0..chain.clause).rev();
    let after = chain.clause + 1..starts.len();
    for neighbours in [before.collect::<Vec<_>>(), after.collect()] {
        for c in neighbours {
            if finite(c) {
                break;
            }
            if marked(c) {
                return true;
            }
        }
    }
    false
}

/// *say* and *tell* as narrator frames: *I tell you, …*, *I tell you that …*, *I say, …*, *I say
/// that …*. An object or a clause end makes them scene actions (*I tell him to hurry*, *I say
/// nothing*).
fn speech_frame(tokens: &[&Token], chain: &Chain) -> bool {
    let verbs = &tokens[chain.head..chain.end];
    let open = |index: usize| {
        tokens.get(index).is_none_or(|token| {
            boundary(token) || matches!(normalized(token).as_str(), "that" | "so")
        })
    };
    if verbs
        .iter()
        .any(|token| has_verb_lemma(token, |lemma| lemma == "say"))
    {
        return open(chain.end);
    }
    if verbs
        .iter()
        .any(|token| has_verb_lemma(token, |lemma| lemma == "tell"))
    {
        return tokens
            .get(chain.end)
            .is_some_and(|next| normalized(next) == "you")
            && (open(chain.end + 1)
                || tokens.get(chain.end + 1).is_some_and(|after| {
                    matches!(normalized(after).as_str(), "what" | "how" | "this")
                }));
    }
    false
}

/// A comment clause set off by punctuation (*The fog, it turns out, is thick*; *But, the truth
/// is, nobody came*): the indexes of its opening boundary, when there is one, and its closing
/// boundary.
fn parenthetical(
    tokens: &[&Token],
    chain: &Chain,
    subject: usize,
    np: usize,
) -> Option<(Option<usize>, usize)> {
    let word = normalized(tokens[subject]);
    let head = tokens[chain.head];
    let comment = (word == "it"
        && np == subject
        && has_verb_lemma(head, |lemma| PARENTHETICAL_IT.contains(&lemma)))
        || (np + 1 == subject
            && normalized(tokens[np]) == "the"
            && PARENTHETICAL_NOUNS.contains(&word.as_str())
            && head_is_be(head));
    if !comment {
        return None;
    }
    let open = match np.checked_sub(1) {
        None => None,
        Some(previous) if boundary(tokens[previous]) => Some(previous),
        Some(previous) if previous == 0 && coordinator(tokens[previous]) => None,
        Some(_) => return None,
    };
    let mut close = chain.end;
    while tokens
        .get(close)
        .is_some_and(|token| matches!(normalized(token).as_str(), "out" | "so"))
    {
        close += 1;
    }
    let closes = tokens
        .get(close)
        .is_some_and(|token| matches!(token.text.as_str(), "," | "—" | "–" | ")" | ";"));
    closes.then_some((open, close))
}

fn narrator_frame(tokens: &[&Token], chain: &Chain, subject: usize) -> bool {
    let first = tokens[subject].text == "I" || normalized(tokens[subject]) == "we";
    let verbs = &tokens[chain.head..chain.end];
    if first {
        if verbs.iter().any(|token| {
            has_verb_lemma(token, |lemma| {
                FRAME.contains(&lemma) || FRAME_EXTRA.contains(&lemma)
            })
        }) || speech_frame(tokens, chain)
        {
            return true;
        }
        // A first-person present perfect of experience ties the past to the narrator's now (*I
        // have known a lot of losses*, *Of Silver we have heard no more*, *I have never forgotten
        // it*). Other perfects are scene actions (*I have counted every boat*).
        let perfect = has_verb_lemma(tokens[chain.head], |lemma| lemma == "have")
            && verbs[1..].iter().any(|token| has_pos(token, "VBN"))
            && (verbs[1..]
                .iter()
                .any(|token| readings(token, "VBN").any(|lemma| PERFECT_FRAME.contains(&lemma)))
                || tokens[chain.head..]
                    .iter()
                    .take_while(|token| !boundary(token))
                    .any(|token| PERFECT_ADVERBS.contains(&normalized(token).as_str())));
        if perfect {
            return true;
        }
        let be = has_verb_lemma(tokens[chain.head], |lemma| lemma == "be");
        if be
            && tokens[chain.end..]
                .iter()
                .take(2)
                .any(|token| FRAME_ADJECTIVES.contains(&normalized(token).as_str()))
        {
            return true;
        }
    }
    // *nowadays*, *to this day*, *these days* or *at present* in the same narration (*now* and
    // *still* are left out; see `NARRATOR_NOW`). Deictics never vote; here they only take a
    // present vote away.
    tokens.iter().enumerate().any(|(i, token)| {
        let word = normalized(token);
        let next = tokens.get(i + 1).map(|t| normalized(t)).unwrap_or_default();
        let after = tokens.get(i + 2).map(|t| normalized(t)).unwrap_or_default();
        NARRATOR_NOW.contains(&word.as_str())
            || (word == "these" && next == "days")
            || (word == "at" && next == "present")
            || (word == "to" && next == "this" && after == "day")
    })
}

fn specific_subject(tokens: &[&Token], subject: usize, np: usize) -> bool {
    let token = tokens[subject];
    let word = normalized(token);
    if matches!(word.as_str(), "i" | "he" | "she" | "it" | "we" | "they") {
        return true;
    }
    if token.is_skipped || has_pos(token, "NNP") {
        return true;
    }
    tokens[np..subject].iter().any(|t| {
        let w = normalized(t);
        w == "the"
            || has_pos(t, "PRP$")
            || matches!(w.as_str(), "'s" | "'")
            || t.is_skipped
            || has_pos(t, "NNP")
            || DEMONSTRATIVES.contains(&w.as_str())
    })
}

fn gnomic_chain(tokens: &[&Token], chain: &Chain, subject: usize, np: usize) -> bool {
    let head = tokens[chain.head];
    let token = tokens[subject];
    let simple = tokens[chain.head + 1..chain.end]
        .iter()
        .all(|t| adverb_only(t) || negation(t));
    let dynamic = !has_verb_lemma(head, |lemma| STATIVE.contains(&lemma));
    let definite = normalized(tokens[np]) == "the";
    let inanimate = common_noun(token)
        && !token.is_skipped
        && !SUBJECT_PRONOUNS.contains(&normalized(token).as_str())
        && !PERSON_NOUNS.contains(&normalized(token).trim_end_matches('s'));
    simple && dynamic && definite && inanimate
}

fn backstory_anchor(tokens: &[&Token]) -> bool {
    tokens.iter().enumerate().any(|(i, token)| {
        let word = normalized(token);
        let next = tokens.get(i + 1).map(|t| normalized(t)).unwrap_or_default();
        let after = tokens.get(i + 2).map(|t| normalized(t)).unwrap_or_default();
        matches!(word.as_str(), "ago" | "last" | "once" | "years")
            || (word == "back" && next == "then")
            || (word == "used" && next == "to")
            || (word == "when"
                && matches!(next.as_str(), "i" | "he" | "she" | "we" | "they")
                && matches!(after.as_str(), "was" | "were"))
            || has_verb_lemma(token, |lemma| matches!(lemma, "remember" | "recall"))
    })
}

/// The sentence's last punctuation that is not a quote mark is `?` or `!`, in narration: the
/// narrator's own question (*Where is he?*), not quoted speech ending the sentence (*He yells,
/// “Get out!”*).
fn question_or_exclamation(document: &Document, sentence: &crate::Sentence) -> bool {
    sentence
        .tokens
        .iter()
        .rev()
        .filter(|token| !token.is_whitespace)
        .find(|token| {
            !token
                .text
                .chars()
                .all(|c| matches!(c, '“' | '”' | '"' | '‘' | '’' | '\'' | '«' | '»' | ')'))
        })
        .is_some_and(|token| {
            token.is_punctuation
                && (token.text.contains('?') || token.text.contains('!'))
                && document.is_narration_at(token.span.byte.start)
        })
}

/// Classifies one sentence's narration by its main-clause finite chains with explicit subjects.
/// Quoted, thought, tagged-speech and caller spans never vote (`Document::narration_runs`).
#[doc(hidden)]
pub fn sentence_tense(
    document: &Document,
    sentence: &crate::Sentence,
    context: &TenseContext,
) -> SentenceVote {
    use TenseExemption::*;
    let mut vote = SentenceVote {
        specific: true,
        ..SentenceVote::default()
    };
    if !context.balanced_quotes {
        vote.exemptions.push(UnbalancedQuotes);
        return vote;
    }
    let question = question_or_exclamation(document, sentence);
    let voiced = sentence
        .tokens
        .iter()
        .any(|token| token.is_word && !document.is_narration_at(token.span.byte.start));
    let (mut past, mut perfect, mut present) = (0, 0, 0);
    let mut tags = true;
    let mut gnomic = 0;
    for run in document.narration_runs(sentence) {
        let indexes: Vec<usize> = run
            .filter(|&index| !sentence.tokens[index].is_whitespace)
            .collect();
        let tokens: Vec<&Token> = indexes
            .iter()
            .map(|&index| &sentence.tokens[index])
            .collect();
        let starts = clause_starts(&tokens);
        let mut found = chains(&tokens);
        vote.backstory |= backstory_anchor(&tokens);
        vote.narrator_now |= tokens
            .iter()
            .any(|token| matches!(normalized(token).as_str(), "now" | "still"));
        // Comment clauses do not vote, and the verb after one takes the subject before it (*The
        // fog, it turns out, is thick*: *fog* is the subject of *is*).
        let mut comments = Vec::new();
        for (n, chain) in found.iter().enumerate() {
            let Some(subject) = chain.subject else {
                continue;
            };
            let np = noun_phrase_start(&tokens, subject);
            if let Some((open, close)) = parenthetical(&tokens, chain, subject, np) {
                comments.push((n, open, close));
            }
        }
        for &(_, open, close) in &comments {
            let Some(open) = open else {
                continue;
            };
            if let Some(next) = found
                .iter_mut()
                .find(|chain| chain.head == close + 1 && chain.subject.is_none())
            {
                if let Some(subject) = subject_of(&tokens, open) {
                    next.subject = Some(subject);
                    next.class = class(&tokens, next.head, next.end, Some(subject));
                }
            }
        }
        for (n, chain) in found.iter().enumerate() {
            let reason = if question {
                Some(QuestionOrExclamation)
            } else {
                match (chain.class, chain.subject) {
                    (ChainClass::Modal { .. }, _) => Some(Modal),
                    (ChainClass::Ambiguous, _) => Some(Ambiguous),
                    (_, None) => Some(NoSubject),
                    (_, Some(_)) if comments.iter().any(|&(c, _, _)| c == n) => Some(Parenthetical),
                    (_, Some(subject)) => {
                        let np = noun_phrase_start(&tokens, subject);
                        if reported(&tokens, &found, n, np) {
                            Some(Reported)
                        } else if subordinate(&tokens, &found, n, starts[chain.clause], np) {
                            Some(Subordinate)
                        // Generic and habitual readings are present-tense commentary (*People
                        // are strange.*). A past chain with such a subject or marker is still
                        // narration (*A package came.*, *She always walked.*) and votes past.
                        } else if chain.class == ChainClass::Present
                            && generic(&tokens, chain, subject, np)
                        {
                            Some(Generic)
                        } else if chain.class == ChainClass::Present
                            && habitual(&tokens, &starts, &found, chain)
                        {
                            Some(Habitual)
                        } else if chain.class == ChainClass::Present
                            && narrator_frame(&tokens, chain, subject)
                        {
                            Some(NarratorFrame)
                        } else {
                            None
                        }
                    }
                }
            };
            vote.chains.push(VoteChain {
                head: indexes[chain.head],
                end: indexes[chain.end - 1] + 1,
                subject: chain.subject.map(|subject| indexes[subject]),
                class: chain.class,
                exemption: reason,
            });
            if let Some(reason) = reason {
                vote.exemptions.push(reason);
                continue;
            }
            let subject = chain.subject.expect("voting chains have subjects");
            let np = noun_phrase_start(&tokens, subject);
            vote.voting.push(indexes[chain.head]);
            match chain.class {
                ChainClass::Past => past += 1,
                ChainClass::PastPerfect => perfect += 1,
                _ => {
                    present += 1;
                    gnomic += usize::from(gnomic_chain(&tokens, chain, subject, np));
                }
            }
            vote.specific &= specific_subject(&tokens, subject, np);
            // A tag: a speech or thought verb in a sentence with a voice span, or inside a tag
            // the voice layer split into its own sentence (*Where is he? she wondered.*).
            tags &= (voiced || document.in_voice_tag(tokens[chain.head].span.byte.start))
                && tokens[chain.head..chain.end]
                    .iter()
                    .any(|token| has_verb_lemma(token, |lemma| tag_verbs().contains(lemma)));
        }
    }
    vote.class = match (past + perfect > 0, present > 0) {
        (true, false) => SentenceTense::Past,
        (false, true) => SentenceTense::Present,
        (true, true) => {
            vote.exemptions.push(MixedSentence);
            SentenceTense::Mixed
        }
        (false, false) => SentenceTense::Unclassified,
    };
    vote.past_perfect_only = past == 0 && perfect > 0;
    vote.gnomic = present == 1 && gnomic == 1 && past + perfect == 0;
    vote.tag_only = !vote.voting.is_empty() && tags;
    vote
}

/// Every sentence's vote in a scene, in order, with its global sentence index.
#[doc(hidden)]
pub fn scene_votes(document: &Document, scene: &Scene) -> Vec<(usize, SentenceVote)> {
    let mut votes = Vec::new();
    let mut index = scene.sentences.start;
    for paragraph in scene.paragraphs.clone() {
        let context = TenseContext::new(document, paragraph);
        for sentence in &document.paragraphs[paragraph].sentences {
            votes.push((index, sentence_tense(document, sentence, &context)));
            index += 1;
        }
    }
    votes
}

/// Past and present classified sentences in a list of votes. Past perfect counts as past.
#[doc(hidden)]
pub fn tense_tally(votes: &[(usize, SentenceVote)]) -> (usize, usize) {
    votes
        .iter()
        .fold((0, 0), |(past, present), (_, vote)| match vote.class {
            SentenceTense::Past => (past + 1, present),
            SentenceTense::Present => (past, present + 1),
            _ => (past, present),
        })
}

/// The scene's inferred tense: at least `minimum_sentences` classified sentences and one tense
/// holding `minimum_majority_percent` of them. Declarations are the lens's business.
#[doc(hidden)]
pub fn scene_tense(
    document: &Document,
    scene: &Scene,
    thresholds: &TenseThresholds,
) -> Option<NarrativeTense> {
    let (past, present) = tense_tally(&scene_votes(document, scene));
    thresholds.infer(past, present)
}
