//! Person drift (#354) fixtures. Every text here is written for these tests (CC0).

use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::narrative::NarrativePerson;
use writemorebetter_style::reports::person_drift::{
    self, PersonDrift, PersonDriftCastMember, PersonDriftKind, PersonDriftNarrator,
    PersonDriftOptions, PersonDriftPronoun, PersonDriftReport, PersonDriftScenePerson,
    PersonDriftSource, PersonDriftSuppression,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{
    analyze_document, DocumentOptions, MarkedSpan, MarkedVoice, OffsetUnits, StyleDocument,
    StyleError,
};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report_with(
    text: &str,
    document_options: &DocumentOptions,
    options: &PersonDriftOptions,
) -> PersonDriftReport {
    let document = analyze_document(tokenizer(), text, document_options);
    person_drift::report(&document, options).unwrap()
}

fn run(text: &str, options: &PersonDriftOptions) -> PersonDriftReport {
    report_with(text, &DocumentOptions::new(), options)
}

/// (span text, kind, word) for each drift.
fn found_with(text: &str, options: &PersonDriftOptions) -> Vec<(String, PersonDriftKind, String)> {
    run(text, options)
        .drifts
        .iter()
        .map(|drift| {
            (
                text[drift.span.byte.clone()].to_owned(),
                drift.kind,
                drift.word.clone(),
            )
        })
        .collect()
}

fn found(text: &str) -> Vec<(String, PersonDriftKind, String)> {
    found_with(text, &PersonDriftOptions::new())
}

fn scenes(text: &str) -> Vec<(Option<NarrativePerson>, PersonDriftSource)> {
    scenes_with(text, &PersonDriftOptions::new())
}

fn scenes_with(
    text: &str,
    options: &PersonDriftOptions,
) -> Vec<(Option<NarrativePerson>, PersonDriftSource)> {
    run(text, options)
        .scenes
        .iter()
        .map(|scene| (scene.person, scene.source))
        .collect()
}

const THIRD: [&str; 14] = [
    "He walked to the window.",
    "She opened the drawer.",
    "He looked at the street.",
    "She folded the letter.",
    "He listened to the rain.",
    "She lit the lamp.",
    "He counted the coins.",
    "She closed the shutters.",
    "He sat by the fire.",
    "She poured the tea.",
    "He watched the road.",
    "She turned the key.",
    "He waited a while.",
    "She brushed the crumbs away.",
];

const FIRST: [&str; 12] = [
    "I walked to the window.",
    "I opened the drawer.",
    "I looked at the street.",
    "I folded the letter.",
    "I listened to the rain.",
    "I lit the lamp.",
    "I counted the coins.",
    "I closed the shutters.",
    "I sat by the fire.",
    "I poured the tea.",
    "I watched the road.",
    "I turned the key.",
];

/// `n` varied past-tense third-person subject sentences in one paragraph.
fn third_scene(n: usize) -> String {
    THIRD
        .iter()
        .cycle()
        .take(n)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `n` varied past-tense first-person subject sentences in one paragraph.
fn first_scene(n: usize) -> String {
    FIRST
        .iter()
        .cycle()
        .take(n)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A third-person scene of twelve subjects with `middle` after the sixth sentence.
fn in_third(middle: &str) -> String {
    format!("{} {middle} {}", third_scene(6), third_scene(6))
}

fn anna() -> PersonDriftOptions {
    PersonDriftOptions::new().with_narrator(Some(PersonDriftNarrator::new(
        vec!["Anna".to_owned()],
        Some(PersonDriftPronoun::She),
    )))
}

/// A first-person scene with `middle` after eight first-person sentences.
fn in_first(middle: &str) -> String {
    format!("{} {middle} {}", first_scene(8), first_scene(2))
}

fn first_in_third(span: &str, word: &str) -> Vec<(String, PersonDriftKind, String)> {
    vec![(
        span.to_owned(),
        PersonDriftKind::FirstInThird,
        word.to_owned(),
    )]
}

#[test]
fn first_person_slips_in_a_third_person_scene() {
    for (middle, span, word) in [
        ("I crossed the room.", "I", "i"),
        ("The dog followed me.", "me", "me"),
        ("My hands shook.", "My", "my"),
        ("She turned and I followed her out.", "I", "i"),
    ] {
        let text = in_third(middle);
        assert_eq!(found(&text), first_in_third(span, word), "{middle}");
        let report = run(&text, &PersonDriftOptions::new());
        assert_eq!(
            (report.scenes[0].person, report.scenes[0].source),
            (Some(NarrativePerson::Third), PersonDriftSource::Inferred)
        );
        let drift = &report.drifts[0];
        assert_eq!(drift.expected, NarrativePerson::Third);
        assert_eq!(&text[drift.sentence_span.byte.clone()].trim(), &middle);
        assert_eq!(drift.sentence, 6);
        assert_eq!(drift.scene, 0);
    }
    // Authorial *we* is never a slip.
    assert_eq!(found(&in_third("We waited by the gate.")), vec![]);
}

#[test]
fn third_person_slips_in_a_first_person_scene() {
    let text = in_first("I crossed the room. She picked up the knife. I waited.");
    assert_eq!(
        found_with(&text, &anna()),
        vec![(
            "She".to_owned(),
            PersonDriftKind::ThirdInFirst,
            "she".to_owned()
        )]
    );
    let text = in_first("I crossed the room. Anna picked up the knife. I waited.");
    let report = run(&text, &anna());
    assert_eq!(report.scenes[0].person, Some(NarrativePerson::First));
    assert_eq!(report.drifts.len(), 1);
    let drift = &report.drifts[0];
    assert_eq!(&text[drift.span.byte.clone()], "Anna");
    assert_eq!(
        (drift.kind, drift.word.as_str(), drift.expected),
        (
            PersonDriftKind::NarratorName,
            "Anna",
            NarrativePerson::First
        )
    );
    // Without a narrator the scene says why it raised nothing.
    let text = in_first("I crossed the room. She picked up the knife. I waited.");
    let quiet = run(&text, &PersonDriftOptions::new());
    assert_eq!(quiet.drifts, vec![]);
    assert_eq!(
        quiet.scenes[0].suppressed,
        Some(PersonDriftSuppression::NoNarrator)
    );
    // A *they* narrator gets name flags only.
    let they = PersonDriftOptions::new().with_narrator(Some(PersonDriftNarrator::new(
        vec!["Anna".to_owned()],
        Some(PersonDriftPronoun::They),
    )));
    assert_eq!(
        found_with(
            &in_first("I crossed the room. They picked up the knife. I waited."),
            &they
        ),
        vec![]
    );
}

#[test]
fn curly_quotes() {
    assert_eq!(found(&in_third("“I can’t,” she said.")), vec![]);
    assert_eq!(
        found_with(
            &in_first("I crossed the room. “Where’s Anna?” he asked. I waited."),
            &anna()
        ),
        vec![]
    );
    // Straight quotes behave the same.
    assert_eq!(found(&in_third("\"I can't,\" she said.")), vec![]);
}

#[test]
fn curly_apostrophes() {
    // *’d gone* is past perfect, which agrees with a past scene.
    assert_eq!(
        found(&in_third("I’d already gone.")),
        first_in_third("I", "i")
    );
    assert_eq!(
        found(&in_third("I'd already gone.")),
        first_in_third("I", "i")
    );
    // *’m* is present: unmarked thought, not a slip (E3).
    assert_eq!(found(&in_third("I’m done.")), vec![]);
    let text = in_first("I crossed the room. Anna’s hand shook. I waited.");
    assert_eq!(
        found_with(&text, &anna()),
        vec![(
            "Anna’s".to_owned(),
            PersonDriftKind::NarratorName,
            "Anna".to_owned()
        )]
    );
}

#[test]
fn em_dashes() {
    assert_eq!(found(&in_third("She turned—I swear it—and left.")), vec![]);
    // A dash pair alone exempts, even when every chain agrees.
    assert_eq!(found(&in_third("She turned—I knew it—and left.")), vec![]);
    assert_eq!(found(&in_third("She turned (I knew it) and left.")), vec![]);
    assert_eq!(found(&in_third("She turned [I knew it] and left.")), vec![]);
    assert_eq!(found(&in_third("She turned, and then—I followed.")), vec![]);
    assert_eq!(
        found(&in_third("She turned and I followed her out.")),
        first_in_third("I", "i")
    );
    // Dash dialogue (E7).
    let text = format!(
        "{}\n\n—I’m here, he said.\n\n—I crossed the room.\n\n{}",
        third_scene(6),
        third_scene(6)
    );
    assert_eq!(found(&text), vec![]);
}

#[test]
fn ellipses() {
    assert_eq!(found(&in_third("I waited…")), vec![]);
    assert_eq!(found(&in_third("I waited...")), vec![]);
    let text = format!("He paused… then sat. {}", in_third("I waited… then left."));
    let report = run(&text, &PersonDriftOptions::new());
    assert_eq!(report.drifts.len(), 1, "{:?}", report.drifts);
    let drift = &report.drifts[0];
    assert_eq!(&text[drift.span.byte.clone()], "I");
    let before = &text[..drift.span.byte.start];
    assert_eq!(drift.span.char.start, before.chars().count());
    assert_eq!(drift.span.utf16.start, before.encode_utf16().count());
    // Each ellipsis before the slip is one UTF-16 unit but three bytes.
    assert_eq!(drift.span.byte.start - drift.span.utf16.start, 2);
}

#[test]
fn line_separators() {
    let text = format!(
        "{} He sat down.\u{2028}I watched the door.\u{2028}He stood. {}",
        third_scene(6),
        third_scene(6)
    );
    let report = run(&text, &PersonDriftOptions::new());
    assert_eq!(report.drifts.len(), 1, "{:?}", report.drifts);
    let drift = &report.drifts[0];
    assert_eq!(&text[drift.span.byte.clone()], "I");
    let before = &text[..drift.span.byte.start];
    assert_eq!(drift.span.byte.start, before.len());
    assert_eq!(drift.span.char.start, before.chars().count());
    assert_eq!(drift.span.utf16.start, before.encode_utf16().count());
    assert_eq!(drift.span.char.end - drift.span.char.start, 1);
    assert!(text[drift.sentence_span.byte.clone()].starts_with("I watched the door."));
}

#[test]
fn thought_and_speech_are_not_narration() {
    for middle in [
        "I have to get out, she thought.",
        "I’m leaving, he said.",
        "I crossed the room, she thought.",
        "I crossed the room, I thought.",
        "“I crossed the room,” she said.",
        "She said, “I crossed the room.”",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
    // A tag's subject is never a candidate, but it still counts toward the scene person.
    let report = run(
        &in_third("I’m leaving, he said."),
        &PersonDriftOptions::new(),
    );
    assert_eq!(report.scenes[0].subjects.first, 0);
    assert_eq!(report.scenes[0].subjects.third, 13);
}

#[test]
fn caller_embedded_spans_are_not_counted() {
    let letter = "I walked home. I sat down. I wrote this. I waited. I slept.";
    let text = format!("{} {letter} {}", third_scene(6), third_scene(6));
    let start = text.find(letter).unwrap();
    let options = DocumentOptions::new().with_marked_spans(
        OffsetUnits::Byte,
        vec![MarkedSpan::new(
            start..start + letter.len(),
            MarkedVoice::Embedded,
        )],
    );
    let report = report_with(&text, &options, &PersonDriftOptions::new());
    assert_eq!(report.scenes[0].subjects.first, 0);
    assert_eq!(report.scenes[0].person, Some(NarrativePerson::Third));
    assert_eq!(report.drifts, vec![]);
    // Unmarked, the letter's five first-person subjects make the scene first person.
    assert_eq!(
        scenes(&text),
        vec![(Some(NarrativePerson::First), PersonDriftSource::Inferred)]
    );
}

#[test]
fn questions_exclamations_and_trailing_off() {
    for middle in [
        "Did I cross the room?",
        "I crossed the room!",
        "I crossed the room, and was it wise?",
        "I crossed the room—",
        "I crossed the room:",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
}

#[test]
fn tense_disagreement() {
    for middle in [
        "I can’t do this.",
        "I suppose she was right.",
        "As I have said, he was tired.",
        "I think he was lying.",
        "I would never go back.",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
    // A present scene accepts only present chains.
    let present: String = THIRD
        .iter()
        .take(12)
        .map(|sentence| {
            sentence
                .replace("walked", "walks")
                .replace("opened", "opens")
                .replace("looked", "looks")
                .replace("folded", "folds")
                .replace("listened", "listens")
                .replace("lit", "lights")
                .replace("counted", "counts")
                .replace("closed", "closes")
                .replace("sat", "sits")
                .replace("poured", "pours")
                .replace("watched", "watches")
                .replace("turned", "turns")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let report_present = run(
        &format!("{present} I cross the room."),
        &PersonDriftOptions::new(),
    );
    assert_eq!(
        report_present.scenes[0].tense,
        Some(writemorebetter_style::narrative::NarrativeTense::Present)
    );
    assert_eq!(report_present.drifts.len(), 1);
    assert_eq!(found(&format!("{present} I crossed the room.")), vec![]);
}

#[test]
fn interjections_and_numerals() {
    for middle in [
        "My God, the noise was loud.",
        "Oh my, the noise was loud.",
        "Dear me, the noise was loud.",
        "Henry I ruled the land.",
        "Chapter I ended there.",
        "They had dances at I.O.O.F. Hall.",
        "They opened the first mine.",
        "The railway linked mine with mine.",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
}

#[test]
fn display_colons() {
    assert_eq!(
        found(&in_third("The note read: I will be back by noon.")),
        vec![]
    );
    assert_eq!(
        found(&in_third("The note read: I went to the store.")),
        vec![]
    );
    // The rest of the paragraph after the lead-in is the document too.
    assert_eq!(
        found(&in_third("The sign said: Closed. I went home.")),
        vec![]
    );
}

#[test]
fn short_lines_and_unbalanced_quotes() {
    let text = format!("{}\n\nMy dear Anna,\n\n{}", third_scene(6), third_scene(6));
    assert_eq!(found(&text), vec![]);
    let text = format!(
        "{}\n\n“He went home and I stayed behind.\n\n{}",
        third_scene(6),
        third_scene(6)
    );
    assert_eq!(found(&text), vec![]);
}

#[test]
fn scene_inference() {
    // *they* is never counted.
    let they = "They walked home. They sat down. They waited. They slept. They ate. They left. \
                They argued. They laughed. They rested.";
    let report_they = run(they, &PersonDriftOptions::new());
    assert_eq!(report_they.scenes[0].subjects.third, 0);
    assert_eq!(scenes(they), vec![(None, PersonDriftSource::Undetermined)]);
    // A second-person scene is reported, never flagged.
    let second = "You walk home. You sit down. You wait. You sleep. You eat. You leave. \
                  You argue. You laugh. I crossed the room.";
    assert_eq!(
        scenes(second),
        vec![(Some(NarrativePerson::Second), PersonDriftSource::Inferred)]
    );
    assert_eq!(found(second), vec![]);
    // A *we*-only scene is never first person.
    let we = "We walked home. We sat down. We waited. We slept. We ate. We left. We argued. \
              We laughed. We rested.";
    assert_eq!(scenes(we), vec![(None, PersonDriftSource::Mixed)]);
    // Below the evidence floor.
    assert_eq!(
        scenes(&in_third_short()),
        vec![(None, PersonDriftSource::Undetermined)]
    );
    assert_eq!(found(&in_third_short()), vec![]);
    // The dead band: two first-person subjects against eight third-person ones.
    let mixed = format!("{} I walked home. I sat down.", third_scene(8));
    assert_eq!(scenes(&mixed), vec![(None, PersonDriftSource::Mixed)]);
    assert_eq!(found(&mixed), vec![]);
    assert_eq!(
        scenes(&first_scene(8)),
        vec![(Some(NarrativePerson::First), PersonDriftSource::Inferred)]
    );
}

fn in_third_short() -> String {
    format!("{} I crossed the room.", third_scene(3))
}

#[test]
fn too_many_slips_and_undetermined_tense() {
    let options = PersonDriftOptions::new().with_person(Some(NarrativePerson::Third));
    let text = format!(
        "{} I crossed the room. {} The dog followed me. {} My hands shook.",
        third_scene(4),
        third_scene(4),
        third_scene(4)
    );
    let result = run(&text, &options);
    assert_eq!(result.drifts, vec![]);
    assert_eq!(
        result.scenes[0].suppressed,
        Some(PersonDriftSuppression::TooManySlips)
    );
    // Two slips are within the default cap.
    let text = format!(
        "{} I crossed the room. {} The dog followed me.",
        third_scene(4),
        third_scene(4)
    );
    assert_eq!(run(&text, &options).drifts.len(), 2);
    assert_eq!(
        run(&text, &options.clone().with_maximum_slips(1)).scenes[0].suppressed,
        Some(PersonDriftSuppression::TooManySlips)
    );
    // No chains to decide a tense: nothing is flagged.
    let text = "The hall. The door. A lamp. I crossed the room.";
    let result = run(text, &options);
    assert_eq!(result.scenes[0].tense, None);
    assert_eq!(
        result.scenes[0].suppressed,
        Some(PersonDriftSuppression::UndeterminedTense)
    );
    assert_eq!(result.drifts, vec![]);
}

#[test]
fn antecedents_block_third_in_first() {
    let narrator = anna();
    let cast = narrator.clone().with_cast(vec![PersonDriftCastMember::new(
        "Mara",
        PersonDriftPronoun::She,
    )]);
    let candidate = "I crossed the room. She picked up the knife. I waited.";
    for (before, options) in [
        ("Mara came in.", &cast),
        ("The woman by the door said nothing.", &narrator),
        ("“She is late,” I said.", &narrator),
        ("I saw Lucy at the gate.", &narrator),
    ] {
        let text = format!("{} {before} {candidate} {}", first_scene(8), first_scene(2));
        assert_eq!(found_with(&text, options), vec![], "{before}");
    }
    // A cast member with a different pronoun is not an antecedent.
    let tom = narrator.clone().with_cast(vec![PersonDriftCastMember::new(
        "Tom",
        PersonDriftPronoun::He,
    )]);
    let text = format!(
        "{} I saw Tom at the gate. {candidate} {}",
        first_scene(8),
        first_scene(2)
    );
    assert_eq!(found_with(&text, &tom).len(), 1);
    assert_eq!(
        found_with(
            &in_first("I crossed the deck. The ship rolled. She shuddered. I waited."),
            &narrator
        ),
        vec![]
    );
    // A self-distancing frame.
    assert_eq!(
        found_with(
            &in_first("The girl I had been was gone. She picked up the knife. I waited."),
            &narrator
        ),
        vec![]
    );
    assert_eq!(
        found_with(&in_first("The girl I had been… she never…"), &narrator),
        vec![]
    );
    // The sandwich rule: no adjacent first-person sentence in the paragraph.
    let text = format!(
        "{}\n\nShe picked up the knife.\n\n{}",
        first_scene(8),
        first_scene(2)
    );
    assert_eq!(found_with(&text, &narrator), vec![]);
    // Recurring *she* subjects are a character, not a slip.
    let text = in_first(
        "She came in. I crossed the room. She sat. I waited. She picked up the knife. I waited.",
    );
    assert_eq!(found_with(&text, &narrator), vec![]);
}

#[test]
fn narrator_name_contexts() {
    let narrator = anna();
    for middle in [
        "I crossed the room. A girl named Anna picked up the knife. I waited.",
        "I crossed the room. My name was Anna. I waited.",
        "I crossed the room. I, Anna, picked up the knife. I waited.",
        "I crossed the room. Mrs. Anna picked up the knife. I waited.",
    ] {
        assert_eq!(found_with(&in_first(middle), &narrator), vec![], "{middle}");
    }
    // A name shared with a cast member is ambiguous.
    let shared = PersonDriftOptions::new()
        .with_narrator(Some(PersonDriftNarrator::new(
            vec!["Anna Reyes".to_owned(), "Anna".to_owned()],
            Some(PersonDriftPronoun::She),
        )))
        .with_cast(vec![PersonDriftCastMember::new(
            "Anna Lee",
            PersonDriftPronoun::She,
        )]);
    assert_eq!(
        found_with(
            &in_first("I crossed the room. Anna picked up the knife. I waited."),
            &shared
        ),
        vec![]
    );
    // Multi-word names match whole; a prefix of a longer word does not.
    let full = PersonDriftOptions::new().with_narrator(Some(PersonDriftNarrator::new(
        vec!["Anna Reyes".to_owned()],
        Some(PersonDriftPronoun::She),
    )));
    assert_eq!(
        found_with(
            &in_first("I crossed the room. Anna Reyes picked up the knife. I waited."),
            &full
        ),
        vec![(
            "Anna Reyes".to_owned(),
            PersonDriftKind::NarratorName,
            "Anna Reyes".to_owned()
        )]
    );
    assert_eq!(
        found_with(
            &in_first("I crossed the room. Annabel picked up the knife. I waited."),
            &narrator
        ),
        vec![]
    );
}

#[test]
fn declarations() {
    // A declared person flags a scene too short to infer, given a determinable tense.
    let short = "He walked in. He sat down. She poured the tea. He waited. I crossed the room.";
    assert_eq!(found(short), vec![]);
    let third = PersonDriftOptions::new().with_person(Some(NarrativePerson::Third));
    assert_eq!(found_with(short, &third), first_in_third("I", "i"));
    assert_eq!(
        scenes_with(short, &third),
        vec![(Some(NarrativePerson::Third), PersonDriftSource::Declared)]
    );
    // A scene declaration beats the document person and inference.
    let text = format!(
        "{}\n\n* * *\n\n{}",
        first_scene(8),
        in_third("I crossed the room.")
    );
    let options = PersonDriftOptions::new()
        .with_person(Some(NarrativePerson::First))
        .with_scene_persons(vec![PersonDriftScenePerson::new(1, NarrativePerson::Third)]);
    assert_eq!(
        scenes_with(&text, &options),
        vec![
            (Some(NarrativePerson::First), PersonDriftSource::Declared),
            (Some(NarrativePerson::Third), PersonDriftSource::Declared)
        ]
    );
    // A declared third-person scene bypasses the witness guard.
    assert_eq!(found_with(&text, &options), first_in_third("I", "i"));
    // Out of range.
    let document = analyze_document(tokenizer(), &text, &DocumentOptions::new());
    let bad = PersonDriftOptions::new()
        .with_scene_persons(vec![PersonDriftScenePerson::new(2, NarrativePerson::Third)]);
    assert!(matches!(
        person_drift::report(&document, &bad),
        Err(StyleError::InvalidOptions { .. })
    ));
    let zero = PersonDriftOptions::new().with_minimum_subjects(0);
    assert!(matches!(
        person_drift::report(&document, &zero),
        Err(StyleError::InvalidOptions { .. })
    ));
    for bad in [
        PersonDriftOptions::new()
            .with_narrator(Some(PersonDriftNarrator::new(vec![" ".to_owned()], None))),
        PersonDriftOptions::new()
            .with_cast(vec![PersonDriftCastMember::new("", PersonDriftPronoun::He)]),
    ] {
        assert!(matches!(
            person_drift::report(&document, &bad),
            Err(StyleError::InvalidOptions { .. })
        ));
    }
}

#[test]
fn options_json() {
    let style = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        &in_third("I crossed the room."),
        &DocumentOptions::new(),
    );
    for json in [
        r#"{"person":"mixed"}"#,
        r#"{"typo":1}"#,
        r#"{"narrator":{"names":["Anna"],"typo":1}}"#,
        r#"{"cast":[{"name":"Mara"}]}"#,
        r#"{"scenePersons":[{"scene":5,"person":"third"}]}"#,
    ] {
        assert!(
            matches!(
                style.person_drift(Some(json)),
                Err(StyleError::InvalidOptions { .. })
            ),
            "{json}"
        );
    }
    let camel = style
        .person_drift(Some(
            r#"{"person":"third","scenePersons":[{"scene":0,"person":"third"}],"narrator":{"names":["Anna","Anna Reyes"],"pronoun":"she"},"cast":[{"name":"Mara","pronoun":"she"}],"minimumSubjects":8,"maximumSlips":2,"includeWitnessScenes":false}"#,
        ))
        .unwrap();
    let snake = style
        .person_drift(Some(
            r#"{"person":"third","scene_persons":[{"scene":0,"person":"third"}],"minimum_subjects":8,"maximum_slips":2,"include_witness_scenes":false}"#,
        ))
        .unwrap();
    assert_eq!(camel.drifts, snake.drifts);
    assert_eq!(camel.drifts.len(), 1);
    assert_eq!(style.person_drift(None).unwrap().drifts.len(), 1);
}

#[test]
fn scene_independence_and_the_witness_guard() {
    let text = format!(
        "Chapter 1\n\n{}\n\nChapter 2\n\n{}",
        first_scene(10),
        in_third("I crossed the room.")
    );
    let result = run(&text, &PersonDriftOptions::new());
    assert_eq!(
        result
            .scenes
            .iter()
            .map(|scene| (scene.person, scene.source))
            .collect::<Vec<_>>(),
        vec![
            (Some(NarrativePerson::First), PersonDriftSource::Inferred),
            (Some(NarrativePerson::Third), PersonDriftSource::Inferred)
        ]
    );
    assert_eq!(result.drifts, vec![]);
    assert_eq!(
        result.scenes[1].suppressed,
        Some(PersonDriftSuppression::WitnessNarrator)
    );
    let include = PersonDriftOptions::new().with_include_witness_scenes(true);
    let drifts: Vec<PersonDrift> = run(&text, &include).drifts;
    assert_eq!(drifts.len(), 1);
    assert_eq!(drifts[0].scene, 1);
    assert_eq!(&text[drifts[0].span.byte.clone()], "I");
}

#[test]
fn compact_documents_and_json_shape() {
    let text = format!("😀 {}", in_third("I crossed the room."));
    let compact = analyze_document(
        tokenizer(),
        &text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        person_drift::report(&compact, &PersonDriftOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    let result = run(&text, &PersonDriftOptions::new());
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&result)).unwrap();
    let scene = &json["scenes"][0];
    assert_eq!(scene["scene"], 0);
    assert_eq!(scene["span"]["start"], 0);
    assert_eq!(
        scene["paragraphs"],
        serde_json::json!({"start": 0, "end": 1})
    );
    assert_eq!(scene["sentences"]["start"], 0);
    assert_eq!(scene["person"], "third");
    assert_eq!(scene["source"], "inferred");
    assert_eq!(scene["tense"], "past");
    assert_eq!(
        scene["subjects"],
        serde_json::json!({"first": 1, "second": 0, "third": 12})
    );
    assert!(scene["narrationWords"].as_u64().unwrap() > 0);
    assert_eq!(scene["suppressed"], serde_json::Value::Null);
    let drift = &json["drifts"][0];
    let start = text.find("I crossed").unwrap();
    let utf16 = text[..start].encode_utf16().count() as u64;
    assert_eq!(
        drift["span"],
        serde_json::json!({"start": utf16, "end": utf16 + 1})
    );
    assert_eq!(drift["sentence"], 6);
    assert_eq!(drift["scene"], 0);
    assert_eq!(drift["kind"], "firstInThird");
    assert_eq!(drift["word"], "i");
    assert_eq!(drift["expected"], "third");
    assert!(drift["sentenceSpan"]["end"].as_u64().unwrap() > utf16);
    let words = json["narrationWords"].as_f64().unwrap();
    assert_eq!(json["perThousandWords"].as_f64().unwrap(), 1000.0 / words);
}

/// The text every binding test round-trips (Swift, WASM, Python).
#[test]
fn binding_fixture() {
    let text = "😀 He sat. He waited. … I crossed the room. She smiled. He nodded.";
    let third = PersonDriftOptions::new().with_person(Some(NarrativePerson::Third));
    let result = run(text, &third);
    assert_eq!(result.drifts.len(), 1, "{result:?}");
    let drift = &result.drifts[0];
    assert_eq!(&text[drift.span.byte.clone()], "I");
    let before = &text[..drift.span.byte.start];
    assert_eq!(drift.span.utf16.start, before.encode_utf16().count());
    assert_eq!(drift.span.char.start, before.chars().count());
    assert_eq!(
        result.scenes[0].tense,
        Some(writemorebetter_style::narrative::NarrativeTense::Past)
    );
    assert_eq!(run(text, &PersonDriftOptions::new()).drifts, vec![]);
}

/// *mine* after a verb is the pronoun, so the slip is flagged (#354 review).
#[test]
fn mine_after_a_verb() {
    for middle in [
        "Her hand found mine.",
        "His eyes met mine.",
        "She took mine.",
    ] {
        assert_eq!(
            found(&in_third(middle)),
            first_in_third("mine", "mine"),
            "{middle}"
        );
    }
}

/// An exempt first-person word does not hide a later slip in the same sentence, and exempt words
/// do not use up the slip budget (#354 review).
#[test]
fn exempt_words_do_not_hide_slips() {
    for middle in [
        "My God, I had forgotten the key.",
        "Oh my, I had forgotten the key.",
        "My goodness, the rain was cold, and I walked faster.",
        "The rain was cold—my God, how cold—and I walked faster.",
        "The rain was cold (my coat was thin) and I walked faster.",
    ] {
        assert_eq!(
            found(&in_third(middle)),
            first_in_third("I", "i"),
            "{middle}"
        );
    }
    let text = format!(
        "{}\n\nOh my God! What had she done?\n\nMy God, the cold!\n\nOh my goodness, the mud!\n\nI walked to the gate.\n\n{}",
        third_scene(6),
        third_scene(6)
    );
    let report = run(&text, &PersonDriftOptions::new());
    assert_eq!(report.scenes[0].suppressed, None);
    assert_eq!(found(&text), first_in_third("I", "i"));
}

/// Titles with a capital *My* or *I* mid-sentence are not the pronoun (#354 review).
#[test]
fn titles_are_not_slips() {
    for middle in [
        "They had seen My Fair Lady at the Regent the week before.",
        "She had read I Capture the Castle twice that summer.",
        "Somewhere a radio was playing I Walked the Line.",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
}

/// Trailing tags the voice layer leaves as narration still report the clause (#354 review).
#[test]
fn trailing_reporting_tags() {
    for middle in [
        "I went to the station at noon, she wrote.",
        "I went to the station at noon, she remembered.",
        "I went to the station at noon, she recalled.",
        "I went to the station at noon, she had said.",
        "I went to the station at noon, Valancy wrote.",
        "I was here first, the note said.",
        "I saw you, the message read.",
        "I’d gone to the station at noon, she’d written.",
    ] {
        assert_eq!(found(&in_third(middle)), vec![], "{middle}");
    }
    for middle in [
        "Anna was a fool, she thought.",
        "Anna was a fool, I thought.",
        "Anna was late again, I said.",
        "I crossed the room. She was a fool, I thought. I waited.",
    ] {
        assert_eq!(found_with(&in_first(middle), &anna()), vec![], "{middle}");
    }
    // A slip before a comma that is not a tag is still flagged.
    assert_eq!(
        found(&in_third("I went to the station, and the train was late.")),
        first_in_third("I", "i")
    );
}

/// Another woman enters the scene by name, by narration *her*, or as an occupation or animal
/// (#354 review).
#[test]
fn more_antecedents_block_third_in_first() {
    for middle in [
        "I opened the door. Mara stood on the step. She was soaked through. I let her in.",
        "I handed her the cup. She drank it slowly. I waited.",
        "Her hands were shaking when I opened the door. She came in. I shut it.",
        "The nurse came in at six. I sat up. She checked the chart. I waited.",
        "I fed the cat. She purred against my ankles. I smiled.",
    ] {
        assert_eq!(found_with(&in_first(middle), &anna()), vec![], "{middle}");
    }
    let ben = PersonDriftOptions::new().with_narrator(Some(PersonDriftNarrator::new(
        vec!["Ben".to_owned()],
        Some(PersonDriftPronoun::He),
    )));
    for middle in [
        "I waited. Tom came in and he sat down. I nodded.",
        "I waited. Tom came in. He sat down. I nodded.",
    ] {
        let text = format!("{} {middle} I sat by the fire.", first_scene(8));
        assert_eq!(found_with(&text, &ben), vec![], "{middle}");
    }
    // The deliberate slip is still flagged.
    assert_eq!(
        found_with(
            &in_first("I crossed the room. She picked up the knife. I waited."),
            &anna()
        )
        .len(),
        1
    );
}

/// The narrator's name in someone else's report or after a kinship title (#354 review).
#[test]
fn narrator_name_in_reports_and_kinship() {
    let narrator = PersonDriftOptions::new().with_narrator(Some(PersonDriftNarrator::new(
        vec!["Anna".to_owned(), "Anna Reyes".to_owned()],
        Some(PersonDriftPronoun::She),
    )));
    for middle in [
        "The captain said that Anna was to stay behind.",
        "Then Tom came running with the news that Anna had gone into the woods.",
        "Word came down from the house that Anna was wanted in the kitchen.",
        "Everyone in town believed Anna was mad.",
        "Aunt Anna had died the winter before I was born.",
        "My grandmother Anna had planted the hedge.",
        "My grandmother Anna’s hedge had grown wild.",
    ] {
        assert_eq!(found_with(&in_first(middle), &narrator), vec![], "{middle}");
    }
    // The narrator's own report of her name is still a slip.
    assert_eq!(
        found_with(&in_first("I knew that Anna was tired."), &narrator).len(),
        1
    );
}

/// The body of an unmarked letter or diary entry after its salutation or header (#354 review).
#[test]
fn embedded_letters_and_diaries() {
    for (header, body) in [
        (
            "Dear Aunt Mildred,",
            "I went to the station on Monday and waited until dark.",
        ),
        (
            "From the diary of Valancy Stirling, June 3rd.",
            "I walked to the gate and back.",
        ),
        (
            "She unfolded the note. It said:",
            "I walked to the gate and back.",
        ),
        ("Dearest Tom—", "I’d walked to the gate and back twice."),
    ] {
        let text = format!(
            "{}\n\n{header}\n\n{body}\n\n{}",
            third_scene(6),
            third_scene(6)
        );
        assert_eq!(found(&text), vec![], "{header}");
        // Without the header the body is narration and its *I* is a slip.
        let bare = format!("{}\n\n{body}\n\n{}", third_scene(6), third_scene(6));
        assert_eq!(found(&bare).len(), 1, "{body}");
    }
    // Narration resumes once third-person subjects return.
    let text = format!(
        "{}\n\nDear Aunt Mildred,\n\nI went to the station on Monday.\n\n{} I crossed the room. {}",
        third_scene(10),
        third_scene(6),
        third_scene(6)
    );
    assert_eq!(found(&text), first_in_third("I", "i"));
    // A sign-off closes the letter; the narration after it is checked again.
    let text = format!(
        "{}\n\nDear Aunt Mildred,\n\nI went to the station on Monday.\n\nYours, Valancy\n\n{}\u{2028}I crossed the room. {}",
        third_scene(10),
        third_scene(6),
        third_scene(6)
    );
    assert_eq!(found(&text), first_in_third("I", "i"));
}
