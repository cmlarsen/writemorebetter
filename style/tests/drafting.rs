use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::drafting::{self, DraftingCategory, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode};

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
fn spoken() -> DraftingOptions {
    let mut options = DraftingOptions::new();
    options.spoken_commands = true;
    options
}
fn dialogue() -> DraftingOptions {
    let mut options = DraftingOptions::new();
    options.include_dialogue = true;
    options
}
/// Every typographic variant AGENTS.md asks for: straight and curly quotes and apostrophes, `—`
/// and `--`, `…` and `...`, and a trailing U+2028.
fn variants(source: &str, expected: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for mask in 0..32 {
        let variant = |value: &str| {
            let mut value = value.to_owned();
            if mask & 1 != 0 {
                value = value.replace(['“', '”'], "\"");
            }
            if mask & 2 != 0 {
                value = value.replace('—', "--");
            }
            if mask & 4 != 0 {
                value = value.replace('…', "...");
            }
            if mask & 8 != 0 {
                value = value.replace(['‘', '’'], "'");
            }
            if mask & 16 != 0 {
                value.push('\u{2028}');
            }
            value
        };
        out.push((variant(source), variant(expected)));
    }
    out
}
fn assert_edits(cases: &[(&str, &str)], options: DraftingOptions) {
    for (source, expected) in cases {
        for (source, expected) in variants(source, expected) {
            assert_eq!(applied(&source, options.clone()), expected, "{source:?}");
        }
    }
}
/// The pause is reported with no edit, so the text comes back unchanged.
fn assert_flag_only(cases: &[&str], options: DraftingOptions) {
    for source in cases {
        for (source, _) in variants(source, source) {
            let result = report(&source, options.clone());
            assert!(
                result
                    .matches
                    .iter()
                    .any(|m| m.category == DraftingCategory::FilledPause),
                "{source:?}: {:?}",
                result.matches
            );
            assert!(
                result.matches.iter().all(|m| m.edits.is_empty()),
                "{source:?}: {:?}",
                result.matches
            );
        }
    }
}
/// Asserts the edited text, and silence when the text should come back unchanged.
fn assert_cases(cases: &[(&str, &str)], options: DraftingOptions) {
    for (source, expected) in cases {
        assert_eq!(applied(source, options.clone()), *expected, "{source:?}");
        if source == expected {
            let result = report(source, options.clone());
            assert!(
                result.matches.is_empty(),
                "{source:?}: {:?}",
                result.matches
            );
        }
    }
}

#[test]
fn pattern_a_spaces_on_both_sides() {
    assert_edits(
        &[
            ("So um I was", "So I was"),
            ("He’d left um.", "He’d left."),
            (
                "“Wait,” she said—so um I didn’t",
                "“Wait,” she said—so I didn’t",
            ),
            ("It was late… so uh we left", "It was late… so we left"),
            ("we walked um uh to the river", "we walked to the river"),
            ("He left.\u{2028}So um I was", "He left.\u{2028}So I was"),
        ],
        DraftingOptions::new(),
    );
    assert_edits(
        &[("“He left um!” she said", "“He left!” she said")],
        dialogue(),
    );
    let result = report("So um I was", DraftingOptions::new());
    assert_eq!(result.matches[0].edits.len(), 1);
    assert_eq!(result.matches[0].edits[0].replacement, "");
}

#[test]
fn pattern_b_sentence_start() {
    assert_edits(
        &[
            ("Um, the door", "The door"),
            ("Um… the door", "The door"),
            ("Um the door", "The door"),
            ("He’d left. Um, the door—", "He’d left. The door—"),
            ("He left!” Um… the door", "He left!” The door"),
            ("Um, I don’t know", "I don’t know"),
            ("Um, ßeta", "ßeta"),
        ],
        DraftingOptions::new(),
    );
    assert_edits(
        &[
            ("“Um, I don’t know.”", "“I don’t know.”"),
            (
                "She stopped. “Um… we’re late,” she said.",
                "She stopped. “We’re late,” she said.",
            ),
        ],
        dialogue(),
    );
    let result = report("Um, the door", DraftingOptions::new());
    assert_eq!(result.matches[0].edits[0].replacement, "T");
}

