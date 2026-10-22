//! The phrase list behind LanguageTool's `MultitokenSpeller`: known multi-word names and phrases,
//! looked up by a normalized key (lowercase, without diacritics, hyphens read as spaces).

use std::{collections::BTreeMap, convert::TryFrom};

use fst::Map;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::types::DefaultHashMap;

/// Known multi-word phrases, read from the build directory's `spell/multitoken.txt`.
///
/// Each spelling and each key is stored once, in a concatenated buffer, and both lookups refer
/// to the spellings by index: the phrases by the first character of their key, and an FST from
/// the key without spaces to its spellings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct MultitokenSpeller {
    /// Every spelling, phrase by phrase.
    spellings: StringList,
    /// The key of each phrase, grouped by first character. Within a character the keys are in
    /// the order LanguageTool's `HashMap` iterates them, because the search stops early on some
    /// matches.
    keys: StringList,
    /// For each phrase, the end of its spellings in `spellings` (the start is the end of the
    /// phrase before it).
    phrase_ends: Vec<u32>,
    /// Each first character of a key with the end of its phrases, sorted by character.
    first_chars: Vec<(char, u32)>,
    /// An FST from each key with the spaces removed to its spellings in `without_spaces_ids`,
    /// packed as `start << 32 | len`.
    without_spaces: Vec<u8>,
    /// Spelling indices, in phrase list order for each key without spaces.
    without_spaces_ids: Vec<u32>,
}

/// Strings concatenated into one buffer, with the end offset of each.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StringList {
    text: String,
    ends: Vec<u32>,
}

impl StringList {
    fn push(&mut self, text: &str) {
        self.text.push_str(text);
        self.ends.push(to_u32(self.text.len()));
    }

    fn get(&self, index: usize) -> &str {
        let start = index.checked_sub(1).map_or(0, |i| self.ends[i] as usize);
        &self.text[start..self.ends[index] as usize]
    }

    fn len(&self) -> usize {
        self.ends.len()
    }
}

impl MultitokenSpeller {
    /// Builds the list from its lines (one spelling per line), as
    /// `MultitokenSpeller.initMultitokenSpeller` does after `prepareLineForSpeller`. Single-word
    /// keys are skipped.
    pub(crate) fn from_lines(lines: impl IntoIterator<Item = String>) -> Self {
        // The phrases in the order their keys are first seen, each with its spellings.
        let mut phrases: Vec<(String, Vec<String>)> = Vec::new();
        let mut phrase_indices: DefaultHashMap<String, usize> = DefaultHashMap::default();
        let mut by_first_char: BTreeMap<char, Vec<usize>> = BTreeMap::new();
        // (phrase, spelling within the phrase) by key without spaces, in line order.
        let mut by_key_without_spaces: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();

        for line in lines {
            let key = normalize_key(&line);
            if !key.contains(' ') {
                continue;
            }
            let index = *phrase_indices.entry(key.clone()).or_insert_with(|| {
                let first = key.chars().next().expect("key contains a space");
                by_first_char.entry(first).or_default().push(phrases.len());
                phrases.push((key.clone(), Vec::new()));
                phrases.len() - 1
            });
            // A repeated line has the same key, so it is a repeat in both lookups.
            let spellings = &mut phrases[index].1;
            if !spellings.contains(&line) {
                by_key_without_spaces
                    .entry(key.replace(' ', ""))
                    .or_default()
                    .push((index, spellings.len()));
                spellings.push(line);
            }
        }

        let mut speller = MultitokenSpeller::default();
        // The index of each phrase's first spelling in `speller.spellings`.
        let mut first_spelling = vec![0u32; phrases.len()];
        for (first, mut indices) in by_first_char {
            let mask = java_hash_map_capacity(indices.len()) - 1;
            indices.sort_by_key(|&i| java_hash_map_hash(&phrases[i].0) & mask);
            for i in indices {
                let (key, spellings) = &phrases[i];
                first_spelling[i] = to_u32(speller.spellings.len());
                speller.keys.push(key);
                for spelling in spellings {
                    speller.spellings.push(spelling);
                }
                speller.phrase_ends.push(to_u32(speller.spellings.len()));
            }
            speller
                .first_chars
                .push((first, to_u32(speller.keys.len())));
        }

        let mut entries = Vec::with_capacity(by_key_without_spaces.len());
        for (key, spellings) in by_key_without_spaces {
            let start = to_u32(speller.without_spaces_ids.len());
            speller.without_spaces_ids.extend(
                spellings
                    .into_iter()
                    .map(|(phrase, j)| first_spelling[phrase] + to_u32(j)),
            );
            let len = to_u32(speller.without_spaces_ids.len()) - start;
            entries.push((key, u64::from(start) << 32 | u64::from(len)));
        }
        speller.without_spaces = Map::from_iter(entries)
            .expect("keys are sorted and unique")
            .into_fst()
            .into_inner();
        speller
    }

