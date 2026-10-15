use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{Document, Span, Token};

const SUBJECT_PRONOUNS: &[&str] = &["i", "you", "he", "she", "it", "we", "they", "one"];
const DETERMINERS: &[&str] = &[
    "the", "a", "an", "his", "her", "my", "your", "our", "their", "its", "this", "that", "these",
    "those", "each", "every", "another", "some",
];
const AUXILIARIES: &[&str] = &["be", "have", "do"];
const PARTICLES: &[&str] = &["on", "in", "out", "up"];
const SPEECH_VERBS: &str = include_str!("../../data/dialogue/speech-verbs.txt");
const NON_SPEECH_VERBS: &str = include_str!("../../data/dialogue/non-speech-verbs.txt");
const ADVERB_EXCLUSIONS: &str = include_str!("../../data/dialogue/adverb-exclusions.txt");
const MAX_SUBJECT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TagPosition {
    Before,
    After,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VerbClass {
    Speech,
    NonSpeech,
    Unlisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DialogueTag {
    pub span: Span,
    pub clause: Span,
    pub verb: Span,
    pub verb_text: String,
    pub lemma: String,
    pub class: VerbClass,
    pub position: TagPosition,
    pub adverb: Option<Span>,
    pub dialogue: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ActionBeat {
    pub span: Span,
    pub dialogue: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UntaggedRun {
    pub span: Span,
    pub dialogue: Vec<usize>,
    pub exchanges: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct VerbCount {
    pub lemma: String,
    pub class: VerbClass,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DialogueTagBaseline {
    pub said_asked_percent: f64,
    pub adverb_percent: f64,
    pub non_speech_percent: f64,
}

impl DialogueTagBaseline {
    pub fn new(said_asked_percent: f64, adverb_percent: f64, non_speech_percent: f64) -> Self {
        Self {
            said_asked_percent,
            adverb_percent,
            non_speech_percent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct DialogueTagOptions {
    #[serde(alias = "maxUntaggedExchanges")]
    pub max_untagged_exchanges: usize,
    #[serde(alias = "minTags")]
    pub min_tags: usize,
    pub baseline: Option<DialogueTagBaseline>,
}

impl Default for DialogueTagOptions {
    fn default() -> Self {
        Self {
            max_untagged_exchanges: 6,
            min_tags: 50,
            baseline: None,
        }
    }
}

impl DialogueTagOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_max_untagged_exchanges(mut self, exchanges: usize) -> Self {
        self.max_untagged_exchanges = exchanges;
        self
    }
    pub fn with_min_tags(mut self, tags: usize) -> Self {
        self.min_tags = tags;
        self
    }
    pub fn with_baseline(mut self, baseline: DialogueTagBaseline) -> Self {
        self.baseline = Some(baseline);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DialogueTagReport {
    pub tags: Vec<DialogueTag>,
    pub beats: Vec<ActionBeat>,
    pub untagged_runs: Vec<UntaggedRun>,
    pub other_verbs: Vec<VerbCount>,
    pub dialogue_spans: usize,
    pub tagged_spans: usize,
    pub exchanges: usize,
    pub untagged_exchanges: usize,
    pub said_asked: usize,
    pub with_adverb: usize,
    pub non_speech: usize,
    pub said_asked_percent: Option<f64>,
    pub adverb_percent: Option<f64>,
    pub non_speech_percent: Option<f64>,
    pub baseline: Option<DialogueTagBaseline>,
}

struct Lists {
    speech: BTreeSet<&'static str>,
    non_speech: BTreeSet<&'static str>,
    adverb_exclusions: BTreeSet<&'static str>,
}

fn lists() -> &'static Lists {
    static LISTS: OnceLock<Lists> = OnceLock::new();
    LISTS.get_or_init(|| {
        let parse = |data: &'static str| {
            data.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect()
        };
        Lists {
            speech: parse(SPEECH_VERBS),
            non_speech: parse(NON_SPEECH_VERBS),
            adverb_exclusions: parse(ADVERB_EXCLUSIONS),
        }
    })
}

pub fn speech_verbs() -> impl Iterator<Item = &'static str> {
    lists().speech.iter().copied()
}

pub fn non_speech_verbs() -> impl Iterator<Item = &'static str> {
    lists().non_speech.iter().copied()
}

#[derive(Clone, Copy)]
struct Item<'a> {
    token: &'a Token,
    sentence: usize,
}

struct Piece {
    dialogue: usize,
    tokens: Range<usize>,
    starts: bool,
    ends: bool,
    last: Option<char>,
}

struct Parse {
    verb: usize,
    lemma: String,
    class: VerbClass,
    adverb: Option<usize>,
    extent: Range<usize>,
}

struct Paragraph<'a> {
    items: Vec<Item<'a>>,
    pieces: Vec<Piece>,
}

fn lower(token: &Token) -> String {
    token.text.to_lowercase()
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token
        .readings
        .iter()
        .any(|reading| reading.pos.starts_with(prefix))
}

fn is_verb(token: &Token) -> bool {
    token.is_word && !token.is_skipped && has_pos(token, "VB")
}

fn is_finite(token: &Token) -> bool {
    token.is_word
        && !token.is_skipped
        && token
            .readings
            .iter()
            .any(|reading| matches!(reading.pos.as_str(), "VBD" | "VBZ" | "VBP" | "MD"))
}

fn is_pronoun(token: &Token) -> bool {
    SUBJECT_PRONOUNS.contains(&lower(token).as_str())
}

fn is_determiner(token: &Token) -> bool {
    DETERMINERS.contains(&lower(token).as_str())
}

fn is_name(token: &Token) -> bool {
    token.is_skipped
        || (token.is_word && token.text.chars().next().is_some_and(char::is_uppercase))
        || has_pos(token, "NNP")
}

fn is_nominal(token: &Token) -> bool {
    token.is_word && (has_pos(token, "NN") || has_pos(token, "JJ") || is_name(token))
}

fn is_adverb(token: &Token) -> bool {
    let text = lower(token);
    token.is_word
        && !token.is_skipped
        && text.ends_with("ly")
        && has_pos(token, "RB")
        && !lists().adverb_exclusions.contains(text.as_str())
}

fn is_auxiliary(token: &Token) -> bool {
    has_pos(token, "MD")
        || token.readings.iter().any(|reading| {
            reading.pos.starts_with("VB") && AUXILIARIES.contains(&reading.lemma.as_str())
        })
}

fn is_clitic(token: &Token) -> bool {
    matches!(
        token.text.replace('’', "'").as_str(),
        "'d" | "'s" | "'ll" | "'ve" | "'re" | "'m"
    )
}

fn stops(token: &Token) -> bool {
    !token.text.is_empty() && token.text.chars().all(|ch| matches!(ch, '!' | '?'))
}

fn ends_sentence(token: &Token) -> bool {
    !token.text.is_empty()
        && token
            .text
            .chars()
            .all(|ch| matches!(ch, '.' | '!' | '?' | '…'))
}

fn verb_lemma(token: &Token, next: Option<&Token>) -> (String, VerbClass, bool) {
    let lists = lists();
    let particle = next
        .filter(|next| next.is_word)
        .map(lower)
        .filter(|word| PARTICLES.contains(&word.as_str()));
    let lemmas: BTreeSet<String> = token
        .readings
        .iter()
        .filter(|reading| reading.pos.starts_with("VB"))
        .map(|reading| reading.lemma.to_lowercase())
        .collect();
    for (set, class) in [
        (&lists.speech, VerbClass::Speech),
        (&lists.non_speech, VerbClass::NonSpeech),
    ] {
        if let Some(particle) = &particle {
            if let Some(lemma) = lemmas
                .iter()
                .map(|lemma| format!("{lemma} {particle}"))
                .find(|phrase| set.contains(phrase.as_str()))
            {
                return (lemma, class, true);
            }
        }
        if let Some(lemma) = lemmas.iter().find(|lemma| set.contains(lemma.as_str())) {
            return (lemma.clone(), class, false);
        }
    }
    let lemma = lemmas.into_iter().next().unwrap_or_else(|| lower(token));
    (lemma, VerbClass::Unlisted, false)
}

fn listed(token: &Token, next: Option<&Token>) -> bool {
    is_verb(token) && verb_lemma(token, next).1 != VerbClass::Unlisted
}

fn span_of(items: &[Item], range: Range<usize>) -> Span {
    join(
        &items[range.start].token.span,
        &items[range.end - 1].token.span,
    )
}

fn first_word(items: &[Item], clause: &Range<usize>) -> usize {
    clause
        .clone()
        .find(|&index| items[index].token.is_word)
        .unwrap_or(clause.start)
}

fn join(first: &Span, last: &Span) -> Span {
    Span {
        byte: first.byte.start..last.byte.end,
        char: first.char.start..last.char.end,
        utf16: first.utf16.start..last.utf16.end,
    }
}

impl<'a> Paragraph<'a> {
    fn word(&self, index: usize) -> Option<&'a Token> {
        self.items
            .get(index)
            .map(|item| item.token)
            .filter(|token| token.is_word)
    }

    fn next_word(&self, index: usize, end: usize) -> Option<&'a Token> {
        self.items[index + 1..end]
            .iter()
            .map(|item| item.token)
            .find(|token| token.is_word)
    }

    fn words(&self, range: &Range<usize>) -> Vec<usize> {
        range
            .clone()
            .filter(|&index| self.items[index].token.is_word)
            .collect()
    }

    fn head(&self, range: &Range<usize>) -> Range<usize> {
        let sentence = self.items[range.start].sentence;
        let mut end = range.start;
        while end < range.end && self.items[end].sentence == sentence {
            end += 1;
            if stops(self.items[end - 1].token) {
                break;
            }
        }
        range.start..end
    }

    fn tail(&self, range: &Range<usize>) -> Range<usize> {
        let sentence = self.items[range.end - 1].sentence;
        let mut start = range.end;
        while start > range.start {
            let previous = start - 1;
            if self.items[previous].sentence != sentence
                || (previous != range.end - 1 && stops(self.items[previous].token))
            {
                break;
            }
            start = previous;
        }
        start..range.end
    }

    fn last_text(&self, range: &Range<usize>) -> &str {
        &self.items[range.end - 1].token.text
    }

    fn subject(&self, words: &[usize], start: usize, length: usize) -> bool {
        let tokens: Vec<&Token> = words[start..start + length]
            .iter()
            .map(|&index| self.items[index].token)
            .collect();
        let first = tokens[0];
        if is_pronoun(first) {
            return length == 1;
        }
        (is_determiner(first) || is_name(first) || is_nominal(first))
            && tokens[1..]
                .iter()
                .all(|token| is_nominal(token) && !is_pronoun(token) && !is_determiner(token))
    }

    fn verb_parse(&self, index: usize, clause_end: usize) -> (String, VerbClass, bool) {
        verb_lemma(self.items[index].token, self.next_word(index, clause_end))
    }

    fn adverb_at(&self, index: Option<&usize>) -> Option<usize> {
        index
            .copied()
            .filter(|&index| is_adverb(self.items[index].token))
    }

    fn after_tag(&self, clause: &Range<usize>) -> Option<Parse> {
        let words = self.words(clause);
        let mut offset = 0;
        if words.len() > 1 {
            let first = self.items[words[0]].token;
            if has_pos(first, "RB") && !has_pos(first, "NN") && !is_verb(first) {
                offset = 1;
            }
        }
        let words = &words[offset..];
        let mut fallback = None;
        if let Some(&verb) = words.first() {
            if is_verb(self.items[verb].token) {
                let (lemma, class, particle) = self.verb_parse(verb, clause.end);
                let after = 1 + usize::from(particle);
                let mut adverb = self.adverb_at(words.get(after));
                let subject_start = after + usize::from(adverb.is_some());
                if let Some(length) = (1..=MAX_SUBJECT)
                    .filter(|&length| subject_start + length <= words.len())
                    .find(|&length| {
                        self.subject(words, subject_start, length)
                            && (subject_start + length == words.len()
                                || !is_nominal(self.items[words[subject_start + length]].token))
                    })
                {
                    let subject_end = subject_start + length;
                    if adverb.is_none() {
                        adverb = self.adverb_at(words.get(subject_end));
                    }
                    let last = adverb.map_or(words[subject_end - 1], |adverb| {
                        adverb.max(words[subject_end - 1])
                    });
                    let parse = Parse {
                        verb,
                        lemma,
                        class,
                        adverb,
                        extent: verb..last + 1,
                    };
                    if class != VerbClass::Unlisted {
                        return Some(parse);
                    }
                    if is_finite(self.items[verb].token) {
                        fallback = Some(parse);
                    }
                }
            }
        }
        for length in 1..=MAX_SUBJECT.min(words.len().saturating_sub(1)) {
            if !self.subject(words, 0, length) {
                continue;
            }
            let mut position = length;
            let mut adverb = None;
            let mut auxiliary = false;
            while position < words.len() {
                let token = self.items[words[position]].token;
                if adverb.is_none() && is_adverb(token) {
                    adverb = Some(words[position]);
                } else if !auxiliary
                    && (is_auxiliary(token) || is_clitic(token))
                    && words
                        .get(position + 1)
                        .is_some_and(|&next| is_verb(self.items[next].token))
                {
                    auxiliary = true;
                } else {
                    break;
                }
                position += 1;
            }
            let Some(&verb) = words.get(position) else {
                continue;
            };
            let token = self.items[verb].token;
            if !is_verb(token) {
                continue;
            }
            let (lemma, class, particle) = self.verb_parse(verb, clause.end);
            if adverb.is_none() {
                adverb = self.adverb_at(words.get(position + 1 + usize::from(particle)));
            }
            let end = adverb.map_or(verb, |adverb| adverb.max(verb));
            let parse = Parse {
                verb,
                lemma,
                class,
                adverb,
                extent: words[0]..end + 1,
            };
            if class != VerbClass::Unlisted {
                return Some(parse);
            }
            if fallback.is_none()
                && (is_finite(token) || auxiliary)
                && !AUXILIARIES.contains(&parse.lemma.as_str())
            {
                fallback = Some(parse);
            }
        }
        fallback
    }

    fn before_tag(&self, clause: &Range<usize>) -> Option<Parse> {
        let words = self.words(clause);
        let tail = words.len().saturating_sub(4);
        for position in (tail..words.len()).rev() {
            let verb = words[position];
            let token = self.items[verb].token;
            if !listed(token, self.next_word(verb, clause.end)) {
                continue;
            }
            let trailing = &words[position + 1..];
            let (lemma, class, particle) = self.verb_parse(verb, clause.end);
            let trailing = &trailing[usize::from(particle).min(trailing.len())..];
            let allowed = trailing.iter().all(|&index| {
                let token = self.items[index].token;
                is_adverb(token)
                    || is_name(token)
                    || has_pos(token, "PRP")
                    || matches!(lower(token).as_str(), "to" | "the")
                    || (has_pos(token, "NN") && !is_verb(token))
            });
            let subject_before = words[..position].iter().any(|&index| {
                let token = self.items[index].token;
                is_pronoun(token) || is_name(token) || has_pos(token, "NN")
            });
            let subject_after = trailing.first().is_some_and(|&index| {
                is_pronoun(self.items[index].token) || is_name(self.items[index].token)
            });
            if !allowed || !(subject_before || subject_after) {
                continue;
            }
            let adverb = self.adverb_at(trailing.first()).or_else(|| {
                position
                    .checked_sub(1)
                    .and_then(|p| self.adverb_at(words.get(p)))
            });
            let start = self.phrase_start(verb, clause.start);
            let end = trailing.last().copied().unwrap_or(verb).max(verb);
            return Some(Parse {
                verb,
                lemma,
                class,
                adverb,
                extent: start.min(adverb.unwrap_or(start))..end + 1,
            });
        }
        let &verb = words.last()?;
        let token = self.items[verb].token;
        let subject = words.len() >= 2 && {
            let previous = self.items[words[words.len() - 2]].token;
            is_pronoun(previous) || is_name(previous)
        };
        if !subject || !is_finite(token) {
            return None;
        }
        let (lemma, class, _) = self.verb_parse(verb, clause.end);
        if AUXILIARIES.contains(&lemma.as_str()) {
            return None;
        }
        Some(Parse {
            verb,
            lemma,
            class,
            adverb: None,
            extent: words[words.len() - 2]..verb + 1,
        })
    }

    fn phrase_start(&self, verb: usize, floor: usize) -> usize {
        let mut start = verb;
        while start > floor && verb - start < MAX_SUBJECT + 1 {
            let token = self.items[start - 1].token;
            let title =
                token.text == "." && start >= floor + 2 && is_name(self.items[start - 2].token);
            if !title
                && !(token.is_word
                    && (is_nominal(token)
                        || is_pronoun(token)
                        || is_determiner(token)
                        || is_adverb(token)))
            {
                break;
            }
            start -= 1;
        }
        start
    }

    fn attached_after(&self, piece: &Piece, clause: &Range<usize>) -> bool {
        let Some(&first) = self.words(clause).first() else {
            return false;
        };
        let lowercase = self.items[first]
            .token
            .text
            .chars()
            .next()
            .is_some_and(char::is_lowercase);
        match piece.last {
            Some(',' | ':' | ';') => true,
            Some('.') => lowercase,
            _ => {
                lowercase
                    || self
                        .after_tag(clause)
                        .is_some_and(|parse| parse.class == VerbClass::Speech)
            }
        }
    }

    fn attached_before(&self, clause: &Range<usize>) -> bool {
        matches!(self.last_text(clause), "," | ":")
            || self.word(clause.end - 1).is_some_and(|token| {
                is_verb(token) && verb_lemma(token, None).1 == VerbClass::Speech
            })
    }

    fn has_verb(&self, clause: &Range<usize>) -> bool {
        let words = self.words(clause);
        words.iter().enumerate().any(|(position, &index)| {
            is_finite(self.items[index].token)
                && !position
                    .checked_sub(1)
                    .is_some_and(|previous| is_determiner(self.items[words[previous]].token))
        })
    }
}

fn last_char(text: &str) -> Option<char> {
    text.chars()
        .rev()
        .skip(1)
        .find(|ch| !ch.is_whitespace() && !matches!(ch, '\u{2060}' | '\u{200B}' | '\u{FEFF}'))
}

fn paragraphs(document: &Document) -> Vec<Paragraph<'_>> {
    document
        .paragraphs
        .iter()
        .map(|paragraph| {
            let items: Vec<Item> = paragraph
                .sentences
                .iter()
                .enumerate()
                .flat_map(|(sentence, tokens)| {
                    tokens
                        .tokens
                        .iter()
                        .filter(|token| !token.is_whitespace)
                        .map(move |token| Item { token, sentence })
                })
                .collect();
            let range = &paragraph.span.byte;
            let first = document
                .dialogue
                .partition_point(|span| span.byte.end <= range.start);
            let pieces = document
                .dialogue
                .iter()
                .enumerate()
                .skip(first)
                .take_while(|(_, span)| span.byte.start < range.end)
                .filter_map(|(dialogue, span)| {
                    let inside: Vec<usize> = items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| {
                            item.token.span.byte.start >= span.byte.start
                                && item.token.span.byte.end <= span.byte.end
                        })
                        .map(|(index, _)| index)
                        .collect();
                    let (&first, &last) = (inside.first()?, inside.last()?);
                    let embedded = first
                        .checked_sub(1)
                        .map(|previous| &items[previous])
                        .is_some_and(|previous| {
                            previous.token.is_word && !listed(previous.token, None)
                        });
                    if embedded {
                        return None;
                    }
                    let ends = span.byte.end <= range.end;
                    let local = span.byte.start.max(range.start) - range.start
                        ..span.byte.end.min(range.end) - range.start;
                    Some(Piece {
                        dialogue,
                        tokens: first..last + 1,
                        starts: span.byte.start >= range.start,
                        ends,
                        last: if ends {
                            paragraph.text.get(local).and_then(last_char)
                        } else {
                            None
                        },
                    })
                })
                .collect();
            Paragraph { items, pieces }
        })
        .collect()
}

