use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use nlprule::Tokenizer;
use serde::{Deserialize, Serialize};

use crate::{Document, Span, Token};

const NO_CLICHES: &str = include_str!("../../data/cliches/no-cliches.txt");
const PROSELINT_CLICHES: &str = include_str!("../../data/cliches/proselint.txt");
const REDUNDANT_ACRONYMS: &str = include_str!("../../data/redundancy/redundant-acronyms.tsv");
const PROSELINT_RAS: &str = include_str!("../../data/redundancy/proselint-ras.tsv");
const SUPPRESSED: &str = include_str!("../../data/suppressed.txt");
const FIXED_WORDING: &str = include_str!("../../data/fixed-wording.txt");

const ARTICLES: &[&str] = &["a", "an", "the"];
const DETERMINERS: &[&str] = &["a", "an", "the", "this", "that"];
const POSSESSIVES: &[&str] = &[
    "my", "your", "his", "her", "its", "our", "their", "thy", "whose",
];
const PERSONAL: &[&str] = &[
    "i", "you", "he", "she", "we", "they", "me", "him", "her", "us", "them",
];
const REFLEXIVES: &[&str] = &[
    "myself",
    "yourself",
    "himself",
    "herself",
    "itself",
    "ourselves",
    "yourselves",
    "themselves",
    "oneself",
];
const JOINERS: &[&str] = &["-", "‐", "‑", "'"];
const PHRASE_PUNCTUATION: &[&str] = &[",", ";", ":", "—", "–", "-", "…"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PhraseKind {
    Cliche,
    Redundancy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PhraseContext {
    Narration,
    Dialogue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PhraseEntry {
    pub kind: PhraseKind,
    pub phrase: String,
    pub source: String,
    #[serde(default)]
    pub acronym: Option<String>,
    #[serde(default)]
    pub fixed_wording: bool,
}

impl PhraseEntry {
    pub fn new(kind: PhraseKind, phrase: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            kind,
            phrase: phrase.into(),
            source: source.into(),
            acronym: None,
            fixed_wording: false,
        }
    }
    pub fn with_acronym(mut self, acronym: impl Into<String>) -> Self {
        self.acronym = Some(acronym.into());
        self
    }
    pub fn with_fixed_wording(mut self, fixed_wording: bool) -> Self {
        self.fixed_wording = fixed_wording;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Possessive,
    Personal,
    PossessiveOrPersonal,
    Reflexive,
}

#[derive(Debug, Clone)]
enum Element {
    Word {
        surface: String,
        lemmas: BTreeSet<String>,
    },
    Acronym(String),
    Slot(Slot),
}

#[derive(Debug, Clone)]
struct Pattern {
    entry: usize,
    optional_article: bool,
    elements: Vec<(Element, bool)>,
}

#[derive(Debug, Clone)]
pub struct PhraseList {
    entries: Vec<PhraseEntry>,
    patterns: Vec<Pattern>,
    by_first_key: HashMap<String, Vec<usize>>,
    slot_first: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PhraseMatch {
    pub kind: PhraseKind,
    pub context: PhraseContext,
    pub phrase: String,
    pub source: String,
    pub span: Span,
    pub paragraph: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PhraseCounts {
    pub cliches: usize,
    pub redundancies: usize,
    pub words: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ClicheReport {
    pub matches: Vec<PhraseMatch>,
    pub narration: PhraseCounts,
    pub dialogue: PhraseCounts,
}

fn normalize(text: &str) -> String {
    text.replace(['’', '‘', 'ʼ'], "'").to_lowercase()
}

fn lines(data: &str) -> impl Iterator<Item = &str> {
    data.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

pub fn suppressed_phrases() -> Vec<&'static str> {
    lines(SUPPRESSED).collect()
}

pub fn vendored_phrase_entries() -> Vec<PhraseEntry> {
    let mut entries = Vec::new();
    for line in lines(NO_CLICHES) {
        entries.push(PhraseEntry::new(PhraseKind::Cliche, line, "no-cliches"));
    }
    for line in lines(PROSELINT_CLICHES) {
        entries.push(PhraseEntry::new(PhraseKind::Cliche, line, "proselint"));
    }
    for line in lines(REDUNDANT_ACRONYMS) {
        let Some((acronym, expansion)) = line.split_once('\t') else {
            continue;
        };
        let words: Vec<_> = expansion.split_whitespace().collect();
        for start in 1..words.len() {
            entries.push(
                PhraseEntry::new(
                    PhraseKind::Redundancy,
                    format!("{acronym} {}", words[start..].join(" ")),
                    "retext-redundant-acronyms",
                )
                .with_acronym(acronym),
            );
        }
    }
    for line in lines(PROSELINT_RAS) {
        let Some((phrase, acronym)) = line.split_once('\t') else {
            continue;
        };
        let acronym = phrase
            .split_whitespace()
            .find(|word| {
                let word = word.to_uppercase();
                word == acronym.to_uppercase()
                    || acronym
                        .strip_suffix('s')
                        .is_some_and(|stem| word == stem.to_uppercase())
            })
            .map(str::to_uppercase);
        let Some(acronym) = acronym else {
            continue;
        };
        let words: Vec<_> = phrase
            .split_whitespace()
            .map(|word| {
                if word.eq_ignore_ascii_case(&acronym) {
                    acronym.clone()
                } else {
                    word.to_lowercase()
                }
            })
            .collect();
        entries.push(
            PhraseEntry::new(PhraseKind::Redundancy, words.join(" "), "proselint")
                .with_acronym(acronym),
        );
    }
    let fixed: BTreeSet<_> = lines(FIXED_WORDING).map(normalize).collect();
    let mut seen = BTreeSet::new();
    entries.retain(|entry| seen.insert((entry.kind, normalize(&entry.phrase))));
    for entry in &mut entries {
        entry.fixed_wording = fixed.contains(&normalize(&entry.phrase));
    }
    entries
}

pub fn bundled_phrase_entries() -> Vec<PhraseEntry> {
    let suppressed: BTreeSet<_> = lines(SUPPRESSED).map(normalize).collect();
    let mut entries = vendored_phrase_entries();
    entries.retain(|entry| !suppressed.contains(&normalize(&entry.phrase)));
    entries
}

fn slot(word: &str) -> Option<Slot> {
    match word {
        "her" => Some(Slot::PossessiveOrPersonal),
        word if POSSESSIVES.contains(&word) => Some(Slot::Possessive),
        word if PERSONAL.contains(&word) => Some(Slot::Personal),
        word if REFLEXIVES.contains(&word) => Some(Slot::Reflexive),
        _ => None,
    }
}

fn hyphen_parts(text: &str) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if matches!(ch, '-' | '‐' | '‑') {
            if start < index {
                parts.push(start..index);
            }
            start = index + ch.len_utf8();
        }
    }
    if start < text.len() {
        parts.push(start..text.len());
    }
    if parts.len() < 2 || !parts.iter().all(|part| crate::is_word(&text[part.clone()])) {
        return std::iter::once(0..text.len()).collect();
    }
    parts
}

fn compile(tokenizer: &Tokenizer, index: usize, entry: &PhraseEntry) -> Option<Pattern> {
    let tokens: Vec<_> = tokenizer
        .analyze(&entry.phrase)
        .into_iter()
        .flat_map(|sentence| sentence.tokens)
        .filter(|token| !token.is_whitespace)
        .collect();
    let acronym = entry.acronym.as_deref().map(normalize);
    let mut elements: Vec<(Element, bool)> = Vec::new();
    let mut loose = false;
    let mut position = 0;
    while position < tokens.len() {
        let token = &tokens[position];
        let surface = normalize(&token.text);
        position += 1;
        if !crate::is_word(&token.text) {
            loose |= !elements.is_empty();
            continue;
        }
        if surface == "one"
            && tokens
                .get(position)
                .is_some_and(|next| normalize(&next.text) == "'s")
        {
            position += 1;
            elements.push((Element::Slot(Slot::Possessive), loose));
        } else if acronym.as_deref() == Some(surface.as_str()) {
            elements.push((Element::Acronym(token.text.to_uppercase()), loose));
        } else if let Some(slot) = slot(&surface).filter(|_| !entry.fixed_wording) {
            elements.push((Element::Slot(slot), loose));
        } else {
            let parts = hyphen_parts(&token.text);
            for part in &parts[..parts.len() - 1] {
                elements.push((
                    Element::Word {
                        surface: normalize(&token.text[part.clone()]),
                        lemmas: BTreeSet::new(),
                    },
                    loose,
                ));
                loose = false;
            }
            let last = normalize(&token.text[parts[parts.len() - 1].clone()]);
            let mut lemmas: BTreeSet<_> = token
                .readings
                .iter()
                .map(|reading| normalize(&reading.lemma))
                .filter(|lemma| !lemma.contains(char::is_whitespace))
                .collect();
            if parts.len() > 1 || !lemmas.contains(&surface) {
                lemmas.clear();
            }
            elements.push((
                Element::Word {
                    surface: last,
                    lemmas,
                },
                loose,
            ));
        }
        loose = false;
    }
    let optional_article = elements.len() > 2
        && matches!(&elements[0].0, Element::Word { surface, .. } if ARTICLES.contains(&surface.as_str()));
    if optional_article {
        elements.remove(0);
        elements[0].1 = false;
    }
    if elements.len() < 2 {
        return None;
    }
    Some(Pattern {
        entry: index,
        optional_article,
        elements,
    })
}

impl PhraseList {
    pub fn bundled(tokenizer: &Tokenizer) -> Self {
        Self::new(tokenizer, bundled_phrase_entries())
    }

    pub fn new(tokenizer: &Tokenizer, entries: Vec<PhraseEntry>) -> Self {
        let mut list = Self {
            patterns: Vec::new(),
            by_first_key: HashMap::new(),
            slot_first: Vec::new(),
            entries,
        };
        for (index, entry) in list.entries.iter().enumerate() {
            let Some(pattern) = compile(tokenizer, index, entry) else {
                continue;
            };
            let id = list.patterns.len();
            match &pattern.elements[0].0 {
                Element::Word { surface, lemmas } => {
                    for key in lemmas.iter().chain([surface]) {
                        list.by_first_key.entry(key.clone()).or_default().push(id);
                    }
                }
                Element::Acronym(acronym) => list
                    .by_first_key
                    .entry(normalize(acronym))
                    .or_default()
                    .push(id),
                Element::Slot(_) => list.slot_first.push(id),
            }
            list.patterns.push(pattern);
        }
        list
    }

    pub fn entries(&self) -> &[PhraseEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

struct Item {
    surface: String,
    span_text: String,
    span: Span,
    is_word: bool,
    is_skipped: bool,
    keys: BTreeSet<String>,
}

fn adjacent(left: &Item, right: &Item) -> bool {
    left.span.byte.end == right.span.byte.start
}

fn is_clitic(item: &Item) -> bool {
    matches!(item.surface.as_str(), "'s" | "'")
}

fn part_span(token: &Token, part: &Range<usize>) -> Span {
    let before = &token.text[..part.start];
    let inside = &token.text[part.clone()];
    let char_start = token.span.char.start + before.chars().count();
    let utf16_start = token.span.utf16.start + before.encode_utf16().count();
    Span {
        byte: token.span.byte.start + part.start..token.span.byte.start + part.end,
        char: char_start..char_start + inside.chars().count(),
        utf16: utf16_start..utf16_start + inside.encode_utf16().count(),
    }
}

fn match_slot(stream: &[Item], at: usize, slot: Slot) -> Option<usize> {
    let token = stream.get(at)?;
    let surface = token.surface.clone();
    let possessive = matches!(slot, Slot::Possessive | Slot::PossessiveOrPersonal);
    let personal = matches!(slot, Slot::Personal | Slot::PossessiveOrPersonal);
    if possessive {
        if POSSESSIVES.contains(&surface.as_str()) {
            return Some(at + 1);
        }
        let start = if DETERMINERS.contains(&surface.as_str()) {
            at + 1
        } else {
            at
        };
        if let (Some(owner), Some(clitic)) = (stream.get(start), stream.get(start + 1)) {
            if owner.is_word && is_clitic(clitic) && adjacent(owner, clitic) {
                return Some(start + 2);
            }
        }
    }
    if personal {
        if PERSONAL.contains(&surface.as_str()) {
            return Some(at + 1);
        }
        if token.is_skipped {
            let mut end = at;
            while stream
                .get(end)
                .is_some_and(|item| item.is_skipped && !is_clitic(item))
            {
                end += 1;
            }
            return Some(end);
        }
    }
    if slot == Slot::Reflexive && REFLEXIVES.contains(&surface.as_str()) {
        return Some(at + 1);
    }
    None
}

fn comparative(pattern: &str, text: &str) -> bool {
    ["er", "est"]
        .iter()
        .any(|suffix| text.ends_with(suffix) && !pattern.ends_with(suffix))
}

fn match_element(stream: &[Item], at: usize, element: &Element) -> Option<usize> {
    let item = stream.get(at)?;
    match element {
        Element::Word { surface, lemmas } => {
            let inflected = item.surface != *surface
                && !comparative(surface, &item.surface)
                && !item.keys.is_disjoint(lemmas);
            (item.is_word && !item.is_skipped && (item.surface == *surface || inflected))
                .then_some(at + 1)
        }
        Element::Acronym(acronym) => {
            let surface = item.span_text.as_str();
            (!item.is_skipped && (surface == acronym || surface.strip_suffix('s') == Some(acronym)))
                .then_some(at + 1)
        }
        Element::Slot(slot) => match_slot(stream, at, *slot),
    }
}

fn match_pattern(stream: &[Item], start: usize, pattern: &Pattern) -> Option<usize> {
    let mut at = start;
    for (index, (element, loose)) in pattern.elements.iter().enumerate() {
        if index > 0 {
            let conjunction = matches!(element, Element::Word { surface, .. } if surface == "and" || surface == "or");
            while let Some(item) = stream.get(at) {
                let surface = item.surface.as_str();
                let joiner = JOINERS.contains(&surface);
                let punctuation = (*loose || conjunction) && surface == ","
                    || *loose && PHRASE_PUNCTUATION.contains(&surface);
                if item.is_word || !(joiner || punctuation) {
                    break;
                }
                at += 1;
            }
        }
        at = match_element(stream, at, element)?;
    }
    Some(at - 1)
}

fn token_keys(document: &Document) -> HashMap<(usize, usize, usize), Vec<&str>> {
    let mut keys: HashMap<_, Vec<&str>> = HashMap::new();
    for (lemma, positions) in &document.lemma_index {
        if lemma.contains(char::is_whitespace) {
            continue;
        }
        for position in positions {
            keys.entry((position.paragraph, position.sentence, position.token))
                .or_default()
                .push(lemma);
        }
    }
    keys
}

fn span_between(first: &Span, last: &Span) -> Span {
    Span {
        byte: first.byte.start..last.byte.end,
        char: first.char.start..last.char.end,
        utf16: first.utf16.start..last.utf16.end,
    }
}

fn in_dialogue(dialogue: &[Span], byte: usize) -> bool {
    let index = dialogue.partition_point(|span| span.byte.end <= byte);
    dialogue
        .get(index)
        .is_some_and(|span| span.byte.start <= byte)
}

pub fn cliches(document: &Document, phrases: &PhraseList) -> ClicheReport {
    let keys = token_keys(document);
    let mut report = ClicheReport::default();
    let mut found = Vec::new();
    for (p, paragraph) in document.paragraphs.iter().enumerate() {
        let mut stream = Vec::new();
        for (s, sentence) in paragraph.sentences.iter().enumerate() {
            for (t, token) in sentence.tokens.iter().enumerate() {
                if token.is_word && !token.is_skipped {
                    let counts = if in_dialogue(&document.dialogue, token.span.byte.start) {
                        &mut report.dialogue
                    } else {
                        &mut report.narration
                    };
                    counts.words += 1;
                }
                if token.is_whitespace {
                    continue;
                }
                let parts = hyphen_parts(&token.text);
                for (index, part) in parts.iter().enumerate() {
                    let text = &token.text[part.clone()];
                    let mut item_keys = BTreeSet::from([normalize(text)]);
                    if index + 1 == parts.len() {
                        item_keys.insert(normalize(&token.text));
                        item_keys.extend(
                            keys.get(&(p, s, t))
                                .into_iter()
                                .flatten()
                                .map(|lemma| normalize(lemma)),
                        );
                    }
                    let span = part_span(token, part);
                    if let Some(last) = stream.last_mut().filter(|last: &&mut Item| {
                        last.surface == "'" && last.span.byte.end == span.byte.start
                    }) {
                        if normalize(text) == "s" {
                            last.span = span_between(&last.span, &span);
                            last.span_text.push_str(text);
                            last.surface = "'s".into();
                            last.is_word = true;
                            last.keys = BTreeSet::from(["'s".to_owned()]);
                            continue;
                        }
                    }
                    stream.push(Item {
                        surface: normalize(text),
                        span_text: text.to_owned(),
                        span,
                        is_word: token.is_word,
                        is_skipped: token.is_skipped,
                        keys: item_keys,
                    });
                }
            }
        }
        for start in 0..stream.len() {
            let candidates = stream[start]
                .keys
                .iter()
                .filter_map(|key| phrases.by_first_key.get(key))
                .flatten()
                .chain(&phrases.slot_first)
                .collect::<BTreeSet<_>>();
            for &id in candidates {
                let pattern = &phrases.patterns[id];
                let Some(end) = match_pattern(&stream, start, pattern) else {
                    continue;
                };
                let first = start
                    .checked_sub(1)
                    .filter(|&previous| {
                        pattern.optional_article
                            && DETERMINERS
                                .iter()
                                .chain(POSSESSIVES)
                                .any(|word| stream[previous].surface == *word)
                    })
                    .unwrap_or(start);
                found.push((
                    p,
                    pattern.entry,
                    span_between(&stream[first].span, &stream[end].span),
                ));
            }
        }
    }
    found.sort_by(|a, b| {
        a.2.byte
            .start
            .cmp(&b.2.byte.start)
            .then(b.2.byte.end.cmp(&a.2.byte.end))
            .then(a.1.cmp(&b.1))
    });
    let mut kept_end = 0;
    for (paragraph, entry, span) in found {
        if span.byte.end <= kept_end {
            continue;
        }
        kept_end = span.byte.end;
        let entry = &phrases.entries[entry];
        let context = if in_dialogue(&document.dialogue, span.byte.start) {
            PhraseContext::Dialogue
        } else {
            PhraseContext::Narration
        };
        let counts = match context {
            PhraseContext::Narration => &mut report.narration,
            PhraseContext::Dialogue => &mut report.dialogue,
        };
        match entry.kind {
            PhraseKind::Cliche => counts.cliches += 1,
            PhraseKind::Redundancy => counts.redundancies += 1,
        }
        report.matches.push(PhraseMatch {
            kind: entry.kind,
            context,
            phrase: entry.phrase.clone(),
            source: entry.source.clone(),
            span,
            paragraph,
        });
    }
    report
}
