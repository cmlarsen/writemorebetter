use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use writemorebetter_style::reports::drafting::{self, DraftingCategory, DraftingOptions};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

#[test]
fn dense_pause_report_scales_linearly() {
    fn best_of_three(text: &str, options: DraftingOptions) -> Duration {
        let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
        (0..3)
            .map(|_| {
                let start = Instant::now();
                let report = drafting::report(&document, &options).unwrap();
                assert!(!report.matches.is_empty());
                start.elapsed()
            })
            .min()
            .unwrap()
    }
    for options in [DraftingOptions::new(), DraftingOptions::dictated()] {
        let small = best_of_three(&"the um dog ".repeat(1_000), options.clone());
        let large = best_of_three(&"the um dog ".repeat(8_000), options);
        assert!(large < small * 12, "report scaling: {small:?} -> {large:?}");
    }
}
/// The report for `text`, after checking that the same options for typed text (no `dictated`)
/// return no edits at all.
fn report(text: &str, options: DraftingOptions) -> drafting::DraftingReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let mut typed = options.clone();
    typed.dictated = false;
    typed.spoken_commands = None;
    let flags = drafting::report(&document, &typed).unwrap();
    assert!(
        flags.matches.iter().all(|m| m.edits.is_empty()),
        "{text:?} typed: {:?}",
        flags.matches
    );
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
/// Text the host knows came from dictation: edits and spoken commands on.
fn spoken() -> DraftingOptions {
    DraftingOptions::dictated()
}
fn dialogue() -> DraftingOptions {
    let mut options = DraftingOptions::dictated();
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
        spoken(),
    );
    assert_edits(
        &[("“He left um!” she said", "“He left!” she said")],
        dialogue(),
    );
    let result = report("So um I was", spoken());
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
            ("Um, I don’t know", "I don’t know"),
            ("Um, ßeta", "ßeta"),
        ],
        spoken(),
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
    let result = report("Um, the door", spoken());
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
        spoken(),
    );
    assert_edits(
        &[("“Oh, um, it’s you,” she said.", "“Oh, it’s you,” she said.")],
        dialogue(),
    );
}

