//! Filters CMUdict syllable counts down to the words the heuristic gets wrong.
//! Run by `scripts/build-cmudict.py`; reads `word\tcount` lines and prints the kept lines.
//! A word is kept unless `syllables_with` already returns its count from the words kept so far,
//! and kept words are added until every input word gets its count.
use std::collections::HashMap;
use writemorebetter_style::reports::readability::syllables_with;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: cmudict_syllables full.tsv");
    let text = std::fs::read_to_string(path).unwrap();
    let full: Vec<(&str, usize)> = text
        .lines()
        .map(|line| {
            let (word, count) = line.split_once('\t').unwrap();
            (word, count.parse().unwrap())
        })
        .collect();
    let all: HashMap<&str, usize> = full.iter().copied().collect();
    // First pass: keep a word when the heuristic, helped by every other headword, misses it.
    let mut kept: HashMap<&str, usize> = full
        .iter()
        .copied()
        .filter(|&(word, count)| {
            let others = |key: &str| (key != word).then(|| all.get(key).copied()).flatten();
            syllables_with(word, &others) != count
        })
        .collect();
    // Then add back any word that dropping its parts or stem broke, until none is left.
    loop {
        let lookup = |key: &str| kept.get(key).copied();
        let missing: Vec<(&str, usize)> = full
            .iter()
            .copied()
            .filter(|&(word, count)| syllables_with(word, &lookup) != count)
            .collect();
        if missing.is_empty() {
            break;
        }
        kept.extend(missing);
    }
    let mut kept: Vec<_> = kept.into_iter().collect();
    kept.sort();
    for (word, count) in kept {
        println!("{word}\t{count}");
    }
}