#[test]
fn pattern_c_discourse_comma_pair() {
    assert_edits(
        &[
            ("Well, um, I was", "Well, I was"),
            (
                "He’d gone. Okay, uh, let’s go…",
                "He’d gone. Okay, let’s go…",
            ),
            ("Yes, um, I know—", "Yes, I know—"),
        ],
        DraftingOptions::new(),
    );
    assert_edits(
        &[("“Oh, um, it’s you,” she said.", "“Oh, it’s you,” she said.")],
        dialogue(),
    );
}

#[test]
fn pattern_d_comma_pair_elsewhere() {
    assert_edits(
        &[
            ("I was, uh, thinking", "I was thinking"),
            ("He, uh, left.", "He left."),
            ("It wasn’t, um, his fault—", "It wasn’t his fault—"),
            (
                "She waited… and, uh, then left",
                "She waited… and then left",
            ),
        ],
        DraftingOptions::new(),
    );
    assert_edits(
        &[(
            "“I was, um, thinking,” she said.",
            "“I was thinking,” she said.",
        )],
        dialogue(),
    );
    let mut options = dialogue();
    options.er = true;
    assert_edits(&[("He was, er, tired", "He was tired")], options);
}

#[test]
fn flag_only_contexts() {
    assert_flag_only(
        &[
            // Dashes.
            "I was—um—thinking",
            "I was – um – thinking",
            "He reached for the knife—um the gun.",
            "He was, um—thinking",
            "Um—the door",
            // Brackets and parentheses.
            "He was (um) late.",
            "He was [um] late.",
            // Quote edges.
            "“I was, um,” she said",
            "He left um”",
            "She said “no” um, then left.",
            // Between terminal marks.
            "He paused. Um. Then he spoke.",
            "He left. Um?! Then",
            "He left. Um. then",
            // Ellipsis pairs and other mixed marks.
            "I was… um… thinking",
            "It was, um… fine",
            "He said, um. Then",
            "it was erm, cold",
            // Chains that aren't all plain spaces.
            "The cat um, uh, sat",
            "um, uh, the door",
            "He left. Uh. Um. Then",
            // U+2028 and paragraph ends.
            "the um\u{2028}next",
            "He left.\u{2028}Um, the door",
            "He was, um,\u{2028}thinking",
            "He waited um\nThen",
            // Comma pairs where the first comma may be the author's.
            "She bought bread, um, and milk.",
            "The house, uh, in the woods was empty.",
            "Mary, um, I was thinking.",
            "I Think, um, That",
            "She paused, um, before she answered.",
        ],
        dialogue(),
    );
}

