//! A Chunker ported from [OpenNLP](https://opennlp.apache.org/).

use half::bf16;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::{cmp::Ordering, collections::BinaryHeap};

use crate::types::{DefaultHashMap, DefaultHasher, IncompleteSentence};

fn softmax(vec: &mut Vec<f32>) {
    for x in vec.iter_mut() {
        *x = x.exp();
    }

    let sum = vec.iter().fold(0., |a, b| a + b);
    for x in vec.iter_mut() {
        *x /= sum;
    }
}

#[derive(Debug, Clone)]
struct Sequence<'a> {
    outcomes: Vec<&'a str>,
}

struct Node {
    outcome: usize,
    parent: Option<usize>,
}

struct Beam {
    log_prob: f32,
    parent: Option<usize>,
    outcome: usize,
}

impl Eq for Beam {}

impl PartialEq for Beam {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Ord for Beam {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(&other).unwrap()
    }
}

impl PartialOrd for Beam {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        other.log_prob.partial_cmp(&self.log_prob)
    }
}

impl<'a> Sequence<'a> {
    #[inline]
    fn outcomes(&self) -> &[&'a str] {
        &self.outcomes
    }
}

pub(crate) mod hash {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    pub fn hash_str(string: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        string.hash(&mut hasher);
        hasher.finish()
    }

    pub fn hash_slice(slice: &[&str]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for s in slice.iter() {
            hasher.write(s.as_bytes());
        }
        // This is intentional.
        // See https://doc.rust-lang.org/stable/src/core/hash/mod.rs.html#598-603
        "".hash(&mut hasher);
        hasher.finish()
    }

    /// Hashes the byte concatenation of `parts` exactly like [`hash_slice`] hashes the same
    /// strings and like [`hash_str`] hashes their join, without allocating the joined string.
    /// `hash_str(s) == hash_parts(&[s])` and `hash_slice(parts) == hash_parts(parts)`.
    pub fn hash_parts(parts: &[&str]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for part in parts.iter() {
            hasher.write(part.as_bytes());
        }
        hasher.write_u8(0xff);
        hasher.finish()
    }

    /// Like [`hash_parts`] but appends the UTF-8 bytes of `chars` before the terminator, so
    /// `hash_slice(&[a, b, &chars.iter().collect::<String>()]) ==
    /// hash_parts_and_chars(&[a, b], chars)` without building the string.
    pub fn hash_parts_and_chars(parts: &[&str], chars: &[char]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for part in parts.iter() {
            hasher.write(part.as_bytes());
        }
        let mut buf = [0u8; 4];
        for c in chars.iter() {
            hasher.write(c.encode_utf8(&mut buf).as_bytes());
        }
        hasher.write_u8(0xff);
        hasher.finish()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn same_hash_if_same_join() {
            let arr = &["ab", "cde", "fg"];
            let string = "abcdefg";

            assert_eq!(hash_slice(arr), hash_str(string));
        }

        #[test]
        fn parts_hash_matches_string_hash() {
            assert_eq!(hash_parts(&["w0=", "hello"]), hash_str("w0=hello"));
            assert_eq!(hash_parts(&["a", "b", "c"]), hash_slice(&["a", "b", "c"]));
            assert_eq!(hash_parts(&[""]), hash_str(""));
        }

        #[test]
        fn chars_hash_matches_collected_string_hash() {
            let chars: Vec<char> = "héllo—!".chars().collect();
            for i in 0..=chars.len() {
                let collected: String = chars[..i].iter().collect();
                assert_eq!(
                    hash_parts_and_chars(&["p="], &chars[..i]),
                    hash_slice(&["p=", &collected])
                );
            }
            assert_eq!(
                hash_parts_and_chars(&["p21="], &[chars[0], chars[1]]),
                hash_slice(&["p21=", &chars[0].to_string(), &chars[1].to_string()])
            );
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ModelFields {
    outcome_labels: Vec<String>,
    pmap: Vec<(u64, u8)>,
    outcomes: Vec<u8>,
    parameters: Vec<bf16>,
}

/// One entry of a [Pmap]. `offset` addresses the flat `outcomes`/`parameters` arrays and fits in
/// `u32` (the largest model has fewer than 700k entries); `length` fits in `u8` because that is
/// the type the on-disk format already stores it in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct PmapEntry {
    key: u64,
    offset: u32,
    length: u8,
}

/// A context map stored as a `Vec` sorted by key with binary-search lookup instead of a
/// `HashMap`. The serialized (on-disk) form is a `Vec<(u64, u8)>` sorted by offset; only this
/// in-memory form, built at load, is different.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Pmap {
    /// Sorted by `key`; keys are unique.
    entries: Vec<PmapEntry>,
}

impl Pmap {
    /// Builds a `Pmap` from `(key, (offset, length))` pairs, asserting the bounds the on-disk
    /// format already relies on. Used by the compiler, where offsets are assigned as the flat
    /// arrays are extended.
    pub(crate) fn from_pairs<I: IntoIterator<Item = (u64, (usize, usize))>>(pairs: I) -> Self {
        let mut entries: Vec<_> = pairs
            .into_iter()
            .map(|(key, (offset, length))| {
                assert!(length <= u8::MAX as usize);
                assert!(offset <= u32::MAX as usize);
                PmapEntry {
                    key,
                    offset: offset as u32,
                    length: length as u8,
                }
            })
            .collect();
        entries.sort_unstable_by_key(|entry| entry.key);
        Pmap { entries }
    }

