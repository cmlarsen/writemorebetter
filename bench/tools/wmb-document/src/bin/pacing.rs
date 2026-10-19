use std::io::{self, BufRead, Write};

use serde::Deserialize;
use serde_json::json;
use writemorebetter_style::reports::pacing::{self, PacingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    text: String,
    #[serde(default)]
    hard_wrapped: bool,
    #[serde(default)]
    options: PacingOptions,
    #[serde(default)]
    sweep: Vec<PacingOptions>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--defaults"] {
        println!("{}", serde_json::to_string(&PacingOptions::new())?);
        return Ok(());
    }
    let [path] = args.as_slice() else {
        return Err("usage: pacing TOKENIZER < documents.jsonl".into());
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
        let report = pacing::report(&document, &input.options)?;
        if input.sweep.is_empty() {
            writeln!(stdout, "{}", serde_json::to_string(&report)?)?;
        } else {
            let sweep = input
                .sweep
                .iter()
                .map(|options| {
                    pacing::report(&document, options).map(|swept| {
                        json!({
                            "runs": swept.slow_runs.len(),
                            "runParagraphs": swept
                                .slow_runs
                                .iter()
                                .map(|run| run.paragraphs.end - run.paragraphs.start)
                                .sum::<usize>(),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            writeln!(
                stdout,
                "{}",
                serde_json::to_string(&json!({"report": report, "sweep": sweep}))?
            )?;
        }
        stdout.flush()?;
    }
    Ok(())
}