#[test]
fn spoken_commands_are_opt_in() {
    assert!(!DraftingOptions::new().spoken_commands);
    assert!(!DraftingOptions::default().spoken_commands);
    for text in [
        "She said comma he left",
        "she left period new paragraph the next day",
        "She spoke scratch that. Then she waited",
    ] {
        assert!(
            report(text, DraftingOptions::new()).matches.is_empty(),
            "{text:?}"
        );
        assert_eq!(applied(text, DraftingOptions::new()), text);
        assert!(report(text, spoken())
            .matches
            .iter()
            .any(|m| m.category == DraftingCategory::SpokenCommand));
    }
    // Without commands there is no dictated quote pair, so the pause is narration.
    let dictated = "open quote I’m fine um close quote she said";
    let off = report(dictated, DraftingOptions::new());
    assert!(off
        .matches
        .iter()
        .all(|m| m.category == DraftingCategory::FilledPause && m.mode == Mode::Narration));
    assert_eq!(off.groups.dialogue.words, 0);
    let on = report(dictated, spoken());
    assert!(!on
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    assert!(on.groups.dialogue.words > 0);
}

#[test]
fn cluster_subsets_apply_cleanly() {
    for (source, expected) in [
        ("We bought milk um comma eggs", "We bought milk, eggs"),
        ("We bought milk comma um eggs", "We bought milk, eggs"),
        ("He said um period Then left", "He said. Then left"),
        ("I said uh question mark then left.", "I said? Then left."),
        (
            "she left period new paragraph the next day",
            "she left.\nThe next day",
        ),
        (
            "she said open quote I’m fine period close quote and left",
            "she said “I’m fine.” and left",
        ),
        (
            "She said uh open quote hello close quote and left.",
            "She said “hello” and left.",
        ),
        ("it was red dash um dash blue", "it was red—blue"),
        (
            "I paused dot dot dot um then continued",
            "I paused… then continued",
        ),
        ("Pause comma um comma then go.", "Pause, then go."),
        ("I went home um scratch that.", "I went home."),
        (
            "Make this um all caps hello and go.",
            "Make this all caps hello and go.",
        ),
        (
            "we stopped um new line\u{2028}They left",
            "we stopped um new line\u{2028}They left",
        ),
        // Flag only as a whole: a flag-only pause, punctuation between members, or two
        // sentence marks meeting.
        (
            "I went home um— dash and then I slept.",
            "I went home um— dash and then I slept.",
        ),
        (
            "I went home um... period Then I slept.",
            "I went home um... period Then I slept.",
        ),
        (
            "He left period um comma then she came.",
            "He left period um comma then she came.",
        ),
        (
            "He left period, um, she came.",
            "He left period, um, she came.",
        ),
        ("I think um, comma, I left.", "I think um, comma, I left."),
    ] {
        let result = report(source, spoken());
        let edits: Vec<_> = result.matches.iter().flat_map(|m| &m.edits).collect();
        for pair in edits.windows(2) {
            assert!(
                pair[0].span.byte.end <= pair[1].span.byte.start,
                "{source:?}"
            );
        }
        for mask in 0..(1 << edits.len()) {
            let subset: Vec<_> = edits
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, edit)| *edit)
                .collect();
            let changed = drafting::apply(source, &subset);
            for bad in [",,", ",.", ".,", "..,", "  ", " ,", " .", "——", "””", "“ "] {
                assert!(!changed.contains(bad), "{source:?}: {changed:?}");
            }
        }
        assert_eq!(drafting::apply(source, &edits), expected, "{source:?}");
        // One edit per cluster, carried by its first match.
        let clusters = result
            .matches
            .iter()
            .filter(|m| !m.edits.is_empty())
            .count();
        assert!(clusters <= 2, "{source:?}: {:?}", result.matches);
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
    let mut options = dialogue();
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
    assert_eq!(applied("He was—er—tired", options), "He was—er—tired");
}

#[test]
fn utf16_spans() {
    let text = "😀. Um, the door";
    let result = report(text, DraftingOptions::new());
    let json: serde_json::Value =
        serde_json::from_str(&writemorebetter_style::reports::to_utf16_json(&result)).unwrap();
    assert_eq!(
        json["matches"][0]["span"],
        serde_json::json!({"start": 4, "end": 6})
    );
    assert_eq!(
        json["matches"][0]["edits"][0]["span"],
        serde_json::json!({"start": 4, "end": 9})
    );
    let mut invalid = result.matches[0].edits[0].clone();
    invalid.span.byte.start = 1; // inside the emoji
    assert_eq!(drafting::apply(text, &[&invalid]), text);
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
        (
            "open quote hello unquote she said",
            "“hello” she said",
            "unquote",
        ),
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
            "She spoke scratch that. Then she waited",
            "She spoke. Then she waited",
            "scratch that",
        ),
        (
            "She spoke delete that. Then she waited",
            "She spoke. Then she waited",
            "delete that",
        ),
        (
            "She spoke strike that. Then she waited",
            "She spoke. Then she waited",
            "strike that",
        ),
        (
            "She spoke undo that. Then she waited",
            "She spoke. Then she waited",
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
        let result = report(source, spoken());
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::SpokenCommand && m.term == term),
            "{source:?}: {:?}",
            result.matches
        );
        assert_eq!(applied(source, spoken()), expected, "{source:?}");
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
        "hello unquote she said",
        "She typed “period” and waited.",
        "At that period, she left the room.",
        "She wrote a sentence. The period was missing.",
        "She marked the page period and folded it.",
    ] {
        assert!(
            !report(text, spoken())
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::SpokenCommand),
            "{text:?}"
        );
    }
}

