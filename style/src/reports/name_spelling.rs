//! Opt-in comparison of capitalized document forms with the caller's name roster.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use super::name_phonetics::{Codes, EnglishNamePhonetics, NamePhonetics};
use super::Split;
use crate::pronunciation::initial_sound;
use crate::{Document, Mode, Span, StyleError, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl Default for NameSpellingOptions {
    fn default() -> Self {
        Self {
            include_dictionary_words: false,
            phonetic: true,
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
    pub variants: Vec<NameSpellingVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingReport {
    pub flags: Vec<NameSpellingFlag>,
    pub names: Vec<NameSpellingName>,
}

#[derive(Clone)]
struct Unit<'a> {
    text: &'a str,
    span: Span,
    token: &'a Token,
    join_next: bool,
}

fn subspan(span: &Span, text: &str, start: usize, part: &str) -> Span {
    let before = text.get(..start).expect("char boundary from split");
    let char_start = span.char.start + before.chars().count();
    let utf16_start = span.utf16.start + before.encode_utf16().count();
    Span {
        byte: span.byte.start + start..span.byte.start + start + part.len(),
        char: char_start..char_start + part.chars().count(),
        utf16: utf16_start..utf16_start + part.encode_utf16().count(),
    }
}

fn units(sentence: &crate::Sentence) -> Vec<Unit<'_>> {
    let mut out = Vec::new();
    for token in &sentence.tokens {
        if !token.is_word {
            continue;
        }
        let mut offset = 0;
        for part in token.text.split(['-', '‐']) {
            if !part.is_empty() {
                let bare = part.trim_end_matches(['\'', '’']);
                let bare = bare
                    .strip_suffix("'s")
                    .or_else(|| bare.strip_suffix("’s"))
                    .unwrap_or(bare);
                if !bare.is_empty() {
                    out.push(Unit {
                        text: bare,
                        span: subspan(&token.span, &token.text, offset, bare),
                        token,
                        join_next: false,
                    });
                }
            }
            offset += part.len();
            if let Some(separator) = token.text.get(offset..).and_then(|s| s.chars().next()) {
                offset += separator.len_utf8();
                if let Some(last) = out.last_mut() {
                    last.join_next = true;
                }
            }
        }
    }
    out
}

fn nfc(value: &str) -> String {
    value.nfc().collect()
}

fn folded(value: &str) -> String {
    nfc(value).to_lowercase()
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
        let plain: String = folded.nfd().filter(|c| !is_combining_mark(*c)).collect();
        let joined = plain
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '\'' && *c != '’' && *c != '-' && *c != '‐')
            .collect();
        Self {
            nfc,
            folded,
            plain,
            joined,
        }
    }
}

struct WordKey {
    folded: String,
    letters: usize,
    sound: &'static str,
    phonetic: Option<Codes>,
}

