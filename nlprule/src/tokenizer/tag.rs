//! A dictionary-based tagger.

use crate::types::*;
use bimap::BiMap;
use fst::{Map, Streamer};
use log::error;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, iter::FusedIterator};

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(transparent)]
pub(crate) struct WordIdInt(u32);

#[allow(dead_code)] // used in compile module
impl WordIdInt {
    pub(crate) fn from_value_unchecked(value: u32) -> Self {
        WordIdInt(value)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(transparent)]
pub(crate) struct PosIdInt(u16);

impl PosIdInt {
    #[allow(dead_code)] // used in compile module
    pub(crate) fn from_value_unchecked(value: u16) -> Self {
        PosIdInt(value)
    }

    pub fn value(&self) -> u16 {
        self.0
    }
}

/// A potentially identified word. If it is identified as a known word, many optimizations can be applied.
#[derive(Clone, PartialEq)]
pub struct WordId<'t>(pub(crate) Cow<'t, str>, pub(crate) Option<WordIdInt>);

impl<'t> fmt::Debug for WordId<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id_str = if let Some(id) = self.1 {
            id.0.to_string()
        } else {
            "none".into()
        };

        write!(f, "{:?}<id={}>", self.0, id_str)
    }
}

impl<'t> WordId<'t> {
    pub(crate) fn to_owned_id(&self) -> owned::WordId {
        owned::WordId(self.0.to_string(), self.1)
    }

    pub(crate) fn id(&self) -> &Option<WordIdInt> {
        &self.1
    }

    pub(crate) fn empty() -> Self {
        WordId("".into(), Some(WordIdInt(0)))
    }

    /// Gets the word as string.
    pub fn as_str(&'t self) -> &'t str {
        self.0.as_ref()
    }
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum SpecialPos {
    None = 0,
    Unknown = 1,
    SentStart = 2,
    SentEnd = 3,
}

impl SpecialPos {
    pub fn as_str(&self) -> &'static str {
        match &self {
            SpecialPos::None => "",
            SpecialPos::Unknown => "UNKNOWN",
            SpecialPos::SentStart => "SENT_START",
            SpecialPos::SentEnd => "SENT_END",
        }
    }

    #[allow(dead_code)] // used in compile module
    pub fn iter() -> impl Iterator<Item = &'static str> + 'static {
        [
            SpecialPos::None,
            SpecialPos::Unknown,
            SpecialPos::SentStart,
            SpecialPos::SentEnd,
        ]
        .iter()
        .map(|pos| pos.as_str())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum InnerPosId<'t> {
    Normal(&'t str, PosIdInt),
    Special(SpecialPos),
}

/// An identified part-of-speech tag. POS tags are treated as a closed set so every POS tag is identified.
#[derive(Clone, Copy, PartialEq)]
pub struct PosId<'t> {
    inner: InnerPosId<'t>,
}

impl<'t> fmt::Debug for PosId<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}<id={}>", self.as_str(), self.id().0)
    }
}

impl<'t> PosId<'t> {
    pub(crate) fn regular(text: &'t str, id: PosIdInt) -> Self {
        PosId {
            inner: InnerPosId::Normal(text, id),
        }
    }

    pub(crate) fn special(special: SpecialPos) -> Self {
        PosId {
            inner: InnerPosId::Special(special),
        }
    }

    /// Converts this ID to an owned ID.
    pub fn to_owned_id(&self) -> owned::PosId {
        owned::PosId(self.as_str().to_string(), self.id())
    }

    pub(crate) fn id(&self) -> PosIdInt {
        match &self.inner {
            InnerPosId::Normal(_, id) => *id,
            InnerPosId::Special(special) => PosIdInt(*special as u16),
        }
    }

