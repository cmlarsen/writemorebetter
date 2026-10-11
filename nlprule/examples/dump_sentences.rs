//! Dumps sentence boundaries deterministically for identity checks.
//!
//! Usage: `cargo run --release --example dump_sentences -- <tokenizer.bin> <text file>`
//!
//! Prints, for every sentence from `tokenizer.pipe(text)`, the sentence's byte span
//! start and end and its text, separated by tabs. Newlines in the sentence text are
//! escaped as `\n` so every sentence occupies exactly one output line.

use std::env;
use std::fs;
use std::io::{self, Write};

use nlprule::Tokenizer;

fn main() {
    let mut args = env::args().skip(1);
    let tokenizer_path = args
        .next()
        .expect("usage: dump_sentences <tokenizer.bin> <text file>");
    let text_path = args
        .next()
        .expect("usage: dump_sentences <tokenizer.bin> <text file>");

    let tokenizer = Tokenizer::new(tokenizer_path).expect("failed to load tokenizer");
    let text = fs::read_to_string(&text_path).expect("failed to read text file");

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for sentence in tokenizer.pipe(&text) {
        let span = sentence.span().byte().clone();
        let escaped = sentence.text().replace('\n', "\\n");
        writeln!(out, "{}\t{}\t{}", span.start, span.end, escaped).unwrap();
    }
}
