use nlprule::Tokenizer;
use writemorebetter_style::{
    analyze_document, dialogue_spans, is_word, paragraph_spans, DocumentOptions, Span,
};

fn tokenizer() -> Tokenizer {
    Tokenizer::new("../storage/en_tokenizer.bin").unwrap()
}

fn assert_span(text: &str, span: &Span, expected: &str) {
    assert_eq!(text.get(span.byte.clone()), Some(expected));
    assert_eq!(
        text.chars()
            .skip(span.char.start)
            .take(span.char.len())
            .collect::<String>(),
        expected
    );
    assert_eq!(
        String::from_utf16(
            &text
                .encode_utf16()
                .skip(span.utf16.start)
                .take(span.utf16.len())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        expected
    );
}

#[test]
fn annotated_document() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/document.json")).unwrap();
    let text = fixture["text"].as_str().unwrap();
    let document = analyze_document(&tokenizer(), text, &DocumentOptions::default());
    assert_eq!(
        document.paragraphs.len(),
        fixture["paragraphs"].as_array().unwrap().len()
    );
    for (p, paragraph) in document.paragraphs.iter().enumerate() {
        assert_span(
            text,
            &paragraph.span,
            fixture["paragraphs"][p].as_str().unwrap(),
        );
        let sentences = fixture["sentences"][p].as_array().unwrap();
        assert_eq!(paragraph.sentences.len(), sentences.len());
        for (sentence, expected) in paragraph.sentences.iter().zip(sentences) {
            assert_span(text, &sentence.span, expected.as_str().unwrap());
            for token in &sentence.tokens {
                assert_span(text, &token.span, &token.text);
            }
        }
    }
    assert_eq!(document.dialogue.len(), 1);
    assert_span(
        text,
        &document.dialogue[0],
        fixture["dialogue"][0].as_str().unwrap(),
    );
}

#[test]
fn paragraph_boundaries_are_independent_of_sentences() {
    let text = "\r\n  Heading\r\n\r\nFragment\n   \n😀 final\u{2029}End\n";
    let spans = paragraph_spans(text);
    assert_eq!(spans.len(), 4);
    for (span, expected) in spans
        .iter()
        .zip(["  Heading", "Fragment", "😀 final", "End"])
    {
        assert_span(text, span, expected);
    }
    let document = analyze_document(&tokenizer(), text, &DocumentOptions::default());
    assert_eq!(document.paragraphs.len(), 4);
    assert!(document.paragraphs.iter().all(|p| p.sentences.len() == 1));
    for text in ["", " \n\r\n\u{2028}"] {
        let document = analyze_document(&tokenizer(), text, &DocumentOptions::default());
        assert!(document.paragraphs.is_empty());
        assert_eq!(document.word_count, 0);
    }
}

#[test]
fn dialogue_delimiters_and_apostrophes() {
    for (text, expected) in [
        (
            "She said, \"I heard 'come home'—twice…\"",
            vec!["\"I heard 'come home'—twice…\""],
        ),
        (
            "‘Stay,’ she said. 'Go!' he cried.",
            vec!["‘Stay,’", "'Go!'"],
        ),
        (
            "At o'clock, goin' past the dogs' bowls wasn’t wise.",
            vec![],
        ),
        (
            "\"First paragraph\n\n\"Second paragraph.\"",
            vec!["\"First paragraph\n\n\"Second paragraph.\""],
        ),
        (
            "😀 “It’s late—again…\u{2028}Go.”",
            vec!["“It’s late—again…\u{2028}Go.”"],
        ),
        ("“unfinished", vec![]),
        (
            "'I’m goin' past the dogs' bowls.'",
            vec!["'I’m goin' past the dogs' bowls.'"],
        ),
        ("'unfinished word'", vec![]),
    ] {
        let spans = dialogue_spans(text);
        assert_eq!(spans.len(), expected.len(), "{text}");
        for (span, expected) in spans.iter().zip(expected) {
            assert_span(text, span, expected);
        }
    }
}

#[test]
fn counts_words_and_distinct_token_lemmas() {
    let tokenizer = tokenizer();
    let text = "Mara walks. Mara walked. 😀 $ % 123";
    let all = analyze_document(&tokenizer, text, &DocumentOptions::default());
    let skipped = analyze_document(
        &tokenizer,
        text,
        &DocumentOptions {
            skip_names: vec!["mara".into()],
        },
    );
    assert_eq!(all.word_count - skipped.word_count, 2);
    assert_eq!(skipped.word_count, 3);
    assert_eq!(skipped.lemma_index["walk"].len(), 2);
    for positions in skipped.lemma_index.values() {
        let mut seen = std::collections::BTreeSet::new();
        for position in positions {
            assert!(seen.insert((position.paragraph, position.sentence, position.token)));
            let token = &skipped.paragraphs[position.paragraph].sentences[position.sentence].tokens
                [position.token];
            assert!(token.is_word && !token.is_skipped);
        }
    }
    for text in ["😀", "‍", "$", "%", "—", "…", "\u{2028}"] {
        assert!(!is_word(text));
    }
    for text in ["o’clock", "goin’", "123", "e\u{301}"] {
        assert!(is_word(text));
    }
}

#[test]
fn names_can_span_multiple_tokens() {
    let tokenizer = tokenizer();
    let text = "Port Royal waits. Port stays. PORT\tROYAL sleeps.";
    let all = analyze_document(&tokenizer, text, &DocumentOptions::default());
    let filtered = analyze_document(
        &tokenizer,
        text,
        &DocumentOptions {
            skip_names: vec!["Port Royal".into()],
        },
    );
    assert_eq!(all.word_count - filtered.word_count, 4);
    assert_eq!(filtered.word_count, 4);
    let port = filtered
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .flat_map(|s| &s.tokens)
        .filter(|t| t.text == "Port")
        .collect::<Vec<_>>();
    assert_eq!(port.len(), 2);
    assert!(port[0].is_skipped);
    assert!(!port[1].is_skipped);
}
