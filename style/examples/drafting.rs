use nlprule::Tokenizer;
use writemorebetter_style::reports::drafting::{self, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").expect("English tokenizer");
    let text = "Um, the door is TK.";
    let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
    let report = drafting::report(&document, &DraftingOptions::new()).expect("drafting report");
    let edits: Vec<_> = report
        .matches
        .iter()
        .flat_map(|found| &found.edits)
        .collect();
    println!("{}", drafting::apply(text, &edits));
}