    /// The spellings whose key without spaces is `key`, in phrase list order.
    pub(crate) fn spellings_without_spaces(
        &self,
        key: &str,
    ) -> Option<impl Iterator<Item = &str> + Clone> {
        let fst = Map::new(self.without_spaces.as_slice()).expect("serialized fst must be valid.");
        let packed = fst.get(key)?;
        let start = (packed >> 32) as usize;
        let len = (packed & u64::from(u32::MAX)) as usize;
        Some(
            self.without_spaces_ids[start..start + len]
                .iter()
                .map(move |&i| self.spellings.get(i as usize)),
        )
    }

    /// The phrases whose key starts with `first`, each as its key and spellings, in the order
    /// LanguageTool's `HashMap` iterates them.
    pub(crate) fn phrases_starting_with(
        &self,
        first: char,
    ) -> impl Iterator<Item = (&str, impl Iterator<Item = &str> + Clone)> {
        let phrases = match self.first_chars.binary_search_by_key(&first, |&(c, _)| c) {
            Ok(i) => {
                let start = i
                    .checked_sub(1)
                    .map_or(0, |i| self.first_chars[i].1 as usize);
                start..self.first_chars[i].1 as usize
            }
            Err(_) => 0..0,
        };
        phrases.map(move |i| {
            let start = i.checked_sub(1).map_or(0, |i| self.phrase_ends[i] as usize);
            let spellings =
                (start..self.phrase_ends[i] as usize).map(move |j| self.spellings.get(j));
            (self.keys.get(i), spellings)
        })
    }

    /// Drops the spare capacity deserialization leaves in the buffers.
    pub(crate) fn shrink_to_fit(&mut self) {
        for list in [&mut self.spellings, &mut self.keys] {
            list.text.shrink_to_fit();
            list.ends.shrink_to_fit();
        }
        self.phrase_ends.shrink_to_fit();
        self.without_spaces.shrink_to_fit();
        self.without_spaces_ids.shrink_to_fit();
    }

    /// The number of distinct spellings.
    pub(crate) fn len(&self) -> usize {
        self.spellings.len()
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("phrase list fits in u32")
}

/// `HashMap.hash(key)` for a Java `String` key: `String.hashCode` with its high half folded in.
fn java_hash_map_hash(key: &str) -> u32 {
    let hash = key
        .encode_utf16()
        .fold(0u32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as u32));
    hash ^ (hash >> 16)
}

/// The table size of a Java `HashMap` after `len` insertions at the default load factor.
fn java_hash_map_capacity(len: usize) -> u32 {
    let mut capacity = 16;
    while len > capacity * 3 / 4 {
        capacity *= 2;
    }
    capacity as u32
}

/// `MultitokenSpeller.getNormalizeKey`: lowercase, without diacritics, hyphens as spaces.
pub(crate) fn normalize_key(text: &str) -> String {
    remove_diacritics(&text.to_lowercase()).replace('-', " ")
}