struct Builder<'a> {
    tags: Vec<DialogueTag>,
    beats: Vec<ActionBeat>,
    attributed: BTreeSet<usize>,
    paragraph: Option<&'a Paragraph<'a>>,
}

impl<'a> Builder<'a> {
    fn tag(
        &mut self,
        clause: Range<usize>,
        parse: Parse,
        position: TagPosition,
        dialogue: Vec<usize>,
    ) {
        let items = &self.paragraph.unwrap().items;
        let token = items[parse.verb].token;
        self.attributed.extend(&dialogue);
        self.tags.push(DialogueTag {
            span: span_of(items, parse.extent),
            clause: span_of(items, first_word(items, &clause)..clause.end),
            verb: token.span.clone(),
            verb_text: token.text.clone(),
            lemma: parse.lemma,
            class: parse.class,
            position,
            adverb: parse.adverb.map(|index| items[index].token.span.clone()),
            dialogue,
        });
    }

    fn beat(&mut self, clause: Range<usize>, dialogue: Vec<usize>) {
        let items = &self.paragraph.unwrap().items;
        self.attributed.extend(&dialogue);
        self.beats.push(ActionBeat {
            span: span_of(items, first_word(items, &clause)..clause.end),
            dialogue,
        });
    }
}

pub fn dialogue_tags(document: &Document, options: &DialogueTagOptions) -> DialogueTagReport {
    let paragraphs = paragraphs(document);
    let mut builder = Builder {
        tags: Vec::new(),
        beats: Vec::new(),
        attributed: BTreeSet::new(),
        paragraph: None,
    };
    for (index, paragraph) in paragraphs.iter().enumerate() {
        if paragraph.items.is_empty() {
            continue;
        }
        builder.paragraph = Some(paragraph);
        let lead_in = paragraphs
            .get(index + 1)
            .and_then(|next| next.pieces.first())
            .filter(|piece| piece.starts && piece.tokens.start == 0)
            .map(|piece| piece.dialogue);
        let mut boundaries = vec![0];
        for piece in &paragraph.pieces {
            boundaries.push(piece.tokens.start);
            boundaries.push(piece.tokens.end);
        }
        boundaries.push(paragraph.items.len());
        for (segment, bounds) in boundaries.chunks(2).enumerate() {
            let range = bounds[0]..bounds[1];
            if paragraph.words(&range).is_empty() {
                continue;
            }
            let previous = segment
                .checked_sub(1)
                .map(|k| &paragraph.pieces[k])
                .filter(|piece| piece.ends);
            let next = paragraph.pieces.get(segment).filter(|piece| piece.starts);
            let head = paragraph.head(&range);
            let tail = paragraph.tail(&range);
            let single = head == tail;
            let mut head_used = false;
            if let Some(previous) = previous {
                if paragraph.attached_after(previous, &head) {
                    if let Some(parse) = paragraph.after_tag(&head) {
                        let terminal = ends_sentence(paragraph.items[head.end - 1].token);
                        match next {
                            Some(next) if head.end == range.end && !terminal => builder.tag(
                                head.clone(),
                                parse,
                                TagPosition::Split,
                                vec![previous.dialogue, next.dialogue],
                            ),
                            _ => builder.tag(
                                head.clone(),
                                parse,
                                TagPosition::After,
                                vec![previous.dialogue],
                            ),
                        }
                    }
                    head_used = true;
                } else if paragraph.has_verb(&head) {
                    let leads = single
                        && (next.is_some() || lead_in.is_some())
                        && paragraph.attached_before(&tail)
                        && paragraph.before_tag(&tail).is_some();
                    if !leads {
                        let mut dialogue = vec![previous.dialogue];
                        if single {
                            dialogue.extend(next.map(|next| next.dialogue));
                        }
                        builder.beat(head.clone(), dialogue);
                        head_used = true;
                    }
                }
            }
            if single && head_used {
                continue;
            }
            let target = match next {
                Some(next) => Some(next.dialogue),
                None if segment == paragraph.pieces.len() => lead_in,
                None => None,
            };
            let Some(target) = target else {
                continue;
            };
            if paragraph.attached_before(&tail) {
                if let Some(parse) = paragraph.before_tag(&tail) {
                    builder.tag(tail, parse, TagPosition::Before, vec![target]);
                    continue;
                }
            }
            if next.is_some() && paragraph.has_verb(&tail) {
                builder.beat(tail, vec![target]);
            }
        }
    }
    let Builder {
        tags,
        beats,
        attributed,
        ..
    } = builder;
    let (untagged_runs, untagged_exchanges) =
        untagged_runs(document, &paragraphs, &attributed, options);
    let exchanges = paragraphs
        .iter()
        .filter(|paragraph| paragraph.pieces.iter().any(|piece| piece.starts))
        .count();
    let mut verbs: BTreeMap<(String, VerbClass), usize> = BTreeMap::new();
    for tag in &tags {
        *verbs.entry((tag.lemma.clone(), tag.class)).or_default() += 1;
    }
    let said_asked = verbs
        .iter()
        .filter(|((lemma, _), _)| matches!(lemma.as_str(), "say" | "ask"))
        .map(|(_, count)| count)
        .sum();
    let mut other_verbs: Vec<VerbCount> = verbs
        .into_iter()
        .filter(|((lemma, _), _)| !matches!(lemma.as_str(), "say" | "ask"))
        .map(|((lemma, class), count)| VerbCount {
            lemma,
            class,
            count,
        })
        .collect();
    other_verbs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.lemma.cmp(&b.lemma)));
    let with_adverb = tags.iter().filter(|tag| tag.adverb.is_some()).count();
    let non_speech = tags
        .iter()
        .filter(|tag| tag.class == VerbClass::NonSpeech)
        .count();
    let percent = |count: usize| {
        (tags.len() >= options.min_tags.max(1)).then(|| 100.0 * count as f64 / tags.len() as f64)
    };
    let tagged_spans = tags
        .iter()
        .flat_map(|tag| tag.dialogue.iter().copied())
        .collect::<BTreeSet<_>>()
        .len();
    DialogueTagReport {
        said_asked_percent: percent(said_asked),
        adverb_percent: percent(with_adverb),
        non_speech_percent: percent(non_speech),
        dialogue_spans: document.dialogue.len(),
        tagged_spans,
        exchanges,
        untagged_exchanges,
        said_asked,
        with_adverb,
        non_speech,
        other_verbs,
        untagged_runs,
        beats,
        tags,
        baseline: options.baseline.clone(),
    }
}