impl WordKey {
    fn new(value: &str, phonetic: bool) -> Self {
        Self {
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
    joined_prefixes: HashSet<String>,
    phrase_first_sounds: HashSet<&'static str>,
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
        if found.is_empty() && fuzzy {
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
                if candidate.len() != name.len() {
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
                    if candidate.len() != name.len() {
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
                    if !b
                        .phonetic
                        .as_ref()
                        .is_some_and(|other| candidate_codes[component].overlaps(other))
                    {
                        continue;
                    }
                    let distance = osa(&a.folded, &b.folded);
                    if distance <= 3 || jaro_winkler(&a.folded, &b.folded) >= 0.85 {
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

fn inc(split: &mut Split<usize>, mode: Mode) {
    for count in split.all_and_mode_mut(mode) {
        *count += 1;
    }
}

pub fn report(
    document: &Document,
    options: &NameSpellingOptions,
) -> Result<NameSpellingReport, StyleError> {
    let mut names: Vec<NameSpellingName> = document
        .skip_names
        .iter()
        .map(|name| NameSpellingName {
            name: name.clone(),
            counts: Split::default(),
            variants: Vec::new(),
        })
        .collect();
    let mut flags = Vec::new();
    if names.is_empty() {
        return Ok(NameSpellingReport { flags, names });
    }
    let index = RosterIndex::new(&names, options.phonetic);
    let sentences: Vec<_> = document.sentences().collect();
    let lowercase_raw: HashSet<&str> = sentences
        .iter()
        .flat_map(|s| units(s))
        .filter(|u| u.text.chars().next().is_some_and(char::is_lowercase))
        .map(|u| u.text)
        .collect();
    let lowercase: HashSet<String> = lowercase_raw.into_iter().map(folded).collect();
    let mut cache: HashMap<(String, bool), Vec<Match>> = HashMap::new();
    let mut variants: Vec<BTreeMap<(String, u8), NameSpellingVariant>> =
        vec![BTreeMap::new(); names.len()];
    for (sentence_index, sentence) in sentences.iter().enumerate() {
        let units = units(sentence);
        let mut at = 0;
        while at < units.len() {
            if !index.lowercase_initial
                && !units[at]
                    .text
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
            {
                at += 1;
                continue;
            }
            let first_folded = folded(units[at].text);
            let first_lower_seen = lowercase.contains(&first_folded);
            let allow_folded = units[at]
                .text
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
                && !first_lower_seen;
            let first_fuzzy = name_like(&units[at], first_lower_seen, options);
            let first_joined: String = first_folded
                .nfd()
                .filter(|c| {
                    !is_combining_mark(*c)
                        && !c.is_whitespace()
                        && *c != '\''
                        && *c != '’'
                        && *c != '-'
                        && *c != '‐'
                })
                .collect();
            let may_join = index.joined_prefixes.contains(&first_joined)
                || index
                    .phrase_first_sounds
                    .contains(initial_sound(units[at].text));
            let mut selected = None;
            for size in (1..=3.min(units.len() - at)).rev() {
                if size > 1 && !may_join {
                    continue;
                }
                let run = &units[at..at + size];
                // A run crosses only whitespace, apostrophe punctuation, or a split hyphen.
                if run.windows(2).any(|pair| {
                    if pair[0].join_next {
                        return false;
                    }
                    let from = pair[0].span.byte.end - sentence.span.byte.start;
                    let to = pair[1].span.byte.start - sentence.span.byte.start;
                    sentence.text.get(from..to).is_none_or(|gap| {
                        !gap.chars()
                            .all(|c| c.is_whitespace() || c == '\'' || c == '’')
                    })
                }) {
                    continue;
                }
                let start = run[0].span.byte.start - sentence.span.byte.start;
                let end = run[size - 1].span.byte.end - sentence.span.byte.start;
                let Some(form) = sentence.text.get(start..end).map(str::to_owned) else {
                    continue;
                };
                let exact = index.correct(&form, at == 0);
                if let Some(index) = exact {
                    selected = Some((size, form, Some(index), Vec::new()));
                    break;
                }
                let fuzzy = first_fuzzy
                    && run.iter().skip(1).all(|u| {
                        !lowercase.contains(&folded(u.text))
                            || u.text.chars().next().is_some_and(char::is_lowercase)
                    });
                // Folded matches can include a lowercase component in a multiword name.
                // The document's skip matcher accepts some case variants of a listed name.
                // Their literal surface still belongs in this report's case tier.
                if !allow_folded {
                    continue;
                }
                let matches = cache
                    .entry((form.clone(), fuzzy))
                    .or_insert_with(|| index.matches(&form, fuzzy, options.phonetic));
                if !matches.is_empty() {
                    selected = Some((size, form, None, matches.clone()));
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
    for (name, variants) in names.iter_mut().zip(variants) {
        name.variants = variants.into_values().collect();
    }
    Ok(NameSpellingReport { flags, names })
}