    /// Gets the part-of-speech as string.
    pub fn as_str(&self) -> &'t str {
        match &self.inner {
            InnerPosId::Normal(text, _) => *text,
            InnerPosId::Special(special) => special.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaggerLangOptions {
    /// Whether to use a heuristic to split potential compound words.
    pub use_compound_split_heuristic: bool,
    /// Whether to always add tags for a lowercase version of the word when assigning part-of-speech tags.
    pub always_add_lower_tags: bool,
    /// Used part-of-speech tags which are not in the tagger dictionary.
    pub extra_tags: Vec<String>,
    /// Whether to retain the last tag if disambiguation leads to an empty tag.
    /// Language-specific in LT so it has to be an option.
    pub retain_last: bool,
}

impl Default for TaggerLangOptions {
    fn default() -> Self {
        TaggerLangOptions {
            use_compound_split_heuristic: false,
            always_add_lower_tags: false,
            extra_tags: Vec::new(),
            retain_last: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct TaggerFields {
    pub(crate) tag_fst: Vec<u8>,
    /// Reverse index for synthesis: lemma bytes + one index byte -> packed (word id, pos id).
    pub(crate) synth_fst: Vec<u8>,
    pub(crate) word_store_fst: Vec<u8>,
    pub(crate) tag_store: BiMap<String, PosIdInt>,
    pub(crate) lang_options: TaggerLangOptions,
    /// Kept for serialized-format compatibility. The FSTs are queried directly at load, so the
    /// anchors (used by the old parallel inflation code) are no longer read.
    #[allow(dead_code)]
    pub(crate) anchors: Vec<String>,
}

impl From<Tagger> for TaggerFields {
    fn from(tagger: Tagger) -> Self {
        let tag_fst = tagger.tag_fst.into_fst().as_bytes().to_vec();
        let synth_fst = tagger.synth_fst.into_fst().as_bytes().to_vec();
        let word_store_fst = tagger.word_store_fst.into_fst().as_bytes().to_vec();

        TaggerFields {
            tag_fst,
            synth_fst,
            word_store_fst,
            tag_store: tagger.tag_store,
            lang_options: tagger.lang_options,
            anchors: Vec::new(),
        }
    }
}

impl From<TaggerFields> for Tagger {
    fn from(data: TaggerFields) -> Self {
        let word_store_fst = Map::new(data.word_store_fst).unwrap();
        let tag_fst = Map::new(data.tag_fst).unwrap();
        let synth_fst = Map::new(data.synth_fst).unwrap();

        // Build a compact word-id -> word index: every word concatenated into a single buffer
        // plus the start offset of each word (and a trailing sentinel with the buffer length).
        // Word ids are dense `0..len`. The FST streams in key order, not id order, so two passes
        // are used to lay the words out in id order without allocating a `String` per word.
        let n = word_store_fst.len();
        let mut word_offsets = vec![0u32; n + 1];

        {
            let mut stream = word_store_fst.stream();
            while let Some((key, value)) = stream.next() {
                word_offsets[value as usize + 1] = key.len() as u32;
            }
        }

        let mut total = 0u32;
        for i in 0..n {
            let len = word_offsets[i + 1];
            word_offsets[i] = total;
            total += len;
        }
        word_offsets[n] = total;

        let mut word_buf = vec![0u8; total as usize];
        {
            let mut stream = word_store_fst.stream();
            while let Some((key, value)) = stream.next() {
                let start = word_offsets[value as usize] as usize;
                word_buf[start..start + key.len()].copy_from_slice(key);
            }
        }

        let word_buf = String::from_utf8(word_buf).expect("word store keys are valid utf-8.");

        Tagger {
            word_store_fst,
            tag_fst,
            synth_fst,
            tag_store: data.tag_store,
            lang_options: data.lang_options,
            word_buf,
            word_offsets,
        }
    }
}

pub(crate) enum AddLower {
    Always,
    IfEmpty,
    Never,
}

struct RawTagIter<'a> {
    tagger: &'a Tagger,
    tags: std::vec::IntoIter<(WordIdInt, PosIdInt)>,
}

impl<'a> Iterator for RawTagIter<'a> {
    type Item = WordData<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let tagger = self.tagger;
        self.tags.next().map(move |(lemma_id, pos_id)| {
            WordData::new(
                tagger.id_word(tagger.str_for_word_id(&lemma_id).into()),
                tagger.id_tag(tagger.str_for_pos_id(&pos_id)),
            )
        })
    }
}

impl<'a> FusedIterator for RawTagIter<'a> {}
impl<'a> ExactSizeIterator for RawTagIter<'a> {
    fn len(&self) -> usize {
        self.tags.len()
    }
}

struct StrictTagIter<'a> {
    tag_iter: RawTagIter<'a>,
    lower_tag_iter: Option<RawTagIter<'a>>,
}

impl<'a> Iterator for StrictTagIter<'a> {
    type Item = WordData<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.tag_iter.next().or_else(|| {
            self.lower_tag_iter
                .as_mut()
                .and_then(|lower_iter| lower_iter.next())
        })
    }
}

impl<'a> FusedIterator for StrictTagIter<'a> {}
impl<'a> ExactSizeIterator for StrictTagIter<'a> {
    fn len(&self) -> usize {
        self.tag_iter.len()
            + self
                .lower_tag_iter
                .as_ref()
                .map_or(0, |lower_iter| lower_iter.len())
    }
}

/// An iterator over [WordData].
pub struct TagIter<'a> {
    tagger: &'a Tagger,
    tag_iter: StrictTagIter<'a>,
    prefix: Option<&'a str>,
}

