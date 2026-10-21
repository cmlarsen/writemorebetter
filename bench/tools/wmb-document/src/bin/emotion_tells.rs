use std::io::{self, BufRead, Write};

use serde::Deserialize;
use writemorebetter_style::reports::emotion_tells::{self, EmotionTellsOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    text: String,
    #[serde(default)]
    hard_wrapped: bool,
    #[serde(default)]
    options: EmotionTellsOptions,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: emotion_tells TOKENIZER < documents.jsonl".into());
    };
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let document = analyze_document(
            &tokenizer,
            &input.text,
            &DocumentOptions::new().with_hard_wrapped(input.hard_wrapped),
        );
        let report = emotion_tells::report(&document, &input.options)?;
        writeln!(stdout, "{}", serde_json::to_string(&report)?)?;
        stdout.flush()?;
    }
    Ok(())
}
