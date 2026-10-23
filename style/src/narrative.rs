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

/// False when the paragraph has an opening quote outside every dialogue span: an unclosed quote
/// that dialogue detection could not pair, so voice and mode there are unreliable.
#[doc(hidden)]
pub fn paragraph_quotes_balanced(document: &Document, paragraph: usize) -> bool {
    let Some(paragraph) = document.paragraphs.get(paragraph) else {
        return true;
    };
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

/// Thresholds for `scene_tense`. The defaults are provisional: tense shift (#355) owns them, and
/// person drift (#354) uses them unchanged so both lenses read one scene tense.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneTenseThresholds {
    /// Past plus present narration chains a scene needs before it has a tense.
    pub minimum_chains: usize,
    /// The majority tense's share of those chains.
    pub minimum_share: f64,
}

impl Default for SceneTenseThresholds {
    fn default() -> Self {
        Self {
            minimum_chains: 5,
            minimum_share: 0.7,
        }
    }
}

/// Past and present narration chains in a scene: past and past-perfect chains count as past,
/// present chains as present. Modal and ambiguous chains are no evidence. Chains in voice tags
/// count, since a tag is narration in the narrator's tense.
#[doc(hidden)]
pub fn scene_tense_counts(document: &Document, scene: &Scene) -> (usize, usize) {
    let mut past = 0;
    let mut present = 0;
    for paragraph in &document.paragraphs[scene.paragraphs.clone()] {
        for sentence in &paragraph.sentences {
            for run in document.narration_runs(sentence) {
                let tokens: Vec<&Token> = sentence.tokens[run]
                    .iter()
                    .filter(|token| !token.is_whitespace)
                    .collect();
                for chain in chains(&tokens) {
                    match chain.class {
                        ChainClass::Past | ChainClass::PastPerfect => past += 1,
                        ChainClass::Present => present += 1,
                        ChainClass::Modal { .. } | ChainClass::Ambiguous => {}
                    }
                }
            }
        }
    }
    (past, present)
}

/// The scene's narrative tense with the default thresholds, or `None` when the narration has too
/// few finite chains or no clear majority.
#[doc(hidden)]
pub fn scene_tense(document: &Document, scene: &Scene) -> Option<NarrativeTense> {
    scene_tense_with(document, scene, &SceneTenseThresholds::default())
}

#[doc(hidden)]
pub fn scene_tense_with(
    document: &Document,
    scene: &Scene,
    thresholds: &SceneTenseThresholds,
) -> Option<NarrativeTense> {
    let (past, present) = scene_tense_counts(document, scene);
    let total = past + present;
    if total < thresholds.minimum_chains.max(1) {
        return None;
    }
    let share = |count: usize| count as f64 / total as f64;
    if share(past) >= thresholds.minimum_share {
        Some(NarrativeTense::Past)
    } else if share(present) >= thresholds.minimum_share {
        Some(NarrativeTense::Present)
    } else {
        None
    }
}

/// Words that introduce displayed or written text before a colon: *The note read:*, *She wrote:*,
/// *the sign said:*. English-specific, like the rest of these helpers.
const DISPLAY_WORDS: [&str; 44] = [
    "read",
    "reads",
    "ran",
    "runs",
    "said",
    "says",
    "wrote",
    "writes",
    "written",
    "write",
    "printed",
    "scrawled",
    "scribbled",
    "typed",
    "inscribed",
    "engraved",
    "stamped",
    "painted",
    "lettered",
    "carved",
    "follows",
    "note",
    "letter",
    "message",
    "sign",
    "notice",
    "inscription",
    "label",
    "caption",
    "headline",
    "telegram",
    "card",
    "words",
    "text",
    "entry",
    "postcard",
    "poster",
    "banner",
    "plaque",
    "memo",
    "diary",
    "journal",
    "wire",
    "cable",
];

/// The index of a colon that follows a writing or display word within four words (*The note
/// read: I will be back by noon.*), in a run of non-whitespace tokens. Text after it, to the end
/// of the paragraph, is an unquoted embedded document rather than narration.
#[doc(hidden)]
pub fn display_colon_lead_in(tokens: &[&Token]) -> Option<usize> {
    tokens.iter().enumerate().find_map(|(index, token)| {
        if token.text != ":" {
            return None;
        }
        let lead_in = tokens[..index]
            .iter()
            .rev()
            .filter(|token| token.is_word)
            .take(4)
            .any(|token| DISPLAY_WORDS.contains(&normalized(token).as_str()));
        lead_in.then_some(index)
    })
}