fn untagged_runs(
    document: &Document,
    paragraphs: &[Paragraph],
    attributed: &BTreeSet<usize>,
    options: &DialogueTagOptions,
) -> (Vec<UntaggedRun>, usize) {
    let mut runs = Vec::new();
    let mut untagged = 0;
    let mut current: Vec<Vec<usize>> = Vec::new();
    let mut close = |current: &mut Vec<Vec<usize>>| {
        if current.len() > options.max_untagged_exchanges {
            let dialogue: Vec<usize> = current.iter().flatten().copied().collect();
            let first = &document.dialogue[dialogue[0]];
            let last = &document.dialogue[*dialogue.last().unwrap()];
            runs.push(UntaggedRun {
                span: join(first, last),
                exchanges: current.len(),
                dialogue,
            });
        }
        current.clear();
    };
    for paragraph in paragraphs {
        let starting: Vec<usize> = paragraph
            .pieces
            .iter()
            .filter(|piece| piece.starts)
            .map(|piece| piece.dialogue)
            .collect();
        if starting.is_empty() {
            if paragraph.pieces.is_empty() && !paragraph.items.is_empty() {
                close(&mut current);
            }
            continue;
        }
        if starting.iter().any(|index| attributed.contains(index)) {
            close(&mut current);
        } else {
            untagged += 1;
            current.push(starting);
        }
    }
    close(&mut current);
    (runs, untagged)
}
