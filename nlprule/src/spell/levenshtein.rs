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
    prev_byte: u8,
    row: Vec<usize>,
    hash: u64,
}

impl LevenshteinState {
    pub fn dist(&self) -> usize {
        self.dist
    }
}

#[derive(Debug, Clone)]
pub struct Levenshtein<'a> {
    query: &'a [u8],
    distance: usize,
    prefix: usize,
    fold_case: bool,
}

impl<'a> Levenshtein<'a> {
    pub fn new(query: &'a str, distance: usize, prefix: usize) -> Self {
        Levenshtein {
            query: query.as_bytes(),
            distance,
            prefix,
            fold_case: false,
        }
    }

    pub fn fold_case(mut self, fold_case: bool) -> Self {
        self.fold_case = fold_case;
        self
    }

    fn equal(&self, a: u8, b: u8) -> bool {
        a == b || (self.fold_case && a.eq_ignore_ascii_case(&b))
    }
}

impl<'a> Automaton for Levenshtein<'a> {
    type State = Option<LevenshteinState>;

    fn start(&self) -> Self::State {
        Some(LevenshteinState {
            dist: self.query.len(),
            n: 0,
            prev_row: None,
            prev_byte: 0,
            row: (0..=self.query.len()).collect(),
            hash: FnvHasher::default().finish(),
        })
    }

    fn is_match(&self, state: &Self::State) -> bool {
        state
            .as_ref()
            .map_or(false, |state| state.dist <= self.distance)
    }

    fn can_match(&self, state: &Self::State) -> bool {
        state.is_some()
    }

    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        state.as_ref().and_then(|state| {
            let mut next_hasher = FnvHasher::with_key(state.hash);
            byte.hash(&mut next_hasher);
            let next_hash = next_hasher.finish();

            let row = &state.row;
            let mut next_row = state.row.to_vec();

            next_row[0] = state.n + 1;

            for i in 1..next_row.len() {
                let mut cost = if self.equal(byte, self.query[i - 1]) {
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
                        if self.equal(byte, self.query[i - 2])
                            && self.equal(state.prev_byte, self.query[i - 1])
                        {
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
                prev_byte: byte,
                row: next_row,
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
        assert_eq!(distance("CAF\u{c9}", "caf\u{e9}", true), Some(1));
        assert_eq!(distance("GOIGN", "going", true), Some(1));
        assert_eq!(distance("WHTA", "what", true), Some(1));
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
}
