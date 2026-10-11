use fnv::FnvHasher;
use fst::Automaton;
use std::{
    cmp::{self, min},
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct LevenshteinState {
    dist: usize,
    n: usize,
    // to compute the next row of the matrix, we also need the row two rows up for transposes
    prev_row: Option<Arc<[usize]>>,
    prev_byte: u8,
    row: Arc<[usize]>,
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
}

impl<'a> Levenshtein<'a> {
    pub fn new(query: &'a str, distance: usize, prefix: usize) -> Self {
        Levenshtein {
            query: query.as_bytes(),
            distance,
            prefix,
        }
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
            let mut left = state.n + 1;
            let next_row: Arc<[usize]> = (0..row.len())
                .map(|i| {
                    if i == 0 {
                        return left;
                    }
                    let mut cost = if byte == self.query[i - 1] {
                        row[i - 1]
                    } else {
                        min(
                            left + 1, // deletes
                            min(
                                row[i - 1] + 1, // inserts
                                row[i] + 1,     // substitutes
                            ),
                        )
                    };

                    if i > 1 {
                        // transposes
                        if let Some(prev_row) = state.prev_row.as_ref() {
                            if byte == self.query[i - 2] && state.prev_byte == self.query[i - 1] {
                                cost = min(cost, prev_row[i - 2] + 1);
                            }
                        }
                    }

                    left = cost;
                    cost
                })
                .collect();

            let distance = if state.n >= self.prefix {
                self.distance
            } else {
                1
            };

            let lower_bound = state.n.saturating_sub(distance);
            let upper_bound = cmp::min(state.n + distance, self.query.len());

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
    use fst::{IntoStreamer, Streamer};

    #[test]
    fn preserve_transposes_prefix_limits_and_utf8_search_distances() {
        let mut words = vec![
            "cat",
            "cats",
            "cast",
            "cta",
            "at",
            "cut",
            "dog",
            "cafe",
            "café",
            "can’t",
            "can't",
            "can—t",
            "can…t",
            "can\u{2028}t",
            "“cat”",
            "‘cat’",
            "cab",
            "ab",
            "ba",
            "ac",
            "ca",
        ];
        words.sort_unstable();
        let set = fst::Set::from_iter(words).unwrap();
        let mut actual = String::new();
        for (query, distance, prefix) in [
            ("cat", 1, 0),
            ("cta", 1, 0),
            ("cat", 2, 2),
            ("ab", 1, 0),
            ("cafe", 1, 0),
            ("café", 2, 2),
            ("can’t", 2, 2),
            ("“cat”", 2, 0),
            ("can\u{2028}t", 2, 0),
        ] {
            let mut stream = set
                .search_with_state(Levenshtein::new(query, distance, prefix))
                .into_stream();
            let mut matches = Vec::new();
            while let Some((word, state)) = stream.next() {
                matches.push((
                    std::str::from_utf8(word).unwrap().to_owned(),
                    state.unwrap().dist(),
                ));
            }
            actual.push_str(&format!("{query:?}/{distance}/{prefix}: {matches:?}\n"));
        }
        assert_eq!(
            actual,
            concat!(
                r#""cat"/1/0: [("ca", 1), ("cab", 1), ("cast", 1), ("cat", 0), ("cats", 1), ("cta", 1), ("cut", 1)]"#,
                "\n",
                r#""cta"/1/0: [("ca", 1), ("cat", 1), ("cta", 0)]"#,
                "\n",
                r#""cat"/2/2: [("ac", 2), ("ba", 2), ("ca", 1), ("cab", 1), ("cafe", 2), ("can't", 2), ("cast", 1), ("cat", 0), ("cats", 1), ("cta", 1), ("cut", 1)]"#,
                "\n",
                r#""ab"/1/0: [("ab", 0), ("ac", 1), ("at", 1), ("ba", 1), ("cab", 1)]"#,
                "\n",
                r#""cafe"/1/0: [("cafe", 0)]"#,
                "\n",
                r#""café"/2/2: [("cafe", 2), ("café", 0)]"#,
                "\n",
                r#""can’t"/2/2: [("can—t", 1), ("can’t", 0), ("can…t", 1), ("can\u{2028}t", 1)]"#,
                "\n",
                r#""“cat”"/2/0: [("‘cat’", 2), ("“cat”", 0)]"#,
                "\n",
                r#""can\u{2028}t"/2/0: [("can—t", 1), ("can’t", 1), ("can…t", 1), ("can\u{2028}t", 0)]"#,
                "\n",
            )
        );
    }
}
