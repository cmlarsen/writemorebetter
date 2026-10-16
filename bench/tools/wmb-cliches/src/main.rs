use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Write};
use std::time::Instant;

use nlprule::{Rules, Tokenizer};
use serde::Deserialize;
use writemorebetter_style::reports::cliches::{cliches, vendored_phrase_entries, PhraseList};
use writemorebetter_style::{analyze_document, DocumentOptions};

#[derive(Deserialize)]
struct Input {
    file: String,
    text: String,
}

const FRAMES: &[&str] = &[
    "{}.",
    "She told him {} and left.",
    "They said it was {} after all.",
];

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

fn covered(
    tokenizer: &Tokenizer,
    rules_path: &str,
    all: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rules = Rules::new(rules_path)?;
    let mut categories = BTreeMap::new();
    for rule in rules.rules_mut() {
        categories.insert(
            rule.id().to_string(),
            (
                rule.category_name().to_owned(),
                rule.category_type().unwrap_or("").to_owned(),
                rule.enabled(),
            ),
        );
        if all {
            rule.enable();
        }
    }
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for entry in vendored_phrase_entries() {
        let mut hits = BTreeSet::new();
        for frame in FRAMES {
            let phrase = if frame.starts_with("{}") {
                capitalize(&entry.phrase)
            } else {
                entry.phrase.clone()
            };
            let text = frame.replace("{}", &phrase);
            let start = text.find(&phrase).unwrap();
            let start = text[..start].chars().count();
            let end = start + phrase.chars().count();
            for suggestion in rules.suggest(&text, tokenizer) {
                let span = suggestion.span().char();
                if span.start < end && start < span.end {
                    let (category, kind, enabled) = categories
                        .get(suggestion.source())
                        .cloned()
                        .unwrap_or_else(|| ("builtin".into(), String::new(), true));
                    hits.insert((suggestion.source().to_owned(), category, kind, enabled));
                }
            }
        }
        for (source, category, kind, enabled) in hits {
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "phrase": entry.phrase,
                    "kind": entry.kind,
                    "rule": source,
                    "category": category,
                    "category_type": kind,
                    "enabled_by_default": enabled,
                }),
            )?;
            writeln!(output)?;
        }
    }
    output.flush()?;
    Ok(())
}

fn predict(tokenizer: &Tokenizer, hard_wrapped: bool) -> Result<(), Box<dyn std::error::Error>> {
    let phrases = PhraseList::bundled(tokenizer);
    let options = DocumentOptions::new()
        .with_token_details(false)
        .with_hard_wrapped(hard_wrapped);
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let input: Input = serde_json::from_str(&line?)?;
        let document = analyze_document(tokenizer, &input.text, &options);
        for found in cliches(&document, &phrases).matches {
            serde_json::to_writer(
                &mut output,
                &serde_json::json!({
                    "file": input.file,
                    "byte": found.span.byte,
                    "utf16": found.span.utf16,
                    "pattern": found.kind,
                    "report": "cliches",
                    "context": found.context,
                    "phrase": found.phrase,
                }),
            )?;
            writeln!(output)?;
        }
        output.flush()?;
    }
    Ok(())
}

fn benchmark(tokenizer: &Tokenizer, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let started = Instant::now();
    let phrases = PhraseList::bundled(tokenizer);
    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
    let options = DocumentOptions::new().with_token_details(false);
    let started = Instant::now();
    let document = analyze_document(tokenizer, &text, &options);
    let analyze_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut times_ms = Vec::new();
    let mut matches = 0;
    for _ in 0..5 {
        let started = Instant::now();
        matches = std::hint::black_box(cliches(&document, &phrases))
            .matches
            .len();
        times_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    times_ms.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "words": document.word_count,
            "patterns": phrases.len(),
            "matches": matches,
            "compile_ms": compile_ms,
            "analyze_document_ms": analyze_ms,
            "report_median_ms": times_ms[2],
        })
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--predict", tokenizer, rest @ ..] if rest.iter().all(|arg| *arg == "--hard-wrapped") => {
            predict(&Tokenizer::new(tokenizer)?, !rest.is_empty())
        }
        ["--covered", tokenizer, rules, rest @ ..] if rest.iter().all(|arg| *arg == "--all") => {
            covered(&Tokenizer::new(tokenizer)?, rules, !rest.is_empty())
        }
        ["--benchmark", tokenizer, text] => benchmark(&Tokenizer::new(tokenizer)?, text),
        _ => Err("usage: wmb-cliches --predict TOKENIZER [--hard-wrapped] | --covered TOKENIZER RULES [--all] | --benchmark TOKENIZER TEXT".into()),
    }
}
