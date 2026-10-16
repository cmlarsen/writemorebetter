use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use nlprule::{analysis::AnalysisSpan, Tokenizer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Span {
    pub byte: Range<usize>,
    pub char: Range<usize>,
    pub utf16: Range<usize>,
}

thread_local! {
    static UTF16_SPANS: Cell<bool> = const { Cell::new(false) };
}

// While set, every Span serializes as its UTF-16 range alone; see reports::to_utf16_json.
pub(crate) fn with_utf16_spans<T>(serialize: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            UTF16_SPANS.with(|flag| flag.set(self.0));
        }
    }
    let _reset = Reset(UTF16_SPANS.with(|flag| flag.replace(true)));
    serialize()
}

impl Serialize for Span {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        if UTF16_SPANS.with(Cell::get) {
            return self.utf16.serialize(serializer);
        }
        let mut span = serializer.serialize_struct("Span", 3)?;
        span.serialize_field("byte", &self.byte)?;
        span.serialize_field("char", &self.char)?;
        span.serialize_field("utf16", &self.utf16)?;
        span.end()
    }
}

impl Span {
    pub fn covering(first: &Span, last: &Span) -> Span {
        Span {
            byte: first.byte.start..last.byte.end,
            char: first.char.start..last.char.end,
            utf16: first.utf16.start..last.utf16.end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Narration,
    Dialogue,
}

impl From<AnalysisSpan> for Span {
    fn from(span: AnalysisSpan) -> Self {
        Self {
            byte: span.byte,
            char: span.char,
            utf16: span.utf16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct DocumentOptions {
    #[serde(alias = "skipNames")]
    pub skip_names: Vec<String>,
    #[serde(alias = "includeTokenDetails")]
    pub include_token_details: bool,
    #[serde(alias = "hardWrapped")]
    pub hard_wrapped: bool,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            skip_names: Vec::new(),
            include_token_details: true,
            hard_wrapped: false,
        }
    }
}

impl DocumentOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_skip_names(mut self, names: Vec<String>) -> Self {
        self.skip_names = names;
        self
    }
    pub fn with_token_details(mut self, include: bool) -> Self {
        self.include_token_details = include;
        self
    }
    pub fn with_hard_wrapped(mut self, hard_wrapped: bool) -> Self {
        self.hard_wrapped = hard_wrapped;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenPosition {
    pub paragraph: usize,
    pub sentence: usize,
    pub token: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Token {
    pub text: String,
    pub span: Span,
    pub readings: Vec<nlprule::analysis::AnalysisReading>,
    pub chunks: Vec<String>,
    pub is_whitespace: bool,
    pub is_punctuation: bool,
    pub is_word: bool,
    pub is_skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Sentence {
    pub text: String,
    pub span: Span,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Paragraph {
    pub text: String,
    pub span: Span,
    pub sentences: Vec<Sentence>,
}

impl Sentence {
    pub fn words(&self) -> impl Iterator<Item = &Token> {
        self.tokens
            .iter()
            .filter(|token| token.is_word && !token.is_skipped)
    }

    pub fn word_count(&self) -> usize {
        self.words().count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Document {
    pub paragraphs: Vec<Paragraph>,
    pub dialogue: Vec<Span>,
    #[serde(default)]
    pub speech: Vec<bool>,
    pub lemma_index: BTreeMap<String, Vec<TokenPosition>>,
    pub word_count: usize,
    #[serde(default = "token_details_default")]
    pub token_details: bool,
    #[serde(default)]
    pub hard_wrapped: bool,
}

fn token_details_default() -> bool {
    true
}

impl Document {
    pub fn sentences(&self) -> impl Iterator<Item = &Sentence> {
        self.paragraphs
            .iter()
            .flat_map(|paragraph| &paragraph.sentences)
    }

    pub fn words(&self) -> impl Iterator<Item = &Token> {
        self.sentences().flat_map(Sentence::words)
    }

    pub fn is_speech(&self, dialogue: usize) -> bool {
        self.speech.get(dialogue).copied().unwrap_or(true)
    }

    pub fn mode_at(&self, byte: usize) -> Mode {
        let index = self.dialogue.partition_point(|span| span.byte.end <= byte);
        if self
            .dialogue
            .get(index)
            .is_some_and(|span| span.byte.start <= byte && self.is_speech(index))
        {
            Mode::Dialogue
        } else {
            Mode::Narration
        }
    }

    pub fn sentence_mode(&self, sentence: &Sentence) -> Mode {
        sentence
            .tokens
            .iter()
            .find(|token| token.is_word)
            .map_or(Mode::Narration, |first| self.mode_at(first.span.byte.start))
    }
}

pub fn is_word(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn name_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('’', "'")
}

fn name_forms(names: &[String]) -> BTreeSet<String> {
    let mut forms = BTreeSet::new();
    for name in names {
        let name = name_key(name);
        if name.is_empty() {
            continue;
        }
        forms.insert(name.to_uppercase());
        if let Some(first) = name.chars().next().filter(|ch| ch.is_lowercase()) {
            forms.insert(
                first.to_uppercase().collect::<String>() + name.get(first.len_utf8()..).unwrap(),
            );
        }
        forms.insert(name);
    }
    forms
}

fn skipped_tokens(
    tokens: &[nlprule::analysis::AnalysisToken],
    names: &BTreeSet<String>,
) -> Vec<bool> {
    let mut skipped = vec![false; tokens.len()];
    let Some(max_length) = names.iter().map(String::len).max() else {
        return skipped;
    };
    for (start, token) in tokens.iter().enumerate() {
        if token.is_whitespace {
            continue;
        }
        let mut surface = String::new();
        for (end, token) in tokens.iter().enumerate().skip(start) {
            surface.push_str(&token.text);
            let key = name_key(&surface);
            if key.len() > max_length {
                break;
            }
            if !token.is_whitespace && names.contains(&key) {
                skipped[start..=end].fill(true);
                if let Some(clitic) = tokens.get(end + 1) {
                    if clitic.span.byte.start == token.span.byte.end
                        && matches!(name_key(&clitic.text).as_str(), "'s" | "'S" | "'")
                    {
                        skipped[end + 1] = true;
                    }
                }
            }
        }
    }
    skipped
}

fn shift(span: AnalysisSpan, base: &Span) -> Span {
    Span {
        byte: span.byte.start + base.byte.start..span.byte.end + base.byte.start,
        char: span.char.start + base.char.start..span.char.end + base.char.start,
        utf16: span.utf16.start + base.utf16.start..span.utf16.end + base.utf16.start,
    }
}

pub fn paragraph_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let (mut byte_start, mut char_start, mut utf16_start) = (0, 0, 0);
    let (mut scalar, mut utf16) = (0, 0);
    for (byte, ch) in text.char_indices() {
        if matches!(ch, '\r' | '\n' | '\u{2029}') {
            if byte_start < byte && !text.get(byte_start..byte).unwrap().trim().is_empty() {
                spans.push(Span {
                    byte: byte_start..byte,
                    char: char_start..scalar,
                    utf16: utf16_start..utf16,
                });
            }
            byte_start = byte + ch.len_utf8();
            char_start = scalar + 1;
            utf16_start = utf16 + ch.len_utf16();
        }
        scalar += 1;
        utf16 += ch.len_utf16();
    }
    if byte_start < text.len() && !text.get(byte_start..).unwrap().trim().is_empty() {
        spans.push(Span {
            byte: byte_start..text.len(),
            char: char_start..scalar,
            utf16: utf16_start..utf16,
        });
    }
    spans
}

fn multiword_end(tokens: &[nlprule::analysis::AnalysisToken], start: usize, lemma: &str) -> usize {
    let normalized_lemma = name_key(lemma).to_lowercase();
    for (offset, _) in normalized_lemma.char_indices().filter(|(offset, _)| {
        *offset == 0 || normalized_lemma.get(..*offset).unwrap().ends_with(' ')
    }) {
        let suffix = normalized_lemma.get(offset..).unwrap();
        let mut surface = String::new();
        for (end, token) in tokens.iter().enumerate().skip(start) {
            surface.push_str(&token.text);
            let normalized = name_key(&surface).to_lowercase();
            if normalized == suffix {
                return end;
            }
            if !suffix.starts_with(&normalized) {
                break;
            }
        }
    }
    tokens
        .iter()
        .enumerate()
        .skip(start + 1)
        .take_while(|(_, token)| {
            token.is_whitespace || token.readings.iter().any(|reading| reading.lemma == lemma)
        })
        .last()
        .map_or(start, |(end, _)| end)
}

#[derive(Default)]
struct SpeechEvidence {
    tokens: bool,
    embedded: bool,
    after_lead_in: bool,
    copula_subject: bool,
}

fn speech_evidence(paragraph: &Paragraph, dialogue: &[Span], evidence: &mut [SpeechEvidence]) {
    use crate::reports::dialogue_tags::{lead_in, starts_copula};
    let items: Vec<&Token> = paragraph
        .sentences
        .iter()
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| !token.is_whitespace)
        .collect();
    let range = &paragraph.span.byte;
    let first = dialogue.partition_point(|span| span.byte.end <= range.start);
    for (index, span) in dialogue
        .iter()
        .enumerate()
        .skip(first)
        .take_while(|(_, span)| span.byte.start < range.end)
    {
        let start = items.partition_point(|token| token.span.byte.start < span.byte.start);
        let end = items.partition_point(|token| token.span.byte.end <= span.byte.end);
        if start >= end {
            continue;
        }
        let found = &mut evidence[index];
        found.tokens = true;
        if span.byte.start >= range.start {
            let previous = start.checked_sub(1).map(|previous| items[previous]);
            found.embedded =
                previous.is_some_and(|previous| previous.is_word && !lead_in(previous));
            found.after_lead_in = previous.is_some_and(lead_in);
        }
        if span.byte.end <= range.end {
            found.copula_subject = items.get(end).is_some_and(|next| starts_copula(next));
        }
    }
}

pub fn analyze_document(tokenizer: &Tokenizer, text: &str, options: &DocumentOptions) -> Document {
    let skipped = name_forms(&options.skip_names);
    let paragraphs = paragraph_spans_mode(text, options.hard_wrapped);
    let dialogue = detect_dialogue(text, &paragraphs);
    let mut evidence: Vec<SpeechEvidence> =
        dialogue.iter().map(|_| SpeechEvidence::default()).collect();
    let mut document = Document {
        paragraphs: Vec::new(),
        dialogue,
        speech: Vec::new(),
        lemma_index: BTreeMap::new(),
        word_count: 0,
        token_details: options.include_token_details,
        hard_wrapped: options.hard_wrapped,
    };
    for span in paragraphs {
        let paragraph_text = text.get(span.byte.clone()).unwrap();
        let paragraph_index = document.paragraphs.len();
        let mut sentences = Vec::new();
        for (sentence_index, sentence) in tokenizer.analyze(paragraph_text).into_iter().enumerate()
        {
            let mut tokens = Vec::new();
            let skipped = skipped_tokens(&sentence.tokens, &skipped);
            let mut multiword_ends = BTreeMap::new();
            let mut indexed_lemmas = Vec::new();
            for (token_index, token) in sentence.tokens.iter().enumerate() {
                if !is_word(&token.text) || skipped[token_index] {
                    indexed_lemmas.push(BTreeSet::new());
                    continue;
                }
                let mut lemmas: BTreeSet<_> = token
                    .readings
                    .iter()
                    .map(|reading| reading.lemma.clone())
                    .collect();
                if lemmas
                    .iter()
                    .all(|lemma| lemma.contains(char::is_whitespace))
                {
                    lemmas.insert(token.text.to_lowercase());
                }
                lemmas.retain(|lemma| {
                    if !lemma.contains(char::is_whitespace) {
                        return true;
                    }
                    if multiword_ends
                        .get(lemma)
                        .is_some_and(|end| token_index <= *end)
                    {
                        return false;
                    }
                    multiword_ends.insert(
                        lemma.clone(),
                        multiword_end(&sentence.tokens, token_index, lemma),
                    );
                    true
                });
                indexed_lemmas.push(lemmas);
            }
            for (token_index, token) in sentence.tokens.into_iter().enumerate() {
                let word = is_word(&token.text);
                let is_skipped = skipped[token_index];
                if word && !is_skipped {
                    document.word_count += 1;
                    for lemma in &indexed_lemmas[token_index] {
                        document
                            .lemma_index
                            .entry(lemma.to_owned())
                            .or_default()
                            .push(TokenPosition {
                                paragraph: paragraph_index,
                                sentence: sentence_index,
                                token: token_index,
                            });
                    }
                }
                tokens.push(Token {
                    text: token.text,
                    span: shift(token.span, &span),
                    readings: token.readings,
                    chunks: token.chunks,
                    is_whitespace: token.is_whitespace,
                    is_punctuation: token.is_punctuation,
                    is_word: word,
                    is_skipped,
                });
            }
            sentences.push(Sentence {
                text: sentence.text,
                span: shift(sentence.span, &span),
                tokens,
            });
        }
        let mut paragraph = Paragraph {
            text: paragraph_text.to_owned(),
            span,
            sentences,
        };
        speech_evidence(&paragraph, &document.dialogue, &mut evidence);
        if !options.include_token_details {
            for token in paragraph
                .sentences
                .iter_mut()
                .flat_map(|sentence| &mut sentence.tokens)
            {
                token.readings = Vec::new();
                token.chunks = Vec::new();
            }
        }
        document.paragraphs.push(paragraph);
    }
    document.speech = evidence
        .iter()
        .map(|found| {
            found.tokens && !found.embedded && !(found.copula_subject && !found.after_lead_in)
        })
        .collect();
    document
}

fn paragraph_spans_mode(text: &str, hard_wrapped: bool) -> Vec<Span> {
    let lines = paragraph_spans(text);
    if !hard_wrapped {
        return lines;
    }
    let mut paragraphs: Vec<Span> = Vec::new();
    for line in lines {
        if let Some(previous) = paragraphs.last_mut() {
            let gap = text.get(previous.byte.end..line.byte.start).unwrap();
            let breaks =
                gap.matches('\n').count() + gap.matches('\r').count() - gap.matches("\r\n").count();
            if breaks < 2 && !gap.contains('\u{2029}') {
                previous.byte.end = line.byte.end;
                previous.char.end = line.char.end;
                previous.utf16.end = line.utf16.end;
                continue;
            }
        }
        paragraphs.push(line);
    }
    paragraphs
}

pub fn dialogue_spans(text: &str) -> Vec<Span> {
    dialogue_spans_with_options(text, &DocumentOptions::new())
}

pub fn dialogue_spans_with_options(text: &str, options: &DocumentOptions) -> Vec<Span> {
    detect_dialogue(text, &paragraph_spans_mode(text, options.hard_wrapped))
}

fn leading_elision(after: &str) -> bool {
    let word = after
        .chars()
        .take_while(|ch| ch.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    matches!(
        word.as_str(),
        "em" | "tis"
            | "twas"
            | "twere"
            | "twill"
            | "twould"
            | "n"
            | "cause"
            | "bout"
            | "round"
            | "til"
            | "till"
            | "ere"
            | "neath"
            | "nother"
            | "cept"
            | "scuse"
            | "twenties"
            | "thirties"
            | "forties"
            | "fifties"
            | "sixties"
            | "seventies"
            | "eighties"
            | "nineties"
    ) || word
        .strip_suffix('s')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}

fn detect_dialogue(text: &str, paragraphs: &[Span]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut open: Option<(char, usize, usize, usize, bool)> = None;
    let mut outer = None;
    for (paragraph_index, paragraph) in paragraphs.iter().enumerate() {
        let surface = text.get(paragraph.byte.clone()).unwrap();
        let chars: Vec<_> = surface.char_indices().collect();
        let mut utf16 = paragraph.utf16.start;
        let mut at_start = true;
        for (index, &(local_byte, ch)) in chars.iter().enumerate() {
            let byte = paragraph.byte.start + local_byte;
            let scalar = paragraph.char.start + index;
            let previous = index.checked_sub(1).map(|i| chars[i].1);
            let next = chars.get(index + 1).map(|(_, ch)| *ch);
            let single = matches!(ch, '\'' | '‘' | '’');
            let apostrophe = single
                && (previous.is_some_and(char::is_alphanumeric)
                    && (next.is_some_and(char::is_alphanumeric)
                        || open
                            .as_ref()
                            .is_none_or(|(kind, ..)| *kind != '\'' && *kind != '‘')));
            if !apostrophe {
                let opener = matches!(ch, '“' | '"' | '‘' | '\'')
                    && (!single
                        || (!previous.is_some_and(char::is_alphanumeric)
                            && next.is_some_and(char::is_alphanumeric)));
                if opener
                    && open.is_none_or(|(kind, _, _, _, tentative)| {
                        tentative
                            && (!single
                                || (ch == kind
                                    && !leading_elision(
                                        surface.get(local_byte + ch.len_utf8()..).unwrap(),
                                    )))
                    })
                {
                    let tentative = single
                        && (ch == '\''
                            || leading_elision(surface.get(local_byte + ch.len_utf8()..).unwrap()));
                    if !single && open.is_some() {
                        outer = open;
                    }
                    open = Some((ch, byte, scalar, utf16, tentative));
                } else if let Some((kind, start_byte, start_char, start_utf16, _)) = open {
                    let closer = match kind {
                        '“' => '”',
                        '‘' => '’',
                        other => other,
                    };
                    if !(ch == kind && at_start)
                        && ch == closer
                        && (!single
                            || previous.is_some_and(|ch| {
                                matches!(ch, '.' | '!' | '?' | ',' | ':' | ';' | '—' | '…')
                            }))
                    {
                        while spans
                            .last()
                            .is_some_and(|span: &Span| span.byte.start > start_byte)
                        {
                            spans.pop();
                        }
                        spans.push(Span {
                            byte: start_byte..byte + ch.len_utf8(),
                            char: start_char..scalar + 1,
                            utf16: start_utf16..utf16 + ch.len_utf16(),
                        });
                        open = outer.take();
                    }
                }
            }
            if !ch.is_whitespace() {
                at_start = false;
            }
            utf16 += ch.len_utf16();
        }
        outer = None;
        if let Some((kind, _, _, _, tentative)) = open {
            let continued = !tentative
                && paragraphs
                    .get(paragraph_index + 1)
                    .and_then(|next| text.get(next.byte.clone()))
                    .is_some_and(|next| {
                        let next = next.trim_start();
                        next.starts_with(kind)
                            && (!matches!(kind, '\'' | '‘')
                                || !leading_elision(next.get(kind.len_utf8()..).unwrap()))
                    });
            if !continued {
                open = None;
            }
        }
    }
    spans
}
