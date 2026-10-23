//! The person-drift probe (#354): one JSON document per line in, one person-drift report per line
//! out, spans with byte, char and UTF-16 ranges.
//!
//! In:  {"text": "...", "hardWrapped": false, "documentOptions": {...}, "options": {...}}
//! Out: PersonDriftReport, or {"error": "..."} when the options are invalid.
use std::io::{self, BufRead, Write};

use serde::Deserialize;
use writemorebetter_style::reports::person_drift::{self, PersonDriftOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    text: String,
    #[serde(default)]
    hard_wrapped: bool,
    #[serde(default)]
    document_options: Option<DocumentOptions>,
    #[serde(default)]
    options: PersonDriftOptions,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: person_drift TOKENIZER < documents.jsonl".into());
    };
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let options = input
            .document_options
            .unwrap_or_default()
            .with_hard_wrapped(input.hard_wrapped);
        let document = analyze_document(&tokenizer, &input.text, &options);
        match person_drift::report(&document, &input.options) {
            Ok(report) => writeln!(stdout, "{}", serde_json::to_string(&report)?)?,
            Err(error) => writeln!(
                stdout,
                "{}",
                serde_json::json!({"error": error.to_string()})
            )?,
        }
        stdout.flush()?;
    }
    Ok(())
}
