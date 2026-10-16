use nlprule::Tokenizer;
use std::io::{self, BufRead};
use writemorebetter_style::reports::{pronoun_starts, transitions};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    for line in io::stdin().lock().lines() {
        let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let text = row["text"].as_str().unwrap();
        let document = analyze_document(
            &tokenizer,
            text,
            &DocumentOptions::new()
                .with_hard_wrapped(row["hardWrapped"].as_bool().unwrap_or(false)),
        );
        println!(
            "{}",
            serde_json::json!({
                "transitions": transitions::report(&document, &Default::default()).unwrap(),
                "pronounStarts": pronoun_starts::report(&document, &Default::default()).unwrap()
            })
        );
    }
}