/// Pattern (d) was removed after review D2h: every comma pair other than pattern (c) is flag only.
#[test]
fn comma_pairs_elsewhere_are_flag_only() {
    assert_flag_only(
        &[
            "I was, uh, thinking",
            "He, uh, left.",
            "It wasn’t, um, his fault—",
            "She waited… and, uh, then left",
            "“I was, um, thinking,” she said.",
            "She laughed, um, cried, and left.",
            "If you want, um, we can go.",
            "As I said, um, it’s late.",
        ],
        dialogue(),
    );
    let mut options = dialogue();
    options.er = true;
    assert_flag_only(&["He was, er, tired"], options);
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
            // After a closing quote, a lowercase dialogue tag may follow.
            "He left!” Um… the door",
            "“Why?” um, she asked.",
            "“Stop!” um he shouted.",
            // Ellipsis pairs and other mixed marks.
            "I was… um… thinking",
            "It was, um… fine",
            "He said, um. Then",
            "it was erm, cold",
            // Chains that aren't all plain spaces.
            "The cat um, uh, sat",
            "The cat um, uh sat down.",
            "The cat um—uh sat down.",
            "He paused um… uh the door was open.",
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
fn edits_and_commands_only_for_dictated_text() {
    let built = DraftingOptions::new()
        .with_placeholders(false)
        .with_dictated(true)
        .with_spoken_commands(false)
        .with_filled_pauses(false)
        .with_er(true)
        .with_include_dialogue(true);
    assert!(!built.placeholders && built.dictated && !built.commands());
    assert!(!built.filled_pauses && built.er && built.include_dialogue);
    assert!(!DraftingOptions::new().dictated);
    assert!(!DraftingOptions::new().commands());
    assert!(DraftingOptions::dictated().commands());
    let mut off = DraftingOptions::dictated();
    off.spoken_commands = Some(false);
    assert!(!off.commands());
    assert!(report("She said comma he left", off).matches.is_empty());
    // Typed text: the pause is flagged with no edit.
    let typed = report("So um I was", DraftingOptions::new());
    assert_eq!(typed.matches.len(), 1);
    assert!(typed.matches[0].edits.is_empty());
    // `spokenCommands: true` without `dictated` is rejected: `dictated` is the one way to edits.
    let document = analyze_document(
        tokenizer(),
        "She said comma he left",
        &DocumentOptions::new(),
    );
    let mut invalid = DraftingOptions::new();
    invalid.spoken_commands = Some(true);
    assert!(matches!(
        drafting::report(&document, &invalid),
        Err(writemorebetter_style::StyleError::InvalidOptions { .. })
    ));
    let parsed: DraftingOptions = serde_json::from_str(r#"{"dictated":true}"#).unwrap();
    assert!(parsed.dictated && parsed.commands());
    let aliased: DraftingOptions =
        serde_json::from_str(r#"{"dictated":true,"spoken_commands":false}"#).unwrap();
    assert!(!aliased.commands());
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
        ("He paused um period Then left", "He paused. Then left"),
        (
            "I paused uh question mark then left.",
            "I paused? Then left.",
        ),
        (
            "she left period new paragraph the next day",
            "she left.\nThe next day",
        ),
        (
            "she said open quote I’m fine period close quote and left",
            "she said “I’m fine.” and left",
        ),
        (
            "She turned uh open quote hello close quote and left.",
            "She turned “hello” and left.",
        ),
        ("it was red dash um dash blue", "it was red—blue"),
        (
            "I paused dot dot dot um then continued",
            "I paused… then continued",
        ),
        ("Pause comma um comma then go.", "Pause, then go."),
        // Review D2o: the command is flag only (was "I went home.").
        ("I went home um scratch that.", "I went home scratch that."),
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
        // Review D2o: scratch that is flag only; its former full edit produced "I went home.".
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
    for (positive, negative) in variants(
        "“She’ll check [name]—later…”",
        "“She’ll check the map—later…”",
    ) {
        let found = report(&positive, DraftingOptions::new());
        assert!(
            found
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::Placeholder && m.term == "[name]"),
            "{positive:?}"
        );
        assert!(found.matches.iter().all(|m| m.edits.is_empty()));
        assert!(
            !report(&negative, DraftingOptions::new())
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::Placeholder),
            "{negative:?}"
        );
    }
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
    let result = report(text, spoken());
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
        // A lone opening mark is flag only, so each opening command closes too.
        (
            "She said open quote hello close quote",
            "She said “hello”",
            "open quote",
        ),
        (
            "She said begin quote hello end quote",
            "She said “hello”",
            "begin quote",
        ),
        // An unpaired closing command is flag only (review D2k).
        (
            "hello close quote she said",
            "hello close quote she said",
            "close quote",
        ),
        (
            "hello end quote she said",
            "hello end quote she said",
            "end quote",
        ),
        (
            "open quote hello unquote she said",
            "“hello” she said",
            "unquote",
        ),
        (
            "She said open single quote hello close single quote",
            "She said ‘hello’",
            "open single quote",
        ),
        (
            "hello close single quote she said",
            "hello close single quote she said",
            "close single quote",
        ),
        (
            "She said open paren hello close paren",
            "She said (hello)",
            "open paren",
        ),
        (
            "She said open parenthesis hello close parenthesis",
            "She said (hello)",
            "open parenthesis",
        ),
        (
            "hello close paren she said",
            "hello close paren she said",
            "close paren",
        ),
        (
            "hello close parenthesis she said",
            "hello close parenthesis she said",
            "close parenthesis",
        ),
        (
            "She said open bracket hello close bracket",
            "She said [hello]",
            "open bracket",
        ),
        (
            "hello close bracket she said",
            "hello close bracket she said",
            "close bracket",
        ),
        ("She stopped apostrophe", "She stopped’", "apostrophe"),
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
            // D2o: was "She spoke. Then she waited" when the command was deleted.
            "She spoke scratch that. Then she waited",
            "She spoke scratch that. Then she waited",
            "scratch that",
        ),
        (
            // D2o: was "She spoke. Then she waited" when the command was deleted.
            "She spoke delete that. Then she waited",
            "She spoke delete that. Then she waited",
            "delete that",
        ),
        (
            // D2o: was "She spoke. Then she waited" when the command was deleted.
            "She spoke strike that. Then she waited",
            "She spoke strike that. Then she waited",
            "strike that",
        ),
        (
            // D2o: was "She spoke. Then she waited" when the command was deleted.
            "She spoke undo that. Then she waited",
            "She spoke undo that. Then she waited",
            "undo that",
        ),
        (
            "She stopped caps on then spoke",
            "She stopped caps on then spoke",
            "caps on",
        ),
        (
            "She stopped caps off then spoke",
            "She stopped caps off then spoke",
            "caps off",
        ),
        (
            "She stopped all caps then spoke",
            "She stopped all caps then spoke",
            "all caps",
        ),
        (
            "She stopped no caps then spoke",
            "She stopped no caps then spoke",
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
                "She said open quote I’m fine period close quote and left",
                "She said “I’m fine.” and left",
            ),
            unchanged("Close quote marks are curly."),
            (
                "He left new paragraph The next day",
                "He left\nThe next day",
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
        ("She said open quote hello close quote", "open quote", "“"),
        (
            "“One,” she said. “Two,” he said. She said open quote hello close quote",
            "open quote",
            "“",
        ),
        (
            "\"One,\" she said. \"Two,\" he said. She said open quote hello close quote",
            "open quote",
            "\"",
        ),
        (
            "“One” and \"Two\". She said open quote hello close quote",
            "open quote",
            "“",
        ),
        (
            "She said open single quote hello close single quote",
            "open single quote",
            "‘",
        ),
        (
            "‘One,’ she said. ‘Two,’ he said. She said open single quote hello close single quote",
            "open single quote",
            "‘",
        ),
        (
            "'One,' she said. 'Two,' he said. She said open single quote hello close single quote",
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
fn converted_pairs_are_atomic_per_match() {
    for (text, opening, closing, open_mark, close_mark) in [
        (
            "She said open quote come in close quote and left.",
            "open quote",
            "close quote",
            '“',
            '”',
        ),
        (
            "She said comma open quote come in close quote and left.",
            "open quote",
            "close quote",
            '“',
            '”',
        ),
        (
            "She found open paren the brass key close paren under the mat.",
            "open paren",
            "close paren",
            '(',
            ')',
        ),
        (
            "She found open bracket the brass key close bracket under the mat.",
            "open bracket",
            "close bracket",
            '[',
            ']',
        ),
    ] {
        let result = report(text, spoken());
        let open = result.matches.iter().find(|m| m.term == opening).unwrap();
        let close = result.matches.iter().find(|m| m.term == closing).unwrap();
        assert!(!open.edits.is_empty(), "{text:?}");
        assert!(close.edits.is_empty(), "{text:?}");
        for found in &result.matches {
            let changed = drafting::apply(text, &found.edits.iter().collect::<Vec<_>>());
            assert_eq!(
                changed.matches(open_mark).count(),
                changed.matches(close_mark).count(),
                "{text:?}, {:?}",
                found.term
            );
        }
    }
}

#[test]
fn retraction_commands_never_offer_edits() {
    for text in [
        "She watched him scratch that.",
        "She watched him delete that.",
        "The gate creaked scratch that. she heard a knock.",
        "She spoke scratch that. Then she waited",
        "She spoke strike that. Then she waited",
        "She spoke undo that. Then she waited",
    ] {
        for options in [DraftingOptions::new(), spoken()] {
            let result = report(text, options);
            assert!(
                result.matches.iter().all(|m| m.edits.is_empty()),
                "{text:?}"
            );
        }
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
    // A lone opening quote would insert an unmatched mark, so it is flag only.
    assert!(unpaired_report
        .matches
        .iter()
        .any(|m| m.term == "open quote" && m.edits.is_empty()));
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
            "She waited\u{2028}then paused um comma he left",
            "She waited\u{2028}then paused, he left",
        ),
    ] {
        assert_eq!(applied(source, spoken()), expected, "{source:?}");
    }
}

#[test]
fn pauses_beside_command_words() {
    // Guards keep these words as prose and only the pause goes. In typed text, `report` checks
    // that nothing is edited.
    assert_cases(
        &[
            ("It was um period furniture.", "It was period furniture."),
            (
                "We had um no caps on the bottles.",
                "We had no caps on the bottles.",
            ),
            ("She liked um dash and verve.", "She liked dash and verve."),
        ],
        spoken(),
    );
}

/// Review D2h: sentence starts come from the tokenizer, capitalized pause words mid-sentence are
/// names, and a command carries its cluster's edit.
#[test]
fn review_d2h_sentence_starts_names_and_clusters() {
    // An abbreviation's period isn't a sentence end, so pattern (a) applies without a capital.
    assert_edits(
        &[
            (
                "He left at 3 p.m. um the door was open.",
                "He left at 3 p.m. the door was open.",
            ),
            (
                "She lived in the U.S. um before the war—",
                "She lived in the U.S. before the war—",
            ),
            (
                "“It’s late, etc. um and so on.”",
                "“It’s late, etc. and so on.”",
            ),
        ],
        dialogue(),
    );
    for text in [
        "They listened to Umm Kulthum all night.",
        "The ship docked at Umm Qasr before dawn.",
        "Umm Kulthum sang on the radio.",
        "“Umm Kulthum’s voice…” she said.",
    ] {
        for (source, _) in variants(text, text) {
            let result = report(&source, dialogue());
            assert!(
                result.matches.is_empty(),
                "{source:?}: {:?}",
                result.matches
            );
        }
    }
    // A capitalized pause that starts a sentence is still a pause, before *I* too.
    assert_edits(
        &[
            ("Umm, the door", "The door"),
            ("Umm I don’t know", "I don’t know"),
        ],
        dialogue(),
    );
    let flag_only = |text: &str| {
        let result = report(text, spoken());
        assert!(!result.matches.is_empty(), "{text:?}");
        assert!(
            result.matches.iter().all(|m| m.edits.is_empty()),
            "{text:?}: {:?}",
            result.matches
        );
    };
    flag_only("he left period close paren period then");
    flag_only("he left period\u{2028}period the door");
    flag_only("he left period\u{2028}um the door");
    // The close quote's cluster is flag only, so its opening quote is too (D2j): a pair is
    // edited together or not at all.
    flag_only("open quote fine period close quote comma he said");
    for (text, command, expected) in [
        (
            "he paused uh question mark then left",
            "question mark",
            "he paused? Then left",
        ),
        ("milk um comma eggs", "comma", "milk, eggs"),
    ] {
        let result = report(text, spoken());
        for found in &result.matches {
            assert_eq!(
                found.edits.is_empty(),
                found.term != command,
                "{text:?}: {:?}",
                result.matches
            );
        }
        assert_eq!(applied(text, spoken()), expected);
    }
}

/// No match at all, in every typographic variant.
fn assert_silent(cases: &[&str], options: DraftingOptions) {
    for source in cases {
        for (source, _) in variants(source, source) {
            let result = report(&source, options.clone());
            assert!(
                result.matches.is_empty(),
                "{source:?}: {:?}",
                result.matches
            );
        }
    }
}

/// Review D2j: edits only for dictated text, quote pairs edited together, and the flag
/// precision guards, which hold in both modes.
#[test]
fn review_d2j_flag_precision() {
    let words = [
        // Interjections written with a space or a comma.
        "Uh oh, he thought.",
        "It was an uh oh moment.",
        "Uh huh. That was all he said.",
        "He said uh huh and kept walking.",
        "Uh, oh. The door.",
        "UH OH, he thought.",
        "Um hm, she said nothing.",
        // Reported, or a noun after an article or possessive.
        "She said um and looked away.",
        "Had he really just said um?",
        "She never said um.",
        "He answered with an um and a shrug.",
        "She hated his um more than his silence.",
        "She hated Mary’s um more than her silence.",
        "The baby went uh uh uh and stopped.",
        "He said um—uh—no.",
        // Names, whatever the pause spelling.
        "Um Kulthum sang.",
        "Umm Kulthum sang.",
        // Quoted words, speech or not, and the report's own speech rule.
        "They called it “the um thing” for years.",
        "He wrote “I was um going” on the wall.",
        "He said 'I was um going' and left.",
        "She said, “I was going home and um the door was open.",
        "“It’s fine,” she said. “I was going home and um the door was open.",
        "“I was going home and um the door was open.\n\nShe left.",
        "—Um, I don’t know, she said.",
        "—um… I don’t know, she said.",
    ];
    for options in [DraftingOptions::new(), spoken()] {
        assert_silent(&words, options);
    }
    // With includeDialogue, quoted and dash-dialogue pauses are reported, flag only.
    assert_flag_only(
        &[
            "They called it “the um thing” for years.",
            "She said, “I was going home and um the door was open.",
            "—Um, I don’t know, she said.",
        ],
        dialogue(),
    );
    // Flag only in dictated text: discourse words that can start the next phrase, a lowercase
    // pause after an unquoted ? or !, camel-cased words, and a pause after a pause.
    assert_flag_only(
        &[
            "No, um, one came to the door.",
            "So, um, much for that idea.",
            "Now, um, is the time to act.",
            "Look, um, at the time.",
            "Listen, um, to me.",
            "Right, um, here is where it starts.",
            "Well, um, the door was open.",
            "He yelled Fire! um and ran.",
            "She thought, What now? um and sighed.",
            "Why? um, she wondered.",
            "Who cares? um she muttered.",
            "Um, iPhone sales were down.",
            "Um, eBay was the answer.",
            "Um, Umm, the door.",
        ],
        spoken(),
    );
    let both = report("Um, Umm, the door.", spoken());
    assert_eq!(both.matches.len(), 1);
    assert_eq!(both.matches[0].span.char, 0..7);
    assert_edits(
        &[
            ("So, um, she left.", "So, she left."),
            ("No, um, I didn’t.", "No, I didn’t."),
            ("Yeah, um, the door was open.", "Yeah, the door was open."),
            (
                "What now? Um, the door was open.",
                "What now? The door was open.",
            ),
            ("I walked to the um store.", "I walked to the store."),
            ("He left her um… then", "He left her um… then"),
        ],
        spoken(),
    );
    // Typed text: every one of these is a flag with no edit (`report` checks).
    for text in [
        "So, um, she left.",
        "I walked to the um store.",
        "Um, the door",
    ] {
        let result = report(text, DraftingOptions::new());
        assert_eq!(result.matches.len(), 1, "{text:?}");
    }
}

#[test]
fn review_d2j_commands_and_pairs() {
    for text in [
        "She typed new paragraph and hit enter.",
        "I said question mark to myself.",
        "The words open quote appeared on the screen.",
        "She wrote comma and then erased it.",
    ] {
        for (source, _) in variants(text, text) {
            let result = report(&source, spoken());
            assert!(
                result.matches.is_empty(),
                "{source:?}: {:?}",
                result.matches
            );
        }
    }
    assert_cases(
        &[
            ("I love you full stop she said", "I love you. She said"),
            ("She said comma he left", "She said, he left"),
            (
                "she said open quote come in close quote and shut the door",
                "she said “come in” and shut the door",
            ),
        ],
        spoken(),
    );
    // Flag only: *full stop* without a cue, a lone opening mark, and a pair whose other side
    // is flag only.
    for text in [
        "I love you full stop.",
        "he said open quote I’m fine",
        "she left open paren quietly",
        "open quote fine period close quote comma he said",
        "she said open quote I’m fine comma close quote period then she left",
    ] {
        let result = report(text, spoken());
        assert!(!result.matches.is_empty(), "{text:?}");
        assert!(
            result.matches.iter().all(|m| m.edits.is_empty()),
            "{text:?}: {:?}",
            result.matches
        );
    }
    // A reported pause stays; the command beside it is still edited.
    assert_eq!(
        applied("I said uh question mark then left.", spoken()),
        "I said uh? Then left."
    );
}

/// Every match is a flag with no edit.
fn assert_commands_flag_only(cases: &[&str], options: DraftingOptions) {
    for source in cases {
        let result = report(source, options.clone());
        assert!(!result.matches.is_empty(), "{source:?}");
        assert!(
            result.matches.iter().all(|m| m.edits.is_empty()),
            "{source:?}: {:?}",
            result.matches
        );
    }
}

/// Review D2k: quote, paren and bracket commands are edited only as a pair from `mark_pairs`, a
/// mark the engine typed beside a command makes it flag only, the dictated dialogue tag, and one
/// mark style for quotes and apostrophes.
#[test]
fn review_d2k_pairs_typed_marks_and_tags() {
    assert_commands_flag_only(
        &[
            // Unpaired opening or closing commands.
            "the words open quote and close quote appeared on the screen",
            "she typed the word open paren then close paren and stopped",
            "the then open quote leader close quote resigned",
            "She said “I’m fine close quote and left.",
            "She said \"I'm fine close quote and left.",
            "He said hi close quote and left.",
            "open quote hi close paren",
            "open paren a open paren b close paren",
            "she said open quote close quote and left",
            // A typed mark beside the command, before or after it.
            "it was late period, she left",
            "she left, full stop she never came back",
            "where are you going, question mark she asked",
            "stop, exclamation point he shouted",
            "it was late question mark. she left",
            "he was tired colon, the end",
            "he paused, dot dot dot then spoke",
            "he paused… dot dot dot then spoke",
            "he paused, em dash then spoke",
            "it was late period\u{2028}, she left",
            "it was late,\u{2028}period she left",
            "it was over, new paragraph the next day",
            "he said, open quote, hi, close quote",
            "She said, open quote, I don’t know, close quote.",
            "She said open quote I don’t know. Close quote.",
            "She said, “Open quote I don’t know close quote.”",
            "she said open quote hi, close quote, and left",
        ],
        spoken(),
    );
    assert_cases(
        &[
            // The later opening is the one paired; the earlier stays literal.
            (
                "she said open quote wait open quote stop close quote and ran",
                "she said open quote wait “stop” and ran",
            ),
            // The second opening fails its guard, so only the first pair converts.
            (
                "he said open quote hi close quote and then open quote bye close quote",
                "he said “hi” and then open quote bye close quote",
            ),
            // The same mark typed beside the command is absorbed.
            ("he paused—dash then spoke", "he paused—then spoke"),
            ("it was late period. she left", "it was late. She left"),
            (
                "he was tired comma, and she left",
                "he was tired, and she left",
            ),
            // The dictated dialogue tag.
            (
                "she said comma open quote come in close quote",
                "she said, “come in”",
            ),
            (
                "open quote I’m fine comma thanks close quote he said period",
                "“I’m fine, thanks” he said.",
            ),
            // Without double quotes in the document, apostrophes set the style: straight here,
            // curly otherwise.
            (
                "she said open quote I don't know close quote and left",
                "she said \"I don't know\" and left",
            ),
            (
                "she said open quote I don’t know close quote and left",
                "she said “I don’t know” and left",
            ),
            (
                "she said open quote come in close quote",
                "she said “come in”",
            ),
        ],
        spoken(),
    );
    // A dialogue-tag mark goes with its pair: a flag-only pair leaves the tag flag only.
    assert_commands_flag_only(
        &[
            "she said comma open quote come in, close quote",
            "open quote fine, close quote he said period",
        ],
        spoken(),
    );
    // Without a pair, *said period* and *said comma* stay mentions.
    assert_silent(&["He wanted the last word, so he said period."], spoken());
}

/// Review D2k: a pause word that is a noun after a determiner and adjective, or the subject of a
/// sentence naming it, is not a pause, in both modes. A subject before the pause, a verb that
/// isn't followed by an article, or a question keeps the pause.
#[test]
fn review_d2k_pause_words_as_nouns() {
    let words = [
        "He let out a long uhhhh.",
        "Her name was Uma, and um was her favorite word.",
        "Um, she thought, was not a word.",
    ];
    assert_silent(&words, DraftingOptions::new());
    assert_silent(&words, spoken());
    for text in [
        "I um was going home.",
        "She stood and um was going to leave.",
        "Um, he said, was the door open?",
        "it was a long um day",
    ] {
        let result = report(text, DraftingOptions::new());
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.category == DraftingCategory::FilledPause),
            "{text:?}: {:?}",
            result.matches
        );
    }
}

