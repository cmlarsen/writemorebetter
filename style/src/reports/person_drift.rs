//! Person drift (#354): isolated slips in grammatical person, scene by scene. A first-person *I*
//! in a confidently third-person scene (`firstInThird`), or the declared narrator's *she*/*he*
//! (`thirdInFirst`) or name (`narratorName`) in a confidently first-person scene. Opt-in,
//! narration-only and diagnostic: it never proposes replacement text. See
//! docs/STYLE-PERSON-DRIFT.md.
//!
//! The scene person comes from threshold rules over the persons of finite-chain subjects; there is
//! no pronoun feature vector and no classifier. The gendered-noun, interjection and self-distancing
//! lists below are English and written for this module (MIT/Apache-2.0).

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::document::honorific;
use crate::narrative::{
    chains, display_colon_lead_in, paragraph_quotes_balanced, pronoun_person, scene_tense, scenes,
    Case, Chain, ChainClass, NarrativePerson, NarrativeTense,
};
use crate::reports::echoes::fold;
use crate::{Document, Sentence, Span, StyleError, Token, Voice};

pub(crate) const REPORT: &str = "personDrift";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PersonPronoun {
    She,
    He,
    They,
}

impl PersonPronoun {
    fn subject(self) -> Option<&'static str> {
        match self {
            Self::She => Some("she"),
            Self::He => Some("he"),
            Self::They => None,
        }
    }

    fn forms(self) -> &'static [&'static str] {
        match self {
            Self::She => &["she", "her", "hers", "herself"],
            Self::He => &["he", "him", "his", "himself"],
            Self::They => &[],
        }
    }

    fn nouns(self) -> &'static [&'static str] {
        match self {
            Self::She => &FEMININE_NOUNS,
            Self::He => &MASCULINE_NOUNS,
            Self::They => &[],
        }
    }
}

/// The first-person narrator: the names and pronoun the narrator would have in third person.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Narrator {
    pub names: Vec<String>,
    pub pronoun: Option<PersonPronoun>,
}

