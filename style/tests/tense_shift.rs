//! The tense-shift lens (#355). Fixture text is written for these tests (CC0).

use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::narrative::NarrativeTense;
use writemorebetter_style::reports::tense_shift::{
    self, SceneTense, TenseExemption, TenseShiftOptions, TenseShiftReport, TenseShiftStrength,
    TenseSource,
};
use writemorebetter_style::reports::{to_char_json, to_utf16_json, StyleResources};
use writemorebetter_style::{
    analyze_document, DocumentOptions, MarkedSpan, MarkedVoice, OffsetUnits, Span, StyleDocument,
    StyleError,
};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

/// Eight past-tense sentences with curly double and single quotes, curly apostrophes, an
/// unspaced em dash, an ellipsis, a U+2028 inside the paragraph and an emoji before the target.
const FRAME: &str = "Mara walked to the station at dawn. She didn’t look back—not once. ‘Ticket,’ \
the porter said, and she’d shown him one. The sign said “No Smoking.” The rain fell on the tracks… \
The train came late.\u{2028}She found a seat by the window. She waved at the boy across the aisle 😀.";

fn framed(target: &str) -> String {
    format!("{FRAME} {target}")
}

fn run(text: &str, options: &TenseShiftOptions) -> TenseShiftReport {
    run_with(text, options, &DocumentOptions::new())
}

fn run_with(
    text: &str,
    options: &TenseShiftOptions,
    document: &DocumentOptions,
) -> TenseShiftReport {
    let document = analyze_document(tokenizer(), text, document);
    let report = tense_shift::report(&document, options).unwrap();
    for shift in &report.shifts {
        check_span(text, &shift.span);
        for verb in &shift.verbs {
            check_span(text, verb);
            assert!(
                shift.span.byte.start <= verb.byte.start && verb.byte.end <= shift.span.byte.end
            );
        }
    }
    report
}

/// Byte, char and UTF-16 ranges agree and sit on character boundaries.
fn check_span(text: &str, span: &Span) {
    assert!(text.is_char_boundary(span.byte.start) && text.is_char_boundary(span.byte.end));
    let before = &text[..span.byte.start];
    let inside = &text[span.byte.clone()];
    assert_eq!(span.char.start, before.chars().count());
    assert_eq!(span.char.end - span.char.start, inside.chars().count());
    assert_eq!(span.utf16.start, before.encode_utf16().count());
    assert_eq!(
        span.utf16.end - span.utf16.start,
        inside.encode_utf16().count()
    );
}

type Found = (String, TenseShiftStrength, NarrativeTense, Vec<String>);

fn found(text: &str, options: &TenseShiftOptions) -> Vec<Found> {
    run(text, options)
        .shifts
        .iter()
        .map(|shift| {
            (
                text[shift.span.byte.clone()].to_owned(),
                shift.strength,
                shift.found,
                shift
                    .verbs
                    .iter()
                    .map(|verb| text[verb.byte.clone()].to_owned())
                    .collect(),
            )
        })
        .collect()
}

fn defaults() -> TenseShiftOptions {
    TenseShiftOptions::new()
}

fn singles() -> TenseShiftOptions {
    TenseShiftOptions::new().with_include_singles(true)
}

fn one_run(target: &str, span: &str, verbs: &[&str]) {
    let text = framed(target);
    let shifts = found(&text, &defaults());
    assert_eq!(
        shifts,
        [(
            span.to_owned(),
            TenseShiftStrength::Run,
            NarrativeTense::Present,
            verbs.iter().map(|verb| verb.to_string()).collect()
        )],
        "{target}"
    );
    let report = run(&text, &defaults());
    assert_eq!(report.shifts[0].expected, NarrativeTense::Past);
    assert_eq!(report.scenes[0].source, TenseSource::Inferred);
}

