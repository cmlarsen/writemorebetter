//! Opt-in comparison of capitalized document forms with the caller's name roster.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use super::Split;
use crate::pronunciation::initial_sound;
use crate::{Document, Mode, Span, StyleError, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NameSpellingReason {
    Edit,
    Diacritics,
    Case,
    Spacing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NameSpellingOptions {
    #[serde(alias = "include_dictionary_words")]
    pub include_dictionary_words: bool,
}

impl NameSpellingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_include_dictionary_words(mut self, include: bool) -> Self {
        self.include_dictionary_words = include;
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
}

impl WordKey {
    fn new(value: &str) -> Self {
        Self {
            folded: folded(value),
            letters: letters(value),
            sound: initial_sound(value),
        }
    }
}

#[derive(Default)]
struct RosterIndex {
    exact: HashMap<String, usize>,
    nfc: HashMap<String, Vec<usize>>,
    folded: HashMap<String, Vec<usize>>,
    plain: HashMap<String, Vec<usize>>,
    joined: HashMap<String, Vec<usize>>,
    words: Vec<Vec<WordKey>>,
    lowercase_initial: bool,
    joined_prefixes: HashSet<String>,
    phrase_first_sounds: HashSet<&'static str>,
}

impl RosterIndex {
    fn new(names: &[NameSpellingName]) -> Self {
        let mut index = Self::default();
        for (at, name) in names.iter().enumerate() {
            index.exact.entry(name.name.clone()).or_insert(at);
            let keys = Keys::new(&name.name);
            index.nfc.entry(keys.nfc).or_default().push(at);
            index.folded.entry(keys.folded).or_default().push(at);
            index.plain.entry(keys.plain).or_default().push(at);
            index.joined.entry(keys.joined).or_default().push(at);
            let name_words: Vec<WordKey> = name.name.split_whitespace().map(WordKey::new).collect();
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

    fn matches(&self, form: &str, fuzzy: bool) -> Vec<Match> {
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
            let candidate: Vec<WordKey> = form.split_whitespace().map(WordKey::new).collect();
            for (index, name) in self.words.iter().enumerate() {
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
    let index = RosterIndex::new(&names);
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
                let exact = index.exact.get(&form).copied();
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
                    .or_insert_with(|| index.matches(&form, fuzzy));
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
