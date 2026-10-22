use nlprule::Tokenizer;
use writemorebetter_style::reports::regional_spelling;
use writemorebetter_style::{analyze_document, DocumentOptions};

/// Prints marker, dominant-system and flag counts for each file: `cargo run --example
/// regional_spelling -- bench/public/*.txt`. Pass `--flags` to list each flagged form.
fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let list = std::env::args().any(|arg| arg == "--flags");
    for path in std::env::args().skip(1).filter(|arg| arg != "--flags") {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        let report = regional_spelling::report(&document, &Default::default()).unwrap();
        let systems: Vec<_> = report
            .systems
            .iter()
            .map(|count| format!("{:?} {}", count.system, count.count))
            .collect();
        println!(
            "{path}: {} words, {} markers ({}), dominant {:?}, {} flags, {} mixed words",
            document.word_count,
            report.markers,
            systems.join(", "),
            report.dominant,
            report.flags.len(),
            report.words.iter().filter(|word| word.mixed).count(),
        );
        if list {
            for flag in &report.flags {
                println!(
                    "  {} ({:?}, {:?}) -> {:?}",
                    flag.form, flag.systems, flag.reason, flag.dominant_form
                );
            }
        }
    }
}