impl Narrator {
    pub fn new(names: Vec<String>, pronoun: Option<PersonPronoun>) -> Self {
        Self { names, pronoun }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CastMember {
    pub name: String,
    pub pronoun: PersonPronoun,
}

impl CastMember {
    pub fn new(name: impl Into<String>, pronoun: PersonPronoun) -> Self {
        Self {
            name: name.into(),
            pronoun,
        }
    }
}

/// A per-scene declaration; `scene` is the index in `PersonDriftReport::scenes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ScenePerson {
    pub scene: usize,
    pub person: NarrativePerson,
}

impl ScenePerson {
    pub fn new(scene: usize, person: NarrativePerson) -> Self {
        Self { scene, person }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct PersonDriftOptions {
    /// The document's person. A `scene_persons` entry beats it.
    pub person: Option<NarrativePerson>,
    #[serde(alias = "scenePersons")]
    pub scene_persons: Vec<ScenePerson>,
    /// Enables `thirdInFirst` and `narratorName`.
    pub narrator: Option<Narrator>,
    pub cast: Vec<CastMember>,
    /// Person-bearing subjects a scene needs before its person is inferred.
    #[serde(alias = "minimumSubjects")]
    pub minimum_subjects: usize,
    /// Slips a scene may hold; more is a voice choice and suppresses the scene.
    #[serde(alias = "maximumSlips")]
    pub maximum_slips: usize,
    /// Flags inferred third-person scenes in a document that also has a first-person scene.
    #[serde(alias = "includeWitnessScenes")]
    pub include_witness_scenes: bool,
}

impl Default for PersonDriftOptions {
    fn default() -> Self {
        Self {
            person: None,
            scene_persons: Vec::new(),
            narrator: None,
            cast: Vec::new(),
            minimum_subjects: 8,
            maximum_slips: 2,
            include_witness_scenes: false,
        }
    }
}

impl PersonDriftOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_person(mut self, person: Option<NarrativePerson>) -> Self {
        self.person = person;
        self
    }
    pub fn with_scene_persons(mut self, scenes: Vec<ScenePerson>) -> Self {
        self.scene_persons = scenes;
        self
    }
    pub fn with_narrator(mut self, narrator: Option<Narrator>) -> Self {
        self.narrator = narrator;
        self
    }
    pub fn with_cast(mut self, cast: Vec<CastMember>) -> Self {
        self.cast = cast;
        self
    }
    pub fn with_minimum_subjects(mut self, minimum: usize) -> Self {
        self.minimum_subjects = minimum;
        self
    }
    pub fn with_maximum_slips(mut self, maximum: usize) -> Self {
        self.maximum_slips = maximum;
        self
    }
    pub fn with_include_witness_scenes(mut self, include: bool) -> Self {
        self.include_witness_scenes = include;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PersonSource {
    Declared,
    Inferred,
    /// Enough subjects, but in the dead band between first and third person.
    Mixed,
    /// Fewer than `minimum_subjects` subjects.
    Undetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PersonDriftKind {
    FirstInThird,
    ThirdInFirst,
    NarratorName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PersonSuppression {
    /// An inferred third-person scene in a document with a first-person scene.
    WitnessNarrator,
    /// More than `maximum_slips` candidate sentences.
    TooManySlips,
    /// A first-person scene with candidates but no declared narrator.
    NoNarrator,
    /// The scene tense could not be decided, so the tense-agreement check cannot run.
    UndeterminedTense,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PersonSubjects {
    pub first: usize,
    pub second: usize,
    pub third: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PersonScene {
    pub scene: usize,
    pub span: Span,
    pub paragraphs: Range<usize>,
    /// Global sentence indexes.
    pub sentences: Range<usize>,
    pub person: Option<NarrativePerson>,
    pub source: PersonSource,
    /// The scene's narrative tense from the shared narrative helpers; tense declarations do not
    /// apply here.
    pub tense: Option<NarrativeTense>,
    pub subjects: PersonSubjects,
    pub narration_words: usize,
    pub suppressed: Option<PersonSuppression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PersonDrift {
    pub span: Span,
    pub sentence_span: Span,
    /// Global sentence index.
    pub sentence: usize,
    pub scene: usize,
    pub kind: PersonDriftKind,
    /// The folded surface (*i*, *my*, *she*), or the matched narrator name as declared.
    pub word: String,
    pub expected: NarrativePerson,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PersonDriftReport {
    pub scenes: Vec<PersonScene>,
    pub drifts: Vec<PersonDrift>,
    pub narration_words: usize,
    pub per_thousand_words: Option<f64>,
}

const FIRST_SINGULAR: [&str; 5] = ["i", "me", "my", "mine", "myself"];

/// Words after *my* that make an interjection: *My God*, *my goodness*.
const MY_INTERJECTIONS: [&str; 7] = ["god", "lord", "goodness", "word", "dear", "heavens", "gosh"];

/// Nouns that give a feminine antecedent. *ship*, *boat*, *vessel* and *car* take *she* too.
const FEMININE_NOUNS: [&str; 60] = [
    "woman",
    "girl",
    "lady",
    "mother",
    "sister",
    "daughter",
    "wife",
    "aunt",
    "niece",
    "queen",
    "princess",
    "widow",
    "bride",
    "nun",
    "maid",
    "maiden",
    "mistress",
    "grandmother",
    "grandma",
    "granny",
    "mom",
    "mum",
    "mama",
    "mamma",
    "mommy",
    "mummy",
    "ma",
    "madam",
    "madame",
    "duchess",
    "countess",
    "baroness",
    "empress",
    "hostess",
    "waitress",
    "actress",
    "landlady",
    "girlfriend",
    "fiancée",
    "fiancee",
    "heroine",
    "goddess",
    "witch",
    "stepmother",
    "governess",
    "housekeeper",
    "lass",
    "mare",
    "hen",
    "cow",
    "sow",
    "ewe",
    "ship",
    "boat",
    "vessel",
    "car",
    "yacht",
    "schooner",
    "steamer",
    "frigate",
];

const MASCULINE_NOUNS: [&str; 48] = [
    "man",
    "boy",
    "gentleman",
    "father",
    "brother",
    "son",
    "husband",
    "uncle",
    "nephew",
    "king",
    "prince",
    "widower",
    "groom",
    "bridegroom",
    "monk",
    "priest",
    "grandfather",
    "grandpa",
    "granddad",
    "dad",
    "daddy",
    "papa",
    "pa",
    "sir",
    "lord",
    "duke",
    "earl",
    "baron",
    "emperor",
    "host",
    "waiter",
    "actor",
    "landlord",
    "boyfriend",
    "fiancé",
    "fiance",
    "hero",
    "god",
    "wizard",
    "stepfather",
    "lad",
    "fellow",
    "chap",
    "stallion",
    "bull",
    "ram",
    "boar",
    "rooster",
];

/// Frames in which a first-person narrator looks at an earlier self in third person.
const SELF_FRAMES: [&str; 6] = [
    "my younger self",
    "my old self",
    "my former self",
    "that girl",
    "that boy",
    "my other self",
];
const SELF_FRAME_NOUNS: [&str; 6] = ["girl", "boy", "woman", "man", "child", "kid"];

/// Words before a narrator name that name rather than refer: *a girl named Anna*, *Dear Anna*.
const NAMING: [&str; 6] = ["named", "called", "name", "signed", "wrote", "dear"];

struct Line<'a> {
    paragraph: usize,
    /// The sentence's index within its paragraph.
    local: usize,
    sentence: &'a Sentence,
    /// Non-whitespace tokens of each narration run.
    runs: Vec<Vec<&'a Token>>,
    chains: Vec<Vec<Chain>>,
}

impl Line<'_> {
    fn chains(&self) -> impl Iterator<Item = (usize, &Chain)> {
        self.chains
            .iter()
            .enumerate()
            .flat_map(|(run, chains)| chains.iter().map(move |chain| (run, chain)))
    }

    /// Subject persons of the sentence's narration chains.
    fn subjects(&self) -> impl Iterator<Item = (NarrativePerson, &Token)> + '_ {
        self.chains().filter_map(|(run, chain)| {
            let subject = chain.subject?;
            match pronoun_person(&self.runs[run], subject) {
                Some((person, Case::Subject)) => Some((person, self.runs[run][subject])),
                _ => None,
            }
        })
    }

    fn first_subject(&self) -> bool {
        self.subjects()
            .any(|(person, _)| person == NarrativePerson::First)
    }
}

fn lines(document: &Document) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for (paragraph_index, paragraph) in document.paragraphs.iter().enumerate() {
        for (local, sentence) in paragraph.sentences.iter().enumerate() {
            let runs: Vec<Vec<&Token>> = document
                .narration_runs(sentence)
                .into_iter()
                .map(|run| {
                    sentence.tokens[run]
                        .iter()
                        .filter(|token| !token.is_whitespace)
                        .collect()
                })
                .collect();
            let chains = runs.iter().map(|run| chains(run)).collect();
            lines.push(Line {
                paragraph: paragraph_index,
                local,
                sentence,
                runs,
                chains,
            });
        }
    }
    lines
}

fn validate(options: &PersonDriftOptions, scene_count: usize) -> Result<(), StyleError> {
    if options.minimum_subjects == 0 {
        return Err(StyleError::options(format!(
            "{REPORT}: minimumSubjects must be at least 1"
        )));
    }
    if let Some(entry) = options
        .scene_persons
        .iter()
        .find(|entry| entry.scene >= scene_count)
    {
        return Err(StyleError::options(format!(
            "{REPORT}: scenePersons names scene {} but the document has {scene_count} scene(s)",
            entry.scene
        )));
    }
    let empty = |name: &String| name.trim().is_empty();
    if options
        .narrator
        .as_ref()
        .is_some_and(|narrator| narrator.names.iter().any(empty))
    {
        return Err(StyleError::options(format!(
            "{REPORT}: narrator names must not be empty"
        )));
    }
    if options.cast.iter().any(|member| empty(&member.name)) {
        return Err(StyleError::options(format!(
            "{REPORT}: cast names must not be empty"
        )));
    }
    Ok(())
}

fn name_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('’', "'")
}

fn name_words(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split_whitespace()
        .map(|word| word.replace('’', "'").to_lowercase())
}

/// The scene person from subject counts, per the dead-band rule.
fn inferred(
    subjects: PersonSubjects,
    we: usize,
    options: &PersonDriftOptions,
) -> (Option<NarrativePerson>, PersonSource) {
    let PersonSubjects {
        first,
        second,
        third,
    } = subjects;
    let total = first + second + third;
    if total < options.minimum_subjects {
        return (None, PersonSource::Undetermined);
    }
    if 2 * second >= total && second >= 4 {
        return (Some(NarrativePerson::Second), PersonSource::Inferred);
    }
    if first >= 3 && 4 * first >= first + third && we < first {
        return (Some(NarrativePerson::First), PersonSource::Inferred);
    }
    if third >= options.minimum_subjects && first <= options.maximum_slips && 10 * first <= third {
        return (Some(NarrativePerson::Third), PersonSource::Inferred);
    }
    (None, PersonSource::Mixed)
}

/// E2: a declarative sentence ending in a full stop, optionally inside a closing quote, with no
/// question or exclamation mark. Endings in an ellipsis, a dash or a colon trail off.
fn declarative(text: &str) -> bool {
    if text.contains(['?', '!', '‽']) {
        return false;
    }
    let trimmed = text.trim_end_matches(|ch: char| ch.is_whitespace() || "”’\"'»)]".contains(ch));
    trimmed.ends_with('.') && !trimmed.ends_with("..") && !trimmed.ends_with(". .")
}

/// E2 inside a sentence the splitter did not break: the candidate's clause ends in a dash, a
/// colon or an ellipsis followed by a capitalized word (*I crossed the room— He was gone.*).
fn trails_off(text: &str, offset: usize) -> bool {
    for (byte, ch) in text.char_indices().skip_while(|&(byte, _)| byte < offset) {
        if !matches!(ch, '—' | '–' | '―' | ':' | '…') {
            continue;
        }
        let rest = &text[byte + ch.len_utf8()..];
        let after = rest.trim_start_matches(|ch: char| ch.is_whitespace() || "“\"‘'".contains(ch));
        if after.len() < rest.len() && after.chars().next().is_some_and(char::is_uppercase) {
            return true;
        }
    }
    false
}

/// E4: the byte offset (relative to `text`) sits inside parentheses, brackets, a dash pair, or
/// after a single dash that runs to the end of the sentence.
fn bracketed(text: &str, offset: usize) -> bool {
    let mut parentheses = 0i32;
    let mut brackets = 0i32;
    let mut dashes = 0;
    let mut previous: Option<char> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((byte, ch)) = chars.next() {
        if byte >= offset {
            break;
        }
        let next = chars.peek().map(|&(_, next)| next);
        match ch {
            '(' => parentheses += 1,
            ')' => parentheses = (parentheses - 1).max(0),
            '[' => brackets += 1,
            ']' => brackets = (brackets - 1).max(0),
            '—' | '―' => dashes += 1,
            // An en dash between digits is a range (*1914–1918*).
            '–' if !(previous.is_some_and(|p| p.is_ascii_digit())
                && next.is_some_and(|n| n.is_ascii_digit())) =>
            {
                dashes += 1
            }
            '-' if next == Some('-') && previous != Some('-') => dashes += 1,
            _ => {}
        }
        previous = Some(ch);
    }
    parentheses > 0 || brackets > 0 || dashes % 2 == 1
}

/// E5: the first-person word is part of an interjection formula.
fn interjection(words: &[String], at: usize) -> bool {
    let word = words[at].as_str();
    let previous = |back: usize| at.checked_sub(back).map(|index| words[index].as_str());
    let next = words.get(at + 1).map(String::as_str);
    match word {
        "my" => {
            next.is_some_and(|next| MY_INTERJECTIONS.contains(&next)) || previous(1) == Some("oh")
        }
        "me" => {
            matches!(previous(1), Some("dear" | "excuse" | "bless"))
                || (previous(1) == Some("help") && previous(2) == Some("god"))
        }
        _ => false,
    }
}

/// A7: a self-distancing frame anywhere in the paragraph (*the girl I had been*, *my younger
/// self*).
fn self_frame(paragraph: &str) -> bool {
    let words: Vec<String> = paragraph
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '\'' || ch == '’'))
        .filter(|word| !word.is_empty())
        .map(fold)
        .collect();
    let text = words.join(" ");
    if SELF_FRAMES
        .iter()
        .any(|frame| format!(" {text} ").contains(&format!(" {frame} ")))
    {
        return true;
    }
    words.windows(4).any(|window| {
        window[0] == "the"
            && SELF_FRAME_NOUNS.contains(&window[1].as_str())
            && matches!(window[2].as_str(), "i" | "i'd")
            && matches!(window[3].as_str(), "was" | "had" | "been" | "used" | "once")
    }) || words.windows(3).any(|window| {
        window[0] == "the" && SELF_FRAME_NOUNS.contains(&window[1].as_str()) && window[2] == "i'd"
    })
}

/// A capitalized mid-sentence word read only as a proper noun (or not read at all): a possible
/// antecedent the lens cannot resolve.
fn proper_name(token: &Token) -> bool {
    let surface = token.text.replace('’', "'");
    if surface == "I" || surface.starts_with("I'") {
        return false;
    }
    token.text.chars().next().is_some_and(char::is_uppercase)
        && (token.is_skipped
            || token.readings.is_empty()
            || token
                .readings
                .iter()
                .all(|reading| reading.pos.starts_with("NNP")))
}

struct Context<'a> {
    document: &'a Document,
    options: &'a PersonDriftOptions,
    lines: &'a [Line<'a>],
    /// Global sentence index of each paragraph's first sentence.
    paragraph_starts: Vec<usize>,
    /// Byte offset of each paragraph's display-colon lead-in, if any (E6).
    display_from: Vec<Option<usize>>,
    /// E7 and E8, per paragraph.
    paragraph_exempt: Vec<bool>,
    self_frames: Vec<bool>,
}

impl<'a> Context<'a> {
    fn new(document: &'a Document, options: &'a PersonDriftOptions, lines: &'a [Line<'a>]) -> Self {
        let mut paragraph_starts = vec![0; document.paragraphs.len()];
        let mut display_from = vec![None; document.paragraphs.len()];
        let mut chained = vec![false; document.paragraphs.len()];
        for (index, line) in lines.iter().enumerate() {
            if line.local == 0 {
                paragraph_starts[line.paragraph] = index;
            }
            if !line.chains.iter().all(Vec::is_empty) {
                chained[line.paragraph] = true;
            }
            if display_from[line.paragraph].is_none() {
                display_from[line.paragraph] = line.runs.iter().find_map(|run| {
                    display_colon_lead_in(run).map(|colon| run[colon].span.byte.end)
                });
            }
        }
        let paragraph_exempt = document
            .paragraphs
            .iter()
            .enumerate()
            .map(|(index, paragraph)| {
                let text = paragraph.text.trim_start();
                let dash_dialogue = text.starts_with(['—', '–', '―']) || text.starts_with("--");
                let words: usize = paragraph.sentences.iter().map(Sentence::word_count).sum();
                let short_line = words <= 6 && !chained[index] && !paragraph_has_chain(paragraph);
                dash_dialogue || short_line || !paragraph_quotes_balanced(document, index)
            })
            .collect();
        let self_frames = document
            .paragraphs
            .iter()
            .map(|paragraph| self_frame(&paragraph.text))
            .collect();
        Self {
            document,
            options,
            lines,
            paragraph_starts,
            display_from,
            paragraph_exempt,
            self_frames,
        }
    }