#[test]
fn the_frame_is_past_and_clean() {
    let report = run(FRAME, &singles());
    assert!(report.shifts.is_empty());
    assert_eq!(report.scenes.len(), 1);
    assert_eq!(report.scenes[0].tense, Some(NarrativeTense::Past));
    assert_eq!(report.scenes[0].votes.past, 8);
    assert_eq!(report.scenes[0].votes.present, 0);
}

#[test]
fn a_present_run_in_a_past_scene() {
    one_run(
        "The room is dark. The air smells of smoke.",
        "The room is dark. The air smells of smoke.",
        &["is", "smells"],
    );
}

#[test]
fn a_run_across_line_separators() {
    one_run(
        "She opened the door.\u{2028}The room is dark.\u{2028}It’s cold—colder than outside…",
        "The room is dark.\u{2028}It’s cold—colder than outside…",
        &["is", "’s"],
    );
}

#[test]
fn a_run_spanning_dialogue() {
    // *Nobody answers* is generic, and the past tag *she called* does not end the run.
    one_run(
        "The room is dark. “Hello?” she called. Nobody answers. The air is cold.",
        "The room is dark. “Hello?” she called. Nobody answers. The air is cold.",
        &["is", "is"],
    );
}

#[test]
fn present_dialogue_tags_vote() {
    one_run(
        "“Fine,” she says, and sits down. “Go,” she says.",
        "“Fine,” she says, and sits down. “Go,” she says.",
        &["says", "says"],
    );
}

#[test]
fn present_perfect_is_present() {
    one_run(
        "She has opened the door. They have left.",
        "She has opened the door. They have left.",
        &["has", "have"],
    );
}

#[test]
fn invariant_verbs_follow_their_subject() {
    one_run(
        "She sets the cup down. He hits the wall.",
        "She sets the cup down. He hits the wall.",
        &["sets", "hits"],
    );
    let control = framed("She set the cup down. He hit the wall.");
    assert!(found(&control, &singles()).is_empty());
}

#[test]
fn a_run_next_to_a_tagged_thought() {
    let text = framed("The room is dark. The air is cold. Where is he? she wondered.");
    let shifts = found(&text, &defaults());
    assert_eq!(shifts.len(), 1);
    assert_eq!(shifts[0].0, "The room is dark. The air is cold.");
    let document = analyze_document(tokenizer(), &text, &DocumentOptions::new());
    let thought: Vec<_> = document
        .voices
        .iter()
        .map(|voice| &text[voice.span.byte.clone()])
        .collect();
    assert!(thought.contains(&"Where is he?"), "{thought:?}");
}

#[test]
fn singles_only_with_include_singles() {
    let text = framed("The room is dark. She sat down. He left the station.");
    assert!(found(&text, &defaults()).is_empty());
    let shifts = found(&text, &singles());
    assert_eq!(
        shifts,
        [(
            "The room is dark.".to_owned(),
            TenseShiftStrength::Single,
            NarrativeTense::Present,
            vec!["is".to_owned()]
        )]
    );
}

#[test]
fn gnomic_singles_are_not_flagged() {
    let text = framed("The river runs east. She sat down.");
    assert!(found(&text, &singles()).is_empty());
}

const PRESENT_SCENE: &str =
    "Ana walks to the window. She opens the shutters. The street is quiet. \
The baker sweeps his step. She watches him for a while. He waves at her. She waves back. The bells \
ring for noon. Ana closes the window. She sits down at her desk.";

#[test]
fn declared_tense_beats_inference() {
    let report = run(
        PRESENT_SCENE,
        &defaults().with_tense(Some(NarrativeTense::Past)),
    );
    assert_eq!(report.scenes[0].source, TenseSource::Declared);
    assert_eq!(report.scenes[0].tense, Some(NarrativeTense::Past));
    assert_eq!(report.shifts.len(), 1);
    assert_eq!(report.shifts[0].strength, TenseShiftStrength::Run);
    assert_eq!(report.shifts[0].found, NarrativeTense::Present);

    // Scene 1 is declared present and overrides the document's past.
    let text = format!("{FRAME}\n\n* * *\n\n{PRESENT_SCENE}");
    let options = defaults()
        .with_tense(Some(NarrativeTense::Past))
        .with_scene_tenses(vec![SceneTense::new(1, NarrativeTense::Present)]);
    let report = run(&text, &options);
    assert_eq!(report.scenes.len(), 2);
    assert_eq!(report.scenes[1].tense, Some(NarrativeTense::Present));
    assert_eq!(report.scenes[1].source, TenseSource::Declared);
    assert!(report.shifts.is_empty(), "{:?}", report.shifts);
}

