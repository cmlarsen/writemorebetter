use std::io::{self, BufRead, Write};
use std::time::Instant;

use serde::Deserialize;
use writemorebetter_style::reports::echoes::{echoes, EchoOptions};
use writemorebetter_style::reports::phrases::{repeated_phrases, PhraseOptions};
use writemorebetter_style::{analyze_document, DocumentOptions, Span};

#[derive(Deserialize)]
struct Input {
    file: String,
    text: String,
}

fn span(span: &Span) -> serde_json::Value {
    serde_json::json!({"byte": span.byte, "utf16": span.utf16})
}

fn median(times: &mut [f64]) -> f64 {
    times.sort_by(f64::total_cmp);
    let middle = times.len() / 2;
    if times.len().is_multiple_of(2) {
        (times[middle - 1] + times[middle]) / 2.0
    } else {
        times[middle]
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    match args.as_slice() {
        [mode, tokenizer, text, rest @ ..] if mode == "--benchmark" => {
            let iterations: usize = rest
                .first()
                .filter(|n| !n.starts_with("--"))
                .map(|n| n.parse())
                .transpose()?
                .unwrap_or(5);
            if iterations == 0 {
                return Err("iterations must be positive".into());
            }
            let text = std::fs::read_to_string(text)?;
            let tokenizer = nlprule::Tokenizer::new(tokenizer)?;
            let phrase_options = PhraseOptions::new();
            let options =
                DocumentOptions::new().with_hard_wrapped(rest.iter().any(|arg| arg == "--hard-wrapped"));
            let warmup = analyze_document(&tokenizer, &text, &options);
            drop(std::hint::black_box((
                echoes(&warmup, &EchoOptions::new()),
                repeated_phrases(&warmup, &PhraseOptions::new()),
            )));
            let words = warmup.word_count;
            drop(warmup);
            let (mut document_ms, mut echoes_ms, mut phrases_ms) =
                (Vec::new(), Vec::new(), Vec::new());
            let (mut echo_count, mut density, mut phrase_count) = (0, 0.0, 0);
            for _ in 0..iterations {
                let started = Instant::now();
                let document = std::hint::black_box(analyze_document(&tokenizer, &text, &options));
                document_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let report = std::hint::black_box(echoes(&document, &EchoOptions::new()));
                echoes_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                let started = Instant::now();
                let phrases = std::hint::black_box(repeated_phrases(&document, &phrase_options));
                phrases_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                (echo_count, density, phrase_count) = (
                    report.echoes.len(),
                    report.density_per_1000,
                    phrases.phrases.len(),
                );
            }
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "bytes": text.len(),
                    "words": words,
                    "hard_wrapped": options.hard_wrapped,
                    "iterations": iterations,
                    "document_median_ms": median(&mut document_ms),
                    "echoes_median_ms": median(&mut echoes_ms),
                    "phrases_median_ms": median(&mut phrases_ms),
                    "echoes": echo_count,
                    "echo_density_per_1000": density,
                    "repeated_phrases": phrase_count
                }),
            )?;
            writeln!(output)?;
        }
        [tokenizer, rest @ ..] if !tokenizer.starts_with("--") => {
            let mut phrase_options = PhraseOptions::new();
            let mut document_options = DocumentOptions::new();
            let mut flags = rest.iter();
            while let Some(flag) = flags.next() {
                match flag.as_str() {
                    "--hard-wrapped" => document_options = document_options.with_hard_wrapped(true),
                    "--min-content-words" => {
                        let count = flags.next().ok_or("missing count")?.parse()?;
                        phrase_options = phrase_options.with_min_content_words(count);
                    }
                    _ => return Err(format!("unknown option {flag}").into()),
                }
            }
            let tokenizer = nlprule::Tokenizer::new(tokenizer)?;
            for line in io::stdin().lock().lines() {
                let input: Input = serde_json::from_str(&line?)?;
                let document = analyze_document(&tokenizer, &input.text, &document_options);
                for echo in echoes(&document, &EchoOptions::new()).echoes {
                    serde_json::to_writer(
                        &mut output,
                        &serde_json::json!({
                            "file": input.file,
                            "report": "echoes",
                            "pattern": "echo",
                            "key": echo.key,
                            "label": echo.label,
                            "spans": [span(&echo.earlier), span(&echo.later)]
                        }),
                    )?;
                    writeln!(output)?;
                }
                for phrase in repeated_phrases(&document, &phrase_options).phrases {
                    serde_json::to_writer(
                        &mut output,
                        &serde_json::json!({
                            "file": input.file,
                            "report": "repeated_phrases",
                            "pattern": "repeated_phrase",
                            "keys": phrase.keys,
                            "labels": phrase.labels,
                            "spans": phrase.spans.iter().map(span).collect::<Vec<_>>()
                        }),
                    )?;
                    writeln!(output)?;
                }
            }
        }
        _ => {
            return Err(
                "usage: wmb-echoes TOKENIZER [--hard-wrapped] [--min-content-words N] < documents.jsonl | wmb-echoes --benchmark TOKENIZER TEXT [ITERATIONS] [--hard-wrapped]"
                    .into(),
            )
        }
    }
    output.flush()?;
    Ok(())
}