    /// E1–E4, E6 (when `display`) and E7–E8 for a candidate token in sentence `line`.
    fn sentence_allows(
        &self,
        line: usize,
        token: &Token,
        tense: Option<NarrativeTense>,
        display: bool,
    ) -> bool {
        let line_ref = &self.lines[line];
        let sentence = line_ref.sentence;
        let byte = token.span.byte.start;
        if !self.document.is_narration_at(byte) || self.document.in_voice_tag(byte) {
            return false;
        }
        if self.paragraph_exempt[line_ref.paragraph] || !declarative(&sentence.text) {
            return false;
        }
        let Some(tense) = tense else {
            return false;
        };
        let mut chains = line_ref.chains().peekable();
        if chains.peek().is_none() {
            return false;
        }
        let agrees = chains.all(|(_, chain)| match tense {
            NarrativeTense::Past => {
                matches!(chain.class, ChainClass::Past | ChainClass::PastPerfect)
            }
            NarrativeTense::Present => chain.class == ChainClass::Present,
        });
        if !agrees {
            return false;
        }
        let offset = byte - sentence.span.byte.start;
        if bracketed(&sentence.text, offset) || trails_off(&sentence.text, offset) {
            return false;
        }
        if display && self.display_from[line_ref.paragraph].is_some_and(|from| from <= byte) {
            return false;
        }
        true
    }
}

