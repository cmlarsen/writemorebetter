//! The voice layer (#353): caller-marked spans, quoted thought, and tag-anchored unquoted thought
//! and speech, resolved into `Document::voices`. See docs/STYLE-VOICES.md.
//!
//! Detection is precision-first. A tag (*she thought*, *he said*) must be attached by position,
//! its subject must be a person, its verb must be in a small list, and the clause it tags must
//! disagree with it in person or tense. Free indirect discourse and indirect thought stay
//! narration.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ops::Range;
use std::sync::OnceLock;

use crate::narrative::{chains, imperative_start, pronoun_person, ChainClass, NarrativePerson};
use crate::reports::dialogue_tags::{has_pos, is_determiner, lower};
use crate::{
    Document, DocumentOptions, MarkedVoice, OffsetUnits, Paragraph, Span, Token, Voice, VoiceShift,
    VoiceSource, VoiceSpan,
};

const THOUGHT_VERBS: &str = include_str!("../data/thought/thought-verbs.txt");
const REFLEXIVE_VERBS: &str = include_str!("../data/thought/reflexive-verbs.txt");

/// At most this many neighbouring sentences join an anchored thought.
const MAX_EXPANSION: usize = 2;
/// At most this many extension tokens (*again*, *bitterly*, *for a moment*) follow a tag's verb.
const MAX_EXTENSION: usize = 4;
/// At most this many tokens in a proper-name subject (*Anne Shirley*).
const MAX_NAME: usize = 3;
/// At most this many modifiers between a determiner and a noun subject (*the old man*).
const MAX_MODIFIERS: usize = 2;

/// The words after a tag verb that make it indirect: *thought that*, *wondered where*,
/// *thought it over*.
const INDIRECT: [&str; 20] = [
    "that", "if", "whether", "how", "what", "why", "where", "when", "who", "which", "about", "of",
    "to", "so", "it", "back", "over", "up", "through", "aloud",
];
const REFLEXIVES: [&str; 8] = [
    "myself",
    "yourself",
    "himself",
    "herself",
    "ourselves",
    "yourselves",
    "themselves",
    "oneself",
];
/// Verbs that invert with a noun subject: *said the old man*, *thought the doctor*.
const INVERTING: [&str; 14] = [
    "say", "ask", "reply", "answer", "cry", "think", "whisper", "exclaim", "shout", "add",
    "continue", "repeat", "observe", "remark",
];
const COORDINATORS: [&str; 7] = ["and", "but", "or", "nor", "so", "then", "yet"];

fn parse_list(data: &'static str) -> HashSet<&'static str> {
    data.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

pub(crate) fn thought_verbs() -> &'static HashSet<&'static str> {
    static LIST: OnceLock<HashSet<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| parse_list(THOUGHT_VERBS))
}

pub(crate) fn reflexive_verbs() -> &'static HashSet<&'static str> {
    static LIST: OnceLock<HashSet<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| parse_list(REFLEXIVE_VERBS))
}