impl<'a> Iterator for TagIter<'a> {
    type Item = WordData<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.tag_iter.next().map(|data| {
            let lemma = if let Some(prefix) = &self.prefix {
                self.tagger
                    .id_word(format!("{}{}", prefix, data.lemma().as_str().to_lowercase()).into())
            } else {
                data.lemma().clone()
            };

            WordData::new(lemma, *data.pos())
        })
    }
}

impl<'a> FusedIterator for TagIter<'a> {}
impl<'a> ExactSizeIterator for TagIter<'a> {
    fn len(&self) -> usize {
        self.tag_iter.len()
    }
}

/// The lexical tagger.
/// Created from a dictionary that looks like this:
///
/// ```raw
/// actualize   actualize   VB
/// actualize   actualize   VBP
/// actualized  actualize   VBD
/// actualized  actualize   VBN
/// actualizes  actualize   VBZ
/// actualizing actualize   VBG
/// actually    actually    RB
/// ```
///
/// i.e. one word (left) associated with one or more pairs of lemma (middle) and POS (part-of-speech) tag (right).
/// From this structure, the tagger must be able to look up:
/// 1. lemma and pos by word: all lemmas and POS tags associated with a given word.
///
/// (1) is called extensively (at least once for every word) so it has to be as fast as possible.
///
/// ## Implementation
///
/// The tagger keeps the two FSTs from the serialized format and queries them directly:
/// - A POS bimap: A bimap assigning each POS tag a 16-bit ID. POS tags are a closed set, so there is an entry
/// for every tag in the bimap. This allows e.g. storing a set of IDs of matching tags for a regex instead of actually
/// evaluating it in the matcher logic.
/// - A word FST: maps each known word to a 32-bit ID. Words are not a closed set, so if an entry in this map
/// does not exist it only means that the word is not known to nlprule. Still, the map can be used for optimizations similar
/// to the POS bimap. The word FST also stores lemmas since there is often a large overlap between known words
/// and known lemmas.
///
/// These are queried directly (instead of being inflated into hash maps at load) so memory stays close to the
/// size of the binary while retaining fast lookup. A compact id -> word index (one concatenated buffer plus start
/// offsets) supports reverse lookups.
///
/// The tag FST associates a Word ID with *multiple* pairs of `(lemma_id, pos_id)` where the
/// ID for the lemma is a regular 32-bit Word ID; keys are the word bytes followed by one index byte.
///
/// The synth FST is the reverse index used for inflection synthesis: keys are the lemma bytes
/// followed by one index byte and values are packed `(word_id, pos_id)` pairs. It answers "given a
/// lemma, which words carry it and with which POS tags".
#[derive(Default, Serialize, Deserialize, Clone)]
#[serde(from = "TaggerFields", into = "TaggerFields")]
pub struct Tagger {
    pub(crate) word_store_fst: Map<Vec<u8>>,
    pub(crate) tag_fst: Map<Vec<u8>>,
    pub(crate) synth_fst: Map<Vec<u8>>,
    pub(crate) tag_store: BiMap<String, PosIdInt>,
    pub(crate) lang_options: TaggerLangOptions,
    /// All words concatenated in id order.
    pub(crate) word_buf: String,
    /// Start offset of each word id in `word_buf`, plus a trailing sentinel (`word_buf.len()`).
    pub(crate) word_offsets: Vec<u32>,
}

