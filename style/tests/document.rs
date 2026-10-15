use nlprule::Tokenizer;
use std::sync::OnceLock;
use writemorebetter_style::{analyze_document, Document, TokenPosition};

fn document(paragraphs: &[&str]) -> Document {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    analyze_document(
        TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap()),
        paragraphs,
    )
}

fn quotes(document: &Document) -> Vec<&str> {
    document
        .dialogue_spans
        .iter()
        .map(|span| &document.text[span.byte.clone()])
        .collect()
}

#[test]
fn paragraph_boundaries_and_all_offset_units_are_preserved() {
    let paragraphs = [
        "😀 Heading",
        "",
        "  “She’s waiting—again…”\u{2028}",
        "He left.\nShe stayed.",
    ];
    let doc = document(&paragraphs);
    assert_eq!(doc.text, paragraphs.join("\n"));
    assert_eq!(doc.paragraphs.len(), 4);
    assert!(doc.paragraphs[1].sentences.is_empty());
    assert_eq!(doc.paragraphs[0].sentences.len(), 1);
    let utf16: Vec<_> = doc.text.encode_utf16().collect();
    let chars: Vec<_> = doc.text.chars().collect();
    for (index, paragraph) in doc.paragraphs.iter().enumerate() {
        assert_eq!(&doc.text[paragraph.span.byte.clone()], paragraphs[index]);
        assert_eq!(
            chars[paragraph.span.char.clone()]
                .iter()
                .collect::<String>(),
            paragraphs[index]
        );
        assert_eq!(
            String::from_utf16(&utf16[paragraph.span.utf16.clone()]).unwrap(),
            paragraphs[index]
        );
        for sentence in &paragraph.sentences {
            assert!(sentence.span.byte.start >= paragraph.span.byte.start);
            assert!(sentence.span.byte.end <= paragraph.span.byte.end);
            assert_eq!(&doc.text[sentence.span.byte.clone()], sentence.text);
            for token in &sentence.tokens {
                assert_eq!(&doc.text[token.span.byte.clone()], token.text);
                assert_eq!(
                    chars[token.span.char.clone()].iter().collect::<String>(),
                    token.text
                );
                assert_eq!(
                    String::from_utf16(&utf16[token.span.utf16.clone()]).unwrap(),
                    token.text
                );
            }
        }
    }
    for span in &doc.dialogue_spans {
        assert_eq!(
            String::from_utf16(&utf16[span.utf16.clone()]).unwrap(),
            &doc.text[span.byte.clone()]
        );
        assert_eq!(
            chars[span.char.clone()].iter().collect::<String>(),
            &doc.text[span.byte.clone()]
        );
    }
}

#[test]
fn nested_curly_and_straight_quotes_form_disjoint_outer_spans() {
    let doc = document(&["Before. “She said, ‘Wait—don’t go…’” Then: \"He said, 'Yes.'\" After."]);
    assert_eq!(
        quotes(&doc),
        ["“She said, ‘Wait—don’t go…’”", "\"He said, 'Yes.'\""]
    );
    for sentence in &doc.paragraphs[0].sentences {
        for token in &sentence.tokens {
            if token.text == "Before" || token.text == "After" {
                assert!(!doc.is_dialogue(&token.span));
            }
            if token.text == "Wait" || token.text == "Yes" {
                assert!(doc.is_dialogue(&token.span));
            }
        }
    }
}

#[test]
fn repeated_openers_continue_across_paragraphs() {
    for paragraphs in [
        ["“First paragraph.", "  “Second paragraph.” Narration."],
        ["\"First paragraph.", "  \"Second paragraph.\" Narration."],
        ["‘First paragraph.", "  ‘Second paragraph.’ Narration."],
    ] {
        let doc = document(&paragraphs);
        let expected = doc.text.split(" Narration.").next().unwrap();
        assert_eq!(quotes(&doc), [expected]);
    }
    let doc = document(&["“Still speaking.", "“And still speaking…"]);
    assert_eq!(quotes(&doc), [doc.text.as_str()]);
}

#[test]
fn apostrophes_and_possessives_do_not_create_dialogue() {
    let doc = document(&["It’s five o'clock. The dogs’ bowls and cats' beds were empty. She's goin' home—again…\u{2028}"]);
    assert!(doc.dialogue_spans.is_empty());
    let doc = document(&["‘I’m goin’ home,’ she said. “The dogs’ bowls are empty.”"]);
    assert_eq!(
        quotes(&doc),
        ["‘I’m goin’ home,’", "“The dogs’ bowls are empty.”"]
    );
}

#[test]
fn lemma_index_deduplicates_ambiguous_readings_and_resets_after_edits() {
    let doc = document(&["She walked home.", "They walk home."]);
    let positions = &doc.lemma_index["walk"];
    assert_eq!(positions.len(), 2);
    assert!(positions[0] < positions[1]);
    assert_eq!(doc.token(positions[0]).unwrap().text, "walked");
    assert_eq!(doc.token(positions[1]).unwrap().text, "walk");
    assert!(!doc.lemma_index.contains_key("."));
    let doc = document(&["They stayed."]);
    assert!(!doc.lemma_index.contains_key("walk"));
    assert!(doc
        .token(TokenPosition {
            paragraph: 99,
            sentence: 0,
            token: 0
        })
        .is_none());
}

#[test]
fn empty_and_unmatched_quotes_have_documented_behavior() {
    assert!(document(&[]).paragraphs.is_empty());
    let doc = document(&["", ""]);
    assert_eq!(doc.text, "\n");
    assert_eq!(doc.paragraphs.len(), 2);
    assert!(doc.lemma_index.is_empty());
    assert!(doc.dialogue_spans.is_empty());
    let doc = document(&["An unmatched closer ” is ignored. “An opener extends to EOF…"]);
    assert_eq!(quotes(&doc), ["“An opener extends to EOF…"]);
    let mut empty = doc.dialogue_spans[0].clone();
    empty.byte.end = empty.byte.start;
    assert!(!doc.is_dialogue(&empty));
}