/// Speech verbs that tag unquoted speech: dialogue tags' list less the thought verbs, which win.
fn unquoted_speech_verbs() -> &'static HashSet<&'static str> {
    static LIST: OnceLock<HashSet<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| {
        crate::reports::dialogue_tags::speech_verbs()
            // *call* names more often than it speaks: *the sailor, called the mincer*.
            .filter(|verb| {
                !verb.contains(' ') && !thought_verbs().contains(verb) && *verb != "call"
            })
            .collect()
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lexicon {
    Thought,
    Speech,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// After the clause: *…, she thought.*, and the bare *I wondered.* after a question.
    Trailing,
    /// Before the clause: *She thought, …*
    Leading,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gender {
    Masculine,
    Feminine,
    /// *they*: a present *they say* / *they think* is a hedge and never anchors.
    Plural,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tense {
    Past,
    Present,
}

struct Tag {
    /// Item range of the tag, extension included.
    range: Range<usize>,
    person: NarrativePerson,
    tense: Tense,
    gender: Gender,
}

#[derive(Clone, Copy)]
struct Item<'a> {
    token: &'a Token,
    sentence: usize,
    blocked: bool,
}

struct Marked {
    index: usize,
    span: Span,
    voice: MarkedVoice,
}

struct Detected {
    span: Span,
    voice: Voice,
    tag: Span,
    shift: VoiceShift,
}

pub(crate) struct Detector {
    thought: bool,
    speech: bool,
    marked: Vec<Marked>,
    ignored: Vec<usize>,
    detected: Vec<Detected>,
    quoted_thought: BTreeSet<usize>,
}

/// Every character start as (byte, char, utf16), plus the end of the text.
fn offsets(text: &str) -> Vec<(usize, usize, usize)> {
    let mut offsets = Vec::with_capacity(text.len() + 1);
    let mut utf16 = 0;
    for (scalar, (byte, ch)) in text.char_indices().enumerate() {
        offsets.push((byte, scalar, utf16));
        utf16 += ch.len_utf16();
    }
    offsets.push((text.len(), offsets.len(), utf16));
    offsets
}

fn marked_spans(text: &str, options: &DocumentOptions) -> (Vec<Marked>, Vec<usize>) {
    if options.marked_spans.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let units = options.marked_span_units.unwrap_or(OffsetUnits::Byte);
    let table = offsets(text);
    let find = |offset: usize| -> Option<(usize, usize, usize)> {
        let key = |entry: &(usize, usize, usize)| match units {
            OffsetUnits::Byte => entry.0,
            OffsetUnits::Char => entry.1,
            OffsetUnits::Utf16 => entry.2,
        };
        // A missing entry is an offset past the end, inside a multibyte character (bytes) or
        // between the halves of a surrogate pair (UTF-16).
        table
            .binary_search_by_key(&offset, key)
            .ok()
            .map(|index| table[index])
    };
    let mut marked = Vec::new();
    let mut ignored = Vec::new();
    for (index, span) in options.marked_spans.iter().enumerate() {
        let found = (span.start < span.end)
            .then(|| find(span.start).zip(find(span.end)))
            .flatten();
        match found {
            Some((start, end)) => marked.push(Marked {
                index,
                span: Span {
                    byte: start.0..end.0,
                    char: start.1..end.1,
                    utf16: start.2..end.2,
                },
                voice: span.voice,
            }),
            None => ignored.push(index),
        }
    }
    (marked, ignored)
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn word(item: &Item) -> String {
    lower(item.token).replace('’', "'")
}

fn is_word(item: &Item) -> bool {
    item.token.is_word
}

fn terminal(item: &Item) -> bool {
    !item.token.text.is_empty()
        && item
            .token
            .text
            .chars()
            .all(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

fn linking(item: &Item) -> bool {
    matches!(item.token.text.as_str(), "," | ";" | ":" | "—" | "–")
}

fn adverb(item: &Item) -> bool {
    let token = item.token;
    token.is_word
        && has_pos(token, "RB")
        && !has_pos(token, "VB")
        && !matches!(word(item).as_str(), "not" | "n't" | "never")
}

/// A past, participle or gerund reading: a second predicate of the tag's subject, not new content.
fn predicate_opening(token: &Token) -> bool {
    token
        .readings
        .iter()
        .any(|reading| matches!(reading.pos.as_str(), "VBD" | "VBN" | "VBG"))
}

fn capitalized_word(token: &Token) -> bool {
    token.text.chars().next().is_some_and(char::is_uppercase)
}

fn capitalized(item: &Item) -> bool {
    item.token
        .text
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
}

impl Detector {
    pub(crate) fn new(text: &str, options: &DocumentOptions) -> Self {
        let (marked, ignored) = marked_spans(text, options);
        Self {
            thought: options.detect_thought,
            speech: options.detect_unquoted_speech,
            marked,
            ignored,
            detected: Vec::new(),
            quoted_thought: BTreeSet::new(),
        }
    }

    pub(crate) fn paragraph(
        &mut self,
        tokenizer: &nlprule::Tokenizer,
        paragraph: &Paragraph,
        dialogue: &[Span],
    ) {
        if !self.thought && !self.speech {
            return;
        }
        let range = &paragraph.span.byte;
        let first = dialogue.partition_point(|span| span.byte.end <= range.start);
        let quotes: Vec<(usize, &Span)> = dialogue
            .iter()
            .enumerate()
            .skip(first)
            .take_while(|(_, span)| span.byte.start < range.end)
            .collect();
        let marked: Vec<Range<usize>> = self
            .marked
            .iter()
            .map(|marked| marked.span.byte.clone())
            .filter(|span| overlaps(span, range))
            .collect();
        let items: Vec<Item> = paragraph
            .sentences
            .iter()
            .enumerate()
            .flat_map(|(sentence, tokens)| {
                tokens
                    .tokens
                    .iter()
                    .filter(|token| !token.is_whitespace)
                    .map(move |token| (sentence, token))
            })
            .map(|(sentence, token)| Item {
                token,
                sentence,
                blocked: quotes
                    .iter()
                    .any(|(_, span)| overlaps(&span.byte, &token.span.byte))
                    || marked.iter().any(|span| overlaps(span, &token.span.byte)),
            })
            .collect();
        if items.is_empty() {
            return;
        }
        let parser = Parser {
            items: &items,
            tokenizer,
        };
        if self.thought {
            for (index, span) in &quotes {
                let after =
                    items.partition_point(|item| item.token.span.byte.start < span.byte.end);
                if after < items.len()
                    && span.byte.end <= range.end
                    && parser
                        .tag(after, Lexicon::Thought, Position::Trailing)
                        .is_some_and(|tag| parser.tag_ends(&tag, Position::Trailing))
                {
                    self.quoted_thought.insert(*index);
                }
            }
        }
        let mut found = Vec::new();
        if self.thought {
            parser.anchors(Lexicon::Thought, &mut found);
        }
        if self.speech {
            parser.anchors(Lexicon::Speech, &mut found);
        }
        self.detected.extend(found);
    }

    pub(crate) fn finish(mut self, document: &mut Document) {
        self.detected
            .sort_by_key(|detected| detected.span.byte.start);
        document.voices = resolve(document, self.marked, self.detected, &self.quoted_thought);
        document.ignored_marked_spans = self.ignored;
    }
}

struct Parser<'a, 'b> {
    items: &'b [Item<'a>],
    tokenizer: &'b nlprule::Tokenizer,
}

impl<'a> Parser<'a, '_> {
    fn sentence(&self, sentence: usize) -> Range<usize> {
        let start = self.items.partition_point(|item| item.sentence < sentence);
        let end = self.items.partition_point(|item| item.sentence <= sentence);
        start..end
    }

    fn tokens(&self, range: &Range<usize>) -> Vec<&'a Token> {
        self.items[range.clone()]
            .iter()
            .map(|item| item.token)
            .collect()
    }

    fn blocked(&self, range: &Range<usize>) -> bool {
        self.items[range.clone()].iter().any(|item| item.blocked)
    }

    /// Subject candidates starting at `at`: (end, person, gender).
    fn subjects(&self, at: usize, position: Position) -> Vec<(usize, NarrativePerson, Gender)> {
        let items = self.items;
        let Some(first) = items.get(at).filter(|item| is_word(item) && !item.blocked) else {
            return Vec::new();
        };
        match word(first).as_str() {
            "i" | "we" => {
                let window: Vec<&Token> = items[at.saturating_sub(1)..=at]
                    .iter()
                    .map(|item| item.token)
                    .collect();
                if word(first) == "i"
                    && pronoun_person(&window, window.len() - 1).map(|(person, _)| person)
                        != Some(NarrativePerson::First)
                {
                    return Vec::new();
                }
                return vec![(at + 1, NarrativePerson::First, Gender::Unknown)];
            }
            "you" => return vec![(at + 1, NarrativePerson::Second, Gender::Unknown)],
            "he" => return vec![(at + 1, NarrativePerson::Third, Gender::Masculine)],
            "she" => return vec![(at + 1, NarrativePerson::Third, Gender::Feminine)],
            "they" => return vec![(at + 1, NarrativePerson::Third, Gender::Plural)],
            "it" | "one" => return Vec::new(),
            _ => {}
        }
        let mut found = Vec::new();
        if first.token.is_skipped {
            let end = (at..items.len())
                .find(|&index| !items[index].token.is_skipped || items[index].blocked)
                .unwrap_or(items.len());
            found.push((end, NarrativePerson::Third, Gender::Unknown));
            return found;
        }
        let name = |item: &Item| {
            let token = item.token;
            is_word(item)
                && !item.blocked
                && capitalized(item)
                && !has_pos(token, "PRP")
                && !is_determiner(token)
                && !has_pos(token, "RB")
                && !has_pos(token, "IN")
                && !has_pos(token, "CC")
                && !has_pos(token, "DT")
                && (has_pos(token, "NNP")
                    || token.readings.is_empty()
                    || (position == Position::Trailing && has_pos(token, "NN")))
        };
        let mut index = at;
        if crate::document::honorific(&first.token.text) {
            index += 1;
            if items.get(index).is_some_and(|item| item.token.text == ".") {
                index += 1;
            }
        }
        // A capitalized title noun before a name: *Aunt Hester*, *Captain Ahab*.
        let titled = |at: usize| {
            items.get(at).is_some_and(|item| {
                is_word(item) && !item.blocked && capitalized(item) && has_pos(item.token, "NN")
            }) && items.get(at + 1).is_some_and(name)
        };
        let mut length = 0;
        while length < MAX_NAME
            && (items.get(index + length).is_some_and(name) || titled(index + length))
        {
            length += 1;
            found.push((index + length, NarrativePerson::Third, Gender::Unknown));
        }
        if is_determiner(first.token) && !has_pos(first.token, "PRP") {
            for modifiers in 0..=MAX_MODIFIERS {
                let head = at + 1 + modifiers;
                let Some(noun) = items.get(head) else { break };
                let middle_ok = items[at + 1..head].iter().all(|item| {
                    is_word(item)
                        && !item.blocked
                        && (has_pos(item.token, "JJ") || has_pos(item.token, "NN"))
                });
                if !middle_ok {
                    break;
                }
                if is_word(noun) && !noun.blocked && has_pos(noun.token, "NN") {
                    found.push((head + 1, NarrativePerson::Third, Gender::Unknown));
                }
            }
        }
        found
    }

    /// The verb reading of a tag verb whose lemma is in the lexicon.
    fn verb(&self, at: usize, lexicon: Lexicon, position: Position) -> Option<(Tense, bool)> {
        let item = self.items.get(at)?;
        if !is_word(item) || item.blocked || item.token.is_skipped {
            return None;
        }
        let mut found = None;
        for reading in &item.token.readings {
            let tense = match reading.pos.as_str() {
                "VBD" => Tense::Past,
                "VBZ" | "VBP" => Tense::Present,
                _ => continue,
            };
            let lemma = reading.lemma.to_lowercase();
            let listed = match lexicon {
                Lexicon::Thought if thought_verbs().contains(lemma.as_str()) => {
                    // *reflect* tags thought only after its clause.
                    (lemma != "reflect" || position == Position::Trailing).then_some(false)
                }
                Lexicon::Thought => reflexive_verbs().contains(lemma.as_str()).then_some(true),
                Lexicon::Speech => unquoted_speech_verbs()
                    .contains(lemma.as_str())
                    .then_some(false),
            };
            if let Some(reflexive) = listed {
                // Prefer the past reading of *thought*/*read*-like forms.
                if found.is_none() || tense == Tense::Past {
                    found = Some((tense, reflexive));
                }
            }
        }
        found
    }

    fn tag(&self, at: usize, lexicon: Lexicon, position: Position) -> Option<Tag> {
        let items = self.items;
        let mut parsed = None;
        // SUBJECT [ADVERB] VERB.
        for (subject_end, person, gender) in self.subjects(at, position) {
            let mut verb = subject_end;
            if items.get(verb).is_some_and(|item| adverb(item)) {
                verb += 1;
            }
            if let Some((tense, reflexive)) = self.verb(verb, lexicon, position) {
                parsed = Some((verb, tense, reflexive, person, gender));
                break;
            }
        }
        // VERB SUBJECT: *thought Anne*, after the clause only.
        if parsed.is_none() && position == Position::Trailing {
            if let Some((Tense::Past, reflexive)) = self.verb(at, lexicon, position) {
                // The longest name: *thought Lily Briscoe*, not *thought Lily*. A noun subject
                // inverts only after a reporting verb (*said the old man*); after another verb it
                // is an object (*finished the paper*).
                let noun_subject_ok = items[at].token.readings.iter().any(|reading| {
                    reading.pos == "VBD"
                        && INVERTING.contains(&reading.lemma.to_lowercase().as_str())
                });
                if let Some(&(end, person, gender)) = self
                    .subjects(at + 1, position)
                    .iter()
                    .filter(|(_, person, _)| *person == NarrativePerson::Third)
                    .filter(|_| noun_subject_ok || !is_determiner(items[at + 1].token))
                    .max_by_key(|(end, _, _)| *end)
                {
                    let tag = Tag {
                        range: at..end,
                        person,
                        tense: Tense::Past,
                        gender,
                    };
                    return (!reflexive).then(|| self.extend(tag)).flatten();
                }
            }
        }
        let (verb, tense, reflexive, person, gender) = parsed?;
        let mut end = verb + 1;
        // *told herself*, *muttered to himself*, *thought to herself*.
        let to = items.get(end).is_some_and(|item| word(item) == "to");
        let reflexive_at = end + usize::from(to);
        let has_reflexive = items
            .get(reflexive_at)
            .is_some_and(|item| REFLEXIVES.contains(&word(item).as_str()));
        if has_reflexive {
            end = reflexive_at + 1;
        } else if reflexive {
            return None;
        }
        self.extend(Tag {
            range: at..end,
            person,
            tense,
            gender,
        })
    }

    /// Applies the indirect guard and takes up to `MAX_EXTENSION` adverbial tokens.
    fn extend(&self, mut tag: Tag) -> Option<Tag> {
        let items = self.items;
        if items
            .get(tag.range.end)
            .is_some_and(|item| is_word(item) && INDIRECT.contains(&word(item).as_str()))
        {
            return None;
        }
        let mut taken = 0;
        while taken < MAX_EXTENSION {
            let at = tag.range.end;
            let phrase: Vec<String> = items[at..items.len().min(at + 3)]
                .iter()
                .map(word)
                .collect();
            if phrase.len() == 3 && phrase[0] == "for" && phrase[1] == "a" && phrase[2] == "moment"
            {
                tag.range.end += 3;
                taken += 3;
                continue;
            }
            match items.get(at) {
                Some(item)
                    if !item.blocked
                        && (adverb(item)
                            || matches!(word(item).as_str(), "again" | "suddenly" | "then"))
                        && !INDIRECT.contains(&word(item).as_str())
                        && word(item) != "out" =>
                {
                    tag.range.end += 1;
                    taken += 1;
                }
                _ => break,
            }
        }
        Some(tag)
    }

    /// Whether a tag ends where the position allows: at the end of its sentence, or at linking
    /// punctuation followed by more narration (trailing), or at , : — before its clause (leading).
    fn tag_ends(&self, tag: &Tag, position: Position) -> bool {
        let items = self.items;
        let sentence = items[tag.range.start].sentence;
        let bounds = self.sentence(sentence);
        if tag.range.end > bounds.end {
            return false;
        }
        let rest = tag.range.end..bounds.end;
        match position {
            Position::Trailing => {
                rest.is_empty()
                    || (terminal(&items[rest.start])
                        && items[rest.clone()].iter().all(|item| !is_word(item)))
                    || (matches!(items[rest.start].token.text.as_str(), "," | ";" | "—" | "–")
                        && items[rest.start + 1..rest.end]
                            .iter()
                            .any(|item| is_word(item) && !item.blocked))
            }
            Position::Leading => {
                !rest.is_empty()
                    && matches!(items[rest.start].token.text.as_str(), "," | ":" | "—")
                    && items[rest.start + 1..rest.end]
                        .iter()
                        .any(|item| is_word(item) && !item.blocked)
            }
        }
    }

    /// Step 5: the clause disagrees with its tag in person or tense.
    fn shift(&self, clause: &Range<usize>, tag: &Tag) -> Option<VoiceShift> {
        // *I think*, *I suppose*, *they say*: present hedges, not tags.
        let hedge = tag.tense == Tense::Present
            && (tag.person == NarrativePerson::First || tag.gender == Gender::Plural);
        if tag.person == NarrativePerson::Second || hedge {
            return None;
        }
        let tokens = self.tokens(clause);
        let found = chains(&tokens);
        // A clause that opens in the frame (*He grunted, or, rather, I might say*) is narration
        // with a narrator's aside, not the tagged character's words.
        if tag.person == NarrativePerson::Third
            && found.first().is_some_and(|chain| {
                matches!(chain.class, ChainClass::Past | ChainClass::PastPerfect)
                    && chain
                        .subject
                        .is_some_and(|subject| self.frame_pronoun(tokens[subject], tag))
            })
        {
            return None;
        }
        let person = tag.person == NarrativePerson::Third
            && (0..tokens.len()).any(|index| {
                pronoun_person(&tokens, index).map(|(person, _)| person)
                    == Some(NarrativePerson::First)
            });
        let tense = tag.tense == Tense::Past
            && (found.first().is_some_and(|chain| {
                matches!(
                    chain.class,
                    ChainClass::Present | ChainClass::Modal { present: true }
                )
            }) || self.imperative(clause));
        match (person, tense) {
            (true, true) => Some(VoiceShift::Both),
            (true, false) => Some(VoiceShift::Person),
            (false, true) => Some(VoiceShift::Tense),
            (false, false) => None,
        }
    }

    /// `imperative_start`, plus a capitalized verb the tagger read only as a name after a tag
    /// (*She thought, Stay calm.*): its lowercase form must have a bare-verb reading in the
    /// lexicon, and the chunker must have opened a verb phrase on it.
    fn imperative(&self, range: &Range<usize>) -> bool {
        let tokens = self.tokens(range);
        if imperative_start(&tokens) {
            return true;
        }
        let Some(offset) = tokens.iter().position(|token| token.is_word) else {
            return false;
        };
        let first = tokens[offset];
        // Only after a tag's comma or colon: a sentence-initial capital is a name (*Harry, he
        // reported, …*), and the tagger reads a sentence-initial verb as a verb anyway.
        let at = range.start + offset;
        let after_tag = at > 0
            && self.items[at - 1].sentence == self.items[at].sentence
            && matches!(self.items[at - 1].token.text.as_str(), "," | ":");
        let named = !first.readings.is_empty()
            && first
                .readings
                .iter()
                .all(|reading| reading.pos.starts_with("NNP"));
        named
            && after_tag
            && capitalized_word(first)
            && first.chunks.iter().any(|chunk| chunk == "B-VP")
            && self
                .tokenizer
                .tagger()
                .get_tags(&first.text.to_lowercase())
                .any(|data| data.pos().as_str() == "VB")
    }

    /// A third-person subject pronoun that can refer to the tag's subject.
    fn frame_pronoun(&self, token: &Token, tag: &Tag) -> bool {
        let word = lower(token);
        match tag.gender {
            Gender::Masculine => word == "he",
            Gender::Feminine => word == "she",
            Gender::Plural => word == "they",
            Gender::Unknown => matches!(word.as_str(), "he" | "she"),
        }
    }

    /// A neighbouring sentence or a second clause that carries the same shift without reverting
    /// to the frame: no past chain for a tense shift, no third-person subject of the tag's gender
    /// for a person shift.
    fn continues(&self, range: &Range<usize>, tag: &Tag, shift: VoiceShift) -> bool {
        if self.blocked(range) || !self.items[range.clone()].iter().any(is_word) {
            return false;
        }
        let Some(own) = self.shift(range, tag) else {
            return false;
        };
        let tokens = self.tokens(range);
        // The clause continues by a path the anchor took: for a tense shift, its own present
        // tense and no past chain; for a person shift, its own first person and no third-person
        // pronoun that could be the tag's subject. A thought that shifted both ways may continue
        // by either (*I have to get out, she thought, before he wakes*).
        let past = || {
            chains(&tokens)
                .iter()
                .any(|chain| matches!(chain.class, ChainClass::Past | ChainClass::PastPerfect))
        };
        let tense = matches!(shift, VoiceShift::Tense | VoiceShift::Both)
            && matches!(own, VoiceShift::Tense | VoiceShift::Both)
            && !past();
        let person = matches!(shift, VoiceShift::Person | VoiceShift::Both)
            && matches!(own, VoiceShift::Person | VoiceShift::Both)
            && !tokens.iter().any(|token| self.frame_pronoun(token, tag));
        tense || person
    }

    /// Thought evidence for expansion: ends in ? ! …, has a first-person pronoun, or is an order.
    fn evidence(&self, range: &Range<usize>) -> bool {
        let tokens = self.tokens(range);
        let ends = tokens
            .iter()
            .rev()
            .find(|token| !matches!(token.text.as_str(), "”" | "’" | "\"" | "'" | ")"))
            .is_some_and(|token| token.text.contains(['?', '!', '…']));
        ends || self.imperative(range)
            || (0..tokens.len()).any(|index| {
                pronoun_person(&tokens, index).map(|(person, _)| person)
                    == Some(NarrativePerson::First)
            })
    }

    /// The clause before a trailing tag at `separator`: back to the sentence start, or to the
    /// last quote, caller span, ; or :.
    fn clause_before(&self, sentence: &Range<usize>, separator: usize) -> usize {
        // Stop at a quote or caller span, at ; : — –, after an unclosed parenthesis, and at a
        // sentence end the splitter missed (*…thought I. But it is…*), but not after *Mrs.*
        let mut depth = 0usize;
        for index in (sentence.start..separator).rev() {
            let item = &self.items[index];
            let text = item.token.text.as_str();
            let honorific =
                index > 0 && crate::document::honorific(&self.items[index - 1].token.text);
            let stop = item.blocked
                || matches!(text, ";" | ":" | "—" | "–")
                || (text == "(" && depth == 0)
                || (matches!(text, "." | "!" | "?") && index + 1 < separator && !honorific);
            if stop {
                return index + 1;
            }
            match text {
                ")" => depth += 1,
                "(" => depth -= 1,
                _ => {}
            }
        }
        sentence.start
    }

    /// Moves a clause start past a ) that closes a parenthesis opened before it: the clause began
    /// inside a parenthesis (*(all, thought Mrs. Ramsay, because …) and denounced …*).
    fn after_closed(&self, start: usize, end: usize) -> usize {
        let mut start = start;
        let mut depth = 0usize;
        for index in start..end {
            match self.items[index].token.text.as_str() {
                "(" => depth += 1,
                ")" if depth == 0 => start = index + 1,
                ")" => depth -= 1,
                _ => {}
            }
        }
        start
    }

    fn trimmed(&self, range: Range<usize>) -> Option<Range<usize>> {
        let mut range = range;
        while range.start < range.end && linking(&self.items[range.start]) {
            range.start += 1;
        }
        while range.end > range.start && linking(&self.items[range.end - 1]) {
            range.end -= 1;
        }
        self.items[range.clone()]
            .iter()
            .any(is_word)
            .then_some(range)
    }

    fn span(&self, range: &Range<usize>) -> Span {
        Span::covering(
            &self.items[range.start].token.span,
            &self.items[range.end - 1].token.span,
        )
    }

    fn push(
        &self,
        found: &mut Vec<Detected>,
        clause: Range<usize>,
        tag: &Tag,
        shift: VoiceShift,
        lexicon: Lexicon,
    ) {
        let Some(clause) = self.trimmed(clause) else {
            return;
        };
        let span = self.span(&clause);
        let tag_span = self.span(&tag.range);
        if found.iter().any(|other| {
            overlaps(&other.span.byte, &span.byte)
                || overlaps(&other.tag.byte, &span.byte)
                || overlaps(&other.span.byte, &tag_span.byte)
        }) {
            return;
        }
        found.push(Detected {
            span,
            voice: match lexicon {
                Lexicon::Thought => Voice::Thought,
                Lexicon::Speech => Voice::Speech,
            },
            tag: tag_span,
            shift,
        });
    }

    /// Walks over up to `MAX_EXPANSION` neighbouring sentences in `direction` from `sentence`.
    fn expand(&self, sentence: usize, tag: &Tag, shift: VoiceShift, backward: bool) -> usize {
        let last = self.items.last().map_or(0, |item| item.sentence);
        let mut edge = sentence;
        for _ in 0..MAX_EXPANSION {
            let next = if backward {
                match edge.checked_sub(1) {
                    Some(next) => next,
                    None => break,
                }
            } else if edge < last {
                edge + 1
            } else {
                break;
            };
            let range = self.sentence(next);
            if range.is_empty() || !self.continues(&range, tag, shift) || !self.evidence(&range) {
                break;
            }
            edge = next;
        }
        edge
    }

    fn anchors(&self, lexicon: Lexicon, found: &mut Vec<Detected>) {
        let items = self.items;
        let last = items.last().map_or(0, |item| item.sentence);
        let thought = lexicon == Lexicon::Thought;
        for sentence in 0..=last {
            let bounds = self.sentence(sentence);
            if bounds.is_empty() {
                continue;
            }
            // B: a question, exclamation or trailing-off sentence, then a sentence that is only a
            // tag (*Where is he? I wondered.*).
            if thought && sentence > 0 {
                let previous = self.sentence(sentence - 1);
                if let Some(tag) =
                    self.tag(bounds.start, lexicon, Position::Trailing)
                        .filter(|tag| {
                            let rest = tag.range.end..bounds.end;
                            rest.is_empty()
                                || (terminal(&items[rest.start])
                                    && items[rest].iter().all(|item| !is_word(item)))
                        })
                {
                    let ends = items[previous.clone()]
                        .iter()
                        .rev()
                        .find(|item| !matches!(item.token.text.as_str(), "”" | "’" | "\"" | "'"))
                        .is_some_and(|item| item.token.text.contains(['?', '!', '…']));
                    if ends && !previous.is_empty() {
                        let start = self.clause_before(&previous, previous.end);
                        let clause = start..previous.end;
                        if let Some(shift) = self.shift(&clause, &tag) {
                            let first = if start == previous.start {
                                self.expand(sentence - 1, &tag, shift, true)
                            } else {
                                sentence - 1
                            };
                            let start = if first < sentence - 1 {
                                self.sentence(first).start
                            } else {
                                start
                            };
                            self.push(found, start..previous.end, &tag, shift, lexicon);
                        }
                    }
                    continue;
                }
            }
            // C: a sentence-initial tag before its clause (*She thought, I can’t do this.*).
            if let Some(tag) = self
                .tag(bounds.start, lexicon, Position::Leading)
                .filter(|tag| self.tag_ends(tag, Position::Leading))
            {
                let start = tag.range.end + 1;
                let end = (start..bounds.end)
                    .find(|&index| items[index].blocked || items[index].token.text == ";")
                    .unwrap_or(bounds.end);
                let clause = start..end;
                // *He promises fairly, and I hope…*, *The miner grunted, put his bag down*, *She
                // prays now, she says, that…*: a coordinator, a second predicate or a second tag
                // continues the narration instead of opening the tag's content.
                let opening = items[clause.clone()].iter().position(|item| is_word(item));
                // *He mentioned, it is true, “my dear friend”*: a clause cut short by a quote is a
                // parenthetical aside, not the tag's content.
                let aside = end < bounds.end && items[end].blocked;
                let continued = aside
                    || opening.is_some_and(|offset| {
                        let first = &items[start + offset];
                        COORDINATORS.contains(&word(first).as_str())
                            || predicate_opening(first.token)
                            || [Lexicon::Thought, Lexicon::Speech].iter().any(|&lexicon| {
                                self.tag(start + offset, lexicon, Position::Trailing)
                                    .is_some()
                            })
                    });
                if let Some(shift) = self.shift(&clause, &tag).filter(|_| !continued) {
                    let mut end = end;
                    if thought && end == bounds.end {
                        let edge = self.expand(sentence, &tag, shift, false);
                        if edge > sentence {
                            end = self.sentence(edge).end;
                        }
                    }
                    self.push(found, start..end, &tag, shift, lexicon);
                    continue;
                }
            }
            // A and D: a clause, a separator, then a tag (*I’m done, she thought.*), optionally
            // followed by a second clause (*I have to go—she thought—before he wakes.*).
            // An earlier tag or span in the sentence bounds the next clause.
            let mut floor = found
                .iter()
                .flat_map(|found| [found.span.byte.end, found.tag.byte.end])
                .filter(|&end| end <= items[bounds.end - 1].token.span.byte.end)
                .map(|end| items.partition_point(|item| item.token.span.byte.start < end))
                .filter(|&index| index > bounds.start && index <= bounds.end)
                .max()
                .unwrap_or(bounds.start);
            for separator in bounds.start + 1..bounds.end {
                let item = &items[separator];
                let separates = match item.token.text.as_str() {
                    "," | "—" | "–" => true,
                    "?" | "!" | "…" | "..." => thought,
                    _ => false,
                };
                if !separates || item.blocked {
                    continue;
                }
                if separator < floor {
                    continue;
                }
                let Some(tag) = self
                    .tag(separator + 1, lexicon, Position::Trailing)
                    .filter(|tag| self.tag_ends(tag, Position::Trailing))
                else {
                    continue;
                };
                let start =
                    self.after_closed(self.clause_before(&bounds, separator).max(floor), separator);
                floor = tag.range.end;
                let keep = !matches!(item.token.text.as_str(), "," | "—" | "–");
                let clause = start..separator + usize::from(keep);
                let Some(shift) = self.shift(&clause, &tag) else {
                    continue;
                };
                let mut first = start;
                if thought && start == bounds.start {
                    let edge = self.expand(sentence, &tag, shift, true);
                    if edge < sentence {
                        first = self.sentence(edge).start;
                    }
                }
                self.push(found, first..clause.end, &tag, shift, lexicon);
                // D: the second clause joins when it continues the same thought.
                let after = tag.range.end;
                if after < bounds.end && matches!(items[after].token.text.as_str(), "," | "—" | "–")
                {
                    let second_start = after + 1;
                    // The second clause ends at a quote or caller span, or before the next tag.
                    let second_end = (second_start..bounds.end)
                        .find(|&index| {
                            items[index].blocked
                                || (matches!(items[index].token.text.as_str(), "," | "—" | "–")
                                    && [Lexicon::Thought, Lexicon::Speech].iter().any(|&lexicon| {
                                        self.tag(index + 1, lexicon, Position::Trailing).is_some()
                                    }))
                        })
                        .unwrap_or(bounds.end);
                    let second = second_start..second_end;
                    // A lowercase continuation that is not a coordinator or a participle or
                    // past predicate of the tag's subject (*she thought, ladling out soup*).
                    let opens = items[second.clone()]
                        .iter()
                        .find(|item| is_word(item))
                        .is_some_and(|item| {
                            !capitalized(item)
                                && !COORDINATORS.contains(&word(item).as_str())
                                && !predicate_opening(item.token)
                        });
                    if opens && self.continues(&second, &tag, shift) {
                        floor = second.end;
                        self.push(found, second, &tag, shift, lexicon);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Winner {
    Marked(usize),
    Quote(usize),
    Detected(usize),
}

fn resolve(
    document: &Document,
    marked: Vec<Marked>,
    detected: Vec<Detected>,
    quoted_thought: &BTreeSet<usize>,
) -> Vec<VoiceSpan> {
    let mut points: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut add = |span: &Span| {
        points.insert(span.byte.start, (span.char.start, span.utf16.start));
        points.insert(span.byte.end, (span.char.end, span.utf16.end));
    };
    marked.iter().for_each(|m| add(&m.span));
    document.dialogue.iter().for_each(&mut add);
    detected.iter().for_each(|d| add(&d.span));
    let points: Vec<(usize, (usize, usize))> = points.into_iter().collect();
    let mut by_start: Vec<usize> = (0..marked.len()).collect();
    by_start.sort_by_key(|&index| marked[index].span.byte.start);
    let mut next_marked = 0;
    let mut active: Vec<usize> = Vec::new();
    let mut voices: Vec<VoiceSpan> = Vec::new();
    let mut previous: Option<Winner> = None;
    for pair in points.windows(2) {
        let ((start, (start_char, start_utf16)), (end, (end_char, end_utf16))) = (pair[0], pair[1]);
        while next_marked < by_start.len() && marked[by_start[next_marked]].span.byte.start <= start
        {
            active.push(by_start[next_marked]);
            next_marked += 1;
        }
        active.retain(|&index| marked[index].span.byte.end > start);
        let containing = |span: &Span| span.byte.start <= start && end <= span.byte.end;
        let winner = active
            .iter()
            .copied()
            .filter(|&index| containing(&marked[index].span))
            .min_by_key(|&index| {
                (
                    marked[index].span.byte.len(),
                    std::cmp::Reverse(marked[index].index),
                )
            })
            .map(Winner::Marked)
            .or_else(|| {
                let index = document
                    .dialogue
                    .partition_point(|span| span.byte.end <= start);
                document
                    .dialogue
                    .get(index)
                    .filter(|span| containing(span))
                    .map(|_| Winner::Quote(index))
            })
            .or_else(|| {
                let index = detected.partition_point(|d| d.span.byte.end <= start);
                detected
                    .get(index)
                    .filter(|d| containing(&d.span))
                    .map(|_| Winner::Detected(index))
            });
        let Some(winner) = winner else {
            previous = None;
            continue;
        };
        let (voice, source, tag, shift, marked_index) = match winner {
            Winner::Marked(index) => (
                Voice::from(marked[index].voice),
                VoiceSource::Caller,
                None,
                None,
                Some(marked[index].index),
            ),
            Winner::Quote(index) => {
                let voice = if !document.is_speech(index) {
                    Voice::Quoted
                } else if quoted_thought.contains(&index) {
                    Voice::Thought
                } else {
                    Voice::Speech
                };
                (voice, VoiceSource::Quote, None, None, None)
            }
            Winner::Detected(index) => (
                detected[index].voice,
                VoiceSource::Tag,
                Some(detected[index].tag.clone()),
                Some(detected[index].shift),
                None,
            ),
        };
        if voice == Voice::Narration {
            previous = Some(winner);
            continue;
        }
        if previous == Some(winner) {
            if let Some(last) = voices.last_mut() {
                last.span.byte.end = end;
                last.span.char.end = end_char;
                last.span.utf16.end = end_utf16;
                continue;
            }
        }
        previous = Some(winner);
        voices.push(VoiceSpan {
            span: Span {
                byte: start..end,
                char: start_char..end_char,
                utf16: start_utf16..end_utf16,
            },
            voice,
            source,
            tag,
            shift,
            marked: marked_index,
        });
    }
    voices
}
