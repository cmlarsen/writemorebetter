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

#[test]
fn spoken_command_table() {
    for (source, expected, term) in [
        ("She said comma he left", "She said, he left", "comma"),
        ("She said coma he left", "She said, he left", "coma"),
        (
            "She left period she returned",
            "She left. She returned",
            "period",
        ),
        (
            "She left full stop she returned",
            "She left. She returned",
            "full stop",
        ),
        (
            "She left question mark she returned",
            "She left? She returned",
            "question mark",
        ),
        (
            "She left exclamation point she returned",
            "She left! She returned",
            "exclamation point",
        ),
        (
            "She left exclamation mark she returned",
            "She left! She returned",
            "exclamation mark",
        ),
        ("She said colon He left", "She said: He left", "colon"),
        (
            "She said semicolon He left",
            "She said; He left",
            "semicolon",
        ),
        ("She stopped dash he ran", "She stopped—he ran", "dash"),
        (
            "She stopped em dash he ran",
            "She stopped—he ran",
            "em dash",
        ),
        ("She stopped m dash he ran", "She stopped—he ran", "m dash"),
        ("She said hyphen He", "She said- He", "hyphen"),
        ("She said ellipsis he left", "She said… he left", "ellipsis"),
        (
            "She said dot dot dot he left",
            "She said… he left",
            "dot dot dot",
        ),
        ("She said open quote hello", "She said “hello", "open quote"),
        (
            "She said begin quote hello",
            "She said “hello",
            "begin quote",
        ),
        (
            "hello close quote she said",
            "hello” she said",
            "close quote",
        ),
        ("hello end quote she said", "hello” she said", "end quote"),
        ("hello unquote she said", "hello” she said", "unquote"),
        (
            "She said open single quote hello",
            "She said ‘hello",
            "open single quote",
        ),
        (
            "hello close single quote she said",
            "hello’ she said",
            "close single quote",
        ),
        ("She said open paren hello", "She said (hello", "open paren"),
        (
            "She said open parenthesis hello",
            "She said (hello",
            "open parenthesis",
        ),
        (
            "hello close paren she said",
            "hello) she said",
            "close paren",
        ),
        (
            "hello close parenthesis she said",
            "hello) she said",
            "close parenthesis",
        ),
        (
            "She said open bracket hello",
            "She said [hello",
            "open bracket",
        ),
        (
            "hello close bracket she said",
            "hello] she said",
            "close bracket",
        ),
        ("She typed apostrophe", "She typed’", "apostrophe"),
        (
            "She left new paragraph He returned",
            "She left\nHe returned",
            "new paragraph",
        ),
        (
            "She left new line He returned",
            "She left\nHe returned",
            "new line",
        ),
        (
            "She left next line He returned",
            "She left\nHe returned",
            "next line",
        ),
        (
            "She spoke scratch that then waited",
            "She spoke then waited",
            "scratch that",
        ),
        (
            "She spoke delete that then waited",
            "She spoke then waited",
            "delete that",
        ),
        (
            "She spoke strike that then waited",
            "She spoke then waited",
            "strike that",
        ),
        (
            "She spoke undo that then waited",
            "She spoke then waited",
            "undo that",
        ),
        (
            "She said caps on then spoke",
            "She said caps on then spoke",
            "caps on",
        ),
        (
            "She said caps off then spoke",
            "She said caps off then spoke",
            "caps off",
        ),
        (
            "She said all caps then spoke",
            "She said all caps then spoke",
            "all caps",
        ),
        (
            "She said no caps then spoke",
            "She said no caps then spoke",
            "no caps",
        ),
    ] {
        let result = report(source, DraftingOptions::new());
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::SpokenCommand && m.term == term),
            "{source:?}: {:?}",
            result.matches
        );
        assert_eq!(
            applied(source, DraftingOptions::new()),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn spoken_command_guards() {
    for text in [
        "a comma",
        "the Edo period",
        "a dash of salt",
        "a new line of work",
        "a period of time",
        "made a dash",
        "I have to dash",
        "Period.",
        "Full stop.",
        "quote, unquote",
        "quote-unquote",
        "She typed “period” and waited.",
        "At that period, she left the room.",
        "She wrote a sentence. The period was missing.",
        "She marked the page period and folded it.",
    ] {
        assert!(
            !report(text, DraftingOptions::new())
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::SpokenCommand),
            "{text:?}"
        );
    }
}

