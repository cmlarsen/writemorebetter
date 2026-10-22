//! Replacement pairs: common misspellings of letter groups (`ph` for `f`, `ei` for `ie`) that the
//! suggestion search treats as cheap edits, ported from Morfologik's `Speller`.
//!
//! The pairs come from the speller dictionary's `.info` file
//! (`fsa.dict.speller.replacement-pairs`) and live in the data binary. A pair `from to` says that
//! `from` in a misspelling may stand for `to` in the correct word. Pairs whose `to` has one or
//! two characters are zero-cost edits inside the edit-distance search; longer ones rewrite the
//! whole word before the search, and each rewrite is looked up and searched as well.
use serde::{Deserialize, Serialize};

/// Morfologik's limit on the recursion depth of [ReplacementPairs::rewrites].
const MAX_RECURSION_LEVEL: usize = 6;

/// One variant's replacement pairs, split the way Morfologik's `Speller` uses them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct ReplacementPairs {
    /// Pairs whose replacement has one or two characters: `(from, to)`.
    edits: Vec<(Vec<char>, Vec<char>)>,
    /// Pairs with a longer replacement, keyed by `from` in first-seen order.
    rewrites: Vec<(String, Vec<String>)>,
}

/// A zero-cost edit found in one query: the query characters `start..end` stand for the
/// candidate characters `to` (one or two).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Edit {
    pub start: usize,
    pub end: usize,
    pub to: [char; 2],
    pub to_len: usize,
    /// How much further the query is than the key after this edit: `end - start - to_len`.
    pub shift: i8,
}

impl ReplacementPairs {
    /// Builds the pairs from `(from, to)` in file order.
    pub fn new(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut out = ReplacementPairs::default();
        for (from, to) in pairs {
            if from.is_empty() || to.is_empty() {
                continue;
            }
            if to.chars().count() <= 2 {
                out.edits
                    .push((from.chars().collect(), to.chars().collect()));
            } else if let Some((_, tos)) = out.rewrites.iter_mut().find(|(key, _)| *key == from) {
                tos.push(to);
            } else {
                out.rewrites.push((from, vec![to]));
            }
        }
        out
    }

    /// The zero-cost edits that apply to `query`, indexed by the query position where the
    /// replaced characters start (`0..=query.len()`). Matching is case-sensitive, as in
    /// Morfologik.
    pub fn edits_for(&self, query: &[char]) -> Vec<Vec<Edit>> {
        let mut by_start = vec![Vec::new(); query.len() + 1];
        for (from, to) in &self.edits {
            if from.len() > query.len() {
                continue;
            }
            for start in 0..=query.len() - from.len() {
                if query[start..start + from.len()] == from[..] {
                    let mut chars = ['\0'; 2];
                    chars[..to.len()].copy_from_slice(to);
                    by_start[start].push(Edit {
                        start,
                        end: start + from.len(),
                        to: chars,
                        to_len: to.len(),
                        shift: (from.len() as i64 - to.len() as i64).clamp(-8, 8) as i8,
                    });
                }
            }
        }
        by_start
    }

    /// The words to search for `word`: `word` itself first, then every combination of the
    /// longer replacements, as Morfologik's `Speller.getAllReplacements` builds them.
    pub fn rewrites(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        if self.rewrites.is_empty() {
            out.push(word.to_owned());
        } else {
            self.rewrite(word, 0, 0, &mut out);
        }
        out
    }

