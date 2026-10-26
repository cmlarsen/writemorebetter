//! Line-oriented release probe for `bench/style/name_spelling.py`.

use std::io::{self, BufRead};
use std::time::Instant;

use nlprule::Tokenizer;
use serde::Deserialize;
use writemorebetter_style::reports::name_spelling::{self, NameSpellingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
struct Input {
    file: String,
    text: String,
    names: Vec<String>,
    #[serde(default)]
    options: NameSpellingOptions,
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").expect("English tokenizer");
    for line in io::stdin().lock().lines() {
        let line = line.expect("probe input line");
        if line.trim().is_empty() {
            continue;
        }
        let input: Input = serde_json::from_str(&line).expect("probe input JSON");
        let document = analyze_document(
            &tokenizer,
            &input.text,
            &DocumentOptions::new().with_skip_names(input.names),
        );
        let started = Instant::now();
        let report = name_spelling::report(&document, &input.options).expect("valid options");
        let report_ns = started.elapsed().as_nanos();
        println!(
            "{}",
            serde_json::json!({"file": input.file, "reportNs": report_ns, "flags": report.flags, "names": report.names})
        );
    }
}
