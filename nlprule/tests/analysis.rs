use lazy_static::lazy_static;
use nlprule::{analysis::AnalysisSentence, Tokenizer};

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
}

fn readings(sentences: &[AnalysisSentence], surface: &str) -> Vec<(String, String)> {
    let token = sentences
        .iter()
        .flat_map(|s| &s.tokens)
        .find(|t| t.text == surface)
        .unwrap();
    token
        .readings
        .iter()
        .map(|r| (r.lemma.clone(), r.pos.clone()))
        .collect()
}

#[test]
fn analysis_readings_follow_lt66_probes() {
    // Expected readings come from LanguageTool 6.6 --taggeronly, using English.
    // See fixtures/analysis/README.md and the checked-in CLI transcripts. These are independent expectations,
    // not a snapshot of our pipe() output. Sentence-end pseudo readings are not exported.
    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/multiword.txt"));
    for surface in ["O", "BRIEN"] {
        assert_eq!(
            readings(&sentences, surface),
            vec![("O'Brien".into(), "NNP".into())]
        );
    }
    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/dropped_g.txt"));
    assert_eq!(
        readings(&sentences, "goin'"),
        vec![("go".into(), "VBG".into())]
    );
    assert_eq!(
        readings(&sentences, "'m"),
        vec![("be".into(), "VBP".into())]
    );
    assert_eq!(
        sentences[0]
            .tokens
            .iter()
            .find(|t| t.text == "goin'")
            .unwrap()
            .chunks,
        ["I-VP"]
    );

    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/fiction_unicode.txt"));
    assert_eq!(
        readings(&sentences, "’s"),
        vec![("be".into(), "VBZ".into())]
    );
    assert_eq!(
        readings(&sentences, "waiting"),
        vec![("wait".into(), "VBG".into())]
    );
    assert_eq!(
        readings(&sentences, "again"),
        vec![("again".into(), "RB".into())]
    );

    let sentences = TOKENIZER.analyze(include_str!("fixtures/analysis/paragraphs.txt"));
    assert_eq!(
        sentences
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["She walked home.\n\n", "He stayed."]
    );
    assert_eq!(
        readings(&sentences, "walked"),
        vec![("walk".into(), "VBD".into())]
    );
    assert_eq!(
        readings(&sentences, "stayed"),
        vec![("stay".into(), "VBD".into())]
    );
}

#[test]
fn original_surfaces_partition_sentences_in_all_offset_units() {
    for text in [
        "  😀 “She’s goin’—again…”\u{2028}\n\nHe left.\t",
        "I’m e\u{301}… ‘fine,’ she said.\r\n\r\nThey waited!",
        "She used L’ORÉAL—twice.",
    ] {
        let utf16: Vec<_> = text.encode_utf16().collect();
        let chars: Vec<_> = text.chars().collect();
        for sentence in TOKENIZER.analyze(text) {
            assert_eq!(text.get(sentence.span.byte.clone()).unwrap(), sentence.text);
            assert_eq!(
                String::from_utf16(&utf16[sentence.span.utf16.clone()]).unwrap(),
                sentence.text
            );
            let mut cursor = sentence.span.byte.start;
            for token in &sentence.tokens {
                assert_eq!(token.span.byte.start, cursor);
                assert!(token.span.byte.end > cursor);
                assert_eq!(text.get(token.span.byte.clone()).unwrap(), token.text);
                assert_eq!(
                    chars[token.span.char.clone()].iter().collect::<String>(),
                    token.text
                );
                assert_eq!(
                    String::from_utf16(&utf16[token.span.utf16.clone()]).unwrap(),
                    token.text
                );
                if token.is_whitespace {
                    assert!(token.readings.is_empty());
                }
                cursor = token.span.byte.end;
            }
            assert_eq!(cursor, sentence.span.byte.end);
            assert_eq!(
                sentence
                    .tokens
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<String>(),
                sentence.text
            );
        }
    }
}

#[test]
fn punctuation_and_whitespace_are_explicit() {
    let sentences = TOKENIZER.analyze("😀 “I’m fine—really…”\u{2028}");
    let tokens: Vec<_> = sentences.iter().flat_map(|s| &s.tokens).collect();
    for surface in ["“", "—", "…", "”"] {
        assert!(
            tokens
                .iter()
                .find(|t| t.text == surface)
                .unwrap()
                .is_punctuation
        );
    }
    let emoji = tokens.iter().find(|t| t.text == "😀").unwrap();
    assert!(!emoji.is_punctuation && !emoji.is_whitespace);
    let clitic = tokens.iter().find(|t| t.text == "’m").unwrap();
    assert!(!clitic.is_punctuation);
    assert!(tokens
        .iter()
        .any(|t| t.is_whitespace && t.text.contains('\u{2028}')));
}

#[test]
fn empty_analysis_and_owned_results() {
    for text in ["", " \t\n\u{2028}"] {
        assert!(TOKENIZER.analyze(text).is_empty());
    }
    let sentences = {
        let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
        let text = String::from("She walked home.");
        tokenizer.analyze(&text)
    };
    assert_eq!(sentences[0].text, "She walked home.");
}