impl Tagger {
    /// Directly looks up the given word in the tag FST and returns
    /// corresponding [WordData].
    fn get_raw(&self, word: &str) -> RawTagIter {
        // Keys are the word bytes followed by exactly one index byte. The index bytes of a word
        // are contiguous `0..n` (the compiler assigns them with `enumerate`), so querying the FST
        // per index until the first miss yields exactly the same tags in the same order as a
        // prefix range scan would. A range scan is avoided deliberately: for a word that is a
        // prefix of many others (e.g. `a`, `i`, `the`) the `word + [0] ..= word + [254]` range
        // walks the whole subtree of longer words before filtering them out by key length, which
        // costs O(FST section) per lookup.
        let mut tags = Vec::new();
        let word_bytes = word.as_bytes();
        let mut key = Vec::with_capacity(word_bytes.len() + 1);
        key.extend_from_slice(word_bytes);
        key.push(0);

        let mut index: u8 = 0;
        loop {
            *key.last_mut().unwrap() = index;
            match self.tag_fst.get(&key) {
                Some(value) => {
                    let value_bytes = value.to_be_bytes();
                    let lemma_id = WordIdInt(u32::from_be_bytes([
                        value_bytes[0],
                        value_bytes[1],
                        value_bytes[2],
                        value_bytes[3],
                    ]));
                    let pos_id = PosIdInt(u16::from_be_bytes([value_bytes[6], value_bytes[7]]));
                    tags.push((lemma_id, pos_id));
                }
                None => break,
            }
            if index == 254 {
                break;
            }
            index += 1;
        }

        RawTagIter {
            tagger: &self,
            tags: tags.into_iter(),
        }
    }

