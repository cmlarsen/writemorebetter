use std::sync::{Arc, OnceLock};

use nlprule::Tokenizer;
use writemorebetter_style::reports::clarity::{self, ClarityClauseIntroducer, ClarityOptions};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

const STICKY: &str =
    "It was as if he had been there for all of the time that she was in it with him.";
const CONCRETE: &str = "Bright copper kettles gleamed beneath ancient oak rafters while grey rain lashed cold windows outside tonight.";
const RELATIVE: &str = "She met the man who had sailed every ocean on the map and carried letters across the cold northern sea for twenty long years, and she smiled.";

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report_with(text: &str, options: &ClarityOptions) -> clarity::ClarityReport {
    clarity::report(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        options,
    )
    .unwrap()
}

fn report(text: &str) -> clarity::ClarityReport {
    report_with(text, &ClarityOptions::new())
}

fn glue_words(text: &str) -> Vec<String> {
    let r = report_with(
        text,
        &ClarityOptions::new()
            .with_minimum_sentence_words(1)
            .with_sticky_glue_percent(0.0),
    );
    r.sticky_sentences
        .iter()
        .flat_map(|s| s.glue.iter().map(|g| text[g.byte.clone()].to_owned()))
        .collect()
}

#[test]
fn flags_glue_heavy_sentences_above_the_length_floor() {
    let r = report(STICKY);
    assert_eq!(r.sticky_sentences.len(), 1, "{r:#?}");
    let sticky = &r.sticky_sentences[0];
    assert_eq!(sticky.span.byte, 0..STICKY.len());
    assert_eq!(sticky.words, 20);
    assert_eq!(sticky.glue_words, sticky.glue.len());
    assert!(sticky.glue_percent >= 65.0, "{sticky:#?}");
    assert_eq!(sticky.mode, Mode::Narration);
    assert!(report(CONCRETE).sticky_sentences.is_empty());
    // Short glue-heavy sentences stay below the length floor.
    assert!(report("It was all of it that she had.")
        .sticky_sentences
        .is_empty());
    assert_eq!(
        report_with(
            "It was all of it that she had.",
            &ClarityOptions::new().with_minimum_sentence_words(5)
        )
        .sticky_sentences
        .len(),
        1
    );
}

#[test]
fn glue_words_come_from_closed_class_readings_and_chunks() {
    assert_eq!(
        glue_words("He had a dog and she was at the door."),
        ["He", "had", "a", "and", "she", "was", "at", "the"]
    );
    // Modal "can" is glue; the noun is not.
    assert_eq!(glue_words("She can open it."), ["She", "can", "it"]);
    assert_eq!(glue_words("She kicked the can."), ["She", "the"]);
    // Verb "like" is content; the preposition is glue.
    assert_eq!(glue_words("They like the rain."), ["They", "the"]);
    assert_eq!(glue_words("It looked like rain."), ["It", "like"]);
    // Adverbs are content even with a closed-class reading.
    assert!(!glue_words("She just left now.").contains(&"just".to_owned()));
    assert!(glue_words("She did not know.").contains(&"did".to_owned()));
}

#[test]
fn long_relative_clause_runs_to_the_next_punctuation() {
    let r = report(RELATIVE);
    assert_eq!(r.long_clauses.len(), 1, "{r:#?}");
    let clause = &r.long_clauses[0];
    assert_eq!(clause.introducer, "who");
    assert_eq!(clause.kind, ClarityClauseIntroducer::Relative);
    assert_eq!(&RELATIVE[clause.introducer_span.byte.clone()], "who");
    assert_eq!(
        &RELATIVE[clause.span.byte.clone()],
        "who had sailed every ocean on the map and carried letters across the cold northern sea for twenty long years"
    );
    assert_eq!(clause.words, 20);
    assert_eq!(clause.sentence, 0);
    assert_eq!(clause.sentence_words, 27);
    let shorter = report_with(RELATIVE, &ClarityOptions::new().with_long_clause_words(21));
    assert!(shorter.long_clauses.is_empty());
    assert_eq!(shorter.groups.all.subordinate_clauses, 1);
}

fn clauses(text: &str) -> Vec<(String, String)> {
    report_with(text, &ClarityOptions::new().with_long_clause_words(1))
        .long_clauses
        .iter()
        .map(|c| (c.introducer.clone(), text[c.span.byte.clone()].to_owned()))
        .collect()
}

#[test]
fn introducers_need_a_clause_not_a_phrase_or_question() {
    assert_eq!(
        clauses("She left after the rain stopped."),
        [("after".to_owned(), "after the rain stopped".to_owned())]
    );
    assert!(clauses("She left after dinner.").is_empty());
    assert!(clauses("Who knows the way?").is_empty());
    assert!(clauses("That was the end of it.").is_empty());
    assert!(clauses("So it rained all day.").is_empty());
    assert!(clauses("She ate that apple.").is_empty());
    assert_eq!(
        clauses("She waited although nobody came."),
        [("although".to_owned(), "although nobody came".to_owned())]
    );
    assert_eq!(
        clauses("She saw the house that stood on the hill."),
        [("that".to_owned(), "that stood on the hill".to_owned())]
    );
}

