//! Prints readability values for each file argument as a Markdown table row.
//! `cargo run --release -p writemorebetter-style --example readability_report -- bench/public/*.txt`
use nlprule::Tokenizer;
use writemorebetter_style::reports::readability::{self, ReadabilityOptions, ReadabilityScores};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn cell(value: Option<f64>) -> String {
    value.map_or("–".to_owned(), |value| format!("{value:.1}"))
}

fn row(name: &str, scores: &ReadabilityScores) -> String {
    format!(
        "| {name} | {} | {} | {:.2} | {} | {} | {} | {} | {:.1} |",
        scores.sentences,
        scores.words,
        scores.syllables as f64 / scores.words.max(1) as f64,
        cell(scores.flesch_reading_ease),
        cell(scores.flesch_kincaid_grade),
        cell(scores.coleman_liau),
        cell(scores.automated_readability),
        scores.reading_minutes,
    )
}

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    println!("| Text | Sentences | Words | Syll./word | FRE | FKGL | CLI | ARI | Minutes |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    let mut paragraphs = Vec::new();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let report = readability::report(&document, &ReadabilityOptions::new()).unwrap();
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
        let excluded = readability::report(&document, &excluded).unwrap();
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
