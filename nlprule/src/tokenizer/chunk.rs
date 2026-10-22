//! A Chunker ported from [OpenNLP](https://opennlp.apache.org/).

use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::sync::OnceLock;
use std::{cmp::Ordering, collections::BinaryHeap};

use crate::types::{DefaultHashMap, IncompleteSentence};

fn softmax(vec: &mut [f32]) {
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

#[derive(Clone, Copy)]
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

/// Writes the best `size` of `candidates` to `best`, best first, in the order that pushing
/// `candidates` one by one into a [`BinaryHeap`] and truncating its `into_sorted_vec` gives.
/// That order is ambiguous only among candidates with equal log probabilities, so the best are
/// selected and sorted directly, and the heap is replayed only if two of them, or the last kept
/// one and the best dropped one, are tied.
fn select_best(candidates: &[Beam], size: usize, best: &mut Vec<Beam>) {
    best.clear();
    best.extend_from_slice(candidates);
    let mut tied = false;
    if best.len() > size {
        best.select_nth_unstable(size);
        let dropped = best[size];
        best.truncate(size);
        best.sort_unstable();
        tied = best.last() == Some(&dropped);
    } else {
        best.sort_unstable();
    }
    tied = tied || best.windows(2).any(|pair| pair[0] == pair[1]);

    if tied {
        let mut heap = BinaryHeap::with_capacity(candidates.len());
        for &candidate in candidates {
            heap.push(candidate);
        }
        *best = heap.into_sorted_vec();
        best.truncate(size);
    }
}

/// The first of the best of `candidates` in the order that pushing them one by one into a
/// [`BinaryHeap`] stores them, which is what a stable sort of the heap's contents puts first.
fn first_best(candidates: &[Beam]) -> Option<Beam> {
    let best = candidates.iter().min()?;
    if candidates
        .iter()
        .filter(|candidate| *candidate == best)
        .count()
        == 1
    {
        return Some(*best);
    }
    let mut heap = BinaryHeap::with_capacity(candidates.len());
    for &candidate in candidates {
        heap.push(candidate);
    }
    heap.into_vec().into_iter().min()
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

    /// A hasher that has consumed the byte concatenation of `parts`, to finish several hashes
    /// that share it as a prefix with [`hash_parts_after`].
    pub fn prefix_hasher(parts: &[&str]) -> DefaultHasher {
        let mut hasher = DefaultHasher::new();
        for part in parts.iter() {
            hasher.write(part.as_bytes());
        }
        hasher
    }

    /// Like [`hash_parts`] of the prefix parts of `prefix` followed by `parts`:
    /// `hash_parts_after(&prefix_hasher(a), b) == hash_parts(&[a, b].concat())`.
    pub fn hash_parts_after(prefix: &DefaultHasher, parts: &[&str]) -> u64 {
        let mut hasher = prefix.clone();
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
        fn prefixed_hash_matches_parts_hash() {
            let prefix = prefix_hasher(&["p_1=", "B-NP"]);
            assert_eq!(
                hash_parts_after(&prefix, &["t0=", "NN"]),
                hash_parts(&["p_1=", "B-NP", "t0=", "NN"])
            );
            assert_eq!(hash_parts_after(&prefix, &[]), hash_str("p_1=B-NP"));
            assert_eq!(
                hash_parts_after(&prefix_hasher(&[]), &["w0=", "“Ahab’s—…\u{2028}"]),
                hash_str("w0=“Ahab’s—…\u{2028}")
            );
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
    parameters: Vec<f32>,
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

/// A context map stored as a `Vec` sorted by key instead of a `HashMap`. The serialized (on-disk)
/// form is a `Vec<(u64, u8)>` sorted by offset; only this in-memory form, built at load, is
/// different.
///
/// The keys are SipHash outputs, so their top bits are uniformly distributed. `buckets` splits the
/// sorted entries by their top key bits, so a lookup scans the few entries of one bucket (usually
/// one cache line) instead of binary-searching the whole table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PmapEntries")]
pub(crate) struct Pmap {
    /// Sorted by `key`; keys are unique.
    entries: Vec<PmapEntry>,
    /// `buckets[b]` is the index of the first entry whose top key bits (`key >> shift`) are
    /// `>= b`; the last element is `entries.len()`. Derived from `entries`, so not serialized.
    #[serde(skip)]
    buckets: Vec<u32>,
    #[serde(skip)]
    shift: u32,
}

/// The serialized fields of a [`Pmap`], from which the bucket index is rebuilt.
#[derive(Deserialize)]
struct PmapEntries {
    entries: Vec<PmapEntry>,
}

impl From<PmapEntries> for Pmap {
    fn from(data: PmapEntries) -> Self {
        Pmap::new(data.entries)
    }
}

impl Default for Pmap {
    fn default() -> Self {
        Pmap::new(Vec::new())
    }
}

impl Pmap {
    /// The average number of entries per bucket that the bucket count aims for.
    const ENTRIES_PER_BUCKET: usize = 4;

    /// Builds the bucket index over `entries`, which must be sorted by key.
    fn new(entries: Vec<PmapEntry>) -> Self {
        assert!(entries.len() < u32::MAX as usize);
        let bits = (entries.len() / Self::ENTRIES_PER_BUCKET)
            .next_power_of_two()
            .trailing_zeros()
            .max(1);
        let shift = 64 - bits;
        let mut buckets = Vec::with_capacity((1 << bits) + 1);
        for (idx, entry) in entries.iter().enumerate() {
            let bucket = (entry.key >> shift) as usize;
            while buckets.len() <= bucket {
                buckets.push(idx as u32);
            }
        }
        buckets.resize((1 << bits) + 1, entries.len() as u32);
        Pmap {
            entries,
            buckets,
            shift,
        }
    }

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
        Pmap::new(entries)
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
        Pmap::new(deduped)
    }

    /// Looks up `key` by scanning its bucket.
    #[inline]
    fn get(&self, key: u64) -> Option<(u32, u8)> {
        let bucket = (key >> self.shift) as usize;
        let start = self.buckets[bucket] as usize;
        let end = self.buckets[bucket + 1] as usize;
        self.entries[start..end]
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| (entry.offset, entry.length))
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
            mut outcomes,
            mut parameters,
        } = data;
        outcomes.shrink_to_fit();
        parameters.shrink_to_fit();

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
    pub(crate) parameters: Vec<f32>,
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
            prior[*idx as usize] += param;
        }
    }

    fn eval(&self, context: &[u64]) -> Vec<f32> {
        let mut prior = self.prior(&[]);
        self.eval_with_prior(&mut prior, context);
        prior
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
    /// looked up and summed earlier, and leaves the probabilities in it. The keys in `context`
    /// are still looked up.
    fn eval_with_prior(&self, prior: &mut [f32], context: &[u64]) {
        for (offset, length) in context.iter().filter_map(|x| self.pmap.get(*x)) {
            self.add_entry(prior, offset, length);
        }

        softmax(prior);
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

    /// Returns the best sequence of OpenNLP's `BeamSearch` over `tokens`.
    ///
    /// `eval_fn(outcomes, window, i, scores)` writes the outcome probabilities at position `i`
    /// into `scores` (which it receives empty), given the last (up to two) predicted outcomes as
    /// labels (`outcomes`) and as indices into `outcome_labels` (`window`). Sequences that share
    /// those outcomes at a position share one evaluation. `valid_fn(window, i, j)` says whether
    /// outcome `j` may follow `window` at position `i`.
    fn beam_search<
        'm,
        S,
        V: Fn(&[usize], usize, usize) -> bool,
        E: FnMut(&[&'m str], &[usize], usize, &mut Vec<f32>),
    >(
        &'m self,
        tokens: &[S],
        valid_fn: V,
        size: usize,
        mut eval_fn: E,
    ) -> Sequence<'m> {
        /// One evaluated context of the current position: its outcome window, the scores and the
        /// outcomes to extend with (index and log probability), best first. Kept across positions
        /// so the buffers are reused.
        struct Evaluated {
            window: [usize; 2],
            scores: Vec<f32>,
            top_n: Vec<(usize, f32)>,
        }

        if tokens.is_empty() {
            return Sequence {
                outcomes: Vec::new(),
            };
        }

        let mut nodes: Vec<Node> = Vec::new();
        let mut current: Vec<(f32, Option<usize>)> = vec![(0., None)];
        // The sequences proposed for the next position, in the order they were proposed.
        let mut next: Vec<Beam> = Vec::new();
        let mut best: Vec<Beam> = Vec::new();

        // The contexts evaluated at the current position are `cache[..cached]`. The window has the
        // same length for every sequence at a position, so the window alone identifies a context.
        let mut cache: Vec<Evaluated> = Vec::new();

        for i in 0..tokens.len() {
            let mut cached = 0;
            if i > 0 {
                select_best(&next, size, &mut best);
                next.clear();
                current.clear();
                for beam in best.iter() {
                    nodes.push(Node {
                        outcome: beam.outcome,
                        parent: beam.parent,
                    });
                    current.push((beam.log_prob, Some(nodes.len() - 1)));
                }
            }

            for &(log_prob, seq_node) in current.iter() {
                let mut labels: [&'m str; 2] = [""; 2];
                let mut window = [usize::MAX; 2];
                let mut len = 0;
                let mut node = seq_node;
                while let (true, Some(idx)) = (len < window.len(), node) {
                    let outcome = nodes[idx].outcome;
                    window[window.len() - 1 - len] = outcome;
                    labels[labels.len() - 1 - len] = self.outcome_labels[outcome].as_str();
                    node = nodes[idx].parent;
                    len += 1;
                }
                let outcomes = &labels[labels.len() - len..];

                let k = match cache[..cached].iter().position(|e| e.window == window) {
                    Some(k) => k,
                    None => {
                        if cached == cache.len() {
                            cache.push(Evaluated {
                                window,
                                scores: Vec::new(),
                                top_n: Vec::new(),
                            });
                        }
                        let Evaluated {
                            window: cached_window,
                            scores,
                            top_n,
                        } = &mut cache[cached];
                        *cached_window = window;
                        scores.clear();
                        eval_fn(outcomes, &window[window.len() - len..], i, scores);
                        top_n.clear();
                        top_n.extend(
                            (0..scores.len())
                                .filter(|&j| scores[j] >= 0.)
                                .map(|j| (j, scores[j])),
                        );
                        // Best first, ties in index order, cut after the `size`-th best but
                        // keeping the outcomes tied with it: what a stable sort of the outcomes
                        // by score followed by the cut gives, without sorting all of them.
                        let order = |a: &(usize, f32), b: &(usize, f32)| {
                            b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0))
                        };
                        if top_n.len() > size {
                            let min = top_n.select_nth_unstable_by(size - 1, order).1 .1;
                            top_n.retain(|(_, p)| *p >= min);
                        }
                        top_n.sort_unstable_by(order);
                        for (_, p) in top_n.iter_mut() {
                            *p = p.ln();
                        }
                        cached += 1;
                        cached - 1
                    }
                };
                let Evaluated { scores, top_n, .. } = &cache[k];
                let window = &window[window.len() - len..];

                for &(j, log_p) in top_n {
                    if valid_fn(window, i, j) {
                        next.push(Beam {
                            log_prob: log_prob + log_p,
                            parent: seq_node,
                            outcome: j,
                        });
                    }
                }

                // if no advanced sequences, advance all valid to match OpenNLP behaviour
                if next.is_empty() {
                    for (j, p) in scores.iter().enumerate() {
                        if valid_fn(window, i, j) {
                            next.push(Beam {
                                log_prob: log_prob + p.ln(),
                                parent: seq_node,
                                outcome: j,
                            });
                        }
                    }
                }
            }
        }

        let best = first_best(&next).unwrap();
        let mut outcomes = Vec::with_capacity(tokens.len());
        outcomes.push(self.outcome_labels[best.outcome].as_str());
        let mut node = best.parent;
        while let Some(idx) = node {
            outcomes.push(self.outcome_labels[nodes[idx].outcome].as_str());
            node = nodes[idx].parent;
        }
        outcomes.reverse();
        Sequence { outcomes }
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
    /// The [`TagDict`] id of each outcome label (`None` if no word allows it), built on first use.
    #[serde(skip)]
    outcome_tag_ids: OnceLock<Box<[Option<u16>]>>,
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

    pub(crate) fn new(model: Model, tagdict: TagDict) -> Self {
        MaxentPosTagger {
            model,
            tagdict,
            outcome_tag_ids: OnceLock::new(),
        }
    }

    /// Whether a word whose tag dictionary entry is `allowed` may have the outcome whose tag id
    /// is `outcome_tag_id`.
    fn valid(allowed: Option<&[u16]>, outcome_tag_id: Option<u16>) -> bool {
        if let Some(allowed) = allowed {
            match outcome_tag_id {
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
        let tag_ids = self.outcome_tag_ids.get_or_init(|| {
            self.model
                .outcome_labels
                .iter()
                .map(|label| self.tagdict.tag_id(label))
                .collect()
        });
        self.model.beam_search(
            tokens,
            |_, i, j| Self::valid(allowed[i], tag_ids[j]),
            3,
            |tags, _, i, scores| {
                scores.extend_from_slice(&priors[i]);
                self.model
                    .eval_with_prior(scores, &Self::tag_context(tags, i));
            },
        )
    }
}

type Pred1Entries = [Option<(u32, u8)>; 20];

/// What the chunk model needs per previous outcome, which does not depend on the position, so it
/// is computed once per model instead of once per beam entry: the pmap entries of the 2 features
/// that involve the outcome two tokens back (`p_2`, `p_2` + `p_1`) and which outcomes may follow
/// which. The index of an outcome is its index in `outcome_labels`, and `outcome_labels.len()`
/// stands for `bos`.
#[derive(Clone)]
struct ChunkTables {
    stride: usize,
    /// `p_2` by the index of the outcome two tokens back.
    pm2: Vec<Option<(u32, u8)>>,
    /// `p_2` + `p_1` by `pm2 * stride + pm1`.
    pm2pm1: Vec<Option<(u32, u8)>>,
    /// [`MaxentChunker::valid`] by `pm1 * stride + outcome`.
    valid: Vec<bool>,
}

impl ChunkTables {
    // Measured with rustc 1.99: `is_some_and` in place of `map_or` here changed codegen elsewhere
    // and cost the chunker about 3% more instructions.
    #[allow(clippy::unnecessary_map_or)]
    fn new(model: &Model) -> Self {
        let labels = &model.outcome_labels;
        let stride = labels.len() + 1;
        // `p_2` has no `=` before a real prediction, but does before `bos` (LanguageTool quirk).
        let pm2_value = |j: usize| labels.get(j).map_or("=bos", String::as_str);
        let pm1_value = |j: usize| labels.get(j).map_or("bos", String::as_str);
        let (pm2p, pm1p) = ("p_2", "p_1=");

        let pm2 = (0..stride)
            .map(|j2| model.pmap.get(hash::hash_parts(&[pm2p, pm2_value(j2)])))
            .collect();
        let pm2pm1 = (0..stride * stride)
            .map(|j| {
                let (pm2v, pm1v) = (pm2_value(j / stride), pm1_value(j % stride));
                model.pmap.get(hash::hash_parts(&[pm2p, pm2v, pm1p, pm1v]))
            })
            .collect();
        let valid = (0..stride * stride)
            .map(|j| {
                let (pm1, outcome) = (labels.get(j / stride), labels.get(j % stride));
                outcome.map_or(false, |outcome| {
                    MaxentChunker::valid(pm1.map(String::as_str), outcome)
                })
            })
            .collect();
        ChunkTables {
            stride,
            pm2,
            pm2pm1,
            valid,
        }
    }

    /// The entries of `p_2` and `p_2` + `p_1` given the indices of the outcomes two tokens back
    /// and one token back (`None` before the start of the sentence).
    #[inline]
    fn get(&self, pm2: Option<usize>, pm1: Option<usize>) -> [Option<(u32, u8)>; 2] {
        let bos = self.stride - 1;
        let (pm2, pm1) = (pm2.unwrap_or(bos), pm1.unwrap_or(bos));
        [self.pm2[pm2], self.pm2pm1[pm2 * self.stride + pm1]]
    }

    /// Whether `outcome` may follow `pm1` (`None` before the start of the sentence).
    #[inline]
    fn valid(&self, pm1: Option<usize>, outcome: usize) -> bool {
        self.valid[pm1.unwrap_or(self.stride - 1) * self.stride + outcome]
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct MaxentChunker {
    pub(crate) model: Model,
    /// Built from `model` on first use.
    #[serde(skip)]
    tables: OnceLock<ChunkTables>,
}

impl MaxentChunker {
    pub(crate) fn new(model: Model) -> Self {
        MaxentChunker {
            model,
            tables: OnceLock::new(),
        }
    }

    #[allow(clippy::manual_strip)]
    /// Whether `outcome` may follow `prev_outcome` (`None` at the start of the sentence).
    fn valid(prev_outcome: Option<&str>, outcome: &str) -> bool {
        if outcome.starts_with("I-") {
            if let Some(prev_outcome) = prev_outcome {
                // can byte index here because "I, B, E, -" are all 1 byte
                prev_outcome != "O" && prev_outcome[2..] == outcome[2..]
            } else {
                false
            }
        } else {
            true
        }
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

    /// The 20 features that involve only the previous outcome `pm1v` (1 pred, 12 pred+tag,
    /// 7 pred+word). With the 2 features in [`ChunkTables`] they are the 22 features that
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

        // Every feature starts with the previous outcome, so its bytes are hashed once.
        let prefix = hash::prefix_hasher(&["p_1=", pm1v]);
        let h = |parts: &[&str]| hash::hash_parts_after(&prefix, parts);

        [
            // pred tags
            h(&[]),
            // pred and tag
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
            // pred and word
            h(&[wm2p, wm2v]),
            h(&[wm1p, wm1v]),
            h(&[w0p, w0v]),
            h(&[wp1p, wp1v]),
            h(&[wp2p, wp2v]),
            h(&[wm1p, wm1v, w0p, w0v]),
            h(&[w0p, w0v, wp1p, wp1v]),
        ]
    }

    fn chunk(&self, input: &[(&str, &str)]) -> Sequence {
        // The word/tag features are position-invariant, so resolve their pmap entries and sum
        // their parameters once instead of for every beam entry. The feature hashes themselves
        // are only needed to resolve the entries, so they never leave the stack.
        let priors: Vec<Vec<f32>> = (0..input.len())
            .map(|i| self.model.prior(&Self::static_context(input, i)))
            .collect();
        let mut pred1: Vec<Vec<(Option<usize>, Pred1Entries)>> = vec![Vec::new(); input.len()];
        let model = &self.model;
        let tables = self.tables.get_or_init(|| ChunkTables::new(model));
        model.beam_search(
            input,
            |window, _, j| tables.valid(window.last().copied(), j),
            10,
            |preds, window, i, prior| {
                let pm1 = window.last().copied();
                let pm2 = window.len().checked_sub(2).map(|k| window[k]);
                let cached = &mut pred1[i];
                let entries = match cached.iter().position(|(p, _)| *p == pm1) {
                    Some(k) => &cached[k].1,
                    None => {
                        let pm1v = if i < 1 { "bos" } else { preds[preds.len() - 1] };
                        let keys = Self::pred1_context(input, pm1v, i);
                        cached.push((pm1, keys.map(|key| model.pmap.get(key))));
                        &cached[cached.len() - 1].1
                    }
                };
                let [pm2, pm2pm1] = tables.get(pm2, pm1);

                prior.extend_from_slice(&priors[i]);
                for (offset, length) in std::iter::once(pm2)
                    .chain(std::iter::once(entries[0]))
                    .chain(std::iter::once(pm2pm1))
                    .chain(entries[1..].iter().copied())
                    .flatten()
                {
                    model.add_entry(prior, offset, length);
                }
                softmax(prior);
            },
        )
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

        // The byte index of every char, then the text's end: the char index of a byte index is
        // its position here.
        let char_starts: Vec<usize> = text
            .char_indices()
            .map(|(bi, _)| bi)
            .chain(std::iter::once(text.len()))
            .collect();
        let char_index = |byte: usize| {
            char_starts
                .binary_search(&byte)
                .expect("byte index is at char boundary")
        };

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
                let char_start = char_index(byte_start);
                let char_end = char_index(byte_start + token.len());

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
        use super::{Model, Pmap};

        let model = Model {
            outcome_labels: vec!["A".into(), "B".into(), "C".into()],
            outcomes: Vec::new(),
            parameters: Vec::new(),
            pmap: Pmap::from_pairs(std::iter::empty()),
        };
        let best = model.beam_search(
            &["x", "y"],
            |_, _, _| true,
            1,
            |preds, _, _, scores| {
                scores.extend_from_slice(match preds {
                    [] => &[0.5, 0.5, 0.],
                    ["B"] => &[0.98, 0.01, 0.01],
                    _ => &[0.4, 0.3, 0.3],
                })
            },
        );

        assert_eq!(best.outcomes(), ["B", "A"]);
    }

    #[test]
    fn best_beams_are_in_heap_order_despite_ties() {
        use super::{first_best, select_best, Beam};
        use std::collections::BinaryHeap;

        let key = |beam: &Beam| (beam.log_prob.to_bits(), beam.parent, beam.outcome);
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 33) as usize
        };
        let mut best = Vec::new();
        for round in 0..2_000 {
            // Few distinct log probabilities in some rounds, so ties are common there.
            let distinct = if round % 2 == 0 { 3 } else { 1_000 };
            let candidates: Vec<Beam> = (0..next() % 40)
                .map(|k| Beam {
                    log_prob: -((next() % distinct) as f32) / 7.,
                    parent: Some(k),
                    outcome: next() % 5,
                })
                .collect();
            let size = 1 + next() % 12;

            let mut heap = BinaryHeap::new();
            for &candidate in &candidates {
                heap.push(candidate);
            }
            let mut drained: Vec<Beam> = heap.clone().into_vec();
            drained.sort();
            let mut expected = heap.into_sorted_vec();
            expected.truncate(size);

            select_best(&candidates, size, &mut best);
            assert_eq!(
                best.iter().map(key).collect::<Vec<_>>(),
                expected.iter().map(key).collect::<Vec<_>>()
            );
            assert_eq!(
                first_best(&candidates).as_ref().map(key),
                drained.first().map(key)
            );
        }
    }

    #[test]
    fn pmap_finds_every_key_after_loading() {
        use super::Pmap;

        let keys: Vec<u64> = (0..5_000u64)
            .map(|i| i.wrapping_mul(0x9e37_79b9_7f4a_7c15))
            .collect();
        let pmap = Pmap::from_pairs(keys.iter().enumerate().map(|(i, &key)| (key, (i, 1))));
        let loaded: Pmap = bincode::deserialize(&bincode::serialize(&pmap).unwrap()).unwrap();

        for (i, &key) in keys.iter().enumerate() {
            assert_eq!(pmap.get(key), Some((i as u32, 1)));
            assert_eq!(loaded.get(key), Some((i as u32, 1)));
            assert_eq!(loaded.get(key ^ 1), None);
        }
        assert_eq!(Pmap::default().get(0), None);
        assert_eq!(Pmap::default().get(u64::MAX), None);
    }

    #[test]
    fn loaded_model_arrays_have_no_spare_capacity() {
        use super::{Model, Pmap};

        let entries = 1_200;
        let length = 250;
        let len = entries * length;
        let model = Model {
            outcome_labels: vec!["A".into()],
            outcomes: vec![0; len],
            parameters: (0..len).map(|i| i as f32 * 1e-3).collect(),
            pmap: Pmap::from_pairs((0..entries).map(|i| (i as u64, (i * length, length)))),
        };
        let bytes = bincode::serialize(&model).unwrap();
        let loaded: Model = bincode::deserialize(&bytes).unwrap();

        assert_eq!(loaded.parameters, model.parameters);
        assert_eq!(loaded.parameters.capacity(), len);
        assert_eq!(loaded.outcomes.capacity(), len);
    }
}