    /// Builds a `Pmap` from the on-disk `(key, length)` list (in ascending-offset order),
    /// reproducing `HashMap::insert` semantics: for a repeated key the later (larger-offset)
    /// entry wins.
    fn from_disk<I: IntoIterator<Item = (u64, u8)>>(pmap: I) -> Self {
        let mut offset = 0usize;
        let mut entries: Vec<_> = pmap
            .into_iter()
            .map(|(key, length)| {
                assert!(
                    offset <= u32::MAX as usize,
                    "chunker pmap offset fits in u32"
                );
                let entry = PmapEntry {
                    key,
                    offset: offset as u32,
                    length,
                };
                offset += length as usize;
                entry
            })
            .collect();
        // Stable sort keeps ascending offset order within a run of equal keys, so the last entry
        // of each run is the one `HashMap::insert` would have kept.
        entries.sort_by_key(|entry| entry.key);
        let mut deduped: Vec<PmapEntry> = Vec::with_capacity(entries.len());
        for entry in entries {
            if deduped.last().map(|last| last.key) == Some(entry.key) {
                *deduped.last_mut().unwrap() = entry;
            } else {
                deduped.push(entry);
            }
        }
        Pmap { entries: deduped }
    }

    /// Looks up `key` in O(log n).
    #[inline]
    fn get(&self, key: u64) -> Option<(u32, u8)> {
        self.entries
            .binary_search_by_key(&key, |entry| entry.key)
            .ok()
            .map(|idx| {
                let entry = self.entries[idx];
                (entry.offset, entry.length)
            })
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The `(key, length)` pairs in on-disk order (ascending offset).
    fn into_sorted_by_offset(self) -> Vec<(u64, u8)> {
        let mut entries = self.entries;
        entries.sort_by_key(|entry| entry.offset);
        entries
            .into_iter()
            .map(|entry| (entry.key, entry.length))
            .collect()
    }
}

impl From<Model> for ModelFields {
    fn from(model: Model) -> Self {
        ModelFields {
            outcome_labels: model.outcome_labels,
            pmap: model.pmap.into_sorted_by_offset(),
            outcomes: model.outcomes,
            parameters: model.parameters,
        }
    }
}

impl From<ModelFields> for Model {
    fn from(data: ModelFields) -> Self {
        assert!(
            data.outcome_labels.len() <= u8::MAX as usize + 1,
            "chunker model has {} outcome labels; outcomes are stored as u8",
            data.outcome_labels.len()
        );
        for outcome in &data.outcomes {
            assert!(
                (*outcome as usize) < data.outcome_labels.len(),
                "chunker outcome {} is out of range for {} labels",
                outcome,
                data.outcome_labels.len()
            );
        }

        let ModelFields {
            outcome_labels,
            pmap,
            outcomes,
            parameters,
        } = data;

        Model {
            outcome_labels,
            pmap: Pmap::from_disk(pmap),
            outcomes,
            parameters,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(from = "ModelFields", into = "ModelFields")]
pub(crate) struct Model {
    pub(crate) outcome_labels: Vec<String>,
    /// Outcome indices into `outcome_labels`; the on-disk type is `u8` and the label count is
    /// asserted to fit it at load.
    pub(crate) outcomes: Vec<u8>,
    /// Kept in the on-disk half-precision type and widened to `f32` at use. Widening is exact,
    /// so this is bit-identical to converting at load.
    pub(crate) parameters: Vec<bf16>,
    pub(crate) pmap: Pmap,
}

impl Model {
    /// Adds one pmap entry's parameters to `prior`.
    #[inline]
    fn add_entry(&self, prior: &mut [f32], offset: u32, length: u8) {
        let start = offset as usize;
        let end = start + length as usize;
        let outcomes = &self.outcomes[start..end];
        let parameters = &self.parameters[start..end];

        for (idx, param) in outcomes.iter().zip(parameters.iter()) {
            prior[*idx as usize] += param.to_f32();
        }
    }

    fn eval(&self, context: &[u64]) -> Vec<f32> {
        self.eval_with_prior(self.prior(&[]), context)
    }

    fn prior(&self, keys: &[u64]) -> Vec<f32> {
        let mut prior =
            vec![(1. / (self.outcome_labels.len() as f32)).ln(); self.outcome_labels.len()];

        for (offset, length) in keys.iter().filter_map(|key| self.pmap.get(*key)) {
            self.add_entry(&mut prior, offset, length);
        }

        prior
    }

    /// Like [`Model::eval`], but starts from `prior`, the [`Model::prior`] of features that were
    /// looked up and summed earlier. The keys in `context` are still looked up.
    fn eval_with_prior(&self, mut prior: Vec<f32>, context: &[u64]) -> Vec<f32> {
        for (offset, length) in context.iter().filter_map(|x| self.pmap.get(*x)) {
            self.add_entry(&mut prior, offset, length);
        }

        softmax(&mut prior);
        prior
    }

    fn get_top_n(&self, probs: &[f32], n: usize, threshold: f32) -> Vec<(usize, f32, &str)> {
        let mut probs: Vec<_> = probs
            .iter()
            .enumerate()
            .filter(|(_, x)| **x >= threshold) // this is slightly different than LT behaviour but deemed okay as long as all tests pass
            .collect();
        probs.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        probs
            .into_iter()
            .map(|(i, p)| (i, *p, self.outcome_labels[i].as_str()))
            .take(n)
            .collect::<Vec<_>>()
    }

    fn beam_search<
        'm,
        S,
        H: Fn(&[&str], usize) -> u64,
        V: Fn(&[S], &[&str], usize, &str) -> bool,
        E: FnMut(&[&'m str], usize) -> Vec<f32>,
    >(
        &'m self,
        tokens: &[S],
        hash_fn: H,
        valid_fn: V,
        size: usize,
        mut eval_fn: E,
    ) -> Vec<Sequence<'m>> {
        if tokens.is_empty() {
            return vec![Sequence {
                outcomes: Vec::new(),
            }];
        }

        let mut nodes: Vec<Node> = Vec::new();
        let mut current: Vec<(f32, Option<usize>)> = vec![(0., None)];
        let mut prev: BinaryHeap<Beam> = BinaryHeap::new();
        let mut next: BinaryHeap<Beam> = BinaryHeap::new();

        let mut cache: DefaultHashMap<u64, (Vec<f32>, Vec<usize>)> = DefaultHashMap::default();

        for i in 0..tokens.len() {
            if i > 0 {
                let mut best = std::mem::take(&mut prev).into_sorted_vec();
                best.truncate(size);
                current.clear();
                for beam in best.drain(..) {
                    nodes.push(Node {
                        outcome: beam.outcome,
                        parent: beam.parent,
                    });
                    current.push((beam.log_prob, Some(nodes.len() - 1)));
                }
                prev = BinaryHeap::from(best);
            }

            for &(log_prob, seq_node) in current.iter() {
                let mut window: [&'m str; 2] = [""; 2];
                let mut len = 0;
                let mut node = seq_node;
                while let (true, Some(idx)) = (len < window.len(), node) {
                    window[window.len() - 1 - len] =
                        self.outcome_labels[nodes[idx].outcome].as_str();
                    node = nodes[idx].parent;
                    len += 1;
                }
                let outcomes = &window[window.len() - len..];

                let (scores, top_n) = &*cache.entry(hash_fn(outcomes, i)).or_insert_with(|| {
                    let scores = eval_fn(outcomes, i);
                    let mut top_n: Vec<usize> =
                        (0..scores.len()).filter(|&j| scores[j] >= 0.).collect();
                    top_n.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap());
                    if let Some(&last) = top_n.get(size - 1) {
                        let min = scores[last];
                        top_n.truncate(top_n.iter().take_while(|&&j| scores[j] >= min).count());
                    }
                    (scores, top_n)
                });

                for &j in top_n {
                    let pred = self.outcome_labels[j].as_str();
                    if valid_fn(tokens, outcomes, i, pred) {
                        next.push(Beam {
                            log_prob: log_prob + scores[j].ln(),
                            parent: seq_node,
                            outcome: j,
                        });
                    }
                }

                // if no advanced sequences, advance all valid to match OpenNLP behaviour
                if next.is_empty() {
                    for (j, p) in scores.iter().enumerate() {
                        let pred = self.outcome_labels[j].as_str();
                        if valid_fn(tokens, outcomes, i, pred) {
                            next.push(Beam {
                                log_prob: log_prob + p.ln(),
                                parent: seq_node,
                                outcome: j,
                            });
                        }
                    }
                }
            }

            std::mem::swap(&mut prev, &mut next);
            next.clear();
        }

        let mut out: Vec<_> = prev.drain().collect();
        out.sort();
        out.into_iter()
            .map(|beam| {
                let mut outcomes = vec![self.outcome_labels[beam.outcome].as_str()];
                let mut node = beam.parent;
                while let Some(idx) = node {
                    outcomes.push(self.outcome_labels[nodes[idx].outcome].as_str());
                    node = nodes[idx].parent;
                }
                outcomes.reverse();
                Sequence { outcomes }
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct MaxentTokenizer {
    pub(crate) model: Model,
}

impl MaxentTokenizer {
    fn add_char_context(key: &str, c: char, context: &mut Vec<u64>) {
        macro_rules! add {
            ($x: expr) => {
                context.push(hash::hash_slice(&[key, $x]));
            };
        }

        context.push(hash::hash_parts_and_chars(&[key, "="], &[c]));

        if c.is_alphabetic() {
            add!("_alpha");
            if c.is_uppercase() {
                add!("_caps");
            }
        } else if c.is_digit(10) {
            add!("_num");
        } else if c.is_whitespace() {
            add!("_ws");
        } else if ['.', '?', '!'].contains(&c) {
            add!("_eos");
        } else if ['`', '"', '\''].contains(&c) {
            add!("_quote");
        } else if ['[', '{', '('].contains(&c) {
            add!("_lp");
        } else if [']', '}', ')'].contains(&c) {
            add!("_rp");
        }
    }

    fn context(chars: &[char], i: usize) -> Vec<u64> {
        let mut context = Vec::new();

        context.push(hash::hash_parts_and_chars(&["p="], &chars[..i]));
        context.push(hash::hash_parts_and_chars(&["s="], &chars[i..]));

        if i > 0 {
            Self::add_char_context("p1", chars[i - 1], &mut context);
            if i > 1 {
                Self::add_char_context("p2", chars[i - 2], &mut context);
                context.push(hash::hash_parts_and_chars(
                    &["p21="],
                    &[chars[i - 2], chars[i - 1]],
                ));
            } else {
                context.push(hash::hash_str("p2=bok"));
            }
            context.push(hash::hash_parts_and_chars(
                &["p1f1="],
                &[chars[i - 1], chars[i]],
            ));
        } else {
            context.push(hash::hash_str("b1=bok"));
        }

        Self::add_char_context("f1", chars[i], &mut context);
        if i + 1 < chars.len() {
            Self::add_char_context("f2", chars[i + 1], &mut context);
            context.push(hash::hash_parts_and_chars(
                &["f12="],
                &[chars[i], chars[i + 1]],
            ));
        } else {
            context.push(hash::hash_str("f2=bok"));
        }

        if chars[0] == '&' && chars[chars.len() - 1] == ';' {
            context.push(hash::hash_str("cc")); // character code
        }

        // TODO: add abbreviations
        if i == chars.len() - 1 && [].contains(&chars) {
            context.push(hash::hash_str("pabb"));
        }

        context
    }

    fn tokenize<'a>(&self, text: &'a str) -> Vec<&'a str> {
        let mut tokens = Vec::new();
        let char_indices: Vec<_> = text.char_indices().collect();

        for pre_token in text.split_whitespace() {
            let byte_offset = pre_token.as_ptr() as usize - text.as_ptr() as usize;
            let char_start = char_indices
                .binary_search_by_key(&byte_offset, |(i, _)| *i)
                .unwrap();
            let byte_start = char_indices[char_start].0;

            let token_char_indices =
                &char_indices[char_start..char_start + pre_token.chars().count()];
            let token_chars: Vec<_> = token_char_indices.iter().map(|(_, c)| c).cloned().collect();

            if (token_char_indices.len() < 2)
                || pre_token.chars().all(|x| x.is_ascii_alphanumeric())
            {
                tokens.push(pre_token);
            } else {
                let mut start = byte_start;

                for i in 1..token_char_indices.len() {
                    let context = Self::context(&token_chars, i);
                    let (_, _, best) = self.model.get_top_n(&self.model.eval(&context), 1, 0.5)[0];

                    if best == "T" {
                        tokens.push(&text[start..token_char_indices[i].0]);
                        start = token_char_indices[i].0;
                    }
                }

                tokens.push(&text[start..byte_start + pre_token.len()]);
            }
        }

        tokens
    }
}

/// The POS tag dictionary: a word plus the POS tags it is allowed to have. Stored as a `Vec`
/// sorted by word with binary-search lookup (instead of a `HashMap`), and the tag strings are
/// interned so a tag that occurs in many entries shares one allocation. The serialized (on-disk)
/// form is still `HashMap<String, Vec<String>>`; [TagDict] deserializes that wire format directly
/// into its sorted representation so no `HashMap` is built at load.
#[derive(Debug, Clone, Default)]
pub(crate) struct TagDict {
    /// Sorted by word; keys are unique.
    entries: Vec<(Box<str>, Box<[u16]>)>,
    /// Interned tag strings, indexed by the `u16` stored in `entries`.
    tags: Vec<Box<str>>,
    tag_ids: DefaultHashMap<Box<str>, u16>,
}

impl<'de> Deserialize<'de> for TagDict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TagDictVisitor;

        impl<'de> serde::de::Visitor<'de> for TagDictVisitor {
            type Value = TagDict;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map from words to their allowed POS tags")
            }

            fn visit_map<A>(self, mut map: A) -> Result<TagDict, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut raw: Vec<(Box<str>, Vec<Box<str>>)> = Vec::new();
                while let Some((word, tags)) = map.next_entry::<String, Vec<String>>()? {
                    raw.push((
                        word.into_boxed_str(),
                        tags.into_iter().map(String::into_boxed_str).collect(),
                    ));
                }
                Ok(TagDict::from_raw(raw))
            }
        }

        deserializer.deserialize_map(TagDictVisitor)
    }
}

impl TagDict {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Builds the sorted, interned representation from unsorted `(word, tags)` pairs. The word
    /// `Box<str>`s are reused; only the distinct tag strings are shared.
    fn from_raw(mut raw: Vec<(Box<str>, Vec<Box<str>>)>) -> Self {
        raw.sort_unstable_by(|a, b| a.0.cmp(&b.0));

        let mut tags: Vec<Box<str>> = Vec::new();
        let mut tag_ids: DefaultHashMap<Box<str>, u16> = DefaultHashMap::default();
        let mut entries = Vec::with_capacity(raw.len());

        for (word, allowed) in raw {
            let ids: Vec<u16> = allowed
                .into_iter()
                .map(|tag| {
                    *tag_ids.entry(tag).or_insert_with_key(|tag| {
                        let id = tags.len() as u16;
                        tags.push(tag.clone());
                        id
                    })
                })
                .collect();
            entries.push((word, ids.into_boxed_slice()));
        }

        TagDict {
            entries,
            tags,
            tag_ids,
        }
    }