/// Review D2l: the edit builder alone decides whether a command absorbs a typed mark beside it,
/// so nothing it can't absorb is ever doubled; line breaks after typed terminal punctuation; the
/// dialogue-tag period before a break or another quote; pause-only pairs; no edit after a
/// determiner and adjective; and mark style per kind.
#[test]
fn review_d2l_absorption_breaks_and_tags() {
    assert_commands_flag_only(
        &[
            // Whitespace other than spaces and tabs before the command isn't consumed.
            "She paused,\u{2028}comma and left.",
            "She paused,\u{a0}comma and left.",
            "It was late —\u{2028}dash she left.",
            // A typed period after an ellipsis command isn't absorbed.
            "I don't know dot dot dot.",
            "He paused dot dot dot. Then he spoke.",
            "She trailed off ellipsis.",
            "She trailed off ellipsis…",
            // A spaced dash would be closed on one side only.
            "It was a long day — dash the longest.",
            "She turned — dash and ran.",
            // A pair with only pauses or commands between them.
            "open paren uh close paren",
            "open quote um comma close quote",
            // A line break after a typed comma, or after U+2028.
            "it was over, new paragraph the next day",
            "He left.\u{2028}New paragraph The next day",
        ],
        spoken(),
    );
    assert_commands_flag_only(
        &[
            "She said open quote um close quote and left",
            "open quote um comma close quote",
        ],
        dialogue(),
    );
    assert_cases(
        &[
            // The same mark typed right before still stands in for the command's.
            ("She paused, comma and left.", "She paused, and left."),
            ("It was late—dash she left.", "It was late—she left."),
            // A line break after typed terminal punctuation or a closing quote keeps the mark.
            ("He left. New paragraph The next day", "He left.\nThe next day"),
            ("Why? New line She left.", "Why?\nShe left."),
            ("Stop! next line He ran.", "Stop!\nHe ran."),
            ("He left… new paragraph The next day", "He left…\nThe next day"),
            ("He left… New paragraph The next day", "He left…\nThe next day"),
            ("“Go.” New paragraph She left.", "“Go.”\nShe left."),
            ("“Go,” she said. New paragraph She left.", "“Go,” she said.\nShe left."),
            ("\"Go.\" New paragraph She left.", "\"Go.\"\nShe left."),
            ("She’d gone. New paragraph The end", "She’d gone.\nThe end"),
            // The dialogue-tag period before a break, at the end, or before another quote.
            (
                "open quote hi close quote he said period new paragraph open quote bye close quote she said period",
                "“hi” he said.\n“bye” she said.",
            ),
            (
                "open quote hi close quote he said period new line She nodded",
                "“hi” he said.\nShe nodded",
            ),
            (
                "“hello” she said period open quote hi close quote he said period",
                "“hello” she said. “hi” he said.",
            ),
        ],
        spoken(),
    );
    // A dialogue-tag period goes with the command after it: the break is flag only, so the
    // period is too, and the quote pair is still edited.
    assert_eq!(
        applied(
            "open quote hi close quote he said period new paragraph, the end",
            spoken()
        ),
        "“hi” he said period new paragraph, the end"
    );
    // A pause after a determiner and adjective is never edited in v1 (manager decision, D2n): a
    // restart there gets no match until restart detection (v1.1), and a pause ending the clause
    // is their noun.
    let restarts = [
        "She grabbed the red um the blue one.",
        "I want the big uh the small box.",
        "It was a good um I mean a great day.",
        "He let out a long uhhhh.",
    ];
    assert_silent(&restarts, spoken());
    assert_silent(&restarts, DraftingOptions::new());
    // Each quote kind follows the document's own marks of that kind.
    assert_cases(
        &[
            (
                "“I can't,” she said. “I won't. It's late…” He said open quote fine close quote and left.",
                "“I can't,” she said. “I won't. It's late…” He said “fine” and left.",
            ),
            (
                "\"I can’t,\" she said. He said open quote fine close quote and left.",
                "\"I can’t,\" she said. He said \"fine\" and left.",
            ),
            (
                "It's late. He said open single quote fine close single quote and left.",
                "It's late. He said 'fine' and left.",
            ),
        ],
        spoken(),
    );
}

