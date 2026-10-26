//! Opt-in comparison of capitalized document forms with the caller's name roster.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use super::diction::ConcretenessTable;
use super::name_phonetics::{Codes, EnglishNamePhonetics, NamePhonetics};
use super::{part_span, parts, Split, HYPHENS};
use crate::pronunciation::initial_sound;
use crate::{Document, Mode, Span, StyleError, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameSpellingReason {
    Edit,
    Phonetic,
    Diacritics,
    Case,
    Spacing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingOptions {
    #[serde(alias = "include_dictionary_words")]
    pub include_dictionary_words: bool,
    pub phonetic: bool,
    /// An edit or phonetic near miss used at least this often in the document is listed under
    /// its roster entry as a possible other character instead of flagged; 0 never lists one.
    #[serde(alias = "other_character_minimum")]
    pub other_character_minimum: usize,
}

impl Default for NameSpellingOptions {
    fn default() -> Self {
        Self {
            include_dictionary_words: false,
            phonetic: true,
            other_character_minimum: 3,
        }
    }
}

impl NameSpellingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_include_dictionary_words(mut self, include: bool) -> Self {
        self.include_dictionary_words = include;
        self
    }

    pub fn with_phonetic(mut self, phonetic: bool) -> Self {
        self.phonetic = phonetic;
        self
    }

    pub fn with_other_character_minimum(mut self, minimum: usize) -> Self {
        self.other_character_minimum = minimum;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingFlag {
    pub span: Span,
    pub sentence: usize,
    pub form: String,
    pub name: String,
    pub alternatives: Vec<String>,
    pub reason: NameSpellingReason,
    pub distance: Option<usize>,
    pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingVariant {
    pub form: String,
    pub reason: NameSpellingReason,
    pub counts: Split<usize>,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingName {
    pub name: String,
    pub counts: Split<usize>,
    /// Flagged forms only.
    pub variants: Vec<NameSpellingVariant>,
    /// Frequent edit or phonetic near misses, not flagged: possibly another character.
    pub possible_characters: Vec<NameSpellingVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingReport {
    pub flags: Vec<NameSpellingFlag>,
    pub names: Vec<NameSpellingName>,
}

// English endings that derive a family plural, demonym or adjective from a listed name
// (*the Stirlings*, *Nantucketers*, *Mingottian*); such forms are not misspellings.
const DERIVED: [&str; 7] = ["s", "es", "er", "ers", "ian", "ians", "ish"];

// The tagger reads capitalized mid-sentence words as NNP, so months and weekdays
// (*June* near *Jane*) need their own list; the bundled lemmas exclude proper nouns.
const CALENDAR: [&str; 19] = [
    "january",
    "february",
    "march",
    "april",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "christmas",
];

#[derive(Clone)]
struct Unit<'a> {
    text: &'a str,
    span: Span,
    token: &'a Token,
    join_next: bool,
}

// A unit joins the next only when both are parts of one hyphenated token.
fn units(sentence: &crate::Sentence) -> Vec<Unit<'_>> {
    let mut out: Vec<Unit<'_>> = Vec::new();
    for token in &sentence.tokens {
        if !token.is_word {
            continue;
        }
        let first = out.len();
        for (start, part) in parts(&token.text, &HYPHENS) {
            let bare = part.trim_end_matches(['\'', '’']);
            let bare = bare
                .strip_suffix("'s")
                .or_else(|| bare.strip_suffix("’s"))
                .unwrap_or(bare);
            if bare.is_empty() {
                continue;
            }
            if out.len() > first {
                if let Some(last) = out.last_mut() {
                    last.join_next = true;
                }
            }
            out.push(Unit {
                text: bare,
                span: part_span(&token.span, &token.text, start, bare),
                token,
                join_next: false,
            });
        }
    }
    out
}

// NFC with every hyphen as '-', so *Mary\u{2011}Ann* is the listed *Mary-Ann*.
fn nfc(value: &str) -> String {
    value
        .nfc()
        .map(|c| if HYPHENS.contains(&c) { '-' } else { c })
        .collect()
}

fn folded(value: &str) -> String {
    nfc(value).to_lowercase()
}

fn plain(folded: &str) -> String {
    folded.nfd().filter(|c| !is_combining_mark(*c)).collect()
}

// The comparison key without spaces, apostrophes or hyphens (*Vanderberg*, *Mary Ann*).
fn joined(plain: &str) -> String {
    plain
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\'' | '’') && !HYPHENS.contains(c))
        .collect()
}

fn letters(value: &str) -> usize {
    value.chars().filter(|c| c.is_alphabetic()).count()
}

// Damerau optimal string alignment, on Unicode scalar values.
fn osa(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

fn jaro_winkler(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let window = (a.len().max(b.len()) / 2).saturating_sub(1);
    let mut a_matches = vec![false; a.len()];
    let mut b_matches = vec![false; b.len()];
    let mut matches = 0;
    for (i, &letter) in a.iter().enumerate() {
        let start = i.saturating_sub(window);
        let end = (i + window + 1).min(b.len());
        for j in start..end {
            if !b_matches[j] && b[j] == letter {
                a_matches[i] = true;
                b_matches[j] = true;
                matches += 1;
                break;
            }
        }
    }
    if matches == 0 {
        return 0.0;
    }
    let mut transposed = 0;
    let mut j = 0;
    for (i, &matched) in a_matches.iter().enumerate() {
        if matched {
            while !b_matches[j] {
                j += 1;
            }
            transposed += usize::from(a[i] != b[j]);
            j += 1;
        }
    }
    let m = matches as f64;
    let jaro = (m / a.len() as f64 + m / b.len() as f64 + (m - transposed as f64 / 2.0) / m) / 3.0;
    let prefix = a.iter().zip(&b).take(4).take_while(|(x, y)| x == y).count();
    if jaro >= 0.7 {
        jaro + 0.1 * prefix as f64 * (1.0 - jaro)
    } else {
        jaro
    }
}

#[derive(Clone, Copy)]
struct Match {
    index: usize,
    reason: NameSpellingReason,
    distance: Option<usize>,
    rank: (u8, usize),
}

struct Keys {
    nfc: String,
    folded: String,
    plain: String,
    joined: String,
}

impl Keys {
    fn new(value: &str) -> Self {
        let nfc = nfc(value);
        let folded = nfc.to_lowercase();
        let plain = plain(&folded);
        let joined = joined(&plain);
        Self {
            nfc,
            folded,
            plain,
            joined,
        }
    }
}

struct WordKey {
    text: String,
    folded: String,
    letters: usize,
    sound: &'static str,
    phonetic: Option<Codes>,
}

impl WordKey {
    fn new(value: &str, phonetic: bool) -> Self {
        Self {
            text: value.to_owned(),
            folded: folded(value),
            letters: letters(value),
            sound: initial_sound(value),
            phonetic: phonetic.then(|| EnglishNamePhonetics.codes(value)),
        }
    }
}

#[derive(Default)]
struct RosterIndex {
    exact: HashMap<String, usize>,
    nfc: HashMap<String, Vec<usize>>,
    all_caps: HashMap<String, usize>,
    initial_caps: HashMap<String, usize>,
    folded: HashMap<String, Vec<usize>>,
    plain: HashMap<String, Vec<usize>>,
    joined: HashMap<String, Vec<usize>>,
    words: Vec<Vec<WordKey>>,
    all_indices: Vec<usize>,
    first_sound_indices: HashMap<&'static str, Vec<usize>>,
    phonetic_index: HashMap<String, Vec<usize>>,
    lowercase_initial: bool,
    stems: HashSet<String>,
    joined_prefixes: HashSet<String>,
    phrase_first_sounds: HashSet<&'static str>,
    particles: HashSet<String>,
    max_units: usize,
}

// A lowercase word in a candidate phrase must be the listed entry's own word at that place
// (*de* in *Anna Maria de Sousa*); otherwise *Jane hurt* would be a near miss of *Jane Hart*.
fn particles_match(candidate: &[WordKey], name: &[WordKey]) -> bool {
    candidate
        .iter()
        .zip(name)
        .all(|(a, b)| !a.text.chars().next().is_some_and(char::is_lowercase) || a.text == b.text)
}

impl RosterIndex {
    fn new(names: &[NameSpellingName], phonetic: bool) -> Self {
        let mut index = Self::default();
        for (at, name) in names.iter().enumerate() {
            index.exact.entry(name.name.clone()).or_insert(at);
            let keys = Keys::new(&name.name);
            index.all_caps.entry(keys.nfc.to_uppercase()).or_insert(at);
            if let Some(first) = name.name.chars().next().filter(|c| c.is_lowercase()) {
                let tail = name
                    .name
                    .get(first.len_utf8()..)
                    .expect("first char boundary");
                let capitalized = first.to_uppercase().collect::<String>() + tail;
                index.initial_caps.entry(nfc(&capitalized)).or_insert(at);
            }
            index.nfc.entry(keys.nfc).or_default().push(at);
            index.folded.entry(keys.folded).or_default().push(at);
            index.plain.entry(keys.plain).or_default().push(at);
            index.joined.entry(keys.joined).or_default().push(at);
            let name_words: Vec<WordKey> = name
                .name
                .split_whitespace()
                .map(|word| WordKey::new(word, phonetic))
                .collect();
            index.all_indices.push(at);
            index
                .stems
                .extend(name_words.iter().map(|word| word.folded.clone()));
            if name_words.len() == 1 {
                index
                    .first_sound_indices
                    .entry(name_words[0].sound)
                    .or_default()
                    .push(at);
            }
            for codes in name_words.iter().filter_map(|word| word.phonetic.as_ref()) {
                for key in [&codes.primary, &codes.alternate] {
                    if !key.is_empty() {
                        index
                            .phonetic_index
                            .entry(key.clone())
                            .or_default()
                            .push(at);
                    }
                }
            }
            if name_words.len() > 1 {
                index.phrase_first_sounds.insert(name_words[0].sound);
            }
            index.particles.extend(
                name_words
                    .iter()
                    .filter(|word| word.text.chars().next().is_some_and(char::is_lowercase))
                    .map(|word| word.text.clone()),
            );
            let units = name
                .name
                .split(|c: char| c.is_whitespace() || HYPHENS.contains(&c))
                .filter(|part| !part.is_empty())
                .count();
            index.max_units = index.max_units.max(units);
            index.words.push(name_words);
            index.lowercase_initial |= name.name.chars().next().is_some_and(char::is_lowercase);
        }
        for key in index.joined.keys() {
            for (offset, _) in key.char_indices().skip(1) {
                index.joined_prefixes.insert(key[..offset].to_owned());
            }
        }
        index
    }

    fn correct(&self, form: &str, sentence_start: bool) -> Option<usize> {
        if let Some(&at) = self.exact.get(form) {
            return Some(at);
        }
        let normalized = nfc(form);
        if let Some(&at) = self
            .nfc
            .get(&normalized)
            .and_then(|indices| indices.first())
        {
            return Some(at);
        }
        let letters: Vec<char> = form.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
            if let Some(&at) = self.all_caps.get(&normalized) {
                return Some(at);
            }
        }
        if sentence_start {
            if let Some(&at) = self.initial_caps.get(&normalized) {
                return Some(at);
            }
        }
        None
    }

    fn derived(&self, folded: &str) -> bool {
        DERIVED.iter().any(|suffix| {
            folded
                .strip_suffix(suffix)
                .is_some_and(|stem| self.stems.contains(stem))
        })
    }

    fn matches(&self, form: &str, fuzzy: bool, phonetic: bool) -> Vec<Match> {
        let keys = Keys::new(form);
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        for (map, key, reason) in [
            (&self.nfc, &keys.nfc, NameSpellingReason::Diacritics),
            (&self.folded, &keys.folded, NameSpellingReason::Case),
            (&self.plain, &keys.plain, NameSpellingReason::Diacritics),
            (&self.joined, &keys.joined, NameSpellingReason::Spacing),
        ] {
            if let Some(indices) = map.get(key) {
                for &index in indices {
                    if seen.insert(index) {
                        found.push(Match {
                            index,
                            reason,
                            distance: None,
                            rank: (0, 0),
                        });
                    }
                }
            }
        }
        if found.is_empty() && fuzzy && !self.derived(&keys.folded) {
            let candidate: Vec<WordKey> = form
                .split_whitespace()
                .map(|word| WordKey::new(word, false))
                .collect();
            let edit_indices = if candidate.len() == 1 {
                self.first_sound_indices
                    .get(candidate[0].sound)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            } else {
                &self.all_indices
            };
            for &index in edit_indices {
                let name = &self.words[index];
                if candidate.len() != name.len() || !particles_match(&candidate, name) {
                    continue;
                }
                let mut differing = candidate
                    .iter()
                    .zip(name)
                    .filter(|(a, b)| a.folded != b.folded);
                let Some((a, b)) = differing.next() else {
                    continue;
                };
                if differing.next().is_some() || a.letters < 4 || b.letters < 4 {
                    continue;
                }
                let limit = if b.letters < 8 { 1 } else { 2 };
                if a.folded.chars().count().abs_diff(b.folded.chars().count()) > limit
                    || a.sound != b.sound
                {
                    continue;
                }
                let distance = osa(&a.folded, &b.folded);
                if distance <= limit {
                    found.push(Match {
                        index,
                        reason: NameSpellingReason::Edit,
                        distance: Some(distance),
                        rank: (1, distance),
                    });
                }
            }
            if found.is_empty() && phonetic {
                let candidate_codes: Vec<Codes> = candidate
                    .iter()
                    .map(|word| EnglishNamePhonetics.codes(&word.folded))
                    .collect();
                let mut plausible = HashSet::new();
                for codes in &candidate_codes {
                    for key in [&codes.primary, &codes.alternate] {
                        if let Some(indices) = self.phonetic_index.get(key) {
                            plausible.extend(indices.iter().copied());
                        }
                    }
                }
                for index in plausible {
                    let name = &self.words[index];
                    if candidate.len() != name.len() || !particles_match(&candidate, name) {
                        continue;
                    }
                    let mut differing = candidate
                        .iter()
                        .zip(name)
                        .enumerate()
                        .filter(|(_, (a, b))| a.folded != b.folded);
                    let Some((component, (a, b))) = differing.next() else {
                        continue;
                    };
                    if differing.next().is_some()
                        || a.letters < 4
                        || b.letters < 4
                        || a.sound != b.sound
                    {
                        continue;
                    }
                    let Some(other) = b
                        .phonetic
                        .as_ref()
                        .filter(|other| candidate_codes[component].overlaps(other))
                    else {
                        continue;
                    };
                    // The second bound limits the vowel and syllable differences the keys drop,
                    // relative to length; spellings of one sound (C/K, PH/F) don't count.
                    let (a, b) = (&candidate_codes[component].transcript, &other.transcript);
                    if jaro_winkler(a, b) >= 0.85 {
                        let distance = osa(a, b);
                        found.push(Match {
                            index,
                            reason: NameSpellingReason::Phonetic,
                            distance: None,
                            rank: (2, distance),
                        });
                    }
                }
            }
        }
        found.sort_by_key(|m| (m.rank, m.index));
        found
    }
}

fn name_like(unit: &Unit<'_>, lowercase_seen: bool, options: &NameSpellingOptions) -> bool {
    let Some(first) = unit.text.chars().next() else {
        return false;
    };
    let letters: Vec<char> = unit.text.chars().filter(|c| c.is_alphabetic()).collect();
    let all_caps = letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    first.is_uppercase()
        && !all_caps
        && !unit.token.is_skipped
        && !lowercase_seen
        && (options.include_dictionary_words
            || unit
                .token
                .readings
                .iter()
                .all(|reading| matches!(reading.pos.as_str(), "NNP" | "NNPS")))
}

// Capitalized common nouns (*Part*, *Port*, *Lady*) read as NNP mid-sentence, so the
// bundled fiction lemmas stand in for a lowercase use the document may not contain.
// A phrase is exempt: its other components must already match the listed name (*Van der Berg*).
fn common_word(text: &str) -> bool {
    let word = crate::reports::echoes::fold(text);
    CALENDAR.contains(&word.as_str())
        || crate::document::honorific(&word)
        || ConcretenessTable::bundled().get(&word).is_some()
}

fn inc(split: &mut Split<usize>, mode: Mode) {
    for count in split.all_and_mode_mut(mode) {
        *count += 1;
    }
}

pub fn report(
    document: &Document,
    options: &NameSpellingOptions,
) -> Result<NameSpellingReport, StyleError> {
    // Entries are trimmed, inner whitespace runs collapse to one space, and duplicates keep the first.
    let mut seen = HashSet::new();
    let mut names: Vec<NameSpellingName> = document
        .skip_names
        .iter()
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|name| !name.is_empty() && seen.insert(name.clone()))
        .map(|name| NameSpellingName {
            name,
            counts: Split::default(),
            variants: Vec::new(),
            possible_characters: Vec::new(),
        })
        .collect();
    let mut flags = Vec::new();
    if names.is_empty() {
        return Ok(NameSpellingReport { flags, names });
    }
    let index = RosterIndex::new(&names, options.phonetic);
    let sentences: Vec<_> = document.sentences().collect();
    let sentence_units: Vec<Vec<Unit<'_>>> = sentences.iter().map(|s| units(s)).collect();
    let lowercase_raw: HashSet<&str> = sentence_units
        .iter()
        .flatten()
        .filter(|u| u.text.chars().next().is_some_and(char::is_lowercase))
        .map(|u| u.text)
        .collect();
    let lowercase: HashSet<String> = lowercase_raw.into_iter().map(folded).collect();
    // Indexed by whether fuzzy matching was allowed.
    let mut cache: [HashMap<String, Vec<Match>>; 2] = Default::default();
    let mut variants: Vec<BTreeMap<(String, u8), NameSpellingVariant>> =
        vec![BTreeMap::new(); names.len()];
    for ((sentence_index, sentence), units) in sentences.iter().enumerate().zip(&sentence_units) {
        let mut at = 0;
        while at < units.len() {
            let first_upper = units[at]
                .text
                .chars()
                .next()
                .is_some_and(char::is_uppercase);
            if !index.lowercase_initial && !first_upper {
                at += 1;
                continue;
            }
            let first_folded = folded(units[at].text);
            let first_lower_seen = lowercase.contains(&first_folded);
            let allow_folded = first_upper && !first_lower_seen;
            // Without folded matches only exact forms count, so the fuzzy gates are not needed.
            let first_fuzzy = allow_folded && name_like(&units[at], first_lower_seen, options);
            let first_ordinary_ok =
                first_fuzzy && (options.include_dictionary_words || !common_word(units[at].text));
            // Runs reach the longest roster entry, or a whole hyphenated token, which may split a
            // listed word into more parts (*Val-an-cy*).
            let token_parts = 1 + units[at..].iter().take_while(|u| u.join_next).count();
            let longest = index.max_units.max(token_parts);
            let may_join = longest > 1
                && (index
                    .phrase_first_sounds
                    .contains(initial_sound(units[at].text))
                    || index
                        .joined_prefixes
                        .contains(&joined(&plain(&first_folded))));
            let mut selected = None;
            for size in (1..=longest.min(units.len() - at)).rev() {
                if size > 1 && !may_join {
                    continue;
                }
                let run = &units[at..at + size];
                // A run crosses only whitespace, apostrophe punctuation, or a split hyphen. The
                // tokenizer splits U+2011 into its own token, so a lone hyphen gap joins too.
                if run.windows(2).any(|pair| {
                    if pair[0].join_next {
                        return false;
                    }
                    let from = pair[0].span.byte.end - sentence.span.byte.start;
                    let to = pair[1].span.byte.start - sentence.span.byte.start;
                    sentence.text.get(from..to).is_none_or(|gap| {
                        let mut chars = gap.chars();
                        let lone_hyphen = chars.next().is_some_and(|c| HYPHENS.contains(&c))
                            && chars.next().is_none();
                        !lone_hyphen
                            && !gap
                                .chars()
                                .all(|c| c.is_whitespace() || c == '\'' || c == '’')
                    })
                }) {
                    continue;
                }
                let start = run[0].span.byte.start - sentence.span.byte.start;
                let end = run[size - 1].span.byte.end - sentence.span.byte.start;
                let Some(form) = sentence.text.get(start..end) else {
                    continue;
                };
                let exact = index.correct(form, at == 0);
                if let Some(index) = exact {
                    selected = Some((size, form.to_owned(), Some(index), Vec::new()));
                    break;
                }
                // Folded matches can include a lowercase component in a multiword name.
                // The document's skip matcher accepts some case variants of a listed name.
                // Their literal surface still belongs in this report's case tier.
                if !allow_folded {
                    continue;
                }
                // Each later word needs a capital not used lowercase in the document, or must be
                // a lowercase particle of a listed name (*de*, *van*), checked per entry.
                let fuzzy = first_fuzzy
                    && (size > 1 || first_ordinary_ok)
                    && run.iter().skip(1).all(|u| {
                        if u.text.chars().next().is_some_and(char::is_uppercase) {
                            !lowercase.contains(&folded(u.text))
                        } else {
                            index.particles.contains(u.text)
                        }
                    });
                let cache = &mut cache[usize::from(fuzzy)];
                let matches = match cache.get(form) {
                    Some(matches) => matches,
                    None => cache
                        .entry(form.to_owned())
                        .or_insert_with(|| index.matches(form, fuzzy, options.phonetic)),
                };
                if !matches.is_empty() {
                    selected = Some((size, form.to_owned(), None, matches.clone()));
                    break;
                }
            }
            if let Some((size, form, exact, matches)) = selected {
                let span = Span::covering(&units[at].span, &units[at + size - 1].span);
                let mode = document.mode_at(span.byte.start);
                if let Some(index) = exact {
                    inc(&mut names[index].counts, mode);
                } else if let Some(best) = matches.first() {
                    let alternatives = matches
                        .iter()
                        .skip(1)
                        .take_while(|m| m.rank == best.rank)
                        .map(|m| names[m.index].name.clone())
                        .collect();
                    flags.push(NameSpellingFlag {
                        span: span.clone(),
                        sentence: sentence_index,
                        form: form.clone(),
                        name: names[best.index].name.clone(),
                        alternatives,
                        reason: best.reason,
                        distance: best.distance,
                        mode,
                    });
                    let reason_key = match best.reason {
                        NameSpellingReason::Edit => 0,
                        NameSpellingReason::Phonetic => 4,
                        NameSpellingReason::Diacritics => 1,
                        NameSpellingReason::Case => 2,
                        NameSpellingReason::Spacing => 3,
                    };
                    let variant = variants[best.index]
                        .entry((form.clone(), reason_key))
                        .or_insert_with(|| NameSpellingVariant {
                            form,
                            reason: best.reason,
                            counts: Split::default(),
                            spans: Vec::new(),
                        });
                    inc(&mut variant.counts, mode);
                    variant.spans.push(span);
                }
                at += size;
            } else {
                at += 1;
            }
        }
    }
    // A near miss used often is more likely another character than a slip. Case, diacritic
    // and spacing variants are spellings of the listed name itself and stay flagged.
    let mut listed = HashSet::new();
    for (name, variants) in names.iter_mut().zip(variants) {
        for variant in variants.into_values() {
            let near = matches!(
                variant.reason,
                NameSpellingReason::Edit | NameSpellingReason::Phonetic
            );
            if near
                && options.other_character_minimum > 0
                && variant.counts.all >= options.other_character_minimum
            {
                listed.insert((name.name.clone(), variant.form.clone(), variant.reason));
                name.possible_characters.push(variant);
            } else {
                name.variants.push(variant);
            }
        }
    }
    flags.retain(|flag| !listed.contains(&(flag.name.clone(), flag.form.clone(), flag.reason)));
    Ok(NameSpellingReport { flags, names })
}