#[test]
fn commands_marks_speech_and_overlap() {
    assert_eq!(
        applied("She said comma, he left", DraftingOptions::new()),
        "She said, he left"
    );
    let dictated = "open quote I’m fine um close quote she said";
    let default = report(dictated, DraftingOptions::new());
    assert_eq!(
        default
            .matches
            .iter()
            .filter(|m| m.category == DraftingCategory::SpokenCommand)
            .count(),
        2
    );
    assert!(!default
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    let mut options = DraftingOptions::new();
    options.include_dialogue = true;
    let included = report(dictated, options);
    assert!(included
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause
            && m.mode == writemorebetter_style::Mode::Dialogue));
    let text = "um comma";
    let result = report(text, DraftingOptions::new());
    let edits: Vec<_> = result.matches.iter().flat_map(|m| &m.edits).collect();
    for pair in edits.windows(2) {
        assert!(pair[0].span.byte.end <= pair[1].span.byte.start);
    }
    assert_eq!(drafting::apply(text, &edits), ",");
}

#[test]
fn command_mark_majority_and_tie() {
    for (source, term, replacement) in [
        ("She said open quote hello", "open quote", "“"),
        (
            "“One,” she said. “Two,” he said. She said open quote hello",
            "open quote",
            "“",
        ),
        (
            "\"One,\" she said. \"Two,\" he said. She said open quote hello",
            "open quote",
            "\"",
        ),
        (
            "“One” and \"Two\". She said open quote hello",
            "open quote",
            "“",
        ),
        ("She said open single quote hello", "open single quote", "‘"),
        (
            "‘One,’ she said. ‘Two,’ he said. She said open single quote hello",
            "open single quote",
            "‘",
        ),
        (
            "'One,' she said. 'Two,' he said. She said open single quote hello",
            "open single quote",
            "'",
        ),
        ("She paused ellipsis he left", "ellipsis", "…"),
        (
            "She paused… then... She paused ellipsis he left",
            "ellipsis",
            "…",
        ),
        (
            "She paused… Then she paused... She paused... She paused ellipsis he left",
            "ellipsis",
            "...",
        ),
    ] {
        let result = report(source, DraftingOptions::new());
        let found = result
            .matches
            .iter()
            .find(|m| m.category == DraftingCategory::SpokenCommand && m.term == term)
            .unwrap_or_else(|| panic!("{source:?}"));
        assert_eq!(found.edits[0].replacement, replacement, "{source:?}");
    }
}