    /// Looks up all `(word, pos)` pairs that have the given lemma in the synth FST.
    ///
    /// Keys are the lemma bytes followed by exactly one index byte. As with the tag FST the index
    /// bytes of a lemma are contiguous `0..n`, so querying per index until the first miss yields
    /// exactly the same pairs a prefix range scan would, without walking the subtree of longer
    /// lemmas.
    fn get_synth_raw<'a>(&'a self, lemma: &str) -> Vec<(&'a str, &'a str)> {
        let lemma_bytes = lemma.as_bytes();
        let mut key = Vec::with_capacity(lemma_bytes.len() + 1);
        key.extend_from_slice(lemma_bytes);
        key.push(0);

        let mut out = Vec::new();
        let mut index: u8 = 0;
        loop {
            *key.last_mut().unwrap() = index;
            match self.synth_fst.get(&key) {
                Some(value) => {
                    let value_bytes = value.to_be_bytes();
                    let word_id = WordIdInt(u32::from_be_bytes([
                        value_bytes[0],
                        value_bytes[1],
                        value_bytes[2],
                        value_bytes[3],
                    ]));
                    let pos_id = PosIdInt(u16::from_be_bytes([value_bytes[6], value_bytes[7]]));
                    out.push((self.str_for_word_id(&word_id), self.str_for_pos_id(&pos_id)));
                }
                None => break,
            }
            if index == 254 {
                break;
            }
            index += 1;
        }
        out
    }

    /// Synthesizes the word forms of `lemma` whose part-of-speech tag is accepted by `pos_match`.
    ///
    /// This is the reverse of [Tagger::get_tags]: given a lemma and a target POS tag (or POS tag
    /// regex), it returns the word forms in the dictionary carrying that lemma and a matching tag.
    /// The result is de-duplicated and sorted, mirroring LanguageTool's `TreeSet`-based synthesis.
    pub(crate) fn synthesize<F>(&self, lemma: &str, mut pos_match: F) -> Vec<&str>
    where
        F: FnMut(&str) -> bool,
    {
        let mut out: Vec<&str> = self
            .get_synth_raw(lemma)
            .into_iter()
            .filter_map(|(word, pos)| if pos_match(pos) { Some(word) } else { None })
            .collect();

        out.sort_unstable();
        out.dedup();
        out
    }

    #[allow(dead_code)] // used in compile module
    pub(crate) fn lang_options(&self) -> &TaggerLangOptions {
        &self.lang_options
    }

    /// Same as [get_tags] but optionally:
    /// - Adds tags for the lower variant of the word (if `add_lower` is `AddLower::Always`).
    /// - Adds tags for the lower variant of the word if no [WordData] is found otherwise.
    /// (if `add_lower` is `AddLower::IfEmpty`).
    fn get_strict_tags(&self, word: &str, add_lower: AddLower) -> StrictTagIter {
        let tag_iter = self.get_raw(&word);
        let lower_tag_iter = if (matches!(add_lower, AddLower::Always)
            || (matches!(add_lower, AddLower::IfEmpty) && tag_iter.len() == 0))
        {
            let lower = word.to_lowercase();

            if (word != lower)
                && (crate::utils::is_title_case(word) || crate::utils::is_uppercase(word))
            {
                Some(self.get_raw(&lower))
            } else {
                None
            }
        } else {
            None
        };

        StrictTagIter {
            tag_iter,
            lower_tag_iter,
        }
    }

    #[allow(dead_code)] // used by compile module
    pub(crate) fn tag_store(&self) -> &BiMap<String, PosIdInt> {
        &self.tag_store
    }

    #[allow(dead_code)] // used by compile module
    pub(crate) fn word_store(&self) -> impl Iterator<Item = (String, WordIdInt)> + '_ {
        let mut stream = self.word_store_fst.stream();
        std::iter::from_fn(move || {
            stream.next().map(|(key, value)| {
                (
                    std::str::from_utf8(key)
                        .expect("word store keys are valid utf-8.")
                        .to_owned(),
                    WordIdInt(value as u32),
                )
            })
        })
    }

    /// Gets the string associated with a word ID.
    pub(crate) fn str_for_word_id(&self, id: &WordIdInt) -> &str {
        let start = self.word_offsets[id.0 as usize] as usize;
        let end = self.word_offsets[id.0 as usize + 1] as usize;
        &self.word_buf[start..end]
    }

    /// Gets the string associated with a POS ID.
    pub(crate) fn str_for_pos_id(&self, id: &PosIdInt) -> &str {
        self.tag_store
            .get_by_right(id)
            .expect("only valid pos ids are created")
    }

    /// Tags the given string representation of a part-of-speech tag.
    /// Part-of-speech tags are treated as a closed set so each valid part-of-speech tag will get a numerical id.
    pub fn id_tag<'a>(&self, tag: &'a str) -> PosId<'a> {
        PosId::regular(
            tag,
            *self.tag_store.get_by_left(tag).unwrap_or_else(|| {
                error!(
                    "'{}' not found in tag store, please add it to the `extra_tags`. Using UNKNOWN instead.",
                    tag
                );
                self.tag_store.get_by_left("UNKNOWN").expect("UNKNOWN tag must exist in tag store")
            }),
        )
    }

    /// Tags the given text.
    /// Unknown words will not get a numerical id.
    pub fn id_word<'a>(&self, text: Cow<'a, str>) -> WordId<'a> {
        let id = self
            .word_store_fst
            .get(text.as_ref())
            .map(|v| WordIdInt(v as u32));
        WordId(text, id)
    }

    /// Get the tags and lemmas (as [WordData]) for the given word.
    ///
    /// # Arguments
    /// * `word`: The word to lookup data for.
    /// * `add_lower`: Whether to add data for the lowercase variant of the word.
    ///     If `None`, will be set according to the language options.
    /// * `use_compound_split_heuristic`: Whether to use a heuristic to split compound words.
    ///     If `None`, will be set according to the language options.
    /// If true, will attempt to find tags for words which are longer than some cutoff and unknown by looking up tags
    /// for substrings from left to right until tags are found or a minimum length reached.
    pub fn get_tags_with_options<'a>(
        &'a self,
        word: &'a str,
        add_lower: Option<bool>,
        use_compound_split_heuristic: Option<bool>,
    ) -> TagIter<'a> {
        let add_lower = add_lower.unwrap_or(self.lang_options.always_add_lower_tags);
        let use_compound_split_heuristic =
            use_compound_split_heuristic.unwrap_or(self.lang_options.use_compound_split_heuristic);

        let mut tags = TagIter {
            tagger: &self,
            tag_iter: self.get_strict_tags(
                word,
                if add_lower {
                    AddLower::Always
                } else {
                    AddLower::IfEmpty
                },
            ),
            prefix: None,
        };

        // compound splitting heuristic, seems to work reasonably well
        if use_compound_split_heuristic && tags.len() == 0 {
            let n_chars = word.chars().count() as isize;

            if n_chars >= 7 {
                let indices = word
                    .char_indices()
                    .take(std::cmp::max(n_chars - 4, 0) as usize)
                    .skip(1)
                    .map(|x| x.0);
                // the word always has at least one char if the above condition is satisfied
                // but semantically this is false if no char exists
                let starts_with_uppercase = word.chars().next().map_or(false, |x| x.is_uppercase());

                for i in indices {
                    let next = if starts_with_uppercase {
                        crate::utils::apply_to_first(&word[i..], |c| c.to_uppercase().collect())
                    } else {
                        word[i..].to_string()
                    };

                    let next_tags = self.get_strict_tags(
                        &next,
                        if add_lower {
                            AddLower::Always
                        } else {
                            AddLower::Never
                        },
                    );

                    if next_tags.len() != 0 {
                        tags = TagIter {
                            tagger: &self,
                            tag_iter: next_tags,
                            prefix: Some(&word[..i]),
                        };
                        break;
                    }
                }
            }
        }

        tags
    }

    /// Get the tags and lemmas (as [WordData][crate::types::WordData]) for the given word
    /// using the default options of the tagger.
    ///
    /// # Arguments
    /// * `word`: The word to lookup data for.
    pub fn get_tags<'a>(&'a self, word: &'a str) -> TagIter<'a> {
        self.get_tags_with_options(word, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_value(word_id: u32, pos_id: u16) -> u64 {
        let word = word_id.to_be_bytes();
        let pos = pos_id.to_be_bytes();
        u64::from_be_bytes([word[0], word[1], word[2], word[3], 0, 0, pos[0], pos[1]])
    }

    /// Builds a tagger with just enough data to exercise [Tagger::synthesize].
    fn synthesize_tagger() -> Tagger {
        // word ids: 0 "", 1 child, 2 children, 3 go, 4 gone, 5 run
        let words = ["", "child", "children", "go", "gone", "run"];
        let word_store_items: Vec<(Vec<u8>, u64)> = words
            .iter()
            .enumerate()
            .map(|(i, word)| (word.as_bytes().to_vec(), i as u64))
            .collect();
        let word_store_fst = Map::from_iter(word_store_items)
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        // tag ids: 0 "", 1 UNKNOWN, 2 SENT_START, 3 SENT_END, 4 NNS, 5 VBN, 6 VB
        let mut tag_store = BiMap::new();
        for (i, tag) in ["", "UNKNOWN", "SENT_START", "SENT_END", "NNS", "VBN", "VB"]
            .iter()
            .enumerate()
        {
            tag_store.insert(tag.to_string(), PosIdInt::from_value_unchecked(i as u16));
        }

        let mut synth_items: Vec<(Vec<u8>, u64)> = vec![
            (b"child\x00".to_vec(), pack_value(2, 4)), // children / NNS
            (b"go\x00".to_vec(), pack_value(4, 5)),    // gone / VBN
            (b"run\x00".to_vec(), pack_value(5, 6)),   // run / VB
            (b"run\x01".to_vec(), pack_value(5, 4)),   // run / NNS (same word, second tag)
        ];
        synth_items.sort_by(|(a, _), (b, _)| a.cmp(b));
        let synth_fst = Map::from_iter(synth_items)
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        let tag_fst = Map::from_iter(Vec::<(Vec<u8>, u64)>::new())
            .unwrap()
            .into_fst()
            .as_bytes()
            .to_vec();

        TaggerFields {
            tag_fst,
            synth_fst,
            word_store_fst,
            tag_store,
            lang_options: TaggerLangOptions::default(),
            anchors: Vec::new(),
        }
        .into()
    }

    #[test]
    fn synthesize_returns_inflected_forms() {
        let tagger = synthesize_tagger();

        assert_eq!(tagger.synthesize("go", |pos| pos == "VBN"), vec!["gone"]);
        assert_eq!(
            tagger.synthesize("child", |pos| pos == "NNS"),
            vec!["children"]
        );
        // a regex-like predicate over the tag selects every matching form; duplicate words
        // (one per matching tag) are de-duplicated
        assert_eq!(
            tagger.synthesize("run", |pos| pos == "VB" || pos == "NNS"),
            vec!["run"]
        );
        // unknown lemmas and non-matching tags yield nothing
        assert!(tagger.synthesize("walked", |_| true).is_empty());
        assert!(tagger.synthesize("child", |pos| pos == "VBN").is_empty());
    }
}