fn paragraph_has_chain(paragraph: &crate::Paragraph) -> bool {
    paragraph.sentences.iter().any(|sentence| {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|token| !token.is_whitespace)
            .collect();
        !chains(&tokens).is_empty()
    })
}

/// A narrator-name match in a sentence: token range in `sentence.tokens` (with a possessive clitic
/// when present) and the declared name it matched.
struct NameMatch {
    tokens: Range<usize>,
    last: usize,
    name: String,
}

fn name_matches(sentence: &Sentence, names: &[String]) -> Vec<NameMatch> {
    let forms: Vec<(String, &String)> = names
        .iter()
        .flat_map(|name| {
            let key = name_key(name);
            [(key.to_uppercase(), name), (key, name)]
        })
        .collect();
    let Some(longest) = forms.iter().map(|(form, _)| form.len()).max() else {
        return Vec::new();
    };
    let tokens = &sentence.tokens;
    let mut found = Vec::new();
    let mut start = 0;
    while start < tokens.len() {
        if !tokens[start].is_word {
            start += 1;
            continue;
        }
        let mut surface = String::new();
        let mut best: Option<(usize, &String)> = None;
        for (end, token) in tokens.iter().enumerate().skip(start) {
            surface.push_str(&token.text);
            let key = name_key(&surface);
            if key.len() > longest {
                break;
            }
            if token.is_whitespace {
                continue;
            }
            if let Some((_, name)) = forms.iter().find(|(form, _)| *form == key) {
                // A match must end at a word boundary: *Anna* is not in *Annabel*.
                let boundary = tokens.get(end + 1).is_none_or(|next| {
                    next.span.byte.start > token.span.byte.end || !next.is_word || clitic(next)
                });
                if boundary {
                    best = Some((end, name));
                }
            }
        }
        let Some((end, name)) = best else {
            start += 1;
            continue;
        };
        let possessive = tokens
            .get(end + 1)
            .is_some_and(|next| next.span.byte.start == tokens[end].span.byte.end && clitic(next));
        found.push(NameMatch {
            tokens: start..end + 1 + usize::from(possessive),
            last: end,
            name: name.clone(),
        });
        start = end + 1;
    }
    found
}