#[test]
fn review_command_guards() {
    fn unchanged(text: &str) -> (&str, &str) {
        (text, text)
    }
    assert_cases(
        &[
            // Attributive nouns are not lists or compounds.
            unchanged("She has colon cancer and lung disease."),
            unchanged("He had colon surgery and chemo."),
            unchanged("Writers overuse semicolon tricks and dashes."),
            unchanged("Use comma splices sparingly."),
            unchanged("Teachers mark comma errors in red."),
            unchanged("She hated hyphen words."),
            // Verb uses of the deletion commands.
            unchanged("The dog likes to scratch that."),
            unchanged("Did Mary delete that?"),
            unchanged("Go ahead and delete that."),
            unchanged("Why did the cat scratch that?"),
            unchanged("Who did undo that?"),
            unchanged("She leaned over to scratch that, then sighed."),
            unchanged("Delete that."),
            unchanged("Scratch that."),
            // Commands at a sentence start, after a pause or not, and names.
            unchanged("Full stop signs were everywhere."),
            unchanged("Exclamation point tattoos were popular."),
            unchanged("New line items appeared on the bill."),
            unchanged("Open quote marks are curly."),
            unchanged("His brother Colon then left."),
            ("Um comma the dog ran.", "Comma the dog ran."),
            ("He left. Um period She left.", "He left. Period She left."),
            ("Uh dash she left.", "Dash she left."),
            // Published prose.
            unchanged("For a short period his patience held."),
            unchanged("Dash it, we missed the train!"),
            unchanged("The birds dash themselves against the glass."),
            unchanged("Shall I strike that?"),
            unchanged("I saw you strike that wooden post."),
            unchanged("You cannot undo that."),
            unchanged("There are no caps at sea."),
            unchanged("Coma patients rarely wake up."),
            unchanged("Hyphen usage varies."),
            unchanged("“Comma,” she said, “goes here.”"),
            unchanged("She defended the Oxford comma."),
            unchanged("She was the best, period, and everyone knew it."),
            unchanged("It ended—period—with no warning."),
            unchanged("Please delete that file before Monday."),
            unchanged("Don’t delete that email."),
            unchanged("I went to the store scratch that the market"),
        ],
        spoken(),
    );
    // Commands that start a sentence and still apply.
    assert_cases(
        &[
            ("Open quote hello close quote she said", "“hello” she said"),
            (
                "She said open quote I’m fine. close quote and left",
                "She said “I’m fine.” and left",
            ),
            unchanged("Close quote marks are curly."),
            (
                "He left. New paragraph The next day",
                "He left.\nThe next day",
            ),
        ],
        spoken(),
    );
}

#[test]
fn commands_marks_speech_and_overlap() {
    assert_eq!(
        applied("She said comma, he left", spoken()),
        "She said, he left"
    );
    assert_eq!(applied("milk, comma, and", spoken()), "milk, and");
    let dictated = "open quote I’m fine um close quote she said";
    let commands = report(dictated, spoken());
    assert_eq!(
        commands
            .matches
            .iter()
            .filter(|m| m.category == DraftingCategory::SpokenCommand)
            .count(),
        2
    );
    let mut options = spoken();
    options.include_dialogue = true;
    let included = report(dictated, options);
    assert!(included
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause && m.mode == Mode::Dialogue));
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
        let result = report(source, spoken());
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
    let result = report(paired, spoken());
    assert!(result.groups.dialogue.words > 0);
    assert!(!result
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    assert!(result
        .matches
        .iter()
        .any(|m| m.term == "comma" && m.mode == Mode::Dialogue));
    let mut options = spoken();
    options.include_dialogue = true;
    assert!(report(paired, options)
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    let unpaired = "She said open quote I’m fine um";
    let unpaired_report = report(unpaired, spoken());
    assert!(unpaired_report
        .matches
        .iter()
        .any(|m| m.category == DraftingCategory::FilledPause));
    assert!(unpaired_report
        .matches
        .iter()
        .any(|m| m.term == "open quote" && !m.edits.is_empty()));
    assert!(!report(paired, DraftingOptions::new())
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
        (
            "She waited\u{2028}then said um comma he left",
            "She waited\u{2028}then said, he left",
        ),
    ] {
        assert_eq!(applied(source, spoken()), expected, "{source:?}");
    }
}

#[test]
fn pauses_beside_command_words() {
    // With commands on, guards keep these words as prose and only the pause goes.
    for options in [DraftingOptions::new(), spoken()] {
        assert_cases(
            &[
                ("It was um period furniture.", "It was period furniture."),
                (
                    "We had um no caps on the bottles.",
                    "We had no caps on the bottles.",
                ),
                ("She liked um dash and verve.", "She liked dash and verve."),
            ],
            options,
        );
    }
}
