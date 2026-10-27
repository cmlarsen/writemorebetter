use nlprule::Tokenizer;
use writemorebetter_style::reports::drafting::{self, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").expect("English tokenizer");
    let text = "Um, the door is TK.";
    let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
    // Dictated text gets edits; typed text (the default) is flag only.
    for (name, options) in [
        ("dictated", DraftingOptions::dictated()),
        ("default", DraftingOptions::new()),
    ] {
        let report = drafting::report(&document, &options).expect("drafting report");
        let edits: Vec<_> = report
            .matches
            .iter()
            .flat_map(|found| &found.edits)
            .collect();
        let terms: Vec<_> = report
            .matches
            .iter()
            .map(|found| found.term.as_str())
            .collect();
        println!(
            "{name}: {} (flags: {}, edits: {})",
            drafting::apply(text, &edits),
            terms.join(", "),
            edits.len()
        );
    }
}
