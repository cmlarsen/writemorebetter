use fnv::FnvHasher;
use fst::Automaton;
use std::{
    cmp::{self, min},
    hash::{Hash, Hasher},
};

#[derive(Clone, Debug)]
pub struct LevenshteinState {
    dist: usize,
    n: usize,
    // to compute the next row of the matrix, we also need the row two rows up for transposes
    prev_row: Option<Vec<usize>>,
    prev_char: char,
    row: Vec<usize>,
    // the bytes of a multi-byte UTF-8 character read so far
    pending: [u8; 4],
    pending_len: usize,
    hash: u64,
}

impl LevenshteinState {
    pub fn dist(&self) -> usize {
        self.dist
    }
}

#[derive(Debug, Clone)]
pub struct Levenshtein {
    query: Vec<char>,
    distance: usize,
    prefix: usize,
    fold_case: bool,
}

impl Levenshtein {
    pub fn new(query: &str, distance: usize, prefix: usize) -> Self {
        Levenshtein {
            query: query.chars().collect(),
            distance,
            prefix,
            fold_case: false,
        }
    }

    pub fn fold_case(mut self, fold_case: bool) -> Self {
        if fold_case && !self.fold_case {
            self.query.iter_mut().for_each(|c| *c = fold(*c));
        }
        self.fold_case = fold_case;
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
            row: (0..=self.query.len()).collect(),
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
            let c = self.normalize(c);

            let row = &state.row;
            let mut next_row = state.row.to_vec();

            next_row[0] = state.n + 1;

            for i in 1..next_row.len() {
                let mut cost = if c == self.query[i - 1] {
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

            let distance = if state.n >= self.prefix {
                self.distance
            } else {
                1
            };

            let lower_bound = (state.n + 1).saturating_sub(distance);
            let upper_bound = cmp::min(state.n + 1 + distance, self.query.len());

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
                dist: next_row[self.query.len()],
                n: state.n + 1,
                prev_row: Some(row.clone()),
                prev_char: c,
                row: next_row,
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