/// Review D2m: title-cased command phrases and names after a break are prose; a pause after a
/// determiner and adjective is their noun, or flag only before a noun or adjective; a spaced typed mark, a break
/// at the paragraph end and a possessive the engine typed are flag only; and the apostrophe
/// follows the document's apostrophes.
#[test]
fn review_d2m_dictated_findings() {
    let prose = [
        "She worked in film. New Line Cinema hired her.",
        "“Where?” New Line Cinema, he said.",
        "She read the first line aloud. Next line Tom read.",
        "He let out a long uhhhh and sat down.",
        "He let out a long uhhhh that filled the room.",
        "She heard a faint um from the back row.",
        "He gave a loud uh of surprise.",
        "He let out a long uhhhh — then sat.",
    ];
    assert_silent(&prose, DraftingOptions::new());
    assert_silent(&prose, spoken());
    assert_commands_flag_only(
        &[
            // A typed mark with whitespace before it would be closed on one side only.
            "It was a well - hyphen known fact.",
            "He was late - hyphen she left.",
            "It was a well -hyphen known fact.",
            "She paused , comma and left.",
            // A line break with nothing after it in the paragraph.
            "He left. New paragraph\nShe stayed.",
            "He left new paragraph\nShe stayed.",
            "He left new paragraph\u{2028}The next day",
            "“Go.” New paragraph",
            // A possessive the engine already typed.
            "the dog's apostrophe s bone",
            "the dog’s apostrophe s bone",
            "the dog apostrophe s' bone",
        ],
        spoken(),
    );
    assert_cases(
        &[
            ("He left. New paragraph The next day", "He left.\nThe next day"),
            ("He left. Next line The next day", "He left.\nThe next day"),
            ("It was a well—hyphen known fact.", "It was a well—hyphen known fact."),
            // The apostrophe follows the apostrophes, not the single quotes.
            (
                "He said 'no' twice. I don’t know. It wasn’t mine. The dog apostrophe s bone was gone.",
                "He said 'no' twice. I don’t know. It wasn’t mine. The dog’s bone was gone.",
            ),
            (
                "‘Go,’ she said. I don't know. It wasn't mine. She can't. The dog apostrophe s bone was gone.",
                "‘Go,’ she said. I don't know. It wasn't mine. She can't. The dog's bone was gone.",
            ),
        ],
        spoken(),
    );
}

