use writemorebetter_style::{analyze_document, DocumentOptions};
fn main() {
    let tokenizer = nlprule::Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    for text in std::env::args().skip(1) {
        let doc = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        println!(
            "== {text:?} dialogue={:?} speech={:?}",
            doc.dialogue
                .iter()
                .map(|s| &text[s.byte.clone()])
                .collect::<Vec<_>>(),
            doc.speech
        );
        for (p, para) in doc.paragraphs.iter().enumerate() {
            for s in &para.sentences {
                println!("-- p{p} sentence {:?}", s.text);
                for t in s.tokens.iter().filter(|t| !t.is_whitespace) {
                    let r: Vec<String> = t
                        .readings
                        .iter()
                        .map(|r| format!("{}/{}", r.lemma, r.pos))
                        .collect();
                    println!("   {:12} {:?} {:?}", t.text, t.chunks, r);
                }
            }
        }
    }
}