#[test]
fn fronted_clause_without_a_comma_stops_at_the_main_clause() {
    assert_eq!(
        clauses("When I came back from the East last autumn I felt tired."),
        [(
            "when".to_owned(),
            "When I came back from the East last autumn".to_owned()
        )]
    );
}

#[test]
fn unicode_punctuation_bounds_clauses_and_spans_stay_on_char_boundaries() {
    // Em dash, ellipsis, curly quotes and curly apostrophes end or sit inside clauses.
    let text = "She knew the sailor who’d crossed the sea—twice…\u{2028}“It’s the man who sailed the cold sea,” he said, “and he’s gone.”";
    let found = clauses(text);
    assert_eq!(
        found,
        [
            ("who".to_owned(), "who’d crossed the sea".to_owned()),
            ("who".to_owned(), "who sailed the cold sea".to_owned()),
        ],
        "{found:#?}"
    );
    let r = report_with(
        text,
        &ClarityOptions::new()
            .with_minimum_sentence_words(1)
            .with_sticky_glue_percent(0.0)
            .with_long_clause_words(1),
    );
    for span in r
        .sticky_sentences
        .iter()
        .flat_map(|s| std::iter::once(&s.span).chain(&s.glue))
        .chain(r.long_clauses.iter().map(|c| &c.span))
    {
        assert!(text.is_char_boundary(span.byte.start) && text.is_char_boundary(span.byte.end));
        let slice = &text[span.byte.clone()];
        assert_eq!(slice.chars().count(), span.char.len());
        assert_eq!(slice.encode_utf16().count(), span.utf16.len());
    }
    let dialogue = r
        .long_clauses
        .iter()
        .find(|c| c.span.byte.start > text.find('“').unwrap());
    assert_eq!(dialogue.unwrap().mode, Mode::Dialogue);
    // A dialogue edge closes a clause even without punctuation inside the quote.
    let open = "He saw the man who “ran past the cold old harbour wall at dawn” and left.";
    assert_eq!(clauses(open)[0].1, "who");
    // Curly apostrophes in contractions still count as glue.
    assert!(glue_words("It’s what she’d done.").contains(&"’s".to_owned()));
}

#[test]
fn groups_report_shares_by_mode_above_the_sample_minimum() {
    let text = format!("{STICKY} {CONCRETE}\n“{STICKY}”\n{RELATIVE}");
    let r = report_with(&text, &ClarityOptions::new().with_minimum_sentences(1));
    let all = &r.groups.all;
    assert_eq!(all.sentences, 4);
    assert_eq!(all.eligible_sentences, 4);
    assert_eq!(all.sticky.count, 2);
    assert_eq!(all.sticky.percent, Some(50.0));
    assert_eq!(all.long_clauses, 1);
    assert_eq!(all.long_clause_sentences.percent, Some(25.0));
    assert!(all.low_confidence);
    assert_eq!(r.groups.dialogue.sentences, 1);
    assert_eq!(r.groups.dialogue.sticky.count, 1);
    assert_eq!(r.groups.narration.sentences, 3);
    assert_eq!(
        all.glue.count,
        r.groups.narration.glue.count + r.groups.dialogue.glue.count
    );
    assert_eq!(r.words.all, 20 + 16 + 20 + 27);
    let suppressed = report(&text);
    assert_eq!(suppressed.groups.all.sticky.count, 2);
    assert_eq!(suppressed.groups.all.sticky.percent, None);
    assert_eq!(suppressed.groups.all.glue.percent, None);
}

#[test]
fn skipped_names_leave_numerator_and_denominator() {
    let text = "Mara was in it with him and all of the time that she was there for it.";
    let plain = report_with(text, &ClarityOptions::new().with_minimum_sentence_words(1));
    let skipped = clarity::report(
        &analyze_document(
            tokenizer(),
            text,
            &DocumentOptions::new().with_skip_names(vec!["Mara".into()]),
        ),
        &ClarityOptions::new().with_minimum_sentence_words(1),
    )
    .unwrap();
    assert_eq!(
        skipped.sticky_sentences[0].words + 1,
        plain.sticky_sentences[0].words
    );
    assert_eq!(
        skipped.sticky_sentences[0].glue_words,
        plain.sticky_sentences[0].glue_words
    );
}

