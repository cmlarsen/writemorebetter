use serde::{Deserialize, Serialize};

use super::Split;
use crate::{Document, Mode, Sentence, Span, StyleError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReadabilityOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
    #[serde(alias = "minimum_paragraph_words")]
    pub minimum_paragraph_words: usize,
    #[serde(alias = "words_per_minute")]
    pub words_per_minute: f64,
    #[serde(alias = "exclude_dialogue_paragraphs")]
    pub exclude_dialogue_paragraphs: bool,
}
impl Default for ReadabilityOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
            minimum_paragraph_words: 30,
            words_per_minute: 260.0,
            exclude_dialogue_paragraphs: false,
        }
    }
}
impl ReadabilityOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, value: usize) -> Self {
        self.minimum_sentences = value;
        self
    }
    pub fn with_minimum_paragraph_words(mut self, value: usize) -> Self {
        self.minimum_paragraph_words = value;
        self
    }
    pub fn with_words_per_minute(mut self, value: f64) -> Self {
        self.words_per_minute = value;
        self
    }
    pub fn with_exclude_dialogue_paragraphs(mut self, value: bool) -> Self {
        self.exclude_dialogue_paragraphs = value;
        self
    }
}

/// Counts and the four indices for one sample: a group, or one paragraph.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReadabilityScores {
    pub sentences: usize,
    pub words: usize,
    pub syllables: usize,
    pub characters: usize,
    pub polysyllables: usize,
    pub low_confidence: bool,
    pub flesch_reading_ease: Option<f64>,
    pub flesch_kincaid_grade: Option<f64>,
    pub coleman_liau: Option<f64>,
    pub automated_readability: Option<f64>,
    pub reading_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReadabilityParagraph {
    pub span: Span,
    pub dialogue_only: bool,
    pub excluded: bool,
    pub scores: ReadabilityScores,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReadabilityReport {
    pub groups: Split<ReadabilityScores>,
    pub paragraphs: Vec<ReadabilityParagraph>,
}

/// Syllables per word. The report takes any counter, so a pronouncing dictionary with a
/// heuristic fallback can replace [`HeuristicSyllables`] without changing the formulas.
pub trait SyllableCounter {
    /// `word` is one merged word as it appears in the text: it may hold curly or straight
    /// apostrophes, hyphens, digits and capitals. Returns at least 1.
    fn syllables(&self, word: &str) -> usize;
}

/// The phase 1 counter: vowel groups with English exceptions. See `docs/STYLE-READABILITY.md`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicSyllables;

impl SyllableCounter for HeuristicSyllables {
    fn syllables(&self, word: &str) -> usize {
        syllables(word)
    }
}

pub fn report(
    document: &Document,
    options: &ReadabilityOptions,
) -> Result<ReadabilityReport, StyleError> {
    report_with(document, options, &HeuristicSyllables)
}

pub fn report_with(
    document: &Document,
    options: &ReadabilityOptions,
    counter: &dyn SyllableCounter,
) -> Result<ReadabilityReport, StyleError> {
    if !options.words_per_minute.is_finite() || options.words_per_minute <= 0.0 {
        return Err(StyleError::options(
            "wordsPerMinute must be finite and positive",
        ));
    }
    let mut groups: Split<ReadabilityScores> = Split::default();
    let mut paragraphs = Vec::with_capacity(document.paragraphs.len());
    for paragraph in &document.paragraphs {
        let mut scores = ReadabilityScores::default();
        let mut modes = Vec::new();
        let mut counted = Vec::new();
        for sentence in &paragraph.sentences {
            let mut sample = ReadabilityScores::default();
            for word in sentence_words(sentence) {
                let syllables = counter.syllables(&word).max(1);
                sample.words += 1;
                sample.syllables += syllables;
                sample.polysyllables += usize::from(syllables >= 3);
                sample.characters += word.chars().filter(|c| c.is_alphanumeric()).count();
            }
            if sample.words == 0 {
                continue;
            }
            sample.sentences = 1;
            let mode = document.sentence_mode(sentence);
            scores.add(&sample);
            modes.push(mode);
            counted.push((mode, sample));
        }
        let dialogue_only = !modes.is_empty() && modes.iter().all(|&m| m == Mode::Dialogue);
        let excluded = dialogue_only && options.exclude_dialogue_paragraphs;
        if !excluded {
            for (mode, sample) in &counted {
                for group in groups.all_and_mode_mut(*mode) {
                    group.add(sample);
                }
            }
        }
        scores.finish(options, options.minimum_paragraph_words, scores.words);
        paragraphs.push(ReadabilityParagraph {
            span: paragraph.span.clone(),
            dialogue_only,
            excluded,
            scores,
        });
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        let sentences = group.sentences;
        group.finish(options, options.minimum_sentences, sentences);
    }
    Ok(ReadabilityReport { groups, paragraphs })
}

impl ReadabilityScores {
    fn add(&mut self, other: &Self) {
        self.sentences += other.sentences;
        self.words += other.words;
        self.syllables += other.syllables;
        self.characters += other.characters;
        self.polysyllables += other.polysyllables;
    }

    fn finish(&mut self, options: &ReadabilityOptions, minimum: usize, sample: usize) {
        self.low_confidence = sample < minimum;
        self.reading_minutes = self.words as f64 / options.words_per_minute;
        if self.words == 0 || self.sentences == 0 {
            return;
        }
        let words = self.words as f64;
        let words_per_sentence = words / self.sentences as f64;
        let syllables_per_word = self.syllables as f64 / words;
        let characters_per_word = self.characters as f64 / words;
        self.flesch_reading_ease =
            Some(206.835 - 1.015 * words_per_sentence - 84.6 * syllables_per_word);
        self.flesch_kincaid_grade =
            Some(0.39 * words_per_sentence + 11.8 * syllables_per_word - 15.59);
        self.coleman_liau = Some(
            0.0588 * (100.0 * characters_per_word) - 0.296 * (100.0 / words_per_sentence) - 15.8,
        );
        self.automated_readability =
            Some(4.71 * characters_per_word + 0.5 * words_per_sentence - 21.43);
    }
}

fn is_joiner(gap: &str) -> bool {
    matches!(gap, "" | "-" | "\u{2010}" | "\u{2011}" | "'" | "’")
}

/// Words as readers count them: adjacent word tokens joined across no gap, a hyphen or an
/// apostrophe, so `can’t`, `She’s` and `well-known` are one word each. Skipped names count.
pub fn sentence_words(sentence: &Sentence) -> Vec<String> {
    let start = sentence.span.byte.start;
    let mut words: Vec<String> = Vec::new();
    let mut previous_end: Option<usize> = None;
    for token in sentence.tokens.iter().filter(|token| token.is_word) {
        let gap = previous_end.and_then(|end| {
            sentence
                .text
                .get(end - start..token.span.byte.start - start)
                .filter(|gap| is_joiner(gap))
        });
        match (gap, words.last_mut()) {
            (Some(gap), Some(word)) => {
                word.push_str(gap);
                word.push_str(&token.text);
            }
            _ => words.push(token.text.clone()),
        }
        previous_end = Some(token.span.byte.end);
    }
    words
}

/// Heuristic syllable count for one word; always at least 1.
pub fn syllables(word: &str) -> usize {
    let word: String = word
        .chars()
        .map(|c| if c == '’' { '\'' } else { c })
        .flat_map(char::to_lowercase)
        .collect();
    word.split(['-', '\u{2010}', '\u{2011}'])
        .filter(|part| part.chars().any(char::is_alphanumeric))
        .map(clitic_syllables)
        .sum::<usize>()
        .max(1)
}

fn ends_in_consonant(stem: &str) -> bool {
    stem.chars()
        .last()
        .is_some_and(|c| c.is_alphabetic() && !is_vowel_letter(c))
}

/// A consonant sound: `didn’t`, and also `haven’t` after a silent e.
fn ends_in_consonant_sound(stem: &str) -> bool {
    ends_in_consonant(stem)
        || stem
            .strip_suffix('e')
            .is_some_and(|rest| rest.len() >= 3 && ends_in_consonant(rest))
}

/// Handles contractions and possessives before counting the stem.
fn clitic_syllables(part: &str) -> usize {
    let part = part.trim_matches(|c: char| !c.is_alphanumeric());
    if let Some(stem) = part.strip_suffix("n't") {
        return letters_syllables(stem) + usize::from(ends_in_consonant_sound(stem));
    }
    if let Some((stem, clitic)) = part.rsplit_once('\'') {
        if !stem.is_empty() {
            let extra = match clitic {
                "ll" | "d" | "ve" | "re" => ends_in_consonant(stem),
                "s" => ["s", "z", "x", "ch", "sh", "ce", "ge", "se", "ze"]
                    .iter()
                    .any(|end| stem.ends_with(end)),
                "m" => false,
                _ => return letters_syllables(&part.replace('\'', "")),
            };
            return letters_syllables(stem) + usize::from(extra);
        }
    }
    letters_syllables(&part.replace('\'', ""))
}

fn is_vowel_letter(c: char) -> bool {
    matches!(
        c,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | 'y'
            | 'à'
            | 'á'
            | 'â'
            | 'ã'
            | 'ä'
            | 'å'
            | 'æ'
            | 'è'
            | 'é'
            | 'ê'
            | 'ë'
            | 'ì'
            | 'í'
            | 'î'
            | 'ï'
            | 'ò'
            | 'ó'
            | 'ô'
            | 'õ'
            | 'ö'
            | 'ø'
            | 'ù'
            | 'ú'
            | 'û'
            | 'ü'
            | 'ý'
            | 'ÿ'
    )
}

/// Silent-e compounds: `something` is `some` + `thing`, not some-e-thing.
const SILENT_E_PREFIXES: [&str; 8] = [
    "some", "where", "there", "here", "home", "fire", "life", "time",
];
/// Suffixes after which a stem's final e stays silent: `lovely`, `statement`, `hopeless`.
const SILENT_E_SUFFIXES: [(&str, usize); 7] = [
    ("ments", 1),
    ("ment", 1),
    ("fully", 2),
    ("ful", 1),
    ("less", 1),
    ("ness", 1),
    ("ly", 1),
];

fn letters_syllables(word: &str) -> usize {
    let chars: Vec<char> = word.chars().collect();
    if !chars.iter().any(|c| c.is_alphabetic()) {
        return 1;
    }
    if let Some(rest) = word.strip_prefix("every").filter(|rest| !rest.is_empty()) {
        // `every` alone is ev-er-y; in compounds it is ev-ry: every-one, every-thing.
        return 2 + letters_syllables(rest);
    }
    for prefix in SILENT_E_PREFIXES {
        if let Some(rest) = word.strip_prefix(prefix) {
            if rest.chars().any(is_vowel_letter)
                && (rest.starts_with(|c: char| !is_vowel_letter(c)) || rest.starts_with("one"))
                && !rest.starts_with("ver")
            {
                return letters_syllables(prefix) + letters_syllables(rest);
            }
        }
    }
    for (suffix, count) in SILENT_E_SUFFIXES {
        if let Some(stem) = word.strip_suffix(suffix) {
            let stem_chars: Vec<char> = stem.chars().collect();
            if let [.., before, 'e'] = stem_chars[..] {
                if stem_chars.len() >= 3 && !is_vowel_letter(before) {
                    return letters_syllables(stem) + count;
                }
            }
        }
    }
    vowel_group_syllables(&chars).max(1)
}

fn vowel_group_syllables(chars: &[char]) -> usize {
    let n = chars.len();
    let at = |i: isize| -> char {
        if i < 0 || i as usize >= n {
            '\0'
        } else {
            chars[i as usize]
        }
    };
    let ends_with = |suffix: &str| {
        let suffix: Vec<char> = suffix.chars().collect();
        n >= suffix.len() && chars[n - suffix.len()..] == suffix[..]
    };
    // Which letters act as vowels: u after q is a consonant, y is a consonant at the start
    // of a word and between vowels (`player`, `beyond`).
    let mut vowel = vec![false; n];
    for i in 0..n {
        let c = chars[i];
        vowel[i] = match c {
            'u' => i == 0 || chars[i - 1] != 'q',
            'y' => {
                let before =
                    i > 0 && (vowel[i - 1] || (chars[i - 1] == 'w' && i > 1 && vowel[i - 2]));
                let after = i + 1 < n && is_vowel_letter(chars[i + 1]);
                i > 0 && !(before && after)
            }
            _ => is_vowel_letter(c),
        };
    }
    // Vowel groups, split before a diaeresis (`naïve`) and after a vowel y that
    // precedes another vowel (`dying`, `everyone`), except a final -ye/-yes/-yed.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !vowel[i] {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < n && vowel[i] && !matches!(chars[i], 'ë' | 'ï' | 'ü') {
            let final_ye = chars[i - 1] == 'y'
                && chars[i] == 'e'
                && (i + 1 == n || (i + 2 == n && matches!(chars[i + 1], 's' | 'd')));
            if chars[i - 1] == 'y' && !final_ye {
                break;
            }
            i += 1;
        }
        groups.push((start, i));
    }
    let mut count = groups.len() as isize;
    let consonant = |i: isize| {
        let c = at(i);
        c.is_alphabetic() && !(i >= 0 && vowel[i as usize])
    };

    // Final silent e: -e, -es, -ed when that e is its own group.
    if let Some(&(start, end)) = groups.last() {
        let lone_e = end == start + 1 && chars[start] == 'e';
        let tail = n - end;
        let s = start as isize;
        let liquid_kept = matches!(at(s - 1), 'l' | 'r')
            && consonant(s - 2)
            && at(s - 2) != at(s - 1)
            && groups.len() > 1;
        if lone_e && groups.len() > 1 && consonant(s - 1) {
            let silent = match (tail, at(s + 1)) {
                (0, _) => !liquid_kept,
                (1, 's') => {
                    !liquid_kept
                        && !matches!(at(s - 1), 's' | 'x' | 'z' | 'c' | 'g')
                        && !(at(s - 1) == 'h' && matches!(at(s - 2), 'c' | 's'))
                }
                (1, 'd') => !liquid_kept && !matches!(at(s - 1), 't' | 'd'),
                _ => false,
            };
            if silent {
                count -= 1;
            }
        }
    }
    // -gue/-que endings (`tongue`, `league`) fold the ue into the consonant.
    if (ends_with("gue") || ends_with("gues"))
        && groups.len() > 1
        && !(ends_with("rgue") || ends_with("rgues"))
    {
        count -= 1;
    }

    // Hiatus: two adjacent vowels pronounced separately.
    for &(start, end) in &groups {
        for k in start..end.saturating_sub(1) {
            let (a, b) = (chars[k], chars[k + 1]);
            let k = k as isize;
            let prev = at(k - 1);
            let next = at(k + 2);
            let after = at(k + 3);
            let split = match (a, b) {
                ('i', 'a') => {
                    !(matches!(prev, 'c' | 't' | 's') && matches!(next, 'l' | 'n'))
                        && !(next == 'g' && after == 'e')
                }
                ('i', 'o') => {
                    !matches!(prev, 'c' | 't' | 's' | 'x' | 'g' | 'n' | 'h')
                        && !(prev == 'l' && at(k - 2) == 'l')
                        && k > 0
                }
                ('e', 'o') => {
                    prev != 'g'
                        && !(prev == 'p' && next == 'p')
                        && !(matches!(prev, 'l' | 'j') && next == 'p')
                }
                ('u', 'a') | ('u', 'o') => {
                    !matches!(prev, 'q' | 'g') && !(prev == 's' && matches!(next, 'd' | 'v'))
                }
                ('u', 'i') => {
                    !matches!(prev, 'q' | 'g') && (next == 'd' || (next == 'n' && after != 'g'))
                }
                ('i', 'u') => true,
                ('i', 'e') => {
                    let tail_ier = (n as isize - k == 3 && next == 'r')
                        || (n as isize - k == 4 && next == 's' && after == 't')
                        || (n as isize - k == 4 && next == 'r' && after == 's');
                    let sc = prev == 'c' && at(k - 2) == 's';
                    let ient = next == 'n'
                        && (matches!(after, 't' | 'c') || k + 3 == n as isize)
                        && (!matches!(prev, 'c' | 't' | 's') || sc);
                    (tail_ier && n >= 5 && consonant(k - 1)) || (next == 't' && k > 0) || ient
                }
                ('o', 'e') => matches!(next, 'm' | 't'),
                ('e', 'a') => {
                    (k + 2 == n as isize && n >= 4 && matches!(prev, 'd' | 'r'))
                        || (next == 't' && after == 'e' && k + 4 == n as isize)
                }
                _ => false,
            };
            if split {
                count += 1;
            }
        }
    }
    // -ing after a vowel: `going`, `being`, `seeing`.
    for suffix in ["ing", "ings"] {
        if ends_with(suffix) {
            let i = n - suffix.len();
            if i >= 2 && vowel[i - 1] && chars[i - 1] != 'y' {
                count += 1;
            }
        }
    }
    // -sm and -thm close a syllable of their own: `prism`, `rhythm`, `criticism`.
    if ends_with("sm") || ends_with("thm") {
        count += 1;
    }
    count.max(1) as usize
}
