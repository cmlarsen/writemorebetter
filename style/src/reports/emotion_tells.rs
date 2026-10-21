use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::reports::echoes::fold;
use crate::reports::Split;
use crate::{Document, Mode, Span, StyleError, Token};

pub(crate) const REPORT: &str = "emotionTells";

const LEXICON: &str = include_str!("../../data/emotions/emotions.tsv");

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Kinds {
    noun: bool,
    adjective: bool,
    verb: bool,
}

fn lexicon() -> &'static HashMap<&'static str, Kinds> {
    static LEXICON_MAP: OnceLock<HashMap<&'static str, Kinds>> = OnceLock::new();
    LEXICON_MAP.get_or_init(|| {
        LEXICON
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let (word, kinds) = line.split_once('\t').expect("emotion TSV");
                let kinds = Kinds {
                    noun: kinds.contains('n'),
                    adjective: kinds.contains('a'),
                    verb: kinds.contains('v'),
                };
                (word, kinds)
            })
            .collect()
    })
}

/// Verbs that link a subject to a named feeling: *was afraid*, *felt angry*, *grew anxious*.
const LINKING: [&str; 11] = [
    "be", "feel", "become", "get", "grow", "seem", "look", "sound", "appear", "remain", "stay",
];
/// Verbs that put a feeling on their object: *made her angry*, *left him shaken*.
const CAUSATIVE: [&str; 4] = ["make", "leave", "drive", "keep"];
/// Verbs whose object is the feeling itself: *felt fear*, *sensed a flicker of panic*.
const EXPERIENCING: [&str; 3] = ["feel", "sense", "experience"];
/// Participles and adjectives that take the feeling after *with* or *by*: *filled with dread*.
const FILLED: [&str; 18] = [
    "fill",
    "overcome",
    "overwhelm",
    "consume",
    "seize",
    "grip",
    "strike",
    "rack",
    "wrack",
    "flood",
    "choke",
    "swamp",
    "engulf",
    "possess",
    "paralyze",
    "paralyse",
    "full",
    "sick",
];
/// Measure nouns before *of*: *a surge of relief*, *a pang of guilt*.
const MEASURES: [&str; 32] = [
    "surge", "wave", "pang", "flash", "flicker", "rush", "stab", "jolt", "twinge", "spark",
    "flood", "tide", "burst", "prickle", "sense", "feeling", "swell", "knot", "spasm", "ripple",
    "hint", "touch", "trace", "glimmer", "spike", "bolt", "tremor", "shiver", "shudder", "chill",
    "frisson", "sting",
];
/// Verbs that move a feeling through a body when the feeling is their subject: *fear washed over her*.
const MOVING: [&str; 48] = [
    "wash",
    "surge",
    "flood",
    "rise",
    "grip",
    "seize",
    "fill",
    "course",
    "sweep",
    "flare",
    "bloom",
    "swell",
    "well",
    "spread",
    "overwhelm",
    "overcome",
    "consume",
    "crash",
    "flicker",
    "flash",
    "twist",
    "coil",
    "tighten",
    "stir",
    "burn",
    "bubble",
    "build",
    "curl",
    "creep",
    "settle",
    "rush",
    "roll",
    "pour",
    "ripple",
    "shoot",
    "stab",
    "clutch",
    "claw",
    "gnaw",
    "prickle",
    "knot",
    "churn",
    "blossom",
    "spike",
    "burst",
    "lance",
    "pulse",
    "radiate",
];
const PERSONS: [&str; 15] = [
    "me",
    "him",
    "her",
    "us",
    "them",
    "my",
    "his",
    "our",
    "their",
    "your",
    "you",
    "myself",
    "himself",
    "herself",
    "themselves",
];
const EXPLETIVE: [&str; 7] = ["it", "this", "that", "there", "what", "which", "everything"];
const FIRST_PERSON_PRESENT: [&str; 6] = ["am", "'m", "are", "'re", "is", "'s"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct EmotionTellsOptions {
    #[serde(alias = "include_dialogue")]
    pub include_dialogue: bool,
}
impl EmotionTellsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_include_dialogue(mut self, value: bool) -> Self {
        self.include_dialogue = value;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmotionTellPattern {
    /// The feeling is a predicate: *she was afraid*, *it made him angry*, *she was in love*.
    Predicate,
    /// The feeling is the object of a perception verb: *he felt a surge of relief*, *filled with dread*.
    Object,
    /// The feeling is the subject of a verb moving through a body: *fear washed over her*.
    Subject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EmotionTell {
    /// From the verb to the feeling, or from the feeling to the verb for subject tells.
    pub span: Span,
    pub emotion: Span,
    pub verb: Span,
    /// Lexicon entry: the emotion word's lemma, or the verb whose participle names the feeling.
    pub word: String,
    pub pattern: EmotionTellPattern,
    pub sentence: usize,
    pub mode: Mode,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EmotionTellsGroup {
    pub tells: usize,
    pub words: usize,
    pub per_thousand_words: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EmotionTellsReport {
    pub tells: Vec<EmotionTell>,
    pub groups: Split<EmotionTellsGroup>,
}

/// Quoted text counts as dialogue here even without speech evidence: an untagged quotation's
/// feelings are a character's words, not the narrator's.
fn mode_at(document: &Document, byte: usize) -> Mode {
    let index = document
        .dialogue
        .partition_point(|span| span.byte.end <= byte);
    if document
        .dialogue
        .get(index)
        .is_some_and(|span| span.byte.start <= byte)
    {
        Mode::Dialogue
    } else {
        document.mode_at(byte)
    }
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token.readings.iter().any(|r| r.pos.starts_with(prefix))
}

fn has_lemma(token: &Token, lemmas: &[&str]) -> bool {
    lemmas.contains(&fold(&token.text).as_str())
        || token
            .readings
            .iter()
            .any(|r| r.pos.starts_with("VB") && lemmas.contains(&fold(&r.lemma).as_str()))
}

fn verb_lemma(token: &Token, lemmas: &[&str]) -> bool {
    // After *’d*, nlprule reads *felt* as the verb *to felt*: *She’d felt angry*.
    (lemmas.contains(&"feel") && fold(&token.text) == "felt")
        || token.readings.iter().any(|r| {
            (r.pos.starts_with("VB") || r.pos.starts_with("MD"))
                && lemmas.contains(&fold(&r.lemma).as_str())
        })
}

fn adverb(token: &Token) -> bool {
    let word = fold(&token.text);
    (has_pos(token, "RB") && !matches!(word.as_str(), "so" | "as" | "then" | "there" | "here"))
        || matches!(word.as_str(), "not" | "n't" | "so" | "very" | "too")
}

fn modifier(token: &Token) -> bool {
    has_pos(token, "JJ")
        || has_pos(token, "RB")
        || has_pos(token, "DT")
        || has_pos(token, "PRP$")
        || matches!(
            fold(&token.text).as_str(),
            "a" | "an" | "the" | "no" | "some" | "such"
        )
}

/// The emotion word a token names as an adjective: its surface, an adjective lemma, or the
/// participle of a lexicon verb (*terrified* from *terrify*).
fn adjective_entry(token: &Token) -> Option<String> {
    let surface = fold(&token.text);
    if lexicon().get(surface.as_str()).is_some_and(|k| k.adjective) {
        return Some(surface);
    }
    for reading in &token.readings {
        let lemma = fold(&reading.lemma);
        let kinds = lexicon().get(lemma.as_str()).copied().unwrap_or_default();
        if reading.pos.starts_with("JJ") && kinds.adjective {
            return Some(lemma);
        }
        let participle = reading.pos == "VBN"
            || ((reading.pos == "VBD" || reading.pos.starts_with("JJ"))
                && surface != lemma
                && (surface.ends_with("ed") || surface.ends_with("en") || surface.ends_with('t')));
        if participle && kinds.verb {
            return Some(lemma);
        }
    }
    None
}

fn noun_entry(token: &Token) -> Option<String> {
    let surface = fold(&token.text);
    let noun =
        |word: &str| !MEASURES.contains(&word) && lexicon().get(word).is_some_and(|k| k.noun);
    if token.readings.is_empty() {
        return noun(&surface).then_some(surface);
    }
    if token.readings.iter().all(|r| r.pos.starts_with("NNP")) {
        return None;
    }
    token
        .readings
        .iter()
        .filter(|r| r.pos == "NN" || r.pos == "NNS")
        .map(|r| fold(&r.lemma))
        .chain(std::iter::once(surface.clone()).filter(|_| has_pos(token, "NN")))
        .find(|lemma| noun(lemma))
}

struct Words<'a> {
    tokens: Vec<&'a Token>,
}

impl<'a> Words<'a> {
    fn word(&self, at: usize) -> String {
        fold(&self.tokens[at].text)
    }

    /// Walks back from `at` over tokens accepted by `skip`, at most `limit`, returning the index
    /// of the first token not skipped.
    fn back(&self, at: usize, limit: usize, skip: impl Fn(&Token) -> bool) -> Option<usize> {
        let mut index = at;
        for _ in 0..=limit {
            index = index.checked_sub(1)?;
            if !skip(self.tokens[index]) {
                return Some(index);
            }
        }
        None
    }

    /// A dummy subject (*It was sad*), but not a relative pronoun after a noun (*the man that was
    /// afraid*).
    fn expletive(&self, at: usize) -> bool {
        let word = self.word(at);
        EXPLETIVE.contains(&word.as_str())
            && !(matches!(word.as_str(), "that" | "which")
                && at > 0
                && has_pos(self.tokens[at - 1], "NN"))
    }

    /// The subject of a verb at `at`, skipping auxiliaries, adverbs and negation.
    fn subject(&self, at: usize) -> Option<usize> {
        self.back(at, 4, |token| {
            adverb(token) || has_pos(token, "MD") || verb_lemma(token, &["be", "have", "do"])
        })
    }

    fn idiom(&self, entry: &str, verb: usize, emotion: usize) -> bool {
        let next = self.tokens.get(emotion + 1).map(|token| fold(&token.text));
        let next = next.as_deref();
        let present_first_person = || {
            FIRST_PERSON_PRESENT.contains(&self.word(verb).as_str())
                && self
                    .subject(verb)
                    .is_some_and(|subject| matches!(self.word(subject).as_str(), "i" | "we"))
        };
        // "his mother’s troubled forehead", "being shy and retiring persons": attributive. A noun
        // followed by a verb opens a that-less clause instead: "afraid things would go wrong".
        if self.tokens.get(emotion + 1).is_some_and(|token| {
            !token.readings.is_empty()
                && token
                    .readings
                    .iter()
                    .all(|r| r.pos.starts_with("NN") && r.pos != "NNP")
        }) && !self
            .tokens
            .get(emotion + 2)
            .is_some_and(|token| has_pos(token, "VB") || has_pos(token, "MD"))
        {
            return true;
        }
        // "she was pleased to call it", "he was happy to say": formulas, not feelings.
        if next == Some("to")
            && self.tokens.get(emotion + 2).is_some_and(|token| {
                matches!(
                    fold(&token.text).as_str(),
                    "say" | "call" | "term" | "report" | "inform" | "announce" | "approve"
                )
            })
        {
            return true;
        }
        match entry {
            // "content to wait", "content with less": willing or settling, not a feeling.
            "content" => matches!(next, Some("to" | "with")),
            // "I'm afraid so", "I'm afraid I can't", "had more, I am afraid, than": a hedge, not fear.
            // "I am afraid." is fear.
            "afraid" => {
                matches!(next, Some("so" | "not"))
                    || (present_first_person()
                        && matches!(
                            next,
                            Some(
                                "i" | "we"
                                    | "you"
                                    | "he"
                                    | "she"
                                    | "it"
                                    | "they"
                                    | "that"
                                    | "there"
                                    | "the"
                                    | ","
                            )
                        ))
            }
            // "I'm sorry": an apology.
            "sorry" => present_first_person() && next != Some("for"),
            // "as far as she was concerned", "where they were concerned": involved, not worried.
            "concerned" => !matches!(next, Some("about" | "for" | "that" | "at")),
            // "he was badly hurt in the crash": injured. "his pride was hurt" is a feeling.
            "hurt" => {
                next == Some("in")
                    || (verb + 1..emotion).any(|at| {
                        matches!(
                            self.word(at).as_str(),
                            "badly"
                                | "seriously"
                                | "slightly"
                                | "severely"
                                | "critically"
                                | "fatally"
                                | "mortally"
                                | "gravely"
                        )
                    })
            }
            // "the jury found him guilty", "he was guilty of theft": culpable, not ashamed.
            "guilty" => !verb_lemma(self.tokens[verb], &["feel", "look", "seem", "sound"]),
            _ => {
                // "I'd be glad to help": an offer.
                next == Some("to")
                    && (0..verb)
                        .rev()
                        .take(3)
                        .any(|at| matches!(self.word(at).as_str(), "would" | "'d" | "will" | "'ll"))
            }
        }
    }

    fn predicate(&self, at: usize) -> Option<(usize, String)> {
        let entry = adjective_entry(self.tokens[at])?;
        // Coordinated predicates count too: *tired and afraid*.
        let coordinated = |token: &Token| {
            adverb(token)
                || (has_pos(token, "JJ") && !has_pos(token, "DT"))
                || matches!(token.text.as_str(), "and" | "or" | ",")
        };
        let mut before = self.back(at, 6, coordinated)?;
        // *a bit*, or *a little* read as a noun, stops the walk on *bit* / *little*; *a little* read as
        // an adjective is skipped and stops it on *a*.
        if self.word(before) == "a"
            && before + 1 < at
            && matches!(self.word(before + 1).as_str(), "little" | "bit")
        {
            before = self.back(before, 4, adverb)?;
        } else if matches!(self.word(before).as_str(), "little" | "bit")
            && before > 0
            && self.word(before - 1) == "a"
        {
            before = self.back(before - 1, 4, adverb)?;
        }
        let verb = if verb_lemma(self.tokens[before], &LINKING) {
            let subject = self.subject(before);
            if subject.is_some_and(|s| self.expletive(s)) {
                return None;
            }
            before
        } else if has_pos(self.tokens[before], "PRP") || has_pos(self.tokens[before], "NNP") {
            let verb = self.back(before, 2, adverb)?;
            verb_lemma(self.tokens[verb], &CAUSATIVE).then_some(verb)?
        } else {
            return None;
        };
        (!self.idiom(&entry, verb, at)).then_some((verb, entry))
    }

    fn in_state(&self, at: usize) -> Option<(usize, String)> {
        let entry = noun_entry(self.tokens[at])?;
        let preposition = self.back(at, 3, modifier)?;
        if self.word(preposition) != "in" {
            return None;
        }
        let verb = self.back(preposition, 3, adverb)?;
        (verb_lemma(self.tokens[verb], &["be", "fall"])
            && !self.subject(verb).is_some_and(|s| self.expletive(s)))
        .then_some((verb, entry))
    }

    /// The token before a noun phrase ending at `at`, past modifiers and an optional measure
    /// (*a sudden surge of*), if the phrase does not open the sentence, and the phrase's first token.
    fn phrase_start(&self, at: usize) -> (Option<usize>, usize) {
        match self.back(at, 4, modifier) {
            Some(of)
                if self.word(of) == "of"
                    && of > 0
                    && MEASURES.contains(&self.word(of - 1).as_str()) =>
            {
                let outer = self.back(of - 1, 4, modifier);
                (outer, outer.map_or(0, |outer| outer + 1))
            }
            Some(before) => (Some(before), before + 1),
            None => (None, 0),
        }
    }

    fn object(&self, at: usize) -> Option<(usize, String)> {
        let entry = noun_entry(self.tokens[at])?;
        let before = self.phrase_start(at).0?;
        let token = self.tokens[before];
        if verb_lemma(token, &EXPERIENCING) {
            return Some((before, entry));
        }
        if matches!(self.word(before).as_str(), "with" | "by") && before > 0 {
            let filler = self.back(before, 2, adverb)?;
            if has_lemma(self.tokens[filler], &FILLED) {
                return Some((filler, entry));
            }
        }
        None
    }

    fn subject_tell(&self, at: usize) -> Option<(usize, usize, String)> {
        let entry = noun_entry(self.tokens[at])?;
        let (outer, first) = self.phrase_start(at);
        // The phrase must open a clause: *Fear washed over her*, *and then panic rose*.
        if outer.is_some_and(|outer| {
            self.tokens[outer].is_word
                && !matches!(
                    self.word(outer).as_str(),
                    "and" | "then" | "but" | "when" | "as" | "until"
                )
        }) {
            return None;
        }
        let mut verb = at + 1;
        while self.tokens.get(verb).is_some_and(|t| adverb(t)) {
            verb += 1;
        }
        let token = self.tokens.get(verb)?;
        if !(has_pos(token, "VB") && verb_lemma(token, &MOVING)) {
            return None;
        }
        let person = (verb + 1..self.tokens.len().min(verb + 6))
            .take_while(|&index| !self.tokens[index].is_punctuation)
            .any(|index| PERSONS.contains(&self.word(index).as_str()));
        person.then_some((first, verb, entry))
    }
}

pub fn report(
    document: &Document,
    options: &EmotionTellsOptions,
) -> Result<EmotionTellsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let included = |mode: Mode| mode == Mode::Narration || options.include_dialogue;
    let mut tells = Vec::new();
    let mut groups: Split<EmotionTellsGroup> = Split::default();
    for token in document.words() {
        let mode = mode_at(document, token.span.byte.start);
        if included(mode) {
            for group in groups.all_and_mode_mut(mode) {
                group.words += 1;
            }
        }
    }
    for (index, sentence) in document.sentences().enumerate() {
        let words = Words {
            tokens: sentence
                .tokens
                .iter()
                .filter(|token| !token.is_whitespace)
                .collect(),
        };
        for at in 0..words.tokens.len() {
            let emotion = words.tokens[at];
            if !emotion.is_word || emotion.is_skipped {
                continue;
            }
            let mode = mode_at(document, emotion.span.byte.start);
            if !included(mode) {
                continue;
            }
            let found =
                if let Some((verb, word)) = words.predicate(at).or_else(|| words.in_state(at)) {
                    Some((verb, at, verb, word, EmotionTellPattern::Predicate))
                } else if let Some((verb, word)) = words.object(at) {
                    Some((verb, at, verb, word, EmotionTellPattern::Object))
                } else {
                    words.subject_tell(at).map(|(first, verb, word)| {
                        (first, verb, verb, word, EmotionTellPattern::Subject)
                    })
                };
            let Some((first, last, verb, word, pattern)) = found else {
                continue;
            };
            if mode_at(document, words.tokens[verb].span.byte.start) != mode {
                continue;
            }
            // Coordinated feelings on one verb are one tell: *was angry and afraid*.
            if let Some(previous) = tells.last_mut().filter(|previous: &&mut EmotionTell| {
                pattern == EmotionTellPattern::Predicate
                    && previous.pattern == pattern
                    && previous.verb == words.tokens[verb].span
            }) {
                previous.span = Span::covering(&previous.span, &emotion.span);
                continue;
            }
            tells.push(EmotionTell {
                span: Span::covering(&words.tokens[first].span, &words.tokens[last].span),
                emotion: emotion.span.clone(),
                verb: words.tokens[verb].span.clone(),
                word,
                pattern,
                sentence: index,
                mode,
            });
            for group in groups.all_and_mode_mut(mode) {
                group.tells += 1;
            }
        }
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        group.per_thousand_words =
            (group.words > 0).then(|| 1000.0 * group.tells as f64 / group.words as f64);
    }
    Ok(EmotionTellsReport { tells, groups })
}