fn clitic(token: &Token) -> bool {
    matches!(token.text.replace('’', "'").as_str(), "'s" | "'S" | "'")
}

pub fn report(
    document: &Document,
    options: &PersonDriftOptions,
) -> Result<PersonDriftReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let scene_list = scenes(document);
    validate(options, scene_list.len())?;
    let lines = lines(document);
    let context = Context::new(document, options, &lines);

    // Steps 1–3: subjects, person and tense per scene.
    let mut records: Vec<PersonScene> = Vec::with_capacity(scene_list.len());
    for scene in &scene_list {
        let mut subjects = PersonSubjects::default();
        let mut we = 0;
        let mut narration_words = 0;
        for line in &lines[scene.sentences.clone()] {
            narration_words += document.sentence_narration_words(line.sentence).count();
            for (person, token) in line.subjects() {
                match person {
                    NarrativePerson::First => {
                        subjects.first += 1;
                        we += usize::from(fold(&token.text) == "we");
                    }
                    NarrativePerson::Second => subjects.second += 1,
                    NarrativePerson::Third => subjects.third += 1,
                }
            }
        }
        let declared = options
            .scene_persons
            .iter()
            .rev()
            .find(|entry| entry.scene == scene.index)
            .map(|entry| entry.person)
            .or(options.person);
        let (person, source) = match declared {
            Some(person) => (Some(person), PersonSource::Declared),
            None => inferred(subjects, we, options),
        };
        records.push(PersonScene {
            scene: scene.index,
            span: scene.span.clone(),
            paragraphs: scene.paragraphs.clone(),
            sentences: scene.sentences.clone(),
            person,
            source,
            tense: scene_tense(document, scene),
            subjects,
            narration_words,
            suppressed: None,
        });
    }

    // Step 4: the witness guard.
    let witness = !options.include_witness_scenes
        && records
            .iter()
            .any(|scene| scene.person == Some(NarrativePerson::First));
    let mut drifts = Vec::new();
    for record in &mut records {
        match record.person {
            Some(NarrativePerson::Third) => {
                if witness && record.source == PersonSource::Inferred {
                    record.suppressed = Some(PersonSuppression::WitnessNarrator);
                    continue;
                }
                if record.tense.is_none() {
                    record.suppressed = Some(PersonSuppression::UndeterminedTense);
                    continue;
                }
                first_in_third(&context, record, &mut drifts);
            }
            Some(NarrativePerson::First) => third_in_first(&context, record, &mut drifts),
            _ => {}
        }
    }
    drifts.sort_by_key(|drift: &PersonDrift| (drift.span.byte.start, drift.span.byte.end));
    let narration_words = records.iter().map(|scene| scene.narration_words).sum();
    let per_thousand_words =
        (narration_words > 0).then(|| 1000.0 * drifts.len() as f64 / narration_words as f64);
    Ok(PersonDriftReport {
        scenes: records,
        drifts,
        narration_words,
        per_thousand_words,
    })
}

