use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::drafting::{self, DraftingCategory, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}
fn report(text: &str, options: DraftingOptions) -> drafting::DraftingReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    drafting::report(&document, &options).unwrap()
}
fn applied(text: &str, options: DraftingOptions) -> String {
    let report = report(text, options);
    drafting::apply(
        text,
        &report
            .matches
            .iter()
            .flat_map(|m| &m.edits)
            .collect::<Vec<_>>(),
    )
}
#[test]
fn deletion_worked_cases() {
    for (source, expected, dialogue) in [
        ("So um I was", "So I was", false),
        ("Well, um, I was", "Well, I was", false),
        ("I was, uh, thinking", "I was thinking", false),
        ("I was—um—thinking", "I was thinking", false),
        ("I was… um… thinking", "I was… thinking", false),
        ("Um, the door", "The door", false),
        ("Um… the door", "The door", false),
        ("He left um.", "He left.", false),
        ("He left um”", "He left”", true),
        ("“I was, um,” she said", "“I was,” she said", true),
        ("“Um, I don’t know.”", "“I don’t know.”", true),
        ("the um\u{2028}next", "the\u{2028}next", false),
        ("He waited um\nThen", "He waited\nThen", false),
    ] {
        let mut options = DraftingOptions::new();
        options.include_dialogue = dialogue;
        for quote in [false, true] {
            for dash in [false, true] {
                for ellipsis in [false, true] {
                    for apostrophe in [false, true] {
                        for separator in [false, true] {
                            let variant = |value: &str| {
                                let mut value = value.to_owned();
                                if quote {
                                    value = value.replace('“', "\"").replace('”', "\"");
                                }
                                if dash {
                                    value = value.replace('—', "--");
                                }
                                if ellipsis {
                                    value = value.replace('…', "...");
                                }
                                if apostrophe {
                                    value = value.replace('’', "'");
                                }
                                if separator {
                                    value.push('\u{2028}');
                                }
                                value
                            };
                            let source = variant(source);
                            let expected = variant(expected);
                            assert_eq!(applied(&source, options.clone()), expected, "{source:?}");
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn guards_and_options() {
    for text in [
        "the word ‘um’",
        "“Um.”",
        "uh-huh",
        "uh-oh",
        "um-hm",
        "UM, the door",
    ] {
        assert!(
            !report(text, DraftingOptions::new())
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::FilledPause),
            "{text}"
        );
    }
    for text in [
        "Chapter XX",
        "Book XX",
        "atkins",
        "Tkachenko",
        "[sic]",
        "What???",
    ] {
        assert!(
            !report(text, DraftingOptions::new())
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::Placeholder),
            "{text}"
        );
    }
    assert!(
        report("TK and [name] and (check this)", DraftingOptions::new())
            .matches
            .len()
            >= 3
    );
    assert!(report("“Um, I went.”", DraftingOptions::new())
        .matches
        .is_empty());
    let mut options = DraftingOptions::new();
    options.include_dialogue = true;
    assert!(report("“Um.”", options.clone()).matches.is_empty());
    assert_eq!(applied("“Um, I went.”", options.clone()), "“I went.”");
    assert!(!report("He was, er, tired", DraftingOptions::new())
        .matches
        .iter()
        .any(|m| m.term == "er"));
    options.er = true;
    assert!(report("He was er tired", options.clone())
        .matches
        .is_empty());
    assert_eq!(
        applied("He was, er, tired", options.clone()),
        "He was tired"
    );
    assert_eq!(applied("He was—er—tired", options.clone()), "He was tired");
    assert_eq!(applied("He was--er--tired", options), "He was tired");
}
#[test]
fn adjacent_and_capitalization() {
    let text = "um, uh, I";
    let result = report(text, DraftingOptions::new());
    let edits: Vec<_> = result.matches.iter().flat_map(|m| &m.edits).collect();
    for pair in edits.windows(2) {
        assert!(pair[0].span.byte.end <= pair[1].span.byte.start);
    }
    assert_eq!(drafting::apply(text, &edits), "I");
    assert_eq!(applied("Um, ßeta", DraftingOptions::new()), "ßeta");
}
#[test]
fn utf16_spans() {
    let text = "😀 um, the";
    let result = report(text, DraftingOptions::new());
    let json: serde_json::Value =
        serde_json::from_str(&writemorebetter_style::reports::to_utf16_json(&result)).unwrap();
    assert_eq!(
        json["matches"][0]["span"],
        serde_json::json!({"start": 3, "end": 5})
    );
    let mut invalid = result.matches[0].edits[0].clone();
    invalid.span.byte.start = 1; // inside the emoji
    assert_eq!(drafting::apply(text, &[&invalid]), text);
}

#[test]
fn typography_variants() {
    let mut options = DraftingOptions::new();
    options.include_dialogue = true;
    for (source, expected) in [
        ("\"I was, um,\" she said", "\"I was,\" she said"),
        ("“I was, um,” she said", "“I was,” she said"),
        ("“Um, I don't know.”", "“I don't know.”"),
        ("“Um, I don’t know.”", "“I don’t know.”"),
        ("I was--um--thinking", "I was thinking"),
        ("I was—um—thinking", "I was thinking"),
        ("I was... um... thinking", "I was... thinking"),
        ("I was… um… thinking", "I was… thinking"),
        ("I was\u{2028}um, thinking", "I was\u{2028}thinking"),
    ] {
        assert_eq!(applied(source, options.clone()), expected, "{source:?}");
    }
}
