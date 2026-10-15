use std::io::{self, BufRead, Write};
use std::time::Instant;

use serde::Deserialize;
use writemorebetter_style::{analyze_document, dialogue_spans, DocumentOptions};

#[derive(Deserialize)]
struct Input {
    file: String,
    text: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    match args.first().map(String::as_str) {
        Some("--dialogue") if args.len() == 1 => {
            for line in io::stdin().lock().lines() {
                let input: Input = serde_json::from_str(&line?)?;
                for span in dialogue_spans(&input.text) {
                    serde_json::to_writer(
                        &mut output,
                        &serde_json::json!({
                            "file": input.file,
                            "byte": span.byte,
                            "utf16": span.utf16,
                            "pattern": "dialogue"
                        }),
                    )?;
                    writeln!(output)?;
                }
            }
        }
        Some("--benchmark") if args.len() == 3 || args.len() == 4 => {
            let iterations: usize = args.get(3).map(|n| n.parse()).transpose()?.unwrap_or(5);
            if iterations == 0 {
                return Err("iterations must be positive".into());
            }
            let text = std::fs::read_to_string(&args[2])?;
            let started = Instant::now();
            let tokenizer = nlprule::Tokenizer::new(&args[1])?;
            let load_ms = started.elapsed().as_secs_f64() * 1000.0;
            let options = DocumentOptions::default();
            drop(std::hint::black_box(analyze_document(
                &tokenizer, &text, &options,
            )));
            let mut times_ms = Vec::with_capacity(iterations);
            for _ in 0..iterations {
                let started = Instant::now();
                let document = std::hint::black_box(analyze_document(&tokenizer, &text, &options));
                times_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                drop(document);
            }
            times_ms.sort_by(f64::total_cmp);
            let middle = iterations / 2;
            let median_ms = if iterations % 2 == 0 {
                (times_ms[middle - 1] + times_ms[middle]) / 2.0
            } else {
                times_ms[middle]
            };
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "bytes": text.len(),
                    "iterations": iterations,
                    "warmups": 1,
                    "tokenizer_load_ms": load_ms,
                    "min_ms": times_ms[0],
                    "median_ms": median_ms,
                    "max_ms": times_ms[iterations - 1],
                    "times_ms": times_ms
                }),
            )?;
            writeln!(output)?;
        }
        _ => {
            return Err(
                "usage: wmb-document --dialogue | --benchmark TOKENIZER TEXT [ITERATIONS]".into(),
            )
        }
    }
    output.flush()?;
    Ok(())
}