/// Step 5: first-person singular words in a third-person scene.
fn first_in_third(context: &Context, record: &mut PersonScene, drifts: &mut Vec<PersonDrift>) {
    let document = context.document;
    // One entry per sentence with candidates: its first candidate and the run words it sits in.
    let mut candidates: Vec<(usize, &Token)> = Vec::new();
    for index in record.sentences.clone() {
        let line = &context.lines[index];
        let found = line.runs.iter().find_map(|run| {
            (0..run.len()).find_map(|at| {
                let token = run[at];
                let first = matches!(pronoun_person(run, at), Some((NarrativePerson::First, _)))
                    && FIRST_SINGULAR.contains(&fold(&token.text).as_str());
                (first && !document.in_voice_tag(token.span.byte.start)).then_some(token)
            })
        });
        if let Some(token) = found {
            candidates.push((index, token));
        }
    }
    if candidates.len() > context.options.maximum_slips {
        record.suppressed = Some(PersonSuppression::TooManySlips);
        return;
    }
    for (index, token) in candidates {
        let line = &context.lines[index];
        let allowed = context.sentence_allows(index, token, record.tense, true)
            && !line.runs.iter().any(|run| {
                let words: Vec<String> = run
                    .iter()
                    .filter(|token| token.is_word)
                    .map(|token| fold(&token.text))
                    .collect();
                run.iter()
                    .filter(|token| token.is_word)
                    .position(|other| std::ptr::eq(*other, token))
                    .is_some_and(|at| interjection(&words, at))
            });
        if allowed {
            drifts.push(PersonDrift {
                span: token.span.clone(),
                sentence_span: line.sentence.span.clone(),
                sentence: index,
                scene: record.scene,
                kind: PersonDriftKind::FirstInThird,
                word: fold(&token.text),
                expected: NarrativePerson::Third,
            });
        }
    }
}

