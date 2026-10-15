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
        &DocumentOptions::new().with_skip_names(vec!["mara".into()]),
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
        &DocumentOptions::new().with_skip_names(vec!["Port Royal".into()]),
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

#[test]
fn review_dialogue_recovery() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/review-dialogue.json")).unwrap();
    for row in rows {
        let text = row["text"].as_str().unwrap();
        let expected = row["dialogue"].as_array().unwrap();
        let spans = dialogue_spans(text);
        assert_eq!(spans.len(), expected.len(), "{text}");
        for (span, expected) in spans.iter().zip(expected) {
            assert_span(text, span, expected.as_str().unwrap());
        }
    }
}

#[test]
fn names_preserve_case_and_skip_adjacent_possessives() {
    let tokenizer = tokenizer();
    let document = analyze_document(
        &tokenizer,
        "Will said he will go. WILL stayed.",
        &DocumentOptions::new().with_skip_names(vec!["Will".into()]),
    );
    let words: Vec<_> = document
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .flat_map(|s| &s.tokens)
        .filter(|t| t.is_word)
        .collect();
    assert!(words
        .iter()
        .filter(|t| matches!(t.text.as_str(), "Will" | "WILL"))
        .all(|t| t.is_skipped));
    assert!(words.iter().any(|t| t.text == "will" && !t.is_skipped));
    assert!(document.lemma_index.contains_key("will"));
    for (text, name) in [
        ("Anna's hat.", "Anna"),
        ("Anna’s hat.", "Anna"),
        ("Charles' hat.", "Charles"),
        ("Zoë's hat.", "zoë's"),
    ] {
        let document = analyze_document(
            &tokenizer,
            text,
            &DocumentOptions::new().with_skip_names(vec![name.into()]),
        );
        assert_eq!(document.word_count, 1, "{text}");
        assert!(!document.lemma_index.contains_key("'s"), "{text}");
        assert!(
            document.paragraphs[0].sentences[0]
                .tokens
                .iter()
                .filter(|t| !t.is_whitespace && t.text != "hat" && t.text != ".")
                .all(|t| t.is_skipped),
            "{text}"
        );
    }
    let detached = analyze_document(
        &tokenizer,
        "Anna 's hat.",
        &DocumentOptions::new().with_skip_names(vec!["Anna".into()]),
    );
    assert_eq!(detached.word_count, 2);
    assert!(detached.paragraphs[0].sentences[0]
        .tokens
        .iter()
        .filter(|token| token.is_whitespace || token.text == "'s")
        .all(|token| !token.is_skipped));
}

#[test]
fn unknown_words_and_compact_documents_retain_counts() {
    let tokenizer = tokenizer();
    let text = "Zzqvorth sang. Zzqvorth sang. as soon as possible";
    let full = analyze_document(&tokenizer, text, &DocumentOptions::new());
    assert_eq!(full.lemma_index["zzqvorth"].len(), 2);
    let compact = analyze_document(
        &tokenizer,
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert_eq!(full.word_count, compact.word_count);
    assert_eq!(full.lemma_index, compact.lemma_index);
    assert!(compact
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .flat_map(|s| &s.tokens)
        .all(|t| t.readings.is_empty() && t.chunks.is_empty()));
    assert!(serde_json::from_str::<DocumentOptions>("{\"skpi_names\":[]}").is_err());
    assert_eq!(
        serde_json::from_str::<DocumentOptions>("{\"skipNames\":[\"Will\"]}")
            .unwrap()
            .skip_names,
        ["Will"]
    );
}

#[test]
fn hard_wrapped_paragraphs_preserve_offsets() {
    let text = "😀 “First wrapped\r\nline.”\r\n\r\nSecond paragraph.";
    let options = DocumentOptions::new().with_hard_wrapped(true);
    let document = analyze_document(&tokenizer(), text, &options);
    assert_eq!(document.paragraphs.len(), 2);
    assert_span(
        text,
        &document.paragraphs[0].span,
        "😀 “First wrapped\r\nline.”",
    );
    assert_span(text, &document.dialogue[0], "“First wrapped\r\nline.”");
    assert!(dialogue_spans(text).is_empty());
}