/// Manager decision D2n (2026-10-26): a pause right after a determiner and adjective is never
/// edited in v1. Before a noun or adjective it is flag only; anywhere else it is their noun, even
/// when the next sentence or a later phrase starts with a determiner, adjective or noun.
#[test]
fn review_d2n_no_edit_after_determiner_adjective() {
    assert_flag_only(
        &[
            "It was a long uh day.",
            "He wore a blue um blue shirt.",
            "She picked a big um big one.",
        ],
        spoken(),
    );
    let nouns = [
        "He let out a long uh the short one.",
        "He let out a long uhhhh. The dog barked.",
        "He let out a long uhhhh! A dog barked.",
        "He let out a long uhhhh. Every head turned.",
        "She heard a faint um every few seconds.",
        "a nervous um the whole time",
        "a long uhhhh a second time",
    ];
    assert_silent(&nouns, spoken());
    assert_silent(&nouns, DraftingOptions::new());
    for text in [
        "It was a long um. Long enough.",
        "It was a long uhhhh. Quiet followed.",
        "He let out a long uhhhh loud enough to wake the dead.",
        "He let out a long uhhhh full of regret.",
    ] {
        for options in [spoken(), DraftingOptions::new()] {
            for (source, _) in variants(text, text) {
                let result = report(&source, options.clone());
                assert!(
                    result.matches.iter().all(|m| m.edits.is_empty()),
                    "{source:?}: {:?}",
                    result.matches
                );
            }
        }
    }
}