/// Step 6: the narrator's third-person pronoun or name in a first-person scene.
fn third_in_first(context: &Context, record: &mut PersonScene, drifts: &mut Vec<PersonDrift>) {
    let document = context.document;
    let options = context.options;
    let lines = &context.lines[record.sentences.clone()];
    let Some(narrator) = options.narrator.as_ref() else {
        let candidates = lines.iter().any(|line| {
            line.subjects().any(|(person, token)| {
                person == NarrativePerson::Third
                    && matches!(fold(&token.text).as_str(), "she" | "he")
            })
        });
        if candidates {
            record.suppressed = Some(PersonSuppression::NoNarrator);
        }
        return;
    };
    let pronoun = narrator.pronoun.and_then(PersonPronoun::subject);
    let gender = narrator.pronoun;
    let cast_words: Vec<(String, PersonPronoun)> = options
        .cast
        .iter()
        .flat_map(|member| name_words(&member.name).map(move |word| (word, member.pronoun)))
        .collect();
    let narrator_words: Vec<String> = narrator
        .names
        .iter()
        .flat_map(|name| name_words(name).collect::<Vec<_>>())
        .collect();

    // Scene-wide antecedent facts (A1–A3).
    let same_subjects = pronoun.map_or(0, |pronoun| {
        lines
            .iter()
            .flat_map(Line::subjects)
            .filter(|(person, token)| {
                *person == NarrativePerson::Third && fold(&token.text) == pronoun
            })
            .count()
    });
    let scene_words: Vec<&Token> = lines
        .iter()
        .flat_map(|line| line.sentence.words())
        .collect();
    let same_cast_mentioned = gender.is_some_and(|gender| {
        let names: Vec<String> = options
            .cast
            .iter()
            .filter(|member| member.pronoun == gender)
            .map(|member| member.name.clone())
            .collect();
        !names.is_empty()
            && lines
                .iter()
                .any(|line| !name_matches(line.sentence, &names).is_empty())
    });
    let voiced_pronoun = gender.is_some_and(|gender| {
        scene_words.iter().any(|token| {
            matches!(
                document.voice_at(token.span.byte.start),
                Voice::Speech | Voice::Quoted | Voice::Thought
            ) && gender.forms().contains(&fold(&token.text).as_str())
        })
    });

    let mut had_candidates = false;
    for (offset, line) in lines.iter().enumerate() {
        let index = record.sentences.start + offset;
        let sentence = line.sentence;
        // narratorName first: it wins when both hit.
        let mut drift: Option<(Span, String, PersonDriftKind)> = None;
        for found in name_matches(sentence, &narrator.names) {
            let first = &sentence.tokens[found.tokens.start];
            let last = &sentence.tokens[found.last];
            let in_narration = sentence.tokens[found.tokens.clone()].iter().all(|token| {
                token.is_whitespace
                    || (document.is_narration_at(token.span.byte.start)
                        && !document.in_voice_tag(token.span.byte.start))
            });
            if !in_narration {
                continue;
            }
            let possessive = found.tokens.end > found.last + 1;
            let subject = line.chains().any(|(run, chain)| {
                chain
                    .subject
                    .is_some_and(|subject| std::ptr::eq(line.runs[run][subject], last))
            });
            if !(possessive || subject) {
                continue;
            }
            had_candidates = true;
            if naming_context(sentence, found.tokens.start)
                || found
                    .name
                    .split_whitespace()
                    .map(|word| word.replace('’', "'").to_lowercase())
                    .any(|word| cast_words.iter().any(|(cast, _)| *cast == word))
            {
                continue;
            }
            if context.sentence_allows(index, first, record.tense, false) {
                let span = Span::covering(&first.span, &sentence.tokens[found.tokens.end - 1].span);
                drift = Some((span, found.name.clone(), PersonDriftKind::NarratorName));
                break;
            }
        }
        if drift.is_none() {
            if let (Some(pronoun), Some(gender)) = (pronoun, gender) {
                for (run, chain) in line.chains() {
                    let Some(subject) = chain.subject else {
                        continue;
                    };
                    let token = line.runs[run][subject];
                    let third = matches!(
                        pronoun_person(&line.runs[run], subject),
                        Some((NarrativePerson::Third, Case::Subject))
                    );
                    if !third || fold(&token.text) != pronoun {
                        continue;
                    }
                    had_candidates = true;
                    let byte = token.span.byte.start;
                    let earlier = scene_words
                        .iter()
                        .take_while(|other| other.span.byte.start < byte);
                    let gendered_noun = earlier.clone().any(|other| {
                        gender.nouns().contains(&fold(&other.text).as_str())
                            || other.readings.iter().any(|reading| {
                                reading.pos.starts_with("NN")
                                    && gender.nouns().contains(&reading.lemma.as_str())
                            })
                    });
                    let unknown_name = lines[..=offset].iter().any(|other| {
                        other
                            .sentence
                            .tokens
                            .iter()
                            .filter(|token| token.is_word)
                            .skip(1)
                            .filter(|other| other.span.byte.start < byte)
                            .any(|other| {
                                let word = fold(&other.text);
                                proper_name(other)
                                    && !narrator_words.contains(&word)
                                    && !cast_words
                                        .iter()
                                        .any(|(cast, pronoun)| *cast == word && *pronoun != gender)
                            })
                    });
                    let paragraph_start = context.paragraph_starts[line.paragraph];
                    let neighbour = |index: usize| {
                        context
                            .lines
                            .get(index)
                            .filter(|other| other.paragraph == line.paragraph)
                            .is_some_and(Line::first_subject)
                    };
                    let sandwich =
                        (index > paragraph_start && neighbour(index - 1)) || neighbour(index + 1);
                    let allowed = same_subjects <= options.maximum_slips
                        && !same_cast_mentioned
                        && !voiced_pronoun
                        && !gendered_noun
                        && !unknown_name
                        && sandwich
                        && !context.self_frames[line.paragraph]
                        && context.sentence_allows(index, token, record.tense, false);
                    if allowed {
                        drift = Some((
                            token.span.clone(),
                            fold(&token.text),
                            PersonDriftKind::ThirdInFirst,
                        ));
                        break;
                    }
                }
            }
        }
        if let Some((span, word, kind)) = drift {
            drifts.push(PersonDrift {
                span,
                sentence_span: sentence.span.clone(),
                sentence: index,
                scene: record.scene,
                kind,
                word,
                expected: NarrativePerson::First,
            });
        }
    }
    if had_candidates && record.tense.is_none() {
        record.suppressed = Some(PersonSuppression::UndeterminedTense);
    }
}

/// Naming contexts before a narrator name: *named Anna*, *Dear Anna*, *my name is Anna*,
/// *I, Anna,*, and an honorific the narrator's names do not list (*Mrs. Reyes* when only
/// *Anna Reyes* is declared; a listed form matches from the honorific).
fn naming_context(sentence: &Sentence, start: usize) -> bool {
    let previous: Vec<&Token> = sentence.tokens[..start]
        .iter()
        .rev()
        .filter(|token| !token.is_whitespace)
        .take(3)
        .collect();
    let word = |index: usize| previous.get(index).map(|token| fold(&token.text));
    let first = word(0);
    if first.as_deref().is_some_and(|word| NAMING.contains(&word)) {
        return true;
    }
    if matches!(first.as_deref(), Some("is" | "was" | "'s")) && word(1).as_deref() == Some("name") {
        return true;
    }
    if first.as_deref() == Some(",") && previous.get(1).is_some_and(|token| token.text == "I") {
        return true;
    }
    let honorific_before = previous
        .iter()
        .find(|token| token.text != ".")
        .is_some_and(|token| honorific(&token.text));
    honorific_before
}
