use std::io::{self, BufRead, Write};
use std::time::Instant;

use serde::Deserialize;
use writemorebetter_style::{analyze_document, dialogue_spans_with_options, DocumentOptions};

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
        Some("--reports") if args.len() == 2 || (args.len() == 3 && args[2] == "--hard-wrapped") => {
            use writemorebetter_style::reports::{sentence_length, sentence_starts};
            let tokenizer = nlprule::Tokenizer::new(&args[1])?;
            for line in io::stdin().lock().lines() {
                let text: String = serde_json::from_str(&line?)?;
                let document = analyze_document(&tokenizer, &text, &DocumentOptions::new().with_hard_wrapped(args.len() == 3));
                let started = Instant::now();
                let length = sentence_length::report(&document, &sentence_length::SentenceLengthOptions::new().with_long_sentences(true), sentence_length::SentenceLengthBaseline::bundled().as_ref())?;
                let starts = sentence_starts::report(&document, &Default::default(), sentence_starts::SentenceStartsBaseline::bundled().as_ref())?;
                let reports = serde_json::json!({"sentenceLength": length, "sentenceStarts": starts});
                let reports_ms = started.elapsed().as_secs_f64() * 1000.0;
                serde_json::to_writer(&mut output, &serde_json::json!({"reports": reports, "reports_ms": reports_ms, "paragraphs": document.paragraphs.len()}))?;
                writeln!(output)?;
                output.flush()?;
            }
        }
        Some("--defaults") if args.len() == 1 => {
            use writemorebetter_style::reports::{sentence_length, sentence_starts};
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "sentenceLength": sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
                    "sentenceStarts": sentence_starts::SentenceStartsOptions::new(),
                    "monotonyMinSentences": sentence_length::MONOTONY_MIN_SENTENCES,
                    "monotonyMaxSpread": sentence_length::MONOTONY_MAX_SPREAD,
                    "sameWordMinSentences": sentence_starts::SAME_WORD_MIN_SENTENCES,
                }),
            )?;
            writeln!(output)?;
        }
        Some("--penn-map") if args.len() == 1 => {
            let tags: Vec<String> = serde_json::from_reader(io::stdin())?;
            let mapping: std::collections::BTreeMap<_,_> = tags.iter().map(|tag| (tag, writemorebetter_style::reports::sentence_starts::penn_to_universal(tag))).collect();
            serde_json::to_writer(&mut output, &mapping)?;
        }
        Some("--dialogue") if args.len() == 1 => {
            for line in io::stdin().lock().lines() {
                let input: Input = serde_json::from_str(&line?)?;
                for span in dialogue_spans_with_options(&input.text, &DocumentOptions::new().with_hard_wrapped(true)) {
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
        Some("--benchmark") if args.len() >= 3 => {
            let iterations: usize = args.get(3).filter(|n| !n.starts_with("--")).map(|n| n.parse()).transpose()?.unwrap_or(5);
            if iterations == 0 {
                return Err("iterations must be positive".into());
            }
            let text = std::fs::read_to_string(&args[2])?;
            let started = Instant::now();
            let tokenizer = nlprule::Tokenizer::new(&args[1])?;
            let load_ms = started.elapsed().as_secs_f64() * 1000.0;
            let options = DocumentOptions::new().with_token_details(!args.iter().any(|arg| arg == "--compact")).with_hard_wrapped(args.iter().any(|arg| arg == "--hard-wrapped"));
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
                    "include_token_details": options.include_token_details,
                    "hard_wrapped": options.hard_wrapped,
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
                "usage: wmb-document --dialogue | --reports TOKENIZER [--hard-wrapped] | --defaults | --penn-map | --benchmark TOKENIZER TEXT [ITERATIONS] [--compact] [--hard-wrapped]".into(),
            )
        }
    }
    output.flush()?;
    Ok(())
}
