use fnv::FnvHasher;
use fst::Automaton;

use super::replacements::{Edit, ReplacementPairs};
use std::{
    cmp::{self, min},
    convert::TryFrom,
    hash::{Hash, Hasher},
};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug)]
pub struct LevenshteinState {
    dist: usize,
    n: usize,
    // to compute the next row of the matrix, we also need the row two rows up for transposes
    prev_row: Option<Vec<usize>>,
    prev_char: char,
    // the previous character as stored in the FST, for replacement pairs (which match case)
    prev_raw: char,
    row: Vec<usize>,
    // the largest row minimum on the way here (see `LevenshteinState::reach`)
    max_cutoff: usize,
    // the offsets between query and key positions that replacement pairs have produced so far,
    // as bits `offset + SHIFT_BIAS`; a pair applies only at one of them
    shifts: u32,
    // the bytes of a multi-byte UTF-8 character read so far
    pending: [u8; 4],
    pending_len: usize,
    hash: u64,
}

impl LevenshteinState {
    pub fn dist(&self) -> usize {
        self.dist
    }

    /// The smallest maximum distance at which a search reaches this state: its distance, or more
    /// when a prefix of the key was further from the query than the whole key (a replacement
    /// pair can make a longer key cheaper than its prefix). Morfologik searches distance 1
    /// before distance 2 and prunes such prefixes in the distance-1 search.
    pub fn reach(&self) -> usize {
        self.dist.max(self.max_cutoff)
    }
}

#[derive(Debug, Clone)]
pub struct Levenshtein {
    query: Vec<char>,
    distance: usize,
    prefix: usize,
    fold_case: bool,
    // the base letter of each query character (see `base`) if accents are ignored, else empty
    query_base: Vec<char>,
    // zero-cost edits from replacement pairs, indexed by the query position where they start
    edits: Vec<Vec<Edit>>,
}

impl Levenshtein {
    pub fn new(query: &str, distance: usize, prefix: usize) -> Self {
        Levenshtein {
            query: query.chars().collect(),
            distance,
            prefix,
            fold_case: false,
            query_base: Vec::new(),
            edits: Vec::new(),
        }
    }

    /// Lets the search apply `pairs` as zero-cost edits. Must be called before
    /// [Levenshtein::fold_case], since the pairs match the query's original case.
    pub fn replacements(mut self, pairs: &ReplacementPairs) -> Self {
        debug_assert!(!self.fold_case);
        self.edits = pairs.edits_for(&self.query);
        if self.edits.iter().all(Vec::is_empty) {
            self.edits = Vec::new();
        }
        self
    }

    pub fn fold_case(mut self, fold_case: bool) -> Self {
        if fold_case && !self.fold_case {
            self.query.iter_mut().for_each(|c| *c = fold(*c));
        }
        self.fold_case = fold_case;
        self
    }

    /// Lets a key character match a query character with the same base letter (`é` matches `e`
    /// and `ë`), as Morfologik's `areEqual` does for dictionaries that ignore diacritics. Must be
    /// called after [Levenshtein::fold_case].
    pub fn ignore_diacritics(mut self, ignore: bool) -> Self {
        self.query_base = if ignore {
            self.query.iter().map(|c| base(*c)).collect()
        } else {
            Vec::new()
        };
        self
    }

    fn normalize(&self, c: char) -> char {
        if self.fold_case {
            fold(c)
        } else {
            c
        }
    }
}

const SHIFT_BIAS: i32 = 16;

/// At most this many replacement pairs apply to one key character.
const MAX_STEP_EDITS: usize = 8;

fn fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => c,
    }
}