#[test]
fn reverse_runs_need_include_reverse_and_no_backstory() {
    let past = "She crossed the room. She opened the window. The wind came in.";
    let text = format!("{PRESENT_SCENE} {past}");
    assert!(found(&text, &defaults()).is_empty());
    let reverse = defaults().with_include_reverse(true);
    assert_eq!(
        found(&text, &reverse),
        [(
            past.to_owned(),
            TenseShiftStrength::Run,
            NarrativeTense::Past,
            vec!["crossed".to_owned(), "opened".to_owned(), "came".to_owned()]
        )]
    );
    let anchored = format!(
        "{PRESENT_SCENE} Years ago, she crossed the room. She opened the window. The wind came in."
    );
    assert!(found(&anchored, &reverse).is_empty());
    // Two past sentences are below the reverse floor of three.
    let short = format!("{PRESENT_SCENE} She crossed the room. She opened the window.");
    assert!(found(&short, &reverse).is_empty());
}

#[test]
fn past_perfect_counts_as_past_and_is_never_flagged() {
    let text = framed("She’d gone before. He had been waiting for hours.");
    assert!(found(&text, &singles()).is_empty());
    let report = run(&text, &singles());
    assert_eq!(report.scenes[0].votes.past, 10);
    // In a present scene past perfect is backstory, not a reverse shift.
    let text =
        format!("{PRESENT_SCENE} She’d gone before. He had been waiting for hours. They had left.");
    let report = run(&text, &defaults().with_include_reverse(true));
    assert!(report.shifts.is_empty());
    assert!(report
        .exemptions
        .iter()
        .any(|count| count.reason == TenseExemption::PastPerfect && count.count == 3));
}

fn clean(target: &str) {
    let text = framed(target);
    let report = run(&text, &singles());
    assert!(
        report.shifts.is_empty(),
        "{target}: {:?}",
        found(&text, &singles())
    );
    assert_eq!(
        report.scenes[0].tense,
        Some(NarrativeTense::Past),
        "{target}"
    );
}

#[test]
fn modal_and_habitual_clauses_do_not_vote() {
    clean("He’d go every Sunday. She would sit by the window. She can see it now.");
}

#[test]
fn reported_clauses_do_not_vote() {
    clean("She said the train leaves at six. She knew that he lies.");
    clean("The sign read: Rooms To Let.");
}

#[test]
fn relatives_and_as_if_do_not_vote() {
    clean("It was the house where she lives now. She ran as if the devil is after her.");
}

#[test]
fn narrator_commentary_does_not_vote() {
    clean("She poured me a cup without asking, which is a language. I learned the schedule the way you learn a scar—by pressing on it.");
    clean("That is the trouble with the things your family tells you.");
}

#[test]
fn generic_subjects_do_not_vote() {
    clean("Some things you never forget. People are strange. A man needs a job. Nobody knows why.");
}

#[test]
fn narrator_frames_do_not_vote() {
    clean("I remember the summer we met. I suppose he knew. This is the story of my father.");
}

#[test]
fn mixed_sentences_are_counted_not_flagged() {
    clean("I told him he could visit, but that was in June, and June is a liar.");
    clean("That’s what she wanted—or so she thinks…");
    let report = run(
        &framed("I told him he could visit, but that was in June, and June is a liar."),
        &singles(),
    );
    assert_eq!(report.scenes[0].votes.mixed, 1);
    assert!(report
        .exemptions
        .iter()
        .any(|count| count.reason == TenseExemption::MixedSentence));
}