    fn rewrite(&self, word: &str, from: usize, level: usize, out: &mut Vec<String>) {
        if level > MAX_RECURSION_LEVEL {
            out.push(word.to_owned());
            return;
        }
        // The first key found at or after `from`; at the same position the longest key wins,
        // and among keys of equal length the last one.
        let mut first: Option<(usize, &str, &[String])> = None;
        for (key, tos) in &self.rewrites {
            if let Some(index) = word.get(from..).and_then(|rest| rest.find(key.as_str())) {
                let index = from + index;
                let better = first.is_none_or(|(best, best_key, _)| {
                    index < best || (index == best && key.len() >= best_key.len())
                });
                if better {
                    first = Some((index, key, tos));
                }
            }
        }
        let Some((index, key, tos)) = first else {
            out.push(word.to_owned());
            return;
        };

        // A branch without the replacement, then one per replacement.
        self.rewrite(word, index + key.len(), level + 1, out);
        for to in tos {
            // Skip a replacement already present around the key (do not replace `L` by `L·L`
            // when `L·L` is there).
            if to.len() > key.len() {
                let mut search_from = from.saturating_sub(to.len() - 1);
                while !word.is_char_boundary(search_from) {
                    search_from -= 1;
                }
                if let Some(found) = word[search_from..].find(to.as_str()) {
                    let found = search_from + found;
                    // At the key, or ending with the key's first character.
                    let first_len = word[index..].chars().next().map_or(1, char::len_utf8);
                    if found == index || found + to.len() == index + first_len {
                        continue;
                    }
                }
            }
            let replaced = format!("{}{}{}", &word[..index], to, &word[index + key.len()..]);
            self.rewrite(&replaced, index + to.len(), level + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Invented pairs in the `.info` format (not a dictionary's own list).
    fn pairs() -> ReplacementPairs {
        ReplacementPairs::new(
            "x ks, ks x, v w, w v, j dg, ung ing, ung ang, ip ipe"
                .split(", ")
                .map(|pair| {
                    let (from, to) = pair.split_once(' ').unwrap();
                    (from.to_owned(), to.to_owned())
                }),
        )
    }

    #[test]
    fn short_replacements_are_edits_and_long_ones_rewrites() {
        let pairs = pairs();
        assert_eq!(pairs.edits.len(), 5);
        assert_eq!(
            pairs.rewrites[0],
            ("ung".to_owned(), vec!["ing".to_owned(), "ang".to_owned()])
        );
    }

    #[test]
    fn edits_are_indexed_by_their_start() {
        let query: Vec<char> = "jake".chars().collect();
        let edits = pairs().edits_for(&query);
        assert_eq!(
            edits[0],
            vec![Edit {
                start: 0,
                end: 1,
                to: ['d', 'g'],
                to_len: 2,
                shift: -1,
            }]
        );
        assert!(edits[1].is_empty());
        // Case-sensitive, as in Morfologik.
        assert!(pairs().edits_for(&['J', 'a']).iter().all(Vec::is_empty));
    }

    #[test]
    fn rewrites_keep_the_word_first_and_combine_replacements() {
        let pairs = pairs();
        assert_eq!(pairs.rewrites("rip"), vec!["rip", "ripe"]);
        assert_eq!(pairs.rewrites("sung"), vec!["sung", "sing", "sang"]);
        assert_eq!(
            pairs.rewrites("sung\u{2019}s rip"),
            vec![
                "sung\u{2019}s rip",
                "sung\u{2019}s ripe",
                "sing\u{2019}s rip",
                "sing\u{2019}s ripe",
                "sang\u{2019}s rip",
                "sang\u{2019}s ripe",
            ]
        );
        assert_eq!(pairs.rewrites("zzz"), vec!["zzz"]);
        assert_eq!(
            pairs.rewrites("Zo\u{eb}\u{2014}\u{2026}"),
            vec!["Zo\u{eb}\u{2014}\u{2026}"]
        );
    }

    #[test]
    fn rewrites_skip_a_replacement_already_present() {
        let pairs = ReplacementPairs::new([("l".to_owned(), "l\u{b7}l".to_owned())]);
        assert_eq!(pairs.rewrites("l\u{b7}l"), vec!["l\u{b7}l"]);
        assert_eq!(pairs.rewrites("al"), vec!["al", "al\u{b7}l"]);
    }
}