#[test]
fn options_validate_and_bindings_use_utf16_spans() {
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        &format!("“It’s—”\u{2028}{RELATIVE}"),
        &DocumentOptions::new(),
    );
    let r = document
        .clarity(Some(
            r#"{"longClauseWords":20,"minimum_sentence_words":15}"#,
        ))
        .unwrap();
    assert_eq!(r.long_clauses.len(), 1);
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&r)).unwrap();
    let span = &json["longClauses"][0]["span"];
    let prefix =
        "“It’s—”\u{2028}".encode_utf16().count() + RELATIVE[..RELATIVE.find("who").unwrap()].len();
    assert_eq!(span["start"], prefix);
    assert!(json["longClauses"][0]["introducerSpan"]["end"].is_number());
    assert_eq!(json["longClauses"][0]["kind"], "relative");
    assert!(json["groups"]["all"]["longClauseSentences"].is_object());
    for options in [
        r#"{"unknown":1}"#,
        r#"{"minimumSentenceWords":0}"#,
        r#"{"longClauseWords":0}"#,
        r#"{"stickyGluePercent":101}"#,
    ] {
        assert!(
            matches!(
                document.clarity(Some(options)),
                Err(StyleError::InvalidOptions { .. })
            ),
            "{options}"
        );
    }
    let compact = analyze_document(
        tokenizer(),
        STICKY,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        clarity::report(&compact, &ClarityOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn quoted_questions_are_not_relative_clauses() {
    for text in [
        "“Why would anyone leave a perfectly good boat sitting out on the rocks all through the long winter like that?” she asked.",
        "She turned to him and asked, “Why would anyone leave a perfectly good boat sitting out on the rocks all winter?”",
        "“Where did you put the keys to the old blue truck that we left down by the river last week?” she asked.",
    ] {
        assert!(
            clauses(text).iter().all(|(word, _)| word != "why" && word != "where"),
            "{text}: {:#?}",
            clauses(text)
        );
    }
    // A WH-word inside a statement still opens a clause.
    assert_eq!(
        clauses("“I know why the boat sank,” she said."),
        [("why".to_owned(), "why the boat sank".to_owned())]
    );
}

#[test]
fn fronted_clause_ending_in_a_verb_stops_at_the_main_clause_subject() {
    assert_eq!(
        clauses("When the rain finally stopped we pulled on our boots and walked down through the wet fields to the river to look at the flood."),
        [("when".to_owned(), "When the rain finally stopped".to_owned())]
    );
    // A nested clause after glue stays inside.
    assert_eq!(
        clauses("She said she would come if we called."),
        [("if".to_owned(), "if we called".to_owned())]
    );
}

#[test]
fn coordinated_main_clause_ends_the_subordinate_clause() {
    assert_eq!(
        clauses("She remembered the night when the storm had torn the roof from the barn and her father stood in the yard shouting at the sky for an hour."),
        [(
            "when".to_owned(),
            "when the storm had torn the roof from the barn".to_owned()
        )]
    );
    // Coordinated verbs without a new subject stay inside.
    assert_eq!(
        clauses("She met the man who sailed the sea and carried letters home."),
        [(
            "who".to_owned(),
            "who sailed the sea and carried letters home".to_owned()
        )]
    );
}

#[test]
fn demonstrative_that_opens_no_clause() {
    assert!(
        clauses("She knew that and kept it to herself for the rest of the long cold winter.")
            .is_empty()
    );
    assert_eq!(
        clauses("He said that the ship had sailed."),
        [("that".to_owned(), "that the ship had sailed".to_owned())]
    );
}

#[test]
fn adverbial_wh_words_are_subordinators() {
    let kinds = |text: &str| -> Vec<_> {
        report_with(text, &ClarityOptions::new().with_long_clause_words(1))
            .long_clauses
            .iter()
            .map(|c| (c.introducer.clone(), c.kind))
            .collect()
    };
    assert_eq!(
        kinds("When the rain stopped, we went out."),
        [("when".to_owned(), ClarityClauseIntroducer::Subordinator)]
    );
    assert_eq!(
        kinds("She remembered the night when the storm came."),
        [("when".to_owned(), ClarityClauseIntroducer::Relative)]
    );
}

#[test]
fn line_separators_and_single_quotes() {
    // U+2028 inside a clause is a soft line break, not a boundary.
    assert_eq!(
        clauses("He saw the man who ran past\u{2028}the cold harbour wall."),
        [(
            "who".to_owned(),
            "who ran past\u{2028}the cold harbour wall".to_owned()
        )]
    );
    // Current behaviour: a closing single quote is not a boundary, so an untagged-comma
    // single-quoted line runs into its tag, and the quote reads as narration.
    let text = "‘I saw the man who lived in the house on the hill’ she said.";
    let r = report_with(text, &ClarityOptions::new().with_long_clause_words(1));
    assert_eq!(
        &text[r.long_clauses[0].span.byte.clone()],
        "who lived in the house on the hill’ she said"
    );
    assert_eq!(r.long_clauses[0].mode, Mode::Narration);
}