#[test]
fn questions_exclamations_and_free_indirect_discourse() {
    clean("Where is he? Why is it always me! Tomorrow was Monday. Now she knew.");
}

#[test]
fn subjectless_clauses_do_not_vote() {
    clean("Read the sign. Let’s go. Here comes the bus.");
}

#[test]
fn quotes_thought_and_unquoted_speech_are_not_narration() {
    clean("“The room is dark,” she said. ‘It’s late,’ he said. She called it “the house that never sleeps.”");
    clean("I have to get out, she thought. I’m leaving, he said.");
}

#[test]
fn caller_spans_are_not_narration() {
    let letter = "Dear John,\u{2028}I hope you are well. The farm is quiet. The cows are fat.";
    let text = framed(letter);
    let start = text.find("Dear John").unwrap();
    let options = DocumentOptions::new().with_marked_spans(
        OffsetUnits::Byte,
        vec![MarkedSpan::new(start..text.len(), MarkedVoice::Embedded)],
    );
    assert!(run_with(&text, &singles(), &options).shifts.is_empty());
    // Unmarked, the letter's present sentences are a run.
    assert_eq!(run(&text, &defaults()).shifts.len(), 1);
}

#[test]
fn unbalanced_quotes_never_vote() {
    let text = format!("{FRAME}\n“The room is dark. The air is cold. The door is open.");
    let report = run(&text, &singles());
    assert!(report.shifts.is_empty());
    assert!(report
        .exemptions
        .iter()
        .any(|count| count.reason == TenseExemption::UnbalancedQuotes && count.count == 3));
}

#[test]
fn scene_breaks_separate_tenses() {
    let text = format!("{FRAME}\n\n* * *\n\n{PRESENT_SCENE}");
    let report = run(&text, &singles());
    assert_eq!(report.scenes.len(), 2);
    assert_eq!(report.scenes[0].tense, Some(NarrativeTense::Past));
    assert_eq!(report.scenes[1].tense, Some(NarrativeTense::Present));
    assert_eq!(report.scenes[1].paragraphs, 1..2);
    assert!(report.shifts.is_empty());
}

const PAST_SENTENCES: [&str; 8] = [
    "Mara walked home.",
    "She opened the gate.",
    "The dog barked twice.",
    "She fed him.",
    "The kettle boiled.",
    "She drank her tea.",
    "The clock struck nine.",
    "She went to bed.",
];

