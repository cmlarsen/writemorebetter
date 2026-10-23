//! The person-drift probe (#354): one JSON document per line in, one person-drift report per line
//! out, spans with byte, char and UTF-16 ranges.
//!
//! In:  {"text": "...", "hardWrapped": false, "documentOptions": {...}, "options": {...}}
//! Out: PersonDriftReport, or {"error": "..."} when the options are invalid (unparseable or
//! failing validation). A malformed line or document options still stop the probe.
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
    /// Parsed after the line so an options error becomes an `{"error"}` line.
    #[serde(default)]
    options: Option<serde_json::Value>,
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
        let parsed = match input.options {
            Some(value) => serde_json::from_value::<PersonDriftOptions>(value)
                .map_err(|error| format!("invalidOptions: personDrift: {error}")),
            None => Ok(PersonDriftOptions::default()),
        };
        let result = parsed.and_then(|options| {
            person_drift::report(&document, &options).map_err(|error| error.to_string())
        });
        match result {
            Ok(report) => writeln!(stdout, "{}", serde_json::to_string(&report)?)?,
            Err(error) => writeln!(stdout, "{}", serde_json::json!({"error": error}))?,
        }
        stdout.flush()?;
    }
    Ok(())
}
