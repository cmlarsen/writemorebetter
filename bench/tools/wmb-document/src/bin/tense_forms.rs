//! The tense-form helper (#355): confirms re-inflected verb forms for the tense-shift
//! perturbations. A candidate is accepted only when the tagger gives it the requested lemma and
//! POS, so the bench never writes a form the tagger would read as something else.
//!
//! In:  {"lemma": "walk", "pos": "VBZ", "candidates": ["walks"]} per line
//! Out: {"form": "walks"} (the first confirmed candidate) or {"form": null}
use std::io::{self, BufRead, Write};

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Input {
    lemma: String,
    pos: String,
    candidates: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: tense_forms TOKENIZER < forms.jsonl")?;
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let tagger = tokenizer.tagger();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let form = input.candidates.iter().find(|candidate| {
            tagger.get_tags(&candidate.to_lowercase()).any(|data| {
                data.lemma().as_str() == input.lemma && data.pos().as_str() == input.pos
            })
        });
        writeln!(stdout, "{}", json!({ "form": form }))?;
    }
    stdout.flush()?;
    Ok(())
}