#[test]
fn floors_dead_bands_and_inheritance() {
    // Four classified sentences are below the floor of six.
    let short = PAST_SENTENCES[..4].join(" ") + " The room is dark. The air is cold.";
    let report = run(
        &short.replace("The room is dark. The air is cold.", ""),
        &defaults(),
    );
    assert_eq!(report.scenes[0].source, TenseSource::Undetermined);
    assert_eq!(report.scenes[0].tense, None);

    // Six past and four present is 60%: inside the dead band, no tense and no flags.
    let dead = format!(
        "{} The room is dark. The air is cold. The lamp is lit. The door is shut.",
        PAST_SENTENCES[..6].join(" ")
    );
    let report = run(&dead, &defaults());
    assert_eq!(report.scenes[0].source, TenseSource::Undetermined);
    assert!(report.shifts.is_empty());

    // A strongly past document lends its tense to a short scene that does not favour present.
    let long = (0..4)
        .map(|_| PAST_SENTENCES.join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    let inherited = format!(
        "{long}\n\n* * *\n\nShe woke early. The room is dark. The air is cold. She dressed."
    );
    let report = run(&inherited, &defaults());
    assert_eq!(report.scenes[1].source, TenseSource::Inherited);
    assert_eq!(report.scenes[1].tense, Some(NarrativeTense::Past));
    assert_eq!(report.shifts.len(), 1);
    assert_eq!(report.shifts[0].scene, 1);
    let off = run(&inherited, &defaults().with_inherit_document_tense(false));
    assert_eq!(off.scenes[1].source, TenseSource::Undetermined);

    // A four-sentence present scene in that document is not inherited.
    let present = format!(
        "{long}\n\n* * *\n\nThe room is dark. The air is cold. The lamp is lit. She waits."
    );
    let report = run(&present, &defaults());
    assert_eq!(report.scenes[1].source, TenseSource::Undetermined);
    assert!(report.shifts.is_empty());
}

#[test]
fn rates_and_narration_words() {
    let text = framed("The room is dark. The air smells of smoke.");
    let report = run(&text, &defaults());
    let document = analyze_document(tokenizer(), &text, &DocumentOptions::new());
    let narration = document
        .words()
        .filter(|token| document.is_narration_at(token.span.byte.start))
        .count();
    assert_eq!(report.narration_words, narration);
    assert_eq!(report.scenes[0].narration_words, narration);
    let rate = report.per_thousand_words.unwrap();
    assert!((rate - 1000.0 / narration as f64).abs() < 1e-9);
    assert_eq!(run("", &defaults()).per_thousand_words, None);
    assert_eq!(report.shifts[0].sentences, 8..10);
}

#[test]
fn errors() {
    let compact = analyze_document(
        tokenizer(),
        FRAME,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        tense_shift::report(&compact, &defaults()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        FRAME,
        &DocumentOptions::new(),
    );
    for bad in [
        "{\"typo\":1}",
        "{\"minimumMajorityPercent\":40}",
        "{\"minimumMajorityPercent\":50}",
        "{\"minimumMajorityPercent\":101}",
        "{\"minimumRun\":1}",
        "{\"minimumSceneSentences\":0}",
        "{\"sceneTenses\":[{\"scene\":1,\"tense\":\"present\"}]}",
        "{\"tense\":\"future\"}",
        "{\"sceneTenses\":[{\"scene\":0,\"tense\":\"past\",\"extra\":1}]}",
    ] {
        assert!(
            matches!(
                document.tense_shift(Some(bad)),
                Err(StyleError::InvalidOptions { .. })
            ),
            "{bad}"
        );
    }
    for good in [
        "{\"includeSingles\":true}",
        "{\"include_singles\":true}",
        "{\"minimumMajorityPercent\":100}",
        "{\"sceneTenses\":[{\"scene\":0,\"tense\":\"present\"}]}",
        "{\"scene_tenses\":[{\"scene\":0,\"tense\":\"present\"}]}",
    ] {
        assert!(document.tense_shift(Some(good)).is_ok(), "{good}");
    }
}

#[test]
fn serialization_in_every_unit() {
    let text = format!(
        "😀 {}\u{2028}The room is dark. The air is cold.",
        PAST_SENTENCES.join(" ")
    );
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        &text,
        &DocumentOptions::new(),
    );
    let report = document.tense_shift(None).unwrap();
    assert_eq!(report.shifts.len(), 1);
    let target = "The room is dark. The air is cold.";
    let byte = text.find(target).unwrap();
    let utf16: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    let start = text[..byte].encode_utf16().count();
    assert_eq!(utf16["shifts"][0]["span"]["start"], start);
    assert_eq!(
        utf16["shifts"][0]["span"]["end"],
        start + target.encode_utf16().count()
    );
    assert_eq!(utf16["shifts"][0]["strength"], "run");
    assert_eq!(utf16["shifts"][0]["expected"], "past");
    assert_eq!(utf16["scenes"][0]["source"], "inferred");
    assert!(utf16["scenes"][0]["votes"]["past"].is_number());
    assert!(utf16["perThousandWords"].is_number());
    let chars: serde_json::Value = serde_json::from_str(&to_char_json(&report)).unwrap();
    let start = text[..byte].chars().count();
    assert_eq!(chars["shifts"][0]["span"]["start"], start);
    assert_eq!(
        chars["shifts"][0]["verbs"][0]["start"],
        start + "The room ".chars().count()
    );

    let options = TenseShiftOptions::new()
        .with_tense(Some(NarrativeTense::Past))
        .with_scene_tenses(vec![SceneTense::new(1, NarrativeTense::Present)])
        .with_include_singles(true);
    let json = serde_json::to_string(&options).unwrap();
    assert!(
        json.contains("\"sceneTenses\":[{\"scene\":1,\"tense\":\"present\"}]"),
        "{json}"
    );
    let back: TenseShiftOptions = serde_json::from_str(&json).unwrap();
    assert_eq!(back, options);
    let documented: TenseShiftOptions = serde_json::from_str(
        "{\"tense\":\"past\",\"sceneTenses\":[{\"scene\":1,\"tense\":\"present\"}],\"includeSingles\":true,\"includeReverse\":false}",
    )
    .unwrap();
    assert_eq!(documented, options);
}

/// The binding tests' text. With the spec's `{"minimumSceneSentences":1}` the scene is two past
/// and two present sentences, so it stays undetermined; the bindings declare past instead.
#[test]
fn bindings_fixture() {
    let text = "😀 She opened the door.\u{2028}The room is dark. The air is cold. “Hi,” she said.";
    let document = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let undetermined = document
        .tense_shift(Some("{\"minimumSceneSentences\":1}"))
        .unwrap();
    assert_eq!(undetermined.scenes[0].source, TenseSource::Undetermined);
    assert!(undetermined.shifts.is_empty());
    let report = document.tense_shift(Some("{\"tense\":\"past\"}")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&to_utf16_json(&report)).unwrap();
    let target = "The room is dark. The air is cold.";
    let start = text[..text.find(target).unwrap()].encode_utf16().count();
    assert_eq!(
        json["shifts"][0]["span"],
        serde_json::json!({"start": start, "end": start + target.len()})
    );
    assert_eq!(json["shifts"][0]["verbs"].as_array().unwrap().len(), 2);
    assert_eq!(json["scenes"][0]["source"], "declared");
}

#[test]
fn a_subject_phrase_before_its_clause_start_does_not_panic() {
    // Pride and Prejudice, chapter 13 (public domain): the noun phrase of *which are instituted*
    // opens before the clause start, which once sliced a token range backwards.
    let text = "My mind however is now made up on the subject, for having received ordination at \
Easter, I have been so fortunate as to be distinguished by the patronage of the Right Honourable \
Lady Catherine de Bourgh, widow of Sir Lewis de Bourgh, whose bounty and beneficence has preferred \
me to the valuable rectory of this parish, where it shall be my earnest endeavour to demean myself \
with grateful respect towards her Ladyship, and be ever ready to perform those rites and \
ceremonies which are instituted by the Church of England.";
    let report = run(text, &TenseShiftOptions::new());
    assert!(report.shifts.is_empty());
}

#[test]
fn the_narrators_present_does_not_vote() {
    // Guards added from the development labels (docs/STYLE-TENSE-SHIFT.md).
    clean("Nowadays we have revues. To this day the house is empty. These days the road is paved.");
    clean("I have known a lot of losses. Of Silver we have heard no more.");
    clean("I am glad to think of it. I recollect the wet housetops. I picture my small self.");
    // Third-person present perfect still votes.
    one_run(
        "She has opened the door. They have left.",
        "She has opened the door. They have left.",
        &["has", "have"],
    );
}

#[test]
fn indefinite_and_habitual_past_chains_vote_past() {
    // *A package came* is narration, not a generic statement: it closes the run before it.
    let text = framed("The room is dark. The air is cold. A package came. She always walked home.");
    let report = run(&text, &defaults());
    assert_eq!(report.shifts.len(), 1);
    assert_eq!(
        &text[report.shifts[0].span.byte.clone()],
        "The room is dark. The air is cold."
    );
    assert_eq!(report.scenes[0].votes.past, 10);
}
