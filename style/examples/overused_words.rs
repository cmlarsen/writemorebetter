use nlprule::Tokenizer;
use writemorebetter_style::reports::overused_words::{
    report, OverusedWordsBaseline, OverusedWordsOptions,
};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn main() {
    let tokenizer = Tokenizer::new("storage/en_tokenizer.bin").unwrap();
    let options = std::env::var("WMB_OVERUSED_OPTIONS")
        .map(|json| serde_json::from_str::<OverusedWordsOptions>(&json).unwrap())
        .unwrap_or_default();
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap();
        let document = analyze_document(
            &tokenizer,
            &text,
            &DocumentOptions::new().with_hard_wrapped(path.contains("moby")),
        );
        let result = report(&document, &options, OverusedWordsBaseline::bundled()).unwrap();
        let context = |span: &writemorebetter_style::Span| {
            let start = text.floor_char_boundary(span.byte.start.saturating_sub(40));
            let end = text.ceil_char_boundary((span.byte.end + 40).min(text.len()));
            text[start..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        for word in &result.overused {
            println!(
                "{}",
                serde_json::json!({"file": path, "words": result.words, "kind": "overused",
                    "lemma": word.lemma, "count": word.count, "ratio": word.ratio,
                    "perTenThousandWords": word.per_ten_thousand_words,
                    "referencePerTenThousandWords": word.reference_per_ten_thousand_words,
                    "example": context(&word.spans[0])})
            );
        }
        for filler in &result.fillers {
            println!(
                "{}",
                serde_json::json!({"file": path, "words": result.words, "kind": "filler",
                    "lemma": filler.word, "count": filler.count, "ratio": filler.ratio,
                    "perTenThousandWords": filler.per_ten_thousand_words,
                    "referencePerTenThousandWords": filler.reference_per_ten_thousand_words,
                    "example": context(&filler.spans[0])})
            );
        }
    }
}
