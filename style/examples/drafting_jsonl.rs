//! Line-oriented release probe for `bench/style/drafting.py`.
//!
//! Reads one JSON object per line: `{id, text, options?}`. Writes `{id, report,
//! applied}`, where `applied` is the text after every edit, produced by
//! `drafting::apply` so the harness scores the same edits hosts receive.

use std::io::{self, BufRead};

use nlprule::Tokenizer;
use serde::Deserialize;
use writemorebetter_style::reports::drafting::{self, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
struct Input {
    id: String,
    text: String,
    #[serde(default)]
    options: DraftingOptions,
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").expect("English tokenizer");
    for line in io::stdin().lock().lines() {
        let line = line.expect("probe input line");
        if line.trim().is_empty() {
            continue;
        }
        let input: Input = serde_json::from_str(&line).expect("probe input JSON");
        let document = analyze_document(&tokenizer, &input.text, &DocumentOptions::new());
        let report = drafting::report(&document, &input.options).expect("valid options");
        let edits: Vec<_> = report
            .matches
            .iter()
            .flat_map(|found| &found.edits)
            .collect();
        let applied = drafting::apply(&input.text, &edits);
        println!(
            "{}",
            serde_json::json!({"id": input.id, "report": report, "applied": applied})
        );
    }
}
