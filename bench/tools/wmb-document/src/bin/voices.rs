//! The voices probe (#353): one JSON document per line in, one line out with the voices report
//! and the Mode-based document stats it is compared against.
//!
//! In:  {"text": "...", "hardWrapped": false, "documentOptions": {...}, "options": {...}}
//! Out: {"voices": VoicesReport, "stats": DocumentStats, "moved": {voice: words}}, spans with
//! byte, char and UTF-16 ranges. `moved` counts the narration-mode words each voice takes out of
//! narration.
use std::io::{self, BufRead, Write};

use serde::Deserialize;
use writemorebetter_style::reports::voices::{self, VoicesOptions};
use writemorebetter_style::reports::DocumentStats;
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, Voice};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    text: String,
    #[serde(default)]
    hard_wrapped: bool,
    #[serde(default)]
    document_options: Option<DocumentOptions>,
    #[serde(default)]
    options: VoicesOptions,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: voices TOKENIZER < documents.jsonl".into());
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
        let report = voices::report(&document, &input.options)?;
        let stats = DocumentStats::new(&document);
        let mut moved: std::collections::BTreeMap<Voice, usize> = Default::default();
        for token in document.words() {
            let byte = token.span.byte.start;
            let voice = document.voice_at(byte);
            if document.mode_at(byte) == Mode::Narration && voice != Voice::Narration {
                *moved.entry(voice).or_default() += 1;
            }
        }
        writeln!(
            stdout,
            "{}",
            serde_json::json!({"voices": report, "stats": stats, "moved": moved})
        )?;
        stdout.flush()?;
    }
    Ok(())
}
