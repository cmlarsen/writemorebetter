use writemorebetter_style::{analyze_document, DocumentOptions};
fn main() {
    let tokenizer = nlprule::Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    for text in std::env::args().skip(1) {
        let text = text.replace("\\n", "\n").replace("<LS>", "\u{2028}");
        let doc = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        let v: Vec<String> = doc
            .voices
            .iter()
            .map(|v| {
                format!(
                    "{:?}/{:?} {:?} tag={:?} shift={:?}",
                    v.voice,
                    v.source,
                    &text[v.span.byte.clone()],
                    v.tag.as_ref().map(|t| &text[t.byte.clone()]),
                    v.shift
                )
            })
            .collect();
        println!(
            "{text:?}\n    {}",
            if v.is_empty() {
                "NONE".to_string()
            } else {
                v.join("\n    ")
            }
        );
    }
}