    /// The tag ids allowed for `word`, or `None` if the word is not constrained.
    #[inline]
    fn get(&self, word: &str) -> Option<&[u16]> {
        self.entries
            .binary_search_by(|(w, _)| w.as_ref().cmp(word))
            .ok()
            .map(|idx| &*self.entries[idx].1)
    }

    /// The interned id of `tag`, if it appears in the dictionary.
    #[inline]
    fn tag_id(&self, tag: &str) -> Option<u16> {
        self.tag_ids.get(tag).copied()
    }
}

impl From<DefaultHashMap<String, Vec<String>>> for TagDict {
    fn from(map: DefaultHashMap<String, Vec<String>>) -> Self {
        TagDict::from_raw(
            map.into_iter()
                .map(|(word, tags)| {
                    (
                        word.into_boxed_str(),
                        tags.into_iter().map(String::into_boxed_str).collect(),
                    )
                })
                .collect(),
        )
    }
}

impl Serialize for TagDict {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_map(self.entries.iter().map(|(word, ids)| {
            let allowed: Vec<&str> = ids.iter().map(|id| &*self.tags[*id as usize]).collect();
            (&**word, allowed)
        }))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct MaxentPosTagger {
    pub(crate) model: Model,
    pub(crate) tagdict: TagDict,
}

impl MaxentPosTagger {
    fn get_suffixes_prefixes(string: &str) -> Vec<u64> {
        let chars: Vec<_> = string.chars().collect();
        let mut output = Vec::new();

        for i in 0..4 {
            let start = std::cmp::max((chars.len() as isize) - 1 - i, 0) as usize;
            output.push(hash::hash_parts_and_chars(&["suf="], &chars[start..]));
        }

        for i in 0..4 {
            output.push(hash::hash_parts_and_chars(
                &["pre="],
                &chars[..std::cmp::min(i + 1, chars.len())],
            ));
        }

        output
    }