/// `StringTools.removeDiacritics`: the canonical decomposition without its combining
/// diacritical marks (U+0300 to U+036F).
fn remove_diacritics(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrases(speller: &MultitokenSpeller, first: char) -> Vec<(&str, Vec<&str>)> {
        speller
            .phrases_starting_with(first)
            .map(|(key, spellings)| (key, spellings.collect()))
            .collect()
    }

    fn without_spaces<'a>(speller: &'a MultitokenSpeller, key: &str) -> Option<Vec<&'a str>> {
        speller
            .spellings_without_spaces(key)
            .map(|spellings| spellings.collect())
    }

    #[test]
    fn normalize_key_strips_case_diacritics_and_hyphens() {
        assert_eq!(
            normalize_key("Antoine de Saint-Exupéry"),
            "antoine de saint exupery"
        );
        assert_eq!(normalize_key("Thé Dákk"), "the dakk");
        assert_eq!(
            normalize_key("Zoë\u{2019}s\u{2014}Quill"),
            "zoe\u{2019}s\u{2014}quill"
        );
        assert_eq!(
            normalize_key("Ka\u{2026}Dor\u{2028}Ash"),
            "ka\u{2026}dor\u{2028}ash"
        );
    }

    #[test]
    fn phrases_keep_the_java_hash_map_order() {
        assert_eq!(java_hash_map_hash("hello"), 99162322 ^ (99162322 >> 16));
        assert_eq!(java_hash_map_capacity(12), 16);
        assert_eq!(java_hash_map_capacity(13), 32);

        let speller = MultitokenSpeller::from_lines(
            [
                "New York",
                "Noah Harari",
                "Notre Dame",
                "North Sea",
                "Nova Scotia",
                "noah harari",
                "Noah Harari",
                "Harari",
                "Zo\u{eb} Quill",
                "Zoe-Quill",
                "Mara\u{2014}Venn",
                "O\u{2019}Brien Hall",
                "\u{201c}Ka\u{2026} Dor\u{201d}",
                "Ash\u{2028}Fall Rise",
            ]
            .map(String::from),
        );

        let keys: Vec<&str> = phrases(&speller, 'n').into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            [
                "noah harari",
                "nova scotia",
                "north sea",
                "notre dame",
                "new york"
            ]
        );
        assert_eq!(phrases(&speller, 'n')[0].1, ["Noah Harari", "noah harari"]);
        assert_eq!(
            phrases(&speller, 'z'),
            [("zoe quill", vec!["Zo\u{eb} Quill", "Zoe-Quill"])]
        );
        assert_eq!(
            phrases(&speller, '\u{201c}'),
            [(
                "\u{201c}ka\u{2026} dor\u{201d}",
                vec!["\u{201c}Ka\u{2026} Dor\u{201d}"]
            )]
        );
        assert_eq!(
            phrases(&speller, 'a'),
            [("ash\u{2028}fall rise", vec!["Ash\u{2028}Fall Rise"])]
        );
        assert!(phrases(&speller, 'h').is_empty());
        assert!(phrases(&speller, 'm').is_empty());
        assert!(phrases(&speller, 'b').is_empty());

        assert_eq!(
            without_spaces(&speller, "o\u{2019}brienhall"),
            Some(vec!["O\u{2019}Brien Hall"])
        );
        assert_eq!(
            without_spaces(&speller, "noahharari"),
            Some(vec!["Noah Harari", "noah harari"])
        );
        assert_eq!(without_spaces(&speller, "o\u{2019}brien hall"), None);
        assert_eq!(without_spaces(&speller, "harari"), None);
        assert_eq!(speller.len(), 11);
    }

    #[test]
    fn spellings_without_spaces_follow_line_order_across_keys() {
        let speller =
            MultitokenSpeller::from_lines(["Ab Cd", "A Bcd", "ab cd", "Abc D"].map(String::from));
        assert_eq!(
            without_spaces(&speller, "abcd"),
            Some(vec!["Ab Cd", "A Bcd", "ab cd", "Abc D"])
        );
    }

    #[test]
    fn loaded_buffers_have_no_spare_capacity() {
        let speller = MultitokenSpeller::from_lines(
            (0..300_000).map(|i| format!("\u{201c}Zo\u{eb} {}\u{2026} Quill\u{2014}Hall", i)),
        );
        let mut loaded: MultitokenSpeller =
            bincode::deserialize(&bincode::serialize(&speller).unwrap()).unwrap();
        // Serde preallocates a sequence up to 1 MiB and grows it by doubling past that, so
        // these Vecs of u32 (over 1 MiB each) arrive with spare capacity.
        assert!(loaded.spellings.ends.len() * 4 > 1 << 20);
        assert!(loaded.spellings.ends.capacity() > loaded.spellings.ends.len());
        loaded.shrink_to_fit();
        assert_eq!(
            loaded.spellings.text.capacity(),
            loaded.spellings.text.len()
        );
        assert_eq!(
            loaded.spellings.ends.capacity(),
            loaded.spellings.ends.len()
        );
        assert_eq!(loaded.keys.text.capacity(), loaded.keys.text.len());
        assert_eq!(loaded.keys.ends.capacity(), loaded.keys.ends.len());
        assert_eq!(loaded.phrase_ends.capacity(), loaded.phrase_ends.len());
        assert_eq!(
            loaded.without_spaces_ids.capacity(),
            loaded.without_spaces_ids.len()
        );
        assert_eq!(
            loaded.without_spaces.capacity(),
            loaded.without_spaces.len()
        );
        assert_eq!(
            without_spaces(&loaded, "\u{201c}zoe299999\u{2026}quill\u{2014}hall"),
            Some(vec!["\u{201c}Zo\u{eb} 299999\u{2026} Quill\u{2014}Hall"])
        );
    }

    #[test]
    fn survives_a_serialization_round_trip() {
        let speller = MultitokenSpeller::from_lines(
            ["Yuval Noah Harari", "Zo\u{eb}\u{2014}Quill Hall"].map(String::from),
        );
        let loaded: MultitokenSpeller =
            bincode::deserialize(&bincode::serialize(&speller).unwrap()).unwrap();
        assert_eq!(
            phrases(&loaded, 'z'),
            [("zoe\u{2014}quill hall", vec!["Zo\u{eb}\u{2014}Quill Hall"])]
        );
        assert_eq!(
            without_spaces(&loaded, "yuvalnoahharari"),
            Some(vec!["Yuval Noah Harari"])
        );
    }
}