/// The first character of `c`'s canonical decomposition (NFD): `é` -> `e`. `ø` has none, so it
/// stays `ø`.
fn base(c: char) -> char {
    if c.is_ascii() {
        return c;
    }
    c.nfd().next().unwrap_or(c)
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

impl Automaton for Levenshtein {
    type State = Option<LevenshteinState>;

    fn start(&self) -> Self::State {
        Some(LevenshteinState {
            dist: self.query.len(),
            n: 0,
            prev_row: None,
            prev_char: '\0',
            prev_raw: '\0',
            row: (0..=self.query.len()).collect(),
            max_cutoff: 0,
            shifts: 1 << SHIFT_BIAS,
            pending: [0; 4],
            pending_len: 0,
            hash: FnvHasher::default().finish(),
        })
    }

    fn is_match(&self, state: &Self::State) -> bool {
        state.as_ref().map_or(false, |state| {
            state.pending_len == 0 && state.dist <= self.distance
        })
    }

    fn can_match(&self, state: &Self::State) -> bool {
        state.is_some()
    }

    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        state.as_ref().and_then(|state| {
            let mut next_hasher = FnvHasher::with_key(state.hash);
            byte.hash(&mut next_hasher);
            let next_hash = next_hasher.finish();

            let c = if state.pending_len == 0 && byte.is_ascii() {
                byte as char
            } else {
                let mut pending = state.pending;
                pending[state.pending_len] = byte;
                let pending_len = state.pending_len + 1;
                if pending_len < utf8_width(pending[0]) {
                    return Some(LevenshteinState {
                        pending,
                        pending_len,
                        hash: next_hash,
                        ..state.clone()
                    });
                }
                std::str::from_utf8(&pending[..pending_len])
                    .ok()
                    .and_then(|s| s.chars().next())
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
            };
            let raw = c;
            let c = self.normalize(c);

            let c_base = if self.query_base.is_empty() {
                c
            } else {
                base(c)
            };
            let row = &state.row;
            let mut next_row = state.row.to_vec();

            // Replacement pairs: query characters `start..end` stand for this key character, or
            // for the previous one and this one, at no cost. As in Morfologik, a pair applies
            // only where it starts at the position in the query that lines up with the key
            // (shifted by the pairs applied before it).
            let mut step_edits = [(0usize, 0usize, 0usize, 0usize); MAX_STEP_EDITS];
            let mut n_step_edits = 0;
            let mut shifts = state.shifts;
            if !self.edits.is_empty() {
                let mut bits = state.shifts;
                while bits != 0 {
                    let shift = bits.trailing_zeros() as i32 - SHIFT_BIAS;
                    bits &= bits - 1;
                    let anchored = |back: usize| {
                        usize::try_from(state.n as i32 + shift - back as i32)
                            .ok()
                            .and_then(|start| self.edits.get(start))
                            .map_or(&[][..], Vec::as_slice)
                    };
                    let one = anchored(0)
                        .iter()
                        .filter(|e| e.to_len == 1 && e.to[0] == raw);
                    let two = state.prev_row.iter().flat_map(|_| {
                        anchored(1)
                            .iter()
                            .filter(|e| e.to_len == 2 && e.to == [state.prev_raw, raw])
                    });
                    for edit in one.chain(two) {
                        let source = if edit.to_len == 1 {
                            row[edit.start]
                        } else {
                            state
                                .prev_row
                                .as_ref()
                                .map_or(usize::MAX, |r| r[edit.start])
                        };
                        if n_step_edits < MAX_STEP_EDITS {
                            step_edits[n_step_edits] = (edit.start, edit.end, edit.to_len, source);
                            n_step_edits += 1;
                        }
                        let next = shift + edit.shift as i32 + SHIFT_BIAS;
                        if (0..32).contains(&next) {
                            shifts |= 1 << next;
                        }
                    }
                }
            }

            next_row[0] = state.n + 1;

            for i in 1..next_row.len() {
                let same = c == self.query[i - 1]
                    || self.query_base.get(i - 1).is_some_and(|q| *q == c_base);
                let mut cost = if same {
                    row[i - 1]
                } else {
                    min(
                        next_row[i - 1] + 1, // deletes
                        min(
                            row[i - 1] + 1, // inserts
                            row[i] + 1,     // substitutes
                        ),
                    )
                };

                if i > 1 {
                    // transposes
                    if let Some(prev_row) = state.prev_row.as_ref() {
                        if c == self.query[i - 2] && state.prev_char == self.query[i - 1] {
                            cost = min(cost, prev_row[i - 2] + 1);
                        }
                    }
                }

                next_row[i] = cost;
            }

            // The column a pair leaves behind, as Morfologik's `findRepl` builds it: the
            // column before the replaced key characters, realigned to the query position after
            // the pair, with the pair's cost on the new diagonal. The pair's cell is not
            // extended by deletions in this column, so a pair is followed by further edits only
            // from the next key character on, and its cost does not leak into cells that
            // Morfologik keeps from the unreplaced path. At the end of a key, the query
            // characters after the pair are deleted (`isEndOfCandidate`).
            let mut dist = next_row[self.query.len()];
            if n_step_edits > 0 {
                let len = self.query.len();
                for &(start, end, to_len, source) in &step_edits[..n_step_edits] {
                    if source == usize::MAX {
                        continue;
                    }
                    dist = min(dist, source + (len - end));
                    // `to_len == 2`: the column before the previous key character is the
                    // pair's cost; the realigned one is the column before this character.
                    let back = end - start + 1 - to_len;
                    let lo = end.saturating_sub(self.distance).max(back);
                    let hi = min(end + self.distance, len);
                    for (q, cell) in next_row.iter_mut().enumerate().take(hi + 1).skip(lo) {
                        let value = if q == end { source } else { row[q - back] };
                        *cell = min(*cell, value);
                    }
                }
            }

            let distance = if state.n >= self.prefix {
                self.distance
            } else {
                1
            };

            // Without replacement pairs only the cells within `distance` of the diagonal can
            // stay within `distance`; a pair can move a cheap path further off it.
            let (lower_bound, upper_bound) = if self.edits.is_empty() {
                (
                    (state.n + 1).saturating_sub(distance),
                    cmp::min(state.n + 1 + distance, self.query.len()),
                )
            } else {
                (0, self.query.len())
            };

            let cutoff = if lower_bound > upper_bound {
                0
            } else {
                *next_row[lower_bound..=upper_bound]
                    .iter()
                    .min()
                    .unwrap_or(&0)
            };

            if cutoff > distance {
                return None;
            }

            Some(LevenshteinState {
                dist,
                n: state.n + 1,
                prev_row: Some(row.clone()),
                prev_char: c,
                prev_raw: raw,
                row: next_row,
                max_cutoff: state.max_cutoff.max(cutoff),
                shifts,
                pending: [0; 4],
                pending_len: 0,
                hash: next_hash,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(query: &str, key: &str, fold_case: bool) -> Option<usize> {
        distance_with_prefix(query, key, fold_case, 0)
    }

    fn distance_with_prefix(
        query: &str,
        key: &str,
        fold_case: bool,
        prefix: usize,
    ) -> Option<usize> {
        let automaton = Levenshtein::new(query, 2, prefix).fold_case(fold_case);
        let mut state = automaton.start();
        for byte in key.bytes() {
            state = automaton.accept(&state, byte);
        }
        state.map(|state| state.dist())
    }

    #[test]
    fn fold_case_matches_ascii_letters_case_insensitively() {
        assert_eq!(distance("TEH", "ten", false), None);
        assert_eq!(distance("TEH", "ten", true), Some(1));
        assert_eq!(distance("Teh", "Ted", true), Some(1));
        assert_eq!(distance("Teh", "the", true), Some(1));
        assert_eq!(distance("Hte", "the", true), Some(1));
        assert_eq!(distance("Hte", "the", false), Some(2));
        assert_eq!(distance("GOIGN", "going", true), Some(1));
        assert_eq!(distance("WHTA", "what", true), Some(1));
    }

    #[test]
    fn fold_case_matches_non_ascii_letters_case_insensitively() {
        assert_eq!(distance("CAF\u{c9}", "caf\u{e9}", true), Some(0));
        assert_eq!(distance("CAF\u{c9}", "caf\u{e9}", false), None);
        assert_eq!(distance("\u{c9}CLAIRR", "\u{e9}clair", true), Some(1));
        assert_eq!(distance("\u{c9}clairr", "\u{e9}clair", true), Some(1));
        assert_eq!(distance("NA\u{cf}VEE", "na\u{ef}ve", true), Some(1));
        assert_eq!(distance("ZO\u{cb}", "Zo\u{eb}", true), Some(0));
        assert_eq!(distance("REN\u{c9}E", "Ren\u{e9}e", true), Some(0));
        assert_eq!(distance("S\u{d8}RENN", "S\u{f8}ren", true), Some(1));
        assert_eq!(distance("STRA\u{1e9e}E", "stra\u{df}e", true), Some(0));
        assert_eq!(
            distance(
                "\u{3a3}\u{39f}\u{3a6}\u{399}\u{391}",
                "\u{3c3}\u{3bf}\u{3c6}\u{3b9}\u{3b1}",
                true
            ),
            Some(0)
        );
        assert_eq!(
            distance(
                "\u{41f}\u{401}\u{422}\u{420}",
                "\u{43f}\u{451}\u{442}\u{440}",
                true
            ),
            Some(0)
        );
        assert_eq!(distance("\u{c9}LCAIR", "\u{e9}clair", true), Some(1));
    }

    #[test]
    fn ignore_diacritics_matches_base_letters() {
        let distance = |query: &str, key: &str| {
            let automaton = Levenshtein::new(query, 2, 0)
                .fold_case(true)
                .ignore_diacritics(true);
            let mut state = automaton.start();
            for byte in key.bytes() {
                state = automaton.accept(&state, byte);
            }
            state.map(|state| state.dist())
        };
        assert_eq!(distance("Zoe", "Zo\u{eb}"), Some(0));
        assert_eq!(distance("Zo\u{e9}", "Zo\u{eb}"), Some(0));
        assert_eq!(distance("REN\u{c9}E", "Renee"), Some(0));
        assert_eq!(distance("NA\u{cf}VEE", "naive"), Some(1));
        assert_eq!(distance("Nescafee", "Nescaf\u{e9}"), Some(1));
        // `\u{f8}` has no decomposition.
        assert_eq!(distance("Soren", "S\u{f8}ren"), Some(1));
        // Punctuation is unaffected.
        assert_eq!(distance("\u{2019}\u{2014}\u{2026}", "'-."), None);
        // Off unless asked for.
        let automaton = Levenshtein::new("Zoe", 2, 0).fold_case(true);
        let mut state = automaton.start();
        for byte in "Zo\u{eb}".bytes() {
            state = automaton.accept(&state, byte);
        }
        assert_eq!(state.map(|state| state.dist()), Some(1));
    }

    #[test]
    fn distance_counts_characters_not_bytes() {
        assert_eq!(distance("Zoe", "Zo\u{eb}", false), Some(1));
        assert_eq!(distance("Zo\u{e9}", "Zo\u{eb}", false), Some(1));
        assert_eq!(distance("Soren", "S\u{f8}ren", false), Some(1));
        assert_eq!(distance("Rene", "Ren\u{e9}e", false), Some(1));
        assert_eq!(
            distance("\u{2014}\u{2026}", "\u{2026}\u{2014}", false),
            Some(1)
        );
        assert_eq!(distance("caf\u{e9}\u{2019}", "cafe'", false), Some(2));
    }

    #[test]
    fn leading_deletions_survive_the_prefix() {
        assert_eq!(distance_with_prefix("Aduke", "duke", true, 2), Some(1));
        assert_eq!(distance_with_prefix("Wat", "at", true, 2), Some(1));
        assert_eq!(distance_with_prefix("IPHONE", "phone", true, 2), Some(1));
        assert_eq!(distance_with_prefix("abxycd", "abcd", false, 2), Some(2));
    }

    #[test]
    fn deletions_anywhere_in_the_word_survive_distance_one() {
        for key in ["hick", "tick", "thck", "thik", "thic"] {
            let automaton = Levenshtein::new("thick", 1, 0);
            let mut state = automaton.start();
            for byte in key.bytes() {
                state = automaton.accept(&state, byte);
            }
            assert_eq!(state.map(|state| state.dist()), Some(1), "{}", key);
        }
    }

    const ALPHABET: [char; 22] = [
        'a',
        'b',
        'A',
        'B',
        'i',
        'I',
        '\u{131}',
        '\u{130}',
        '\u{e9}',
        '\u{c9}',
        '\u{df}',
        '\u{1e9e}',
        '\u{3c3}',
        '\u{3c2}',
        '\u{3a3}',
        '\u{2019}',
        '\u{201c}',
        '\u{2014}',
        '\u{2026}',
        '\u{2028}',
        '\u{65e5}',
        '\u{1f600}',
    ];

    fn word(indices: &[u8]) -> String {
        indices
            .iter()
            .take(7)
            .map(|i| ALPHABET[*i as usize % ALPHABET.len()])
            .collect()
    }

    fn reference_distance(query: &str, key: &str, fold_case: bool) -> usize {
        let chars = |s: &str| -> Vec<char> {
            s.chars()
                .map(|c| if fold_case { fold(c) } else { c })
                .collect()
        };
        let (a, b) = (chars(query), chars(key));
        let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];
        for (i, row) in d.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, cell) in d[0].iter_mut().enumerate() {
            *cell = j;
        }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let substitution = d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]);
                d[i][j] = substitution.min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
                }
            }
        }
        d[a.len()][b.len()]
    }

    #[quickcheck_macros::quickcheck]
    fn agrees_with_char_level_distance_on_multibyte_text(
        query: Vec<u8>,
        key: Vec<u8>,
        fold_case: bool,
    ) -> bool {
        let (query, key) = (word(&query), word(&key));
        let automaton = Levenshtein::new(&query, 2, 0).fold_case(fold_case);
        let mut state = automaton.start();
        for (i, byte) in key.bytes().enumerate() {
            state = automaton.accept(&state, byte);
            if !key.is_char_boundary(i + 1) && automaton.is_match(&state) {
                return false;
            }
        }
        let expected = reference_distance(&query, &key, fold_case);
        let dist = state.as_ref().map(LevenshteinState::dist);
        if expected <= 2 {
            dist == Some(expected) && automaton.is_match(&state)
        } else {
            dist.map_or(true, |dist| dist == expected) && !automaton.is_match(&state)
        }
    }
}