    fn hash(tags: &[&str], i: usize) -> u64 {
        let mut s = DefaultHasher::default();
        if i >= 1 {
            tags[tags.len() - 1].hash(&mut s);
        }
        if i >= 2 {
            tags[tags.len() - 2].hash(&mut s);
        }
        i.hash(&mut s);
        s.finish()
    }

    fn static_context(tokens: &[&str], i: usize) -> Vec<u64> {
        let mut context = Vec::new();

        let lex = tokens[i];
        let next = if i + 1 < tokens.len() {
            tokens[i + 1]
        } else {
            "*SE*"
        };
        let nextnext = if i + 2 < tokens.len() {
            tokens[i + 2]
        } else {
            "*SE*"
        };
        let prev = if i >= 1 { tokens[i - 1] } else { "*SB*" };
        let prevprev = if i >= 2 { tokens[i - 2] } else { "*SB*" };

        context.push(hash::hash_str("default"));
        context.push(hash::hash_slice(&["w=", lex]));

        context.extend(Self::get_suffixes_prefixes(&lex));

        if lex.contains('-') {
            context.push(hash::hash_str("h"));
        }
        if lex.chars().any(|c| c.is_ascii_uppercase()) {
            context.push(hash::hash_str("c"));
        }
        if lex.chars().any(|c| c.is_ascii_digit()) {
            context.push(hash::hash_str("d"));
        }

        context.push(hash::hash_slice(&["p=", prev]));
        if prev != "*SB*" {
            context.push(hash::hash_slice(&["pp=", prevprev]));
        }

        context.push(hash::hash_slice(&["n=", next]));
        if next != "*SE*" {
            context.push(hash::hash_slice(&["nn=", nextnext]));
        }

        context
    }

