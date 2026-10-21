use std::io::{self, BufRead, Write};

use serde_json::json;
use writemorebetter_style::reports::{diction, overused_words, sensory_balance};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: lexical_reports TOKENIZER DICTION_TABLE".into());
    }
    let tokenizer = nlprule::Tokenizer::new(&args[0])?;
    let table = diction::ConcretenessTable::parse(&std::fs::read_to_string(&args[1])?)?;
    let norms = sensory_balance::PerceptualNorms::bundled();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line?)?;
        let document = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        let overused = overused_words::report(
            &document,
            &overused_words::OverusedWordsOptions::new(),
            overused_words::OverusedWordsBaseline::bundled(),
        )?;
        let sensory = sensory_balance::report(
            &document,
            norms,
            &sensory_balance::SensoryBalanceOptions::new(),
        )?;
        let diction = diction::report(&document, &table, &diction::DictionOptions::new())?;
        serde_json::to_writer(
            &mut output,
            &json!({"overusedWords": overused, "sensoryBalance": sensory, "diction": diction}),
        )?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
