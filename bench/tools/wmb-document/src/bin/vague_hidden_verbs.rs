use std::io::{self, BufRead, Write};

use serde_json::json;
use writemorebetter_style::reports::{hidden_verbs, vague_words};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: vague_hidden_verbs TOKENIZER < texts.jsonl".into());
    };
    let tokenizer = nlprule::Tokenizer::new(path)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line?)?;
        let document = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        let vague = vague_words::report(&document, &vague_words::VagueWordsOptions::new())?;
        let hidden = hidden_verbs::report(&document, &hidden_verbs::HiddenVerbsOptions::new())?;
        serde_json::to_writer(
            &mut stdout,
            &json!({"vagueWords": vague, "hiddenVerbs": hidden}),
        )?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