    fn tag_context(tags: &[&str], i: usize) -> Vec<u64> {
        let mut context = Vec::with_capacity(2);
        if i >= 1 {
            let tagprev = tags[tags.len() - 1];
            context.push(hash::hash_slice(&["t=", tagprev]));

            if i >= 2 {
                let tagprevprev = tags[tags.len() - 2];
                context.push(hash::hash_slice(&["t2=", tagprevprev, ",", tagprev]));
            }
        }
        context
    }

    fn valid(&self, allowed: Option<&[u16]>, outcome: &str) -> bool {
        if let Some(allowed) = allowed {
            match self.tagdict.tag_id(outcome) {
                Some(id) => allowed.contains(&id),
                None => false,
            }
        } else {
            true
        }
    }

    fn tag(&self, tokens: &[&str]) -> Sequence {
        let allowed: Vec<Option<&[u16]>> = tokens.iter().map(|t| self.tagdict.get(t)).collect();
        let priors: Vec<Vec<f32>> = (0..tokens.len())
            .map(|i| self.model.prior(&Self::static_context(tokens, i)))
            .collect();
        self.model
            .beam_search(
                tokens,
                Self::hash,
                |_, _, i, outcome| self.valid(allowed[i], outcome),
                3,
                |tags, i| {
                    self.model
                        .eval_with_prior(priors[i].clone(), &Self::tag_context(tags, i))
                },
            )
            .remove(0)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct MaxentChunker {
    pub(crate) model: Model,
}

impl MaxentChunker {
    #[allow(clippy::manual_strip)]
    fn valid(&self, _input: &[(&str, &str)], outcomes: &[&str], _i: usize, outcome: &str) -> bool {
        if outcome.starts_with("I-") {
            if let Some(prev_outcome) = outcomes.iter().next_back() {
                // can byte index here because "I, B, E, -" are all 1 byte
                *prev_outcome != "O" && prev_outcome[2..] == outcome[2..]
            } else {
                false
            }
        } else {
            true
        }
    }

    fn hash(preds: &[&str], i: usize) -> u64 {
        let mut s = DefaultHasher::default();
        if i >= 1 {
            preds[preds.len() - 1].hash(&mut s);
        }
        if i >= 2 {
            preds[preds.len() - 2].hash(&mut s);
        }
        i.hash(&mut s);
        s.finish()
    }

    /// The 19 word and tag features (5 word unigrams + 2 word bigrams + 5 tag unigrams + 4 tag
    /// bigrams + 3 tag trigrams) for position `i`. They do not depend on the predicted outcomes,
    /// so they are computed once per position instead of once per beam entry. Each feature is the
    /// concatenation of a fixed prefix and a value (bos/eos are values), hashed like `format!` +
    /// `hash_str`/`hash_slice` would, without allocating the joined strings.
    fn static_context(input: &[(&str, &str)], i: usize) -> [u64; 19] {
        let n = input.len();
        let tok = |j: usize| input[j].0;
        let tag = |j: usize| input[j].1;

        let (wm2p, wm2v) = ("w_2=", if i < 2 { "bos" } else { tok(i - 2) });
        let (wm1p, wm1v) = ("w_1=", if i < 1 { "bos" } else { tok(i - 1) });
        let (w0p, w0v) = ("w0=", tok(i));
        let (wp1p, wp1v) = ("w1=", if i + 1 >= n { "eos" } else { tok(i + 1) });
        let (wp2p, wp2v) = ("w2=", if i + 2 >= n { "eos" } else { tok(i + 2) });

        let (tm2p, tm2v) = ("t_2=", if i < 2 { "bos" } else { tag(i - 2) });
        let (tm1p, tm1v) = ("t_1=", if i < 1 { "bos" } else { tag(i - 1) });
        let (t0p, t0v) = ("t0=", tag(i));
        let (tp1p, tp1v) = ("t1=", if i + 1 >= n { "eos" } else { tag(i + 1) });
        let (tp2p, tp2v) = ("t2=", if i + 2 >= n { "eos" } else { tag(i + 2) });

        let h = |parts: &[&str]| hash::hash_parts(parts);

        [
            // word features
            h(&[wm2p, wm2v]),
            h(&[wm1p, wm1v]),
            h(&[w0p, w0v]),
            h(&[wp1p, wp1v]),
            h(&[wp2p, wp2v]),
            h(&[wm1p, wm1v, w0p, w0v]),
            h(&[w0p, w0v, wp1p, wp1v]),
            // tag features
            h(&[tm2p, tm2v]),
            h(&[tm1p, tm1v]),
            h(&[t0p, t0v]),
            h(&[tp1p, tp1v]),
            h(&[tp2p, tp2v]),
            h(&[tm2p, tm2v, tm1p, tm1v]),
            h(&[tm1p, tm1v, t0p, t0v]),
            h(&[t0p, t0v, tp1p, tp1v]),
            h(&[tp1p, tp1v, tp2p, tp2v]),
            h(&[tm2p, tm2v, tm1p, tm1v, t0p, t0v]),
            h(&[tm1p, tm1v, t0p, t0v, tp1p, tp1v]),
            h(&[t0p, t0v, tp1p, tp1v, tp2p, tp2v]),
        ]
    }

    /// The 2 features that involve the outcome two tokens back (`p_2`, `p_2` + `p_1`).
    fn pred2_context(preds: &[&str], i: usize) -> [u64; 2] {
        // `p_2` has no `=` before a real prediction, but does before `bos` (LanguageTool quirk).
        let (pm2p, pm2v) = (
            "p_2",
            if i < 2 {
                "=bos"
            } else {
                preds[preds.len() - 2]
            },
        );
        let (pm1p, pm1v) = ("p_1=", if i < 1 { "bos" } else { preds[preds.len() - 1] });

        let h = |parts: &[&str]| hash::hash_parts(parts);

        [h(&[pm2p, pm2v]), h(&[pm2p, pm2v, pm1p, pm1v])]
    }

    /// The 20 features that involve only the previous outcome `pm1v` (1 pred, 12 pred+tag,
    /// 7 pred+word). With [`pred2_context`](Self::pred2_context) they are the 22 features that
    /// involve a predicted outcome; the 19 word/tag features are
    /// [`static_context`](Self::static_context).
    fn pred1_context(input: &[(&str, &str)], pm1v: &str, i: usize) -> [u64; 20] {
        let n = input.len();
        let tok = |j: usize| input[j].0;
        let tag = |j: usize| input[j].1;

        let (wm2p, wm2v) = ("w_2=", if i < 2 { "bos" } else { tok(i - 2) });
        let (wm1p, wm1v) = ("w_1=", if i < 1 { "bos" } else { tok(i - 1) });
        let (w0p, w0v) = ("w0=", tok(i));
        let (wp1p, wp1v) = ("w1=", if i + 1 >= n { "eos" } else { tok(i + 1) });
        let (wp2p, wp2v) = ("w2=", if i + 2 >= n { "eos" } else { tok(i + 2) });

        let (tm2p, tm2v) = ("t_2=", if i < 2 { "bos" } else { tag(i - 2) });
        let (tm1p, tm1v) = ("t_1=", if i < 1 { "bos" } else { tag(i - 1) });
        let (t0p, t0v) = ("t0=", tag(i));
        let (tp1p, tp1v) = ("t1=", if i + 1 >= n { "eos" } else { tag(i + 1) });
        let (tp2p, tp2v) = ("t2=", if i + 2 >= n { "eos" } else { tag(i + 2) });

        let pm1p = "p_1=";

        let h = |parts: &[&str]| hash::hash_parts(parts);

        [
            // pred tags
            h(&[pm1p, pm1v]),
            // pred and tag
            h(&[pm1p, pm1v, tm2p, tm2v]),
            h(&[pm1p, pm1v, tm1p, tm1v]),
            h(&[pm1p, pm1v, t0p, t0v]),
            h(&[pm1p, pm1v, tp1p, tp1v]),
            h(&[pm1p, pm1v, tp2p, tp2v]),
            h(&[pm1p, pm1v, tm2p, tm2v, tm1p, tm1v]),
            h(&[pm1p, pm1v, tm1p, tm1v, t0p, t0v]),
            h(&[pm1p, pm1v, t0p, t0v, tp1p, tp1v]),
            h(&[pm1p, pm1v, tp1p, tp1v, tp2p, tp2v]),
            h(&[pm1p, pm1v, tm2p, tm2v, tm1p, tm1v, t0p, t0v]),
            h(&[pm1p, pm1v, tm1p, tm1v, t0p, t0v, tp1p, tp1v]),
            h(&[pm1p, pm1v, t0p, t0v, tp1p, tp1v, tp2p, tp2v]),
            // pred and word
            h(&[pm1p, pm1v, wm2p, wm2v]),
            h(&[pm1p, pm1v, wm1p, wm1v]),
            h(&[pm1p, pm1v, w0p, w0v]),
            h(&[pm1p, pm1v, wp1p, wp1v]),
            h(&[pm1p, pm1v, wp2p, wp2v]),
            h(&[pm1p, pm1v, wm1p, wm1v, w0p, w0v]),
            h(&[pm1p, pm1v, w0p, w0v, wp1p, wp1v]),
        ]
    }

    fn chunk(&self, input: &[(&str, &str)]) -> Sequence {
        // The word/tag features are position-invariant, so resolve their pmap entries and sum
        // their parameters once instead of for every beam entry. The feature hashes themselves
        // are only needed to resolve the entries, so they never leave the stack.
        let priors: Vec<Vec<f32>> = (0..input.len())
            .map(|i| self.model.prior(&Self::static_context(input, i)))
            .collect();
        let mut pred1: Vec<Vec<(&str, [Option<(u32, u8)>; 20])>> = vec![Vec::new(); input.len()];
        let model = &self.model;
        model
            .beam_search(
                input,
                Self::hash,
                |a, b, c, d| self.valid(a, b, c, d),
                10,
                |preds, i| {
                    let pm1v = if i < 1 { "bos" } else { preds[preds.len() - 1] };
                    let cached = &mut pred1[i];
                    let entries = match cached.iter().position(|(p, _)| std::ptr::eq(*p, pm1v)) {
                        Some(k) => &cached[k].1,
                        None => {
                            let keys = Self::pred1_context(input, pm1v, i);
                            cached.push((pm1v, keys.map(|key| model.pmap.get(key))));
                            &cached[cached.len() - 1].1
                        }
                    };
                    let [pm2, pm2pm1] =
                        Self::pred2_context(preds, i).map(|key| model.pmap.get(key));

                    let mut prior = priors[i].clone();
                    for (offset, length) in std::iter::once(pm2)
                        .chain(std::iter::once(entries[0]))
                        .chain(std::iter::once(pm2pm1))
                        .chain(entries[1..].iter().copied())
                        .flatten()
                    {
                        model.add_entry(&mut prior, offset, length);
                    }
                    softmax(&mut prior);
                    prior
                },
            )
            .remove(0)
    }
}

/// Predicts noun chunks and verb chunks through a [Maximum Entropy Model](https://www.aclweb.org/anthology/W00-0729.pdf).
/// Grammatical number (i. e. singular and plural) is also assigned through the part-of-speech tags of the tokens.
#[derive(Serialize, Deserialize, Clone)]
pub struct Chunker {
    pub(crate) token_model: MaxentTokenizer,
    pub(crate) pos_model: MaxentPosTagger,
    pub(crate) chunk_model: MaxentChunker,
}

/// The number LanguageTool's `EnglishChunkFilter` assigns to the noun phrase
/// that starts at `internal_chunks[start]`.
///
/// LanguageTool 5.2 marked a phrase plural if *any* of its tokens had a plural
/// reading. LanguageTool 5.9 added `endOfNounPhraseIsSingular`: a phrase whose
/// last token is not plural is singular even if it contains one (so
/// `a pens and paper` is singular). `number_from_last_token` selects the 5.9+
/// behavior.
fn noun_phrase_number<F>(
    internal_chunks: &[(&str, Range<usize>)],
    has_nns: F,
    start: usize,
    number_from_last_token: bool,
) -> &'static str
where
    F: Fn(&Range<usize>) -> bool,
{
    let is_end_of_np =
        |j: usize| j + 1 >= internal_chunks.len() || internal_chunks[j + 1].0 != "I-NP";
    let chunk_type_plural = |start: usize| {
        let mut plural = false;
        for (chunk, char_span) in internal_chunks[start..].iter() {
            if *chunk != "I-NP" && *chunk != "B-NP" {
                break;
            }
            if has_nns(char_span) {
                plural = true;
            }
        }
        plural
    };

    if !chunk_type_plural(start) {
        return "singular";
    }

    if number_from_last_token {
        // The first end of the phrase decides; LanguageTool then checks whether
        // the phrase ending there is singular.
        for j in start..internal_chunks.len() {
            if is_end_of_np(j) {
                return if chunk_type_plural(j) {
                    "plural"
                } else {
                    "singular"
                };
            }
        }
    }

    "plural"
}

impl Chunker {
    /// Populates the `.chunks` field of the passed tokens by predicting with the maximum entropy model.
    ///
    /// `number_from_last_token` selects LanguageTool's noun-phrase number rule:
    /// `false` for its 5.2 behavior, `true` for the 5.9+ behavior (see
    /// [`noun_phrase_number`]).
    pub fn apply(&self, sentence: &mut IncompleteSentence, number_from_last_token: bool) {
        let text = sentence.text().replace('’', "\'");

        let mut bi_to_ci: DefaultHashMap<usize, usize> = text
            .char_indices()
            .enumerate()
            .map(|(ci, (bi, _))| (bi, ci))
            .collect();
        bi_to_ci.insert(text.len(), text.chars().count());

        // the chunker expects tokens tokenized with a maximum entropy tokenizer
        let internal_tokens = self.token_model.tokenize(&text);
        // the chunker gets part-of-speech tags as input so we also have to run a maximum entropy POSTagger before the chunker
        let tags = self.pos_model.tag(&internal_tokens);
        let chunks = self.chunk_model.chunk(
            &internal_tokens
                .iter()
                .cloned()
                .zip(tags.outcomes().iter().cloned())
                .collect::<Vec<_>>(),
        );

        // compute the char span of each chunk to be able to match it with the input tokens
        let internal_chunks: Vec<_> = chunks
            .outcomes()
            .iter()
            .zip(internal_tokens)
            .map(|(chunk, token)| {
                let byte_start = token.as_ptr() as usize - text.as_ptr() as usize;
                let char_start = *bi_to_ci
                    .get(&byte_start)
                    .expect("byte index is at char boundary");
                let char_end = *bi_to_ci
                    .get(&(byte_start + token.len()))
                    .expect("byte index is at char boundary");

                (*chunk, char_start..char_end)
            })
            .collect();
        let mut token_nns: DefaultHashMap<Range<usize>, bool> = DefaultHashMap::default();
        for token in sentence.iter() {
            token_nns
                .entry(token.span().char().clone())
                .or_insert_with(|| {
                    token
                        .word()
                        .tags()
                        .iter()
                        .any(|tag| tag.pos().as_str() == "NNS")
                });
        }
        let has_nns = |char_span: &Range<usize>| token_nns.get(char_span).copied().unwrap_or(false);

        let mut chunks = Vec::new();
        let mut number = "singular";

        for i in 0..internal_chunks.len() {
            let chunk = internal_chunks[i].0;

            if chunk == "B-NP" {
                number = noun_phrase_number(
                    &internal_chunks,
                    |char_span| has_nns(char_span),
                    i,
                    number_from_last_token,
                );
            }

            let is_noun_end = i + 1 >= internal_chunks.len() || internal_chunks[i + 1].0 != "I-NP";
            let mut to_push = Vec::new();

            if chunk == "B-NP" {
                to_push.push(format!("B-NP-{}", number));
                if is_noun_end {
                    to_push.push(format!("E-NP-{}", number));
                }
            } else if chunk == "I-NP" {
                if is_noun_end {
                    to_push.push(format!("E-NP-{}", number));
                } else {
                    to_push.push(format!("I-NP-{}", number));
                }
            } else {
                to_push.push(chunk.into())
            }

            chunks.push(to_push);
        }

        // chunks with exactly the same char span as the input tokens get assigned to the token to match LT
        let mut chunk_by_span: DefaultHashMap<&Range<usize>, &Vec<String>> =
            DefaultHashMap::default();
        for (chunk, (_, char_span)) in chunks.iter().zip(internal_chunks.iter()) {
            chunk_by_span.insert(char_span, chunk);
        }
        for token in sentence.iter_mut() {
            if let Some(chunk) = chunk_by_span.get(token.span().char()) {
                *token.chunks_mut() = (*chunk).clone();
            }
        }
    }

    /// Runs the chunker's internal tokenizer, POS tagger and chunk model on `text` and returns
    /// `(tokens, pos_tags, chunks)` as owned strings. For the benchmark harness's golden dump only.
    #[cfg(feature = "bench-harness")]
    #[doc(hidden)]
    pub fn predict_internal(&self, text: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
        let internal_tokens = self.token_model.tokenize(text);
        let tags = self.pos_model.tag(&internal_tokens);
        let chunks = self.chunk_model.chunk(
            &internal_tokens
                .iter()
                .cloned()
                .zip(tags.outcomes().iter().cloned())
                .collect::<Vec<_>>(),
        );
        (
            internal_tokens.iter().map(|s| (*s).to_string()).collect(),
            tags.outcomes().iter().map(|s| (*s).to_string()).collect(),
            chunks.outcomes().iter().map(|s| (*s).to_string()).collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::noun_phrase_number;

    // "a pens and paper" as LanguageTool's chunker tags it: a four-token noun
    // phrase whose second token ("pens") is plural and whose last ("paper") is
    // not.
    fn chunks() -> Vec<(&'static str, std::ops::Range<usize>)> {
        vec![
            ("B-NP", 0..1),
            ("I-NP", 1..2),
            ("I-NP", 2..3),
            ("I-NP", 3..4),
        ]
    }

    #[test]
    fn number_uses_any_plural_reading_for_5_2() {
        let chunks = chunks();
        // Only "pens" is plural.
        let has_nns = |span: &std::ops::Range<usize>| span.start == 1;
        assert_eq!(noun_phrase_number(&chunks, has_nns, 0, false), "plural");
    }

    #[test]
    fn number_uses_last_token_for_5_9_and_later() {
        let chunks = chunks();
        let has_nns = |span: &std::ops::Range<usize>| span.start == 1;
        assert_eq!(noun_phrase_number(&chunks, has_nns, 0, true), "singular");

        // If the last token is plural, the phrase stays plural.
        let last_nns = |span: &std::ops::Range<usize>| span.start == 3;
        assert_eq!(noun_phrase_number(&chunks, last_nns, 0, true), "plural");
    }

    #[test]
    fn number_is_singular_without_any_plural_reading() {
        let chunks = chunks();
        assert_eq!(noun_phrase_number(&chunks, |_| false, 0, false), "singular");
        assert_eq!(noun_phrase_number(&chunks, |_| false, 0, true), "singular");
    }

    #[test]
    fn beam_search_keeps_tags_tied_with_the_last_kept_one() {
        use super::{MaxentChunker, Model, Pmap};

        let model = Model {
            outcome_labels: vec!["A".into(), "B".into(), "C".into()],
            outcomes: Vec::new(),
            parameters: Vec::new(),
            pmap: Pmap::from_pairs(std::iter::empty()),
        };
        let best = model
            .beam_search(
                &["x", "y"],
                MaxentChunker::hash,
                |_, _, _, _| true,
                1,
                |preds, _| match preds {
                    [] => vec![0.5, 0.5, 0.],
                    ["B"] => vec![0.98, 0.01, 0.01],
                    _ => vec![0.4, 0.3, 0.3],
                },
            )
            .remove(0);

        assert_eq!(best.outcomes(), ["B", "A"]);
    }
}
