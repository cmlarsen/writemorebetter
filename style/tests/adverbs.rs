use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::{
    adverbs::{self, AdverbsOptions},
    to_utf16_json, StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

#[test]
fn manner_adverbs_split_and_mark_tags() {
    let text = "She walked slowly—quietly… “Go,” he said softly.\u{2028}“I’m ready,” she whispered gently.";
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let report = adverbs::report(&document, &AdverbsOptions::new()).unwrap();
    let lemmas: Vec<_> = report
        .matches
        .iter()
        .map(|item| item.lemma.as_str())
        .collect();
    assert!(lemmas.contains(&"slowly"));
    assert!(lemmas.contains(&"quietly"));
    assert!(lemmas.contains(&"softly"));
    assert!(
        report
            .matches
            .iter()
            .find(|item| item.lemma == "softly")
            .unwrap()
            .tag_adverb
    );
    assert_eq!(
        report.groups.all.count,
        report.groups.narration.count + report.groups.dialogue.count
    );
    assert_eq!(report.groups.all.words, document.word_count);
    for item in &report.matches {
        assert!(text.get(item.span.byte.clone()).is_some());
    }
    let utf16: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    assert!(utf16["matches"][0]["span"]["start"].is_number());
    let quiet = report
        .matches
        .iter()
        .find(|item| item.lemma == "quietly")
        .unwrap();
    assert!(quiet.span.utf16.start < quiet.span.byte.start);
    let serialized = utf16["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["lemma"] == "quietly")
        .unwrap();
    assert_eq!(
        serialized["span"]["start"].as_u64(),
        Some(quiet.span.utf16.start as u64)
    );
}

#[test]
fn exclusions_and_input_contract() {
    let text = "“I can’t,” she said softly. He apparently agreed, but walked slowly.";
    let shared = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let report = shared.adverbs(None).unwrap();
    assert!(!report.matches.iter().any(|item| item.lemma == "apparently"));
    assert!(shared.adverbs(Some("{\"unknown\":true}")).is_err());
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        adverbs::report(&compact, &Default::default()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn ambiguous_adjectives_and_frequency_words_are_excluded() {
    let text = "It was a lovely, friendly, only, holy day. Daily, weekly, she sat quietly—then spoke softly…\u{2028}“Go,” she said gently.";
    let report = adverbs::report(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        &Default::default(),
    )
    .unwrap();
    let lemmas: Vec<_> = report
        .matches
        .iter()
        .map(|item| item.lemma.as_str())
        .collect();
    assert!(lemmas.contains(&"quietly"));
    assert!(lemmas.contains(&"softly"));
    assert!(!["friendly", "holy", "daily", "weekly", "only", "lovely"]
        .iter()
        .any(|word| lemmas.contains(word)));
}
