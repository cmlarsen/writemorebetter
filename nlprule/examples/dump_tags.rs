//! Dumps tagger output deterministically for identity checks.
//!
//! Usage: `cargo run --release --example dump_tags -- <tokenizer.bin> <text file>`
//!
//! Prints, for every sentence from `tokenizer.pipe(text)` and every token: the token
//! text, a tab, then every `(lemma, pos)` pair as `lemma/pos` joined by spaces, in the
//! order returned, then a second tab and the token's chunks joined by commas, in order.
//! Then a line `== RAW`, then for every distinct whitespace-separated word of the text in
//! sorted order: the word, a tab, then every `lemma/pos` from
//! `tokenizer.tagger().get_tags(word)` in order.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Write};

use nlprule::Tokenizer;

fn pairs(tags: impl Iterator<Item = (String, String)>) -> String {
    tags.map(|(lemma, pos)| format!("{}/{}", lemma, pos))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let mut args = env::args().skip(1);
    let tokenizer_path = args
        .next()
        .expect("usage: dump_tags <tokenizer.bin> <text file>");
    let text_path = args
        .next()
        .expect("usage: dump_tags <tokenizer.bin> <text file>");

    let tokenizer = Tokenizer::new(tokenizer_path).expect("failed to load tokenizer");
    let text = fs::read_to_string(&text_path).expect("failed to read text file");

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for sentence in tokenizer.pipe(&text) {
        for token in sentence.tokens() {
            let word = token.word();
            let joined = pairs(word.tags().iter().map(|data| {
                (
                    data.lemma().as_str().to_owned(),
                    data.pos().as_str().to_owned(),
                )
            }));
            let chunks = token.chunks().join(",");
            writeln!(out, "{}\t{}\t{}", word.text().as_str(), joined, chunks).unwrap();
        }
    }

    writeln!(out, "== RAW").unwrap();

    let words: BTreeSet<&str> = text.split_whitespace().collect();
    for word in words {
        let joined = pairs(tokenizer.tagger().get_tags(word).map(|data| {
            (
                data.lemma().as_str().to_owned(),
                data.pos().as_str().to_owned(),
            )
        }));
        writeln!(out, "{}\t{}", word, joined).unwrap();
    }
}
