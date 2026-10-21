//! Prints readability values for each file argument as a Markdown table row.
//! `cargo run --release -p writemorebetter-style --example readability_report -- bench/public/*.txt`
//! `--heuristic` first counts syllables with the phase 1 heuristic alone; `--timing` instead
//! prints the time per report with each counter.
use std::time::Instant;

use nlprule::Tokenizer;
use writemorebetter_style::reports::readability::{
    self, DictionarySyllables, HeuristicSyllables, ReadabilityOptions, ReadabilityScores,
    SyllableCounter,
};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn cell(value: Option<f64>) -> String {
    value.map_or("–".to_owned(), |value| format!("{value:.1}"))
}

fn row(name: &str, scores: &ReadabilityScores) -> String {
    format!(
        "| {name} | {} | {} | {} | {:.2} | {} | {} | {} | {} | {:.1} |",
        scores.sentences,
        scores.words,
        scores.syllables,
        scores.syllables as f64 / scores.words.max(1) as f64,
        cell(scores.flesch_reading_ease),
        cell(scores.flesch_kincaid_grade),
        cell(scores.coleman_liau),
        cell(scores.automated_readability),
        scores.reading_minutes,
    )
}

fn timing(tokenizer: &Tokenizer, paths: &[String]) {
    let start = Instant::now();
    DictionarySyllables.syllables("a");
    println!(
        "table load: {:.2} ms ({} keys)",
        start.elapsed().as_secs_f64() * 1000.0,
        writemorebetter_style::pronunciation::syllable_exception_count()
    );
    println!("| Text | Heuristic ms | Dictionary ms | Words counted differently |");
    println!("|---|---:|---:|---:|");
    for path in paths {
        let text = std::fs::read_to_string(path).unwrap();
        let document = analyze_document(
            tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let median = |counter: &dyn SyllableCounter| {
            let mut times: Vec<f64> = (0..21)
                .map(|_| {
                    let start = Instant::now();
                    readability::report_with(&document, &ReadabilityOptions::new(), counter)
                        .unwrap();
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            times.sort_by(f64::total_cmp);
            times[10]
        };
        let changed = document
            .sentences()
            .flat_map(readability::sentence_words)
            .filter(|word| {
                HeuristicSyllables.syllables(word) != DictionarySyllables.syllables(word)
            })
            .count();
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".txt");
        println!(
            "| {name} | {:.2} | {:.2} | {changed} |",
            median(&HeuristicSyllables),
            median(&DictionarySyllables)
        );
    }
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--timing") {
        return timing(&tokenizer, &args[1..]);
    }
    let heuristic = args.first().is_some_and(|arg| arg == "--heuristic");
    if heuristic {
        args.remove(0);
    }
    let counter: &dyn SyllableCounter = if heuristic {
        &HeuristicSyllables
    } else {
        &DictionarySyllables
    };
    println!(
        "| Text | Sentences | Words | Syllables | Syll./word | FRE | FKGL | CLI | ARI | Minutes |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let mut paragraphs = Vec::new();
    for path in args {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let report =
            readability::report_with(&document, &ReadabilityOptions::new(), counter).unwrap();
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".txt");
        println!("{}", row(name, &report.groups.all));
        println!(
            "{}",
            row(&format!("{name} narration"), &report.groups.narration)
        );
        println!(
            "{}",
            row(&format!("{name} dialogue"), &report.groups.dialogue)
        );
        let excluded = ReadabilityOptions::new().with_exclude_dialogue_paragraphs(true);
        let excluded = readability::report_with(&document, &excluded, counter).unwrap();
        println!(
            "{}",
            row(
                &format!("{name} without dialogue-only paragraphs"),
                &excluded.groups.all
            )
        );
        let grades: Vec<f64> = report
            .paragraphs
            .iter()
            .filter(|p| !p.scores.low_confidence)
            .filter_map(|p| p.scores.flesch_kincaid_grade)
            .collect();
        paragraphs.push((name.to_owned(), report.paragraphs.len(), grades));
    }
    println!();
    println!("| Text | Paragraphs | Scored (≥30 words) | FKGL min | median | max |");
    println!("|---|---:|---:|---:|---:|---:|");
    for (name, total, mut grades) in paragraphs {
        grades.sort_by(f64::total_cmp);
        let at = |q: f64| {
            grades
                .get(((grades.len() as f64 - 1.0) * q).round() as usize)
                .copied()
        };
        println!(
            "| {name} | {total} | {} | {} | {} | {} |",
            grades.len(),
            cell(at(0.0)),
            cell(at(0.5)),
            cell(at(1.0))
        );
    }
}
