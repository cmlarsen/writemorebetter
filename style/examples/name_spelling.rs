use nlprule::Tokenizer;
use std::time::Instant;
use writemorebetter_style::reports::name_spelling;
use writemorebetter_style::{analyze_document, DocumentOptions};

/// `cargo run --release -p writemorebetter-style --example name_spelling -- Mara Zoë -- files...`
fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let mut args = std::env::args().skip(1);
    let mut names = Vec::new();
    for arg in args.by_ref() {
        if arg == "--" {
            break;
        }
        names.push(arg);
    }
    for path in args {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_skip_names(names.clone()),
        );
        let started = Instant::now();
        let report = name_spelling::report(&document, &Default::default()).unwrap();
        println!(
            "{path}: {} flags in {:?}",
            report.flags.len(),
            started.elapsed()
        );
        for flag in report.flags {
            println!("  {} -> {} ({:?})", flag.form, flag.name, flag.reason);
        }
    }
}
