//! Filters CMUdict first phonemes down to the words the spelling heuristic gets wrong.
//! Run by `scripts/build-cmudict.py`; reads `word\tsound` lines and prints the kept lines.
//! A word is kept unless `initial_sound_with` already returns its sound from the words kept so
//! far, and kept words are added until every input word gets its sound.
use std::collections::HashMap;
use writemorebetter_style::pronunciation::initial_sound_with;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: cmudict_onsets full.tsv");
    let text = std::fs::read_to_string(path).unwrap();
    let full: Vec<(&str, &str)> = text
        .lines()
        .map(|line| line.split_once('\t').unwrap())
        .collect();
    let all: HashMap<&str, &str> = full.iter().copied().collect();
    // First pass: keep a word when the spelling, helped by every other headword, misses it.
    let mut kept: HashMap<&str, &str> = full
        .iter()
        .copied()
        .filter(|&(word, sound)| {
            let others = |key: &str| (key != word).then(|| all.get(key).copied()).flatten();
            initial_sound_with(word, &others) != sound
        })
        .collect();
    // Then add back any word that dropping its first part or stem broke, until none is left.
    loop {
        let lookup = |key: &str| kept.get(key).copied();
        let missing: Vec<(&str, &str)> = full
            .iter()
            .copied()
            .filter(|&(word, sound)| initial_sound_with(word, &lookup) != sound)
            .collect();
        if missing.is_empty() {
            break;
        }
        kept.extend(missing);
    }
    let mut kept: Vec<_> = kept.into_iter().collect();
    kept.sort();
    for (word, sound) in kept {
        println!("{word}\t{sound}");
    }
}