#[test]
fn dictated_quote_pair_and_options() {
    let paired = "open quote I’m fine um comma he left close quote she said";
    let result = report(paired, DraftingOptions::new());
    assert!(result.groups.dialogue.words > 0);
    assert!(!result
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    assert!(result
        .matches
        .iter()
        .any(|m| m.term == "comma" && m.mode == writemorebetter_style::Mode::Dialogue));
    let mut options = DraftingOptions::new();
    options.include_dialogue = true;
    assert!(report(paired, options)
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    let unpaired = "open quote I’m fine um";
    let unpaired_report = report(unpaired, DraftingOptions::new());
    assert!(unpaired_report
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    assert!(unpaired_report
        .matches
        .iter()
        .any(|m| m.term == "open quote" && !m.edits.is_empty()));
    let mut disabled = DraftingOptions::new();
    disabled.spoken_commands = false;
    assert!(!report(paired, disabled)
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::SpokenCommand));
}

#[test]
fn spoken_commands_keep_utf8_context() {
    for (source, expected) in [
        ("😀 She said comma he left", "😀 She said, he left"),
        (
            "“I’m here,” she said comma he left",
            "“I’m here,” she said, he left",
        ),
        (
            "\"I'm here,\" she said comma he left",
            "\"I'm here,\" she said, he left",
        ),
        (
            "She waited—then said comma he left",
            "She waited—then said, he left",
        ),
        (
            "She waited... then said comma he left",
            "She waited... then said, he left",
        ),
        (
            "She waited… then said comma he left",
            "She waited… then said, he left",
        ),
        (
            "She waited\u{2028}then said comma he left",
            "She waited\u{2028}then said, he left",
        ),
    ] {
        assert_eq!(
            applied(source, DraftingOptions::new()),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn review_cleanup_regressions() {
    let cases = [
        ("The cat um, uh, sat", "The cat sat"),
        ("He left. Um, uh, the door", "He left. The door"),
        ("um, uh, the door", "The door"),
        ("It was, um… fine", "It was… fine"),
        ("He was, um—thinking", "He was—thinking"),
        ("He said, um. Then", "He said. Then"),
        ("He was—um, thinking", "He was, thinking"),
        ("He paused—um.", "He paused."),
        ("He left. Well, um, I was", "He left. Well, I was"),
        ("I, um, think", "I think"),
        (
            "She said \"no\" um, then left.",
            "She said \"no\" then left.",
        ),
        ("He said um--no", "He said no"),
        ("Um--the door", "The door"),
        ("I was--um, thinking", "I was, thinking"),
        ("He said um--uh--no", "He said no"),
        ("I was - um - thinking", "I was thinking"),
        (
            "the light turned green period she drove on",
            "the light turned green. She drove on",
        ),
        (
            "are you coming question mark he asked",
            "are you coming? He asked",
        ),
        (
            "she listed the tools colon hammer saw and nails",
            "she listed the tools: hammer saw and nails",
        ),
        (
            "I reached for the handle dash and stopped",
            "I reached for the handle—and stopped",
        ),
        (
            "the road ran north hyphen south for miles comma then turned",
            "the road ran north-south for miles, then turned",
        ),
        (
            "I bought bread coma cheese and apples",
            "I bought bread, cheese and apples",
        ),
        (
            "she said open quote come in close quote and shut the door",
            "she said “come in” and shut the door",
        ),
        (
            "the door opened new paragraph Outside",
            "the door opened\nOutside",
        ),
        ("milk, comma, and", "milk, and"),
        ("left—then right, comma, and", "left—then right, and"),
        ("it apostrophe s mine", "it’s mine"),
    ];
    for (source, expected) in cases {
        assert_eq!(
            applied(source, DraftingOptions::new()),
            expected,
            "{source:?}"
        );
    }
    for source in [
        "“Um,” she said.",
        "\"Um,\" she said.",
        "Um.",
        "Put it on the next line of the form.",
        "The em dash is my favorite mark.",
        "Chapter XXX began",
        "A lone period in a well-punctuated paragraph.",
    ] {
        assert_eq!(
            applied(source, DraftingOptions::new()),
            source,
            "{source:?}"
        );
    }
    for source in ["[Name]", "[NAME]"] {
        assert!(report(source, DraftingOptions::new())
            .matches
            .iter()
            .any(|found| found.category == DraftingCategory::Placeholder));
    }
    let mut dialogue = DraftingOptions::new();
    dialogue.include_dialogue = true;
    for source in ["“Um,” she said.", "\"Um,\" she said.", "Um."] {
        assert_eq!(applied(source, dialogue.clone()), source);
        assert!(!report(source, dialogue.clone())
            .matches
            .iter()
            .any(|found| found.category == DraftingCategory::FilledPause));
    }
    assert_eq!(
        applied("“Well, um, I was going.”", dialogue),
        "“Well, I was going.”"
    );
    for source in [
        "The cat um, uh, sat",
        "He left. Um, uh, the door",
        "um, uh, the door",
    ] {
        let found = report(source, DraftingOptions::new());
        let edits: Vec<_> = found.matches.iter().flat_map(|item| &item.edits).collect();
        assert_eq!(edits.len(), 1, "{source:?}");
        assert_eq!(
            drafting::apply(source, &edits),
            applied(source, DraftingOptions::new())
        );
        assert_eq!(drafting::apply(source, &[]), source);
    }
}

#[test]
fn published_prose_command_guards() {
    for source in [
        "For a short period his patience held.",
        "Dash it, we missed the train!",
        "Dash the paint from the door.",
        "The birds dash themselves against the glass.",
        "But dash my luck!",
        "Shall I strike that?",
        "I saw you strike that wooden post.",
        "You cannot undo that.",
        "There are no caps at sea.",
    ] {
        assert_eq!(
            applied(source, DraftingOptions::new()),
            source,
            "{source:?}"
        );
    }
}
